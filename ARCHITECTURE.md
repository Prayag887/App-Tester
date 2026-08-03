# Architecture

APIQA uses adapters around one Rust core. `apps/cli` and `apps/desktop/src-tauri` translate user input and display errors; business behavior and persistence live in `crates/apiqa-core`.

## Core boundaries

- `engine/mod.rs`: intent-level API facade.
- `engine/run.rs`: run lifecycle, baseline policy, terminalization, and blocking-work isolation.
- `engine/request.rs`: execution classification.
- `engine/transport.rs`: request construction and memory-bounded response streaming.
- `engine/evaluation.rs`: assertions and extraction.
- `engine/variables.rs`: variable resolution and substitution.
- `storage/mod.rs`: SQLite ownership plus collection, environment, settings, and rule repositories.
- `storage/saves.rs`: atomic collection, project, and workspace writes.
- `storage/runs.rs`: incremental run lifecycle and response-body compression.
- `storage/cleanup.rs`: retention and orphaned-blob collection.
- `model.rs`: serialized domain types. Changes require backward-compatible serde defaults or migration.

`ApiQaEngine` owns a private `Store`. Adapters call intent methods rather than reaching into persistence. SQLite, JSON persistence, zstd, and retention work used by async execution run through `spawn_blocking`; network I/O remains async.

HTTP bodies stream to completion for accurate size accounting while retaining at most 5 MiB in memory. Retention is best-effort maintenance after terminal run persistence and cannot invalidate a successful run result.

History lists include execution metadata without decompressing response bodies. Desktop loads one full run through `get_run` only when its report opens. Stored-body decompression validates declared size and enforces the same 5 MiB bound.

## Run persistence

Run metadata remains in shipped `runs` table. New executions append to `run_executions`; each response body is hashed and compressed once into deduplicated `response_blobs`. Readers prefer incremental rows and fall back to executions embedded in legacy run JSON, preserving existing databases without eager rewrites.

Only `Completed` and `CompletedWithFindings` runs qualify as automatic or explicit baselines. `Running` and `Failed` runs never influence comparisons.

## Error policy

Core storage and orchestration return `CoreError`. Parsing/report modules may retain `anyhow` where errors cross format or adapter boundaries. CLI and Tauri convert core errors only at presentation edges.

## Android capture

Android discovery and proxy mutation live in `android.rs`; commands execute `adb` with argument arrays and never interpolate through a host shell. Only authorized devices are eligible for app discovery, and app selection is limited to packages Android marks debuggable.

`capture/proxy.rs` owns local CA and MITM proxy lifecycle. Body frames are forwarded unchanged while a streaming observer retains at most 48 KiB per side; 250 transactions therefore retain at most 24 MiB of payload. Header capture rejects an entire set above 100 fields, 32 KiB total, or 8 KiB per value before retaining strings. Captured headers, absolute/origin-form URL userinfo/query values, JSON and text bodies, and log messages are redacted before reaching core buffers or Tauri events. Transaction events carry initial and terminal snapshots; lagged desktop relays resume from the next available event. HTTPS capture requires Android to trust generated user CA and app network-security policy to permit user certificates; certificate pinning remains opaque.

`capture/logcat.rs` owns one UID-scoped `adb logcat` child with awaited cancellation, at most three bounded reconnect attempts, 16 KiB lines, and `kill_on_drop` fallback. `diagnostics.rs` retains at most 2,000 raw lines and 100 grouped incidents. Tauri owns one serialized capture session: emulator listeners use loopback; physical-device listeners use a random LAN port and authorize only the selected device Wi-Fi source IP parsed from `adb shell ip route`. Unauthorized requests, including CONNECT, receive 403 before forwarding. Stop attempts every cleanup stage and retains serial ownership when Android proxy cleanup must be retried. React disables device/app selection for session duration.
