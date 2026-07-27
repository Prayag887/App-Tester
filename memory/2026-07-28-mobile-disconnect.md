# Mobile companion disconnect

- Goal: allow user to disconnect desktop pairing from mobile app before or during VPN capture.
- Root cause: disconnect button rendered only while VPN was active and called `stopVpn`; paired monitoring state had no exit action and retained endpoint/package preferences.
- Fix: show disconnect for monitoring and VPN states, cancel desktop config polling, stop VPN when needed, stop monitoring, clear saved host/port/package, reset Wi-Fi state, and restart QR scanner.
- Release: Companion `0.2.2+4` with immutable APK URL.
- Preserved behavior: Android VPN fail-open, package-scoped routing, connection-first pairing, complete debug package discovery, and zero mobile data entry remain.
- Verification: Flutter analyze/tests, signed APK build, desktop tests/typecheck, Rust checks, desktop release build, release upload, local installation, and GitHub merge.
