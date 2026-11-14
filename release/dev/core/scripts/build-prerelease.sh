#!/usr/bin/env bash
# Runs release gates and builds a checksummed local prerelease review bundle.
set -euo pipefail

# Desktop and CI shells may not source the user's interactive profile.
export PATH="${HOME}/.cargo/bin:${PATH}"

command -v cargo >/dev/null 2>&1 || {
  echo "FAIL: cargo was not found; install Rust 1.83 or newer" >&2
  exit 1
}
command -v rustc >/dev/null 2>&1 || {
  echo "FAIL: rustc was not found; install Rust 1.83 or newer" >&2
  exit 1
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPO_DIR="$(cd "${WORKSPACE_DIR}/../../.." && pwd)"
VERSION="0.1.0-rc.1"
TARGET_TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
OUT_ROOT="${WORKSPACE_DIR}/target/prerelease"
BUNDLE_NAME="fayger-${VERSION}-${TARGET_TRIPLE}"
BUNDLE_DIR="${OUT_ROOT}/${BUNDLE_NAME}"
ARCHIVE="${OUT_ROOT}/${BUNDLE_NAME}.tar.gz"
QUALITY_LOG="${OUT_ROOT}/quality-gates.log"

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

run_gate() {
  local label="$1"
  shift
  printf '\n==> %s\n' "${label}" | tee -a "${QUALITY_LOG}"
  "$@" 2>&1 | tee -a "${QUALITY_LOG}"
}

cd "${WORKSPACE_DIR}"
rm -rf "${BUNDLE_DIR}" "${ARCHIVE}" "${ARCHIVE}.sha256"
mkdir -p "${OUT_ROOT}"
: >"${QUALITY_LOG}"

run_gate "format" cargo fmt --all -- --check
run_gate "clippy" cargo clippy --workspace --all-targets --all-features -- -D warnings
run_gate "tests" cargo test --workspace --all-features
run_gate "documentation" env RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --all-features --no-deps
run_gate "release build" cargo build --release -p fayger-cli --bin fayger

BIN="${WORKSPACE_DIR}/target/release/fayger"
[[ -x "${BIN}" ]] || fail "release binary not found: ${BIN}"

mkdir -p "${BUNDLE_DIR}/bin" "${BUNDLE_DIR}/examples" "${BUNDLE_DIR}/evidence"
cp "${BIN}" "${BUNDLE_DIR}/bin/fayger"
cp "${WORKSPACE_DIR}/crates/fayger-faymanifest/examples/drone-controller.faymanifest.json" \
  "${BUNDLE_DIR}/examples/"
cp "${WORKSPACE_DIR}/crates/fayger-cap/examples/drone.driver.json" \
  "${BUNDLE_DIR}/examples/"
cp "${REPO_DIR}/LICENSE" "${BUNDLE_DIR}/LICENSE"
cp "${WORKSPACE_DIR}/RELEASE_NOTES.md" "${BUNDLE_DIR}/RELEASE_NOTES.md"
cp "${WORKSPACE_DIR}/docs/PRERELEASE_ACCEPTANCE.md" "${BUNDLE_DIR}/ACCEPTANCE.md"
cp "${WORKSPACE_DIR}/scripts/verify-bundle.sh" "${BUNDLE_DIR}/verify-bundle.sh"
chmod +x "${BUNDLE_DIR}/bin/fayger" "${BUNDLE_DIR}/verify-bundle.sh"

"${BIN}" version >"${BUNDLE_DIR}/evidence/version.txt"
"${BIN}" demo drone >"${BUNDLE_DIR}/evidence/routine.txt"
"${BIN}" evaluate \
  --manifest "${BUNDLE_DIR}/examples/drone-controller.faymanifest.json" \
  --driver "${BUNDLE_DIR}/examples/drone.driver.json" \
  --resource flight_controller --operation not_declared \
  >"${BUNDLE_DIR}/evidence/unknown.txt"
"${BIN}" evaluate \
  --manifest "${BUNDLE_DIR}/examples/drone-controller.faymanifest.json" \
  --driver "${BUNDLE_DIR}/examples/drone.driver.json" \
  --resource flight_controller --operation upload_firmware \
  >"${BUNDLE_DIR}/evidence/high.txt"
"${BIN}" evaluate \
  --manifest "${BUNDLE_DIR}/examples/drone-controller.faymanifest.json" \
  --driver "${BUNDLE_DIR}/examples/drone.driver.json" \
  --resource flight_controller --operation arm_motors \
  >"${BUNDLE_DIR}/evidence/critical.txt"

assert_contains "${BUNDLE_DIR}/evidence/version.txt" "fayger ${VERSION}" "version"
assert_contains "${BUNDLE_DIR}/evidence/routine.txt" "ALLOW_SIMULATION" "routine decision"
assert_contains "${BUNDLE_DIR}/evidence/routine.txt" "simulated_success" "routine status"
assert_contains "${BUNDLE_DIR}/evidence/routine.txt" '"production_verified": false' "routine boundary"
assert_contains "${BUNDLE_DIR}/evidence/unknown.txt" "unknown_operation" "unknown reason"
assert_contains "${BUNDLE_DIR}/evidence/unknown.txt" "DENY" "unknown decision"
assert_contains "${BUNDLE_DIR}/evidence/unknown.txt" "not_executed" "unknown status"
assert_contains "${BUNDLE_DIR}/evidence/high.txt" "HOLD_FOR_POLICY" "high decision"
assert_contains "${BUNDLE_DIR}/evidence/high.txt" "not_executed" "high status"
assert_contains "${BUNDLE_DIR}/evidence/critical.txt" "DENY" "critical decision"
assert_contains "${BUNDLE_DIR}/evidence/critical.txt" "not_executed" "critical status"

cp "${QUALITY_LOG}" "${BUNDLE_DIR}/evidence/quality-gates.log"

SOURCE_COMMIT="$(git -C "${REPO_DIR}" rev-parse HEAD)"
SOURCE_STATE="clean"
if [[ -n "$(git -C "${REPO_DIR}" status --porcelain)" ]]; then
  SOURCE_STATE="dirty (local candidate; not committed or published)"
fi
VERIFIED_AT="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
cat >"${BUNDLE_DIR}/EVIDENCE.md" <<EOF
# Verification Evidence

- Version: ${VERSION}
- Host target: ${TARGET_TRIPLE}
- Verified at: ${VERIFIED_AT}
- Source commit: ${SOURCE_COMMIT}
- Source worktree: ${SOURCE_STATE}
- Format: PASS
- Clippy: PASS
- Workspace tests: PASS
- Rustdoc: PASS
- Release build: PASS
- Routine simulation: PASS
- Unknown operation denial: PASS
- High-risk hold: PASS
- Critical operation denial: PASS

Detailed outputs are in the evidence directory. This evidence applies only to
the checksummed local bundle and does not represent production authorization.
EOF

(
  cd "${BUNDLE_DIR}"
  if command -v shasum >/dev/null 2>&1; then
    find . -type f ! -name MANIFEST.sha256 -print | LC_ALL=C sort | while IFS= read -r file; do
      shasum -a 256 "${file}"
    done >MANIFEST.sha256
  elif command -v sha256sum >/dev/null 2>&1; then
    find . -type f ! -name MANIFEST.sha256 -print | LC_ALL=C sort | while IFS= read -r file; do
      sha256sum "${file}"
    done >MANIFEST.sha256
  else
    fail "no SHA-256 utility is available"
  fi
)

FAYGER_BUNDLE_DIR="${BUNDLE_DIR}" "${BUNDLE_DIR}/verify-bundle.sh" | tee -a "${QUALITY_LOG}"

tar -C "${OUT_ROOT}" -czf "${ARCHIVE}" "${BUNDLE_NAME}"
(
  cd "${OUT_ROOT}"
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "${BUNDLE_NAME}.tar.gz" >"${BUNDLE_NAME}.tar.gz.sha256"
  else
    sha256sum "${BUNDLE_NAME}.tar.gz" >"${BUNDLE_NAME}.tar.gz.sha256"
  fi
)

echo ""
echo "PASS: prerelease candidate built"
echo "Bundle:  ${BUNDLE_DIR}"
echo "Archive: ${ARCHIVE}"
echo "SHA-256: ${ARCHIVE}.sha256"
