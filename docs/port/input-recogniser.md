# Input decoding and the special-move recogniser (`0x14E1B`)

Target: `SF2.EXE` (MS-DOS 1995), same conventions as `action-execution.md` / `fight-engine.md`:
`linear = file_offset + 0xFFA0`, code/CS base linear `0x10000` (CS offset `o` ⇔ linear `0x10000+o`),
DS base linear `0x1AA40` (DS offset `d` ⇔ linear `0x1AA40+d`), little-endian.
Evidence classes: **observed** (bytes read), **derived** (reproducible), **inferred**, **UNKNOWN**.

This file finishes the three open items of `action-execution.md` §4.4/§8.4–8.5:
the recogniser `0x14E1B`, the attack path at `0x107D9`, and the raw input byte.

---

## 1. Summary (what the shipped code actually does)

1. **The eight bytes per character at `DS:0x78` are not patterns — they are four pointers.**
   `0x10800..0x10807` computes `BX = 0x78 + char*8`; the recogniser (`0x14E2E`) does
   `mov si,[si]` for four successive words of that block, i.e. `ptr[i] = word[DS:0x78+char*8+2i]`
   (`i = 0..3`). Each non-null pointer addresses a *condition block* in DS:
   `{word flags, element…, word 0 terminator}`. The block index `i` is the recogniser's result
   and becomes **action `0x34+i`**.
2. A **condition block element** is 4 bytes: `{byte v, byte f, byte btn, byte pad}`:
   * `btn` = the input byte to match (bit `0x80` = "mask" mode: require any common bit with
     `btn & 0x7F` instead of equality);
   * `v,f` = a constraint on the *run length* of the matched input: `f & 0x01` ⇒ `run_len > v`,
     `f & 0x02` ⇒ `run_len <= v`. `f == 0` is an **assert/abort** path (see §4.5) — unreachable in
     the shipped data.
3. Input is sampled by a **timer-interrupt handler** and folded into a per-player record
   (`0x2F0C`/`0x2F15`) plus a **16-entry ring of "input runs"** (`0x2F55`/`0x2F9F`) holding
   `{word run_length, byte input}` — that ring is what the recogniser matches against.
4. Block `flags & 0x8000` selects the "gap tolerance" mode:
   * `0x8000` clear → `[0x3028] = 0xFF` ⇒ runs whose input byte is `0` are *skipped* while matching
     (allowed only if their run length `<= 6`);
   * `0x8000` set → `[0x3028] = 0` ⇒ no skipping (used by the button-mash blocks).
5. The four action slots per character are `0x34..0x37` (the same ids the hit code treats as
   "attacks", §5.2 of `action-execution.md`): the recogniser's `CL` selects the slot, and the
   `ait` table then picks the script (that is why RYU/KEN's fireball spawn hook keys on
   action `0x35` = block index 1).
6. The **normal-attack path** (recogniser miss, CF clear) does **not** use `CL`: it computes a
   table index from `player[0x0A] & 3` (posture), `DS:0x4E` (distance bucket), the input class and
   the punch/kick bit, and returns the byte at that index in a **per-character resource**
   (directory handle `DS:0x60[char]`, see §5.2/§5.4). Those resource bytes are **not in `SF2.EXE`**
   (loaded at run time, optionally through EMS) — see §9.
7. Raw input byte (`AL` from the device handler, `[BP+4] | [BP+8]` in the fighter):
   `bit0 up`, `bit1 down`, `bit2 left`, `bit3 right`, `bit4 "attack pressed/held"`,
   `bit5 punch`, `bit6 kick`, `bit7 unused` (all observed, §2.1).

---

## 2. Raw input byte

### 2.1 Bit table

| bit | meaning | evidence |
|---|---|---|
| `0x01` | up | `DS:0x2F1E` default key map: scancode `0x48` (up arrow) → `0x01`; `INPUT_CLASS[1] = 0x00` (class "up"); recogniser patterns use `0x01` for "up" (GUILE kick-charge: `down>100` then `0x01` then kick) |
| `0x02` | down | `DS:0x2F1E`: `0x50` (down arrow) → `0x02`; QCF/QCB patterns start with `0x02` |
| `0x04` | left | `DS:0x2F1E`: `0x4B` → `0x04`; QCB pattern uses `0x04` as "back" when facing right |
| `0x08` | right | `DS:0x2F1E`: `0x4D` → `0x08`; QCF pattern uses `0x08` as "forward" |
| `0x10` | attack button held (any) — also the menu "start/fire" signal | `0x14F3E..0x14F46`: `if (mapped_byte & 0x60) or al,0x10`; joystick `0x12DC6/0x12DD0`: any pad button ⇒ `or bl,0x10`; decoder `0x10781: test al,0x10 / jne 0x107d9` (= "attack pressed"); poller `0x14DD9` uses it for the `[di+6]` bit1 "button held" latch read by `0x14CE2` (menus) |
| `0x20` | punch | key map ds:0x2F1E scancode `0x39` (space) → `0x20`; recogniser masks `0xA0` (`0x80|0x20`); normal path `test al,0x20` |
| `0x40` | kick | key map scancode `0x1C` (enter) → `0x40`; recogniser masks `0xC0`; normal path |
| `0x80` | unused by any shipped device mapping | no key map entry or pad conversion sets it; decoder masks it off (`0x1077D and al,0x7f`) before testing `0x10`; recogniser *does* see it (raw history byte) |

Valid direction nibbles (low 4 bits) and their `INPUT_CLASS` values (`DS:0x2FDF`, `0x10763`):

| nibble | bits | dir | class |
|---|---|---|---|
| `0` | – | neutral | `0xFF` → treated as `0` |
| `1` | up | up | `0` |
| `2` | down | down | `1` |
| `4` | left | left | `2` |
| `5` | up+left | up-left | `3` |
| `6` | down+left | down-left | `4` |
| `8` | right | right | `5` |
| `9` | up+right | up-right | `6` |
| `A` | down+right | down-right | `7` |
| `3,7,B,C,D,E,F` | invalid | – | `8` (= stand) |

### 2.2 Where the byte comes from — device table `DS:0x2F37` (observed)

