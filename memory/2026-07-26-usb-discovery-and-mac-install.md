# Debug report — USB discovery and Mac installation

- **Symptom:** A OnePlus device connected over USB was not available in App Tester unless another connection flow was used.
- **Root cause:** ADB saw the authorized USB transport, but the desktop UI discovered devices only once during startup. Plugging a phone in after App Tester opened could never update the device selector.
- **Fix:** Poll device discovery every two seconds, preserve a still-valid explicit selection, and otherwise prefer an authorized USB device. The previously corrected proxy lifecycle remains in place.
- **Evidence:** ADB reports OnePlus CPH2493 (`JFR8T8YDFI9955MB`) as an authorized USB device. The phone proxy remains cleared at `:0`. The release bundle was built, ad-hoc signed, copied to `/Applications/App Tester.app`, checksum-verified against the build output, restarted, and observed running from `/Applications` at 11:03:15.
- **Certificate:** The current App Tester CA (SHA-256 `5D:C4:18:6A:93:0A:2F:F0:23:30:3E:2F:5F:6E:C2:F7:EF:42:56:26:76:93:AD:28:39:34:A5:EE:23:D3:DA:2A`) was copied to the OnePlus Downloads folder and Android's certificate installer was opened. Android deliberately requires the user to approve or remove a user CA on-device.
- **Regression test:** `apps/desktop/src/App.test.ts` verifies USB preference, selection preservation, and disconnected-device recovery.
- **Verification:** `cargo test --workspace` passes 26 tests; `pnpm --dir apps/desktop check` passes 8 tests and TypeScript validation.
- **Status:** DONE_WITH_CONCERNS — application and USB fix are verified; Android does not expose a non-root ADB API to verify the user's final CA confirmation.
