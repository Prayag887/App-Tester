# Debug report: ADB UID resolution and log filtering

- **Symptom:** Starting capture for `com.yajtech.eynorixdev` failed with `could not determine UID`, on both USB-connected Android devices and Android Studio emulators. The header also contained an inactive App Tester brand, and app logs omitted warnings and unclassified errors.
- **Root cause:** The first fix only relaxed parsing around `userId`, but the affected Android emulator actually reports `appId=10228` in `dumpsys package`. Its package manager reports the authoritative value as `package:com.yajtech.eynorixdev uid:10228`. Separately, log incidents were emitted only when their text matched a small known-signature list; severity alone was ignored.
- **Fix:** Resolve the full UID through `cmd package list packages -U <package>` first, then accept either `userId` or `appId` from `dumpsys` as a compatibility fallback. Emit known actionable issue categories at any severity, generic incidents for `W`, `E`, `F`, and `A`, and discard ordinary `V`, `D`, and `I` lines. Remove the inactive header brand and let proxy status occupy its layout position.
- **Evidence:** `cargo test --workspace` passed; desktop Vitest passed 5/5; TypeScript and Vite production build passed; `git diff --check` passed.
- **Regression tests:** `android::tests::extracts_uid_from_modern_package_manager_output`, `android::tests::extracts_app_uid_when_android_appends_package_fields`, and `diagnostics::tests::includes_unclassified_errors_and_warnings_but_drops_normal_logs`.
- **Related:** The affected UID and incident paths were introduced together in commit `92b4e18`.
- **Status:** DONE

## Follow-up: empty capture after navigation

- **Symptom:** The proxy and Eynorix were connected, but the desktop UI remained at `0 requests`.
- **Root causes:** The installed `EynorixDevDebug` APK trusted only system CAs, so Android rejected the inspection CA. After adding debug-only user-CA trust, the proxy persisted fresh successful requests but React still did not receive live transaction events.
- **Fix:** Added Android `debug-overrides` user-CA trust in the Eynorix network-security configuration; release builds continue to trust system CAs only. Added a 750 ms database-backed refresh while capture is active as a reliable fallback for dropped or missed desktop events.
- **Evidence:** Eynorix produced successful HTTP 200 responses without TLS trust errors. The installed App Tester then displayed 70 live requests, including successful `lms-api.eynorix.xyz` and `lms-admin.eynorix.xyz` endpoints with bodies, status codes, and timings.
