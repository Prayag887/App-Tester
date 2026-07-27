package dev.prayag.apptester.companion

import android.app.Activity
import android.content.Intent
import android.net.VpnService
import android.content.pm.PackageManager
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    private lateinit var controller: ProxySafetyController
    private var waitingForVpnConsent = false

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        controller = ProxySafetyController(this)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "dev.prayag.apptester/proxy_safety")
            .setMethodCallHandler { call, result ->
                try {
                    val status = when (call.method) {
                        "installedDebugApps" -> {
                            @Suppress("DEPRECATION")
                            val apps = packageManager.getInstalledApplications(PackageManager.GET_META_DATA)
                                .filter { info -> info.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE != 0 }
                                .map { info -> mapOf("package_name" to info.packageName, "label" to packageManager.getApplicationLabel(info).toString()) }
                                .sortedBy { it["label"] }
                            result.success(apps)
                            return@setMethodCallHandler
                        }
                        "status" -> controller.status()
                        "startMonitoring" -> {
                            val host = call.argument<String>("host") ?: error("Desktop host is required.")
                            val port = call.argument<Int>("port") ?: error("Proxy port is required.")
                            controller.startMonitoring(host, port)
                        }
                        "stopMonitoring" -> controller.stopMonitoring()
                        "startVpn" -> {
                            val host = call.argument<String>("host") ?: error("Desktop host is required.")
                            val port = call.argument<Int>("port") ?: error("Proxy port is required.")
                            val targetPackage = call.argument<String>("targetPackage") ?: error("Selected package is required.")
                            controller.configureVpn(host, port, targetPackage)
                            val consent = VpnService.prepare(this)
                            if (consent != null) {
                                waitingForVpnConsent = true
                                startActivityForResult(consent, VPN_CONSENT_REQUEST)
                                controller.status("Approve Android's VPN consent, then tap Start VPN capture again.")
                            } else controller.startVpn()
                        }
                        "stopVpn" -> controller.stopVpn()
                        else -> {
                            result.notImplemented()
                            return@setMethodCallHandler
                        }
                    }
                    result.success(status.asMap())
                } catch (error: IllegalArgumentException) {
                    result.error("invalid_configuration", error.message, null)
                }
            }
    }

    @Deprecated("Deprecated in Java")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != VPN_CONSENT_REQUEST || !waitingForVpnConsent) return
        waitingForVpnConsent = false
        if (resultCode == Activity.RESULT_OK) controller.startVpn()
        else controller.stopVpn("VPN consent was not granted. Direct networking is unchanged.")
    }

    private companion object {
        const val VPN_CONSENT_REQUEST = 4401
    }
}
