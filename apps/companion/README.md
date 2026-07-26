# App Tester Companion

The companion is a regular Android application: it does not request device-owner, device-admin, accessibility, or root access. It provides a branded desktop-link screen and a persistent activity log so a person can verify that the desktop endpoint is reachable before starting a desktop capture.

## What it does today

1. Accepts the **Desktop host** displayed in green by the desktop App Tester capture header.
2. Monitors that endpoint in a foreground service and records health events in the app.
3. Stops monitoring safely when the desktop endpoint is unreachable. It never modifies the phone's global network settings.

## What a production fail-open capture needs

Android does not permit a normal app to set or clear the global HTTP proxy. A production implementation therefore needs a complete `VpnService` packet-forwarding engine (TUN/TCP/UDP/DNS handling) which routes traffic to the desktop capture proxy while it is available and returns it directly when it is not. It uses Android's ordinary, user-approved VPN consent prompt—not device-owner enrollment.

This repository intentionally does not claim to provide that relay until that packet-forwarding engine is included and independently tested. A partial `VpnService` implementation can blackhole device traffic, which is worse than the failure it is meant to solve.

## Build and validate

```sh
cd apps/companion
flutter pub get
flutter analyze
flutter test
flutter build apk --release
```

Release signing remains required. Create the ignored `android/signing.properties` locally with the keystore values described by the Android build error.

## Structure

- `lib/features/proxy_safety/`: desktop-link state, repository, view model, and screen.
- `lib/shared/brand/`: the code-native App Tester mark shared visually with desktop branding.
- `android/...`: foreground endpoint monitor and Flutter method channel.
