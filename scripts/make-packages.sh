#!/usr/bin/env bash
# Build the distributable Linux packages for rustfighter: .deb, AppImage and
# a portable tarball. The binaries are data-free: they load the user's own
# Street Fighter II DOS files (`rfight --game DIR`, else SF2_DIR, else the
# folder next to the binary).
#
# Usage: scripts/make-packages.sh [GAME_DIR] [OUT_DIR] [WORK_DIR]
set -euo pipefail

cd "$(dirname "$0")/.."
GAME="${1:-${SF2_DIR:-}}"
OUT="${2:-/tmp/opencode/release}"
WORK="${3:-/tmp/opencode/package}"
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
ARCH=amd64

echo "== rustfighter $VERSION packages =="
cargo build --release --features audio
strip target/release/rfight target/release/rfplay target/release/rfdump || true

rm -rf "$WORK" "$OUT/pkg"
mkdir -p "$WORK/deb/DEBIAN" "$WORK/deb/usr/bin" \
         "$WORK/deb/usr/share/applications" \
         "$WORK/deb/usr/share/icons/hicolor/256x256/apps"
cp target/release/rfight target/release/rfplay target/release/rfdump "$WORK/deb/usr/bin/"
chmod 755 "$WORK"/deb/usr/bin/*

cat > "$WORK/deb/DEBIAN/control" <<EOF
Package: rustfighter
Version: $VERSION
Section: games
Priority: optional
Architecture: $ARCH
Maintainer: Mynd-labs <noreply@github.com>
Description: Street Fighter II (DOS) native Rust port
 A faithful native port of the 1995 MS-DOS Street Fighter II
 (U.S. Gold / Creative Materials) engine. Needs the original game files:
 run rfight --game DIR or set SF2_DIR. Game data is not distributed.
EOF

cat > "$WORK/deb/usr/share/applications/rustfighter.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Rustfighter Fight
GenericName=Street Fighter II (DOS) native port
Comment=Play the native Rust port of Street Fighter II (DOS)
Exec=rfight
Icon=rustfighter
Terminal=false
Categories=Game;ArcadeGame;
Keywords=Street Fighter;Capcom;DOS;retro;fight;
EOF
cat > "$WORK/deb/usr/share/applications/rustfighter-assets.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Rustfighter Asset Browser
Comment=Browse the original Street Fighter II assets natively
Exec=rfplay
Icon=rustfighter
Terminal=false
Categories=Game;
Keywords=Street Fighter;assets;viewer;
EOF

# Icon: the engine's own SF2 logo on black (needs the game data).
ICON="$WORK/deb/usr/share/icons/hicolor/256x256/apps/rustfighter.png"
if [ -n "$GAME" ] && [ -d "$GAME" ]; then
    TMPD=$(mktemp -d)
    ./target/release/rfdump --game "$GAME" --out "$TMPD" --what screens >/dev/null 2>&1 || true
    if [ -f "$TMPD/screens/linSF2BLOGO.png" ]; then
        convert "$TMPD/screens/linSF2BLOGO.png" -fuzz 8% -fill black -opaque magenta \
            -background black -gravity center -extent 256x256 "$ICON" 2>/dev/null || \
        cp "$TMPD/screens/linSF2BLOGO.png" "$ICON"
    fi
    rm -rf "$TMPD"
fi
if [ ! -f "$ICON" ]; then
    convert -size 256x256 xc:black -fill white -pointsize 48 -gravity center \
        -annotate 0 "SF2" "$ICON" 2>/dev/null || : > "$ICON"
    echo "note: placeholder icon (pass the game dir to embed the real logo)"
fi

dpkg-deb --build --root-owner-group "$WORK/deb" "$OUT/rustfighter_${VERSION}_${ARCH}.deb"

# AppImage
mkdir -p "$WORK/AppDir/usr"
cp -a "$WORK/deb/usr/bin" "$WORK/deb/usr/share" "$WORK/AppDir/usr/"
cp "$WORK/deb/usr/share/applications/rustfighter.desktop" "$WORK/AppDir/rustfighter.desktop"
cp "$ICON" "$WORK/AppDir/rustfighter.png"
cat > "$WORK/AppDir/AppRun" <<'EOF'
#!/bin/sh
SELF=$(readlink -f "$0")
HERE=${SELF%/*}
exec "${HERE}/usr/bin/rfight" "$@"
EOF
chmod 755 "$WORK/AppDir/AppRun"
if [ ! -x /tmp/opencode/appimagetool ]; then
    curl -sSL -o /tmp/opencode/appimagetool \
        https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
    chmod +x /tmp/opencode/appimagetool
fi
APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64 /tmp/opencode/appimagetool \
    "$WORK/AppDir" "$OUT/rustfighter-${VERSION}-x86_64.AppImage" 2>&1 | grep -E "Success|error" || true

# Portable tarball
mkdir -p "$OUT/pkg"
cp target/release/rfight target/release/rfplay target/release/rfdump "$OUT/pkg/"
chmod 755 "$OUT"/pkg/*
cp README.md LICENSE "$OUT/pkg/"
tar -czf "$OUT/rustfighter-v${VERSION}-linux-x86_64.tar.gz" -C "$OUT/pkg" .

echo
echo "Packages in $OUT:"
ls -la "$OUT"/rustfighter_* "$OUT"/rustfighter-* 2>/dev/null
