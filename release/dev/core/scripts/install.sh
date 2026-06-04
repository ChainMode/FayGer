#!/usr/bin/env bash
# 把 fayger 可执行文件安装到用户 PATH（默认 ~/.fayger/bin）。
#
# 本脚本不联网下载；它复制一个已构建/已下载的二进制到安装目录。两种用法：
#
#   1) 指定已构建的二进制：
#        FAYGER_BIN=/path/to/fayger ./scripts/install.sh
#   2) 作为参数传入：
#        ./scripts/install.sh /path/to/fayger
#
# 若都未提供，尝试 release/dev/core/target/release/fayger。
#
# 安装目录可用 FAYGER_HOME 覆盖（默认 ~/.fayger）。
set -euo pipefail

FAYGER_HOME="${FAYGER_HOME:-${HOME}/.fayger}"
INSTALL_BIN_DIR="${FAYGER_HOME}/bin"

# 解析源二进制路径。
SRC="${1:-${FAYGER_BIN:-}}"
if [[ -z "${SRC}" ]]; then
  SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  WORKSPACE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
  CANDIDATE="${WORKSPACE_DIR}/target/release/fayger"
  if [[ -x "${CANDIDATE}" ]]; then
    SRC="${CANDIDATE}"
  elif [[ -x "${CANDIDATE}.exe" ]]; then
    SRC="${CANDIDATE}.exe"
  fi
fi

if [[ -z "${SRC}" || ! -x "${SRC}" ]]; then
  echo "error: no fayger binary found." >&2
  echo "  build first:  ./scripts/build-release.sh" >&2
  echo "  or pass path: FAYGER_BIN=/path/to/fayger ./scripts/install.sh" >&2
  exit 1
fi

mkdir -p "${INSTALL_BIN_DIR}"
DEST="${INSTALL_BIN_DIR}/fayger"
cp "${SRC}" "${DEST}"
chmod +x "${DEST}"

echo "==> Installed fayger to ${DEST}"
"${DEST}" version || true

# PATH 提示。
case ":${PATH}:" in
  *":${INSTALL_BIN_DIR}:"*)
    echo "==> ${INSTALL_BIN_DIR} is already on your PATH. Run: fayger version"
    ;;
  *)
    echo ""
    echo "==> Add fayger to your PATH by appending this line to your shell profile:"
    echo "      export PATH=\"${INSTALL_BIN_DIR}:\$PATH\""
    echo "    (e.g. ~/.bashrc, ~/.zshrc) then restart your shell."
    ;;
esac
