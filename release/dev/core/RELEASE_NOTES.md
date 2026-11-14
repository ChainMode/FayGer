# Fayger 0.1.0-rc.1

Status: local prerelease candidate for Product Owner review.

## Included

- One-command drone telemetry demonstration: `fayger demo drone`.
- FayManifest parsing and dependency assembly.
- Conservative CAP driver operation evaluation.
- Human-readable output plus a machine-readable audit record.
- Deterministic routine telemetry simulation.
- Fail-closed handling for unknown, high-risk, and critical operations.

## Safety Boundary

This candidate does not control hardware and does not perform production
Faying authorization. Every MVP authorization record is labeled
`source=demo_fixture` and `production_verified=false`.

| Operation class | Candidate behavior |
| --- | --- |
| routine | simulation only |
| high | held for policy; not executed |
| critical | denied; not executed |
| unknown | treated as critical, denied; not executed |

## Known Limitations

- CAP D-1/D-2/D-5/D-6 remain pending and are named in every audit record.
- No real mandate lookup, grade matrix, confirmation ceremony, or revocation
  transport is wired into the CLI.
- The prerelease bundle contains a binary only for the host that built it.
- Registry resolution remains a local stub.

## Review Decision

Approval of this candidate means accepting it as a local Phase 1 simulation
baseline. It does not approve a production release or hardware control.

