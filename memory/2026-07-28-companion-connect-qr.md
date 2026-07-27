# Companion connection QR fixed

- Symptom: clicking `Connect companion` sometimes showed no QR.
- Cause: connection generation trusted a host cached during startup. A resolving or failed cache caused an early return or invalid QR request, with no visible loading state.
- Fix: resolve the current desktop network address on every click, keep the action clickable, show `Preparing QR...`, and report missing-package or host-resolution errors directly.
- Verification: desktop unit tests, TypeScript build, release app build, local installation, and GitHub merge.
