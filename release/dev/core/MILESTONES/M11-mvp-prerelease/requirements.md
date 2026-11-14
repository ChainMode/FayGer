# M11: MVP Prerelease Candidate Requirements

## Goal

Produce a local, reviewable Fayger MVP prerelease candidate without claiming
production CAP/Faying authorization or real hardware control.

## Requirements

1. The candidate MUST identify itself as `0.1.0-rc.1` in the CLI and audit output.
2. `fayger demo drone` MUST remain a one-command, deterministic telemetry demo.
3. Every audit record MUST identify the Fayger version and release channel.
4. Routine telemetry MAY run only in simulation. Unknown and critical operations
   MUST be denied; high-risk operations MUST be held; non-allow decisions MUST
   remain `not_executed`.
5. Every authorization record MUST retain `source=demo_fixture` and
   `production_verified=false`.
6. A local release command MUST run the quality gates, build the release binary,
   execute positive and negative smoke checks, and produce a checksummed review
   bundle.
7. The bundle MUST contain the binary, demo fixtures, license, release notes,
   acceptance guide, and generated verification evidence.
8. No tag, push, merge, or `main` branch change is part of M11.

## Out Of Scope

- Real drone or other hardware control.
- Production Faying mandate verification.
- Freezing CAP D-1/D-2/D-5/D-6.
- Cross-platform binaries not built on the current host.

