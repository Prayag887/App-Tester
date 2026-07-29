# Debug report — USB capture to Wi-Fi handoff

- **Symptom:** Starting desktop capture through USB and unplugging the phone
  either stopped app log collection or left cleanup targeting the unavailable
  USB serial. The companion VPN was reported unreliable, but it is not part of
  this desktop capture path.
- **Root cause:** The desktop UI disabled **USB to Wi-Fi** while capture was
  active. Its inactive-only handoff also changed the selected device without
  moving managed proxy ownership or launching logcat against the new Wi-Fi ADB
  serial. `adb tcpip` intentionally tears down the USB transport, so the
  existing logcat child exited at that point.
- **Fix:** Allow handoff during capture. Once ADB connects to the Wi-Fi
  endpoint, select it immediately, reconfigure the same Android proxy through
  that endpoint to transfer cleanup ownership, then restart app-scoped logcat.
  The existing capture session and proxy remain running.
- **Regression test:** `apps/desktop/src/App.test.ts` checks that an active
  handoff requires both proxy ownership refresh and logcat restart.
- **Evidence:** `pnpm --dir apps/desktop check` passes 13 tests and TypeScript
  validation; `cargo test --workspace` passes 32 tests.
- **Status:** DONE_WITH_CONCERNS — source-level lifecycle behavior is covered,
  but final verification requires an authorized physical device on the same
  Wi-Fi network: start capture over USB, click **USB to Wi-Fi**, wait for the
  confirmation, unplug USB, generate app traffic, and stop capture.
