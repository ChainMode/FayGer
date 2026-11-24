# Getting Started with Fayger

**Fayger** (`v0.3.1`) is the cross-platform three-in-one Virtual Runtime Environment (`faygerd` daemon + `fayger` CLI + embedded **FayGer Dashboard** & `/mobile` **Pocket Guardian** Web Console) for Fay (`iFay` and `coFay`). It ingests portable **BuF (Built-up Fay)** artifacts and declarative **FayManifest** assembly descriptors, providing unified cryptographic verification, **Ego** + External LLM brain orchestration, native OS permission probing, **CAP**-gated hardware/OS/desktop application control, **GMChain** + **MeriToken** (`μ`) standing & priority consumption, and tamper-evident SHA-256 audit chains across macOS, Linux, Windows, mobile companions, server, and embodied hardware **Host** devices.

> Terminology Note: **Host** strictly denotes the terminal device or application process inhabited by a Fay; the natural person is always referred to as the **Human Prime**.

## Desktop Install & Graphical App (`v0.3.1` — macOS `.dmg` / Linux / Windows)

**macOS Graphical App (`FayGer.app` on Apple Silicon):**
- Download **`FayGer-macos-arm64.dmg`** from GitHub Releases, drag `FayGer.app` into `/Applications`, and open it directly (packaged via [`scripts/package-macos-dmg.sh`](../../scripts/package-macos-dmg.sh)).

**macOS (Apple Silicon & Intel) and Linux (x86_64 & ARM64) One-Line CLI:**

```bash
curl -fsSL https://raw.githubusercontent.com/ChainMode/FayGer/main/scripts/install.sh | sh
export PATH="$HOME/.fayger/bin:$PATH"
fayger up --open
```

**Windows (PowerShell x86_64):**

```powershell
irm https://raw.githubusercontent.com/ChainMode/FayGer/main/scripts/install.ps1 | iex
fayger up --open
```

Running `fayger up --open` launches `faygerd` (`127.0.0.1:7979`) and opens the **FayGer Dashboard** so the **Human Prime** can probe & request native OS permissions, mount `iFay` / `coFay` souls, bind an External LLM + **Ego** persona/redlines, flip the explicit **Faying** supervision switch, trigger the red **`G0` Emergency Brake**, approve high-risk hardware/OS/app actions, and manage **MeriToken** (`μ`) standing.

## Mobile Companion (`Pocket Guardian` — `/mobile` PWA, Android `.apk` & iOS TestFlight)

Supervise mounted avatars, approve remote **CAP** prompts, inspect **FayID** vault standing, and trigger the **Red `G0` Emergency Brake** from your phone (details in [`release/mobile/README.md`](../../release/mobile/README.md)):

- **Zero-Install PWA (iOS & Android)**: Visit `http://<HOST-LAN-IP>:7979/mobile` over local Wi-Fi and choose **Add to Home Screen**.
- **Android Native App**: Install **`fayger-mobile-guardian-android.apk`** from GitHub Releases or build from `release/mobile/android/`.
- **iOS TestFlight Kit**: Archive and upload to App Store Connect TestFlight (up to `10,000` external testers) via [`release/mobile/ios-testflight/README.md`](../../release/mobile/ios-testflight/README.md).

## Hardware Vendor Embodiment SDK (Drones, RC Cars, Robots, CNCs & Camera Jibs)

Hardware vendors can onboard physical machines as **CAP**-governed **Host** devices through the 3-Tier SDK in [`release/hardware-sdk/README.md`](../../release/hardware-sdk/README.md):

1. **Tier A — Embedded Linux Direct Binary**: Run prebuilt `aarch64` / `x86_64` Linux binaries with `systemd` on Raspberry Pi, Jetson, or RK3588 companion computers.
2. **Tier B — Single-Header C99 Firmware Bridge (`FHBP/1.0`)**: Embed [`fayger_hw_bridge.h`](../../release/hardware-sdk/c99/fayger_hw_bridge.h) into MCU/RTOS/PLC controllers for **RC Cars, CNC Mills, Camera Jibs, Drones, and Robots**, enforcing firmware-side physical safety envelopes, dead-man watchdogs, and instant `G0` safe halts.
3. **Tier C — Custom OS Rust `HostAdapter` HAL**: Implement the 5-method `HostAdapter` trait and validate behavioral equivalence with `fayger conformance`.

## Three-Layer Architecture

1. **Loader Layer**: Parses five-segment `BuF` artifacts and verifies cryptographic digests, Ed25519 signatures, and `Runtime_Interface` version compatibility.
2. **Runtime Layer**: Enforces deterministic `Runtime_Interface` lifecycle transitions (`Loaded -> Initialized -> Running -> Suspended -> Terminated/Failed`) and resource quotas, verified across implementations via the black-box `fayger-conformance` suite.
3. **Adapter Layer**: Translates platform-neutral `Universal_Instruction` streams and controlled host requests into capability-trimmed **Host** operations behind explicit **Faying** mandates, `BuF ∩ HostPool ∩ PerFayPolicy` intersection checks, and **CAP** gatekeeping.

## Quick CLI Commands

```bash
fayger up --open
fayger version
fayger info
fayger profile --json
fayger conformance
fayger assemble app.faymanifest.json --registry-index registry-index.json --trust-anchors trust-anchors.json
fayger demo live-ifay
```

See the `blueprint/` directory for the complete architectural chapters.
