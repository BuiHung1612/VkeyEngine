#!/bin/sh
# Build VKeyEngine.app from the Rust sources.
# Usage: ./build-macos.sh [--debug | --universal]   (output: target/VKeyEngine.app)
set -eu
cd "$(dirname "$0")"
APP=target/VKeyEngine.app
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
BUILD=1

if [ "${1:-}" = "--debug" ]; then
    cargo build
    BIN=target/debug/VKeyEngine
elif [ "${1:-}" = "--universal" ]; then
    cargo build --release --target aarch64-apple-darwin
    cargo build --release --target x86_64-apple-darwin
    BIN=target/VKeyEngine-universal
    lipo -create -output "$BIN" target/aarch64-apple-darwin/release/VKeyEngine target/x86_64-apple-darwin/release/VKeyEngine
else
    cargo build --release
    BIN=target/release/VKeyEngine
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/VKeyEngine"
cp macos/Status*.png macos/Icon.icns "$APP/Contents/Resources/"
# black / white accent color (needs Xcode's actool; without it controls use the system accent)
if xcrun --find actool >/dev/null 2>&1; then
    xcrun actool macos/Assets.xcassets --compile "$APP/Contents/Resources" --platform macosx \
        --minimum-deployment-target 11.0 --output-partial-info-plist target/assets-info.plist >/dev/null
fi
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key><string>vi</string>
	<key>CFBundleDisplayName</key><string>VKeyEngine</string>
	<key>CFBundleExecutable</key><string>VKeyEngine</string>
	<key>CFBundleIconFile</key><string>Icon</string>
	<key>CFBundleIdentifier</key><string>com.vkeyengine.app</string>
	<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
	<key>CFBundleName</key><string>VKeyEngine</string>
	<key>CFBundlePackageType</key><string>APPL</string>
	<key>CFBundleShortVersionString</key><string>$VERSION</string>
	<key>CFBundleVersion</key><string>$BUILD</string>
	<key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
	<key>LSMinimumSystemVersion</key><string>11.0</string>
	<key>LSUIElement</key><true/>
	<key>NSAccentColorName</key><string>AccentColor</string>
	<key>NSHumanReadableCopyright</key><string>© 2026 Bùi Hùng (BuiHung1612). GPL v3. Based on OpenKey © 2019 Mai Vu Tuyen.</string>
	<key>NSPrincipalClass</key><string>NSApplication</string>
</dict>
</plist>
PLIST
# Launch agent: starts the app at login and restarts it if it is killed or crashes
# (KeepAlive on unsuccessful exit only, so Quit stays quit).
mkdir -p "$APP/Contents/Library/LaunchAgents"
cat > "$APP/Contents/Library/LaunchAgents/com.vkeyengine.app.agent.plist" <<AGENT
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key><string>com.vkeyengine.app.agent</string>
	<key>BundleProgram</key><string>Contents/MacOS/VKeyEngine</string>
	<key>RunAtLoad</key><true/>
	<key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
	<key>ProcessType</key><string>Interactive</string>
	<key>LimitLoadToSessionType</key><string>Aqua</string>
</dict>
</plist>
AGENT
# A fixed certificate keeps the Accessibility permission across rebuilds and updates (an
# ad-hoc signature changes every build, so macOS forgets the grant). CI sets SIGN_ID to the
# self-signed "VKeyEngine Code Signing" certificate; locally the Apple Development one is
# the default. Falls back to ad-hoc if the identity is missing. No -v: a self-signed
# identity is "not trusted" but signs fine.
SIGN_ID=${SIGN_ID:-"Apple Development: hungbt20@gmail.com (TMJH9RR69S)"}
security find-identity -p codesigning | grep -qF "$SIGN_ID" || SIGN_ID=-
echo "Signing with: $SIGN_ID"
codesign --force --deep --sign "$SIGN_ID" "$APP"
echo "Built $APP ($VERSION)"
