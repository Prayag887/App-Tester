# Launch Readiness

This checklist defines the minimum bar for an App Tester public release. An
item is complete only when its acceptance criteria are demonstrated on the
release commit. A green build is necessary but never substitutes for device
and installer checks.

## Product capabilities

| Area | Status | Release acceptance criteria |
| --- | --- | --- |
| HTTP(S) capture and redaction | Partial | USB and Wi-Fi devices can capture representative HTTP and HTTPS traffic; sensitive headers, query parameters, and JSON fields are redacted before persistence, display, and export. |
| Android connection recovery | Partial | USB disconnect, Wi-Fi handoff, unavailable ADB, authorization failure, and client-isolated Wi-Fi each result in a clear recovery path without leaving a device proxy configured. |
| Diagnostics | Partial | Logcat monitoring starts, stops, reconnects, and reports actionable incidents for the selected app without collecting unrelated app data. |
| Comparison workflows | Partial | Users can select, pin, edit, and delete baselines; schema differences have clear, testable semantics. |
| Data portability | Partial | Redacted metadata export/import is implemented. Verify export and import on each supported desktop installer and add user-directed destination selection before release. |
| Protocol coverage | Missing | WebSocket behavior, HTTP/2 concurrency, certificate pinning limitations, and HTTP/3 bypasses are either supported and tested or explicitly surfaced before capture starts. |
| Accessibility and recovery | Partial | Main flows work by keyboard, expose visible focus and semantic labels, and cover loading, empty, permission-denied, disconnected, and retry states. |

## Security and privacy

| Area | Status | Release acceptance criteria |
| --- | --- | --- |
| Tauri authority | Needs audit | Capabilities expose only commands required by the shipped windows; every IPC command validates its inputs and no unscoped shell access exists. |
| Local data | Partial | Captures, certificates, and logs have documented retention, deletion, and file-permission behavior; users can clear all persisted capture data. |
| Android companion | Needs audit | The companion uses least privilege, never writes sensitive configuration to shared storage, has a tested fail-open path, and documents its VPN behavior. |
| Dependency response | Missing | Dependency updates and vulnerability reporting have an owner, update cadence, and a documented release response. |

## Release engineering

| Area | Status | Release acceptance criteria |
| --- | --- | --- |
| Automated checks | Partial | The exact release commit passes Rust, desktop, and companion checks on CI. |
| Installers | Partial | Signed, versioned installers are built and smoke-tested on supported macOS architectures, Windows, and Linux. |
| Update and rollback | Missing | Releases document update behavior, installer provenance, rollback steps, and support windows. |
| Release notes | Partial | Every release states user-visible changes, migration notes, known limitations, and security-impacting changes. |

## Current evidence

The following checks were completed locally on 2026-07-30 for commit
`4328f16` before its documentation update:

- Android 17 emulator (`emulator-5554`) discovery, capture startup, fallback
  from an occupied local proxy port, scoped HTTP(S) capture, comparison output,
  Logcat incident reporting, and proxy cleanup were exercised. Cleanup was
  confirmed by checking that the Android global proxy returned to `:0`.
- The rebuilt Companion was installed on that emulator. It opens to an
  explicit **Scan connection code** action without requesting camera access on
  launch. Flutter analysis and its test suite passed with Flutter 3.44.7.
- The macOS ARM64 DMG was built, checksum-verified when mounted, launched from
  the mounted image, quit cleanly, and ejected.
- `pnpm qa:production` passed locally: formatting, clippy, Rust tests, desktop
  tests/type-check/build, Companion analysis/tests, and debug APK build.

This evidence reduces the outstanding device and installer work but does not
change any status to **Implemented**: CI must pass for the exact pushed commit,
Windows/Linux and macOS Intel installers still require native smoke tests, and
the remaining checklist acceptance criteria must be demonstrated.

## Open-source project health

| Area | Status | Release acceptance criteria |
| --- | --- | --- |
| License | Implemented | GPL-3.0 is committed at the repository root and declared in Rust and JavaScript package metadata. |
| Contribution path | Implemented | `CONTRIBUTING.md` explains setup, checks, pull requests, and how to report bugs. |
| Community standards | Implemented | Code of conduct, support guidance, and issue/PR templates are present and maintained. |
| Governance | Implemented | Maintainer roles, decision-making, release ownership, and supported-platform policy are public. |

## Release decision

The project is ready for a public launch only when all items marked **Missing**
or **Needs audit** are resolved and every **Partial** item satisfies its listed
acceptance criteria. The current state is **not ready for launch**.

The checklist follows GitHub's community-health guidance, Tauri's runtime
authority model, and OWASP mobile storage/privacy principles. See
[GitHub community standards](https://docs.github.com/en/communities/setting-up-your-project-for-healthy-contributions),
[Tauri runtime authority](https://v2.tauri.app/security/runtime-authority/), and
[OWASP mobile data storage guidance](https://mas.owasp.org/MASTG/0x05d-Testing-Data-Storage/).
