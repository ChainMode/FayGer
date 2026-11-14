#!/usr/bin/env bash
# Verifies an extracted Fayger prerelease bundle without network access.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUNDLE_DIR="${FAYGER_BUNDLE_DIR:-${SCRIPT_DIR}}"
BIN="${BUNDLE_DIR}/bin/fayger"
MANIFEST="${BUNDLE_DIR}/examples/drone-controller.faymanifest.json"
DRIVER="${BUNDLE_DIR}/examples/drone.driver.json"
EXPECTED_VERSION="0.1.0-rc.1"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_contains() {
  local file="$1"
  local expected="$2"
  local label="$3"
  if ! grep -Fq -- "${expected}" "${file}"; then
    fail "${label}: missing ${expected}"
  fi
}

[[ -x "${BIN}" ]] || fail "binary is missing or not executable: ${BIN}"
[[ -f "${MANIFEST}" ]] || fail "FayManifest fixture is missing"
[[ -f "${DRIVER}" ]] || fail "driver fixture is missing"

if [[ -f "${BUNDLE_DIR}/MANIFEST.sha256" ]]; then
  (
    cd "${BUNDLE_DIR}"
    if command -v shasum >/dev/null 2>&1; then
      shasum -a 256 -c MANIFEST.sha256
    elif command -v sha256sum >/dev/null 2>&1; then
      sha256sum -c MANIFEST.sha256
    else
      fail "no SHA-256 utility is available"
    fi
  )
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

"${BIN}" version >"${TMP_DIR}/version.txt"
"${BIN}" demo drone >"${TMP_DIR}/routine.txt"
"${BIN}" evaluate --manifest "${MANIFEST}" --driver "${DRIVER}" \
  --resource flight_controller --operation not_declared >"${TMP_DIR}/unknown.txt"
"${BIN}" evaluate --manifest "${MANIFEST}" --driver "${DRIVER}" \
  --resource flight_controller --operation upload_firmware >"${TMP_DIR}/high.txt"
"${BIN}" evaluate --manifest "${MANIFEST}" --driver "${DRIVER}" \
  --resource flight_controller --operation arm_motors >"${TMP_DIR}/critical.txt"

assert_contains "${TMP_DIR}/version.txt" "fayger ${EXPECTED_VERSION}" "version"

for marker in "ALLOW_SIMULATION" "simulated_success" '"telemetry_status": "nominal"' \
  '"source": "demo_fixture"' '"production_verified": false' \
  '"fayger_version": "0.1.0-rc.1"' '"release_channel": "prerelease"'; do
  assert_contains "${TMP_DIR}/routine.txt" "${marker}" "routine path"
done

for marker in "critical" "DENY" "unknown_operation" "not_executed" \
  '"production_verified": false'; do
  assert_contains "${TMP_DIR}/unknown.txt" "${marker}" "unknown-operation path"
done

for marker in "high" "HOLD_FOR_POLICY" "confirmation_policy_pending" \
  "not_executed" '"production_verified": false'; do
  assert_contains "${TMP_DIR}/high.txt" "${marker}" "high-risk path"
done

for marker in "critical" "DENY" "critical_operation_blocked_in_demo" \
  "not_executed" '"production_verified": false'; do
  assert_contains "${TMP_DIR}/critical.txt" "${marker}" "critical path"
done

echo "PASS: Fayger ${EXPECTED_VERSION} prerelease bundle verified"

