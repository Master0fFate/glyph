#!/usr/bin/env bash
set -euo pipefail

binary="$1"
root="$(cd "$(dirname "$0")/../.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -n 1)"
work="$root/target/dmg"
app="$work/Glyph.app"
iconset="$work/glyph.iconset"
stage="$work/stage"

rm -rf "$work"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$iconset" "$stage" "$root/dist"
cp "$binary" "$app/Contents/MacOS/glyph"
sed "s/__VERSION__/$version/g" "$root/installer/macos/Info.plist" > "$app/Contents/Info.plist"

for size in 16 32 128 256; do
    sips -z "$size" "$size" "$root/assets/glyph.png" --out "$iconset/icon_${size}x${size}.png" > /dev/null
    sips -z "$((size * 2))" "$((size * 2))" "$root/assets/glyph.png" --out "$iconset/icon_${size}x${size}@2x.png" > /dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/glyph.icns"

codesign --force --deep --sign - "$app"

cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
hdiutil create -volname Glyph -srcfolder "$stage" -ov -format UDZO "$root/dist/Glyph-$version-macos-universal.dmg"
