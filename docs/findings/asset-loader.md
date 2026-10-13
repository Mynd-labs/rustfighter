# Street Fighter II (MS-DOS, 1995) — Asset loader / INDEX.SF2 findings

Target: `/tmp/opencode/rea/game/street-fighter-ii/SF2.EXE` (79,948 bytes, MZ, 16-bit x86).
Address mapping used throughout: **CS base = linear 0x10000**, file_offset = linear − 0xFFA0,
**DS base = linear 0x1AA40**. `FUN_1000_xxxx` = linear 0x10000 + 0xxxx.

---

## Summary

* The game ships 8 files: `INDEX.SF2`, four asset archives (`sprite1.sf2`, `sprite2.sf2`,
  `general.sf2`, `backdrop.sf2`), plus `CONFIG.SF2`, `HIGHSCOR.SF2`, `EXTRAS.SF2`, `SBDRIVER.DRV`.
* `INDEX.SF2` = 0x80-byte header (8 × 16-byte file slots) + **3749 × 17-byte directory records**
  (`(63861 − 128) / 17 = 3749`). Each record = `{u8 flags; u24 file_offset; u16 length; char name[11]}`.
* At startup `FUN_1000_3575` opens `INDEX.SF2` + every named archive, reads the directory into a
  compact **11-byte/record** in-memory table at `ES:[0x19C2]` (names are **dropped**), then closes
  `INDEX.SF2`. Asset reads use the archive handle stored in the file slot selected by the record flags.
* Runtime asset references are **baked-in negative handles**, `SI = −(recordIndex + 1)`.
  There is **no runtime name lookup**: no asset-name strings exist in the EXE, no 11-byte compare
  loop touches the directory, and per-character tables in DS contain negative handles
  (e.g. at DS:0x266: `7e f1 7d f1 7c f1 …` = −0xE82, −0xE83, −0xE84 …).
  The only routine that can expose a record's 11-byte name (`0x139A4`) is **unreferenced dead code**
  in this build (verified: no CALL/JMP/pointer reference anywhere in the file).
* Compression: records with flags low nibble 3 use a custom **bit-packed LZ77/LZSS-family
  decompressor** (`FUN_1000_87C7` → core `FUN_1000_8850`); flags low nibble 0 = raw copy.
  Decompressed length is a u16 header inside the compressed stream (max 0x8000).
* Corrects an earlier note: linear **0x132CF is `mov ah,42h; int 67h` (EMS page mapping)**, not a
  DOS seek. The DOS `lseek` is `FUN_1000_14D6` at **0x114D6** (`mov ax,4200h; int 21h`).

---

## Call graph (address → purpose)

### Asset directory / load API (all in segment 0x1000)

| Linear | Symbol | Purpose |
|---|---|---|
| 0x13575 | `FUN_1000_3575` | **Asset system init**: open INDEX.SF2, read 0x80 header, open each slot file (handle → slot+0x0E), build 11-byte in-memory directory, close INDEX, init decompress workspace, read EXTRAS.SF2 header word. Sets `[0x1A5C]=1`. |
| 0x136AA | `FUN_1000_36AA` | **Shutdown**: close all slot handles (`≠ −1`), free decompress workspace (`0x8788`), clear `[0x1A5C]`. |
| 0x136F0 | `FUN_1000_36F0` | **Load one asset** by handle in `SI`: seek → raw read or decompress → mark loaded, stamp. |
| 0x137AF | `FUN_1000_37AF` | **Free one asset** by handle in `SI` (if loaded). |
| 0x137C8 | `FUN_1000_37C8` | **Eviction victim picker** (loaded & unpinned & unmapped, smallest [2..4] key) then frees it; called by the allocation retry loop. |
| 0x13846 | `FUN_1000_3846` | Free record buffer (`ES = [DI]` → `0x30E1`), zero `[DI]`, clear flag bits 2 (loaded) and 3 (pinned). |
| 0x13867 | `FUN_1000_3867` | Free **all** records (loop `[0x19C4]`). |
| 0x13880 | `FUN_1000_3880` | Free all **unpinned** records (flags bit 3 == 0). |
| 0x138A0 | `FUN_1000_38A0` | Load array: `DI` → word array of handles, terminated by 0. |
| 0x138B3 | `FUN_1000_38B3` | Free array: `DI` → word array of handles, terminated by 0. |
| 0x138C6 | `FUN_1000_38C6` | Load `CX` handles, consecutive words starting at `DI`. |
| 0x138D5 | `FUN_1000_38D5` | Free `CX` handles, consecutive words starting at `DI`. |
| 0x138E4 | `FUN_1000_38E4` | **acquire view**: ensure loaded, set flag bit 7 (in-use/mapped), map (0x3118). Fatal if already mapped or handle 0. |
| 0x1391C | `FUN_1000_391C` | **release view**: flush (0x3138), update eviction stamp [2..4], clear bit 7. Fatal if not mapped. |
| 0x13960 | `FUN_1000_3960` | **pin**: if loaded, set flag bit 3. |
| 0x1398F | `FUN_1000_398F` | **handle → record pointer** helper: `index = −SI − 1`, `DI = index*0xB`, `ES = [0x19C2]`. |
| 0x139A4 | (unnamed) | Dead code: read 17-byte INDEX record (incl. name) for handle `SI`; opens INDEX.SF2, seeks 0x80+17·index, reads into DS:0x19CA, closes. Never called. |

