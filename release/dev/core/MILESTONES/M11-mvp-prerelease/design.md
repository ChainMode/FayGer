# M11: MVP Prerelease Candidate Design

## Version And Scope

The first review candidate is `0.1.0-rc.1`. It is a local Phase 1 simulation
artifact. The version appears in both `fayger version` and every MVP command
audit record so evidence can be tied to the reviewed binary.

## Release Flow

`scripts/build-prerelease.sh` is the single release entry point:

1. run format, lint, test, and documentation gates;
2. build the optimized `fayger` binary;
3. run the routine, unknown, high, and critical demo scenarios;
4. assert the safety decisions and non-production markers;
5. assemble a host-specific bundle under `target/prerelease/`;
6. emit a SHA-256 checksum and a text evidence report.

The bundle is intentionally local and generated under ignored build output. It
does not imply publication or approval.

## Safety Invariants

| Scenario | Decision | Execution status |
| --- | --- | --- |
| routine telemetry | `ALLOW_SIMULATION` | `simulated_success` |
| unknown operation | `DENY` | `not_executed` |
| high operation | `HOLD_FOR_POLICY` | `not_executed` |
| critical operation | `DENY` | `not_executed` |

All four scenarios must say `production_verified: false`. The release script
fails closed if an expected marker is missing.

