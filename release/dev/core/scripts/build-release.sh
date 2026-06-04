#!/usr/bin/env bash
# 构建优化版 fayger 二进制并打印产物路径与 SHA-256 校验和。
#
# 用法：
#   ./scripts/build-release.sh
#
# 在 release/dev/core/ 目录下运行（脚本会自行切到 workspace 根）。
set -euo pipefail

# 切到 workspace 根（脚本所在目录的上一级）。
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${WORKSPACE_DIR}"

echo "==> Building fayger (release) in ${WORKSPACE_DIR}"
cargo build --release -p fayger-cli --bin fayger

BIN="${WORKSPACE_DIR}/target/release/fayger"
if [[ ! -x "${BIN}" ]]; then
  # Windows 下产物带 .exe 后缀。
  if [[ -x "${BIN}.exe" ]]; then
    BIN="${BIN}.exe"
  else
    echo "error: built binary not found at ${BIN}" >&2
    exit 1
  fi
fi

echo "==> Built: ${BIN}"
"${BIN}" version

echo "==> SHA-256:"
if command -v shasum >/dev/null 2>&1; then
  shasum -a 256 "${BIN}"
elif command -v sha256sum >/dev/null 2>&1; then
  sha256sum "${BIN}"
else
  echo "(no shasum/sha256sum available; skipping checksum)" >&2
fi

echo "==> Done. Install with: FAYGER_BIN=${BIN} ./scripts/install.sh"