### Decompression

| Linear | Symbol | Purpose |
|---|---|---|
| 0x1874A | `FUN_1000_874A` | Init decompress workspace: alloc/map 0x4000-byte window table (→ `CS:[0x893D]`) + 0x8000-byte staging buffer (→ `[0x43BC]`), set `[0x43C0]=1`. |
| 0x18788 | `FUN_1000_8788` | Free decompress workspace (two frees + clear `[0x43C0]`). |
| 0x187B6 | `FUN_1000_87B6` | Decompress dispatch: requires `AL == 3`, else fatal (`DX=0x0B87`); jumps to 0x187C7. |
| 0x187C7 | `FUN_1000_87C7` | **Compressed record loader**: read u16 decompressed size, allocate, read compressed remainder to staging, call 0x8850, verify length, flush. |
| 0x18850 | `FUN_1000_8850` | **Core decompressor** (bit-packed LZ77/LZSS-family, see below). |

### Memory manager used by the loader

| Linear | Symbol | Purpose |
|---|---|---|
| 0x13023 | `FUN_1000_3023` | `alloc_bytes(DX:AX)` → AX = segment (or EMS address); `DX≠0` fatal; EMS path `0x3054`, else DOS `AH=48h` (`0x14DE`). CF = failure. |
| 0x130C8 | `FUN_1000_30C8` | `alloc_with_evict(DX:AX)`: save size at 0x1978/0x197A, retry `0x3023`; on failure call `0x37C8` (evict) and retry. Returns segment in AX. |
| 0x130E1 | `FUN_1000_30E1` | `free(ES)`: `ES < 0xA000` → DOS `AH=49h` (`0x14FF`); else EMS free (`0x30F5`). |
| 0x13054 | `FUN_1000_3054` | EMS page allocation (`int 67h`, uses page count at 0x197C). |
| 0x1310E/0x13118 | `FUN_1000_310E/3118` | Map a buffer for CPU access (EMS staging window; identity for DOS buffers). |
| 0x13138/0x13141/0x13149 | `FUN_1000_3138/3141/3149` | Commit/flush a mapped buffer back to EMS (identity for DOS). |
| 0x132A5 | `FUN_1000_32A5` | EMS detect/init: `int 67h`, page frame → 0x198A/0x198C, page count → 0x197C, sets 0x19B0. |

### DOS file I/O helpers (near calls, register ABI)

| Linear | ABI | Purpose |
|---|---|---|
| 0x11470 | `DS:DX`=ASCIIZ path → AX=handle, CF=err | `mov ax,3D00h; int 21h` (open read-only) |
| 0x1147F | `BX`=h, `CX`=count, **`DX:AX`=far buffer** (`16 8E DA 8B D0` = `push ds; mov ds,dx; mov dx,ax`) → CF, AX=read | read `AH=3Fh` |
| 0x1148A | same args as read | write `AH=40h` |
| 0x11495 | `BX`=h → CF | close `AH=3Eh` |
| 0x1149A | `BX`=h → AX:DX=size (32-bit) | size via seek-cur/size/seek-back (`0x14C8`, `AX=4202h`, `0x14D6`) |
| 0x114C8 | `BX`=h → DX:AX=position | tell (`AX=4201h`, CX=DX=0) |
| 0x114D6 | `BX`=h, `CX:DX`=offset → CF | **seek absolute (`AX=4200h`)** |
| 0x114DE | `DX:AX`=bytes → AX=segment | DOS allocate paragraphs (`AH=48h`) |
| 0x114FF | `ES`=segment | DOS free (`AH=49h`) |

