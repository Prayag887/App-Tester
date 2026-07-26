# App Tester Companion

The companion protects a development device from a stale App Tester proxy. It is an Android device-owner application: when capture is armed, it applies the desktop proxy and runs a foreground health monitor. If the desktop proxy cannot be reached three times, it clears the proxy and direct networking resumes.

## Why this is a companion

The desktop application cannot clear an Android proxy after the desktop has crashed, powered off, or lost access to the device. Android only permits an app to manage a global proxy when it is the **device owner**. The companion uses that authority only for the proxy lifecycle.

It is intentionally not a packet-inspecting VPN. Android VPNs must forward every packet themselves; a partial implementation would risk blackholing traffic. This companion keeps the existing desktop proxy capture model and makes its device-side lifecycle fail open.

## Setup

Use a dedicated development device. Device-owner provisioning changes how Android manages the device and normally requires a newly provisioned or factory-reset device.

1. Build and install this app on the development device.
2. Provision it as the device owner using your organization’s Android Enterprise / test-device process.
3. Open the companion and enter the Mac's LAN address and App Tester proxy port (normally `8080`).
4. Start **protected capture**. The companion applies the proxy and shows a persistent notification.
5. Start the desktop capture. Keep the phone and Mac on a network where the phone can reach the Mac.

If the Mac disappears, the companion attempts three TCP connections at five-second intervals and then clears the proxy. The worst-case recovery interval is about 15 seconds plus Android scheduling overhead.

## Development

```bash
cd apps/companion
flutter pub get
flutter analyze
flutter test
flutter build apk --debug
```

If `android/local.properties` is not created by Flutter, add one containing `flutter.sdk=/absolute/path/to/flutter`.

The checked-in release configuration uses the local Android debug signing key so the generated development APK is installable by QR. Configure your managed release keystore before distributing outside the development team.

The package is feature-first:

- `lib/features/proxy_safety/data`: Android channel and repository contract.
- `lib/features/proxy_safety/presentation`: view model and declarative UI.
- `android/...`: device-owner proxy controller and foreground health service.

The Flutter layer never touches Android policy APIs directly; the native controller is the only platform authority boundary.
