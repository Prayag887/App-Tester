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
- **Physical investigation:** A USB-authorized phone successfully entered ADB
  TCP mode and listened on port 5555, but the desktop could not reach that port
  or resolve the phone at link layer while both reported the same Wi-Fi subnet.
  This conclusively identifies Wi-Fi client/AP isolation, outside the VPN and
  application layers. The phone was returned to USB-only ADB after the test.
- **Follow-up fix:** Split TCP-mode preparation from the ADB handshake and
  verify the endpoint with a five-second TCP preflight first. The app now
  reports an unreachable/isolation error promptly instead of hanging during
  `adb connect` or reporting a successful handoff.
- **Evidence:** `pnpm --dir apps/desktop check` passes 14 tests and TypeScript
  validation; `cargo test --workspace` passes 33 core tests plus the desktop
  unit test.
- **Status:** DONE_WITH_CONCERNS — the failure mode is verified and handled;
  a successful unplugged-capture smoke test still requires a network that
  permits direct desktop-to-phone traffic.
