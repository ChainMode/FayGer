# Contributing to FayGer

Thank you for contributing to **FayGer**! Please review our [Code of Conduct](./CODE_OF_CONDUCT.md) before participating, and report any sandbox, parser, or authorization vulnerabilities privately according to [SECURITY.md](./SECURITY.md).

## Local Setup & Verification

The reference `FayGer` runtime workspace is written in **Rust** (requires **Rust `>= 1.83`**, `2021` edition) under `release/dev/core`.

```bash
# Clone your fork
git clone https://github.com/ChainMode/FayGer.git
cd FayGer/release/dev/core

# Run all workspace unit, integration, and property-based tests
cargo test --workspace

# Run the runtime black-box conformance suite
cargo run -p fayger-cli -- conformance --json
```

All workspace crates (`fayger-error`, `fayger-buf`, `fayger-loader`, `fayger-runtime`, `fayger-adapter`, `fayger-observability`, `fayger-cli`, `fayger-bridge`, `fayger-faymanifest`, `fayger-cap`, and `fayger-conformance`) must compile cleanly and pass all tests before submitting a Pull Request.

## Contribution Workflow

1. **Fork & Branch**: Fork `https://github.com/ChainMode/FayGer.git` and create a feature or fix branch off `main`.
2. **Conventional Commits**: Format commit subjects using Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`, `refactor:`, `chore:`).
3. **Sign Off Your Commits (DCO 1.1)**: Commit with `git commit -s` so each commit carries a `Signed-off-by` trailer.
4. **Pull Request**: Open a Pull Request against `main` summarizing the runtime change, safety invariants preserved, and `cargo test --workspace` verification results.

## Developer Certificate of Origin v1.1 (DCO 1.1) & Licensing

We use the **Developer Certificate of Origin Version 1.1 (DCO 1.1)** (`https://developercertificate.org/`). **No CLA (Contributor License Agreement) is required.**

Add your sign-off to every commit via `git commit -s`:

```text
Signed-off-by: Jane Doe <jane.doe@example.com>
```

Contributions follow an **Inbound = Outbound** dual-track license model (`Copyright (c) 2025-2026 ChainMode contributors`):

- **Source Code, Adapters & Conformance Suites** (`release/`): Licensed under the **MIT License** ([LICENSE](./LICENSE)).
- **Specifications & Architectural Blueprints** (`docs/`): Licensed under **Creative Commons Attribution 4.0 International (CC BY 4.0)** ([LICENSE-DOCS](./LICENSE-DOCS)).

## Ecosystem Terminology Alignment

Please use canonical ecosystem terminology consistently across Rust docstrings, CLI messages, and `docs/`:

- **`Human Prime` (`人类原型`)**: Use `Human Prime` (`人类原型`) for the living human individual issuing mandates or `G0` takeovers.
- **`Host` (`宿主`)**: Use `Host` (`宿主`) strictly for terminal hardware devices, operating systems, browsers, or embedded application runtime environments where `FayGer` executes.
- **`Fay = iFay + coFay`**: Use `Fay` as the umbrella term, `iFay` for an individual avatar, and `coFay` for a public or organizational role avatar.
- **`GMChain` (`公誉链`) & `MeriToken (μ)` (`美誉值`)**: Use `GMChain` (`公誉链`) and non-monetary `MeriToken` (`μ` / `Merit Unit`) when referencing ecosystem contribution accounting.