`0x14DBD` → `0x14DD5: call word ptr [bx+0x2F37]`, `bx = word[DS:0x2F08]` / `word[DS:0x2F0A]`.
The table holds **CS offsets** (10 entries, indexed by even selector values):

| selector | target (CS:off → linear) | what it returns |
|---|---|---|
| 0 | `0x4F16` → `0x14F16` | `xor al,al; ret` (no device) |
| 2 | `0x4F19` → `0x14F19` | `mov al,[0x16A7]` = **joystick 1 record byte** (`record 0x16A4`, field `+3`) |
| 4 | `0x4F1D` → `0x14F1D` | `mov al,[0x16BD]` = **joystick 2 record byte** (`record 0x16BA`, field `+3`) |
| 6 | `0x4F21` → `0x14F21` | **keyboard key-set 1**: for the 6 `{scancode, byte}` pairs at `DS:0x2F1E`, OR in `byte` if that scancode is held (key state `[scancode+0x16D4]` bit1); if `byte & 0x60` also OR `0x10` |
| 8 | `0x4F26` → `0x14F26` | same for **keyboard key-set 2** using `DS:0x2F2A` |
| `0xA` | `0x306A` → `0x1306A` | extra device modes (see §8 unknowns) |
| `0xC` | `0x302C` → `0x1302C` | idem |
| `0xE` | `0x3039` → `0x13039` | idem |
| `0x10` | `0x3046` → `0x13046` | idem |
| `0x12` | `0x3058` → `0x13058` | idem |

* Selector assignment — `0x14B65..0x14BE5` (observed), driven by `[0x16D1]` (joystick detection,
  set by `0x12BFC..0x12C29` using the game-port reader `0x12C86`):
  `[0x302B] = 0x0C | joystick_bits`; then
  `[0x2F08],[0x2F0A] = (2,4)` both joysticks · `(2,8)` joystick 1 only · `(4,8)` joystick 2 only ·
  `(6,8)` no joystick (both keyboard key-sets). The same routine sets `bit 0x04 of the record's
  +6 byte` for each joystick player (`0x14BD5 or byte [0x2f12],bh` / `0x14BDD or byte [0x2f1b],bl`)
  — that bit is the "device is a joystick" flag used in §6.2.
* Default key maps in the EXE image (identical to the tail of `CONFIG.SF2`):
  `DS:0x2F1E = 48 01 50 02 4B 04 4D 08 39 20 1C 40` (P1: ↑↓←→, space=punch, enter=kick)
  `DS:0x2F2A = 10 01 1E 02 2C 04 2D 08 02 20 03 40` (P2: Q,A,Z,X = ↑↓←→, 1=punch, 2=kick).
* Joystick byte is built at `0x12D6E..0x12DD9` from port `0x201` + the axis timers: axis bits go to
  `bl` bits 1/2/4/8 (up/down/left/right), pad buttons map `0x20|0x10` / `0x40|0x10`
  (`0x12DC2 test al,0x50` / `0x12DCC test al,0xa0`).
* Keyboard scanning: INT 9 handler `0x12E40` stores per-scancode state bits into `0x16D4`
  (`or byte [bx],3` on make — bit0 = "consumed", bit1 = held; `and byte [bx],0xFD` on break);
  it is installed at `0x12E18` (`mov ax,0x2509; mov dx,0x2E40; int 21h`). The key-set readers only
  look at "held" (bit1).

### 2.3 Per-player input record (`0x2F0C` P1, `0x2F15` P2) (observed)

| off | writer | meaning |
|---|---|---|
| `+0` | `0x14C1E/0x14C22` (from `[0x2F08]`/`[0x2F0A]`) | word: device selector (0 ⇒ no device ⇒ CPU/AI) |
| `+2` | `0x14BD9/0x14BE1` | word: selector copy (`0x2F0E`/`0x2F17`) |
| `+4` | poller `0x14E0A` | byte: **first poll of the frame** = `dirs | newly-pressed buttons` |
| `+6` | poller / setup | flags: `bit0` button-press latch, `bit1` button held, `bit2` "joystick", `bit3` "already polled this frame" |
| `+7` | poller `0x14DFD` | byte: last raw device byte (held directions **and held** buttons) |
| `+8` | poller `0x14E17`, cleared by `0x17953` | byte: OR-accumulated **newly pressed** button bits (`new & ~old & 0xF0`) |

Fighter read (`0x17953`, observed): `AL = [BP+4]; AL |= [BP+8]; [BP+8] = 0; and byte [BP+6],0xF7`
⇒ the decoder sees `directions (latched at the first poll) | any button press seen this frame`.
It then stores the same byte to `player[0x17]` (`0x17967`).

### 2.4 Poller `0x14D52` → `0x14DBD` (observed)

```
0x14D52  if ([0x16D1] & 3) CALL 0x12C43        ; poll one joystick via IRQ-safe path
0x14D5C  CALL 0x14DBD                          ; read both devices
0x14D5F  mov al,[0x2F13]; mov di,0x2F4B; CALL 0x14D72   ; history update P1 (byte +7 !)
0x14D68  mov al,[0x2F1C]; mov di,0x2F95; CALL 0x14D72   ; history update P2
```
Called **once per timer tick** from the game's IRQ0 handler `0x12B14` (`0x12B35 call 0x12B74`,
`0x12B74` → `0x12B96 call 0x14D52`). The handler is installed at `0x12A53`
(`mov ax,0x2508; mov dx,0x2B14; int 21h` — INT 8). Several polls can therefore precede one fighter
frame; the `[+4]`/`[+6]bit3` latch and the `[+8]` accumulator merge them.

