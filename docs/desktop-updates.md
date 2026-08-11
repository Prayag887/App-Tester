# Desktop updates

App Tester uses Tauri's signed updater for the Windows NSIS and Linux AppImage
bundles. The Android companion APK is bundled with the desktop release but is
not part of the desktop updater.

## Release path

1. Increase the application version in every versioned workspace manifest.
2. Merge the release commit to `main`.
3. The `Release Windows and Linux` workflow builds the Windows NSIS installer
   and Linux AppImage, signs their updater archives, and uploads the archives,
   signatures, and `latest.json` to the matching GitHub Release.
4. Verify that the release contains `latest.json` and signed updater artifacts
   for both `windows-x86_64-nsis` and `linux-x86_64` before announcing it.
5. Smoke-test an upgrade from the preceding release on native Windows and
   Linux installations.

The application checks
`https://github.com/Prayag887/App-Tester/releases/latest/download/latest.json`
shortly after startup. A person must approve **Update and restart** before any
update is installed. A manual check is available in the top bar.

## Signing keys

The application contains only the updater public key. GitHub Actions owns the
encrypted private material through these repository secrets:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

Never commit, print, or attach the private key or password to a release. Access
to those secrets must be limited to release maintainers. Keep an encrypted,
offline backup of the private key and password: losing them prevents installed
versions from trusting future updates.

If the private key may have been disclosed, rotate it before publishing any
release. Because an installed application trusts its embedded public key, a
normal key rotation requires publishing one final update signed by the old key
that embeds the new public key. If the old private key is unavailable, existing
users must install the new release manually.

## Failure behavior

- Background-check failures do not interrupt startup or capture.
- Manual-check and installation failures remain recoverable and never delete
  the installed version.
- Update metadata and artifacts are authenticated with Tauri's updater
  signature before installation.
- Windows uses a per-user NSIS installation and passive updater UI; it does not
  require administrator privileges.

Windows Authenticode signing is separate from Tauri updater signing. Add a
trusted Windows code-signing certificate before calling the Windows installer
production-ready.
