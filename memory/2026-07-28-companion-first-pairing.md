# Companion-first pairing

- Symptom: desktop required a package before connecting companion, but package inventory exists only on phone.
- Root cause: version 1 connection QR embedded `package_name` and started VPN immediately after scan. Desktop package discovery still depended on ADB.
- Decision: connection and capture configuration are separate phases. QR carries host, port, and random token only.
- Desktop change: start local endpoint, show QR, receive authenticated launchable-app inventory, populate package picker, then publish selected package when capture starts. Connected companion takes precedence over stale USB selection.
- Companion change: scan QR, verify reachability/Wi-Fi, query Android launcher apps, register them with desktop, poll for selected package, then request standard VPN consent and start package-scoped relay.
- Preserved behavior: download QR remains always visible; no mobile text entry; only selected package enters VPN; fail-open relay and Wi-Fi status remain.
- Verification: Flutter analyze/tests, Rust core tests, desktop Vitest/TypeScript, release APK build, desktop release build, local installation, and GitHub merge.
