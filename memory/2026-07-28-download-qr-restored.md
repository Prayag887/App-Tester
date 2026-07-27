# Companion download QR restored

- Symptom: App Tester header showed only `Connect companion`, disabled until a package was selected. New users could not reach the APK download QR.
- Cause: scan-to-connect work replaced the always-visible installer action; installer became a secondary action inside an unreachable connection dialog.
- Fix: keep separate `Download app` and `Connect companion` actions. Download never depends on device or package selection; connection still requires a selected package.
- Verification: desktop unit tests, TypeScript build, release app build, local installation, and GitHub merge.
