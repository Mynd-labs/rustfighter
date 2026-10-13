# general.sf2 — asset formats and chain/gap analysis

Reverse-engineering notes for **Street Fighter II (MS-DOS, U.S. Gold / Creative Materials, 1995)**,
based on the shipped files in `/tmp/opencode/rea/game/street-fighter-ii/` (read-only):

- `SF2.EXE` (79,948 bytes, DOS MZ, loader base linear 0x10000)
- `INDEX.SF2` (63,861 bytes), `GENERAL.SF2` (643,620 bytes), `SPRITE1/2.SF2`, `BACKDROP.SF2`, `SBDRIVER.DRV`

Evidence classes used below: **observed** (bytes measured in the shipped files),
**derived** (reproducible decode validated against the whole file set), **inferred**
(code reading / cross-checks). Every decode in this document was executed over *all* records;
the LZW decoder reproduces the exact uncompressed size for all 3,749 records in all four `.sf2`
files, and the sprite decoder consumes **3,223 / 3,223** `spr*` records exactly.

---

## 1. Summary

1. `INDEX.SF2` is one 128-byte header (eight 16-byte filename slots) followed by 3,749 × 17-byte
   records: `[u8 flag][u24 LE offset][u16 LE size][11-byte space-padded name]`.
   `flag>>4` = file index (0 sprite1, 1 sprite2, 2 general, 3 backdrop).
2. **`flag & 3` is a *storage method*, not an asset attribute**: `0` = record stored raw,
   `3` = record payload is LZW-compressed. This is enforced by code:
   `0x136F0` (asset fetch): `mov al,es:[di+0xa]; and al,3; je raw_read; call 0x187B6` and
   `0x187B6`: `cmp al,3; jne fatal_error` — method 3 is the only compressed method.
   Bits `0x04/0x08/0x80` are runtime state (loaded / no-evict) added after load; the INDEX
   loader (`0x13575`) masks each flag with `0x73` when building the in-memory 11-byte entry.
3. **Compressed records are LZW**: 2-byte LE uncompressed-size header, then a variable-width
   code stream (9 bits start, LSB-first 16-bit window, escape code 0x100: `+00` = widen code,
   `+01` = table reset, `+10/11` = end). All 3,749 records decode; declared size always matches.
4. `general.sf2` is **fully tiled by its 353 records with zero overlaps**, except for
   **7 unreferenced holes totalling 8,956 bytes (1.39 % of the file)** in the frm/ait tail.
   The holes contain **stale earlier revisions** of the same `frm*`/`ait*` assets
   (after decompression they differ from the live copies by 1–30 bytes).
   The parent-observed "chain end 634,683 (0x9AF3B)" is the end of the last record when the
   index is walked in *record order*; that is exactly where the physically re-written tail begins.
5. Asset classes found in `general.sf2` (353 records):
   `fnt*` (6), `pal*` (38 incl. raw & compressed), `voc*` (30 Creative Voice), `tun*` (17),
   `spr*` (187 UI/effect sprites), `lin*` (25 linear 8-bpp strips/screens), `ani*` (12),
   `frm*` (12), `ait*` (12), `mit*` (13, **standard MIDI files**), `drvSBDRIVER` (1 driver blob).

---

## 2. INDEX.SF2 record layout and flag semantics (observed + code)

Sample raw records (file 0, offset 0x80):

```
rec0: 03 000000 4205 "sprCHLIB000"   flag=0x03 off=0      size=0x0542=1346   (compressed)
rec1: 03 420500  8a03 "sprCHLIB001"   flag=0x03 off=1346   size=0x038a= 906
rec2: 03 cc0800  9f03 "sprCHLIB002"   flag=0x03 off=2252   size=0x039f= 927
```

Flag census over all 3,749 records:

| file | flag 0x00/0x10/0x20/0x30 (raw) | flag 0x03/0x13/0x23/0x33 (LZW) |
|---|---|---|
| 0 sprite1.sf2 | 27 | 1,605 |
| 1 sprite2.sf2 | 1 (`sprVEGAT013`) | 1,421 |
| 2 general.sf2 | 74 | 279 |
| 3 backdrop.sf2 | 1 (`palBOATCONE`) | 341 |

