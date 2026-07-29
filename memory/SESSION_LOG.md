# Session Handoff Log

This append-only, redacted log is the authoritative handoff record for future
sessions. Read it before making changes and append an entry after every
meaningful change. See `../AGENTS.md` for the required fields.

## 2026-07-29 — Delivery contract and production gate

- **Intent:** Establish a mandatory, scalable engineering workflow for every
  update, upgrade, and feature.
- **Changed:** Added the repository delivery contract, this handoff log,
  production-readiness documentation, a shared production QA command, and CI
  parity for the desktop and Android companion.
- **Verification:** `pnpm qa:production` passed Rust formatting/clippy and 32
  Rust tests, desktop type/unit checks (12 tests), and the desktop production
  build. The companion stage is blocked before execution because macOS denies
  access to the configured Flutter executable at
  `/Users/prayag/Documents/flutter_sdk/flutter/bin/flutter` (exit 126).
- **Risks / follow-up:** Android emulator/device smoke testing and signing are
  intentionally manual release checks because they require release credentials
  or hardware. Configure GitHub branch protection to require the `Production
  QA` workflow before direct merges to `main`.
- **Next handoff:** Restore access to a trusted Flutter SDK, rerun
  `pnpm qa:production`, then commit this coherent change, push `main`, confirm
  GitHub Actions, and record its result below.

## 2026-07-29 — USB capture to Wi-Fi handoff

- **Intent:** Keep an already active USB desktop capture functional after the
  phone is moved to Wi-Fi ADB and unplugged, without relying on the companion
  VPN.
- **Changed:** Allowed the active-capture handoff, transferred Android proxy
  cleanup ownership to the Wi-Fi serial, restarted scoped logcat, documented
  the flow, and added a regression test.
- **Verification:** Desktop check passes 13 tests and TypeScript validation;
  workspace Rust tests pass 32 tests. Physical-device verification remains
  required. The full production gate remains blocked by the inaccessible local
  Flutter SDK recorded above.
- **Risks / follow-up:** Legacy ADB-over-TCP/IP requires the phone and desktop
  to stay on the same Wi-Fi; disconnecting or changing networks still requires
  reconnection. Do not advertise a release as production-ready before a device
  handoff and hosted CI have passed.
- **Next handoff:** Perform the physical handoff scenario, record the result,
  and then resolve the Flutter SDK access before release qualification.
