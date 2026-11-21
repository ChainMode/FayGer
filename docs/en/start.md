# Getting Started with Fayger

**Fayger** (`v0.3.0`) is the cross-platform three-in-one Virtual Runtime Environment (`faygerd` daemon + `fayger` CLI + embedded **FayGer Dashboard** Web Console) for Fay (`iFay` and `coFay`). It ingests portable **BuF (Built-up Fay)** artifacts and declarative **FayManifest** assembly descriptors, providing unified cryptographic verification, **Ego** + External LLM brain orchestration, native OS permission probing, **CAP**-gated hardware/OS/desktop application control, **GMChain** + **MeriToken** (`μ`) standing & priority consumption, and tamper-evident SHA-256 audit chains across macOS, Linux, Windows, server, and embedded hosts.

> Terminology Note: **Host** strictly denotes the terminal device or application process inhabited by a Fay; the natural person is always referred to as the **Human Prime**.

## One-Line Download & Install (macOS / Linux / Windows)

**macOS (Apple Silicon & Intel) and Linux (x86_64 & ARM64):**

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

Running `fayger up --open` launches `faygerd` (`127.0.0.1:7979`) and opens the **FayGer Dashboard** so the **Human Prime** can probe & request native OS permissions, mount `iFay` / `coFay` souls, bind an External LLM + **Ego** persona/redlines, flip the explicit **Faying** supervision switch, approve high-risk hardware/OS/app actions, and manage **MeriToken** (`μ`) standing.

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