> The far-pointer convention of the read/write helpers is **segment:offset = DX:AX** (the file
> bytes `1E 8E DA 8B D0` at 0x1147F are `push ds; mov ds,dx; mov dx,ax`). Callers load
> `MOV DX,DS / MOV AX,buffer_off` before the call (e.g. 0x1359E–0x135A6).

---

## In-memory directory layout

### Globals (DS)

| Address | Meaning |
|---|---|
| 0x19BE | INDEX.SF2 handle, open only during `0x3575` |
| 0x19C2 | far pointer (stored as segment/offset words) to the 11-byte-per-record directory table |
| 0x19C4 | record count (3749) |
| 0x19C6/0x19C8 | 24-bit eviction-stamp source, **only ever zeroed** (by 0x3575); read via `LES AX,[0x19C6]` |
| 0x19C0 | scratch: handle of the archive for the record currently being loaded |
| 0x19DB..0x1A5A | 8 × 16-byte file-slot table, read from the first 0x80 bytes of INDEX.SF2 |
| 0x1A5B | "staging busy" flag; set to 0xFF during a load (0x3560/0x372F), cleared after |
| 0x1A5C | "asset system initialized" flag (0 → 0xFF) |
| 0x1A5D | ASCIIZ string `"INDEX.SF2"` |
| 0x4E3C | EXTRAS.SF2 header word read at init |
| 0x4E40 | ASCIIZ string `"extras.sf2"` |
| 0x43BC/0x43BE/0x43C0 | decompress workspace: staging segment / decompressed-size word / init flag |

### File-slot table (16 bytes each, from INDEX.SF2 header byte 0x00–0x7F)

```
00..0D  ASCIIZ archive filename, NUL-padded (e.g. "sprite1.sf2\0...")
0E..0F  u16 handle of the open file, written at runtime; 0xFFFF if not open
```
Observed header: slots 0–3 = `sprite1.sf2`, `sprite2.sf2`, `general.sf2`, `backdrop.sf2`;
slots 4–7 empty (loader stores −1 in their handle field).

### In-memory directory record (11 bytes, built by 0x3575, addressed as `index*0xB`)

```
00..01  u16 segment of the loaded buffer (0 = not loaded)
02..04  24-bit eviction stamp, copied from [0x19C6] far pointer on load (0x36F0) and release
        (0x391C); uninitialized until first load; source is always 0 in this build
05..06  u16 asset file offset, low 16 bits      ) from disk record bytes 1..2
07      u8  asset file offset, bits 16..23      ) disk record byte 3
08..09  u16 asset length in bytes               ) disk record bytes 4..5
0A      u8  flags = disk record byte 0 & 0x73
```
Note: offsets 0x02–0x04 are *not* written from the disk record; the loader only zeroes 0x00–0x01
and copies 0x05–0x0A.

### On-disk directory record (17 bytes at file offset 0x80 + 17·index in INDEX.SF2)

```
00      u8   flags   (0x00/0x03 raw|LZ for file 0, 0x10/0x13 file 1, 0x20/0x23 file 2, 0x30/0x33 file 3)
01..03  u24  file offset inside the archive file (little-endian)
04..05  u16  length in bytes
06..10  char name[11], left-justified, **space padded** (e.g. "sprCHLIB000", "tunBSFX2   ")
```
Example first records (hexdump of INDEX.SF2):
`03 00 00 00 42 05 "sprCHLIB000"`, `03 42 05 00 8a 03 "sprCHLIB001"` (offset = previous offset + length).

### Handle encoding

`recordIndex = −SI − 1` (`0x1398F`: `MOV AX,SI / NEG AX / DEC AX / MOV DX,0xB / MUL DX / MOV DI,AX / MOV ES,[0x19C2]`).
* `SI < 0` → valid handle (e.g. SI = 0xFFFF → index 0; SI = 0xF17E → index 3713).
* `SI == 0` → fatal (null handle trap, `0x136F6`).
* `SI > 0` → silently ignored (returns).

Record flags histogram over all 3749 records:

