# APIQA Working Memory

Read this file first when locating behavior. `ARCHITECTURE.md` explains design constraints; this file maps concrete ownership and common change paths.

## Workspace map

- `crates/apiqa-core`: shared Rust behavior used by desktop and CLI.
- `apps/cli`: command-line argument and process-exit adapter.
- `apps/desktop/src-tauri`: Tauri commands, app-data setup, and error presentation.
- `apps/desktop/src`: React UI and TypeScript transport types.

## Core ownership

### Engine

- `engine/mod.rs`: `ApiQaEngine` facade and intent-level persistence methods.
- `engine/run.rs`: collection/run lifecycle, baseline validation, terminal states, blocking-work boundary, best-effort retention.
- `engine/request.rs`: turns one HTTP attempt into `RequestExecution` and classifies result state.
- `engine/transport.rs`: HTTP request construction, authentication/body encoding, streamed response capture, 5 MiB memory bound, request/transport errors.
- `engine/evaluation.rs`: response assertions, JSON/header extraction.
- `engine/variables.rs`: collection/environment variable precedence and substitution.
- `engine/run_tests.rs`: lifecycle and end-to-end engine tests.
- `engine/transport_tests.rs`: request validation and response-size tests.

### Persistence

- `storage/mod.rs`: SQLite connection ownership; collection/environment/settings/rule reads.
- `storage/schema.sql`: additive SQLite schema.
- `storage/saves.rs`: collection/environment writes and atomic project/workspace transactions.
- `storage/runs.rs`: incremental run metadata/executions, bounded body loading, body hashing, zstd deduplication, legacy-run reads, summaries/counts.
- `storage/cleanup.rs`: age/byte retention, shared-blob reference accounting, orphan collection.
- `error.rs`: typed `CoreError` boundary.

### Domain and formats

- `model.rs`: persisted and IPC domain types. Compatibility-sensitive.
- `import.rs`: Postman collection/environment parsing and supported script extraction.
- `bundle.rs`: `.apiqa` project and workspace import/export plus secret-value sanitization.
- `compare.rs`: deterministic status/header/JSON/text/timing comparison.
- `report.rs`: JSON, HTML, and JUnit rendering.
- `lib.rs`: deliberate public core surface.

### Android capture

- `android.rs`: ADB discovery, authorized-device metadata, debuggable-app filtering, UID lookup, safe Wi-Fi route source parsing, and direct-argument system proxy control.
- `capture/proxy.rs`: generated local CA, source-authorized frame-preserving streaming capture, random-port HTTP(S) MITM proxy lifecycle, and 250-transaction retention.
- `capture/transaction.rs`: bounded, redacted capture DTOs; each body retains at most 48 KiB, header sets are rejected above count/byte bounds, and absolute or relative URLs never retain userinfo or query values.
- `capture/logcat.rs`: managed UID-scoped `adb logcat` child lifecycle, awaited cancellation, bounded reconnects, and 16 KiB input-line limit.
- `diagnostics.rs`: logcat parsing/redaction plus bounded raw (2,000) and grouped diagnostic (100) buffers.
- `apps/desktop/src/useCaptureSession.ts`: Android capture lifecycle; takes one bounded state snapshot on load/start and then consumes Tauri capture events, avoiding repeated transfers of retained logs, diagnostics, and transactions. The UI retains only its latest 80 log lines; core retains the diagnostic 2,000-line buffer.
- USB devices can be handed off to ADB Wi-Fi (`adb tcpip 5555` followed by explicit Wi-Fi-IP connect) through the API Hits setup UI; this is a user-initiated transport change and leaves normal capture ownership unchanged.
- `apps/desktop/src/ApiHitsView.tsx`: dedicated live Android traffic screen and hit inspector.
- `apps/desktop/src/IssueTriageView.tsx`: dedicated regression/failure triage screen with baseline-to-current response comparison.

## Runtime flow

1. Adapter invokes `ApiQaEngine` intent method.
2. `run.rs` loads eligible baseline through blocking storage boundary.
3. Run metadata is inserted as `Running`.
4. `transport.rs` streams HTTP response and retains at most 5 MiB.
5. `evaluation.rs` evaluates assertions/extractions; `compare.rs` computes differences.
6. `runs.rs` appends execution and stores unique compressed body once.
7. Run reaches `Completed`, `CompletedWithFindings`, or `Failed`; started runs are terminalized after recoverable errors.
8. Retention runs as best-effort maintenance and cannot invalidate successful run result.

## Persistence compatibility

- `runs.data` stores run metadata for new runs and may contain embedded executions for shipped legacy runs.
- `run_executions` stores new executions incrementally by `(run_id, position)`.
- `response_blobs` stores SHA-256-addressed zstd bodies shared across executions.
- Readers prefer incremental executions and fall back to embedded legacy executions.
- Never remove serde fields or alter enum wire names without migration and fixtures.

## Common changes

- HTTP/auth/body behavior: `engine/transport.rs`.
- Assertions/extractions: `engine/evaluation.rs`, then `model.rs` if contract changes.
- Run state/baseline behavior: `engine/run.rs` and `storage/runs.rs`.
- History performance/retention: `storage/runs.rs` and `storage/cleanup.rs`.
- Postman support: `import.rs`.
- Desktop command: `apps/desktop/src-tauri/src/lib.rs`, then `apps/desktop/src/api.ts` and `types.ts` if IPC changes.
- CLI command/exit code: `apps/cli/src/main.rs`.
- Report behavior: `report.rs`; comparison semantics: `compare.rs`.

## Verification

Use commands listed in `AGENTS.md`. Rust core currently has lifecycle, bounded-response, deterministic-comparison, JUnit, atomic-save, deduplication, cleanup, and legacy-run tests.

Android capture checks are included in workspace tests. Local end-to-end verification additionally requires authorized ADB device, debuggable app, and user-installed APIQA CA for HTTPS interception.

Desktop `start_capture` owns selected serial, random proxy port, Android proxy configuration, and logcat as one serialized lifecycle. `stop_capture` always attempts logcat, Android proxy, and local proxy cleanup; failed device cleanup retains ownership so stop can be retried. Emulator proxy listens on loopback; physical-device proxy listens on LAN only for owned-session duration and rejects every source except selected device Wi-Fi IP. Capture relays survive broadcast lag, and transactions emit initial plus terminal snapshots to bound event amplification.
