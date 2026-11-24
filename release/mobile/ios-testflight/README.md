# FayGer Pocket Guardian — iOS TestFlight Distribution Guide

This directory packages the **FayGer Pocket Guardian** companion client (`ai.ifay.fayger`) for **Apple TestFlight** external beta distribution (up to `10,000` external testers via public invitation link) while reusing the exact zero-bloat web assets in [`../android/app/src/main/assets/index.html`](../android/app/src/main/assets/index.html).

> **Terminology Note**: **Human Prime** (`人类原型`) denotes the natural person exercising `G0` supervisory authority; **Host** (`宿主`) denotes the desktop, server, or embedded device running `FayGer`.

---

## Option 0: Instant Zero-Install iOS PWA (Works Today without Apple Fee)

Before setting up an Apple Developer account, any **Human Prime** on iPhone or iPad can pair immediately over local Wi-Fi:

1. Start `FayGer` on your desktop or embedded **Host**: `fayger up --bind 0.0.0.0:7979`
2. Open `http://<LAN-IP>:7979/mobile` in **Safari** on iOS.
3. Tap **Share → Add to Home Screen** (`添加到主屏幕`) to pin **FayGer Pocket Guardian** as a full-screen standalone companion with instant access to the **Red `G0` Emergency Brake**, **Faying** switch, and **CAP** remote approvals.

---

## Option 1: Native iOS App & TestFlight Beta (`10,000` External Testers)

### Prerequisites

1. macOS with **Xcode 15+** and Xcode Command Line Tools (`xcode-select --install`).
2. Active **Apple Developer Program** membership (`USD 99/year`) at `https://developer.apple.com/programs/`.
3. Record your 10-character **Apple Team ID** (`APPLE_TEAM_ID`, e.g. `ABCDE12345`) from **Membership Details**.
4. In **App Store Connect → Apps**, register a new iOS App with Bundle ID **`ai.ifay.fayger`**.
5. *(Recommended for CI / CLI upload)* In **App Store Connect → Users and Access → Integrations → App Store Connect API**, generate an API Key (`.p8`), save it to `~/.appstoreconnect/private_keys/AuthKey_<KEY_ID>.p8`, and note `ASC_API_KEY_ID` and `ASC_API_ISSUER_ID`.

### Files in This Kit

- [`capacitor.config.json`](./capacitor.config.json) — Configures Bundle ID `ai.ifay.fayger` and points `webDir` to [`../android/app/src/main/assets`](../android/app/src/main/assets/index.html) with local LAN navigation enabled.
- [`ExportOptions.plist`](./ExportOptions.plist) — Configures `app-store-connect` automatic code signing and symbol upload.
- [`build-testflight.sh`](./build-testflight.sh) — Automated CLI script that initializes the Xcode project, injects `NSAllowsLocalNetworking` + `NSLocalNetworkUsageDescription` into `Info.plist`, archives `FayGerGuardian.xcarchive`, exports the signed `.ipa`, and uploads via `xcrun altool`.

### One-Command Archive & Upload to TestFlight

```bash
cd release/mobile/ios-testflight

APPLE_TEAM_ID="ABCDE12345" \
ASC_API_KEY_ID="YOUR_KEY_ID" \
ASC_API_ISSUER_ID="00000000-0000-0000-0000-000000000000" \
./build-testflight.sh
```

### Manual Upload Alternative (Xcode Organizer or `xcrun altool`)

If `destination = export` is used instead of direct upload, upload any exported `.ipa` manually:

```bash
xcrun altool --upload-app \
  --type ios \
  --file build/export/App.ipa \
  --apiKey "$ASC_API_KEY_ID" \
  --apiIssuer "$ASC_API_ISSUER_ID"
```

*(Note: `xcrun notarytool` is used for macOS `.dmg` notarization in [`../../../scripts/package-macos-dmg.sh`](../../../scripts/package-macos-dmg.sh), whereas iOS TestFlight `.ipa` builds are validated and distributed directly by App Store Connect via `xcodebuild -exportArchive` or `xcrun altool`.)*

### Publishing Your Public TestFlight Link (`10,000` Testers)

1. Open **App Store Connect → My Apps → FayGer Guardian → TestFlight**.
2. Wait ~5–10 minutes for Apple's automated binary processing to complete, then complete the **Export Compliance** prompt (standard HTTPS/OS networking only).
3. Under **External Testing**, create a group (e.g. `Public Guardians`), attach the processed build, and submit for brief Beta App Review.
4. Enable **Public Link** (`Enable Public Link`) to share a single `https://testflight.apple.com/join/...` URL supporting up to **10,000** external iPhone & iPad testers.
