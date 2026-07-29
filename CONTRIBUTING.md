# Contributing to App Tester

Thanks for helping improve App Tester. By contributing, you agree to follow
the project's [Code of Conduct](CODE_OF_CONDUCT.md).

## Before opening a pull request

1. Search existing issues and discussions for related work.
2. Keep a change focused; explain the user-facing behavior and any privacy or
   device-impacting consequences.
3. Add or update tests for changed behavior. Do not include credentials,
   certificate keys, unredacted traffic, device identifiers, or customer data.

## Local checks

Install Rust, pnpm, Android Platform Tools, and Flutter for companion changes.

```bash
pnpm install
pnpm qa:production
```

If Flutter or a physical device is unavailable, run the applicable targeted
checks and explain the limitation in the pull request. Never report a release
as ready without the full gate and the manual release checklist.

## Pull requests

Use a focused branch name such as `fix/logcat-reconnect` or
`feature/redacted-export`. Describe the problem, solution, verification, and
any migration, security, or rollback implications. Maintainers may request
changes for safety, privacy, compatibility, or scope before merging.

## Reporting bugs and requesting features

Use the issue forms. For help using the project, see [SUPPORT.md](SUPPORT.md).
For a vulnerability, follow [SECURITY.md](SECURITY.md) and do not open a public
issue.
