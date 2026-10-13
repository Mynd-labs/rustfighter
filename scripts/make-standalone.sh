#!/usr/bin/env bash
# Build a SINGLE-FILE Rustfighter with the game data compiled into the binary.
# No game files are needed at runtime afterwards. For personal use only — the
# resulting binary contains the copyrighted game data and must not be
# redistributed or uploaded.
#
# usage: scripts/make-standalone.sh <game-folder> [output-binary]

set -euo pipefail

game="${1:-}"
out="${2:-rustfighter-standalone}"
if [ -z "$game" ]; then
    echo "usage: $0 <game-folder> [output-binary]" >&2
    exit 2
fi
if [ ! -f "$game/INDEX.SF2" ] && [ ! -f "$game/index.sf2" ]; then
    echo "error: $game does not contain INDEX.SF2 (is that the extracted game folder?)" >&2
    exit 1
fi

SF2_DIR="$game" cargo build --release --features embedded,audio --target-dir target/standalone

cp target/standalone/release/rfplay "$out"
cp target/standalone/release/rfdump "${out}-rfdump"
chmod +x "$out" "${out}-rfdump"

echo
echo "Single-file build ready:"
echo "  $out          ($(du -h "$out" | cut -f1)) — double-click to play"
echo "  ${out}-rfdump ($(du -h "${out}-rfdump" | cut -f1)) — headless decoder"
echo "No game files are needed; the data is inside the binary."
echo "NOTE: personal use only — do not redistribute this binary."
