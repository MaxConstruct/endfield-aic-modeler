#!/usr/bin/env bash
# Builds "AIC Modeler.app" and a .dmg holding it, in target/dist/. The game data is built into the
# binary (crates/aic-app-egui/build.rs), so the app needs nothing beside it.
#
#   scripts/package-macos.sh              # for this Mac's architecture
#   scripts/package-macos.sh --universal  # Apple silicon + Intel; needs
#                                         # `rustup target add aarch64-apple-darwin x86_64-apple-darwin`
#
# The app is signed ad hoc, not notarized: on another Mac, open it the first time with right-click >
# Open (or System Settings > Privacy & Security > Open Anyway).
set -euo pipefail

cd "$(dirname "$0")/.."

NAME="AIC Modeler"
BIN="aic-app-egui"
BUNDLE_ID="io.github.maxconstruct.aic-modeler"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"

universal=false
for arg in "$@"; do
    case "$arg" in
        --universal) universal=true ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done

if $universal; then
    targets=(aarch64-apple-darwin x86_64-apple-darwin)
    for target in "${targets[@]}"; do
        cargo build --release --locked -p aic-app-egui --target "$target"
    done
    binary="target/universal/$BIN"
    mkdir -p target/universal
    lipo -create -output "$binary" "${targets[@]/#/target/}"/release/"$BIN"
    arch="universal"
else
    cargo build --release --locked -p aic-app-egui
    binary="target/release/$BIN"
    arch="$(uname -m)"
fi

dist="target/dist"
app="$dist/$NAME.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/$BIN"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>$NAME</string>
    <key>CFBundleDisplayName</key>
    <string>$NAME</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleExecutable</key>
    <string>$BIN</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.productivity</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$app"

# The disk image: the app and a link to /Applications to drag it onto.
staging="$dist/dmg"
rm -rf "$staging"
mkdir -p "$staging"
cp -R "$app" "$staging/"
ln -s /Applications "$staging/Applications"
dmg="$dist/AIC-Modeler-$VERSION-macos-$arch.dmg"
rm -f "$dmg"
hdiutil create -quiet -volname "$NAME" -srcfolder "$staging" -ov -format UDZO "$dmg"
rm -rf "$staging"

echo "$app"
echo "$dmg"
