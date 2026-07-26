package dev.prayag.apptester.companion

import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        val controller = ProxySafetyController(this)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "dev.prayag.apptester/proxy_safety")
            .setMethodCallHandler { call, result ->
                try {
                    val status = when (call.method) {
                        "status" -> controller.status()
                        "startMonitoring" -> {
                            val host = call.argument<String>("host") ?: error("Desktop host is required.")
                            val port = call.argument<Int>("port") ?: error("Proxy port is required.")
                            controller.startMonitoring(host, port)
                        }
                        "stopMonitoring" -> controller.stopMonitoring()
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
}
