#!/bin/sh
# FayGer One-Line Installer for macOS & Linux
# Usage: curl -fsSL https://raw.githubusercontent.com/ChainMode/FayGer/main/scripts/install.sh | sh
set -eu

VERSION="${FAYGER_VERSION:-v0.3.0}"
INSTALL_DIR="${FAYGER_INSTALL_DIR:-${HOME}/.fayger/bin}"
REPO="${FAYGER_GITHUB_REPO:-ChainMode/FayGer}"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)
    case "$ARCH" in
      arm64|aarch64) TARGET="aarch64-apple-darwin" ;;
      x86_64|amd64)  TARGET="x86_64-apple-darwin" ;;
      *)
        echo "Error: Unsupported macOS architecture: $ARCH" >&2
        exit 1
        ;;
    esac
    ;;
  Linux)
    case "$ARCH" in
      x86_64|amd64)  TARGET="x86_64-unknown-linux-gnu" ;;
      aarch64|arm64) TARGET="aarch64-unknown-linux-gnu" ;;
      *)
        echo "Error: Unsupported Linux architecture: $ARCH" >&2
        exit 1
        ;;
    esac
    ;;
  *)
    echo "Error: Unsupported OS '$OS'. On Windows, use scripts/install.ps1 via PowerShell." >&2
    exit 1
    ;;
esac

ASSET="fayger-${TARGET}"
BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"
BIN_URL="${BASE_URL}/${ASSET}"
SUM_URL="${BASE_URL}/${ASSET}.sha256"

TMP_DIR="$(mktemp -d 2>/dev/null || mktemp -d -t fayger-install)"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT INT TERM

download_file() {
  url="$1"
  dest="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$dest"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO "$dest" "$url"
  else
    echo "Error: Neither 'curl' nor 'wget' was found on PATH." >&2
    exit 1
  fi
}

echo "==> Downloading FayGer ${VERSION} for ${TARGET}..."
download_file "$BIN_URL" "${TMP_DIR}/${ASSET}"
download_file "$SUM_URL" "${TMP_DIR}/${ASSET}.sha256"

EXPECTED_SHA="$(awk '{print $1}' "${TMP_DIR}/${ASSET}.sha256" | tr 'A-F' 'a-f')"
if [ -z "$EXPECTED_SHA" ]; then
  echo "Error: Failed to parse SHA-256 checksum from ${ASSET}.sha256" >&2
  exit 1
fi

if command -v shasum >/dev/null 2>&1; then
  ACTUAL_SHA="$(shasum -a 256 "${TMP_DIR}/${ASSET}" | awk '{print $1}' | tr 'A-F' 'a-f')"
elif command -v sha256sum >/dev/null 2>&1; then
  ACTUAL_SHA="$(sha256sum "${TMP_DIR}/${ASSET}" | awk '{print $1}' | tr 'A-F' 'a-f')"
else
  echo "Error: Neither 'shasum' nor 'sha256sum' available to verify SHA-256." >&2
  exit 1
fi

if [ "$EXPECTED_SHA" != "$ACTUAL_SHA" ]; then
  echo "Error: SHA-256 verification failed!" >&2
  echo "  Expected: $EXPECTED_SHA" >&2
  echo "  Actual:   $ACTUAL_SHA" >&2
  exit 1
fi
echo "==> SHA-256 verified: ${ACTUAL_SHA}"

mkdir -p "$INSTALL_DIR"
cp "${TMP_DIR}/${ASSET}" "${INSTALL_DIR}/fayger"
chmod +x "${INSTALL_DIR}/fayger"

echo "==> Running installation smoke check..."
"${INSTALL_DIR}/fayger" version

echo ""
echo "FayGer ${VERSION} installed to ${INSTALL_DIR}/fayger"
case ":${PATH:-}:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    echo ""
    echo "Add FayGer to your PATH:"
    echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    ;;
esac
echo ""
echo "Launch FayGer Engine + Web Console:"
echo "  fayger up --open"
