# Character gameplay tables: `frm*`, `ani*`, `ait*` — byte-level specification

Target: `SF2.EXE` (MS-DOS, 1995 U.S. Gold / Creative Materials port of Street Fighter II).
Address convention used below: **linear = file_offset + 0xFFA0**, **image/CS base = 0x10000**,
**DS base = 0x1AA40** (a DS-relative offset `d` is at linear `0x1AA40+d`).
All multi-byte values are **little-endian** unless stated otherwise.
Decoded assets: `/tmp/opencode/sf2/dec/{frm,ani,ait}<CHAR>.bin` (LZW already removed).

This document was written by mapping the 36 assets, disassembling the readers, and
cross-checking the data against the code (function addresses are quoted throughout).

---

## 1. Summary

The three tables are **per-character assets stored in `GENERAL.SF2`** (file slot 2, flags `0x23` = LZW).
`FUN_1000_0944` (0x10944) is the preload: it `acquire()`s the character's `ani` and `frm`
handles, then walks the `frm` frame lists to **load every referenced `spr<CODE>T###`/`spr<CODE>B###`
sprite** (0x10978–0x109A7). `ait` is fetched on demand by the AI code.

Runtime chain (all links verified in code):

```
action code  (player[0x3d], 0..0x4B)          input/state machine picks it
      │  ait: sparse action→script table  (read at 0x17BE1)   -- or a direct decision code
      ▼
script id    (0..63)  = byte offset player[0x39]/4
      │  frm: 64-entry script header table (4 B each)
      ▼
script header {u16 frame_list_offset, u8 flags, u8 event_ani}
      │  frm: 0xFF-terminated list of 4-byte frames, advanced by player[0x32] (+4/frame)
      ▼
frame {u8 ani_index, i8 dx, i8 dy, u8 flags}   (player[0x35] = ani_index)
      │  ani: 200 × 42-byte records indexed by ani_index*42
      ▼
record { T-handle,x,y,xflip , B-handle,x,y,xflip , 4 hitboxes , 4+4 box-type nibbles }
      │  handle → INDEX.SF2 record index = 0xFFFF - handle
      ▼
spr<CODE>T### / spr<CODE>B### + pal<CODE>T001/T002   (sprite + player palette)
```

`T` and `B` are the **two sprite layers of one pose** (each is a complete, disjoint artwork bank).
They are not two facings and not alternative frames: the fighter process spawns **two layer
processes** at init (`0x109C1` with BP=0 and `0x109C8` with BP=8, spawned at 0x1753B/0x1754B),
and each layer draws its own half of the same animation record.

### Character ids, names and record indices

`char_id` (also `char_id*2` as a byte in `player[0x37]`) is used to index four DS word tables.
The order is **not** alphabetical; it is fixed as:

| id | char | sprite code | `ani` idx/handle | `frm` idx/handle | `ait` idx/handle |
|---:|---|---|---:|---:|---:|
| 0 | KEN | `KENN` | 3713 / 0xF17E | 3731 / 0xF16C | 3743 / 0xF160 |
| 1 | GUILE | `GUIL` | 3714 / 0xF17D | 3730 / 0xF16D | 3742 / 0xF161 |
| 2 | E.HONDA | `HOND` | 3715 / 0xF17C | 3729 / 0xF16E | 3741 / 0xF162 |
| 3 | RYU | `RYUT` | 3716 / 0xF17B | 3733 / 0xF16A | 3745 / 0xF15E |
| 4 | ZANGIEF | `ZANG` | 3717 / 0xF17A | 3736 / 0xF167 | 3748 / 0xF15B |
| 5 | CHUN LI | `CHLI` | 3718 / 0xF179 | 3727 / 0xF170 | 3739 / 0xF164 |
| 6 | DHALSIM | `DHAL` | 3719 / 0xF178 | 3728 / 0xF16F | 3740 / 0xF163 |
| 7 | BLANKA | `BLAN` | 3720 / 0xF177 | 3726 / 0xF171 | 3738 / 0xF165 |
| 8 | BALROG (boxer) | `BALR` | 3721 / 0xF176 | 3725 / 0xF172 | 3737 / 0xF166 |
| 9 | VEGA (claw) | `VEGA` | 3722 / 0xF175 | 3735 / 0xF168 | 3747 / 0xF15C |
| 10 | SAGAT | `SAGE` | 3723 / 0xF174 | 3734 / 0xF169 | 3746 / 0xF15D |
| 11 | M.BISON (dictator) | `BISO` | 3724 / 0xF173 | 3732 / 0xF16B | 3744 / 0xF15F |

