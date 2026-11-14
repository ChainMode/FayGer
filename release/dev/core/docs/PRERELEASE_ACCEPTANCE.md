# Fayger 0.1.0-rc.1 Acceptance Guide

This guide verifies the packaged local MVP without requiring a Rust toolchain.
Run every command from the extracted bundle directory.

## 1. Verify Bundle Contents

```bash
shasum -a 256 -c MANIFEST.sha256
```

Every listed file must report `OK`.

## 2. Run The Automated Acceptance Check

```bash
./verify-bundle.sh
```

Expected final line:

```text
PASS: Fayger 0.1.0-rc.1 prerelease bundle verified
```

## 3. Review The Product Path

```bash
./bin/fayger version
./bin/fayger demo drone
```

The version must be `0.1.0-rc.1`. The demo must report:

- `decision: ALLOW_SIMULATION`
- `operation_status: simulated_success`
- telemetry status `nominal`, GPS lock `true`, battery `87`
- `source: demo_fixture`
- `production_verified: false`
- `release_channel: prerelease`

## 4. Review Safety Boundaries

Unknown operation:

```bash
./bin/fayger evaluate --manifest examples/drone-controller.faymanifest.json \
  --driver examples/drone.driver.json --resource flight_controller \
  --operation not_declared
```

Expected: `critical`, `DENY`, `unknown_operation`, `not_executed`.

High-risk operation:

```bash
./bin/fayger evaluate --manifest examples/drone-controller.faymanifest.json \
  --driver examples/drone.driver.json --resource flight_controller \
  --operation upload_firmware
```

Expected: `high`, `HOLD_FOR_POLICY`, `confirmation_policy_pending`,
`not_executed`.

Critical operation:

```bash
./bin/fayger evaluate --manifest examples/drone-controller.faymanifest.json \
  --driver examples/drone.driver.json --resource flight_controller \
  --operation arm_motors
```

Expected: `critical`, `DENY`, `critical_operation_blocked_in_demo`,
`not_executed`.

## 5. Inspect Evidence And Record A Decision

Review `EVIDENCE.md` and the four files under `evidence/`. Accept only if the
scope and limitations in `RELEASE_NOTES.md` match the intended review target.

Suggested decision wording:

```text
Accepted as the Fayger 0.1.0-rc.1 local Phase 1 simulation baseline.
Production Faying authorization and real hardware control remain unapproved.
```

