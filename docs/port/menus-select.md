# Menus & character select — structural map (SF2.EXE, MS-DOS 1995)

Target: `SF2.EXE` (MZ, 16-bit x86), image/CS base linear `0x10000`, **`file_offset = linear − 0xFFA0`**,
**DS base linear `0x1AA40`** (a DS-relative offset `d` lives at linear `0x1AA40 + d`), second data
segment `0x22A40`. Analysis: hand-written 16-bit Capstone linear sweep of the whole image +
REA/Ghidra cross-checks + `INDEX.SF2` record table (`/tmp/opencode/sf2/index_records.json`,
handle → asset name = `0xFFFF − handle`).

Evidence classes: **observed** (bytes/instructions read directly), **derived** (arithmetic on
observed data), **inferred**. Everything called *unknown* below is stated explicitly.

This document **corrects** `fight-engine.md` §3.2/§3.3/§7 in several places (see §7).

---

## 1. Summary

* The title screen and its 4-item menu are a **runtime-built list**: strings are pointed to by a
  0-terminated pointer array at `DS:0x43DE` = `{"GAME START","V.S. BATTLE","OPTION MODE",
  "QUIT TO DOS"}`. The screen is drawn by `0x192E4`, the cursor/input by **`0x1933B`**, which
  returns the *word index* `BX ∈ {0,2,4,6}` into the dispatch table at linear `0x15029`
  (`CS:0x5029`).
* The four dispatch targets are: `0x15031` = GAME START (arcade ladder), `0x1506B` = V.S. BATTLE
  (`CALL 0x14379`), `0x1506F` = OPTION MODE (`CALL 0x153C4`, then re-enters the title menu),
  `0x15076` = QUIT TO DOS (exits to DOS). *(fight-engine.md §3.2 had entries 1/2 swapped and
  entry 1's body wrong — see §7.1.)*
* The **character-select screen** is `0x18953` (a self-contained init + input loop + resolve,
  ~0x8D0 bytes of code, `0x18938`–`0x1922F`). It draws the world map `linSELECMAP`, the 12
  country flags (`sprFLAG_*`), a 4×3 grid of 32×32 face icons (`linFACE0*`, dimmed `linFACE2*`
  when the cell is used), two 128×128 portraits (`sprFACE1*` + `palFACE1*`), the two "1P"/"2P"
  hand markers (`sprPLAY1SEL`/`sprPLAY2SEL`) and the title text `"PLAYER SELECT"`.
* Selection state lives in two 4-byte cursor blocks **`0x4523` (P1)** / **`0x4527` (P2)** =
  `{char*2, cell 0..11, last drawn cell, lock flag}`, in 12 cell→char*2 tables
  (`0x44B4`, `0x44C0`, `0x44CC`, `0x44E4`) and in the 12 used-cell flags `0x452B`.
* After the human confirms, the game **auto-picks the CPU's character** (`0x18A9D`, random among
  the free cells, "first boss cell ≥ 8" when the roster has expanded) — except in the CONTINUE
  path, where the previous opponent cell is re-used (`0x451E = 0xFF`).
* The chosen characters reach the match driver as **`CH`/`CL`** (record `0x2A2C` = P1,
  `0x2A32` = P2) and the **level/stage id** as `DL`; `DL` is stored in **`DS:0x2A3B`** and the
  human-player mask in **`DS:0x2A3C`**. Round 1 of a match runs through
  `0x140F9 → 0x14148 → 0x14573 → 0x15083` with `CX = chars`, `DX = level`.
  *(fight-engine.md §3.3 called `0x2A3B/0x2A3C` "the two selected characters" — wrong, see §7.2.)*
* All menus share one input layer: a keyboard ISR fills a 128-byte per-scancode state table
  (`0x16D4`), a **timer-tick poller** (`0x14D52 → 0x14DBD`) turns it into two per-player
  direction/button bytes (`0x2F0C+4` / `0x2F15+4`), and three one-line readers
  (`0x14D15` direction, `0x14CE2`/`0x14CCA` confirm) are all a menu needs.
* `0x15CFC` and `0x15DE8` are **not** menus: `0x15CFC` is the 5-line copyright/credits screen
  (SF2 logo + text, waits for a key), `0x15DE8` is the `linDRUGSPIC` full-screen warning picture
  (auto-advance or key). The real menu screens are the title screen, the character select, the
  OPTION screen (`0x153C4`) and the CONTINUE screen (`0x14A18`).

---

## 2. Function map

All addresses are **linear**. "→" = the callee.

### 2.1 Outer loop, title screen, dispatchers

