package dev.prayag.apptester.companion

internal object VpnNative {
    init {
        System.loadLibrary("apptester_tun2socks")
    }

    external fun start(tunFd: Int, proxyUrl: String): String
    external fun stop()
}