```
0x14DBD  if ([0x3029] == 0xFF) RET              ; input locked (recogniser/clear running)
         ...for each device di = 0x2F0C/0x2F15, bx = [0x2F08]/[0x2F0A]:
0x14DD5  AL = handler(bx)                        ; raw byte
0x14DD9  if (AL & 0x10) [di+6] |= 2              ; "button held"
         else if ([di+6] & 2) { [di+6] &= ~2; [di+6] |= 1 }   ; falling edge → press latch
0x14DF1  bl = (AL ^ [di+7]) & AL & 0xF0          ; newly-pressed buttons
0x14DFD  [di+7] = AL
0x14E00  AL = (AL & 0x0F) | bl                   ; dirs + new buttons
0x14E04  if (!([di+6] & 8)) { [di+4] = AL; [di+6] |= 8 }      ; first poll this frame latches
0x14E11  if (AL & 0x10) { AL &= 0xF0; [di+8] |= AL }          ; accumulate pressed buttons
```
Readers (observed): `0x14CE2` returns bit0/bit1 of the two records' `[+6]` press latches (P1 ⇒
bit0, P2 ⇒ bit1) and consumes them; `0x14D15` returns `([0x2F10] | [0x2F19]) & 0x0F` (the two
latched direction bytes ORed) and clears `[+6]` bit3 for both; `0x14D29` resets *both* input records
(`0x14D3D`: `[+6] &= 0xFE`, `[+6] &= 0xFD`, `[+7] = [+8] = [+4] = 0`); `0x14C42` clears a 64-byte
history ring (`[di+2] = [di+4] = base`, `[di] = 0`, `0x40` zero bytes, guarded by `[0x3029]`).
`0x14CCA` returns the press latch or `[0x16F0]` bit0.

---

## 3. Per-player recogniser state (`0x2F4B` P1 / `0x2F95` P2) (observed)

By `0x14E1B` (`di = 0x2F4B` if `AH == 0` else `0x2F95`) and by `0x14D72`:

| off | initial (image) | meaning |
|---|---|---|
| `+0` | `00` | ring index `0..0xF` |
| `+1` | `10` | ring size (16) |
| `+2` | `55 2F` (`9F 2F` for P2) | word: ring **base** pointer |
| `+4` | `55 2F` | word: current ring write pointer |
| `+6` | `0000` | word: run-length counter of the current input |
| `+8` | `00` | byte: last stored input byte |

Ring: **16 entries × 4 bytes**, P1 at `0x2F55..0x2F94`, P2 at `0x2F9F..0x2FDE`
(`0x2F55+0x40 = 0x2F95`, `0x2F9F+0x40 = 0x2FDF` = `INPUT_CLASS` — the layout is contiguous).

History update `0x14D72` (observed; `AL` = raw device byte from `[record+7]`):

```
AL  = (AL & 0xF0) | MIRROR_ID[AL & 0xF]      ; DS:0x2FFF = {0,1,2,0,4,5,6,0,8,9,A,0,...}
AH  = [di+8]                                 ; previous input byte
BX  = [di+4]                                 ; write pointer
if (AL != AH) {
    cx = [di+6]; [bx+2] = AH; [bx] = cx      ; finalise the run that just ended
    cl = [di]; cl++; if (cl >= [di+1]) { cl = 0; bx = [di+2] } else bx += 4
    [di] = cl; [di+4] = bx; [di+8] = AL; [di+6] = 0
}
[di+6]++; cx = [di+6]; [bx+2] = AH; [bx] = cx
```
**Quirk (observed, must be reproduced):** on the change frame the *new* slot also receives the
**old** byte `AH`. The slot is corrected on the next frame only if the input is unchanged. A
one-frame run therefore keeps the previous run's byte forever. (Simulation: a 3-frame-per-step
motion is byte-identical to the intended semantics; 1-frame taps are seen one frame late.)

`MIRROR_ID[4↔8,5↔9,6↔A]` is the identity table at `DS:0x2FFF`; the recogniser uses the *mirroring*
table `DS:0x2FEF` (`4↔8, 5↔9, 6↔A`) itself when `[0x3027] == 0xFF` (facing left).

---

## 4. The recogniser `FUN_1000_4E1B` (`0x14E1B`) (observed)

### 4.1 Inputs / output

| in | value | set at |
|---|---|---|
| `SI` | `0x78 + char*8` (pointer array) | `0x10807`, `0x1081E` |
| `AH` | `obj[0x36]` = player number (0 ⇒ `0x2F4B`, else `0x2F95`) | `0x1080B` |
| `[0x3027]` | `0` = facing right, `0xFF` = facing left ⇒ mirror history directions | `0x1080E..0x10819` |
| `[0x3028]` | per-block: `0xFF` if block flags bit15 clear else `0` (set inside) | `0x14E37/0x14E42` |
| `[0x3029]` | input lock (set while the success path clears the ring) | `0x14ED6/0x14EE6` |
| out | `CF` = matched; `CL` = `0..3` = block index ⇒ action `0x34+CL` | `0x14EF1/0x14EF3` |

### 4.2 Data format (observed by construction from the code + data)

Pointer array (`DS:0x78 + char*8`, 4 words): `ptr[i]`, `0` = slot disabled.
Condition block at `ptr`:

```
word flags        ; bit15 = 0x8000
element[]         ; 4 bytes each, terminated by a zero word
  byte v          ; run-length threshold
  byte f          ; f&1: require run_len > v ; f&2: require run_len <= v ; f==0: abort path
  byte btn        ; input byte to match; btn&0x80 ⇒ mask ((btn&0x7F)&input)!=0
  byte pad        ; never read
word 0            ; terminator
```

### 4.3 Algorithm (asm-level, then prose)

