package dev.prayag.apptester.companion

import android.app.Activity
import android.content.Intent
import android.net.VpnService
import android.os.Bundle
import android.provider.Settings
import android.util.Base64
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

open class MainActivity : FlutterActivity() {
    private lateinit var controller: ProxySafetyController
    private var waitingForVpnConsent = false

    override fun onCreate(savedInstanceState: Bundle?) {
        controller = ProxySafetyController(this)
        super.onCreate(savedInstanceState)
        handleUsbCaptureIntent(intent)
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "dev.prayag.apptester/proxy_safety")
            .setMethodCallHandler { call, result ->
                try {
                    val status = when (call.method) {
                        "status" -> controller.status()
                        "startVpn" -> connectDesktop()
                        "stopVpn" -> controller.stopVpn()
                        "installCa" -> {
                            openCaInstaller()
                            controller.status("Android Security Settings opened. Install AppTester-HTTPS-CA.pem from Downloads as a CA certificate.")
                        }
                        "removeCa" -> {
                            startActivity(Intent("com.android.settings.TRUSTED_CREDENTIALS_USER"))
                            controller.status("Android Trusted credentials opened. Select App Tester HTTPS CA, then remove or disable it.")
                        }
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

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleUsbCaptureIntent(intent)
    }

    private fun handleUsbCaptureIntent(intent: Intent?) {
        if (this !is UsbCaptureActivity) return
        intent?.getStringExtra(EXTRA_CA_BASE64)?.let { encoded ->
            controller.configureCa(String(Base64.decode(encoded, Base64.DEFAULT)))
        }
        if (intent?.getBooleanExtra(EXTRA_START_CAPTURE, false) != true) return
        val host = intent.getStringExtra(EXTRA_HOST) ?: return
        val port = intent.getIntExtra(EXTRA_PORT, 0)
        val targetPackage = intent.getStringExtra(EXTRA_PACKAGE) ?: return
        if (host != USB_HOST || port != USB_PORT) {
            controller.stopVpn("Invalid USB relay configuration. Direct networking is unchanged.")
            return
        }
        controller.configureVpn(host, port, targetPackage)
        connectDesktop()
    }

    private fun connectDesktop(): ProxySafetyStatus {
        val consent = VpnService.prepare(this)
        if (consent != null) {
            waitingForVpnConsent = true
            startActivityForResult(consent, VPN_CONSENT_REQUEST)
            return controller.status("Approve Android's VPN consent to connect to App Tester desktop.")
        } else {
            return controller.startVpn()
        }
    }

    private fun openCaInstaller() {
        controller.caPem()
            ?: throw IllegalArgumentException("Connect to App Tester desktop over USB before installing its CA.")
        startActivity(Intent(Settings.ACTION_SECURITY_SETTINGS))
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
        const val USB_HOST = "127.0.0.1"
        const val USB_PORT = 8080
        const val EXTRA_HOST = "app_tester_host"
        const val EXTRA_PORT = "app_tester_port"
        const val EXTRA_PACKAGE = "app_tester_package"
        const val EXTRA_START_CAPTURE = "app_tester_start_capture"
        const val EXTRA_CA_BASE64 = "app_tester_ca_base64"
    }
}