Code corroboration:

- `FUN_1000_3575` (`0x13575`, INDEX loader): reads the 8 filename slots at `DS:0x19DB`
  (16-byte stride), opens them, stores handles at `slot+0x0E`;
  `count = (file_size-0x80)/17` (INDEX.SF2: `(63861-0x80)/17 = 3749`), then builds an
  in-memory table of **11-byte entries**: `+5..7` = u24 offset, `+8..9` = u16 size,
  `+10` = `flag & 0x73`; `+0..1` zeroed; `+2..4` = 24-bit far pointer to loaded data.
- `0x136F0..0x137AE` (asset fetch): `test es:[di+0xa],4` (already loaded) →
  `mov dl,es:[di+0xa]; and dl,0x70` → **file index = bits 4–6** (slot = `0x19DB + (flag>>4)*0x10`);
  seek `u16 offset` (`es:[di+5..6]`) + `u8 high byte` (`es:[di+7]`), then:
  - raw path: `mov cx,es:[di+8]` (size) → read directly;
  - compressed path: `mov al,es:[di+0xa]; and al,3` ≠ 0 → `call 0x187B6`.
- `0x187C7`: reads 2 bytes (`DS:0x43BE`) = **decompressed size**, allocates that many bytes,
  reads `size-2` compressed bytes, calls `FUN_1000_8850` (`0x18850`) = LZW core, then
  verifies `output_length == [0x43BE]`.
- `0x139A4`: "load 17-byte index record #n" helper: open `INDEX.SF2`, seek `0x80 + 17*n`,
  read 17 bytes into `DS:0x19CA`, close.

So: `record_size` includes the 2-byte LZW header for compressed records;
`record[0..1] = uncompressed size` and `record[2..]` = compressed stream.

---

## 3. general.sf2 chain / gap analysis

### 3.1 Coverage

- Sorted by file offset, the 353 records **tile the file with 0 overlapping bytes**:
  `sum(record sizes) = 634,664`; `128 (header) + 634,664 + 8,956 = 643,620` = file size.
- 345 of 352 adjacent pairs abut exactly; the 7 exceptions are holes:

| # | range (hex) | range (dec) | size |
|---|---|---|---|
| h1 | 0x96646–0x96C18 | 616,006–617,496 | 1,491 |
| h2 | 0x98E6D–0x991B8 | 626,285–627,128 | 844 |
| h3 | 0x99502–0x99F19 | 627,970–630,553 | 2,584 |
| h4 | 0x9A468–0x9A7AD | 631,912–632,749 | 838 |
| h5 | 0x9AF3B–0x9B2AA | 634,683–635,562 | 880 |
| h6 | 0x9B95C–0x9BCA7 | 637,276–638,119 | 844 |
| h7 | 0x9C69F–0x9CC61 | 640,671–642,145 | 1,475 |

Total hole bytes = **8,956 (1.39 %)**; there are **no overlaps and no unreadable regions**.
`sprite1.sf2`, `sprite2.sf2` and `backdrop.sf2` have 0 hole regions and 0 overlaps.

### 3.2 The holes are stale copies of changed frm/ait assets (derived)

Every hole starts with a valid 2-byte LZW size header and decompresses cleanly:

| hole | bytes | decompressed | identified as | diff vs live (after decompress) |
|---|---|---|---|---|
| h1 | 1,491 | 2,248 | `frmGUILE` (earlier) | 30 / 2,248 bytes |
| h2 | 844 | 1,728 | `aitBLANKA` (earlier) | 1 / 1,728 |
| h3a (0x99502) | 879 | 1,728 | `aitDHALISM` (earlier) | 3 / 1,728 |
| h3b (0x99872) | 829 | 1,728 | `aitEHONDO` (earlier) | 2 / 1,728 |
| h3c (0x99BAF) | 875 | 1,728 | `aitGUILE` (earlier) | 2 / 1,728 |
| h4 | 838 | 1,728 | `aitRYU` (earlier) | 2 / 1,728 |
| h5 | 880 | 1,728 | `aitDHALISM` (other revision) | 1 / 1,728 |
| h6 | 844 | 1,728 | `aitBLANKA` (other revision) | 1 / 1,728 |
| h7 | 1,475 | 2,248 | `frmGUILE` (other revision) | 1 / 2,248 |