```
outer: for i in 0..3                                       ; CX = 4, BH = i
    ptr = word[SI + 2*i]; if (ptr == 0) continue            ; 0x14E2E/0x14E30
    [0x3028] = 0xFF                                        ; 0x14E37
    if (word[ptr] & 0x8000) [0x3028] = 0                   ; 0x14E3C/0x14E42
    si = ptr + 2                                           ; 0x14E47
    if (word[si] == 0) continue                            ; 0x14E4A (empty block)
    di = word[[0x2F4B|0x2F95] + 2]                          ; ring base 0x14E52
    for start in {0,4,...,0x3C}:                            ; 0x14E55/0x14E57
        e = 0; pos = start; dx = start
        loop:
            AH = word[si + 2] (btn); AL = ring[pos].byte
            if ([0x3028] == 0xFF && AL == 0) {              ; 0x14E61..0x14E6A quiet skip
                 if (ring[pos].word > 6) fail_start         ; 0x14E6C..0x14E73
                 pos = (pos+4) & 0x3C; if (pos == start) fail_start   ; 0x14EC7..0x14ECD
                 continue                                   ; same element, next run
            }
            if ([0x3027] == 0xFF) AL = (AL & 0xF0)|MIRROR[AL & 0xF]   ; 0x14E78..0x14E8D
            if (!(AH & 0x80)) { if (AL != AH) fail_start }  ; 0x14E8E..0x14E97
            else              { if ((AH & 0x7F) & AL) == 0 fail_start } ; 0x14E99..0x14E9E
            cx = v; ax = ring[pos].word                     ; 0x14EA0..0x14EA4
            if (f & 0x01) { if (ax <= cx) fail_start }      ; 0x14EA6..0x14EC1 (jbe fail)
            else if (f & 0x02) { if (ax >  cx) fail_start }  ; 0x14EAC..0x14EBE (ja fail)
            else  abort();                                  ; 0x14EB4 (see 4.5)
            si += 4; pos = (pos+4) & 0x3C                   ; 0x14EC4..0x14ECA
            if (pos == start) fail_start                    ; 0x14ECD
            if (word[si] == 0) SUCCESS(i)                   ; 0x14ED1/0x14ED4
        fail_start: pos = start+4 (masked); if 0 → fail_block; else retry from start
        fail_block: next i
SUCCESS(i):
    [0x3029] = 0xFF; zero 0x40 bytes at ring base; [0x3029] = 0
    CL = i; STC; return                                         ; 0x14ED6..0x14EF4
fall through all four → CLC; return                             ; 0x14F13/0x14F14
```

Prose: for every ring position (oldest→newest, wrapping) try to match the block's elements
*consecutively forward in the ring*, skipping "no input" runs of `len <= 6` when the block allows
it. Match condition per element: the run's byte matches `btn` (equality, or non-empty AND for
mask elements) and the run's length satisfies `v`/`f`. Reaching the zero terminator is a match;
the ring is then cleared so the same motion cannot re-trigger immediately.

First matching block wins (`i` order = priority); `0x3029` blocks the poller during the clear.

### 4.4 Motion data — full table (observed bytes, `DS`)

Pointer arrays (`DS:0x78`, what `tables.rs::MOTION_PATTERNS` holds):

| char | ptr[0] | ptr[1] | ptr[2] | ptr[3] |
|---|---|---|---|---|
| KEN 0 | `00B8` | `0170` | `020C` | `0224` |
| GUILE 1 | `00CC` | `0184` | `0220` | `0238` |
| HONDA 2 | `00DC` | `0194` | `0220` | `0238` |
| RYU 3 | `00B8` | `0170` | `020C` | `0224` |
| ZANGIEF 4 | `00EC` | `01BC` | `0220` | `0238` |
| CHUN-LI 5 | `0104` | `01D4` | `0220` | `0238` |
| DHALSIM 6 | `0130` | `01E8` | `0220` | `0238` |
| BLANKA 7 | `0148` | `01FC` | `0220` | `0238` |
| BALROG 8 | `0000` | `021A` | `0002` | `021A` |
| VEGA 9 | `0006` | `021A` | `0004` | `021A` |
| SAGAT 10 | `00C0` | `0000` | `0000` | `0164` |
| BISON 11 | `0002` | `021A` | `0001` | `021A` |

Blocks (contiguous `0xB8..0x23B`; `elems` = `(v,f,btn)`; `btn` with `0x80` = mask):

| block | flags | users | elements |
|---|---|---|---|
| `00B8` | `0000` | KEN[0], RYU[0] | `(1A,02,02) (1A,02,06) (1A,02,04) (1A,02,C0)` = ↓, ↙, ← + kick → **QCB+K**, action 0x34 |
| `00CC` | `0000` | GUILE[0] | `(64,01,02) (1A,02,01) (1A,02,C0)` = hold ↓ >100f, ↑, kick → **flash kick**, 0x34 |
| `00DC` | `0000` | HONDA[0] | `(64,01,04) (1A,02,08) (1A,02,A0)` = hold ← >100f, →, punch → **sumo headbutt**, 0x34 |
| `00EC` | `8000` | ZANGIEF[0] | `(06,02,A0) (06,02,00)×4` = mash punch ×5, action 0x34 |
| `0104` | `8000` | CHUN-LI[0] | `(06,02,C0) (06,02,00)×4` = mash kick ×5, action 0x34 |
| `012C` | `0000` | – (unreferenced) | – |
| `0130` | `0000` | DHALSIM[0] | `(1A,02,06) (1A,02,02) (1A,02,0A) (1A,02,08) (1A,02,A0)` = ↙↓↘→ + punch, 0x34 |
| `0148` | `8000` | BLANKA[0] | `(06,02,A0) (06,02,00)×4` = mash punch ×5, 0x34 |
| `0170` | `0000` | KEN[1], RYU[1] | `(1A,02,02) (1A,02,0A) (1A,02,08) (1A,02,A0)` = ↓↘→ + punch → **hadouken**, 0x35 |
| `0184` | `0000` | GUILE[1] | `(64,01,04) (1A,02,08) (1A,02,A0)` = hold ← >100f, →, punch → **sonic boom**, 0x35 |
| `0194` | `8000` | HONDA[1] | `(06,02,A0) (06,02,00)×4` = mash punch ×5, 0x35 |
| `01BC` | `0000` | ZANGIEF[1] | `(1A,02,06) (1A,02,02) (1A,02,0A) (1A,02,08) (1A,02,A0)` = ↙↓↘→+punch, 0x35 |
| `01D4` | `0000` | CHUN-LI[1] | `(64,01,02) (1A,02,01) (1A,02,C0)` = hold ↓ >100f, ↑, kick, 0x35 |
| `01E4` | `0000` | – (unreferenced) | – |
| `01E8` | `0000` | DHALSIM[1] | `(1A,02,02) (1A,02,0A) (1A,02,08) (1A,02,A0)` = ↓↘→+punch, 0x35 |
| `01FC` | `0000` | BLANKA[1] | `(64,01,04) (1A,02,08) (1A,02,A0)` = charge ← + punch, 0x35 |
| `020C` | `0000` | KEN[2], RYU[2] | `(1A,02,08) (1A,02,02) (1A,02,08) (1A,02,A0)` = →↓→ + punch, 0x36 |
| `0220` | `0000` | 6 chars [2] | (empty block ⇒ never matches) |
| `0224` | `0000` | KEN[3], RYU[3] | `(1A,02,08) (1A,02,02) (1A,02,0A) (1A,02,A0)` = →↓↘ + punch → **shoryuken**, 0x37 |
| `0238` | `0000` | 6 chars [3] | (empty block ⇒ never matches) |
| `021A` | `021A` | bosses [1],[3] | `(A0,00,00)` — cannot match (see 4.5/4.6) |
| `0164` | `00A0` | SAGAT[3] | `(06,02,00) (06,02,A0)` — cannot match (element "neutral run" is captured by the quiet-skip rule) |