| flags | count | meaning |
|---|---|---|
| 0x03 | 1605 | file 0 (sprite1), LZ |
| 0x00 | 27 | file 0, raw |
| 0x13 | 1421 | file 1 (sprite2), LZ |
| 0x10 | 1 | file 1, raw |
| 0x23 | 279 | file 2 (general), LZ |
| 0x20 | 74 | file 2, raw |
| 0x33 | 341 | file 3 (backdrop), raw (all) |
| 0x30 | 1 | file 3, raw |

---

## Asset load ABI

### Game-facing calls (register ABI, `unaff_*` in Ghidra output = register args)

```
load(handle)          SI = handle   → CALL 0x136F0      ; ensure loaded (idempotent)
free(handle)          SI = handle   → CALL 0x137AF      ; free if loaded
acquire(handle)       SI = handle   → CALL 0x138E4      ; load + map + mark in-use (bit 7)
release(handle)       SI = handle   → CALL 0x1391C      ; flush + unmark (bit 7)
pin(handle)           SI = handle   → CALL 0x13960      ; set bit 3 (protected from eviction)
load_list(DI)         DI → 0-term word array → CALL 0x138A0
free_list(DI)         DI → 0-term word array → CALL 0x138B3
load_n(DI,CX)         DI → word array, CX = count → CALL 0x138C6
free_n(DI,CX)         DI → word array, CX = count → CALL 0x138D5
free_all()            → CALL 0x13867
free_all_unpinned()   → CALL 0x13880
```

Real caller example — `FUN_1000_0944` (character asset preload), char id in `AL`:
```
0x1094c: MOV BL,AL / 0x1094e: XOR BH,BH
0x10956: MOV SI,[BX+0x60]  / 0x1095a: CALL 0x1000:36F0   ; load(handle)
0x10964: MOV SI,[BX+0x266] / 0x10969: CALL 0x1000:38E4   ; acquire(handle)
0x1096e: MOV SI,[BX+0x27E] / 0x10973: CALL 0x1000:38E4   ; acquire(handle)
```
The tables at `BX+0x266` / `BX+0x27E` contain the negative handles (see Summary).

### Core load flow — `FUN_1000_36F0` (SI = handle)

1. `0x1398F` → `ES:DI` = record.
2. If `flags & 0x04` (already loaded) → done.
3. Set busy flag `[0x1A5B]=0xFF`.
4. Archive handle:
   `MOV DL,ES:[DI+0xA] & 0x70; MOV SI,DX; ADD SI,0x19DB; MOV AX,[SI+0xE]; MOV [0x19C0],AX`
   (flags bits 4–6 × 0x10 = slot byte offset).
5. Seek `SEEK_SET` to the 24-bit offset:
   `MOV CL,ES:[DI+7]` (high byte), `MOV DX,ES:[DI+5]` (low word), `CALL 0x114D6` (= `AX=4200h`).
6. Branch on `flags & 0x03`:
   * `== 0` (raw): `AX = ES:[DI+8]` (length) `→ CALL 0x30C8` (alloc with eviction) `→ ES:[DI]=AX`;
     map (`0x310E`); DOS read with `BX=[0x19C0]`, `CX=ES:[DI+8]` into the new buffer; commit (`0x3141`).
   * `!= 0`: `MOV BX,[0x19C0]; CALL 0x87B6` (requires AL==3; compressed path).
7. `OR ES:[DI+0xA], 0x04` (loaded); copy 24-bit stamp from `[0x19C6]` to `[DI+2..4]`;
   clear busy flag; return.

### Compressed path — `FUN_1000_87C7`

* Reject `length > 0x8000` (fatal).
* `read(BX=handle, CX=2)` → **u16 decompressed size** at DS:0x43BE.
* `alloc(0x43BE)` with eviction → segment → `ES:[DI]`.
* `read(CX = length−2)` the compressed bytes into the 0x8000-byte staging buffer at `[0x43BC]`.
* Call core decoder `0x8850` (dest `ES:DI` = new buffer, source `DS:SI` = staging).
* Verify `DI == [0x43BE]` (decompressed length must match exactly), else fatal.
* Flush staging (`0x3149` / `0x3138`).

### Memory-manager ABI used above

* `FUN_1000_30C8(DX:AX = size)` → AX = segment of a DX:AX-byte buffer; retries via eviction
  (`0x37C8`) when `FUN_1000_3023` reports failure. Sizes > 0xFFFF are fatal (`DX≠0` in 0x3023).
