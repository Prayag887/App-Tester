# Changelog

## [0.2.0] - 2026-08-02

### Added

- USB-only Android companion installation, launch, capture relay, and disconnect controls.
- Companion-managed HTTPS CA setup/removal guidance and immediate direct-network recovery.
- Searchable debug-package picker that opens the selected Android application.
- Specific DNS, timeout, connection, DTO, database, memory, and framework log summaries.
- Safe capture import/export with redacted metadata and replay safeguards.

### Changed

- Android capture now uses per-app VPN traffic through `adb reverse` instead of a global device proxy.
- USB loss stops interception on the phone within one failed relay health check.
- Desktop releases embed one signed companion APK across macOS, Windows, and Linux builds.
- Log incidents use one compact, expandable, copyable, redacted evidence block.
- Logcat capture reconnects after transient ADB transport failures.

### Removed

- QR scanning, camera permission, Wi-Fi pairing, wireless ADB handoff, and LAN companion registration.

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
