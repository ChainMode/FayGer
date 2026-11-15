# Getting Started with Fayger

**Fayger** is the multi-platform Virtual Runtime Environment for Fay (`iFay` and `coFay`). It ingests portable **BuF (Built-up Fay)** artifacts and declarative **FayManifest** assembly descriptors, providing unified verification, lifecycle execution, capability-scoped host adaptation, and tamper-evident audit chains across desktop, server, browser, and embedded hosts.

> Terminology Note: **Host** strictly denotes the terminal device or application process inhabited by a Fay; the natural person is always referred to as the **Human Prime**.

## Three-Layer Architecture

1. **Loader Layer**: Parses five-segment `BuF` artifacts and verifies cryptographic digests, Ed25519 signatures, and `Runtime_Interface` version compatibility.
2. **Runtime Layer**: Enforces deterministic `Runtime_Interface` lifecycle transitions (`Loaded -> Initialized -> Running -> Suspended -> Terminated/Failed`) and resource quotas, verified across implementations via the black-box `fayger-conformance` suite.
3. **Adapter Layer**: Translates platform-neutral `Universal_Instruction` streams into capability-trimmed host operations behind production CAP mandate gates and two-tier `Ego_Package` verification.

## Quick CLI Commands

```bash
fayger version
fayger info
fayger conformance
fayger assemble app.faymanifest.json --registry-index registry-index.json --trust-anchors trust-anchors.json
fayger demo live-ifay
```

See the `blueprint/` directory for the complete architectural chapters.
