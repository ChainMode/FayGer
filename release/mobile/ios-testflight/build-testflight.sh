#!/usr/bin/env bash
# FayGer Mobile Pocket Guardian — iOS Archive & App Store Connect TestFlight Uploader
# Usage:
#   APPLE_TEAM_ID="ABCDE12345" \
#   ASC_API_KEY_ID="KEYID12345" \
#   ASC_API_ISSUER_ID="00000000-0000-0000-0000-000000000000" \
#   ./build-testflight.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

if [ -z "${APPLE_TEAM_ID:-}" ]; then
  echo "Error: APPLE_TEAM_ID environment variable is required (10-character Apple Developer Team ID)." >&2
  exit 1
fi

BUILD_DIR="${SCRIPT_DIR}/build"
ARCHIVE_PATH="${BUILD_DIR}/FayGerGuardian.xcarchive"
EXPORT_DIR="${BUILD_DIR}/export"
EFFECTIVE_PLIST="${BUILD_DIR}/ExportOptions.effective.plist"

mkdir -p "$BUILD_DIR"

# 1. Bootstrap Capacitor iOS project if ios/App has not been generated yet
if [ ! -d "${SCRIPT_DIR}/ios/App" ]; then
  echo "==> Initializing Capacitor iOS wrapper for ai.ifay.fayger..."
  if [ ! -f "${SCRIPT_DIR}/package.json" ]; then
    cat > "${SCRIPT_DIR}/package.json" <<'EOF'
{
  "name": "fayger-ios-testflight",
  "private": true,
  "version": "0.3.1",
  "dependencies": {
    "@capacitor/core": "^6.1.0",
    "@capacitor/cli": "^6.1.0",
    "@capacitor/ios": "^6.1.0"
  }
}
EOF
  fi
  npm install --no-audit --no-fund
  npx cap add ios
fi

echo "==> Syncing offline Pocket Guardian bundle into ios/App..."
npx cap sync ios

# Enable local LAN networking (NSAllowsLocalNetworking) so iOS can pair with LAN FayGer Hosts
INFO_PLIST="${SCRIPT_DIR}/ios/App/App/Info.plist"
if [ -f "$INFO_PLIST" ] && command -v /usr/libexec/PlistBuddy >/dev/null 2>&1; then
  /usr/libexec/PlistBuddy -c "Add :NSAppTransportSecurity dict" "$INFO_PLIST" 2>/dev/null || true
  /usr/libexec/PlistBuddy -c "Set :NSAppTransportSecurity:NSAllowsLocalNetworking true" "$INFO_PLIST" 2>/dev/null || \
    /usr/libexec/PlistBuddy -c "Add :NSAppTransportSecurity:NSAllowsLocalNetworking bool true" "$INFO_PLIST"
  /usr/libexec/PlistBuddy -c "Add :NSLocalNetworkUsageDescription string 'FayGer Guardian connects to your local LAN FayGer Host for Faying supervision, CAP approval, and G0 emergency halt.'" "$INFO_PLIST" 2>/dev/null || true
fi

sed "s/REPLACE_WITH_APPLE_TEAM_ID/${APPLE_TEAM_ID}/g" "${SCRIPT_DIR}/ExportOptions.plist" > "$EFFECTIVE_PLIST"

echo "==> Archiving FayGer Guardian (Team ID: ${APPLE_TEAM_ID})..."
xcodebuild \
  -project "${SCRIPT_DIR}/ios/App/App.xcodeproj" \
  -scheme App \
  -configuration Release \
  -destination "generic/platform=iOS" \
  -archivePath "$ARCHIVE_PATH" \
  DEVELOPMENT_TEAM="$APPLE_TEAM_ID" \
  CODE_SIGN_STYLE=Automatic \
  -allowProvisioningUpdates \
  archive

echo "==> Exporting signed .ipa for App Store Connect / TestFlight..."
xcodebuild \
  -exportArchive \
  -archivePath "$ARCHIVE_PATH" \
  -exportOptionsPlist "$EFFECTIVE_PLIST" \
  -exportPath "$EXPORT_DIR" \
  -allowProvisioningUpdates

IPA_FILE="$(find "$EXPORT_DIR" -maxdepth 1 -name '*.ipa' | head -n 1 || true)"
if [ -n "$IPA_FILE" ] && [ -n "${ASC_API_KEY_ID:-}" ] && [ -n "${ASC_API_ISSUER_ID:-}" ]; then
  echo "==> Uploading ${IPA_FILE} to App Store Connect TestFlight via xcrun altool..."
  xcrun altool --upload-app \
    --type ios \
    --file "$IPA_FILE" \
    --apiKey "$ASC_API_KEY_ID" \
    --apiIssuer "$ASC_API_ISSUER_ID"
fi

echo "==> TestFlight build pipeline completed. Artifacts in: ${BUILD_DIR}"