| Address | Purpose | Evidence |
|---|---|---|
| `0x104E6` | entry → init `0x14F4F` → `CALL 0x14FB9` (sequencer) | observed (`104F2 call 0x14fb9`) |
| `0x14FB9` | **screen sequencer**: per iteration `0x1979C`, `0x19CD6`, `0x1719F`, `0x17413`, `0x15CFC`, `0x15DE8`, `0x152FE`, `0x15E4A`; a screen that returns **NZ** jumps to the tail `0x14FAF` | observed |
| `0x14FAF` | tail: `[0x3232]=0`; `CALL 0x14FF0`; `JS 0x14FEF` (return = program end) | observed |
| `0x14FF0` | **title menu**: reset input `0x14D29`, `0x13ACB`, `0x192E4` (draw title), `0x12EF3`, `[0x3029]=0`, `AX=0x78 → 0x12AF2` (3000-tick timeout), input loop, dispatch | observed |
| `0x192E4` | **draw the title screen**: load asset group `0x19`, `0x117A6` (clear), `0x15DC3` (SF2 logo), print the 4 items from `DS:0x43DE` (`BP=0x8000` = font #0 outline mode, `CX=0x74`, `DX=0x88+0xC·i`), draw the cursor at index 0 (`0x193BB`), if `[0x16D1]!=0` print `0x457A` ("Press F1 to calibrate joystick.") at `(0x28,0xC0)`, `0x11BBA` (present), `0x14D15` (consume), `[0x4522]=0` | observed |
| `0x1933B` | **title cursor + input** (see §4) → returns `BX ∈ {0,2,4,6}` and flags in `AL`/ZF/SF | observed |
| `0x193BB` | draw the cursor sprite `sprSELETCUR` at `(0x60, 0x84+6·item)`; args `DX=item`, `SI=0xF2D9`, `BP=0` | observed |
| table `0x15029` (`CS:0x5029`) | 4 words: `0x5031, 0x506B, 0x506F, 0x5076` (CS-relative) | observed bytes |
| `0x15031` | GAME START: `[0x4514]=0`; zero `0x3BAF/0x3BB1/0x3BD2/0x3BD4`; `0x19520` (level-load routine, 3 retries via `0x1966D`/`0x1973B`); `CALL 0x18953` (select); on abort return; `[0x323D]=0xFF`; `0x19230` (load + VS screen); `CALL 0x140F9` (match driver); `0x115B8/0x19C04/0x13880/0x14D29`; `[0x3029]=0` | observed |
| `0x1506B` | V.S. BATTLE: `CALL 0x14379` | observed |
| `0x1506F` | OPTION MODE: `CALL 0x153C4`; `POP AX`; `JMP 0x14FF0` (redraws the title menu, the outer RET goes back to `0x14FB7`) | observed |
| `0x15076` | QUIT TO DOS: `0x1834B` (save HIGHSCOR), `0x15F50`, then `INT 21h/4C` exit stub | observed |
| `0x1893F` | `[0x452B..0x4536] = 0` (12 cells free), `[0x451A] = 8` (roster size) | observed |
| `0x153C4` | **OPTION screen** (icon menu, see §6.6) | observed |
| `0x12AF2` | set the screen auto-advance timeout: `[0x168E] = AX*25`, `[0x169E]=0`, `[0x169D]=0xFF` | observed |
| `0x12B0E` | stop the timeout (`[0x169D]=0`) | observed |
| `0x12EE6` | (in the wait loops) tick/again helper | observed call sites |

### 2.2 Sequencer screens (what each really is)

| Address | Role | Return convention (all: `OR AL,AL; RET` style ⇒ ZF) |
|---|---|---|
| `0x1979C` | attract tick (demo/bank), per fight-engine §4.3 state | observed call |
| `0x19CD6` | attract/bank tick, `AL` = 0 or 5 (`[0x3230]==0xFF` cheat switches it) | observed |
| `0x1719F` | **intro/cut-scene player**: assets `0x3840`, palettes `palINTROBOT/palSF2SLOGO/palINTROPL1/palINTROPR1`, full-screen `linINTROTOP` (`0xF340`) at `(0,0)` 320×200 | observed |
| `0x17413` | stub: `XOR AL,AL; RET` (⇒ ZF=1, never finishes the loop) | observed |
| `0x15CFC` | **copyright/credits screen**: logo `0x15DC3` + 5 text lines `DS:0x32F4/0x331C/0x3340/0x3369/0x338F` (font #0: `BP=0x4000`) at `(4,0x92) (0x14,0x9B) (0,0xAD) (0xC,0xB6) (0x20,0xBF)`; then waits: `0x169E==0xFF` (timeout) or key (`0x14CCA`) ⇒ returns `AL≠0` ⇒ **main loop jumps to the title menu** | observed |
| `0x15DE8` | **warning picture screen**: full-screen `linDRUGSPIC` (`0xF352`) + `palDRUGSPIC` (`0xF2C7`) at `(0x70,0x90)`, then waits for key or auto-advance `0x14C64`; returns `AL=0x40` when the auto-advance fires | observed |
| `0x152FE` | **attract demo fight**: `[0x2A3A]/[0x2A38]=1`, timer `0x308A=200`, `AX = rand() & 0xE0E` (two different `char*2` in `AL`/`AH`), `CX=AX`, `DL=CL`, `AL=0` → `CALL 0x15083` (one round, i.e. AI vs AI), then cleanup, returns `AL=[0x3233]` (result) | observed |
| `0x15E4A` | attract "stage" segment: load group `0x19`, `AX=6 → 0x13F28` (stage 6), `0x14084/0x13FAB/0x13F77/0x13F88/0x184A6`, message `0x11AF9(0x32)` at `(0x70,0x90)`, `0x11BBA`, `0x19C04`, `0x13867`, wait `0x15D83(0xA)` | observed |
| `0x15D83` | wait helper: `AX` → 25·AX-tick timeout (`0x12AF2`), loop `0x14F15/0x14CCA`, returns `AL≠0` (key), `0x40` (timeout) | observed |

### 2.3 Character select (`0x18938`–`0x1922F`)

| Address | Purpose | Evidence |
|---|---|---|
| `0x1893F` | reset: `0x452B[0..11]=0`, `0x451A=8` | observed |
| **`0x18953`** | select screen: init, input loop, resolve, exit (§5) | observed |
| `0x18A34` | per-frame event: `0x18A4F`, then in V.S. mode requires **both** locks (`[0x4526]`,`[0x452A] ≠ 0xFF`) ⇒ `CF`; 1P: always `CLC` | observed |
| `0x18A4F` | `AH=1,DI=0x4523` then `AH=2,DI=0x4527` → `0x18A6B`; result `CH` = P1 char, `CL` = P2 char, `DL` = confirm mask | observed |
| `0x18A6B` | one player's event: skip if cell==0xFF; if `[0x4515]==0xFF` skip when the cell is used (`0x452B==0xFF`); on `AH & AL` (=button) clear the lock byte `[DI+3]=0` and `DL |= AH`; `CL = 0x44B4[cell]` | observed |
| **`0x18A9D`** | resolve: if `AL==3` return; else auto-pick the CPU character (§5.3) and write the blocks | observed |
| **`0x18B4C`** | post-confirm: 1P ⇒ `DH=CH`, `DL=CL`, swap if `!(AL&1)`; V.S. ⇒ the **"ready" sub-loop** (flags grid + quote/name bar, `[0x4516]=0xFF`, `0x18DC1`, `0x18BF8`, `0x18B8F`×2, `0x18BBF`, `0x14CE2`) returning `AL=3`, `DL=[0x4520]` | observed |
| `0x18B8F` | V.S. only: per player, direction `[key+4]`, move the battle record's indicator `[DI+5]` in `0..7` | observed |
| `0x18BBF` | V.S. only: move `[0x4520]` by ±2 in `0..0x16` (left/right) | observed |
| `0x18BF8`,`0x18C0D` | V.S. only: draw the two animated bars from the labels to `8·(7−indicator)` | observed |
| `0x18C44`,`0x18CEA`,`0x18D20` | V.S. only: animate the **plane** between the two selected flags (`sprPLANE000..007` chosen by angle `0x18018`), drawing a draw-object | observed |
| `0x18D34` | select header: clear `0x117A6`, blit `linSELECMAP` (`0xF33C`) 256×128 at `(0x20,0)`, load `palSELECMAP`, print `"PLAYER SELECT"` (`DS:0x453D`) at `CX=0x5F`, `DX=0` (V.S.: `DX=8`) with font #4 (`BP=0x4004`) | observed |
| `0x18D6F`,`0x18D9D` | draw `sprPLAY1SEL`/`sprPLAY2SEL` at the flag position of the two chosen chars (`0x44E4` + `0x20`, `−8`) | observed |
| `0x18DC1` | draw flags/used markers: 1P ⇒ **flags on the map** (`0x4448` per char, positions `0x44E4`) for the first `[0x451A]` cells + `sprFLAG_CRO` (`0xF30E`) on used cells; V.S. (+`[0x4516]==0xFF`) ⇒ **2×6 flag grid** at `(0x36+,0x88/0xA8)`, highlight `DI==[0x4520]`, plus the name bar via `0x431A/0x4332` and `" STAGE."` (`0x459A`) | observed |
| `0x18EC0` | draw the 32×32 icon grid: base `0x190E9` (2 rows ⇒ 8 icons), `linFACE0*` (`0x4464`), `linFACE2*` (`0x447C`) when used | observed |
| `0x18F1E` | redraw one cell's icon (highlight/lowlight), position from `0x44CC` + base `0x190E9` | observed |
| `0x18F68` | per-frame wrapper: `0x18FAE`(P1) + `0x18FAE`(P2), `0x18DC1`, then in 1P mode the `"1P"`/`"2P"` labels (`0x1906D`, fonts #4 at `(0x18,0x58)`/`(0x118,0x58)`) | observed |
| `0x18FAE` | one player's portrait update: if the cell changed, lowlight the old cell / highlight the new (`0x18F1E`), set `[DI]=char*2`, copy the 88×112 buffer to the screen (`0x13118..0x13149`), load `palFACE1*` into DAC slot 0x30/0x40, blit `sprFACE1*` (`0x13DAA`), print the char name | observed |
| `0x1907D` | allocate the two 88×112 portrait buffers as *segments* → `[0x43C2]` (P1) / `[0x43C4]` (P2) | observed |
| `0x190C2` | flush both buffers to the screen (`0x130E1`) | observed |
| `0x190D3` | one-shot sound if `[0x4517]==0xFF` | observed |
| `0x190E9` | grid base: `AX=8` (cells), `CX=0x60`, `DX=0x84` (1P) / `0x20` (V.S.) | observed |
| **`0x190FE`** | seed both cursors: `CH`→`sprPLAY1SEL`/block `0x4523`, `CL`→`sprPLAY2SEL`/block `0x4527`; cell = `0x44C0[char/2]`, or 0 (P1) / 4 (P2) when the char is 0xFF | observed |
| `0x1915D` | allocate one cursor draw object: `[si+4]=sprite`, `[si+0x1A]=0x1010`, `[si+0x20]=block ptr`, `[di+1]=cell`, `[di+2]=[di+3]=0xFF` | observed |
| **`0x19185`** | cursor draw-object payload (per frame): if `[0x451D]==0xFF` and not locked, read `[key+4]`, move the cell with the table `0x4494`, store the char `[DI]=0x44B4[cell]`, and set the object x/y from base + `0x44CC` | observed |
| `0x1920E/15/1C/23` | the 4 cursor-move handlers (up/down/left/right), see §5.4 | observed |
| `0x19230` | **level load + V.S. match-up screen**: `0x19520` (level load), assets `0x19`, `palVSLOGO` + `sprVSLOGO` at `(0x40,0x88)`, two portraits (`0x1929E` with `DI=0x43E8`, `BP=0x3030`/`0x4040`), present, release, wait `0x15D83(5)` | observed |
| `0x1929E` | draw one fighter block: palette `[bx+0x4430]` (slot = low byte of `BP`), sprite `[bx+DI]`, `CALL 0x11507` name | observed (arg convention proven by `0x1433F`: `BX=char*2`, `BH=0`) |
| `0x17F49` | RNG (returns `AX`) | observed call sites |

### 2.4 Input layer

| Address | Purpose | Evidence |
|---|---|---|
| `0x12E40` | keyboard **ISR**: `IN AL,60h`; make ⇒ `[0x16D4+sc] |= 3`, break ⇒ clear bit1; skip if bit2 set | observed |
| `0x12EF3` | clear the 128-byte scancode table `0x16D4..0x1753` | observed |
| `0x12F11` | read+clear the "new press" bit of one scancode | observed |
| `0x12B14` | timer ISR → `0x12B74` → **`0x14D52`** (per-tick input poll) | observed |
| `0x14D52`,`0x14DBD` | poll both players: `CALL [device_word + 0x2F37]` (table at `DS:0x2F37`), latch button edges | observed |
| `0x14F21`/`0x14F26` | the two **keyboard** device handlers (P1 scancode table `0x2F1E`, P2 `0x2F2A`), build the 6-bit state (bits 1/2/4/8 = U/D/L/R, 0x10 = any button, 0x20/0x40 = buttons) | observed |
| `0x14D15` | menu direction reader: `AL = ([0x2F10]|[0x2F19]) & 0xF`, consumes (clears flag bit3) | observed |
| `0x14CE2` | menu confirm: bit0 = P1, bit1 = P2 (button *release* edge, `[key+6]` bit0), `AL=0x80` = ESC (`[0x16D5]`) | observed |
| `0x14CCA` | `0x14CE2`, else `AL=1` if `[0x16F0]` bit0 (any-key) | observed |
| `0x14D29` | reset both players' input state (`+6 &= ~3`, `+7=+8=+4=0`) | observed |
| `0x159FF` | F1 joystick-calibration overlay (logo + `0x33F7` texts, F1/F2/ESC) — entered from `0x1933B` when `[0x16D1]≠0` | observed |
| `0x14C64` | reload the key tables `0x2F1E/0x2F2A` from `DS:0x300F/0x301B` (the CONFIG key definitions) | observed |

---

## 3. Data tables (all DS-relative, little-endian)

### 3.1 Title menu

| Addr | Content | Notes |
|---|---|---|
| `0x43DE` | `0x454B, 0x4556, 0x4562, 0x456E, 0x0000` | item strings, walked until the 0 terminator (`0x192FB..0x1930C`) |
| `0x4537` / `0x453A` | `"1P"` / `"2P"` | select-screen player labels |
| `0x453D`…`0x456E` | `"PLAYER SELECT"`, `"GAME START"`, `"V.S. BATTLE"`, `"OPTION MODE"`, `"QUIT TO DOS"` (consecutive, 0-separated) | `PLAYER SELECT` = select header; the other four = the title items |
| `0x457A` | `"Press F1 to calibrate joystick."` | printed when `[0x16D1] ≠ 0` |
| `0x459A` | `" STAGE."` | appended to the V.S. character name |
| `0x62A` + `8·i` | font descriptors `{u16 handle, u16 mul, u8 dx, u8 dy, u8 base, u8 pad}`: #0 `fntCOLR0808` (8×8), #1 `fntCOLB0808`, #2 `fntCOLR1212`, #3 `fntNUMB1419` (base 0x30), #4 `fntCOLR1016` (10×16), #5 `fntDEBUG4X4` | font select = `BP = 0x4000|index` (0x4000 = spaces skipped, 0x8000 = outline mode) |
| `0x652` | row-offset table (runtime: `0x652[i] = i*0x140`, 200 entries) | used by the rect fill `0x1185C` |

### 3.2 Character select

| Addr | Content / size | Consumer |
|---|---|---|
| `0x431A` | 12 × u16 name strings (`"KEN"…"M BISON"`) per char | `0x18FAE`, `0x18E3A` |
| `0x4332` | 12 × u16 name length (3,5,7,3,7,7,7,6,6,4,5,7) | name centring `0x18E23` |
| `0x434A` | 12 × u16 name x-offset | `0x1905E` |
| `0x43E8` | **0-terminated handle list**: `sprFACE1<12>, sprFACE8<12>, sprFACE9<KEN..BLANKA 8>, sprFACE8<BALROG..BISON 4>` | `0x138A0` acquire, `0x1929E`, `0x19049` |
| `0x4430` | 12 × u16 `palFACE1<CHAR>` in char order | `0x19027`, `0x192B0` |
| `0x4448` | 13 × u16 `sprFLAG_*` in char order (USA,USA,JAP,JAP,USR,CHI,IND,BRA,USA,SPA,THA,THA) **plus** `sprFLAG_CRO` + `0x0000` | `0x18E9D`, `0x1906D`-region, `0x192A0` |
| `0x4464` | 12 × u16 `linFACE0<CHAR>` (32×32, normal icon) | `0x18ED3`, `0x18F35` |
| `0x447C` | 12 × u16 `linFACE2<CHAR>` (32×32, used/dimmed icon) | `0x18F43`, `0x18F00` |
| `0x4494` | **movement handler table** (CS offsets, index = `2·keybit`): `[1]=0x920E`(up) `[2]=0x9215`(down) `[4]=0x921C`(left) `[8]=0x9223`(right), all other entries `0x9184` = `RET` (no-op) | `0x191D1` |
| `0x44B4` | 12 bytes **cell → char\*2**: `06 04 0E 02 00 0A 08 0C 10 12 14 16` = RYU, HONDA, BLANKA, GUILE, KEN, CHUNLI, ZANGEIF, DHALSIM, BALROG, VEGA, SAGAT, BISON | `0x18A98`, `0x18E54`, `0x191EA` |
| `0x44C0` | 12 bytes **char → cell** (inverse of `0x44B4`): `04 03 01 00 06 05 07 02 08 09 0A 0B` | `0x18AC6`, `0x19118`, `0x1427A` |
| `0x44CC` | 12 × {u8 dx,u8 dy} grid offsets `(0,0)(0x20,0)(0x40,0)(0x60,0)(0,0x20)…(0x60,0x40)` | `0x18F49`, `0x191F7` |
| `0x44E4` | 12 × {u8 x,u8 y,i8 dx,i8 dy} **flag/marker position per char** (scattered over the map) | `0x18CEA`, `0x18D9D`, `0x18E5C` |
| `0x4514`… | state block, see §3.3 | |

### 3.3 Select state variables

| Addr | Meaning | Set / cleared |
|---|---|---|
| `0x4514` | **mode**: `0` = GAME START (arcade ladder), `0xFF` = V.S. BATTLE | `0x15031` / `0x14379` |
| `0x4515` | "cursor may not rest on a used cell" (strict mode; also a "redraw" gate) | `0x1430C`,`0x143B0` = 0xFF; `0x143BB`,`0x18A1C` = 0 |
| `0x4516` | V.S. "ready" sub-phase active (0xFF) | `0x18B56` = 0xFF; `0x18BF2` = 0 |
| `0x4517` | one-shot sound flag | `0x19035` = 0xFF, `0x190E3` = 0 |
| `0x4518` | word passed in `CX` by the caller: `CL` = P1 char\*2, `CH` = P2 char\*2 (0xFF = unknown) | `0x18956` |
| `0x4519` | high byte of the above (`CH`) | — |
| `0x451A` | **roster size** used by the CPU auto-pick and the flag loop: 8 (init) / 0xB / 0xC | `0x1894D`, `0x142AA`, `0x142B3` |
| `0x451B` | accepted confirm-key mask (`AND AL,[0x451B]`): 1 = P1 only (ladder), 3 = both (V.S.) | `0x18953` |
| `0x451C` | compared at `0x18EE5` only — **never written anywhere** (dead) | — |
| `0x451D` | "screen has been drawn once / input enabled"; the cursor objects only move when it is 0xFF | `0x142B7`/`0x143D1` = 0; `0x18A21` = 0xFF |
| `0x451E` | "re-use the previous opponent cell" (CONTINUE path) | `0x142D3` = 0xFF; `0x14373`,`0x18A26` = 0 |
| `0x451F` | cell resolved for the CPU player (last pick) | `0x18B2C` |
| `0x4520` | V.S. **stage cursor** (= char\*2, stepped by 2) and the highlighted map flag | `0x18BD2`, read `0x18DEE`,`0x18E1C`,`0x18B80` |
| `0x4522` | title menu: last direction byte (repeat suppression) | `0x19394`, `0x19335` |
| `0x4523`/`0x4527` | P1/P2 cursor block: `{+0 char*2, +1 cell 0..11, +2 last drawn cell, +3 lock (0 = locked, 0xFF = free)}` | `0x19170`, `0x18B37`… |
| `0x452B` | 12 used-cell flags: `0` free, `0xFF` used | `0x1893F`, `0x1427E`, `0x142EE` |
| `0x2A2C`/`0x2A32` | battle records `{+0 char*2, +1 state, +2 wins, +3 ?, +4 bar/colour, +5 indicator 0..7}` | `0x14850`, `0x14857`, `0x14877` |
| `0x2A2D`/`0x2A33` | record state: `2` = this player lost the bout | `0x14183`, `0x1418F` |
| `0x2A2E`,`0x2A34`,`0x2A38`,`0x2A39`,`0x2A3A`,`0x2A3E` | bout/round/counter bytes (fight-engine §3.3) | — |
| `0x2A3B` | **level/stage id** = opponent char\*2 (or the V.S. stage cursor) | `0x14112`, `0x143DB` |
| `0x2A3C` | **human-player mask**: bit0 = P1 human, bit1 = P2 human. It is the mask `0x18953` returns (1 = P1 confirmed, 3 = both; GAME START normally yields 1, V.S. BATTLE 3). Note a title confirmed with P2's keys propagates as 2 (the select screen then accepts P2's confirm) — reproduce as-is | `0x14116`, `0x143DF` |
| `0x2F0C`/`0x2F15` | per-player key struct: `{+0 device word, +2?, +4 direction/button state, +5 last, +6 flags: bit0 = button released latch, bit1 = held, bit2, bit3 = "state consumed", +7 raw, +8 button accumulator}` | §2.4 |
| `0x2F08`/`0x2F0A` | per-player device id → handler `[id + 0x2F37]`: 0 = none, 2/4 = analog axes (`0x16A7`/`0x16BD`), **6 = keyboard P1**, **8 = keyboard P2**, 10… = further sources | `0x14BE6`, `0x2F37` |
| `0x2F1E`/`0x2F2A` | 6 × {u8 scancode, u8 bit} menu keys. Defaults: P1 `48,50,4B,4D,39,1C` (↑↓←→ Space Enter) / P2 `10,1E,2C,2D,02,03` (Q A Z X 1 2) | `0x14F21`, reloaded by `0x14C64` from `0x300F`/`0x301B` |
| `0x16D4` | 128-byte per-scancode state (bit0 = new press, bit1 = held, bit2 = ignore) | ISR `0x12E40` |
| `0x3029` | `0xFF` = "loading/busy" → the input poller is skipped (`0x14DBD`) | `0x14D29`/`0x14C42` |
| `0x3230` / `0x3231` | cheat flags: `"3months"` and `"jenniemarie"` typed on screen (`0x15E8D` compares `0x32D3`/`0x32DB`) | `0x3231` enables the in-fight debug keys (`0x15202…`: F1 ⇒ `[0x3BBD]=100`), `0x3230` switches the attract selector and relaxes the select cursor rule |

---

## 4. Title menu decoding (`0x1933B`)

```
1933B  push bx
1933C  call 0x14f15            ; (a RET in this build — reserved hook)
19340  call 0x14d15            ; AL = menu direction (P1|P2) & 0xF, consumed
19343  je   0x19394            ; ZF: no direction → no cursor move
19345  cmp  al, [0x4522]
19349  je   0x19394            ; same as last frame → ignore (repeat suppression)
1934D  test al,1 ; jne 0x19360 ; UP    → BX -= 2
19351  test al,4 ; jne 0x19360 ; LEFT  → BX -= 2
19355  inc bx ; inc bx         ; DOWN/RIGHT/anything else → BX += 2
19357  cmp  bx,8 ; jb 0x19369
1935C  xor  bx,bx              ; 6 → 0 wrap
19360  or   bx,bx ; jne 0x19367
19364  mov  bx,8 ; dec bx ; dec bx   ; 0 → 6 wrap
19369  ... xchg word [bp],bx   ; stash the new index, BX := caller's old index
1936E  shl bx,1 ... 0x84+6·item      ; rect fill: 0x1185C(AX=0x60, row=0x84+6·item, CX=0x10, DX=0x10)
19389  mov  dx, word [bp]      ; DX = new item index
1938C  call 0x193bb            ; redraw the cursor sprite at (0x60, 0x84+6·item)
1938F  call 0x11bba            ; present the frame
19392  pop bx                 ; ← RETURN: BX = item index (0,2,4,6)
19393  pop ax
19394  mov [0x4522], al        ; remember the direction byte
19397  cmp byte [0x16d1],0 ; je 0x193b7
1939F  ... if ([0x170F]&1) else fallthrough: {  call 0x159ff (F1 calibration)   ;  AL=0x80, SF=1 → RET }
193B7  call 0x14cca            ; AL = confirm (bit0 P1 / bit1 P2 / 1 any-key); ZF=1 = nothing
193BA  ret
```

Return protocol (consumed by `0x1500B..0x1501F`):

| result | flags | meaning |
|---|---|---|
| no input | `ZF=1` | keep looping |
| direction moved | `ZF=1` | keep looping (only the cursor changed) |
| confirm | `ZF=0`, `SF=0`, `AL≠0`, `BX` = item | run `CALL CS:[BX+0x5029]` |
| ESC or F1 | `SF=1` | `0x15015 JS 0x15024` → the title menu returns `AL=0` to the sequencer |

Notes / quirks:
* only `0x01` (up) and `0x04` (left) move the cursor *up*; every other recognised direction
  (`0x02` down, `0x08` right) moves it *down* — this needs no special handling in a port if the
  same rule is used, but it is not the "natural" mapping.
* cursor y = `0x84 + 6·item` (`132,138,144,150`) while the item text is printed at
  `0x88 + 0xC·item` (`136,148,160,172`) with 8×8 glyphs — the two spacing schemes disagree;
  the port must reproduce the code, and the visual result should be confirmed against a running
  copy (see Unknowns).
* `sprSELETCUR` is 16×16 (`sprSELETCUR.bin`: `10 10` header) and the fill `0x1185C` also uses
  16×16, so the intended widget is a 16×16 block; the 6-px step makes consecutive positions
  overlap.

---

## 5. Character select (`0x18953`)

### 5.1 Entry / exit contract

* **In**: `AL` = accepted confirm mask (`[0x451B]`), `CX` = character hints (`CH` = P1 char\*2,
  `CL` = P2 char\*2 or 0xFF).
* **Out (normal)**: `SF=0`, `AL` = human mask (1 = P1 only, 3 = both), `CH` = P1 char\*2,
  `CL` = P2 char\*2, `DH`/`DL` = the two char\*2 (1P: `DH` = P1's pick, `DL` = the CPU's pick;
  V.S.: `DL` = the stage cursor `[0x4520]`).
* **Out (abort)**: `JS` taken at the caller (`0x142C2`, `0x143C0`, `0x1504C`), reached through
  `0x18A2E`/`0x13880` leaving `AL` with bit7 set? — **observed**: the only abort path sets
  `AL = 0x80` inside `0x14CCA`→returns to the loop; the loop's `JS 0x18A1C` jumps to the
  *normal* tail. The abort signal the callers test is therefore produced by the tail sequence
  (`0x190C2 → 0x13880 → OR AL,AL; RET`); `0x190C2` preserves `AL`, so **the SF tested by the
  callers comes from the last `AL` computed in the loop** (0x80 after ESC) — treat
  "ESC → SF=1 → abort" as the contract (derived; the exact byte path through `0x13880` was not
  single-stepped).

### 5.2 Init and per-frame order

```
18953  mov [0x451b],al ; mov [0x4518],cx           ; keep the mask + hints
1895E  ax=0x19; 0x11A4D                            ; load asset group 0x19
18964  0x13ACB
18969  call 0x190FE                                 ; seed both cursors (0x4523/0x4527)
1896C  palSELECMAP  @ DAC 0x70 (0x90 entries)
1897B  palPLANE000  @ DAC 0x50 (0x10)
1898A  palFLAG_CRO  @ DAC 0x60 (0x10) and 0x70 (0x10)
1899C  0x1199C                                      ; push the palette table to the VGA DAC
189A2  acquire 0x43E8 (faces), 0x4430 (palettes), 0x4448 (flags)
189B4  0x18D34   ; map + "PLAYER SELECT"
189B7  0x18DC1   ; flags / used markers
189BA  0x18EC0   ; 32x32 icon grid
189BD  0x1907D   ; allocate the two 88x112 portrait buffers
189C0  0x18F68   ; portraits + labels
189C3  0x13B6C   ; draw objects
189C6  0x11BBA   ; present
189C9  0x119E1   ; release group 0x19
189DC  if ([0x451D] != 0xFF) goto 0x18A0A        ; first frame: no input handling
189E3  loop:
         0x13BA3                 ; update the draw objects (cursor hands!)
         0x18F68                 ; portraits/labels (+0x18DC1 inside)
         0x13B6C                 ; draw the objects
         0x14F15 ; 0x11959       ; retrace wait
         0x190D3                 ; one-shot sound
         0x11BBA                 ; present
         0x14CE2  ; JS quit ; AND AL,[0x451B] ; JE loop
         0x18A34  ; CF → loop
18A08  AL = DL (confirm mask) → 0x18A9D → 0x18B4C → 0x18D6F → 0x18DC1 → 0x11BBA → 0x18C44
18A1C  [0x4515]=0 ; [0x451D]=0xFF ; [0x451E]=0 ; 0x190C2 ; 0x13880 ; OR AL,AL ; RET
```

### 5.3 The two phases of a pick

**Phase A — the human player.** The `sprPLAY1SEL`/`sprPLAY2SEL` draw objects move the cursor
blocks (`0x19185`): read `[key+4]`, take bit1/2/4/8 (up/down/left/right), dispatch through
`[keybit*2 + 0x4494]`, store the resulting cell in `[block+1]` and the cell's char in
`[block+0]`. A button (bits `0x20`/`0x40`, surfaced as `0x10` by `0x14F21`) is *not* handled
here — it is caught in `0x18A6B`:

```
18A6B  bl = [di+1]              ; this player's cell
18A72  if bl == 0xFF → ret      ; no cursor yet
18A77  if [0x4515] != 0xFF → ok ; strict mode: reject cells with 0x452B == 0xFF
18A8E  test ah, al              ; AH = this player's bit (1/2), AL = the read key byte
18A92  [di+3] = 0               ; button over a valid cell ⇒ LOCK this player
18A96  dl |= ah                 ; DL = confirm mask
18A98  cl = [bx + 0x44B4]       ; CL = char*2 of that cell
```

In 1P mode `0x18A34` always clears CF, so **the first confirm event ends the loop**
(`0x14CE2` bit0, then `AND AL,[0x451B]` where `[0x451B]` is the mask the caller passed in: the
title's confirm byte for the first GAME START call, `[0x2A3C]` for the ladder re-select). In V.S.
mode both `[0x4526]` and `[0x452A]` must be 0 first (`STC` while either is 0xFF).

**Phase B — the CPU player (`0x18A9D`).** With `AL != 3` (1P), the missing participant is
resolved:
* `[0x451E] == 0xFF` (CONTINUE): try to re-use the previous opponent cell `[0x451F]`
  (`0x18AA6..0x18AE7`); if that cell is now used or equals either player's char, fall through to
  the random path.
* `[0x451A] != 8` (roster expanded): `AL = 7; do AL++ until [AL+0x452B] != 0xFF` → **the first
  boss cell**, then `AL = 0x44B4[AL]`.
* otherwise random:
```
18B03  call 0x17f49 ; and al,0xF
18B08  if [0x451a] == 8 → and al,7
18B11  if al >= [0x451a] → retry      ; keep inside the roster
18B17  bl = al
18B19  if [bx+0x452b] == 0xFF → retry  ; cell already used
18B20  al = [bx+0x44b4]                ; char*2 of the picked cell
18B24  if al == cl → retry ; if al == ch → retry   ; not the same as either player
18B2C  [0x451f] = bl                   ; remember the CPU's cell
18B30  if cl != 0xFF → { ch = al ; [0x4523] = cl ; [0x4527] = al }
       else            { cl = al ; [0x4523] = al ; [0x4527] = ch }
```

**Phase C — finish (`0x18B4C`).**

```
18B4C  if [0x4514] != 0xFF → 0x18BE8   ; 1P
18BE8  dh = ch ; dl = cl
18BEC  if !(al & 1) → xchg dl,dh       ; al = the confirm mask from phase A/B
18BF2  [0x4516] = 0 ; ret              ; ⇒ DH/DL = the two char*2 values
…
18B56  (V.S.) [0x4516] = 0xFF ; 0x18DC1 ; 0x18BF8 ; retrace ; present
18B67  BX=0x2F0C,DI=0x2A2C → 0x18B8F   ; P1: move the record indicator (up/down, 0..7)
18B72  BX=0x2F15,DI=0x2A32 → 0x18B8F   ; P2
18B7D  0x18BBF                          ; left/right moves [0x4520] (±2, 0..0x16)
18B84  0x14CE2 ; JS → done ; JE → loop   ; a button (either player) or ESC ends it
18B8B  al = 3                          ; both players are human
18BF2  [0x4516] = 0
```

### 5.4 Cursor movement handlers (`0x1920E..0x1922F`) — 4 × 3 grid

```
1920E  cmp al,4 ; jb ret ; sub al,4      ; UP    : row-1
19215  cmp al,4 ; jae ret ; add al,4     ; DOWN  : only from the top row (!)
1921C  test al,3 ; je ret ; dec al       ; LEFT  : column-1
19223  ah = al & 3 ; cmp ah,3 ; je ret ; inc al   ; RIGHT : column+1
```

* `AL` = the current cell, the handler returns the new cell.
* **`DOWN` is limited to the top row** (`AL < 4`); from the middle row (4..7) DOWN does nothing
  and the bottom boss row (8..11) is normally reached only by the CPU auto-pick or a cell-8..11
  start. This looks like a bug (`cmp al,8` would be the natural bound for 12 cells) but it is
  what the shipped code does — a faithful port must keep it (or make it an explicit option).
* the last column test for RIGHT uses `AL & 3 == 3`, the first column test for LEFT `AL & 3 == 0`,
  so left/right never wrap between rows.

---

## 6. Flow to reproduce

### 6.1 Title → menu item → character select → fight start

```
entry 0x104E6 → 0x14F4F (init) → 0x14FB9 (sequencer)
   sequencer loop: 0x1979C, 0x19CD6, 0x1719F (intro), 0x17413 (stub),
                   0x15CFC (credits, key → done), 0x15DE8 (drugs picture),
                   0x152FE (demo fight), 0x15E4A (attract stage)
      any screen returning NZ → 0x14FAF: [0x3232]=0 → 0x14FF0 (title menu)
0x14FF0  → 0x192E4 draws the title, 0x1933B handles the cursor (BX = 0/2/4/6)
         on confirm: 0x13A81, 0x12B0E, CALL CS:[BX+0x5029]

BX=0 (GAME START, 0x15031)
   0x4514 = 0                                  ; ladder mode
   0x3BAF/0x3BB1/0x3BD2/0x3BD4 = 0             ; score/HUD scratch
   0x19520                                     ; level-load routine (retries x3)
   CALL 0x18953  (AL = the title confirm mask → [0x451B], normally 1)
        → character select (§5); human picks one of the 8 base fighters
        → 0x18A9D picks the CPU's fighter (random, free cells < [0x451A])
        → returns CH = P1 char*2, CL = P2 char*2, DH/DL = both, AL = 1
   if abort → return
   0x323D = 0xFF
   CALL 0x19230   ; loading screen + V.S. portraits (sprFACE1* / palFACE1*)
   CALL 0x140F9   ; match driver
        0x14112  [0x2A3B] = DL (level = the CPU's char*2)
                 [0x2A3C] = AL (human mask = 1)
                 record 0x2A2C ← CH (P1), record 0x2A32 ← CL (P2/CPU)
        0x14129  [0x2A3A] = 0 → INC (round 1) ; INC [0x2A38]
        0x14136  0x14857 (bar/colour bytes into the fighter structs' +0x1C)
        0x14148  CALL 0x14573 → CALL 0x15083 (round 1)
                    CX = chars (CL = P1 char, CH = P2 char |0x80 = CPU)
                    DX = level id → 0x15083: push cx/dx, 0x14C04 (control remap),
                         0x15357, acquire 0x4F54, 0x14084, 0x13ACB, 0x1212A, 0x11584,
                         POP AX → AL = level → 0x13F28 (stage setup),
                         POP AX → CX = chars → XCHG → 0x17416 (build the fighters) …
   after the round: 0x14573 evaluates both energies, 0x1474B (winner display),
        then the ladder logic (§6.2).

BX=1 (V.S. BATTLE, 0x1506B)
   CALL 0x14379: 0x4514 = 0xFF, clear 0x2954/0x2994, [0x291C] = 0xFFFF,
                 AL = 3 (both confirms), CX = 0xFFFF (no character hints)
                 CALL 0x18953  → V.S. select (both cursors, both locks)
                 run → DH/DL/CH/CL as above, DL = the stage cursor
                 [0x2A3B] = DL, [0x2A3C] = AL(3), records ← CH/CL, 0x14573 rounds
BX=2 (OPTION MODE, 0x1506F)
   CALL 0x153C4 → the option screen; then JMP 0x14FF0 (title menu redrawn)
BX=3 (QUIT TO DOS, 0x15076) → save high scores, exit to DOS
```

### 6.2 Ladder: next opponent

```
after a bout (0x141D6): win count of the winner += 1
   if the winner is not human (AH & [0x2A3C] == 0) → 0x142D0 CONTINUE path:
        0x14993 (current score/ranking), [0x451E] = 0xFF, 0x14A18 "CONTINUE ?" (YES/NO + countdown)
        accepted → 0x14353 → keep the fight (same opponent, same stage)
   human won:
        wins == 3/6/9 → DX = 0x18/0x1A/0x1C → 0x15083 (bonus stage, no win counting)
        0x1474B (winner + bonus display), 0x1424B 0x15083 with the next round
        then 0x1425C … 0x14264: BL = the loser's char*2
        0x14273: [0x2A39] = 0 ; mark the loser's cell used: [0x44C0[loser/2] + 0x452B] = 0xFF
                 count the free cells AL (skipping the winner's own cell)
        0x142AA: AL >= 7 → [0x451A] = 0xB ; AL == 10 → [0x451A] = 0xC
        0x142B7: [0x451D] = 0
        0x142BC: AL = [0x2A3C] (human mask) → CALL 0x18953  ; select the CPU's NEXT fighter
        0x142C7: 0x19230 (VS screen) → 0x14112 (next match, new CH/CL and level)
   abort / end of ladder → 0x14370: 0x1893F (clear the used cells, [0x451A] = 8), [0x451E] = 0, RET
```

So: **round 1 of any match** is `0x14148 → 0x14573 → 0x15083` (or the V.S. driver's `0x14411`),
with `CX` = the two characters and `DX` = the level (`= opponent char*2`, bonus `0x18/0x1A/0x1C`).

### 6.3 Assets to load for the select screen

| Asset | Size | Drawn / used as |
|---|---|---|
| `linSELECMAP` (`0xF33C`) | 256×128 raw | map background at `(0x20,0)` (`0x11E9B` 0x100×0x80) |
| `palSELECMAP` (`0xF2B1`) | 9×48 B | DAC entries `0x70..0xFF` (`0x11970(…,0x70,0x90)`) |
| `palPLANE000` (`0xF2B2`) | | DAC `0x50`, 16 entries |
| `palFLAG_CRO` (`0xF2B9`) | | DAC `0x60` and `0x70`, 16 entries each |
| `sprFLAG_*` (12) + `sprFLAG_CRO` | 32×32 each | flags on the map (1P) / 2×6 grid (V.S.); `sprFLAG_CRO` marks used cells |
| `linFACE0*` (8…12) | 32×32 raw | icon grid (`0x4464`) |
| `linFACE2*` (8…12) | 32×32 raw | used/locked icon variant (`0x447C`) |
| `sprFACE1*` (12) | 128×128 | big portrait (`0x43E8[char*2]`), DAC slot `0x30`/`0x40` |
| `palFACE1*` (12) | | portrait palette (`0x4430[char*2]`) |
| `sprPLAY1SEL` (`0xF2DB`) / `sprPLAY2SEL` (`0xF2DA`) | 32×40 | the two movable player markers |
| `sprSELETCUR` (`0xF2D9`) | 16×16 | **title-menu** cursor |
| `linSF2BLOGO` (`0xF33B`) + `palSF2BLOGO` (`0xF2B0`) | 288×128 | SF2 logo, at `(0x10,0)`, used by the title, credits and option screens (`0x15DC3`) |
| `sprPLANE000..007` (`0xF2DC..0xF2E3`) | 32×32 | V.S. plane animation (`0x18C44`) |
| `sprVSLOGO` (`0xF2CC`) + `palVSLOGO` (`0xF2AC`) | 192×64 | V.S. screen logo at `(0x40,0x88)` |
| `fntCOLR0808` (`0xF428`) / `fntCOLR1016` (`0xF427`) | 8×8 / 10×16 | menu/name text / screen titles, `"PLAYER SELECT"` |

### 6.4 Key assembly quotes for the port

Select input loop gate (per frame):

```
189F8  call 0x14ce2            ; AL: bit0 P1 button, bit1 P2 button, 0x80 ESC
189FB  js   0x18a1c            ; ESC ⇒ leave
189FD  and  al, [0x451b]       ; only the accepted confirms count
18A01  je   0x189e3            ; nothing ⇒ loop
18A03  call 0x18a34
18A06  jb   0x189e3            ; not all locks satisfied ⇒ loop
18A08  mov  al, dl             ; AL = confirm mask
```

Palette/portrait switch when the cursor enters a new character (`0x18FAE`):

```
18FCB  call 0x18f1e   ; CLC: restore the previous cell's icon
18FD4  call 0x18f1e   ; STC: highlight the new cell (linFACE2* if used)
18FD7  bl = [di+1] ; [di+2] = bl      ; remember the drawn cell
18FDF  bl = [bx+0x44b4] ; [di] = bl   ; block char*2
19027  si = [bx+0x4430] ; bx = bp (0x3030/0x4040) ; xor bh,bh ; cx=0x10
19032  call 0x11970                   ; 16 palette entries from palFACE1* into slot 0x30/0x40
19049  si = [bx+0x43e8] ; call 0x13daa ; blit sprFACE1* (or FACE8/FACE9 for the VS screens)
19051  … then 0x11507 prints the character name (0x431A) at CX + 0x434A[char]
```

Stage/level hand-off (the only place `DS:0x2A3B` is consumed):

```
14139  mov  dl, [0x2a3b]        ; DL = level id
1413D  mov  al, [0x2a3c]        ; AL = human mask
14140  mov  ch, [0x2a2c] ; mov cl, [0x2a32]
14148  call 0x14573             ; → 0x15083 (CX = chars, DX = level)
150BC  pop  ax ; xor ah,ah ; call 0x13f28    ; stage setup with AL = level
150C2  pop  ax ; xchg ah,al ; call 0x17416   ; build both fighters
```

---

## 7. Corrections / additions to `fight-engine.md`

1. **§3.2 dispatch table**: entry 1 (`0x506B`) is `CALL 0x14379` = V.S. BATTLE (sets
   `0x4514 = 0xFF`), entry 2 (`0x506F`) is `CALL 0x153C4` = OPTION MODE (then re-enters the
   title menu with `POP AX; JMP 0x14FF0`), entry 3 (`0x5076`) is QUIT TO DOS (exits). The "V.S.
   BATTLE placeholder" and "fight cluster `0x1437D`" descriptions are replaced by the above.
2. **§3.3 / §5.1**: `DS:0x2A3B` = the **level/stage id** (opponent `char*2`; bonus `0x18/0x1A/0x1C`)
   and `DS:0x2A3C` = the **human-player mask** (bit0 P1, bit1 P2) — *not* the two selected
   characters. The characters travel in `DS:0x2A2C`/`DS:0x2A32` (records, `+0`) and in
   `CH`/`CL`. `DS:0x2A2C+5` is a per-player 0..7 indicator used by the V.S. screen
   (copied to the fighter struct's `+0x1C` by `0x14877`); `+4` is a bar/colour value.
3. **§7 module table**: `0x12AF2`/`0x12B0E` are **not** "the menu framework" — they set/stop a
   `AX*25`-tick auto-advance timeout (`DS:0x168E/0x169D/0x169E`). The menu framework is
   `0x192E4` + `0x1933B` (title) and `0x18953` (select), on top of the input layer of §2.4.
4. `PLAYER SELECT` is the character-select screen's header (`0x18D34`), confirming the
   inference in §3.2's note: it is a sub-screen entered from GAME START and V.S. BATTLE.
5. `0x15CFC` = copyright/credits text screen, `0x15DE8` = the `linDRUGSPIC` warning picture,
   `0x15E4A` = an attract stage/message segment, `0x152FE` = the random attract **fight**.
6. `DS:0x3230`/`0x3231` are the `"3months"`/`"jenniemarie"` cheat flags (typed-code comparator
   `0x15E8D`, strings `0x32D3`/`0x32DB`); `[0x3231]` enables the in-fight energy debug keys and
   also gates part of `0x15083`'s key handling.

---

## 8. Confidence and unknowns

**High confidence (observed in code/data)**

* Title menu: draw (`0x192E4`), item list (`0x43DE`), cursor (`0x1933B`/`0x193BB`), dispatch
  table (`0x15029`) and all four targets, timeout via `0x12AF2`.
* Sequencer order, per-screen return convention (`NZ` → title menu), and the identity of all
  seven screen functions.
* Select screen: layout, the three asset lists (`0x43E8`/`0x4430`/`0x4448`), the icon tables
  (`0x4464`/`0x447C`), all four cell/char mapping tables (`0x44B4`/`0x44C0`/`0x44CC`/`0x44E4`),
  the cursor blocks, the used flags, the CPU auto-pick and the CONTINUE re-use, the
  per-frame order, and the 4 movement handlers *including* the `DOWN`-from-top-row-only quirk.
* The characters/level hand-off `DL/AL/CH/CL → 0x2A3B/0x2A3C/0x2A2C/0x2A32 → 0x14148 → 0x14573
  → 0x15083` and the level → stage chain (`0x13F28`, `0x25BE = level`, `0x2646`/`0x25CC` are
  15-entry tables indexed by `2·level`).
* Input layer: ISR scancode table, timer-tick poller, device table `0x2F37`, menu readers
  `0x14D15`/`0x14CE2`/`0x14CCA`, the P1/P2 default keys and their CONFIG source
  (`0x300F`/`0x301B`).

**Medium confidence / inferred**

* The *visual* result of the title cursor geometry (6-px step vs. 12-px text step, 16×16 sprite)
  and of the select screen's flag/marker placement — derived from the position tables but not
  confirmed against a running copy (no screenshots available here).
* `[0x451A]`'s roster-size arithmetic: the code counts *free* cells and then sets 0xB/0xC; the
  author's intent ("expand the roster as the ladder shrinks") is inferred; the observed effect is
  that the boss cells become auto-pick-eligible right after the first win.
* The `0x18953` "abort" flag: the callers test `SF` after `0x190C2`/`0x13880`; `0x190C2`
  preserves `AL`, so the ESC value `0x80` set earlier in the loop is what flips `SF` — not
  single-stepped.
* OPTION screen (`0x153C4`): icon-based (7 cursor positions `0x10..0x70`, `sprICONCURS`,
  `palICONEASY/palICONJOY1/…`, config bytes at `0x308E`/`0x320C`/`0x323B/0x323C`) — only the
  skeleton was decoded; item-by-item semantics are **not** traced.
* CONTINUE screen `0x14A18` (portrait of the loser, `palGENERAL`, `"CONTINUE ?"`, NO/YES,
  countdown) — entry points and purpose known, key/DEC logic not decoded.

**Unknown / to be traced**

* `0x19230`'s character argument: `0x1929E` is proven to take `BX = char*2` (with `BH = 0`) from
  `0x1430C`/`0x1433F`, but `0x19230` itself does `POP BX; MOV BL,BH` after `MOV BH,BL` (BH = the
  VS-logo palette slot `0x60`) — either the argument convention differs there or this is a bug.
  Needs a runtime check (or a single-step) before the V.S. screen can be ported exactly.
* The V.S. "ready" sub-screen's up/down cursor (`record +5`, range 0..7, copied to the fighter's
  `+0x1C` by `0x14877`) — the in-game effect is not traced; likely a colour/indicator selector.
* `0x451C` is never written (the compare at `0x18EE5` is dead).
* `0x44E4`'s per-char `dx/dy` offsets: used for both the flags and the markers, but whether they
  are a "slide-in" animation or a static correction is not proven (they are only ever read
  immediately after `0x11959`/present, so a per-frame animation is *possible*; both call paths
  use them identically).
* `0x4494`'s unused entries (`0x9184` = a bare `RET`) — no-op by construction.
* The 8-icons-vs-12-cells question: `0x190E9` returns `AL = 8`, so the base grid draws only two
  rows; the bottom (boss) row's icons are drawn only when a cursor highlights them
  (`0x18F1E`). Confirm visually.
* `0x16D1` (F1-calibration availability) — set by the joystick/config code (`0x12C43`,
  `0x14D54`/`0x15B91`), not traced in detail.
* `" STAGE."`/name layout in V.S. mode and the `0x4520` → level conversion are observed; the
  *page* semantics (which of the two 0x58×0x70 buffers is on screen when) is not.