`v=0x1A`=26 frames (motion steps must be quick), `v=0x64`=100 (charge), `v=6` (mash speed).
Side note: SAGAT[1] and [2] are `NULL`; block `0x00C0` (action 0x34) is his only reachable block
(0x0164 can never match, see above), and the boss slots `0x0000`/`0x0002`/`0x0004`/`0x0006`/`0x0001`
parse onto the 3-byte stub at `DS:0x0000` (`5B 58 C3` = `pop bx; pop ax; ret`) where they are
skipped or can never match. The four bosses therefore have **no working special input data** (they
were not playable in the original) — relevant only if the port makes them selectable.

### 4.5 The `f == 0` case is a fatal assert (observed)

`0x14EAC..0x14EB3`: `F7 04 00 02 74 02 EB 06` = `test word[si],0x200 / je 0x14EB4 / jmp 0x14EBA`.
At `0x14EB4` the bytes are `0E E8 7C B2 94 08` = `push cs; call 0x10134` followed by unreachable
garbage. `0x10134` is the program's **assert/abort routine**: it prints the RTC time
(`int 1Ah`, `int 21h` AH=9/2), dumps registers/memory (`call 0x17D94`, `call 0x102AF`…) and ends
at `0x102AC` → `0x11465: mov ax,0x4C00; int 0x21` (terminate). It never returns, which is why the
bytes after the call are meaningless. Reached only by an element with `f == 0` **and** a matched
byte; every such element in the shipped data (`0x21A`, and the boss stubs) is either skipped
(`btn = 0` + `[0x3028] = 0xFF` ⇒ quiet-skip takes precedence; block `0x21A` can then never
terminate) or unreferenced. **Port recommendation:** treat `f == 0` as unreachable
(`debug_assert!`), never as a match rule.

### 4.6 Success path side effect

`0x14ED6..0x14EE6`: `[0x3029] = 0xFF; ES = [0x4A] (=DS); rep stosb AL=0, CX=0x40` at the ring base
= clear the 64-byte ring, `[0x3029] = 0`. The index/write-pointer/counter fields are **not**
reset. `[0x3029]` also makes `0x14DBD` skip polling meanwhile.

---

## 5. The human decoder `FUN_1000_075A` (`0x1075A`) — full flow (observed)

Entry registers (fixed by `0x176CA`/`0x17929`): `AL` = input byte (dirs | new buttons),
`AH` = `opp[0x0A]`, `BX` = opponent player struct, `DI` = my player struct, `SI` = my driver
object, `BP` = my input record (`0x2F0C`/`0x2F15`).

```
0x1075A  push bx/cx/si/es
0x1075E  ah = [bx+0x0A]                    ; opponent flags
0x10761  bl = INPUT_CLASS[al & 0xF]        ; DS:0x2FDF
0x1076A  if (bl == 8)  → 0x107CE (= 0x108C6: stand 0x47/0x48)
0x1076F  if (bl == 0xFF) bl = 0
0x10776  if ([0x302A] == 0xFF) → 0x107D9   ; dead in shipped data, see §7 note
0x1077D  al &= 0x7F
0x1077F  if (al == 0) → 0x107D4 (al = 0x47)
0x10781  if (al & 0x10) → 0x107D9          ; any attack button ⇒ attack/special path
0x10785  if (bl == 8) → 0x107CE (stand)
0x1078A  if ((ah & 0x20) && (my[0xA]&3) != 2) {          ; opponent attacking, I'm not in state 2
             ; pressing "back" (facing-dependent): l→ if (al&8), r→ if (al&4)
0x107AA     al = 0x3E; if ((my[0xA]&3) == 1) → tail; al = 0x3F; → tail  ; throw counter
         }
0x107B9  ax = my[0xA] & 3; ax <<= 3; al = INPUT_ACTION[bl + ax]   ; DS:0x23C
0x107CB  → 0x108AD (tail)
```

### 5.1 Attack/special path `0x107D9`

```
0x107D9  if (obj[0x1E] & 4) skip two lines        ; facing right (bit2) ⇒ no remap
0x107DF     bl = al; bl &= 0xF; al = (al & 0xF0) | INPUT_REMAP[al & 0xF]   ; DS:0x254
0x107EA     bl = INPUT_CLASS[al & 0xF]; if (bl == 0xFF) bl = 0            ; class of the *remapped* dir
0x107FA  save ax/bx/di/si
0x107FE  bx = 0x78 + char*8  (char = obj[0x37]*2 → *4 → +0x78)
0x1080B  ah = obj[0x36]                          ; player number
0x1080E  [0x3027] = 0; if (!(obj[0x1E] & 4)) [0x3027] = 0xFF   ; 0xFF = facing left ⇒ mirror
0x1081E  si = bx; call 0x14E1B                   ; recogniser
0x10823  restore ax/bx/di/si
0x10827  if (!CF) → 0x1083E                      ; no special
         ; ---- CF set: special ----
0x10829  al = cl + 0x34                          ; action 0x34..0x37 (CL = block index)
0x1082E  if (char*2 == 8 /*BLANKA*/ && al == 0x35) al = 0x33
0x1083C  → tail
```
`INPUT_REMAP` (`DS:0x254`, image = `00 01 02 00 08 09 0A 00 04 05 06 00 00 00 00 00`) mirrors the
direction nibble `4↔8,5↔9,6↔A`, i.e. a left-facing player's input is turned into the
"facing-right" frame before the *normal-attack* index is computed, and the class used by that
index is recomputed from the mirrored nibble (`0x107EA..0x107F8`, kept in `BX` across the
recogniser call). (The recogniser applies its own mirror through `[0x3027]`; both are applied when
facing left, none when facing right.)

### 5.2 CF clear — the normal-attack lookup (`0x1083E..0x108AA`)

