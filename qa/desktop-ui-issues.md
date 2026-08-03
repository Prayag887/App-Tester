# Desktop UI QA log

## 2026-08-03 — live-app regression pass

- **Fixed:** collapsed Log Inspector cards still retained all diagnostic content
  in the layout. On a short window this created large blank gaps and made the
  actual expandable content appear cut off. The details are now mounted only
  for the selected expanded card; the existing Log Inspector viewport scrolls
  independently of the application shell.
- **Fixed:** a transient/incomplete package discovery result cleared the target
  package while a USB capture was still active. The active target is now
  preserved for the lifetime of that capture.
- **Observed:** the live Android device generated Logcat network failures
  (`SSLHandshakeException`, `UnknownHostException`, and `SocketException`) but
  no new proxy transactions were persisted to the active desktop session.
  This is a capture-path issue, not a Traffic Lab filtering issue. It remains
  open for the next capture integration pass.
- **Controlled traffic check:** a direct `curl` request configured for the
  advertised proxy port could not establish a TCP connection, despite the app
  previously advertising the port. This confirms that the live proxy endpoint
  needs lifecycle verification before its results can be used to validate the
  Traffic Lab UI.

## Verification to run after build

1. Start a USB companion capture with a selected package.
2. Leave Traffic Lab visible and send a controlled request through the active
   proxy endpoint; confirm a row appears without navigating away.
3. Switch to Log Inspector, expand several cards, reduce the window height,
   and verify the viewport and each raw-log box remain scrollable.
