# Street Fighter II (U.S. Gold / Creative Materials, DOS 1995) — Sprite Bank Format

Analysis target: `/tmp/opencode/rea/game/street-fighter-ii/` (`SF2.EXE`, `INDEX.SF2`,
`SPRITE1.SF2`, `SPRITE2.SF2`, `GENERAL.SF2`, `BACKDROP.SF2`), treated read-only.
Date: 2026-10-13. Address mapping used throughout: `file_offset = linear - 0xFFA0`
(image base `0x10000`, 0x60-byte MZ header); runtime data segment `DS = 0x12A4`
(relocated) = linear `0x22A40`.

---

## 1. Summary

* All game resources are described by `INDEX.SF2`: 8 filename slots (16 bytes each)
  followed by **3749 records of 17 bytes**: `[u8 flags][u24 LE offset][u16 LE size][11-byte name]`.
* `flags >> 4` selects the data file; `flags & 0x0F` selects the storage method:
  **0 = stored raw, 3 = LZ-compressed** (the engine's `_decMth3`, "method 3"). No other
  method values occur in the shipped index.
* **Sprite resources (`spr…`) contain exactly one frame each** (no frame counts, no
  directory inside the bank, no palette reference, no origin/hotspot fields):
  * method 0 → `[u8 width][u8 height][RLE pixel stream]`
  * method 3 → `[u16 LE decompressed_size][LZ stream]` which expands to the same
    `[u8 width][u8 height][RLE pixel stream]`
* Pixels are **4 bits per pixel, packed 2 per byte, high nibble = left (even) pixel,
  contiguous row-major, no row padding**; value 0 is transparent. Payload size is
  always exactly `ceil(width*height/2)` bytes after RLE expansion.
* The RLE is a single-escape scheme: `00 NN` emits `NN` transparent bytes (2·NN pixels);
  every other byte emits itself (2 pixels).
* Palettes are separate resources (`pal…T00N`, 48 bytes = 16 × 3-byte RGB in VGA
  6-bit DAC range), not referenced from sprite records.
* Per character the sprites come in two complete, *disjoint* artwork banks,
  `spr<CODE>B###` and `spr<CODE>T###`. They are **not** pixel-identical and **not**
  mirrored duplicates of each other; the semantic meaning of B vs T is unresolved
  (see §5).
* Verification: **all 3749 index records were decoded** (3587 method-3 records, of which
  every one decompressed to exactly its `u16` header length; 162 raw records). For the
  3223 sprite records the RLE output additionally matched `ceil(w*h/2)` exactly —
  **0 failures**.

---

## 2. Bank layout spec

### 2.1 `INDEX.SF2`

```
offset 0x00  8 × 16-byte filename slot (NUL padded): "sprite1.sf2","sprite2.sf2",
             "general.sf2","backdrop.sf2", then 4 empty slots
offset 0x80  3749 × 17-byte resource record, no trailer
```

| Offset | Size | Field | Notes |
|---|---|---|---|
| +0 | u8 | flags | `>>4` = file index (0 sprite1, 1 sprite2, 2 general, 3 backdrop); `&0x0F` = storage method (0 raw, 3 LZ) |
| +1 | u24 LE | offset | byte offset of the resource inside its data file |
| +4 | u16 LE | size | stored size (compressed size for method 3) |
| +6 | 11 bytes | name | ASCII, **space padded**, no NUL terminator; unique |

Hex example (record 0, `sprCHLIB000`):

```
03 00 00 00 42 05 73 70 72 43 48 4c 49 42 30 30 30
^^ ^^^^^^^^^ ^^^^^ ^--------- "sprCHLIB000" --------+
flag=0x03 offset=0x000000 size=0x0542(1346)   (file 0 = SPRITE1.SF2, method 3)
```

Per-file record counts and byte-chains: `SPRITE1.SF2` 1632 records / 1 297 466 B,
`SPRITE2.SF2` 1422 / 1 302 349 B, `BACKDROP.SF2` 342 / 774 204 B — each recorded
`offset` chain is perfectly contiguous and ends exactly at the file size.
`GENERAL.SF2` (353 records / 643 620 B) has 7 small holes (838–2584 B) not referenced
by the index, but its chain also ends at the file size.

### 2.2 Sprite resource (method 0 = raw)

```
[u8 width][u8 height][RLE pixels…]      ; stored size = 2 + rle_len
```

Hex example — `sprHITSTAR0` (GENERAL.SF2 @ 535698, size 35, 16×16 hit star):

```
10 10 00 23 01 00 07 11 10 00 05 01 10 11 00 05 11 00 01 01 10 00 04 01 10 11 00 06 11 10 00 06 01 00 2c
w=16 h=16 |<-------------------------- RLE stream ------------------------->
```

Expansion: `00 23`→35·0x00, `01`, `00 07`→7·0, `11 10`, `00 05`→5·0, `01 10 11`,
`00 05`→5·0, `11`, `00 01`→1·0, `01 10`, `00 04`→4·0, `01 10 11`, `00 06`→6·0,
`11 10`, `00 06`→6·0, `01`, `00 2c`→44·0 = **128 bytes = 16·16/2** ✔ (renders as a
symmetric star).

Other raw examples: `sprCHLIT131` (SPRITE1 @ 193393, 221 B, 26×17), `sprVEGAT013`
(SPRITE2 @ 828504, 618 B, 76×32), `sprRYUTB014`/`016` = `02 01 00 01` (2×1 blank
place-holder frames), `sprBLANKSPR` = `08 08 00 20` (8×8 fully transparent).

### 2.3 Sprite resource (method 3 = LZ)

```
[u16 LE decompressed_size][_decMth3 LZ stream]   ; stored size = 2 + compressed
```

Hex example — `sprBLANB000` (SPRITE2.SF2 @ 0, size 1315, 112×60):

```
a1 07 | 70 78 00 58 81 00 00 89 01 00 14 cc 00 90 60 06 0c 00 …
u16 = 0x07A1 = 1953   LZ stream (1313 bytes)
```
Decoded (1953 bytes) head:
```
70 3c 00 2b 08 00 24 03 00 0a 33 00 09 33 30 00 1f b0 00 01 …
w=0x70=112 h=0x3c=60 | RLE stream…
```
RLE expands to 3360 B = 112·60/2 ✔.

`sprCHLIB000` (SPRITE1.SF2 @ 0, size 1346):

```
index : 03 00 00 00 42 05 "sprCHLIB000"     (offset 0, size 1346)
data  : ae 07 70 78 00 58 11 00 00 09 02 00 14 10 01 90 80 08 10 00 1f a0 …
        u16 = 0x07AE = 1966
decoded (1966 B) head: 70 3c 00 2b 01 00 24 04 00 0a 44 00 09 44 40 00 1f 50 00 01 …
        w=112 h=60, RLE→3360 B ✔
```

### 2.4 RLE pixel stream

| Byte sequence | Meaning |
|---|---|
| `00 NN` | emit `NN` zero bytes (2·NN transparent pixels) |
| any byte `b != 00` | emit `b` literally (2 pixels) |

* Byte value 0x00 never appears as literal pixel data (it is always an escape).
* Output length before packing is always exactly `ceil(width*height/2)`.
* Decoded size prefix (method 3) counts this whole `[w][h][RLE]` blob, not just pixels.

### 2.5 Pixel encoding

* 4 bpp, 2 pixels per byte, **high nibble = first (left/even) pixel**, low nibble = next.
* No row padding; rows are byte-aligned only because every shipped width is even.
  Row stride in bytes = `width/2`, confirmed empirically per sprite by minimising
  vertical neighbour differences over all candidate strides (best stride == `w` pixels)
  and by rendering (`sprPLANE000` 32×32 renders as a recognisable aeroplane;
  `sprHITSTAR0` as a star; both from GENERAL.SF2).
* Palette index 0 = transparent (the RLE escape only ever skips index 0).
* No per-sprite palette reference; palettes are global per character and supplied
  separately (`pal<CODE>T001` / `pal<CODE>T002`).

### 2.6 `_decMth3` LZ stream (method 3)

Recovered from `SF2.EXE` (see §3 evidence) — bit reader is plain **LSB-first over the
byte stream**, starting at bit 0.

```
state: bitpos=0; code_width=9; table = {}; BX = 0x404; BP = 1
code = read(code_width)                 # first code is always emitted as a literal byte
emit(code)
loop:
  code = read(code_width)
  if code < 0x100:                      # literal
      emit(code)
      table[BX] = (out_len-1 - BP, BP+1)
      BP = 1; BX += 4
  elif code == 0x100:                   # escape
      c = read(2)
      if c == 0: code_width += 1; continue           # widen code (max 16)
      elif c == 1: code_width = 9; BX = 0x404; BP = 1
                   emit(read(9))                     # reset + literal token
      else: stop                                     # c == 2 or 3 → end of stream
  else:                                 # back-reference, slot = code*4
      if slot < BX:
          (off, ln) = table[slot]
          table[BX] = (out_len - BP, BP+1)
          BP = ln
          copy ln bytes from out[off]                # absolute output offset
          BX += 4
      else:
          off = out_len - BP                          # "new slot": repeat last match +1
          BP += 1
          table[BX] = (off, BP)
          copy BP bytes from out[off]
          BX += 4
```

Notes:
* The table holds up to `(0x800-0x404)/4 = 255` entries of `(absolute output offset,
  length)`; slots are allocated sequentially and only become valid references once
  filled. Resets (`c==1`) clear the table pointer back to 0x404.
* The `else` branch is a self-replicating copy (distance == last match's span), i.e. the
  encoder extends the previous match by one byte.
* Argument contract seen in the wrapper `FUN_1000_87c7`: the caller rejects resources
  whose compressed size ≥ 0x8000 ("Compressed data > 32k"), reads the `u16` block size,
  checks the final output length against it ("Decompressed to wrong size"), and dispatches
  only when the method byte is 3 ("Unknown compression method").
* The same method-3 codec is used for `spr`, `pal`, `lin`, `ani`, `frm`, `ait`, `fnt`,
  `mit`, `tun`, `drv` resources; all 3587 method-3 records in all four data files
  decompressed to exactly their declared length.

### 2.7 Palette resources

`pal<CODE>T001` / `pal<CODE>T002`, 48 bytes, method 0, e.g. `palCHLIT001`
(SPRITE1.SF2 @ 1296413):

```
00 14 00 | 3c 38 28 | 3c 30 24 | 38 24 1c | 30 20 18 | 28 18 14 | 20 14 00 |
1c 10 00 | 14 00 00 | 00 00 00 | 24 00 14 | 2c 14 20 | 34 1c 28 | 38 28 30 |
3c 3c 3c | 3c 3c 00
```
= 16 RGB triples, values 0–63 → VGA DAC 6-bit values. `…T001` and `…T002` share their
first entries and differ in the tail (alternate player colour scheme). Note that only the
`T` palette variant exists per character; larger `pal*` resources (169–478 B) also exist
for non-character purposes and were not analysed.

### 2.8 Naming scheme

`<prefix 3><name 4-5><index 3>`, 11 characters:

| Prefix | Family |
|---|---|
| `spr` | sprite frame (one record per frame) |
| `pal` | palette (48 B for characters) |
| `fnt` | font, `lin` line/effect graphics, `mit` misc |
| `ani` | animation program, `frm` frame table, `ait` AI table (per character) |
| `voc` | digitised sample, `tun` music, `drv` sound driver |

Character codes (confirmed against display-name table at EXE file offset 0xEDBC
pointers → 0xEE03…): `CHLI`=CHUN LI, `RYUT`=RYU, `KENN`=KEN, `DHAL`=DHALSIM,
`BLAN`=BLANKA, `GUIL`=GUILE, `HOND`=E HONDA, `ZANG`=ZANGIEF, `BALR`=BALROG,
`BISO`=M BISON, `SAGE`=SAGAT, `VEGA`=VEGA. Accessory families use different 5th letters
(`sprPLANE…`, `sprBON3…`, `sprCARB…`, `fntCOLR…`, `sprSHAD…`), so the 5th character is
not always a "variant" flag — only for character sprites is it consistently `B`/`T`.

---

## 3. Evidence

### 3.1 Executable cross-check (code)

| Item | Linear | File offset | Content |
|---|---|---|---|
| `_decHandle` dispatcher | 0x187B6 | 0x8816 | `cmp al,3` / else fatal `mov dx,0B87h` = `_decHandle: Unknown compression method.` |
| `_decMth3` wrapper `FUN_1000_87c7` | 0x187C7 | 0x8827 | size<0x8000 check, reads `u16` block size, calls decoder, compares length |
| `_decMth3` core `FUN_1000_8850` | 0x18850 | 0x88B0 | bit-reader + dictionary loop, table at `DS:0x404`, 4-byte entries |
| `_decMth3: Can't read block size.` | 0x23670 | 0x136D0 | DS offset 0xBD0 |
| `_decMth3: Decompressed to wrong size.` | 0x23685 | 0x136E5 | DS offset 0xC45 |
| `_sprInit: Can't alloc sprite decomp buffer.` | 0x23163 | 0x131C3 | DS offset 0x723 → sprites are decompressed into a RAM buffer |
| `_spriConvertLR: Code not written.` | 0x2326E | 0x132CE | L/R sprite conversion routine is an unimplemented stub |

The bit reader was verified two ways: the disassembly trick
(`LODSW SI / XOR AL,[SI] / AND AL,CH / XOR AL,[SI] / ROR AX,CL / AND AH,DH`) was emulated
instruction-by-instruction and compared against a plain LSB-first bit reader for all 64
start positions × 8 bit offsets × 8 code widths — **identical results**, so the simple
bit-stream model is exact.

`mov ax,12A4h / mov ds,ax` appears in the startup path; `0x12A4*16 + 0x10000 = 0x22A40`,
which is exactly where the error-message blob begins (`…Error header file. (c) JEF '93$`),
confirming the DS base used to resolve all `dx = offset` message references.

### 3.2 Validation runs (python3, read-only)

* Sprite-only pass: 3223/3223 records OK — for method 3 `len(decode) == u16` **and**
  `len(RLE(output[2:])) == ceil(w*h/2)`; for method 0 `len(RLE(output[2:])) == ceil(w*h/2)`.
* Full-index pass: 3749 records — all 3587 method-3 records decode to exactly the
  declared length (method-3 families: `spr` 3199, `lin` 327, `pal` 55, `tun` 9, `mit` 13,
  `ait` 12, `ani` 12, `frm` 12, `fnt` 6, `drv` 1; method-0/raw: 162 records incl. all 30
  `voc` samples); 0 failures.
* Chain check: for `SPRITE1.SF2`, `SPRITE2.SF2`, `BACKDROP.SF2` the sorted
  `offset[i+1] == offset[i] + size[i]` chain has 0 gaps and ends at the file size;
  `GENERAL.SF2` has 7 unreferenced holes totalling ~9 KB but also ends at file size.
* Decoded sprite inventory (all 3223 sprites): width range 2–222, height range 1–145;
  compressed/stored ratio ≈ 1.2×; sprite-1/2 banks are ~1.3 MB each.
* Every character's frame `…B000` decodes to 112×60 (u16 = 1966, or 1953 for `KENN`/`BLAN`)
  and the first stored bytes are `ae 07 70 78 00 58` — a strong sign of an identical
  leading transparent run plus a shared first-pose template.
* Render spot-checks (16 luminance levels from `palCHLIT001`/`palRYUTT001`):
  `sprHITSTAR0` = symmetric star, `sprPLANE000` = aircraft, `sprJAP2DRP2` = droplet,
  `sprCARBIT03` = wedge. These fix the nibble order and the row-major stride.

### 3.3 Raw values for the named banks requested

| Record | File | Offset | Size | flags | Method | Stored header | Decoded (w×h) |
|---|---|---|---|---|---|---|---|
| `sprCHLIB000` | SPRITE1 | 0 | 1346 | 0x03 | 3 | `ae 07` = 1966 | 112×60 (RLE→3360) |
| `sprRYUTB000` | SPRITE1 | 1 071 590 | 1346 | 0x03 | 3 | `ae 07` = 1966 | 112×60 (RLE→3360) |
| `sprKENNB000` | SPRITE2 | 1 088 588 | 1321 | 0x13 | 3 | `a1 07` = 1953 | 112×60 (RLE→3360) |
| `sprBLANB000` | SPRITE2 | 0 | 1315 | 0x13 | 3 | `a1 07` = 1953 | 112×60 (RLE→3360) |
| `sprGUILB000` | SPRITE1 | 200 045 | 1345 | 0x03 | 3 | `ae 07` = 1966 | 112×60 (RLE→3360) |
| `sprCHLIT131` | SPRITE1 | 193 393 | 221 | 0x00 | 0 | — | 26×17 |
| `sprVEGAT013` | SPRITE2 | 828 504 | 618 | 0x10 | 0 | — | 76×32 |
| `sprRYUTT025` | SPRITE1 | 1 186 874 | 616 | 0x00 | 0 | — | 44×32 |
| `sprRYUTB014` / `016` | SPRITE1 | 1 081 953 / 1 082 544 | 4 | 0x00 | 0 | — | 2×1 blank |
| `palCHLIT001` | SPRITE1 | 1 296 413 | 48 | 0x00 | 0 | — | 16 colours |

(The index contains 24 method-0 sprite records in total — the ones above plus blank/effect
sprites such as `sprBLANKSPR`, `sprHITSTAR0`, `sprCARBIT03`, `sprPLANE000/001`; all other
sprite records are method 3.)

### 3.4 B vs T sprite banks

Counts per character (`spr<CODE>B***` / `spr<CODE>T***`):

| Code | B frames | T frames | notes |
|---|---|---|---|
| `RYUT` | 156 | 161 | shared dims: 23; pixel-identical frames: 0 |
| `KENN` | 153 | 148 | 1 identical frame (blank place-holder) |
| `GUIL` | 147 | 166 | |
| `BLAN` | 144 | 144 | 0 identical frames |
| `ZANG` | 148 | 150 | |
| `CHLI` | 131 | 136 | 2 identical (blank) frames |
| `DHAL` | 124 | 132 | |
| `HOND` | 122 | 123 | |
| `VEGA` | 103 | 118 | |
| `BISO` | 97 | 113 | |
| `BALR` | 81 | 97 | |
| `SAGE` | 63 | 73 | |
| `SHAD` | — | — | `sprSHAD0000` special shadow sprite |

Findings: the two sets are essentially disjoint artwork (0 mirrored matches; only 1–2
pixel-identical frames per character, all blank place-holders); dimension multisets
overlap only slightly (e.g. 7 shared dims out of ~120 for CHLI); bounding-box aspect
ratios differ hugely, so T is neither a scaled nor a mirrored copy of B. Both sets are
full animation banks of comparable size, so they are two hand-authored renditions of the
same character. Palette resources exist only in the `T` form (two per character), so the
`T` in palette names and the `B`/`T` sprite banks are separate naming axes.

---

## 4. Confidence

| Statement | Confidence | Basis |
|---|---|---|
| Index 17-byte record layout, flags→file/method | **High** | Byte-exact parse + contiguity chain matches all file sizes |
| Method 0 sprite layout `[w][h][RLE]` | **High** | 24 records; every RLE expansion equals `ceil(w*h/2)`; visual renders |
| Method 3 = `[u16 size][LZ]` → same `[w][h][RLE]` | **High** | All 3199 sprite records + 388 other records decode to exact declared length |
| RLE semantics (`00 NN` = NN transparent bytes) | **High** | Length identity for all 3223 sprites; unambiguous on the star/plane renders |
| Pixel packing 4bpp, high nibble first, row-major, no padding | **High** | Renders of GENERAL.SF2 sprites + stride minimisation + nibble-order comparison |
| `_decMth3` algorithm (bit widths, escape codes, dictionary) | **High** | Recovered from disassembly; validated against all 3646 real streams (3 117 173 tokens, 8 710 code-width increases, 54 resets, 3646 end tokens, max code width used = 12) |
| Palette = 16×RGB 6-bit, two per character | **High** for the 48-byte character palettes | Sizes and value ranges; `T001`/`T002` differ only in tail entries |
| Sprite name scheme & character codes | **High** | Index names + EXE display-name pointer table |
| B/T banks = two distinct complete artwork sets | **High** | Hashing/mirroring/dimension analysis over all 12 characters |
| What B and T actually *mean* | **Low / unknown** | see §5 |

---

## 5. Unknowns

1. **Semantics of `B` vs `T` sprite banks** — two full, disjoint, non-mirrored artwork
   sets per character. No mirror, rescale or duplicate relation was found. Resolving this
   needs the per-character animation/frame resources (`ani<CHAR>`, `frm<CHAR>`), whose
   internal schema is only partly understood (see below). Note the engine contains
   `_spriConvertLR: Code not written.`, i.e. no runtime left/right conversion.
2. **`frm<CHAR>` frame table schema** — method-3 compressed; entries look like 4-byte
   records `[u16 offset/pointer][u8 x][u8 y]` whose `u16` increments by 5–101 between
   entries and is not a multiple-of-17 name/index offset. Not decoded.
3. **`ani<CHAR>` animation program schema** — method-3 compressed; decoded bytes look like
   signed 16-bit deltas (`ff7b ffdd ffc0 …`), not a plain frame list. Not decoded.
4. **Sprite origin/hotspot** — sprite records carry only `w`,`h`; the draw origin must come
   from `frm`/`ani` data (unresolved), not from the bank.
5. **`flag&0x0F` values other than 0/3** — none occur in the shipped index; the engine's
   dispatcher only accepts 3 (and raw records bypass it), so extra methods may exist
   unseen (`_decInit: No space for dictionary` implies other methods were planned).
6. **`GENERAL.SF2` holes** (7 gaps, 838–2584 B) — unreferenced by the index; maybe
   deleted/relocated resources.
7. **Non-character `pal*` records larger than 48 B** (169–478 B) — structure not analysed
   (likely 256-colour palettes or palette+image blobs).
8. **Whether method-3 streams are byte-aligned/padded per record** — decoded sizes always
   matched the `u16` header, so no trailing padding was needed; end-of-stream token
   (`c ∈ {2,3}`) is the only terminator observed.
