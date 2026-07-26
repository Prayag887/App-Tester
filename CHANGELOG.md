# Changelog

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
