# App Tester Companion

The companion is a regular Android application: it does not request device-owner, device-admin, accessibility, or root access. It provides a branded desktop-link screen and a persistent activity log so a person can verify that the desktop endpoint is reachable before starting a desktop capture.

## What it does

1. Accepts the **Desktop host** displayed in green by the desktop App Tester capture header and the Android package selected for capture.
2. Requests Android's normal one-time VPN consent and routes **only that package** through the desktop HTTP capture proxy.
3. Runs a complete `tun2socks` packet-forwarding engine in the foreground VPN service, including TCP, UDP, DNS, IPv4, and IPv6 handling.
4. Checks the desktop endpoint every five seconds. After three failures, it stops the VPN; Android restores the selected app's direct networking automatically.
5. Retains endpoint and VPN lifecycle events in the in-app activity log.

## Permissions and behavior

The app never asks for device-owner, device-admin, accessibility, root, or broad VPN access. Android displays its standard VPN consent dialog and its system notification while capture is active. The VPN uses `addAllowedApplication`, so every package other than the selected app keeps its normal direct network path.

When the VPN file descriptor closes—because the desktop is unreachable, Android revokes consent, the app crashes, or the user taps Stop—Android automatically restores the selected app's normal network path.

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
- `vpn_engine/`: Go/JNI binding for the MIT-licensed `xjasonlyu/tun2socks` engine. `build-android.sh` produces arm64 and x86_64 relay libraries as part of the Android Gradle pre-build task.