(Handles are negative: `record_index = 0xFFFF - u16_handle`. In `INDEX.SF2` the `frm*` records sit
at 3725–3736 in *alphabetical* order, so the per-character order must come from the DS tables, not
from the file order.)

DS tables that carry this order:

| DS offset | contents | reader |
|---|---|---|
| `DS:0x0060` | 12 × `u16` `ait` handles, indexed `char_id*2` | 0x106DF, 0x108E1, 0x10956, 0x17BF5 |
| `DS:0x0266` | 12 × `u16` `ani` handles | 0x10964, 0x10A07, 0x12752, 0x12810 |
| `DS:0x027E` | 12 × `u16` `frm` handles | 0x1096E, 0x10ABE, 0x10C0B, 0x10C3D |
| `DS:0x3C11` / `DS:0x3C29` | 12 × `u16` palette handles `pal<CODE>T001` / `pal<CODE>T002` | 0x1758C–0x175A2 (byte index `char_id*2`) |
| `DS:0x3BF9` | 12 × `u16` **per-character handler offsets** (segment 0x1000) | `call ds:[bp+0x3BF9]`, BP=`char_id*2` |

`DS:0x3BF9` values: 0x6993 KEN/RYU, 0x6D1F GUILE, 0x707B HONDA, 0x70E8 ZANGIEF/BALROG/BISON,
0x7016 CHUN-LI, 0x6AF3 DHALSIM, 0x6E63 BLANKA, 0x70B3 VEGA, 0x6EA4 SAGAT. Each is a stub of the form
`call cs:[bx+T]; ret` (or a small handler) that dispatches on an **input mode** passed in BX
(0/2/4/6/8, table at stub+6): mode 0 = action-start hook (0x17BC2), 2 = script-change hook
(0x10C75), 4 = list-end/tick hook (0x10D57), 6 = per-frame hook (0x110C1, AH=`player[0x21]`),
8 = 0x177F5. Several modes are plain `ret` for most characters.

Related: `DS:0x3BEF` is a small `{u8 action, u8 pad, u16 handler}` table terminated by `0xFF`
(keys `0x40`,`0x41` → handler 0x108FA; dispatcher at 0x176E0 = `FUN_1000_76e0`), and
`DS:0x1618` is the hitbox size table (16 × `(u8 w, u8 h)`; type 0 = 16×16, 1 = 32×16, 2 = 48×16,
3 = 64×16, 4 = 16×32, 5 = 16×48, 6 = 16×64, 7 = 32×32, 8 = 48×32, 9 = 64×32, 10 = 32×48,
11 = 32×64, 12 = 48×48, 13 = 64×48, 14 = 48×64, 15 = 64×32).

---

## 2. `frm<CHAR>` — per-character script ("frame") table

* Decompressed size 1872–2292 bytes (`frmBALROG` 1872, `frmSAGAT` 1924, `frmKEN` 2288, …).
* Usage: `FUN_1000_09C1/09C8` (layers), `FUN_1000_10AB1` (advance frame), `FUN_1000_10B7F`
  (stage frame), `FUN_1000_10C32`/`10BFD` (read header flags), `FUN_1000_10C5D` (commit script),
  `FUN_1000_0944` (preload). All read with `ES` = the loaded `frm` buffer.

### 2.1 Layout