* `FUN_1000_30E1(ES = segment)` frees it. `FUN_1000_3846` loads `ES` from record[0..1] before freeing.

---

## Flag semantics & decompression

### Record flag byte (in-memory byte 0x0A = disk byte 0 & 0x73)

```
bit 0..1   compression method: 0 = raw copy, 3 = LZ (1,2 cause a fatal in 0x87B6)
bit 2      loaded into memory (set by 0x36F0, cleared by 0x3846)
bit 3      pinned (set by 0x3960; eviction skips pinned records; 0x3880 frees only unpinned)
bit 4..6   archive-file slot index (0..7) -> byte offset in the 16-byte slot table (0x19DB + idx*0x10)
bit 7      in-use / mapped (set by 0x138E4 acquire, cleared by 0x1391C release; fatal to acquire twice)
```
The `& 0x73` mask keeps bits 0,1,4,5,6 — i.e. method + slot; the runtime state bits 2,3,7 are
added in RAM only.

### Decompressor algorithm (FUN_1000_8850, called only with method == 3)

High level: **bit-packed LZ77/LZSS-family decoder with adaptive symbol width and a small
dictionary of recent output runs — not a simple RLE**.

* Compressed stream layout: `u16 decompressedSize` header (read by 0x87C7) followed by a bit-packed
  payload; the decoder must produce exactly `decompressedSize` bytes.
* Bit reader: 16-bit window with bit position in `CL`; `LODSW`, `XOR/AND/XOR` mask merge, `ROR AX,CL`,
  `ADD CL,DL` (symbol width, starts at 9), `SBB SI,0` compensation when crossing a word boundary.
* Symbol decoding:
  * `< 0x100` → literal byte (`STOSB`), and the run table is updated.
  * `== 0x100` → escape; 2 extra bits select: 0 = widen symbols (`DL++`), 1 = re-align/restart bit
    stream (jump back to init at 0x8862), 2/3 = end of stream (`RET` at 0x8936).
  * `> 0x100` → back-reference: the code indexes a growing 4-byte-entry table at DS:0x404
    (`(position, length)` pairs recorded as output is produced); `CMP AX,BX` chooses between
    "dictionary hit" (copy the stored run with `REP MOVSB`) and a "raw" path that records a new
    entry and copies/overlaps.
* Workspace: a 0x4000-byte table buffer allocated by `FUN_1000_874A` and installed at `CS:[0x893D]`
  (the decoder subtracts 0x40 from it), plus the 0x8000-byte staging buffer `[0x43BC]`.

---

## Evidence (addresses + quotes)

**INDEX open / header read / slot open (0x3575):**
```
0x13589: MOV DX, 0x1A5D          ; DS:0x1A5D = "INDEX.SF2"
0x1358C: CALL 0x1000:1470        ; open -> 0x11470 (mov ax,3D00h; int 21h)
0x13597: MOV [0x19BE], AX
0x1359A: MOV BX,[0x19BE] / 0x1359E: MOV CX,0x80 / 0x135A1: MOV DX,DS
0x135A3: MOV AX,0x19DB  / 0x135A6: CALL 0x1000:147F   ; read 128-byte slot table
0x135BA: CMP byte ptr [SI],0 / ... / 0x135CC: MOV [SI+0xE],AX  ; per-slot handle
```

**Record count / directory allocation / 11-byte records:**
```
0x135D8: CALL 0x1000:149A        ; size(DX:AX)
0x135E7: SUB AX,0x80 / 0x135ED: MOV CX,0x11 / 0x135F0: DIV CX / 0x135F2: MOV [0x19C4],AX
0x135F5: MOV CX,0xB / 0x135F8: MUL CX / 0x13606: CALL 0x1000:3023 / 0x13611: MOV [0x19C2],AX
0x13624: MOV AX,0x19CA / 0x13627: MOV CX,0x11 / 0x1362A: CALL 0x1000:147F  ; read 17-byte record
0x13635: MOV SI,0x19CA / 0x13638: MOV AL,[SI] / 0x1363A: MOV ES:[DI+0xA],AL
0x13641: MOV CX,[SI+1] / 0x13648: MOV ES:[DI+5],CX
0x1363E: MOV DL,[SI+3] / 0x13644: MOV ES:[DI+7],DL
0x1364C: MOV AX,[SI+4] / 0x1364F: MOV ES:[DI+8],AX
0x13653: AND byte ptr ES:[DI+0xA],0x73
0x13658: MOV word ptr ES:[DI],0x0
```

