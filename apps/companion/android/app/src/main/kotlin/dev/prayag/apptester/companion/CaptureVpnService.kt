package dev.prayag.apptester.companion

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Intent
import android.content.pm.PackageManager
import android.net.VpnService
import android.os.Build
import android.os.IBinder
import android.os.ParcelFileDescriptor
import java.net.InetSocketAddress
import java.net.Socket
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

class CaptureVpnService : VpnService() {
    private var tun: ParcelFileDescriptor? = null
    private val scheduler = Executors.newSingleThreadScheduledExecutor()
    private var failures = 0
    private var monitoringStarted = false
    private var stopped = false

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            stopCapture()
            stopSelf()
            return START_NOT_STICKY
        }
        val host = intent?.getStringExtra(EXTRA_HOST) ?: return START_NOT_STICKY
        val port = intent.getIntExtra(EXTRA_PORT, 0)
        val packageName = intent.getStringExtra(EXTRA_PACKAGE) ?: return START_NOT_STICKY
        if (port !in 1..65535) return START_NOT_STICKY

        startForeground(NOTIFICATION_ID, notification(packageName))
        runCatching {
            packageManager.getApplicationInfo(packageName, 0)
            tun?.close()
            tun = Builder()
                .setSession("App Tester capture")
                .setMtu(MTU)
                .addAddress("10.8.0.2", 32)
                .addRoute("0.0.0.0", 0)
                .addRoute("::", 0)
                .addAllowedApplication(packageName)
                .establish()
                ?: error("Android could not establish the capture VPN.")
            val error = VpnNative.start(tun!!.detachFd(), "http://$host:$port")
            require(error.isEmpty()) { error }
            ProxySafetyController(this).markVpnActive(packageName)
            if (!monitoringStarted) {
                monitoringStarted = true
                scheduler.scheduleWithFixedDelay({ checkDesktop(host, port) }, CHECK_INTERVAL_SECONDS, CHECK_INTERVAL_SECONDS, TimeUnit.SECONDS)
            }
        }.onFailure { error ->
            ProxySafetyController(this).record("VPN relay could not start: ${error.message ?: error.javaClass.simpleName}")
            ProxySafetyController(this).stopVpn("VPN relay could not start. Direct networking resumed.")
            stopSelf()
        }
        return START_NOT_STICKY
    }

    override fun onRevoke() {
        ProxySafetyController(this).record("Android revoked VPN permission. Direct networking resumed.")
        stopSelf()
    }

    override fun onDestroy() {
        stopCapture()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = super.onBind(intent)

    private fun checkDesktop(host: String, port: Int) {
        val connected = runCatching {
            Socket().use { socket -> socket.connect(InetSocketAddress(host, port), CONNECT_TIMEOUT_MS) }
        }.isSuccess
        if (connected) {
            failures = 0
            return
        }
        failures += 1
        if (failures >= MAX_FAILURES) {
            ProxySafetyController(this).record("Desktop proxy is unreachable. Stopping VPN so $host:$port fails open to direct networking.")
            stopSelf()
        }
    }

    private fun stopCapture() {
        if (stopped) return
        stopped = true
        scheduler.shutdownNow()
        runCatching { VpnNative.stop() }
        tun?.close()
        tun = null
        stopForeground(STOP_FOREGROUND_REMOVE)
        ProxySafetyController(this).markVpnStopped()
    }

    private fun notification(packageName: String): android.app.Notification {
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel(CHANNEL_ID, "App Tester capture", NotificationManager.IMPORTANCE_LOW),
        )
        return android.app.Notification.Builder(this, CHANNEL_ID)
            .setContentTitle("App Tester capture relay")
            .setContentText("Capturing only $packageName. Other apps use direct networking.")
            .setSmallIcon(android.R.drawable.stat_sys_upload_done)
            .setOngoing(true)
            .build()
    }

    companion object {
        const val EXTRA_HOST = "host"
        const val EXTRA_PORT = "port"
        const val EXTRA_PACKAGE = "package"
        const val ACTION_STOP = "dev.prayag.apptester.companion.STOP_VPN"
        private const val CHANNEL_ID = "capture_vpn"
        private const val NOTIFICATION_ID = 22
        private const val MTU = 1500
        private const val CHECK_INTERVAL_SECONDS = 1L
        private const val CONNECT_TIMEOUT_MS = 500
        private const val MAX_FAILURES = 1
    }
}
