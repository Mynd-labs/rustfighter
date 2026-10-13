# rustfighter

Native Rust reimplementation of the MS-DOS release of **Street Fighter II**
(U.S. Gold / Creative Materials, 1995) — asset pipeline, renderer and players,
with the original game files as input.

## What works today

| Component | Status |
|---|---|
| `INDEX.SF2` directory parser (3,749 records) | ✅ validated against the original loader math |
| LZW codec ("method 3") | ✅ all 3,749 records decode to exact declared sizes |
| Nibble-RLE sprites (4 bpp) | ✅ all 3,223 sprite records expand exactly |
| Palettes (6-bit VGA, 16 / 256 colour, embedded headers) | ✅ |
| Fonts (8×8, 10×16, 12×12, 14×19) | ✅ rendered with the game's own glyph data |
| Screens (intro, title, world-select, messages) | ✅ with colour-key handling |
| Stages (64-px vertical slices, TOP+BOT) | ✅ assembled and pannable |
| Windowed player (`rfplay`, minifb) | ✅ 4 modes, keyboard controls |
| Headless dumper (`rfdump`) | ✅ PNG evidence + `index.tsv` |
| Fight engine / game logic | ❌ not yet ported (next milestone) |

## Requirements

* Rust (stable, 1.75+; tested with 1.99)
* The original game data files (`SF2.EXE`, `INDEX.SF2`, the four `.SF2`
  archives, ...) — **not** redistributed here. Either set `SF2_DIR` or pass
  `--game <dir>`.

## Build & run

```bash
cargo build --release

# Interactive player (Linux desktop; opens a 320x200 window, scaled)
./target/release/rfplay --game /path/to/street-fighter-ii

# No-window verification: renders one frame of each mode to PNG
./target/release/rfplay --game /path/to/street-fighter-ii --shot /tmp/shots

# Decode everything: index.tsv, screens, stages, character sheets, fonts
./target/release/rfdump --game /path/to/street-fighter-ii --out /tmp/dump

# Validation suite (skips cleanly when the data files are absent)
SF2_DIR=/path/to/street-fighter-ii cargo test --release
```

Headless builds (no windowing dependency) work with
`cargo build --release --no-default-features`.

### Player controls

| Key | Action |
|---|---|
| `1`–`4` | screens / characters / stages / fonts |
| `←` `→` | previous/next item (frame step in character mode) |
| `↑` `↓` | character, stage; screen pan |
| `Space` | pause auto-play |
| `V` | sprite bank variant (B/T) |
| `P` | character palette (T001/T002) |
| `S` | save screenshot |
| `Esc` | quit |

## Reverse-engineering documentation

Everything the port is built on, with code addresses and validation results:

* [docs/README.md](docs/README.md) — findings index
* [docs/findings/engine-overview.md](docs/findings/engine-overview.md)
* [docs/findings/asset-loader.md](docs/findings/asset-loader.md)
* [docs/findings/sprite-format.md](docs/findings/sprite-format.md)
* [docs/findings/general-sf2-formats.md](docs/findings/general-sf2-formats.md)
* [docs/findings/config-highscores.md](docs/findings/config-highscores.md)

Analysis was performed with [REA](https://github.com/morluto/rea) 6.4.0 and
its Ghidra 12.1.4 provider; Ghidra linear addresses map to file offsets via
`file_offset = linear - 0xFFA0`.

## Roadmap toward full gameplay

1. Decode `ani*`/`frm*`/`ait*` per-character tables (animation, frame, AI) —
   required for sprite origins, action sequencing and hitboxes.
2. Port the fight state machine (actions, movement, hit resolution) from
   `SF2.EXE` function by function.
3. Input + configuration (`CONFIG.SF2` layouts are already documented).
4. Sound: PC-speaker `tun*` tables, VOC samples, MIDI (`mit*`), plus the
   Sound Blaster driver path.
