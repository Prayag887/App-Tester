# Repository workflow

Before changing code:

1. Read every relevant file in `memory/` to preserve shipped behavior and prior fixes.
2. Check recent commits for affected files before forming a solution.

After each logical change:

1. Add or update a dated `memory/YYYY-MM-DD-<topic>.md` log.
2. Record symptom or goal, root cause or decision, files/behavior changed, verification, and remaining concerns.
3. Commit source and its memory log together so context cannot be separated from the change.

Do not remove or reverse behavior documented in `memory/` without explicitly recording why it is being superseded.

# Delivery Contract for App Tester

These instructions apply to every update, upgrade, refactor, dependency change,
bug fix, and feature addition in this repository. They are a mandatory delivery
contract, not suggestions.

## Non-negotiable engineering standard

Treat App Tester as a security-sensitive, local-first desktop product. Preserve
the same observable behavior unless the change explicitly describes and tests a
user-visible behavior change. Prefer small, composable boundaries; typed data
at process/UI boundaries; explicit error, cancellation, and recovery paths; and
least-privilege Tauri capabilities. Never expose, persist, log, or render
unredacted secrets, traffic, certificates, or personally identifying data.

Every meaningful change must include, in the same change set:

1. A focused test or an explicit, documented reason a test is infeasible.
2. Updated user/developer documentation when behavior, setup, architecture,
   privacy, release procedure, or limitations change.
3. A dated entry in `memory/SESSION_LOG.md` describing intent, files/areas
   changed, verification, known risks, and the next handoff.
4. A conventional Git commit once that coherent change is verified. Do not mix
   unrelated work in one commit.

Before calling work production-ready, run `pnpm qa:production` successfully
from the repository root and complete the manual release checklist in
`docs/production-readiness.md`. A failed, skipped, or unavailable check means
the release is **not ready**; record the reason and owner in the session log.

## Session continuity

At the start of a session, read `memory/SESSION_LOG.md` and the newest relevant
entry under `memory/`. Before ending, append a handoff entry to
`memory/SESSION_LOG.md`. Do not rewrite historical entries. The log must be
safe to commit: use issue IDs and redacted summaries, never credentials,
device IDs, URLs with tokens, or captured payloads.

## Change workflow

1. Inspect the affected architecture, public contract, current tests, and
   `git status`; preserve unrelated user changes.
2. State the acceptance criteria and risk level. For security, persistence,
   proxy/VPN, Tauri commands/capabilities, migrations, or releases, include a
   negative-path test and a rollback/recovery note.
3. Implement the smallest complete vertical slice. Keep UI, Rust core, Tauri,
   and companion contracts compatible or version their change deliberately.
4. Run targeted tests while iterating, then the production gate before release.
5. Append the session log, review the diff, and create a conventional commit.
6. Push the verified branch. Merge to `main` only after required CI is green
   and the production gate/manual checklist are satisfied. Resolve conflicts by
   rerunning the affected checks and recording the result.

## Quality bar

- Accessibility: keyboard operation, visible focus, semantic controls, readable
  status/error states, and no color-only meaning.
- Desktop resilience: no UI-thread blocking for I/O; bounded work; cancellation;
  restart/reconnect behavior; clear offline/permission/device states.
- Security/privacy: validate all IPC inputs; minimize capabilities; redact at
  the source; do not weaken TLS, authentication, or certificate safeguards.
- Compatibility: preserve migrations and stored data, and test representative
  success, failure, and empty/loading states.
- Release integrity: version/changelog/release notes are consistent; generated
  artifacts are reproducible; supported OS installers are built and smoke-tested.

If a request conflicts with this contract, explain the conflict and obtain an
explicit exception documented in `memory/SESSION_LOG.md` before proceeding.