```
0x1083E  test al,0x0F
0x10840  jne 0x10860                              ; a direction is held
  ; --- button only (no direction) ---
0x10842  bx = my[0xa] & 3;  bx <<= 1              ; state*2
0x1084A  if ([bp+6] & 4) {                        ; device is a JOYSTICK
0x10851      bx += 6
0x10854      if (!(al & 0x20)) bx += 6            ; kick
         }
0x1085B  ax = 0x0E                                ; resource table selector
0x1085E  jmp 0x10892
  ; --- direction held ---
0x10860  bx <<= 3                                 ; bx = class (remapped at 0x107EA if facing left)
0x10866  bx += [0x4E]                             ; distance bucket (0x104FB)
0x1086A  if (!([bp+6] & 4)) ax = 0x10             ; keyboard
0x10876  else if (al & 0x20) ax = 0x12            ; joystick punch
0x1087F  else ax = 0x14                           ; joystick kick
0x10882  cx = my[0xa] & 3; ch = cl; cl = 0; cx >>= 2 (twice)   ; cx = state*64
0x10890  bx += cx
0x10892  push bx; bx = char*2; si = word[bx+0x60]              ; per-character resource handle
0x1089D  push si; call 0x138E4                    ; resolve handle → ES = resource segment
0x108A1  si = ax                                   ; AX = the table selector 0x0E/0x10/0x12/0x14
0x108A3  si = word[es:si]                          ; sub-table offset inside the resource
0x108A6  al = byte[es:bx+si]                       ; ← the requested ACTION
0x108A9  pop si; call 0x1391C                      ; release handle
```

So the **final `AL` in the normal case = a data byte from the character's resource**, indexed by:

* button-only: `index = posture*2 (+6 if joystick) (+6 if kick)`, sub-table `es[0x0E]`;
* with direction: `index = input_class*8 + distance_bucket + posture*64`, sub-table
  `es[0x10]` (keyboard), `es[0x12]` (joystick + punch), `es[0x14]` (joystick + kick);
  `input_class` is the *remapped* direction's class (left-facing) or the raw one (right-facing).

`posture = player[0x0A] & 3` (set from the frame header flags at `0x10D63`; 0 = standing,
2 = airborne — `0x17929`/§4.1 treat `&3 == 2` specially; the stand helper returns 0x47 for 0 and
0x48 otherwise). `[0x4E]` = distance bucket `0..7` computed at `0x104FB`.

### 5.3 Tail `0x108AD`

```
0x108AD  pop es/si/cx/bx
0x108B1  if (obj[0x1E] & 4) { if (al == 0x40) al = 0x41 else if (al == 0x41) al = 0x40 }
0x108C5  ret
```
(0x40/0x41 = the two jump directions, swapped for the facing.)

### 5.4 Helpers referenced above

* `0x108C6` (stand): `al = 0x48; if ((my[0xa]&3) == 0) al = 0x47`.
* `0x138E4(handle)` (observed): `handle == 0` → assert/abort; `handle > 0` (signed) → return
  (already resolved); else `di = ((0x10000 - handle - 1) * 11) & 0xFFFF` in segment `[0x19C2]`
  (resource directory record), assert record `[di+0x0A] & 0x80 == 0`, `call 0x136F0` if
  `[di+0x0A] & 4` clear (load), set `0x80`, `es = word[di]`, `call 0x13118` (EMS/overlay mapping if
  `es >= 0xA000`; `0x134F4`/`0x13366` handle the two memory models).
* `0x1391C`: releases the handle (`es = [di]; [di+0x0A] &= ~0x80` after test).
* `DS:0x60 + char*2` handles (image): KEN `F160`, GUILE `F161`, HONDA `F162`, RYU `F15E`,
  ZANGIEF `F15B`, CHUN-LI `F164`, DHALSIM `F163`, BLANKA `F165`, BALROG `F166`, VEGA `F15C`,
  SAGAT `F15D`, BISON `F15F`.

---

## 6. Port-ready pseudocode

### 6.1 Input sampling (once per game frame is enough; see §7)

```rust
struct InputRecord { selector: u16, dir_latch: u8, flags: u8, last_raw: u8, pressed_acc: u8 }

fn poll_devices(recs: [&mut InputRecord; 2], raw: [u8; 2]) {   // 0x14DBD
    for i in 0..2 {
        let al = raw[i];                                        // device byte, §2.1
        let r = recs[i];
        if al & 0x10 != 0 { r.flags |= 0x02 }                   // held
        else if r.flags & 0x02 != 0 { r.flags = (r.flags & !0x02) | 0x01 } // press latch
        let new_btn = (al ^ r.last_raw) & al & 0xF0;
        r.last_raw = al;
        let al2 = (al & 0x0F) | new_btn;
        if r.flags & 0x08 == 0 { r.dir_latch = al2; r.flags |= 0x08 }   // first poll of frame
        if al2 & 0x10 != 0 { r.pressed_acc |= al2 & 0xF0 }
    }
}
fn decoder_input(r: &mut InputRecord) -> u8 {                    // 0x17953
    let al = r.dir_latch | r.pressed_acc;
    r.pressed_acc = 0; r.flags &= !0x08;
    al
}
```

### 6.2 Recogniser

