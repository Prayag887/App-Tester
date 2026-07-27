# Companion protocol versioning

- Symptom: scanning current desktop QR showed `Connection code is incomplete` in companion.
- Root cause: connection-first pairing changed QR fields from `package_name` to `token` but retained protocol version 1 and Android version `0.1.0+1`. Old and new apps were indistinguishable, while overwritten stable APK URLs could remain cached.
- Fix: bump QR protocol to 2, companion to `0.2.0+2`, include minimum companion version, use versioned GitHub APK URL, and show explicit update guidance for schema mismatch.
- Preserved behavior: connection happens before package selection, download QR remains independent, Wi-Fi check and package-scoped VPN remain unchanged.
- Verification: Flutter analyze/tests, signed APK build, Rust tests/check, frontend tests/typecheck, desktop release build, release upload, local installation, and GitHub merge.
