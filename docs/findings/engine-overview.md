# Engine Overview — SF2.EXE (DOS)

## Identity

| Property | Value |
|---|---|
| File | `SF2.EXE`, 79,948 bytes |
| SHA-256 | `cdcbb616e4256f7174a7e018977185825107584a215cd6eec48693117e990c15` |
| Format | MS-DOS MZ (16-bit real mode, x86) |
| MZ header | 96 bytes, 16 relocations, min/max alloc 64 paragraphs |
| Entry (CS:IP) | `0000:04E6` → linear `0x104E6` |
| Stack (SS:SP) | `137F:0400` |
| Analysis | Ghidra 12.1.4 via REA (`rea-workflow`); 373 procedures, 203 strings |

Credits strings embedded in the binary:

- Game Code: James E Fisher; Original Logic: Gordon Fong
- Music and Sound: Dave Lowe
- **Disk compression: Martin H Smith, James E Fisher**
- "Street Fighter ][ Copyright (C) CAPCOM 1992 / All rights reserved. (Dated: 08-Sep-1993 12:52)"

## Memory map (Ghidra linear coordinates)

`file_offset = linear - 0xFFA0`. Primary data segment (DS) base is linear `0x1AA40`
(relocated `0x0AA4`), so DS-relative references appear in code as immediates
`linear - 0x1AA40`. A **second data segment** (relocated `0x12A4`) sits at linear
`0x22A40` and holds the error-message blob (listed as `CODE_3` in the raw overview).

| Region | Range | Size | Notes |
|---|---|---|---|
| Image | `0x10000` | — | code starts here; `entry` at `0x104E6` |
| Code | `0x10000`–`~0x1AA40` | ~43 KB | 16-bit real-mode code + embedded tables |
| Data segment | `0x1AA40`–`~0x23BEC` | ~37 KB | strings, tables, runtime variables referenced via DS |

## Runtime & system interface

- DOS `INT 21h`: 38 sites — file open (`AH=3Dh`) / read (`AH=3Fh`) / write
  (`AH=40h`) / close (`AH=3Eh`) / seek (`AH=42h`) / allocate (`AH=48h`) wrappers at
  `0x11470`–`0x1150D`; `0x114C8` = tell, `0x114D6` = absolute seek. Alloc also at
  `0x181AD`. Note: `0x132CF` is **EMS page mapping** (`AH=42h`, `INT 67h`), not DOS seek.
- `INT 10h` video (5 sites), `INT 16h` keyboard (3 sites), `INT 1Ah` time (3 sites),
  `INT 67h` EMS (page frame + count detected at `0x132A5`).
- Runtime error strings `_errDOSCritical: Fatal error.$` and `Internal error @ $`.
- Sound output is selectable: `SBDRIVER.DRV` (Sound Blaster), `SPEAKER.`,
  `ROLAND LAPC-1.` — the external `SBDRIVER.DRV` (2,048 bytes, COM-style) is
  loaded at runtime.
- Video: string "Run in 1024x768x256 VESA mode 105." indicates an optional
  VESA 105h high-resolution path in addition to the standard 320x200 mode 13h
  presentation (path not yet traced).
- Memory: DOS allocation with eviction retry; optional EMS staging for an
  expanded-memory asset cache (see [asset-loader.md](asset-loader.md)).

## File inventory

| File | Size | Role |
|---|---|---|
| `SF2.EXE` | 79,948 | engine |
| `INDEX.SF2` | 63,861 | directory of all indexed assets (below) |
| `SPRITE1.SF2` | 1,297,466 | indexed sprite data (file index 0) |
| `SPRITE2.SF2` | 1,302,349 | indexed sprite data (file index 1) |
| `GENERAL.SF2` | 643,620 | fonts, UI screens, palettes, animation tables, audio (file index 2); 7 unreferenced holes (8,956 B) are stale `frm`/`ait` revisions |
| `BACKDROP.SF2` | 774,204 | stage backdrops / layers (file index 3) |
| `CONFIG.SF2` | 47 | saved settings (buffer `DS:0x2F08`; key bindings at +0x16) |
| `HIGHSCOR.SF2` | 94 | high-score table (buffer `DS:0x41FE`; 5 × 18 B + u32) |
| `EXTRAS.SF2` | 182 | `u16 0x14` (20) + 20 × 9-byte records (`09 01 01 01 "BHYY\0"`); header word read at init into `DS:0x4E3C` and required `>= 0x0F`; record consumption open |
| `SBDRIVER.DRV` | 2,048 | Sound Blaster driver, loaded by name from the EXE |

