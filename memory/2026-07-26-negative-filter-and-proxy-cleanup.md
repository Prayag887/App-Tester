# Debug report — negative filter and Android proxy cleanup

- **Symptom:** Typing a domain such as `google.com` did not hide matching traffic, the negative filter offered no captured-endpoint suggestions, and a physical Android device lost internet access after HTTPS certificate setup.
- **Root cause:** The negative filter compared only exact full URLs. The capture workspace redesign also removed the only Stop action while leaving cleanup code unreachable, and every device was configured with the emulator-only proxy host `10.0.2.2`.
- **Fix:** Match exclusions by full URL prefix, exact host, parent domain, or host/path prefix; show deduplicated captured endpoints while typing; keep selection within visible traffic; restore Stop; use `10.0.2.2` only for emulators and the Mac's LAN IPv4 address for physical devices; remember and clear the configured Android proxy when the desktop process exits.
- **Evidence:** The connected physical device was confirmed at `10.0.2.2:8080`, cleared with ADB, and verified at `:0`. `pnpm --dir apps/desktop check`, `pnpm --dir apps/desktop build`, and `cargo test --workspace` pass.
- **Regression test:** `apps/desktop/src/App.test.ts` covers host/domain/path matching and endpoint suggestion filtering/deduplication.
- **Related:** Commit `e7d9a3a` introduced exact-only exclusions and removed the Stop control during the capture UI redesign.
- **Status:** DONE