```rust
struct Hist { ring: [[u16; 2]; 16], idx: u8, cnt: u16, last: u8 }   // {len, byte}

fn hist_update(h: &mut Hist, raw: u8) {                             // 0x14D72
    let al = (raw & 0xF0) | MIRROR_ID[(raw & 0xF) as usize];
    let ah = h.last;
    if al != ah {
        h.ring[h.idx as usize] = [h.cnt, ah as u16];                // finalise old run
        h.idx = (h.idx + 1) & 0x0F;
        h.last = al; h.cnt = 0;
    }
    h.cnt += 1;
    h.ring[h.idx as usize] = [h.cnt, ah as u16];                    // NOTE stale byte on change
}

fn recognise(blocks: &MotionBlocks, h: &Hist, facing_right: bool) -> Option<u8> {
    for i in 0..4 {                                                 // 0x14E1B, returns None = CF clear
        let Some(blk) = blocks.block(i) else { continue };
        let skip_mode = blk.flags & 0x8000 == 0;                    // [0x3028] == 0xFF
        for start in 0..16 {
            let (mut e, mut pos) = (0usize, start);
            let ok = loop {
                let (len, byte) = (h.ring[pos][0], h.ring[pos][1] as u8);
                let el = &blk.elems[e];
                if skip_mode && byte == 0 {                          // quiet-skip
                    if len > 6 { break false }
                    pos = (pos + 1) & 0x0F; if pos == start { break false }
                    continue
                }
                let al = if facing_right { byte } else { mirror_dir(byte) };
                if el.btn & 0x80 == 0 { if al != el.btn { break false } }
                else if el.btn & 0x7F & al == 0 { break false }
                if el.f & 0x01 != 0 { if len <= el.v as u16 { break false } }
                else if el.f & 0x02 != 0 { if len > el.v as u16 { break false } }
                else { unreachable!("f==0 = abort path, not in shipped data") }
                e += 1; pos = (pos + 1) & 0x0F;
                if pos == start { break false }
                if e == blk.elems.len() { break true }               // terminator word reached
            };
            if ok { /* clear ring (0x40 bytes) here */ return Some(i as u8); }
        }
    }
    None
}
```

### 6.3 Decoder

```rust
fn decode_input(my_obj, my, opp, input: u8) -> u8 {                 // 0x1075A
    let mut class = INPUT_CLASS[input as usize & 0xF];
    if class == 8 { return stand(my) }
    if class == 0xFF { class = 0 }
    let mut al = input & 0x7F;
    if al == 0 { return 0x47 }
    if al & 0x10 != 0 { return attack_path(my_obj, my, opp, input) } // §5.1/5.2
    if opp.flags_a & 0x20 != 0 && my.flags_a & 3 != 2 {
        let back = if my_obj.facing & 4 != 0 { al & 8 != 0 } else { al & 4 != 0 };
        if back { return if my.flags_a & 3 == 1 { 0x3E } else { 0x3F } }
    }
    INPUT_ACTION[class as usize + 8 * (my.flags_a & 3) as usize]
}
```

---

## 7. Evidence index (all byte sequences re-read from the image)

| address | bytes | meaning |
|---|---|---|
| `0x14E2E` | `8B 34 0B F6 75 03 E9 CC 00` | `mov si,[si] / or si,si / jne / jmp` — pointer deref (4 words) |
| `0x14E3C` | `F7 04 00 80 74 05 C6 06 28 30` | `test word[si],0x8000` → `[0x3028]` |
| `0x14E4A` | `83 3C 00 75 03 E9 B1 00` | empty-block skip |
| `0x14E52` | `8B 7D 02 33 DB` | `di = [di+2]` (ring base), `bx = 0` |
| `0x14E61` | `80 3E 28 30 FF 75 10 0A C0 75 0C 8B 01 3D 06 00 7E 03 E9 80 00 EB 4F` | quiet-skip (`byte==0` ⇒ run ≤ 6) |
| `0x14E78` | `80 3E 27 30 FF 75 0F 53 8A D8 32 FF 80 E3 0F 24 F0 0A 87 EF 2F 5B` | mirror under `[0x3027]==0xFF` using `DS:0x2FEF` |
| `0x14E8E` | `F6 C4 80 75 06 3A C4 74 09 EB 5D 80 E4 7F 84 C4 74 56` | btn equality vs mask |
| `0x14EA6` | `F7 04 00 01 75 14 F7 04 00 02 74 02 EB 06 0E E8 7C B2 …` | `f&1`/`f&2`/abort dispatch |
| `0x14EBA`/`0x14EC0` | `3B C1 77 38 EB 04` / `3B C1 76 32` | `ax > v` / `ax <= v` |
| `0x14EC4` | `83 C6 04 83 C3 04 83 E3 3C 3B DA 74 25 83 3C 00 75 85` | advance element+run, wrap test, terminator test |
| `0x14ED6` | `C6 06 29 30 FF 32 C0 B9 40 00 8E 06 4A 00 F3 AA C6 06 29 30 00` | clear ring, `[0x3029]` guard |
| `0x14EF1` | `8A CF F9 EB 1E` | `cl = bh; stc` — return CF+CL |
| `0x14F21` | `BE 1E 2F EB 03 BE 2A 2F 32 C0 B9 06 00 32 FF 8A 1C 80 A7 D4 16 FE F6 87 D4 16 02 74 0B F6 44 01 60 74 02 0C 10 0A` | key-set reader (6 `{scancode,byte}` pairs; `byte&0x60 ⇒ or 0x10`) |
| `0x14D72` | (see §3) | history update |
| `0x14DBD` | (see §2.4) | device poll |
| `0x14CE2` | `32 C0 50 A0 D5 16 24 01 80 26 D5 16 FE 0A C0 58 74 05 E8 FC E1 B0 80 B4 01 BF 0C 2F E8 05 00 B4 02 BF 15 2F F6 45 06 01 74 06 80 65 06 FE` | press-latch reader |
| `0x10761` | `8A D8 83 E3 0F 8A 9F DF 2F` | `INPUT_CLASS[dir]` |
| `0x10781` | `A8 10 75 54` | `test al,0x10 → 0x107D9` |
| `0x107D9` | `F6 44 1E 04 75 1B 8A D8 83 E3 0F 24 F0 0A 87 54 02 8A D8 83 E3 0F 8A 9F DF 2F 80 FB FF 75 02 32 DB` | remap + class recompute |
| `0x1081E` | `8B F3 E8 F8 45` | `si = bx; call 0x14E1B` |
| `0x10829` | `80 C1 34 8A C1 8A 64 37 80 FC 08 75 77 3C` | special action + BLANKA `0x35 → 0x33` |
| `0x1083E` | `A8 0F 75 1E … 3E F6 46 06 04 74 0A 83 C3 06 A8 20 75 03 83 C3 06 B8 0E 00` | button-only index |
| `0x10860` | `D1 E3 D1 E3 D1 E3 03 1E 4E 00 3E F6 46 06 04 75 05 B8 10 00 EB 0C A8 20 74 05 B8 12 00 EB 03 B8 14 00` | direction index + hand selector |
| `0x10892` | `53 32 FF 8A 5C 37 8B B7 60 00 5B 56 E8 43 30 8B F0 26 8B 34 26 8A 00 5E E8 6F` | resource lookup |
| `0x12B14` | `50 1E B8 A4 0A 8E D8 FC FB B0 20 E6 20 E8 35 00` | IRQ0 handler prologue (installed at `0x12A53`: `mov ax,0x2508; mov dx,0x2B14; int 21h`); `0x12B35 call 0x12B74` |
| `0x12D6E` | (see §2.2) | joystick byte build |
| `0x12E40` | `9C 50 53 1E B8 A4 0A 8E D8 E4 60 …` | INT 9 scancode state into `0x16D4+code` |
| `0x14B65` | (see §2.2) | selector assignment + joystick flag |

