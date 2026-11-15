# Security Policy

## Supported Versions

Security fixes and advisories apply to the active Rust workspace baseline on `main`:

| Component / Workspace | Version / Branch | Supported |
| --- | --- | --- |
| `fayger` Rust Workspace (`release/dev/core`) & Specs (`docs/`) | `main` / `0.3.x` (`0.3.0-rc.1`) | Yes |
| Earlier milestone snapshots | `< 0.3.0` (`0.1.x`, `0.2.x`) | No |

## Security Scope

`FayGer` enforces the runtime isolation boundary between portable `Fay` artifacts and terminal Host environments. High-priority security areas include:

1. **`BuF` (Binary Universal Format) Parsing & Signature Verification (`fayger-buf`, `fayger-loader`)**:
   - Out-of-bounds reads, integer overflows, malformed CBOR canonicalization, section-index spoofing, memory exhaustion during `BuF` parsing, or signature/digest verification bypasses in the load chain.
2. **Capability Intersection & CAP Double-Gate (`fayger-runtime`, `fayger-cap`)**:
   - Any execution path bypassing the strict `Manifest ∩ Host ∩ Mandate` capability intersection, `Faying` Mandate expiration/revocation checks, `G0` Human Prime takeover preemption, or CAP `1.0` `D-1..D-7` policy evaluation before invoking Host drivers.
3. **Sandboxed `real_exec` Syscalls & Resource Quota Isolation**:
   - Path traversal, symlink escape, unauthorized subprocess/network access, quota enforcement bypass, or unbounded Host log growth (`BoundedHostLogSink` in `fayger-observability`) during controlled `real_exec` operations.
4. **Signed Component Registry Provenance (`fayger-faymanifest`)**:
   - Ed25519 trust-anchor verification bypasses, RFC 8785 JCS canonicalization mismatches, namespace confusion (`core` vs `organization` vs `ungoverned`), or unsafe downgrade attacks in `StaticIndexRegistry` and offline cache resolution.

## Reporting a Vulnerability

**Please do NOT open public GitHub Issues or Pull Requests for suspected sandbox escapes, parser memory bugs, or authorization bypasses.**

- **Primary Channel (Recommended)**: Submit a report privately via **GitHub Private Vulnerability Reporting (Security Advisories)**:
  - https://github.com/ChainMode/FayGer/security/advisories/new
- **Fallback Email Channel**: If GitHub Security Advisories is unavailable to you, email **[jah.g@ifay.ai](mailto:jah.g@ifay.ai)** with `[SECURITY][FayGer]` in the subject line.

Please attach minimal `BuF` or `FayManifest` reproduction inputs, crate names, target OS/architecture details, and observed behavior.

## Coordinated Disclosure Policy

- **Acknowledgment**: Within **72 hours** of receiving your report.
- **Triage & Assessment**: Initial reproduction, severity rating, and mitigation plan within **7 calendar days**.
- **Coordinated Disclosure Window**: **90 days** from receipt to public advisory and regression-test release.
- **Credit**: Researchers are recognized in the security advisory unless anonymity is requested.