```
+0x000: 64 × 4-byte script headers:
          +0  u16  frame_list_offset   (byte offset of the 0xFF-terminated frame list)
          +2  u8   flags
          +3  u8   event_ani            (ANI record index that triggers a hit/impact event)
+0x100: frame lists (each 0xFF-terminated), usually tightly packed.
```

* Exactly **64 scripts** (0..63). The first list always starts at `0x100`; the preload loop
  (0x10978: `mov cx,0x40 … add di,4`) walks all 64 headers and every frame entry.
* A script header offset is `script_id*4`; `FUN_1000_10C5D` stores the caller's AX (=`player[0x39]`)
  into `player[0x20]`, and every later reader uses `DI = player[0x20]`
  (`0x10AB1: mov di,[si+0x20]; mov bx,es:[di]`, `0x10C32: mov di,[si+0x20]; mov al,es:[di+2]`).

### 2.2 Flags byte (+2)

Used only as bits (0x10B20–0x10BC0, 0x10C26):

| bit | effect (evidence) |
|---|---|
| 0x04 | at list end: restart list **and** set `obj[0x38]|=0x10` (loop flag) — 0x10B20–0x10B2B |
| 0x08 | at list end: restart list silently — 0x10B2E–0x10B35 |
| 0x10 | at list end: keep last frame (`bx -= 4`) and set `obj[0x38]|=0x10` — 0x10B38–0x10B45 (if none of 0x04/0x08/0x10 is set the position stays on the `0xFF` slot) |
| 0x20 | mirrors into owner flags: `player[0xa]` bit 5 cleared, set if bit present — 0x10BA3–0x10BB6; also `FUN_1000_10C26` returns 0xFFFF when set |
| 0x80 | `obj[0x38] |= 0x10` — 0x10BB8–0x10BC0 |
| 0x01,0x02,0x40 | not referenced by the traced readers (unknown) |

Observed flag values: `0x24, 0x25, 0x51, 0x66, 0x89, 0x90, 0xC6, 0x01, 0x02, 0x04, 0x05, 0x06, 0x08, 0x11, 0x12, 0x13, 0x10, 0x17, 0x19, 0x36, 0x3A, 0x51, 0x6B, 0x70, 0x83, 0x84, 0x91`.
Rough grouping (from the script semantics, medium confidence): `0x2x` = looping/idle-type sets,
`0x6x`/`0x4x` = attack sets, `0x8x`/`0x9x`/`0xCx` = one-shot/hold sets.

### 2.3 Frame list and frame entry

```
frame entry = 4 bytes:
  +0  u8  ani_index      0..199 ; 0xFF = end of list
  +1  i8  dx             signed X delta (added to the layer's X accumulator every frame)
  +2  i8  dy             signed Y delta (added to the layer's Y accumulator every frame)
  +3  u8  flags          bit 0x04 -> XOR into obj[0x1c] bit 2 (mirror X select)
                         bit 0x08 -> copy into obj[0x1c] bit 3
```

Evidence: frame staging in `FUN_1000_10B7F` (0x10BC3: `al=es:[bx+1]→CX`, `es:[bx+2]→DX`,
`ah=es:[bx+3]`), end-of-list test `cmp byte es:[bx],0xFF` at 0x10AE5/0x10B1B, and the delta
application in the per-frame layer update:

```
0x10e67: call 0x10ab1                 ; stage next frame (returns dx/dy in CX/DX)
0x10e6a: add word ptr [si + 0x24], cx ; X accumulator
0x10e6d: add word ptr [si + 0x28], dx ; Y accumulator
```

Frame advance: `player[0x32]` holds the **byte offset inside the list**; it is incremented by 4
per frame (0x109F4 `add byte ptr [si+0x32],4`, 0x10B63 `sub bx,es:[di]; mov [si+0x32],bl`).

The list terminator is a **frame whose byte 0 is 0xFF** (the remaining 3 bytes of that slot are
ignored and usually belong to the next list).

---

## 3. `ani<CHAR>` — animation records (geometry + sprite layers + hitboxes)