`EXTRAS.SF2` and `SBDRIVER.DRV` are referenced directly by name in the EXE;
`CONFIG.SF2` / `HIGHSCOR.SF2` likewise. `INDEX.SF2` names the four large data
files, which are otherwise never referenced by name in the EXE.

## INDEX.SF2 format (validated at data and code level)

```
0x0000  8 × 16-byte file-name slots (NUL padded)
          [0]=sprite1.sf2 [1]=sprite2.sf2 [2]=general.sf2 [3]=backdrop.sf2
          Slots 4-7 empty. At runtime byte 14-15 of each used slot is
          overwritten with the DOS file handle (0xFFFF = not open).
0x0080  3,749 × 17-byte records:
          u8   flag      high nibble = file index (0..3), low nibble = method
          u24  offset    little-endian, running offset inside the target file
          u16  size      little-endian, record length
          11 B name      space-padded ASCII (e.g. "sprRYUTB000", "aitKEN     ")
```

- Validation: `(file_size - 0x80) / 17` = record count — exactly what the loader
  computes (`FUN_1000_3575`, linear `0x13575`); `0x80 + 3749*17 = 63861` = file size.
- Per-file offset chains are contiguous and end exactly at the file size for
  `SPRITE1.SF2`, `SPRITE2.SF2` and `BACKDROP.SF2`. `GENERAL.SF2` has gaps —
  see [general-sf2-formats.md](general-sf2-formats.md).
- Asset name families: `spr*` sprites (per-character B/T banks), `pal*` palettes,
  `lin*` backdrop layers, `fnt*` fonts, `ani*`/`frm*`/`ait*` animation/frame/attack
  tables, `tun*` tunes, `voc*` voice samples, `drv*` driver image.
- Low nibble `0x3` = disk-compressed record (custom bit-packed LZ, "method 3");
  `0x0` = raw. Fully decoded and validated over all 3,587 method-3 records — see
  [sprite-format.md](sprite-format.md) §2.6 (`_decMth3`).

## Asset pipeline

`FUN_1000_3575` (linear `0x13575`) once-only init:

1. Guarded init (`DS:0x1A5C` flag).
2. Open `INDEX.SF2`; read the 8 × 16-byte filename slots; open each non-empty
   file; store its handle at slot+0x0E.
3. Compute record count `(file_size - 0x80) / 0x11`; allocate an 11-byte-per-record
   in-memory directory (`FUN_1000_3023`); copy flags/offset/length for all records
   (**names are dropped in RAM**); close.
4. Read the 2-byte header of `EXTRAS.SF2` into `DS:0x4E3C`; close.

Runtime asset references are **baked-in negative handles**:
`recordIndex = −SI − 1` (so `SI=0xFFFF` → record 0). There is **no runtime name
lookup** in this build; the only routine that can re-read a record name from
`INDEX.SF2` (`0x139A4`) is unreferenced dead code. Game-facing API (register ABI,
handle in `SI`): `load` `0x136F0`, `free` `0x137AF`, `acquire` `0x138E4`,
`release` `0x1391C`, `pin` `0x13960`, list/n helpers `0x138A0`–`0x138D5`.
Compressed records take `0x13750` → `0x187B6`/`0x187C7` (u16 decompressed size
header `≤ 0x8000`, then LZ payload) → core `0x18850`.

