package dev.prayag.apptester.companion

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.IBinder
import java.net.InetSocketAddress
import java.net.Socket
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

class ProxySafetyService : Service() {
    private val scheduler = Executors.newSingleThreadScheduledExecutor()
    private var failures = 0

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startForeground(NOTIFICATION_ID, notification())
        scheduler.scheduleWithFixedDelay(::checkDesktop, 0, CHECK_INTERVAL_SECONDS, TimeUnit.SECONDS)
        return START_STICKY
    }

    override fun onDestroy() {
        scheduler.shutdownNow()
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun checkDesktop() {
        val endpoint = ProxySafetyController(this).endpoint() ?: return
        val connected = runCatching {
            Socket().use { socket -> socket.connect(InetSocketAddress(endpoint.first, endpoint.second), CONNECT_TIMEOUT_MS) }
        }.isSuccess
        val controller = ProxySafetyController(this)
        if (connected) {
            failures = 0
            controller.record("Desktop endpoint is reachable.")
            return
        }
        failures += 1
        if (failures >= MAX_FAILURES) {
            controller.record("Desktop endpoint is unreachable. Direct networking is not modified.")
            controller.stopMonitoring("Desktop endpoint is unavailable. Monitoring stopped safely.")
            stopSelf()
        }
    }

    private fun notification() : android.app.Notification {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel(CHANNEL_ID, "Proxy safety", NotificationManager.IMPORTANCE_LOW))
        return android.app.Notification.Builder(this, CHANNEL_ID)
            .setContentTitle("App Tester desktop link")
            .setContentText("Monitoring the configured desktop endpoint.")
            .setSmallIcon(android.R.drawable.stat_sys_upload_done)
            .setOngoing(true)
            .build()
    }

    companion object {
        private const val CHANNEL_ID = "proxy_safety"
        private const val NOTIFICATION_ID = 7
        private const val CHECK_INTERVAL_SECONDS = 5L
        private const val CONNECT_TIMEOUT_MS = 1500
        private const val MAX_FAILURES = 3
    }
}
