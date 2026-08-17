# Changelog

## [0.2.9] - 2026-08-17

- Replaced wireless, emulator, and QR connection paths with a single physical-USB capture flow backed by ADB reverse and the per-app Companion VPN.
- Reworked the desktop traffic inspector with resizable panels, bounded WebView payloads, lazy transaction details, and a virtualized request list.
- Simplified Companion capture status and fail-open handling for explicit USB sessions.
- Hardened logcat incident boundaries and subprocess coverage under interactive release checks.

## [0.2.8] - 2026-08-12

- Simplified Composer by removing collections, environments, and request history.
- Added runnable cURL copying for requests configured in Composer.
- Made Windows and Linux releases tag-driven with duplicate-release protection.

## [0.1.2] - 2026-07-30

### Added

- Safer capture import and export with redacted metadata and replay safeguards.
- More resilient USB-to-Wi-Fi ADB handoff and proxy/CA cleanup recovery.

### Changed

- Log incidents now present a single copyable, redacted evidence block.
- TLS reports select the actual certificate failure and give certificate-specific reproduction guidance.

## [0.1.1] - 2026-07-26

### Added

- Dark indigo traffic-inspector interface with capture metrics and clearer comparison status.
- Exact full-endpoint negative filtering with removable exclusion chips.
- Broader Android logcat error and warning correlation scoped to the selected app UID.

### Changed

- Response comparisons now report JSON key, type, and nullability changes while ignoring scalar values and array lengths.
- Android package UID detection now supports modern package-manager output and additional package dump formats.

## [0.1.0] - 2026-07-23

### Added

- New App Tester product foundation replacing the previous API client.
- Local ADB discovery for USB, wireless, and emulator devices.
- Authorization status and enriched Android device metadata.
- Responsive desktop device-selection screen and JSON diagnostics CLI.
- Parser, classification, and presentation tests.
- Cross-platform validation and release workflow support.

### Removed

- Postman collection import, HTTP request execution, response history, and the previous APIQA interface.