Data tables (DS): `0x254` remap, `0x2FDF` classes, `0x2FEF` mirror (recogniser), `0x2FFF` identity
(poller), `0x2F1E`/`0x2F2A` key maps, `0x2F37` device table, `0x2F4B`/`0x2F95` state blocks,
`0x2F55`/`0x2F9F` rings, `0x60` resource handles, `0x78` pointer arrays, `0xB8..0x23B` blocks,
`0x23C` fallback action table, `0x4E` distance bucket, `0x3027` facing mirror flag,
`0x3028` skip mode, `0x3029` input lock, `0x302A` dead flag.

---

## 8. Validation

A Python simulator of `0x14D72` + `0x14E1B` (exact, including the stale-byte quirk and the quiet
skip) was run against the shipped tables:

| input (facing right unless noted) | result |
|---|---|
| KEN: ↓ (3f) ↘ (3f) → (3f) →+P (5f) | block 1 ⇒ action `0x35` (hadouken) ✓ matches the `0x1035` spawn hook |
| KEN: ↓ ↙ ← +K | block 0 ⇒ `0x34` |
| KEN: → ↓ ↘ +P | block 3 ⇒ `0x37` |
| GUILE: hold ← 120f, → 3f, +P | block 1 ⇒ `0x35` (sonic boom; hook `0xC35`) ✓ |
| GUILE: hold ↓ 120f, ↑ 3f, +K | block 0 ⇒ `0x34` (flash kick) |
| CHUN-LI: mash K (0x40 ⇄ 0, 2f each) ×5 | block 0 ⇒ `0x34` (needs `0x40`, not `0x20`) |
| HONDA / ZANGIEF / BLANKA: mash P (0x20 ⇄ 0) | HONDA block 1 ⇒ `0x35`; ZANGIEF/BLANKA block 0 ⇒ `0x34` |
| KEN QCF+P while facing left (mirrored input) | block 1 ⇒ `0x35` ✓ |
| KEN QCF+P with a 9-frame neutral gap | no match (gap limit 6) ✓ |
| KEN QCF+P with ≤ 6-frame neutral gaps between steps | block 1 matches (gaps are skipped) ✓ |
| KEN QCF+P with 30-frame steps | no match (`f=2`, `v=0x1A`) ✓ |

---

## 9. Confidence and unknowns

**High confidence (byte-exact):** the recogniser algorithm, its inputs/outputs, the pointer-array
+ block + element format, the full table contents above, the ring state layout and the history
update (including the stale-byte quirk), the `0x107D9` control flow, the `CL+0x34` / BLANKA
`0x35→0x33` mapping, the input-bit table, the poller/press-latch logic, the device table, the
default key maps, and the fact that `[0x302A] == 0xFF` is never true (byte is 0 in the image and
has exactly one reference, the compare itself).

**Medium confidence / inferred:**
* Names of the four "postures" (`player[0x0A] & 3`): 0 = standing (idle 0x47), 2 = airborne
  (special-cased everywhere), 1/3 = other stances (idle 0x48). Values are observed; labels are not.
* `bit4` is documented as "attack button held (any)" because the keyboard reader ORs it from
  `byte & 0x60` and the joystick converter ORs it from any pad button; the same bit drives the
  menu press latch (`0x14CE2`), i.e. it doubles as "start". No shipped mapping sets it alone.
* The "one-frame-lagged byte" in the ring is reproduced from the code; whether the original
  developers intended it is unknown (it does not affect motions that hold each step ≥ 2 frames).
* The joystick selector modes `0xA..0x12` (`0x1306A/0x1302C/0x13039/0x13046/0x13058`) were not
  decoded beyond their entry points (their bodies look like a resource/calibration path); the
  default selector assignment (`0x14B65`) only produces `2/4/6/8`, so they appear to be
  options-menu variants. Not needed for the standard devices.
* `0x138E4`'s "abort when `handle == 0`" / record-flag asserts are read as asserts; the routine
  never returns (ends in `int 21h` AH=4Ch at `0x11465`).

**UNKNOWN (explicitly not resolved here):**
1. **Contents of the normal-attack resources** (`DS:0x60[char]` → directory record →
   `ES:[0x0E/0x10/0x12/0x14]` sub-table → action byte). The index formulas in §5.2 are exact, but
   the bytes live in a run-time resource (directory segment `[0x19C2]`, set at `0x13611`; EMS
   device-name check `EMMXXXX0` at `DS:0x19B4`; page mapping `0x13118`). They are **not in
   `SF2.EXE`** and `INDEX.SF2` is a text name list, not the 11-byte-record directory. Dumping them
   needs a runtime trace (watch `ES:word[0x0E/0x10/0x12/0x14]` at `0x108A1`) or a decode of the
   resource loader (`0x13575/0x136x`). Consequently the *exact* action ids the normal path can
   return, and whether the keyboard path really cannot distinguish punch from kick in the
   button-only index (`[bp+6] & 4` clear ⇒ `index = posture*2`), remain to be confirmed by data.
2. The `f == 0` element flag combination: the shipped code asserts/aborts (§4.5); no reachable
   element uses it, so its intended semantics is not recoverable from this image.
3. The exact poll cadence (IRQ0 handler with a `0x168C += 0x3D7` / `-= 0x100` divider at
   `0x12B24..0x12B35`) — not needed if the port polls once per frame, but a bit-exact port should
   verify with a runtime trace whether `[+8]` accumulation spans multiple game frames.
4. The device-detect routine `0x12BFC` runs once at startup; whether a joystick can be
   hot-plugged/re-detected later (`0x14B65` re-check path) was not pursued.