**Handle → record, handle=index encoding (0x1398F):**
```
0x13991: MOV AX,SI / 0x13993: NEG AX / 0x13995: DEC AX
0x13996: MOV DX,0xB / 0x13999: MUL DX / 0x1399B: MOV DI,AX / 0x1399D: MOV ES,[0x19C2]
```

**Slot lookup, seek, method branch (0x36F0):**
```
0x1371A: XOR DH,DH / 0x1371C: MOV DL,ES:[DI+0xA] / 0x13720: AND DL,0x70
0x13723: MOV SI,DX / 0x13725: ADD SI,0x19DB / 0x13729: MOV AX,[SI+0xE] / 0x1372C: MOV [0x19C0],AX
0x13735: MOV CL,ES:[DI+7] / 0x13739: MOV DX,ES:[DI+5] / 0x1373D: CALL 0x1000:14D6  ; lseek CX:DX
0x13748: MOV AL,ES:[DI+0xA] / 0x1374C: AND AL,0x3 / 0x1374E: JZ 0x3759        ; raw path
0x13750: MOV BX,[0x19C0] / 0x13754: CALL 0x1000:87B6                          ; LZ path
0x13759: XOR DX,DX / 0x1375B: MOV AX,ES:[DI+8] / 0x1375F: CALL 0x1000:30C8    ; alloc
0x1376A: MOV ES:[DI],AX / 0x13778: MOV CX,ES:[DI+8] / 0x1377C: CALL 0x1000:147F; read raw bytes
0x1378D: OR byte ptr ES:[DI+0xA],0x4    ; loaded
0x13793: LES AX,[0x19C6] / 0x1379A: MOV ES:[DI+2],AX / 0x1379E: MOV ES:[DI+4],DL  ; stamp
```

**Free (0x3846):**
```
0x13846: TEST byte ptr ES:[DI+0xA],0x4
0x1384F: MOV ES, word ptr ES:[DI] / 0x13852: CALL 0x1000:30E1   ; free segment in record[0..1]
0x13857: MOV word ptr ES:[DI],0x0
0x1385C: AND ES:[DI+0xA],0xF7 / 0x13861: AND ES:[DI+0xA],0xFB  ; clear pin + loaded
```

**Compressed loader (0x87C7) and dispatch (0x87B6):**
```
0x187B6: CMP AL,0x3 / 0x187B8: JNZ 0x87BC / 0x187BC: MOV DX,0xB87 ... ; only method 3
0x187D1: CMP AX,0x8000 / 0x187D4: JBE ...
0x187DC: MOV DX,DS / 0x187DE: MOV AX,0x43BE / 0x187E1: MOV CX,0x2 / 0x187E4: CALL 0x1000:147F  ; u16 size
0x187F2: MOV AX,[0x43BE] / 0x187F5: CALL 0x1000:30C8 / 0x18800: MOV ES:[DI],AX
0x1880C: MOV CX,ES:[DI+8] / 0x18810: SUB CX,0x2 / 0x18817: CALL 0x1000:147F  ; compressed payload
0x18823: XOR SI,SI / 0x18825: XOR DI,DI / 0x1882E: CALL 0x1000:8850          ; decompress
0x18833: MOV CX,[0x43BE] / 0x18837: CMP DI,CX / 0x18839: JNZ fatal          ; exact length check
```

**Core decoder samples (0x8850):**
```
0x18867: SHL CH,CL / 0x18869: LODSW SI / 0x1886A: XOR AL,[SI] / 0x1886C: AND AL,CH / 0x1886E: XOR AL,[SI]
0x18870: ROR AX,CL / 0x18872: AND AH,DH / 0x18874: ADD CL,DL / 0x18879: SBB SI,0x0 / 0x18884: STOSB ES:DI
0x188D3: CMP AX,0x100 / 0x188D6: JL 0x88FA (literal) / 0x188D8: JZ 0x890A (escape)
0x188DB: SHL AX,0x1 / 0x188DD: SHL AX,0x1 / 0x188DF: CMP AX,BX / 0x188E1: JC 0x888D (dictionary hit)
0x18923: AND AL,0x3 / 0x18925: JNZ 0x892F ; 0 -> widen (RCL DH,1 / INC DL), 1 -> restart, 2/3 -> RET
```