In-memory directory record (11 bytes): `[0..1]` buffer segment, `[2..4]` eviction
stamp, `[5..7]` file offset (u24), `[8..9]` length, `[0xA]` flags (bits 0-1
method, 2 loaded, 3 pinned, 4-6 file slot, 7 in-use).

## Sprite assets (validated)

Each `spr…` record is exactly one frame:

```
method 0: [u8 width][u8 height][RLE…]
method 3: [u16 LE decompressed_size][LZ…] -> [u8 width][u8 height][RLE…]
```

RLE: `00 NN` emits NN transparent bytes; any other byte emits itself. Pixels are
4 bpp, 2 per byte, high nibble = left pixel, row-major, no padding; the payload is
exactly `ceil(width*height/2)` bytes; index 0 is transparent. Character palettes
are separate 48-byte records (`pal<CODE>T001/002` = 16 × 3-byte RGB in VGA 6-bit
range). Verification: 3,223/3,223 sprites decode to exact declared lengths
(3,587/3,587 method-3 records across all families), and renders confirmed
(`sprPLANE000` = plane, `sprHITSTAR0` = star). See [sprite-format.md](sprite-format.md).

## Other asset families (validated)

- `pal*`: banks of 48-byte 16-colour RGB palettes (VGA 6-bit DAC values).
- `fnt*`: raw 8-bpp glyph arrays (96 glyphs, ASCII − 32).
- `lin*`: headerless 8-bpp strips/screens (320×200-class) — UI screens in
  `GENERAL.SF2`, stage layers in `BACKDROP.SF2`.
- `voc*`: Creative Voice File 1.10 samples, stored raw.
- `tun*`: `u16 (period, duration)` note pairs, `0xFFFF`-terminated.
- `mit*`: Standard MIDI Files (format 0, `MThd` header).
- `drvSBDRIVER`: Creative CT-VOICE driver image (differs from the `SBDRIVER.DRV` file).
- `ani*`/`frm*`/`ait*`: per-character animation/frame/attack tables (partially resolved).

`GENERAL.SF2`'s 7 holes were identified as stale compressed revisions of
`frmGUILE`/`ait*` assets left behind by in-place rewrites; no index entry references
them. See [general-sf2-formats.md](general-sf2-formats.md).

## String inventory highlights (203 total)

- Modes/menus: `GAME START`, `OPTION MODE`, `PLAYER SELECT`, `V.S. BATTLE`,
  `R A N K I N G`, `CONTINUE ?`, `GAME OVER`, `QUIT TO DOS`, `LOADING LEVEL ....`
- Controls: `DEFINE CONTROLS`, `JOYSTICK #1/#2`, `KEYBOARD ONE/TWO`,
  `F1/F2 to calibrate`, `Press 'M' for Music`, `Press 'S' for Sound Effects`
- Copy protection prompt: `"Please type in KIXX and press ENTER."`
  (referenced by text routine `FUN_1000_9587` at `0x19587`, which also validates
  the `EXTRAS.SF2` header word); nearby string `"Sorry, key already used!"`
  (role unconfirmed).
- Debug/developer overlay: `Energy`, `Frame`, `Punch`, `AITimer`, `FireButs`,
  `SpriteX/SpriteY`, `Player`, `Action`, `HitFrame`, `Movement`, `PushDist`,
  `DiffLevel`, `Character`, … (an on-screen per-player state display)
- All 12 characters with endings/win-quote dialogue (family story arcs for
  Guile, Blanka, etc.), `PERFECT`, `TIME OVER`, `DRAW GAME`, `YOU LOSE`
- Runtime errors: `_errDOSCritical: Fatal error.$`, `Internal error @ $`

## Key function address map