* Decompressed size **exactly 8400 bytes for all 12 characters** = **200 records × 42 bytes**.
  The reader enforces the bound: `0x109E1: cmp al,0xC8; jae fatal` followed by
  `0x109ED: mov cl,0x2A; mul cl; mov di,ax` (index = `ani_index*42`).
* Readers: `FUN_1000_09C1/09C8` (0x109CF, sprite layers), `FUN_1000_12712` (0x12712, hitboxes),
  `FUN_1000_0944` (preload of T/B sprites from list entries).

### 3.1 Record layout (42 bytes)

```
+0x00  u16  T_handle    spr<CODE>T###   (0xF33A = sprBLANKSPR = "draw nothing")
+0x02  i16  T_x         X offset when obj[0x1c] bit 2 == 0
+0x04  i16  T_y         Y offset
+0x06  i16  T_x_flip    X offset when obj[0x1c] bit 2 == 1
+0x08  u16  B_handle    spr<CODE>B###   (0xF33A = blank)
+0x0A  i16  B_x
+0x0C  i16  B_y
+0x0E  i16  B_x_flip
+0x10  4 × 6 bytes: hitbox candidate {i16 x_a, i16 y, i16 x_b}; 0xFFF8 (-8) = empty slot
+0x28  u8   box types, 4 nibbles: (b>>4, b&0xF) for boxes 0,1
+0x29  u8   box types, 4 nibbles: (b>>4, b&0xF) for boxes 2,3
```

`T`/`B` are the two layers of the pose (see §1). X selection: `0x10A54 mov ax,es:[bp+di+2]`
(`bp=0` → `T_x`) and `0x10A5F mov ax,es:[bp+di+6]` when `obj[0x1c]&4` (`T_x_flip`); with `bp=8`
the same two instructions read `B_x`/`B_x_flip` at +0x0A/+0x0E.

Y derivation (0x10A10–0x10A51), `h()` = sprite header height:

```
bp=0 (T layer):  Y = T_y - h(T) + 0x10
bp=8 (B layer):  Y = T_y - B_y - h(B) + 0x10
X:               X = (obj[0x1c]&4) ? *x_flip : *x
```

(the sprite height comes from `FUN_1000_13EF9`: byte0 = width, byte1 = height of the `spr` record).

### 3.2 Hitboxes (built by `FUN_1000_12712`, called from 0x12143/0x1214C)

```
for k in 0..3:
  word0 = rec[0x10+6k]  # x (facing dir 0)
  word1 = rec[0x12+6k]  # y
  word2 = rec[0x14+6k]  # x (facing dir 1)
  if any of the three == -8: no box
  type = nibble k            -> (w,h) = DS:0x1618[type*2]
  left   = (facing? word2 : word0) + playerX - 4
  right  = left + w + 4
  top    = word1 + playerY - 4
  bottom = top + h + 4
  emit {u8 valid=0xFF, u8 type, i16 left, i16 top, i16 right, i16 bottom}   (10 bytes)
```

The nibble order is `[b28>>4, b28&0xF, b29>>4, b29&0xF]` (0x1276C–0x1279B).
The 10-byte boxes are consumed by an AABB overlap test over the 4 boxes in `FUN_1000_1281E`
(0x1282D–0x12847 test `[di]==0xFF`, `[di+2..di+8]` = left/top/right/bottom). Box type numbers
(0..15) index `DS:0x1618`, which quantises sizes to a 16-pixel grid (type 0 = 16×16).

---

## 4. `ait<CHAR>` — AI decision tables

* Sizes: **1728 bytes** for the 8 world warriors, **1150 bytes** for the four bosses
  (BALROG, MBISON, SAGAT, VEGA).
* Readers: code at 0x105B1/0x1068D/0x10721 (0x105EF, 0x106E8, 0x10721) and `FUN_1000_17BE1`
  (0x17BE1); all use `ES` = `ait` buffer.

### 4.1 Header: 11 × `u16` section offsets (`base[k]`)

Offsets are absolute within the record and are monotonic when sorted; the first data section
always starts at offset **22** (= after the 11-word header), which pins the header length.

