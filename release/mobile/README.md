# FayGer Mobile Pocket Guardian (`release/mobile/`) / 移动端随身监护套件

**FayGer Pocket Guardian** empowers the **Human Prime** (`人类原型`) to supervise mounted `iFay` / `coFay` souls, toggle explicit **Faying** mandates, approve or reject high-risk **CAP** capability requests, inspect **FayID** & **GMChain** (`公誉链`) / **MeriToken** (`美誉值`, `μ`) standing, monitor `FHBP/1.0` embodied hardware **Host** (`宿主`) telemetry, and press the **Red `G0` Emergency Brake** from any smartphone on the local network.

---

## Delivery Matrix (Android & iOS) / 双端获取与构建矩阵

| Platform / 平台 | Zero-Install Today / 即开即用 | Native App Package / 原生安装包与工程 |
| :--- | :--- | :--- |
| **Android (8.0+ / API 26+)** | Open `http://<HOST-LAN-IP>:7979/mobile` in Chrome → **Add to Home Screen** | Direct downloadable **`fayger-mobile-guardian-android.apk`** on GitHub Releases, built from [`./android/`](./android/app/build.gradle) |
| **iOS (iPhone & iPad)** | Open `http://<HOST-LAN-IP>:7979/mobile` in Safari → **Share → Add to Home Screen** | **TestFlight** external beta (`10,000` testers) via [`./ios-testflight/`](./ios-testflight/README.md) |

---

## 1. Android Native Companion (`fayger-mobile-guardian-android.apk`)

### Download Prebuilt APK

Download **`fayger-mobile-guardian-android.apk`** (and verify `fayger-mobile-guardian-android.apk.sha256`) directly from the [GitHub Releases](https://github.com/ChainMode/FayGer/releases) page and install on any Android 8.0+ (`minSdk 26`) phone or tablet:

```bash
adb install -r fayger-mobile-guardian-android.apk
```

### Build from Source (`release/mobile/android/`)

The Android project in [`./android/`](./android/settings.gradle) uses pure Android SDK classes (`android.app.Activity` + `android.webkit.WebView`, `compileSdk 34`, **zero AndroidX or Node.js dependencies**) so CI and local builds finish in ~15 seconds:

- Manifest (`INTERNET` + `usesCleartextTraffic="true"` for LAN `FayGer` pairing): [`android/app/src/main/AndroidManifest.xml`](./android/app/src/main/AndroidManifest.xml)
- Native WebView + `SharedPreferences` pairing shell: [`android/app/src/main/java/ai/ifay/fayger/MainActivity.java`](./android/app/src/main/java/ai/ifay/fayger/MainActivity.java)
- Bundled offline preview + LAN pairing console: [`android/app/src/main/assets/index.html`](./android/app/src/main/assets/index.html)

```bash
cd release/mobile/android
gradle assembleDebug
# Output: app/build/outputs/apk/debug/app-debug.apk
```

---

## 2. iOS PWA & TestFlight Pipeline (`release/mobile/ios-testflight/`)

- **Today (Zero Cost)**: Visit `http://<HOST-LAN-IP>:7979/mobile` in mobile Safari and tap **Share → Add to Home Screen** for full-screen `/mobile` Pocket Guardian supervision.
- **App Store Connect & TestFlight (`10,000` External Beta Testers)**: Follow the step-by-step guide in [`ios-testflight/README.md`](./ios-testflight/README.md) and run [`ios-testflight/build-testflight.sh`](./ios-testflight/build-testflight.sh) with your Apple Developer Team ID.
