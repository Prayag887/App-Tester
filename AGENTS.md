# Project Engineering Rules

Read `memory.md` before changing code. Update it in the same change whenever module ownership, persistence shape, public commands, or verification commands change.

## Rust architecture

- Keep business behavior in `crates/apiqa-core`; CLI and Tauri remain thin adapters.
- Keep SQLite, compression, and filesystem work off async executor threads. Use the engine blocking boundary.
- Preserve shipped SQLite and serde compatibility. Use additive schema changes, explicit migrations, or tested legacy fallbacks.
- Persist runs incrementally. Never rewrite or recompress complete run history for one execution.
- Bound network and file inputs before buffering. Resource limits must constrain actual allocation, not truncate after allocation.
- Prefer typed core errors. Convert errors to strings only at CLI/Tauri presentation boundaries.
- Keep output deterministic. Avoid unordered iteration where ordering reaches reports, snapshots, or user-visible comparisons.

## Maintainability

- Organize modules by one stable responsibility. Split a file when unrelated reasons to change accumulate, not only when line count grows.
- Treat 250 production lines as review threshold, not hard limit. Tests may live in dedicated sibling test modules.
- Avoid pass-through abstractions, speculative traits, and one-function modules unless they enforce a real boundary.
- Keep public APIs intent-based. Do not expose storage internals to adapters.
- Add regression tests for lifecycle failure, resource bounds, persistence compatibility, and transaction atomicity.

## Required gates

Run before completion:

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
pnpm --dir apps/desktop check
pnpm --dir apps/desktop build
```
