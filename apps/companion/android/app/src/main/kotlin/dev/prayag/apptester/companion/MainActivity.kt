package dev.prayag.apptester.companion

import android.app.Activity
import android.content.ComponentName
import android.content.Intent
import android.net.VpnService
import java.net.HttpURLConnection
import java.net.URL
import org.json.JSONArray
import org.json.JSONObject
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    private lateinit var controller: ProxySafetyController
    private var waitingForVpnConsent = false
    private var pendingTargetPackage: String? = null

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        controller = ProxySafetyController(this)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "dev.prayag.apptester/proxy_safety")
            .setMethodCallHandler { call, result ->
                try {
                    val status = when (call.method) {
                        "status" -> controller.status()
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
        startUsbCaptureFromIntent(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        startUsbCaptureFromIntent(intent)
    }

    override fun onResume() {
        super.onResume()
        // ADB can bring an existing task forward without dispatching a fresh
        // onNewIntent callback on some OEM task managers. Consume any pending
        // one-shot command here as well; each branch removes its extra first.
        startUsbCaptureFromIntent(intent)
    }

    @Deprecated("Deprecated in Java")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != VPN_CONSENT_REQUEST || !waitingForVpnConsent) return
        waitingForVpnConsent = false
        if (resultCode == Activity.RESULT_OK) {
            controller.startVpn()
            pendingTargetPackage?.let(::launchTargetApp)
        }
        else controller.stopVpn("VPN consent was not granted. Direct networking is unchanged.")
        pendingTargetPackage = null
    }

    private fun startUsbCaptureFromIntent(intent: Intent?) {
        if (intent?.getBooleanExtra(EXTRA_STOP_VPN, false) == true) {
            // Desktop capture owns the companion VPN lifecycle for USB
            // sessions. Stop it immediately instead of waiting for the
            // endpoint watchdog to notice that the proxy disappeared.
            controller.stopVpn("VPN capture stopped by desktop. Direct networking resumed.")
            intent.removeExtra(EXTRA_STOP_VPN)
            return
        }
        val host = intent?.getStringExtra(EXTRA_HOST) ?: return
        val port = intent.getIntExtra(EXTRA_PORT, 0)
        val token = intent.getStringExtra(EXTRA_TOKEN) ?: return
        val targetPackage = intent.getStringExtra(EXTRA_PACKAGE) ?: return
        if (port !in 1..65535 || targetPackage.isBlank()) return

        // Consume the one-shot ADB payload so a rotation or activity restore
        // cannot accidentally restart a later capture.
        intent.removeExtra(EXTRA_HOST)
        intent.removeExtra(EXTRA_PORT)
        intent.removeExtra(EXTRA_TOKEN)
        intent.removeExtra(EXTRA_PACKAGE)
        controller.startMonitoring(host, port)
        registerWithDesktop(host, port, token, targetPackage)
        startVpnCapture(host, port, targetPackage)
    }

    private fun startVpnCapture(host: String, port: Int, targetPackage: String): ProxySafetyStatus {
        controller.configureVpn(host, port, targetPackage)
        val consent = VpnService.prepare(this)
        return if (consent != null) {
            waitingForVpnConsent = true
            pendingTargetPackage = targetPackage
            startActivityForResult(consent, VPN_CONSENT_REQUEST)
            controller.status("Approve Android's VPN consent to start USB capture for $targetPackage.")
        } else {
            val status = controller.startVpn()
            launchTargetApp(targetPackage)
            status
        }
    }

    private fun launchTargetApp(targetPackage: String) {
        val launcher = Intent(Intent.ACTION_MAIN)
            .addCategory(Intent.CATEGORY_LAUNCHER)
            .setPackage(targetPackage)
        val target = packageManager.queryIntentActivities(launcher, 0)
            .firstOrNull { !it.activityInfo.name.contains("leakcanary", ignoreCase = true) }
            ?.activityInfo
        if (target == null) {
            controller.record("VPN capture started, but $targetPackage has no launchable activity.")
            moveTaskToBack(true)
            return
        }
        startActivity(
            Intent()
                .setComponent(ComponentName(target.packageName, target.name))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED),
        )
        controller.record("VPN capture started. Opened $targetPackage for testing.")
    }

    private fun registerWithDesktop(host: String, port: Int, token: String, targetPackage: String) {
        val targetInfo = packageManager.getApplicationInfo(targetPackage, 0)
        val apps = listOf(
            JSONObject()
                .put("package_name", targetPackage)
                .put("label", packageManager.getApplicationLabel(targetInfo).toString()),
        )
        Thread {
            val connection = runCatching {
                (URL("http://$host:$port/__app_tester/companion/register").openConnection() as HttpURLConnection).apply {
                    requestMethod = "POST"
                    connectTimeout = 2_000
                    readTimeout = 2_000
                    doOutput = true
                    setRequestProperty("Content-Type", "application/json")
                }
            }.getOrElse { error ->
                controller.record("Could not create desktop USB connection: ${error.message}")
                return@Thread
            }
            runCatching {
                connection.outputStream.bufferedWriter().use { writer ->
                    writer.write(JSONObject().put("token", token).put("apps", JSONArray(apps)).toString())
                }
                if (connection.responseCode == HttpURLConnection.HTTP_OK) {
                    controller.record("Desktop USB connection established.")
                } else {
                    controller.record("Desktop rejected the USB companion connection.")
                }
            }.onFailure { error -> controller.record("USB connection failed: ${error.message}") }
                .also { connection.disconnect() }
        }.start()
    }

    private companion object {
        const val VPN_CONSENT_REQUEST = 4401
        const val EXTRA_HOST = "app_tester_host"
        const val EXTRA_PORT = "app_tester_port"
        const val EXTRA_TOKEN = "app_tester_token"
        const val EXTRA_PACKAGE = "app_tester_package"
        const val EXTRA_STOP_VPN = "app_tester_stop_vpn"
    }
}
