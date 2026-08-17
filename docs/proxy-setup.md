# Proxy setup

1. Connect a physical Android device by USB and approve its debugging prompt.
2. Select that USB device and the application to test.
3. Choose **Start USB capture**. App Tester starts its local proxy, creates an
   `adb reverse` tunnel, installs or opens the bundled Companion, and supplies
   the selected package directly.
4. Approve Android's one-time VPN prompt if it appears.
5. Navigate the target app manually.
6. Stop capture before unplugging USB.

For HTTPS inspection, generate and transfer the local CA, confirm Android's
certificate installer, and ensure the target application's network-security
policy trusts user certificates.

The Companion VPN is restricted to the selected package. It receives no LAN
host or wireless pairing information: the device sees a loopback endpoint and
ADB carries the traffic over the USB cable. App Tester intentionally ignores
emulators, wireless ADB devices, QR pairing, pairing codes, and ADB-over-TCP.

If USB is removed, the Companion's endpoint watchdog stops its VPN and Android
restores direct networking. App Tester does not reconnect automatically; plug
the device in and start a new capture.

If no traffic appears, do not assume no request occurred. Verify certificate
trust, the app's network-security configuration, certificate pinning,
QUIC/HTTP/3 bypass, and direct socket use.

<!-- Legacy global-proxy cleanup remains at startup only so upgrades from an
older App Tester version cannot leave a device proxy behind. New captures do
not change Android's global proxy setting. -->