| k | warriors (KEN,RYU,BLANKA,CHUNLI,DHALISM,EHONDO,GUILE,ZANGEIF) | bosses (BALROG,MBISON,SAGAT,VEGA) |
|---:|---:|---:|
| 0 | 366 | 382 |
| 1 | 22 | 22 |
| 2 | 430 | 446 |
| 3 | 942 | 958 |
| 4 | 174 | 174 |
| 5 | 238 | 246 |
| 6 | 302 | 318 |
| 7 | 1134 | 1150 |
| 8 | 1152 | 1150 |
| 9 | 1344 | 1150 |
| 10 | 1536 | 1150 |

Bosses have the last sections empty (`1150` = EOF). Sections in ascending order and their sizes
(warriors): `22→174` (152), `174→238` (64), `238→302` (64), `302→366` (64), `366→430` (64),
`430→942` (512), `942→1134` (192), `1134→1152` (18), `1152→1344` (192), `1344→1536` (192),
`1536→1728` (192).

### 4.2 Verified section readers

| base | code | index formula | use |
|---|---|---|---|
| `hdr[1]` (22) | 0x17BE1 | sparse: probe `u16` at `base + action*2`, step down by 2 while the pair is `0x0000`; result `CH` = even byte (script id), `CL` = odd byte (param). If `action >= 0x40` → `CL = 0` | **action → script id** map (feeds `player[0x39] = CH*4`) |
| `hdr[3]` (942) | 0x105EF | `24*distance_bucket + 8*(player[0xa]&3) + rand(0..7)` | AI action pick per distance bucket (returns a byte action code) |
| `hdr[0]` (366) | 0x106E8 | `+ player[0x3d]` (action) | per-action byte table, value tested `!= 0` |
| `hdr[2]` (430) | 0x10721 | `16*x + bucket + rand&8` | attack-selection byte table |

`distance_bucket` (0..7) is itself computed from `|playerX - opponentX|` against an 8-entry
threshold table at `DS:0x0050` (`FUN_1000_04FB` at 0x104FB: `(0x30,0x40,0x50,0x60,0x70,0x90,0xC0,0x8000)`)
and stored at `DS:0x4E` (word) / `DS:0x265` (byte).

The sparse `action→script` table (base 22) is an interleaved pair of byte arrays: even bytes =
script ids, odd bytes = parameters (nonzero only for a couple of entries per character, e.g.
`action 0x2C`→script `0x28` param `0x14`, `action 0x2D`→script `0x29` param `0x15` for KEN/RYU;
`0x2C` VM param `0x80` for VEGA; `0x2C` param `0x14` for M.BISON). The `0x40..0x4A` tail is
identical for all characters (`04,04,03,05,05,06,07,06,3F,0A,0B` with params `0x80..0x83`).
Because the reader scans **down** to the last nonzero pair, an entry applies to all actions from
its index up to the next entry. Nonzero examples (KEN/RYU, identical):

```
action: 00 01 02 04 05 06 08 09 0A 0B 10 11 12 13 18 19 1A 20 21 22 34 35 36 40 41 42 43 44 45 46 47 48 49 4A
script: 21 20 27 10 11 08 23 25 26 24 16 15 14 13 2A 2A 2B 1E 17 2A 12 09 22 04 04 03 05 05 06 07 06 3F 0A 0B
param:  00 ...                                        (0x14 at 0x2C, 0x15 at 0x2D, 0x80..0x83 at 0x40..0x44)
```

The remaining header words (4..10) point at further 64-byte/192-byte data sections whose readers
were not located in the main segment; they contain per-script / per-distance tuning bytes.

---

## 5. Worked example — KEN, action code `0x0B` (script 36)

All bytes below are from `dec/frmKEN.bin` / `dec/aniKEN.bin` / `dec/aitKEN.bin`.

**Step 1 — action → script id** (`FUN_1000_17BE1`, base = `ait[2] = 22`):

