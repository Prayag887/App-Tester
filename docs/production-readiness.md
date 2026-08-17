# Production Readiness

`pnpm qa:production` is the local automated production gate. A green result
means this workspace passes formatting, static analysis, automated tests, and
local builds across the Rust core, desktop UI, and Android Companion.

It does **not** certify a release by itself. A release owner must record the
following manual checks in the release pull request or release notes; any
unchecked item makes the release not production-ready.

## Automated gate

Run from the repository root:

```bash
pnpm qa:production
```

The command checks formatting and linting, Rust tests, desktop unit/type/build
checks, and Flutter analysis/tests/signed release-APK build. It is intentionally
fail-fast: address the first failure and rerun the full gate.

## Manual release checklist

- [ ] Test the signed desktop installers on Windows and Linux; launch, quit,
  and relaunch each.
- [ ] Connect an authorized physical Android device over USB; verify discovery,
  authorization-denied, unplugged, empty, and error states. Reconnect is not a
  supported lifecycle: every new USB connection starts a new capture.
- [ ] Exercise traffic capture, redaction, persistence/restart, comparison, and
  companion VPN fail-open recovery using non-sensitive test traffic.
- [ ] Verify keyboard navigation, visible focus, screen-reader labels, scaling,
  and contrast on the changed UI flows.
- [ ] Review Tauri capabilities, dependency upgrades, migrations, and logs for
  least privilege and absence of secrets or raw sensitive payloads.
- [ ] Confirm version, changelog, release notes, artifact names, signatures,
  and rollback instructions.
- [ ] Verify `latest.json` references the signed Windows NSIS and Linux
  AppImage updater artifacts, then upgrade an installation of the preceding
  version on both platforms.

Production readiness is decided from the local gate plus the manual USB-device
and native-installer evidence above. Repository checks may protect code quality,
but they are not used as production-readiness evidence.
