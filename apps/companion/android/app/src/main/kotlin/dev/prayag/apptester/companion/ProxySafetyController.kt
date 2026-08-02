package dev.prayag.apptester.companion

import android.content.Context
import android.content.Intent
import android.os.Build
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

data class ProxySafetyStatus(
    val isMonitoring: Boolean,
    val isVpnActive: Boolean,
    val host: String?,
    val port: Int?,
    val message: String?,
    val targetPackage: String?,
    val caAvailable: Boolean,
    val logs: List<Map<String, String>>,
) {
    fun asMap() = mapOf(
        "isMonitoring" to isMonitoring,
        "isVpnActive" to isVpnActive,
        "host" to host,
        "port" to port,
        "message" to message,
        "targetPackage" to targetPackage,
        "caAvailable" to caAvailable,
        "logs" to logs,
    )
}

class ProxySafetyController(private val context: Context) {
    private val preferences = context.getSharedPreferences("proxy_safety", Context.MODE_PRIVATE)

    fun status(message: String? = null): ProxySafetyStatus {
        val host = preferences.getString(HOST, null)
        val port = preferences.getInt(PORT, 0).takeIf { it > 0 }
        return ProxySafetyStatus(
            preferences.getBoolean(MONITORING, false),
            preferences.getBoolean(VPN_ACTIVE, false),
            host,
            port,
            message ?: preferences.getString(MESSAGE, null),
            preferences.getString(TARGET_PACKAGE, null),
            preferences.contains(CA_PEM),
            readLogs(),
        )
    }

    fun configureVpn(host: String, port: Int, targetPackage: String): ProxySafetyStatus {
        require(host.isNotBlank()) { "Desktop host is required." }
        require(port in 1..65535) { "Proxy port must be between 1 and 65535." }
        require(targetPackage.isNotBlank()) { "Selected package is required." }
        context.packageManager.getApplicationInfo(targetPackage, 0)
        preferences.edit().putString(HOST, host).putInt(PORT, port)
            .putString(TARGET_PACKAGE, targetPackage).putBoolean(VPN_ACTIVE, false).apply()
        record("VPN consent requested for $targetPackage")
        return status("Approve Android's VPN consent to start capture for $targetPackage.")
    }

    fun configureCa(pem: String) {
        require(pem.contains("BEGIN CERTIFICATE")) { "Desktop CA certificate is invalid." }
        preferences.edit().putString(CA_PEM, pem).apply()
    }

    fun caPem(): String? = preferences.getString(CA_PEM, null)

    fun startVpn(): ProxySafetyStatus {
        val endpoint = endpoint() ?: throw IllegalArgumentException("Desktop host and proxy port are required.")
        val targetPackage = preferences.getString(TARGET_PACKAGE, null)
            ?: throw IllegalArgumentException("Selected package is required.")
        val intent = Intent(context, CaptureVpnService::class.java)
            .putExtra(CaptureVpnService.EXTRA_HOST, endpoint.first)
            .putExtra(CaptureVpnService.EXTRA_PORT, endpoint.second)
            .putExtra(CaptureVpnService.EXTRA_PACKAGE, targetPackage)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) context.startForegroundService(intent) else context.startService(intent)
        return status("Starting VPN capture relay for $targetPackage.")
    }

    fun markVpnActive(targetPackage: String) {
        preferences.edit().putBoolean(VPN_ACTIVE, true).putBoolean(MONITORING, true).apply()
        record("USB capture active for $targetPackage")
    }

    fun stopVpn(message: String = "VPN capture stopped. Direct networking resumed."): ProxySafetyStatus {
        context.startService(
            Intent(context, CaptureVpnService::class.java)
                .setAction(CaptureVpnService.ACTION_STOP),
        )
        return markVpnStopped(message)
    }

    fun markVpnStopped(message: String = "VPN capture stopped. Direct networking resumed."): ProxySafetyStatus {
        preferences.edit().putBoolean(VPN_ACTIVE, false).putBoolean(MONITORING, false).apply()
        record(message)
        return status(message)
    }

    fun endpoint(): Pair<String, Int>? {
        val host = preferences.getString(HOST, null) ?: return null
        val port = preferences.getInt(PORT, 0).takeIf { it > 0 } ?: return null
        return host to port
    }

    fun record(message: String) {
        val stamp = SimpleDateFormat("HH:mm:ss", Locale.US).format(Date())
        val existing = preferences.getStringSet(LOGS, emptySet()).orEmpty().toMutableList()
        existing += "$stamp|$message"
        preferences.edit().putStringSet(LOGS, existing.takeLast(MAX_LOGS).toSet()).putString(MESSAGE, message).apply()
    }

    private fun readLogs() = preferences.getStringSet(LOGS, emptySet()).orEmpty()
        .mapNotNull { row -> row.split("|", limit = 2).takeIf { it.size == 2 } }
        .sortedByDescending { it[0] }
        .map { mapOf("time" to it[0], "message" to it[1]) }

    companion object {
        private const val HOST = "host"
        private const val PORT = "port"
        private const val MONITORING = "monitoring"
        private const val MESSAGE = "message"
        private const val TARGET_PACKAGE = "target_package"
        private const val VPN_ACTIVE = "vpn_active"
        private const val CA_PEM = "ca_pem"
        private const val LOGS = "logs"
        private const val MAX_LOGS = 100
    }
}
