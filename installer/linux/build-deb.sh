#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -n 1)"
pkg="$root/target/deb/glyph"

rm -rf "$pkg"
install -Dm755 "$root/target/release/glyph" "$pkg/usr/bin/glyph"
install -Dm644 "$root/installer/linux/glyph.desktop" "$pkg/usr/share/applications/glyph.desktop"
install -Dm644 "$root/assets/glyph.png" "$pkg/usr/share/icons/hicolor/256x256/apps/glyph.png"

mkdir -p "$pkg/DEBIAN" "$root/dist"
cat > "$pkg/DEBIAN/control" <<EOF
Package: glyph
Version: $version
Section: editors
Priority: optional
Architecture: amd64
Maintainer: Glyph contributors
Installed-Size: $(du -sk "$pkg/usr" | cut -f1)
Depends: libc6, libfontconfig1, libfreetype6, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libx11-xcb1, libvulkan1
Description: A native Markdown editor for notes, code and diagrams
EOF

dpkg-deb --build --root-owner-group "$pkg" "$root/dist/Glyph-$version-linux-amd64.deb"
