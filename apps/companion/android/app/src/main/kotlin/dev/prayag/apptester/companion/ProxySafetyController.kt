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
    val logs: List<Map<String, String>>,
) {
    fun asMap() = mapOf(
        "isMonitoring" to isMonitoring,
        "isVpnActive" to isVpnActive,
        "host" to host,
        "port" to port,
        "message" to message,
        "targetPackage" to targetPackage,
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
            readLogs(),
        )
    }

    fun startMonitoring(host: String, port: Int): ProxySafetyStatus {
        require(host.isNotBlank()) { "Desktop host is required." }
        require(port in 1..65535) { "Proxy port must be between 1 and 65535." }
        record("Desktop link started for $host:$port")
        preferences.edit().putString(HOST, host).putInt(PORT, port).putBoolean(MONITORING, true).apply()
        val intent = Intent(context, ProxySafetyService::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) context.startForegroundService(intent) else context.startService(intent)
        return status("Checking $host:$port in the background.")
    }

    fun stopMonitoring(message: String = "Desktop link stopped. Direct networking is unchanged."): ProxySafetyStatus {
        context.stopService(Intent(context, ProxySafetyService::class.java))
        record(message)
        preferences.edit().remove(HOST).remove(PORT).putBoolean(MONITORING, false).apply()
        return status(message)
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

    fun startVpn(): ProxySafetyStatus {
        val endpoint = endpoint() ?: throw IllegalArgumentException("Desktop host and proxy port are required.")
        val targetPackage = preferences.getString(TARGET_PACKAGE, null)
            ?: throw IllegalArgumentException("Selected package is required.")
        val intent = Intent(context, CaptureVpnService::class.java)
            .putExtra(CaptureVpnService.EXTRA_HOST, endpoint.first)
            .putExtra(CaptureVpnService.EXTRA_PORT, endpoint.second)
            .putExtra(CaptureVpnService.EXTRA_PACKAGE, targetPackage)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) context.startForegroundService(intent) else context.startService(intent)
        preferences.edit().putBoolean(VPN_ACTIVE, true).apply()
        return status("Starting VPN capture relay for $targetPackage.")
    }

    fun stopVpn(message: String = "VPN capture stopped. Direct networking resumed."): ProxySafetyStatus {
        context.stopService(Intent(context, CaptureVpnService::class.java))
        preferences.edit().putBoolean(VPN_ACTIVE, false).apply()
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
        private const val LOGS = "logs"
        private const val MAX_LOGS = 100
    }
}
