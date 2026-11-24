# FayGer

[![Docs CI](https://github.com/ChainMode/FayGer/actions/workflows/docs-ci.yml/badge.svg)](https://github.com/ChainMode/FayGer/actions/workflows/docs-ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)
[![Docs: CC BY 4.0](https://img.shields.io/badge/Docs-CC%20BY%204.0-lightgrey.svg)](./LICENSE-DOCS)
[![Website: ifay.ai](https://img.shields.io/badge/Website-ifay.ai-blue)](https://ifay.ai)

**FayGer** (`v0.3.1`) is the cross-platform three-in-one virtual runtime engine (`faygerd` daemon + `fayger` CLI + embedded **FayGer Dashboard** & `/mobile` **Pocket Guardian** Web Console) for **Fay** autonomous avatars (`iFay` personal digital souls anchored to a **Human Prime** and `coFay` public-role collaborative seats) across macOS, Linux, Windows, mobile companions, server, and physical embodied **Host** devices. It enforces a two-layer capability authorization chain (`BuF ∩ HostPool ∩ PerFayPolicy`), explicit **Faying** lifecycle supervision with a red `G0` emergency brake (**Rogue Fay** isolation), a built-in **FayID** & **Auth Exchange** vault, per-`iFay` **Ego** + External LLM brain orchestration, native OS permission probing & hardware/software **CAP** gatekeeping, **ICP** (`.ctx`) adaptive UI cards, **Telepathy (TP)** shared cognitive spaces, and native **GMChain** + **MeriToken** (`μ`) standing & priority consumption.

## Desktop Download & One-Line Install (`v0.3.1`)

**macOS Graphical App (`FayGer.app`):**
- Download **`FayGer-macos-arm64.dmg`** from [GitHub Releases](https://github.com/ChainMode/FayGer/releases), drag `FayGer.app` to `/Applications`, and launch (bundler script: [`scripts/package-macos-dmg.sh`](./scripts/package-macos-dmg.sh)).

**macOS (Apple Silicon & Intel) / Linux (x86_64 & ARM64) One-Line CLI:**

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

Installer scripts: [`scripts/install.sh`](./scripts/install.sh) · [`scripts/install.ps1`](./scripts/install.ps1) (downloads prebuilt `v0.3.1` release binary + verifies `.sha256` checksum).

## Mobile Companion (`Pocket Guardian` — Android `.apk`, `/mobile` PWA & iOS TestFlight)

Supervise mounted souls, toggle explicit **Faying** mandates, approve **CAP** requests, and press the **Red `G0` Emergency Brake** from your phone:

- **Instant PWA (iOS & Android)**: Open `http://<LAN-IP>:7979/mobile` on the same Wi-Fi and tap **Add to Home Screen**.
- **Android Native App**: Download **`fayger-mobile-guardian-android.apk`** from [GitHub Releases](https://github.com/ChainMode/FayGer/releases) or build from [`release/mobile/android/`](./release/mobile/README.md).
- **iOS TestFlight Pipeline (`10,000` External Testers)**: See [`release/mobile/ios-testflight/README.md`](./release/mobile/ios-testflight/README.md).

## Hardware Vendor Embodiment SDK (Drones, RC Cars, Robots, CNCs & Camera Jibs)

Connect physical machines safely as **CAP**-gated **Host** devices via the 3-Tier Hardware SDK ([`release/hardware-sdk/README.md`](./release/hardware-sdk/README.md)):

1. **Tier A — Embedded Linux Direct Binary**: Run `fayger-aarch64-unknown-linux-gnu` / `fayger-x86_64-unknown-linux-gnu` under `systemd` on Raspberry Pi, Jetson, or RK3588 companion boards.
2. **Tier B — Zero-Dependency C99 Firmware Bridge (`FHBP/1.0`)**: Drop [`release/hardware-sdk/c99/fayger_hw_bridge.h`](./release/hardware-sdk/c99/fayger_hw_bridge.h) (reference node: [`example_firmware_node.c`](./release/hardware-sdk/c99/example_firmware_node.c)) into MCU/RTOS/PLC firmware for **RC Cars, CNC Mills, Camera Jibs, Drones, and Robots** with firmware-enforced physical envelopes, dead-man watchdogs, and instant `G0` safe halts.
3. **Tier C — Custom OS Rust `HostAdapter` HAL**: Implement the 5-function `HostAdapter` trait in [`release/dev/core/`](./release/dev/core/README.md) and verify with `fayger conformance`.

## Build & Run from Source (30-Second Quickstart)

Launch the local `faygerd` daemon and automatically open the embedded **FayGer Dashboard** at `http://127.0.0.1:7979` (zero external runtime dependencies, works 100% offline):

```bash
cd release/dev/core
cargo run -p fayger-cli -- up --open
```

Or drive `FayGer` directly from the terminal:

```bash
# Inspect canonical Device Runtime Environment Profile (fayger-device-profile/1.0)
fayger profile --json

# Initialize the local FayID & Auth Exchange vault and create a runnable iFay .buf soul
fayger wallet init --name "My Vault"
fayger soul create --kind ifay --name alice --out alice.buf
```

## Documentation & Reference Runtime

- Getting Started: [English (`docs/en/start.md`)](./docs/en/start.md) · [简体中文 (`docs/zh-CN/start.md`)](./docs/zh-CN/start.md)
- Reference Core Workspace (`Rust`): [release/dev/README.md](./release/dev/README.md) · [release/dev/core/README.md](./release/dev/core/README.md)
- Mobile Companion Kit: [release/mobile/README.md](./release/mobile/README.md)
- Hardware Vendor SDK (`FHBP/1.0` C99): [release/hardware-sdk/README.md](./release/hardware-sdk/README.md)

## Governance & Security

- Contribution guidelines & DCO 1.1 sign-off: [CONTRIBUTING.md](./CONTRIBUTING.md)
- Private vulnerability reporting: [SECURITY.md](./SECURITY.md)
- Community standards: [CODE_OF_CONDUCT.md](./CODE_OF_CONDUCT.md)

## License

This repository uses a dual-track open-source license (`Copyright (c) 2025-2026 ChainMode contributors`):

- **Source Code, Schemas & Conformance Suites** (`release/`, `scripts/`): Licensed under the **MIT License** — see [LICENSE](./LICENSE).
- **Specifications & Architectural Blueprints** (`docs/`): Licensed under **Creative Commons Attribution 4.0 International (CC BY 4.0)** — see [LICENSE-DOCS](./LICENSE-DOCS).
