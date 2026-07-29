# Production Readiness

`pnpm qa:production` is the single automated production gate. It executes the
same checks locally and in GitHub Actions, so a green local result and CI result
mean the tested source is buildable and passes static analysis and automated
tests across the Rust core, desktop UI, and Android companion.

It does **not** certify a release by itself. A release owner must record the
following manual checks in `memory/SESSION_LOG.md`; any unchecked item makes
the release not production-ready.

## Automated gate

Run from the repository root:

```bash
pnpm qa:production
```

The command checks formatting and linting, Rust tests, desktop unit/type/build
checks, and Flutter analysis/tests/release-APK build. It is intentionally
fail-fast: address the first failure and rerun the full gate.

## Manual release checklist

- [ ] Test the signed desktop installers on macOS (Apple Silicon and Intel),
  Windows, and Linux; launch, quit, and relaunch each.
- [ ] Connect a supported USB Android device and an emulator or wireless device;
  verify discovery, permission-denied, reconnect, empty, and error states.
- [ ] Exercise traffic capture, redaction, persistence/restart, comparison, and
  companion VPN fail-open recovery using non-sensitive test traffic.
- [ ] Verify keyboard navigation, visible focus, screen-reader labels, scaling,
  and contrast on the changed UI flows.
- [ ] Review Tauri capabilities, dependency upgrades, migrations, and logs for
  least privilege and absence of secrets or raw sensitive payloads.
- [ ] Confirm version, changelog, release notes, artifact names, signatures,
  and rollback instructions.
- [ ] Confirm required GitHub Actions are green for the exact merge commit.

## CI enforcement

The `Production QA` workflow runs on pull requests and pushes to `main`.
Repository administrators must protect `main` and require its `production-qa`
check before merging. The workflow is the automated guard; branch protection
prevents bypassing it.
