#!/bin/sh
# FayGer Clean Uninstaller for macOS & Linux
# Default (preserves FayID vault & MeriToken state):
#   curl -fsSL https://raw.githubusercontent.com/ChainMode/FayGer/main/scripts/uninstall.sh | sh
# Full purge (also deletes ~/.fayger/state):
#   curl -fsSL https://raw.githubusercontent.com/ChainMode/FayGer/main/scripts/uninstall.sh | sh -s -- --purge-state
set -eu

PURGE_STATE=0
for arg in "$@"; do
  case "$arg" in
    --purge-state)
      PURGE_STATE=1
      ;;
    -h|--help)
      echo "Usage: uninstall.sh [--purge-state]"
      echo ""
      echo "Options:"
      echo "  --purge-state   Also delete \$HOME/.fayger/state (FayID vault, mounted souls & MeriToken ledger)."
      echo "                  By default, state is preserved so your digital soul credentials are never lost."
      exit 0
      ;;
    *)
      echo "Error: Unknown argument '$arg' (expected --purge-state or --help)" >&2
      exit 2
      ;;
  esac
done

INSTALL_DIR="${FAYGER_INSTALL_DIR:-${HOME}/.fayger/bin}"
STATE_DIR="${FAYGER_STATE_DIR:-${HOME}/.fayger/state}"
BIN_PATH="${INSTALL_DIR}/fayger"
FAYGER_ROOT="$(dirname "$INSTALL_DIR")"

echo "==> Checking for running local fayger daemon..."
if command -v pgrep >/dev/null 2>&1 && pgrep -f "${BIN_PATH} up" >/dev/null 2>&1; then
  echo "==> Stopping running fayger daemon (${BIN_PATH})..."
  pkill -TERM -f "${BIN_PATH} up" >/dev/null 2>&1 || true
  sleep 1
else
  echo "==> No background fayger daemon detected at ${BIN_PATH} (or already stopped)."
fi

if [ -f "$BIN_PATH" ]; then
  rm -f "$BIN_PATH"
  echo "==> Removed binary: ${BIN_PATH}"
else
  echo "==> Binary not present at ${BIN_PATH} (nothing to remove)."
fi

if [ -d "$INSTALL_DIR" ]; then
  rmdir "$INSTALL_DIR" 2>/dev/null || true
fi

if [ "$PURGE_STATE" -eq 1 ]; then
  if [ -d "$STATE_DIR" ]; then
    rm -rf "$STATE_DIR"
    echo "==> Purged state directory: ${STATE_DIR}"
  else
    echo "==> State directory not present at ${STATE_DIR}."
  fi
  if [ -d "$FAYGER_ROOT" ]; then
    rmdir "$FAYGER_ROOT" 2>/dev/null || true
  fi
else
  if [ -d "$STATE_DIR" ]; then
    echo "==> Preserved state directory (FayID vault & MeriToken pocket kept safe): ${STATE_DIR}"
    echo "    To permanently delete local state as well, re-run with --purge-state:"
    echo "    curl -fsSL https://raw.githubusercontent.com/ChainMode/FayGer/main/scripts/uninstall.sh | sh -s -- --purge-state"
  fi
fi

echo "==> FayGer uninstall complete."