**Dead "read INDEX record by handle" (manual decode of bytes at 0x139A4):**
```
0x139A8: BA 5D 1A      MOV DX,0x1A5D      ; "INDEX.SF2"
0x139AB: E8 C2 DA      CALL 0x11470       ; open
0x139B6: A3 BE 19      MOV [0x19BE],AX
0x139B9: 8B C6 F7 D8 48 MOV AX,SI / NEG / DEC        ; index = -SI-1
0x139BE: BA 11 00 F7 E2 MOV DX,0x11 / MUL DX         ; index*17
0x139C5: 81 C2 80 00   ADD DX,0x80                  ; file offset 0x80+17*index
0x139CF: E8 04 DB      CALL 0x114D6                ; lseek
0x139E6: E8 96 DA      CALL 0x1147F                ; read 17 bytes -> DS:0x19CA (name at +6)
0x139F8: E8 9A DA      CALL 0x11495                ; close
```
No CALL/JMP/pointer reference to 0x139A4 exists anywhere in SF2.EXE (byte scan of every `E8`/`E9`
rel16 target and of the word 0x39A4).

**EXTRAS.SF2 at init (0x3575):**
```
0x13678: MOV DX,0x4E40 / 0x1367B: CALL 0x1000:1470    ; open "extras.sf2"
0x1368B: MOV AX,0x4E3C / 0x1368E: MOV CX,0x2 / 0x13691: CALL 0x1000:147F  ; 2-byte header -> [0x4E3C]
0x1369D: CALL 0x1000:1495                              ; close
```

**Data corroboration (read-only):**
* INDEX.SF2 first 0x90 bytes: `sprite1.sf2`/`sprite2.sf2`/`general.sf2`/`backdrop.sf2` slots;
  first record `03 00 00 00 42 05 73 70 72 43 48 4C 49 42 30 30 30` ("sprCHLIB000").
* DS:0x266 handle table: `7E F1 7D F1 7C F1 7B F1 …` = −0xE82, −0xE83, −0xE84, −0xE85 …
* File sizes match the loader's checks: EXTRAS.SF2 = 182, CONFIG.SF2 = 47 (0x2F read by 0x15F09),
  HIGHSCOR.SF2 = 94 (0x5E read by 0x1831B).

---

## Confidence and unknowns

**High confidence**
* INDEX.SF2 layout (header + 17-byte records), 11-byte in-memory record field sources, count math.
* Handle encoding `SI = −(index+1)`, all API entry points and their register ABIs (verified in assembly).
* Flag bits 0–1 (method), 4–6 (file slot), 2 (loaded), 3 (pinned), 7 (in-use) semantics.
* Raw vs. compressed branch and the compressed stream header (u16 decompressed size, ≤ 0x8000).
* No runtime name lookup; names only on disk (space-padded, 11 bytes); 0x139A4 is dead code.

**Partially resolved / unknowns**
* Exact bit-level format of the LZ stream (which generic compressor family, if any; the
  dictionary-code semantics at 0x888D–0x88A6) — described structurally only.
* The [2..4] "eviction stamp": its only source `0x19C6/0x19C8` is merely zeroed in this build
  (no store instruction to it anywhere), so eviction by 0x37C8 degenerates to lowest-record-index
  among eligible records. Producer of that 24-bit value is unresolved (possibly vestigial or
  meant to be updated by an overlay that never does).
* Purpose of the 0x1A5B busy flag and of the 2-byte EXTRAS.SF2 header word at 0x4E3C
  (used as a threshold in FUN_1000_9587: `if (*(u16*)0x4E3C < 0xF) fatal`).
* EMS manager internals (0x12A63–0x134F4 cluster) only mapped at a high level; the 0x310E/0x3118/
  0x3138/0x3141/0x3149 staging-window protocol is inferred from usage, not fully traced.
* CONFIG.SF2 / HIGHSCOR.SF2 are opened by FUN_1000_5F09 / FUN_1000_831B (size-checked reads),
  but the filename pointer setup in their caller FUN_1000_4F4F was not fully traced.
* DOS helper cluster renamed vs. earlier notes: 0x1147F read / 0x1148A write (not read/seek);
  0x114D6 seek; 0x1149A size; 0x114C8 tell.