```
aitKEN[0x2C..0x30] = 24 00 00 00           ; action 0x0B entry = (script 0x24, param 0)
action 0x0B → scan stops at index 0x0B → CH = 0x24
player[0x39] = CH*4 = 0x90                 ; == script_id 36 * 4
```

**Step 2 — script header** (4 bytes at `frm + 0x90`):

```
frmKEN[0x90..0x94] = 00 04 25 00
  frame_list_offset = 0x0400
  flags             = 0x25   (bits 0x20|0x04|0x01, see §2.2)
  event_ani         = 0x00
```

**Step 3 — frame list at 0x400** (`frmKEN[0x400..0x415]`):

```
6f 00 00 00   6f  -> ani 111, dx 0, dy 0, flags 0x00
70 00 00 00   70  -> ani 112, dx 0, dy 0, flags 0x00
71 00 00 00   71  -> ani 113, dx 0, dy 0, flags 0x00
71 00 00 00   71  -> ani 113, dx 0, dy 0, flags 0x00
ff 72 00 00   terminator (0xFF), next list starts at 0x411
```

**Step 4 — ANI record 111** (`aniKEN[111*42 = 4662 = 0x1236]`, 42 bytes):

```
52f4 dcff b6ff f4ff   e6f4 e2ff daff e4ff
ddff b8ff f3ff f8ff   0000 f8ff e9ff d1ff
f7ff ebff bdff f5ff   80 bb
```
decoded:

```
T = sprKENNT112   T_x=-36 T_y=-74 T_x_flip=-12
B = sprKENNB112   B_x=-30 B_y=-38 B_x_flip=-28
boxes:
  box0 {x_a=-35, y=-72, x_b=-13}  type 8  (48×32)
  box1 {-8,0,-8}                  EMPTY (0xFFF8)
  box2 {-23,-47,-9}               type 11 (32×64)
  box3 {-21,-67,-11}              type 11 (32×64)
box-type nibbles: byte 0x28 = 0x80 -> 8,0 ; byte 0x29 = 0xBB -> 11,11
```

Used later as (facing bit `player[0x1e]&4` = 0 → words 0/1):

```
T layer drawn at X = obj.X + (-36), Y = obj.Y + (-74) - h(sprKENNT112) + 16
B layer drawn at X = obj.X + (-30), Y = (T_y - B_y - h(sprKENNB112)) + 16 = -74+38-h+16
hitboxes: left = -35 + playerX - 4, right = left + 48 + 4, top = -72 + playerY - 4, bottom = top + 32 + 4
```

**Name resolution**: handle `0xF452` → index `0xFFFF-0xF452 = 2989` → `INDEX.SF2` record 2989 =
`sprKENNT112`. The T/B handles always resolve into the character's own bank (`sprKENN*`), except
`0xF33A` = `sprBLANKSPR` (skip drawing that layer) and a handful of VEGA records that reference
`sprVEGAT###` in the B slot.

---

## 6. How an action/frame number maps to a sprite asset name

1. `player[0x3d]` = **action code** (0..0x4B; values 0x40/0x41 are dispatched through `DS:0x3BEF`).
2. `script_id` = even byte of the `ait` sparse table entry at/just below the action
   (or a decision code chosen directly in the AI code, e.g. 0x0D/0x32/0x3D).
3. `player[0x20] = script_id*4` → header at `frm + script_id*4` → `frame_list_offset`.
4. Frames are 4 bytes; the current one is at `frame_list_offset + player[0x32]` (a multiple of 4).
   `player[0x32]` advances by 4 per animation step; at a 0xFF byte-0 the list behaviour depends on
   the header flags (§2.2).
5. `player[0x35] = frame.ani_index`; `ANI = ani + ani_index*42`.
6. Layer sprite name = `INDEX.SF2[0xFFFF - ANI.T_handle].name` for the first layer and
   `INDEX.SF2[0xFFFF - ANI.B_handle].name` for the second (banks `spr<CODE>T###` / `spr<CODE>B###`,
   `###` = 3-digit decimal index). Draw nothing for `sprBLANKSPR` (handle 0xF33A).
