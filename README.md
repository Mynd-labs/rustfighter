# rustfighter

Native Rust reimplementation of the MS-DOS release of **Street Fighter II**
(U.S. Gold / Creative Materials, 1995) — asset pipeline, renderer and players,
with the original game files as input.

## Install (double-click friendly)

**Ubuntu / Debian — `.deb` (recommended):** download
`rustfighter_<version>_amd64.deb` from
[Releases](https://github.com/Mynd-labs/rustfighter/releases), double-click it
to install, then launch **Rustfighter** from your applications menu.

**Any Linux — AppImage:** download `rustfighter-<version>-x86_64.AppImage`,
make it executable (`chmod +x` or right-click → Properties → *Allow executing
file as a program*), and double-click it. On Ubuntu 24.04 you may first need
`sudo apt install libfuse2t64`.

**Tarball:** unpack and run `./rfplay` — nothing to install.

On first start the app looks for the original game files automatically
(working directory, next to the executable, `~/Downloads`, `~/Desktop`,
`~/Games`); if it cannot find them it opens a folder picker to select the
folder containing `INDEX.SF2`. The chosen folder is remembered in
`~/.config/rustfighter/config`.

> The original game data files are **not** distributed with this project.

**Everything in one double-clickable folder (personal use):** if you already have
the game files, `scripts/make-bundle.sh` (run from a source checkout) produces a
self-contained folder — program *and* game data plus a launcher — that needs no
configuration at all:

```bash
scripts/make-bundle.sh /path/to/extracted/street-fighter-ii
# -> Rustfighter/ (double-click rfplay) and Rustfighter.tar.gz
```

The bundle is for **personal use only**: the game data is copyrighted and must
not be uploaded or redistributed.

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
| `0` | attract mode (auto-cycling) |
| `←` `→` | previous/next item (frame step in character mode) |
| `↑` `↓` | character, stage; screen pan |
| `Space` | pause auto-play |
| `V` | sprite bank variant (B/T) |
| `P` | character palette (T001/T002) |
| `S` | save screenshot (to your home directory) |
| `Esc` | quit |

## Troubleshooting

* **It seems stuck / the screen never changes:** the player starts in *attract
  mode* and rotates through screens (every ~4 s), characters and stages
  automatically. Press `0` to return to attract, `1`–`4` for a specific mode,
  `Space` to pause/resume, `Esc` to quit. A full startup log is written to
  `~/.config/rustfighter/log.txt`.
* **"Game files not found" window:** put the extracted game folder where the app
  looks (next to the program, `~/Downloads`, `~/Desktop`, `~/Games`) and it is
  found automatically — or launch `rfplay --game <folder>`, or use the folder
  picker that appears on first start. The choice is remembered.
* **AppImage does not start:** on Ubuntu 24.04 install FUSE first
  (`sudo apt install libfuse2t64`), or use the `.deb` / personal bundle instead.
* **Wrong colours or missing artwork:** report it with the log file and your
  distribution; rendering is validated against all 3,749 records on the
  reference data set.

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
