#!/bin/sh
# FayGer macOS Graphical .app & .dmg Bundler (with Ad-Hoc or Apple Developer ID Signing + Optional Notarization)
# Usage: ./scripts/package-macos-dmg.sh <binary_path> <version> <out_dmg_path>
set -eu

if [ "$#" -ne 3 ]; then
  echo "Usage: $0 <binary_path> <version> <out_dmg_path>" >&2
  exit 1
fi

BIN_PATH="$1"
RAW_VERSION="$2"
OUT_DMG="$3"

if [ ! -f "$BIN_PATH" ]; then
  echo "Error: FayGer binary not found at: $BIN_PATH" >&2
  exit 1
fi

if [ "$(uname -s)" != "Darwin" ]; then
  echo "Error: package-macos-dmg.sh requires macOS (codesign + hdiutil)." >&2
  exit 1
fi

# Strip optional leading 'v' so CFBundleShortVersionString is strictly numeric (e.g. 0.3.1)
BUNDLE_VERSION="${RAW_VERSION#v}"
case "$BUNDLE_VERSION" in
  [0-9]*) ;;
  *) BUNDLE_VERSION="0.3.1" ;;
esac

STAGE_ROOT="$(mktemp -d 2>/dev/null || mktemp -d -t fayger-dmg)"
cleanup() {
  rm -rf "$STAGE_ROOT"
}
trap cleanup EXIT INT TERM

DMG_SRC="${STAGE_ROOT}/dmg-root"
APP_DIR="${DMG_SRC}/FayGer.app"
CONTENTS_DIR="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"

mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

# 1. Copy compiled fayger engine binary
cp "$BIN_PATH" "${MACOS_DIR}/fayger"
chmod +x "${MACOS_DIR}/fayger"

# 2. Create fayger-launcher GUI entrypoint script
cat > "${MACOS_DIR}/fayger-launcher" <<'EOF'
#!/bin/sh
set -eu

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
FAYGER_BIN="${SCRIPT_DIR}/fayger"
STATE_DIR="${HOME}/.fayger"
LOG_FILE="${STATE_DIR}/faygerd.log"
CONSOLE_URL="http://127.0.0.1:7979"

mkdir -p "$STATE_DIR"

# If FayGer Dashboard is already listening on :7979, open browser directly
if command -v lsof >/dev/null 2>&1 && lsof -nP -iTCP:7979 -sTCP:LISTEN >/dev/null 2>&1; then
  open "$CONSOLE_URL"
  exit 0
fi

if command -v curl >/dev/null 2>&1 && curl -fsS --max-time 1 "$CONSOLE_URL" >/dev/null 2>&1; then
  open "$CONSOLE_URL"
  exit 0
fi

nohup "$FAYGER_BIN" up --open >>"$LOG_FILE" 2>&1 </dev/null &
exit 0
EOF
chmod +x "${MACOS_DIR}/fayger-launcher"

# 3. Write Contents/Info.plist
cat > "${CONTENTS_DIR}/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>FayGer</string>
    <key>CFBundleDisplayName</key>
    <string>FayGer</string>
    <key>CFBundleIdentifier</key>
    <string>ai.ifay.fayger</string>
    <key>CFBundleVersion</key>
    <string>${BUNDLE_VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${BUNDLE_VERSION}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleExecutable</key>
    <string>fayger-launcher</string>
    <key>LSMinimumSystemVersion</key>
    <string>12.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF

# 4. Sign FayGer.app (Developer ID with Hardened Runtime if provided, otherwise clean Ad-Hoc)
if [ -n "${APPLE_DEVELOPER_IDENTITY:-}" ]; then
  echo "==> Signing FayGer.app with Apple Developer identity: ${APPLE_DEVELOPER_IDENTITY}"
  codesign --force --options runtime --timestamp --sign "$APPLE_DEVELOPER_IDENTITY" "${MACOS_DIR}/fayger"
  codesign --force --options runtime --timestamp --sign "$APPLE_DEVELOPER_IDENTITY" "${MACOS_DIR}/fayger-launcher"
  codesign --force --options runtime --timestamp --sign "$APPLE_DEVELOPER_IDENTITY" "$APP_DIR"
else
  echo "==> Signing FayGer.app with clean ad-hoc signature for Apple Silicon..."
  codesign --force --deep --sign - "$APP_DIR"
fi

codesign --verify --deep --strict "$APP_DIR"

# 5. Add /Applications drag-and-drop symlink and create compressed UDZO .dmg
ln -s /Applications "${DMG_SRC}/Applications"

OUT_DIR="$(dirname "$OUT_DMG")"
mkdir -p "$OUT_DIR"
rm -f "$OUT_DMG"

echo "==> Packaging ${OUT_DMG}..."
hdiutil create \
  -volname "FayGer" \
  -srcfolder "$DMG_SRC" \
  -ov \
  -format UDZO \
  "$OUT_DMG" >/dev/null

if [ -n "${APPLE_DEVELOPER_IDENTITY:-}" ]; then
  codesign --force --timestamp --sign "$APPLE_DEVELOPER_IDENTITY" "$OUT_DMG"
fi

# 6. Optional Apple Notarization + Stapling when credentials are configured
if [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_APP_SPECIFIC_PASSWORD:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ]; then
  echo "==> Submitting ${OUT_DMG} to Apple Notary Service..."
  xcrun notarytool submit "$OUT_DMG" \
    --apple-id "$APPLE_ID" \
    --password "$APPLE_APP_SPECIFIC_PASSWORD" \
    --team-id "$APPLE_TEAM_ID" \
    --wait
  echo "==> Stapling notarization ticket to ${OUT_DMG}..."
  xcrun stapler staple "$OUT_DMG"
fi

echo "==> Successfully created macOS disk image: ${OUT_DMG}"
