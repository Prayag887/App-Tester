# apiqa-core architecture rules

## Layer dependency direction

```
infrastructure ──► application ──► domain
interop ─────────► application ──► domain
desktop/Tauri ───► application
```

**NOT:** application → infrastructure.

## Layer responsibilities

### domain/
Pure business rules. Depends on nothing outside domain.

Must not import: `crate::persistence`, `crate::proxy`, `crate::adb`, `rusqlite`,
`reqwest`, `hudsucker`, `tauri`, `std::process::Command`.

Handles: validation, variable resolution rules, redaction policies, request
normalization, history fingerprints, assertion evaluation, data transformations.

### application/
Use cases AND port traits. May depend on domain.

Must not depend on concrete infrastructure implementations.

OWNS the trait definitions that infrastructure implements (e.g. `HttpTransport`,
`HistoryRepository`, `CollectionRepository`). Infrastructure modules import
these traits from application and implement them.

### infrastructure/
Implements application ports using reqwest, rusqlite, ADB, filesystem, proxy
libraries, WebSocket libraries, and Tauri adapters. Depends on application
(for ports) and domain (for types).

### interop/
External format adapters (Postman v2.1, curl import). Converts external
representations into application input models and vice versa. Depends on
application for command/output types.

Must NOT call repositories or own persistence logic. The conversion chain is:

```
External JSON → Interop adapter → Application model → Application use case → Repository port → Infrastructure impl
```

### desktop/Tauri
Thin command layer. Calls application use cases. Depends on application.
Never reaches into infrastructure directly except to set up dependency injection.

## Migration rules

- `crates/apiqa-core/migrations/` contains forward-only migration files named
  `NNNN_description.sql`, embedded in the binary via `include_str!`
- Migrations are immutable after release — the `checksum` column in
  `schema_migrations` enforces this by rejecting a database whose already-applied
  migration checksum differs from the compiled migration
- Each migration runs inside a SQLite transaction; a failed migration rolls back
  its own partial changes
- Startup backs up the database using the SQLite backup API before applying
  new migrations. On failure: close all handles, restore backup, reopen,
  verify integrity, return error
- Legacy databases (no `schema_migrations` table) are detected, validated against
  the expected initial schema, bootstrapped, then migrated forward
- A database at a version higher than the app knows about is rejected — never
  silently downgraded

## Enforcement

- A source-boundary test in `lib.rs` scans `domain/*.rs` for forbidden imports
  and fails the build if any are found
- `pub(crate)` visibility is used aggressively in infrastructure modules to
  prevent accidental exposure of internals
- The `#[deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` lint
  (non-test only) is enforced at the crate level