| Linear | Symbol | Role (confirmed) |
|---|---|---|
| `0x104E6` | `entry` | C-runtime startup: set DS, init libraries, jump to main |
| `0x11465` | `FUN_1000_1465` | main (tail-called from entry) |
| `0x11470` | `FUN_1000_1470` | open file (`AH=3Dh`) |
| `0x1147F` | `FUN_1000_147f` | read file (`AH=3Fh`, far buffer `DX:AX`) |
| `0x1148A` | `FUN_1000_148a` | write file (`AH=40h`) |
| `0x11495` | `FUN_1000_1495` | close file (`AH=3Eh`) |
| `0x1149A` | `FUN_1000_149a` | file size |
| `0x114C8` | `FUN_1000_14c8` | tell (`AH=42h`, `CX:DX=0`) |
| `0x114D6` | `FUN_1000_14d6` | absolute seek (`AH=42h`, `CX:DX=offset`) |
| `0x13575` | `FUN_1000_3575` | `INDEX.SF2` directory loader / asset system init |
| `0x136AA` | `FUN_1000_36aa` | asset system shutdown (close slots, free workspace) |
| `0x136F0` | `FUN_1000_36f0` | load asset by handle (seek + raw/decompress) |
| `0x137AF` | `FUN_1000_37af` | free asset |
| `0x138E4` | `FUN_1000_38e4` | acquire/map asset (in-use bit) |
| `0x1391C` | `FUN_1000_391c` | release/mark clean |
| `0x13960` | `FUN_1000_3960` | pin asset (eviction protection) |
| `0x139A4` | — | dead code: would re-read a record's name from INDEX.SF2 |
| `0x13DEC` | — | sprite renderer (reads `[w][h]` + nibble-RLE pixels) |
| `0x187B6`/`0x187C7`/`0x18850` | — | decompress dispatch / compressed loader / LZW core |
| `0x12FF3/0x12FF9/0x13023` | — | loader helpers (directory build/alloc) |
| `0x15F09` | `FUN_1000_5f09` | load `CONFIG.SF2` (size must be `0x2F`; buffer `DS:0x2F08`) |
| `0x15F50` | `FUN_1000_5f50` | save `CONFIG.SF2` |
| `0x14B65` | `FUN_1000_4b65` | recompute per-player control parameters on input-mode change |
| `0x1831B` | `FUN_1000_831b` | load `HIGHSCOR.SF2` (size must be `0x5E`; buffer `DS:0x41FE`) |
| `0x1834B` | `FUN_1000_834b` | save `HIGHSCOR.SF2` |
| `0x18382` | `FUN_1000_8382` | high-score update (top-score compare, new-record flag `DS:0x42DA`) |
| `0x19587` | `FUN_1000_9587` | text rendering; references KIXX protection text |

## Open questions

1. Full field semantics of `ani*`/`frm*`/`ait*` tables — partially resolved
   (section tables, sizes, character indexing observed) but the runtime readers
   were not traced; needed for sprite origins/hotspots and character logic.
2. Semantics of `spr<CODE>B###` vs `spr<CODE>T###`: two complete, disjoint,
   non-mirrored artwork banks per character; no runtime L/R conversion exists
   (`_spriConvertLR: Code not written.`).
3. `lin*` per-record geometry and `tun*` period-field semantics (inferred; see
   [general-sf2-formats.md](general-sf2-formats.md)).
4. `EXTRAS.SF2` 9-byte record consumption (`09 01 01 01 "BHYY"`) beyond the
   init-time header word.
5. `SBDRIVER.DRV` call convention. Observed: 2,048-byte blob with DOS message
   header (`EB 2A` jump + "(c) 1993" banner), entry at 0x2C saves all registers
   and sets `DS=CS`, state word at `CS:0xEE` (mode 1/2); it contains no `INT`
   instructions and installs no vectors — it is far-called from the EXE
   (call sites not yet mapped).
6. VESA mode 105 path and its relationship to the default 320x200 renderer.
7. Eviction-stamp producer (`DS:0x19C6`) — only ever zeroed in this build.
8. `CONFIG.SF2` scalar field meanings (values, buffers and write sites are known;
   option-screen labels not traced — see [config-highscores.md](config-highscores.md)).