7. Palette: `DS:0x3C11[char_id*2]` (or the second bank `DS:0x3C29[char_id*2]` when the game wants
   the alternate colour set) → `pal<CODE>T001` / `pal<CODE>T002`.
8. Hitbox sizes: box type nibble → `DS:0x1618[type*2]` = `(w,h)`; the box list built in
   `FUN_1000_12712` is used by the AABB test `FUN_1000_1281E`.

---

## 7. Evidence summary (addresses)

| Address | Role |
|---|---|
| 0x104FB | `FUN_1000_04FB` distance bucket vs `DS:0x0050` thresholds → `DS:0x4E` |
| 0x10524 | action/AI wrapper: `bp = DS:0x60[char*2]` (`ait` handle), `call [bp+0x3BF9]` modes |
| 0x105B1 | reads `ait[hdr[3] + 24*bucket + 8*(player[0xa]&3) + rand(7)]` (0x105C5–0x105F6) |
| 0x1068D/0x106E8 | reads `ait[hdr[0] + action]` |
| 0x10721 | reads `ait[hdr[2] + 16*x + bucket + rand&8]` |
| 0x10944 | preload: `acquire(ani)`, `acquire(frm)`, then all 64 lists' T/B sprites (0x10978–0x109A7) |
| 0x109C1/0x109C8 | layer update (BP=0 = T layer, BP=8 = B layer): ANI index = `player[0x35]*42`, sprite X/Y/flip from the record (0x109CF–0x10A69) |
| 0x10AB1 | frame advance: `mov di,[si+0x20]`, list base `es:[di]`, `+player[0x32]`, terminator `0xFF`, bytes 1/2 → CX/DX |
| 0x10B7F | frame staging: list-end wrap per header flags, byte3 bits → `obj[0x1c]` bits 2/3 |
| 0x10C26/0x10C32/0x10C58 | header readers: `es:[di+2]` (flags), `es:[di+3]` (event ANI) |
| 0x10C5D | script commit: `player[0x20] = AX(=player[0x39])`, resets frame position (0x10C8D–0x10C9E) |
| 0x10E67 | frame deltas: `add [si+0x24],cx; add [si+0x28],dx` |
| 0x12712 | hitbox builder (4 boxes × type nibbles + `DS:0x1618`) |
| 0x1281E | AABB overlap test over the 4 boxes |
| 0x13EF9 | sprite size reader (`acquire(spr)`; byte0 = w, byte1 = h) |
| 0x1746E | fighter init: sets `player[0x37]=char*2`, `player[0x3d]=0x47`, spawns handlers 0x7665/0x9C1/0x9C8/0xA8A (0x174E2–0x17558) |
| 0x176E0 | `DS:0x3BEF` `{action,pad,handler}` dispatcher, 0xFF-terminated |
| 0x176FC | start action: AL=`player[0x39]` → `FUN_1000_10C5D` |
| 0x17BE1 | `ait[hdr[1]]` sparse action→script lookup (`es:[bx+si]`, `es:[bx+si+1]`) |
| 0x17C4C | commit: `player[0x3d] = action`, `player[0x39] = ch*4`, `player[0x3a] = cl` (0x17C84–0x17C91) |
| 0x110C1 / 0x10C75 / 0x10D57 | per-character mode-6 / mode-2 / mode-4 dispatches |

Sample pseudocode (REA, `FUN_1000_12712`, hitbox builder — abridged):

```
iVar5 = *(byte*)(ani_rec+0x28) << 8 | *(byte*)(ani_rec+0x29);   /* the two nibble bytes */
do {  bVar6 = iVar5 >> 8;  uVar7 = (bVar6 >> 4) * 2;
      width  = *(byte*)(uVar7 + 0x1618);
      height = *(byte*)(uVar7 + 0x1619);
      type   = bVar6 >> 4;  iVar5 <<= 4; } while (--iVar3);
...
iVar5 = *piVar8 + *piVar11 - 4;          /* box word + player X - 4 */
*(int*)(out+2) = iVar5;  *(int*)(out+6) = iVar5 + width + 4;
iVar5 = piVar8[1] + piVar11[1] - 4;      /* y */
*(int*)(out+4) = iVar5;  *(int*)(out+8) = iVar5 + height + 4;
```

