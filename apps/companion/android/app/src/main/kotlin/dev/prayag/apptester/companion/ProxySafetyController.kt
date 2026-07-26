package dev.prayag.apptester.companion

import android.app.admin.DevicePolicyManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.ProxyInfo
import android.os.Build

data class ProxySafetyStatus(
    val isDeviceOwner: Boolean,
    val isArmed: Boolean,
    val host: String?,
    val port: Int?,
    val message: String?,
) {
    fun asMap() = mapOf(
        "isDeviceOwner" to isDeviceOwner,
        "isArmed" to isArmed,
        "host" to host,
        "port" to port,
        "message" to message,
    )
}

class ProxySafetyController(private val context: Context) {
    private val policyManager = context.getSystemService(DevicePolicyManager::class.java)
    private val admin = ComponentName(context, ProxySafetyAdminReceiver::class.java)
    private val preferences = context.getSharedPreferences("proxy_safety", Context.MODE_PRIVATE)

    fun status(message: String? = null): ProxySafetyStatus {
        val deviceOwner = policyManager.isDeviceOwnerApp(context.packageName)
        val host = preferences.getString(HOST, null)
        val port = preferences.getInt(PORT, 0).takeIf { it > 0 }
        return ProxySafetyStatus(deviceOwner, host != null && port != null, host, port, message)
    }

    fun arm(host: String, port: Int): ProxySafetyStatus {
        require(policyManager.isDeviceOwnerApp(context.packageName)) { DEVICE_OWNER_REQUIRED }
        require(host.isNotBlank()) { "Desktop host is required." }
        require(port in 1..65535) { "Proxy port must be between 1 and 65535." }
        policyManager.setRecommendedGlobalProxy(admin, ProxyInfo.buildDirectProxy(host, port))
        preferences.edit().putString(HOST, host).putInt(PORT, port).apply()
        val intent = Intent(context, ProxySafetyService::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) context.startForegroundService(intent) else context.startService(intent)
        return status("Monitoring $host:$port. Direct networking is restored after three failed checks.")
    }

    fun disarm(message: String = "Proxy cleared. Direct networking is active."): ProxySafetyStatus {
        if (policyManager.isDeviceOwnerApp(context.packageName)) {
            policyManager.setRecommendedGlobalProxy(admin, null)
        }
        context.stopService(Intent(context, ProxySafetyService::class.java))
        preferences.edit().clear().apply()
        return status(message)
    }

    fun endpoint(): Pair<String, Int>? {
        val host = preferences.getString(HOST, null) ?: return null
        val port = preferences.getInt(PORT, 0).takeIf { it > 0 } ?: return null
        return host to port
    }

    companion object {
        const val DEVICE_OWNER_REQUIRED = "This companion must be provisioned as the Android device owner before it can manage the global proxy."
        private const val HOST = "host"
        private const val PORT = "port"
    }
}
