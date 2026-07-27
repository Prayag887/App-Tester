# Complete debug package discovery

- Symptom: companion connected, but desktop package picker omitted some debuggable apps.
- Root cause: companion queried only `MAIN/LAUNCHER` activities. That returns apps with launcher icons, not every installed package marked debuggable.
- Fix: request package visibility, enumerate installed applications, filter by Android `FLAG_DEBUGGABLE`, and report package name plus label. Companion bumped to `0.2.1+3` with immutable APK URL.
- Preserved behavior: desktop still selects package after pairing; only selected package enters VPN; release/system apps remain excluded; no phone data entry added.
- Verification: Flutter analyze/tests, signed APK build, desktop tests/typecheck, Rust checks, desktop release build, release upload, local installation, and GitHub merge.
- Distribution note: `QUERY_ALL_PACKAGES` is appropriate for this sideloaded developer diagnostics tool but would require policy justification for Google Play distribution.