---

## 8. Confidence and unknowns

**High confidence (byte-exact, cross-checked against code):**

* `frm` = 64 × `{u16 list_off, u8 flags, u8 event_ani}` + `0xFF`-terminated 4-byte frame lists;
  frame = `{u8 ani_index, i8 dx, i8 dy, u8 flags}`; frame deltas are added to per-layer accumulators.
* `ani` = 200 × 42 bytes; field offsets, box slots with `0xFFF8` sentinel, box-type nibbles,
  `DS:0x1618` size table; index = `ani_index*42` with `ani_index < 0xC8`.
* `ait` = 11-word offset header + sections; the four verified readers and their index formulas;
  bosses have 1150-byte `ait` with the last sections empty.
* Character order, per-character DS handle tables, palette tables, `DS:0x3BEF` action table.
* Handle encoding `index = 0xFFFF - handle` and the `spr<CODE>T###/B###` name scheme.

**Medium confidence:**

* `T`/`B` = the two sprite **layers** of one pose (upper/lower). Evidence: two layer processes are
  spawned with BP=0/BP=8 selecting ANI words 0–3 / 4–7 of the *same* record, both are updated per
  frame. The artwork itself is unusual to render (the decoded frames look sheared/diagonal at
  several orientations), so the "upper/lower body" reading is inferred from the layout and the
  Y-offset arithmetic, not from a rendered composite.
* Flag-bit names in §2.2 (loop/restart/hold) — behaviour verified, external meaning inferred.
* Meaning of the box-type number beyond "index into `DS:0x1618`" (e.g. which types are hurtboxes
  vs hitboxes).
* The exact semantics of sections `hdr[4..10]` of `ait` and of the per-action bytes read at
  `hdr[0]` / `hdr[2]` (these also carry values ≥ 0x40 that look like action codes).

**Unknown / unresolved:**

* The full input→action state machine (which inputs set which `player[0x3d]`/`player[0x39]`);
  only the AI/decision paths and the commit functions were traced.
* `frm` flag bits 0x01, 0x02, 0x40 (referenced nowhere in the traced readers).
* Whether the `ait` per-action bytes at `hdr[0]`/`hdr[2]` are attack ids, priorities, or timing.
* Why a few VEGA records reference a `sprVEGAT###` sprite in the B slot, and why several ANI
  records have one of the two layer handles set to `sprBLANKSPR`.
* Whether the near-copy `frmGUILE`/`aitBLANKA`/`aitDHALISM`/`aitEHONDO`/`aitGUILE`/`aitRYU`
  revisions in the `GENERAL.SF2` holes correspond to the shipped tables (they are byte-identical
  apart from 1–30 bytes; the live records were used here).

---

## 9. Rust port checklist

* Load `frm<CODE>`, `ani<CODE>`, `ait<CODE>` per character (LZW-decompress first).
* Keep `char_id` 0..11 with the fixed order of §1 and the handle→INDEX-record mapping.
* Implement three decoders: `FrmScripts` (64 headers + frame lists), `AniRecord` (42-byte
  structs), `AitHeader` (11 `u16` offsets).
* Draw both layers per frame (skip `sprBLANKSPR`), using `x/x_flip` (selected by a facing bit),
  `y`, `-h + 0x10` / `T_y - B_y - h(B) + 0x10` origins, and the frame `dx/dy` accumulators.
* Hitboxes: 4 slots, `{x_a,y,x_b}` + type nibble → `(w,h)` from the 16-entry size table; emit
  `{valid,type,l,t,r,b}` records.
* AI: distance bucket from the 8 thresholds, then the `ait` sparse action→script table and the
  distance/aggression decision tables.