Note the 1-byte delimiter at `0x99871` (value `0x04`) between h3a and h3b:
`h3 = 879 + 1 + 829 + 875` bytes, and `880 + 829 + 875 = 2584` if that byte is counted
with h3a (LZW streams may end before the record's last byte).

**Gen-A reconstruction (exact)**: the ait block `[0x98C76, 0x9AF3B)` tiles as a strictly
alphabetical sequence mid-way through the file:

```
0x98C76 aitBALROG  503  (live)
0x98E6D aitBLANKA  844  (hole h2)
0x991B9 aitCHUNLI  841  (live)
0x99502 aitDHALISM 880  (hole h3a, incl. 1 pad byte)
0x99872 aitEHONDO  829  (hole h3b)
0x99BAF aitGUILE   875  (hole h3c)
0x99F1A aitKEN     839  (live)
0x9A261 aitMBISON  519  (live)
0x9A468 aitRYU     838  (hole h4)
0x9A7AE aitSAGAT   540  (live)
0x9A9CA aitVEGA    532  (live)
0x9ABDE aitZANGEIF 861  (live)   -> ends at 0x9AF3B exactly
```

The second half of the tail `[0x9AF3B, 0x9DF24)` also tiles completely, with the earlier
copies of `aitBLANKA` (h6) and `frmGUILE` (h7) followed immediately by their current copies:

```
0x9AF3B aitDHALISM-old 880 (h5)
0x9B2AB aitRYU    838 (live)      0x9B95C aitBLANKA-old 844 (h6)
0x9B5F1 aitGUILE  875 (live)      0x9BCA8 aitDHALISM    879 (live)
0x9C017 aitBLANKA 844 (live)      0x9C363 aitEHONDO     828 (live)
0x9C69F frmGUILE-old 1475 (h7)    0x9CC62 frmGUILE      1474 (live) -> end of file
```

Conclusion: `general.sf2` is **not a sequence of independent sub-chains**. It is a compact
archive whose tail was re-written in place more than once; changed `frm*`/`ait*` assets were
re-emitted (leaving their previous compressions behind as unreferenced holes) and the INDEX
points at the current copies. The "directory order" of the tail is alphabetical
(`frmBALROG..frmZANGEIF` then `aitBALROG..aitZANGEIF`), while the physical order is the
residue of the updates.

### 3.3 INDEX record-order anomalies (why the parent saw "non-monotonic chain")

Walking the index in record order, 10 tail records break the `offset == prev_offset+prev_size`
chain (`idx` = record number in INDEX.SF2):

```
idx 3730 frmGUILE   off 0x9CC62 after frmEHONDO   end 0x96646  delta +26140
idx 3731 frmKEN     off 0x96C19 after frmGUILE    end 0x9D224  delta -26123
idx 3738 aitBLANKA  off 0x9C017 after aitBALROG   end 0x98E6D  delta +12714
idx 3739 aitCHUNLI  off 0x991B9 after aitBLANKA   end 0x9C363  delta -12714
idx 3740 aitDHALISM off 0x9BCA8 after aitCHUNLI   end 0x99502  delta +10150
idx 3741 aitEHONDO  off 0x9C363 after aitDHALISM  end 0x9C017  delta  +844
idx 3742 aitGUILE   off 0x9B5F1 after aitEHONDO   end 0x9C69F  delta -4270
idx 3743 aitKEN     off 0x99F1A after aitGUILE    end 0x9B95C  delta -6722
idx 3745 aitRYU     off 0x9B2AB after aitMBISON   end 0x9A468  delta +3651
idx 3746 aitSAGAT   off 0x9A7AE after aitRYU      end 0x9B5F1  delta -3651
```

Sorting by offset removes all anomalies; the file has no ambiguous record boundaries.
The last record in record order is `aitZANGEIF` ending at **634,683 = 0x9AF3B**.

---

## 4. Per-prefix format notes (hex evidence)

### 4.1 `fnt*` — fonts, raw 8-bpp glyph bitmaps (derived)

All fonts are LZW-compressed; the decompressed payload is a flat array of 8-bpp
(one byte per pixel, palette-indexed) glyphs:

| record | decompressed | glyph W×H | glyphs | bytes/glyph |
|---|---|---|---|---|
| fntCOLB0808 | 6,144 | 8×8 | 96 | 64 |
| fntCOLR0808 | 6,144 | 8×8 | 96 | 64 |
| fntCOLR1016 | 15,360 | 10×16 | 96 | 160 |
| fntCOLR1212 | 13,824 | 12×12 | 96 | 144 |
| fntDEBUG4X4 | 3,072 | 4×4 (8×8 cell?) | 96 | 32 |
| fntNUMB1419 | 2,660 | 14×19 | 10 (digits) | 266 |

Glyph order = ASCII `char - 32`. Rendering glyph indices 1, 33, 65 of `fntCOLB0808` gives
`!`, `A`, `a` shapes, confirming the mapping (glyph 1 = the vertical bar). Pixel values are
antialiasing levels (COLB max 31 → 32-level ramp; COLR max 14; NUMB max 14), not 4-bpp nibbles
(fntCOLR0808 has 8 distinct values, e.g. counts of `0`:2908, `8`:1389, `11`:465 …).
Note `fntDEBUG4X4` starts immediately with pixel data (first bytes `00 00 00 …`), i.e. fonts
have no per-record header.

### 4.2 `pal*` — banks of 16-color VGA palettes (derived)

After decompression every `pal*` record is a multiple of 48 bytes and all byte values are ≤ 63:

```
palFACE1RYU (raw, flag 0x20, 48 B): 000000 3c3424 3c2c2038 ...   (16 × RGB, 6-bit DAC values)
palGENERAL  (LZW, 86 B → 96 B):    2 banks   (0x60 0x00 = u16 size header inside stream)
palINTROBOT (LZW, 275 B → 432 B):  9 banks (144 colors)
palVSLOGO   (LZW, 121 B → 768 B): 16 banks (256 colors)
palZANGOUT1 (backdrop, 478 B → 672 B): 14 banks (largest seen in backdrop.sf2)
```

So `pal*` = `N × 48`-byte blocks, each block = 16 colors × 3 bytes of **6-bit VGA DAC**
values. Multi-bank records are palette-bank sets (used for palette animation / screen
regions). There is no palette header; the compressed stream's 2-byte LZW size is the only
header, e.g. `palDISKLOGO` compressed 39 bytes starts `30 00 00 02 92 18 …` where `0x0030`
= 48 = 1 bank. Raw vs compressed form is per-record and interchangeable: 10 of the 12
`palFACE1*` records are raw 48-byte palettes, while `palFACE1KEN`/`palFACE1BAL` hold the same
kind of content LZW'd to 47/46 bytes.

### 4.3 `voc*` — Creative Voice Files, stored raw (observed)

`vocPUNCH` (1,255 bytes, flag 0x20) begins:

```
43 72 65 61 74 69 76 65 20 56 6F 69 63 65 20 46 69 6C 65 1A   "Creative Voice File\x1A"
1A 00   header size 26
0A 01   version 1.10
29 11   checksum (0x1234 + ~0x010A)
01 C8 04 00   block type 1 (sound data), u24 length 0x0004C8 = 1224
38 00   TC=0x38 (rate 1e6/(256-56) = 5000 Hz), pack method 0 = 8-bit unsigned PCM
...
00      final terminator block
```

All 30 `voc*` records parse as VOC 1.10 with one or more type-1 blocks (e.g. `vocBUZZ`
has several), pack method 0, sample rates 4,000–10,000 Hz (TC 6/56/89/113/131/156).
They are stored raw (`flag&3 == 0`) and are *not* LZW-compressed.

### 4.4 `tun*` — driver note tables: u16 (divisor/period, duration) pairs, `0xFFFF` terminated (derived)

All `tun*` decompress to even-length tables ending `… 00 00 02 00 FF FF` (SFX) or
`… xx xx yy yy FF FF` (tunes). Examples:

```
tunBSFX9 (10 B):  F2 08 32 00 00 00 02 00 FF FF   = (2290, 50) (0, 2) END
tunBSFX2 (34 B):  C7 17 01 00 49 19 01 00 CB 1A 01 00 ... 00 00 02 00 FF FF
                  values 6087, 6473, 6859, 7244, 7728, 8200, 8696  (ratio ≈ 2^(1/12))
tunBTUNE1 (158 B → 370 B): 0000 0500 3828 0500 C823 0A00 3828 0500 ...
                  = (0,5) (10296,5) (9160,10) ...  (note period, duration)
```

Semitone-spaced chains in `tunBSFX*` indicate note/pitch tables; `tunBTUNE*` are short note
sequences (jingle/tune data, 5 records, 370–558 bytes each after decompression).

### 4.5 `drvSBDRIVER` — Creative CT-VOICE driver (observed + compare)

`drvSBDRIVER` is LZW-compressed: 3,372 bytes → **3,910 bytes**, which begin:

```
E9 92 0A  "CT-VOICE\x00"  "Creative Sound Blaster 2.0"  …
strings: "CT-VOICE", "Creative Sound Blaster 2.0",
         "Copyright (c) Creative Technology Pte Ltd., 1989-1990.  All rights reserved."
```

i.e. it is an embedded copy of Creative's **CT-VOICE.DRV** digitized-voice driver.
It is *not* the file `SBDRIVER.DRV` present on disk (2,048 bytes, begins
`EB 2A 90 0D 0A "Street Fighter - SB Driver (c) 1993."`, no "Creative" string, different
jump table). `SBDRIVER.DRV` is a custom driver shipped as a file and referenced by name in
`SF2.EXE` (`DS` offset pointing at `0x1FB22`), while `drvSBDRIVER` is the archived CT-VOICE
driver — consistent with the game options "NONE. / ROLAND LAPC-1. / SPEAKER. / SBDRIVER.DRV"
found in the `.EXE` string table.

### 4.6 `mit*` — Standard MIDI files (observed)

All 13 `mit*` records are LZW-compressed and decompress to **SMF (Standard MIDI File)**:

```
mitRFX      2092 B →  3151 B : 4D 54 68 64 00 00 00 06 00 00 00 01 00 60  "MThd" fmt 0, 1 track, 96 ppq
                               4D 54 72 6B ... 00 FF 03 08 "Untitled" FF 54 05 ... FF 58 04 ...
mitSF2AT1   24965 B → 34226 B : same "MThd" header (attract-mode music)
mitSF2TUNE1 32579 B → 42440 B : same "MThd" header (6 in-game tunes)
```

`MThd` (format 0, one track, division 0x0060) + `MTrk`, with meta events
`FF 03` (track name "Untitled"), `FF 54` (tempo), `FF 58` (time signature).
Totals: `mitSF2AT1..6` ≈ 97.8 KB, `mitSF2TUNE1..6` ≈ 151.4 KB decompressed.
("mit" is evidently the music/MIDI asset family; the credits string lists
"Music and Sound : Dave Lowe".)

### 4.7 `spr*` — 4-bpp nibble-RLE sprite images (derived; format read from the renderer)

Renderer/decoder = `FUN_1000_3DEC` (`0x13DEC`, 105 bytes), called from the object-list
drawer at `0x13D20` with `ES:SI` = record data, `DL` = palette offset:

```
0x13DFB  mov al,es:[si]        ; width  (byte 0)
0x13DFE  mov bl,es:[si+1]      ; height (byte 1)
0x13E04  mul bl                ; ax = w*h
0x13E06  shr ax,1              ; words = w*h/2   (one word = 2 pixels)
0x13E14  lodsb                 ; control byte = two 4-bit pixels (hi nibble first)
             if control == 0:    lodsb -> count ; rep stosw of 0x0000  (run of count*2 pixels)
             else: p0 = control>>4, p1 = control&0xF;
                   if p0: p0 += dl ; if p1: p1 += dl ; stosw  (byte order: p0 then p1)
```

Format: `[u8 width][u8 height]` + pixel stream where each byte = two pixels
(high nibble first); byte `0x00` = escape followed by `u8 count` = run of `count` *words*
(2×count) of zero/transparent pixels. Colour 0 stays transparent (no palette offset added);
nonzero nibbles get `+ DL` at blit time. **Validation: 3,223/3,223 `spr*` records across all
four files consume their record exactly** (including raw ones like
`sprBLANKSPR = 08 08 00 20`: 8×8, then a 0x20-word zero run = 64 transparent pixels,
and `sprFIREBAL0`, `sprCARFRM00` etc.).
Example `sprJAP2DRP3` (raw, 10×6): bytes
`0a 06 | 00 02 22 22 00 00 03 22 00 00 04 22 00 00 04 43 00 00 04 33 00 07`.

### 4.8 `lin*` — linear 8-bpp strips/screens, no header (derived)

`lin*` records are LZW-compressed but their payload is a **raw linear 8-bpp bitmap**
(one byte per pixel), dimension implied by the asset name (no header). Observed payload
sizes line up with fixed widths:

| example | payload | consistent geometry |
|---|---|---|
| linKO / linKOF | 224 | 32×7 (renders legibly as small 2-colour KO glyphs) |
| linDOUBLEKO | 1,920 | 320×6 |
| linFACE0KEN / linFACE2KEN | 1,024 each | 64×16 (or 32×32) |
| linSELECMAP | 32,768 | 256×128 |
| linSF2BLOGO | 36,864 | 256×144 |
| linEASYMESS, linDRUGSPIC, linINTROBOT, linINTROTOP | 64,000 | **320×200 full screen** |
| linBARLTOP*, linJAPTOP* (backdrop) | 11,264 | 256×44 (stage upper strip) |
| linBARLBOT*, linJAPBOT* (backdrop) | 3,584 | 256×14 (stage lower strip) |

`lin` records exist in **both** `general.sf2` (flag 0x23; UI screens/strips: KO, faces, select
map, logos, intro, ending outro strips 25–48 KB) and `backdrop.sf2` (flag 0x33; per-stage
layers). They are LZW-compressed, unlike the plain `lin` payload itself.

### 4.9 `ani*`, `frm*`, `ait*` — per-character data tables (structure partially resolved)

All 36 records are LZW-compressed; **none is a bitmap** (they only resemble sprites
superficially; the sprite decoder fails on them).

- `frm*` (12 records, 1,924–2,292 bytes decompressed): begins `00 01` then a table of u16
  pairs, then `0xFF`-terminated sections. Example `frmSAGAT`:
  ```
  0001 4400 1101 4500 2201 4500 3F01 C600 6001 8900 7501 ...
  pairs: (0x0044,0x0111) (0x0045,0x0122) (0x0045,0x013F) (0x00C6,0x0160) ...
  section offsets are monotonic; every section ends with 0xFF (frmSAGAT: 68 sections).
  Section bodies are u16 pairs, e.g. sec1: 15 04 | 00 00 | 06 04 | 00 00 | ... (frame tables)
  ```
  Interpretation (inferred): per-character "frame list" mapping frames/actions to
  per-frame data (offsets point inside the same record). The earlier revision of `frmGUILE`
  survives in holes h1/h7 (only 1–30 bytes different).
- `ait*` (12 records): u16 arrays with long zero runs and self/file-relative-looking
  constants (`aitBALROG` contains `0x047E` = 1,150 = its own size several times;
  `aitKEN` words begin `366,22,430,942,174,238,302,1134,1152,1344,1536,33,32,39,0,16,...`).
  Several values exceed the record size, so it is not a flat offset table.
  Sizes cluster by character class: 1,150 bytes for the four bosses
  (BALROG/MBISON/SAGAT/VEGA), 1,728 bytes for the eight world warriors.
  Interpretation (inferred): per-character attack/hitbox script table.
- `ani*` (12 records, **8,400 bytes each**): i16 array (4,200 words) dominated by small
  negative values (small negatives 2,629 / 4,200 words; `-43` occurs 445×; a 652-word zero
  tail at word 3,548). Example `aniKEN` first words:
  `-2891,-43,-44,-3,-2731,-42,-15,-10,-36,-45,20,-8,0,-8,-30,...`.
  Interpretation (inferred): per-animation frame delta/timing scripts.

These three tables are *indexed by name* per character
(`KEN, RYU, GUILE, EHOND, DHALI/CHUNLI, MBISO, BALRO, VEGA, SAGAT, ZANGEIF`, 12 characters),
and the per-character tables that exist only in `general.sf2` (`ait*`, `frm*`, `ani*`) plus the character
sprite banks (`sprite1/2.sf2`) should be cross-referenced for a complete character model.

---

## 5. Compressed-stream format (for reimplementation)

Not specified anywhere in the game files themselves; derived from `FUN_1000_8850`
(`0x18850`) and validated on all 3,749 records. For `flag & 3 == 3`:

```
payload[0..1] = u16 LE  uncompressed_size   (included in record size)
stream = LZW-variant bitstream, LSB-first (bit i = bit (i%8) of byte (i//8))
  code width starts at 9 bits; window read = 16-bit LE word >> bit_position
  code <= 0xFF  -> literal byte; add table entry {pos - prev_len, prev_len+1}
  code == 0x100 -> escape; read 2 more bits:
                     00 = code width += 1 (mask = mask<<1 | 0xFF)
                     01 = table reset (next code is a literal, width back to 9)
                     10/11 = end of stream
  code >  0x100 -> dictionary copy: table[code] = (start, len);
                   if code not yet filled, use (pos - prev_len, prev_len+1)
  dictionary table index = code * 4 (4-byte entries, {ptr,len}); first new code = 0x101
```

A working Python implementation (`lzw_decompress`) and the decompressed corpus
(`/tmp/opencode/sf2/dec/*.bin`) were produced during this investigation; both are
reproducible from the recipe above.

## 6. Confidence and unknowns

**High confidence (validated end-to-end):**

- INDEX.SF2 layout, flag semantics (bits 0–1 = method, bits 4–6 = file), LZW stream format.
- 0 overlaps; 7 holes / 8,956 bytes; all hole contents identified as earlier `frmGUILE`,
  `aitBLANKA`, `aitDHALISM`, `aitEHONDO`, `aitGUILE`, `aitRYU` revisions (1–30 decompressed
  bytes different), with an exact gen-A alphabetical tiling of `[0x98C76,0x9AF3B)`.
- `voc*` = Creative Voice 1.10; `pal*` = N×48-byte 6-bit VGA palette banks;
  `fnt*` = raw 8-bpp glyph arrays; `lin*` = raw 8-bpp strips (320×200 etc.);
  `spr*` = 2-byte-header 4-bpp nibble RLE (3,223/3,223 records decode);
  `mit*` = Standard MIDI files; `drvSBDRIVER` = CT-VOICE.DRV (≠ `SBDRIVER.DRV`).

**Medium confidence:**

- Exact geometry of individual `lin*` strips (widths 256/320 confirmed by sizes, but the
  per-record heights are inferred); `fntDEBUG4X4` glyph cell layout (4×4 vs 8×4 vs nibble).
- `tun*` = (period, duration) u16 pairs for the speaker/SB driver; the exact device and
  semantics of the first field (period divisor vs note number) are inferred from semitone
  ratios and the `0xFFFF` terminator.

**Unknown / unresolved:**

- Full semantics of `ani*`, `frm*`, `ait*` field layouts (the runtime code that reads them
  was not traced); `frm*`'s leading table (u16 `0x0100` + pairs of (id, section offset), ids non-monotonic) and
  `ait*`'s command encoding need the character logic routines to decode.
- Why exactly the tail was rewritten three-ish times (build/patch process unknown); the
  holes are certainly dead weight but the *reasons* (size growth on re-export, different
  LZW encoder settings) are inferred from the fact that compressed bytes differ strongly
  while decompressed bytes differ by 1–30.
- Whether anything ever reads the holes (no index entry references them; all load paths
  use the INDEX offsets/sizes).
- `drvSBDRIVER`'s role at runtime vs `SBDRIVER.DRV` (both exist; likely CT-VOICE for
  digitized voice and SBDRIVER.DRV for music/FM), and the "KIXX" / `extras.sf2` mechanism.
