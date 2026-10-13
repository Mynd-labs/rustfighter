#!/usr/bin/env bash
# Build a self-contained, double-clickable Rustfighter folder from YOUR OWN copy
# of the game files. For personal use — do not redistribute the result, as the
# game data is copyrighted.
#
# usage: scripts/make-bundle.sh <game-folder> [output-dir]

set -euo pipefail

game="${1:-}"
out="${2:-Rustfighter}"
if [ -z "$game" ]; then
    echo "usage: $0 <game-folder> [output-dir]" >&2
    exit 2
fi
if [ ! -f "$game/INDEX.SF2" ] && [ ! -f "$game/index.sf2" ]; then
    echo "error: $game does not contain INDEX.SF2 (is that the extracted game folder?)" >&2
    exit 1
fi

cargo build --release --features audio

rm -rf "$out"
mkdir -p "$out/street-fighter-ii"
cp target/release/rfplay target/release/rfdump "$out/"
cp README.md LICENSE "$out/"
cp -a "$game"/. "$out/street-fighter-ii/"

abs="$(cd "$out" && pwd)"
cat > "$out/Rustfighter.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Rustfighter
Comment=Street Fighter II (DOS) native player (personal bundle)
Exec=$abs/rfplay
Icon=applications-games
Terminal=false
Categories=Game;ArcadeGame;
EOF

cat > "$out/START-HERE.txt" <<'EOF'
Rustfighter personal bundle
===========================

Double-click "rfplay" to start (or run START via the .desktop launcher).
The game files live in the street-fighter-ii folder next to the program
and are found automatically — no configuration needed.

Manual start:   ./rfplay --game street-fighter-ii
Headless dump:  ./rfdump --game street-fighter-ii --out dump

Controls: 1-4 screens/characters/stages/fonts, 0 attract mode,
arrows navigate, Space pauses, V/P variants, S screenshot, Esc quits.
EOF

chmod +x "$out/rfplay" "$out/rfdump"
strip "$out/rfplay" "$out/rfdump" 2>/dev/null || true

# Also include a single-file standalone build with the data inside the binary.
if SF2_DIR="$game" cargo build --release --features embedded,audio \
    --target-dir target/standalone >/dev/null 2>&1; then
    cp target/standalone/release/rfplay "$out/rfplay-standalone"
    chmod +x "$out/rfplay-standalone"
    strip "$out/rfplay-standalone" 2>/dev/null || true
fi

archive="$out.tar.gz"
tar -czf "$archive" -C "$(dirname "$out")" "$(basename "$out")"

echo "Bundle ready:  $out"
echo "Archive:       $archive"
echo
echo "Double-click $out/rfplay, or extract the archive anywhere and run rfplay."
