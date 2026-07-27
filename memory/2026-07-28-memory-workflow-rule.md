# Memory workflow rule

- Goal: prevent new work from silently dropping previously shipped fixes.
- Decision: repository agents must read relevant `memory/` logs before editing and write a dated log after every logical change.
- Change: added root `AGENTS.md` with read-before-edit, post-change logging, verification, and supersession rules.
- Verification: rule and this log are committed together.
