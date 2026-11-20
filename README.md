# FayGer

[![Docs CI](https://github.com/ChainMode/FayGer/actions/workflows/docs-ci.yml/badge.svg)](https://github.com/ChainMode/FayGer/actions/workflows/docs-ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)
[![Docs: CC BY 4.0](https://img.shields.io/badge/Docs-CC%20BY%204.0-lightgrey.svg)](./LICENSE-DOCS)
[![Website: ifay.ai](https://img.shields.io/badge/Website-ifay.ai-blue)](https://ifay.ai)

**FayGer** is the portable three-in-one virtual runtime engine (`faygerd` daemon + `fayger` CLI + embedded **FayGer Dashboard** Web Console) for **Fay** autonomous avatars (`iFay` personal digital souls anchored to a **Human Prime** and `coFay` public-role collaborative seats) across desktop, server, mobile, embedded **Host** devices, and in-app containers. It enforces a two-layer capability authorization chain (`BuF ∩ HostPool ∩ PerFayPolicy`), explicit **Faying** lifecycle supervision with a red `G0` emergency brake (**Rogue Fay** isolation), a built-in **FayID** & **Auth Exchange** vault, **CAP** host driver gatekeeping, **ICP** (`.ctx`) adaptive UI cards, and local **TP** (**Telepathy**) shared cognitive workspaces.

## 30-Second Quickstart

Launch the local `faygerd` daemon and embedded **FayGer Dashboard** (zero external dependencies, works 100% offline):

```bash
cd release/dev/core
cargo run -p fayger-cli -- up
```

Then open `http://127.0.0.1:7979` in your browser, or drive `FayGer` from the terminal:

```bash
# Inspect canonical Device Runtime Environment Profile (fayger-device-profile/1.0)
cargo run -p fayger-cli -- profile --json

# Initialize the local FayID & Auth Exchange vault and create a runnable iFay .buf soul
cargo run -p fayger-cli -- wallet init --name "My Vault"
cargo run -p fayger-cli -- soul create --kind ifay --name alice --out alice.buf
```

## Documentation & Reference Runtime

- Getting Started: [English (`docs/en/start.md`)](./docs/en/start.md) · [简体中文 (`docs/zh-CN/start.md`)](./docs/zh-CN/start.md)
- Reference Core Workspace (`Rust`): [release/dev/README.md](./release/dev/README.md) · [release/dev/core/README.md](./release/dev/core/README.md)

## Governance & Security

- Contribution guidelines & DCO 1.1 sign-off: [CONTRIBUTING.md](./CONTRIBUTING.md)
- Private vulnerability reporting: [SECURITY.md](./SECURITY.md)
- Community standards: [CODE_OF_CONDUCT.md](./CODE_OF_CONDUCT.md)

## License

This repository uses a dual-track open-source license (`Copyright (c) 2025-2026 ChainMode contributors`):

- **Source Code, Schemas & Conformance Suites** (`release/`): Licensed under the **MIT License** — see [LICENSE](./LICENSE).
- **Specifications & Architectural Blueprints** (`docs/`): Licensed under **Creative Commons Attribution 4.0 International (CC BY 4.0)** — see [LICENSE-DOCS](./LICENSE-DOCS).
