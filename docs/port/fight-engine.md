# Fight engine — structural map of the GAMEPLAY code (SF2.EXE, MS-DOS 1995)

Target: `SF2.EXE` (79,948 B, MZ, 16-bit x86), sha256 `cdcbb616…0c15`, image base linear `0x10000`,
`file_offset = linear − 0xFFA0`, runtime `DS` value `0x0AA4(+load)` → **DS base linear `0x1AA40`**,
second data segment `0x12A4(+load)` → linear `0x22A40`.
Analysis: Ghidra 12.1.4 via REA + a hand-written 16-bit linear disassembler for the regions Ghidra
left unanalyzed (very common in this binary — see “gaps” below).

Evidence classes used below: **observed** (bytes/instructions read directly), **derived**
(reproducible from observed data), **inferred** (stated explicitly).

---

## 1. Summary

* The real program flow is `entry 0x104E6 → 0x14F4F (init) → 0x14FB9 (main loop) → 0x14F93 (shutdown)`,
  with a final `JMP 0x11465` that is only the `exit()` stub (`MOV AX,4C00h; INT 21h`). The
  “main = 0x11465” note in `engine-overview.md` is corrected here.
* Gameplay is a **screen state machine** (`0x14FB9`) feeding a **4-item title menu** whose dispatch
  table is at `CS:0x5029`; item 0 = GAME START → match driver `0x140F9` → round loop `0x15083`.
* Fighters are two fixed structs at **`DS:0x3BA9` (P1) / `DS:0x3BCC` (P2)**, stride `0x23`,
  with energy at `+0x14` (bytes `DS:0x3BBD` / `DS:0x3BE0`).
* Each visible entity (fighter, fireball sprite, hit spark) is a **0x48-byte “draw object”** in a
  32-slot array at `DS:0x1B5A`; the per-frame active list of pointers is at `DS:0x245A`
  (count `DS:0x1B58`).
* Per-character data comes from three indexed asset tables (word-strided by `char*2`):
  `DS:0x60`=`ait<CHR>` (attack/AI table), `DS:0x266`=`ani<CHR>` (animation), `DS:0x27E`=`frm<CHR>`
  (frame/hotspot table). **`ani<CHR>` is 200 × 42-byte frame records**; the 42-byte record carries
  sprite refs, the fighter’s hitboxes and hit-type nibbles.
* Hit detection: per frame build 4 hitboxes per fighter from `ani[frame*42+0x10]`
  (`0x12712`), then resolve with a body-overlap test plus a per-character “block rule” function
  table (`0x121A0`, `CS:0x22B3`).
* Character-specific behaviour (fireball release, special moves) lives in **unanalyzed code
  0x16993–0x170F8**, dispatched per character through `DS:0x3BF9` and per action through
  `CS:0x6999`-style tables.
* The on-screen debug panel described by the `Energy :`/`Action :`/… strings is **dead data in this
  build** (no reference to its label table `DS:0x385C` exists anywhere) — but its label list sits
  directly in front of the live player state and is a good field-name hint.

---

## 2. Deterministic facts used by the map

| Fact | Evidence |
|---|---|
| `DS` = `0x0AA4`(+load) (reloc at module off `0x4E8`) | bytes `FC B8 A4 0A 8E D8` at `0x104E6`; MZ reloc list |
| Title strings `PLAYER SELECT/GAME START/V.S. BATTLE/OPTION MODE/QUIT TO DOS` consecutive at `DS:0x453D–0x456E` | bytes `0x1EF7D…`; table of pointers at `DS:0x3DCD` |
| `char` in gameplay is **`2×character`** (`0…0x16`), used as byte offset into word tables | `MOV AL,[SI+0x37]; XOR BH,BH; MOV SI,[BX+0x266]` (`0x1274B`, `0x10956`); tables at `0x60/0x266/0x27E/0x2DE` are word-strided and their names map to characters in order |
| Character order = KEN, GUILE, EHONDO, RYU, ZANGEIF, CHUNLI, DHALISM, BLANKA, BALROG, VEGA, SAGAT, MBISON | `DS:0x60[0..11]` = `aitKEN, aitGUILE, aitEHONDO, aitRYU, aitZANGEIF, aitCHUNLI, aitDHALISM, aitBLANKA, aitBALROG, aitVEGA, aitSAGAT, aitMBISON` (names read from `INDEX.SF2`) |

---

## 3. Mode / state map

### 3.1 Outer loop — `FUN_1000_4FB9` (`0x14FB9`) — observed

```
0x14FB9  CALL 0x979C              ; attract/demo tick
0x14FBE  AL = (DS:0x3230 == 0xFF) ? 5 : 0
0x14FC7  CALL 0x9CD6              ; bank/attract screen tick (AL = 0 or 5)
0x14FCA  DS:0x3232 = 0xFF
0x14FCF  CALL 0x719F   JNZ tail   ; story/ending text scroller
0x14FD4  CALL 0x7413   JNZ tail   ; stub: returns 0
0x14FD9  CALL 0x5CFC   JNZ tail   ; menu screen A
0x14FDE  CALL 0x5DE8   JNZ tail   ; menu screen B
0x14FE3  CALL 0x52FE   JNZ tail   ; V.S. / GAME-START wrapper  (CALLs 0x5083)
0x14FE8  CALL 0x5E4A   JNZ tail   ; screen teardown / return-to-title
0x14FED  JMP  0x4FB9
tail 0x14FAF: DS:0x3232 = 0; CALL 0x4FF0; JS exit   ; title menu + dispatch, sign = quit
```
State variables: `DS:0x3230` (attract/demo selector), `DS:0x3232` (frame/blit flag),
`DS:0x3233` (round/mode result), `DS:0x3235` (pause/quit flag), `DS:0x323D` (screen id, 3/4).

### 3.2 Title menu + dispatch — `FUN_1000_4FF0` (`0x14FF0`) — observed

```
0x1500B: if (DS:0x169E == 0xFF) → return 0
0x15012: CALL 0x933B        ; input + menu cursor; returns BX = item index (0,2,4,6) and ZF
0x15019: CALL 0x3A81 ; CALL 0x2B0E
0x1501F: CALL word ptr CS:[BX+0x5029]     ; ← dispatch table (4 entries, linear 0x15029)
```
| `CS:0x5029` entry | Target | Role |
|---|---|---|
| 0 | `0x5031` | **GAME START**: resets `0x4514/0x3BAF/0x3BB1/0x3BD2/0x3BD4`, `0x9520`, `0x8953`, then `FUN_1000_40F9` (match driver); `DS:0x323D=0xFF` |
| 1 | `0x506B` | `DS:0x3029 = 0; RET` (V.S. BATTLE placeholder; the actual duel is started by the `0x52FE` branch) |
| 2 | `0x506F` | `CALL 0x1437D` (fight-cluster routine; body still unanalyzed) |
| 3 | `0x5076` | saves CONFIG+HIGHSCOR (`0x5F50`,`0x834B`) then runs a full fight loop and re-enters itself (QUIT/exit path) |

> **Inferred**: there are five title strings (`PLAYER SELECT`, `GAME START`, `V.S. BATTLE`,
> `OPTION MODE`, `QUIT TO DOS`) but only four dispatcher entries, so one string belongs to a
> sub-screen — the character-select screen (`PLAYER SELECT`) is entered from GAME START/V.S. BATTLE.
> The exact string→slot mapping still needs the input/menu code (`0x933B`, `0x2AF2`) decoded.

### 3.3 Match driver — `FUN_1000_40F9` (`0x140F9`) — derived

Counters and state (all bytes unless noted):
`0x2A38` “BATTLE ??” (bouts), `0x2A3A` “ROUND ?” (round no.), `0x2A3E` = 3 (continue counter),
`0x2A39`, `0x2A2E`, `0x2A34`, `0x2A3B/0x2A3C` = the two selected characters (`char*2`),
player battle records `0x2A2C`/`0x2A32` = `{+0 char*2, +1 state, +2 win count, +3 ?}`,
select-lock tables `DS:0x44C0` (12-byte in-game→select-order remap) and `DS:0x452B` (12-byte “used” flags),
ladder level `0x451A`.

Per opponent: `0x4850` ×2, `0x4857`; then rounds via `0x4573`; every 3rd/6th/9th win →
next opponent (`0x474B` + `0x5083` re-entry); exit when the ladder ends. Continue/Game-over chain:
`0x4A18` (continue countdown, `DS:0x2A3D` selection, “CONTINUE ?” = `DS:0x2A89`), `0x4AED`,
`0x4B41` (“CREDITS ?” `DS:0x2A7C`), `0x83B3`, `0x893F`.

### 3.4 Round loop — `FUN_1000_5083` (`0x15083`) — observed/derived

Setup (in order): `CX/DX` = characters (`CL`/`CH`, bit7 = CPU) →
`0x4C04` (control remap from CONFIG), `0x5357`, `0x38A0` (asset list at `DS:0x4F54`),
`0x4084` (clear 16 words at `0x259E` = effect list), `0x3ACB`, `0x212A`, `0x1584`,
**`0x3F28` = stage setup** (`DS:0x25BE` = level id, descriptor table `DS:0x2646`,
per-level handler table `DS:0x25CC`), **`0x7416` = fighter construction** (see §4),
`0x04FB`, `0x850A`, `0x8233`, `0x7119`, `0x1A4D`, HUD `0x488D`, backdrop `0x2A8B`, `0x717F`.
Then the loop:

```
do { 0x52EE()                      ; per-frame
     if (DS:0x3233 == 0xFF) {      ; round not decided
        if (0x4CCA() == 0 && energy1 != 0 && energy2 != 0) {
           CX = DS:0x308A; if (CX) { DS:0x308A = CX-1 } }   ; 0x1516B..0x1517B round timer
        else DS:0x3233 = result }
     ... key handling (quit 0x16D5, pause 0x322E/0x322F, `9A66`), debug energy keys:
         F-keys set DS:0x3BBD / DS:0x3BE0 to 100 or 1        ; 0x150xx..
} while (...)
```
**Round timer = `DS:0x308A`** (u16, initialized 200 in `0x152FE`, decremented in the round loop,
0 ⇒ TIME OVER). Round result = `DS:0x3233`.

### 3.5 Per-frame — `FUN_1000_52EE` (`0x152EE`) — observed (disassembly confirmed)

```
0x152D2 FUN_1000_52D2  : game logic
        0x2A9D   frame-tick bookkeeping
        0x04FB   |x1−x2| → DS:0x4C, threshold index (table DS:0x50) → DS:0x4E, DS:0x265
        0x75C5   camera: DS:0x3BA3 = DS:0x3BA7, DS:0x3BA5 = f(min(y1,y2)); 0x7620 ×2 = wall clamp
        0x827B   “BONUS: ?????” score display ([0x3658]/[0x365A] 32-bit)
        0x3FAB   backdrop scroll + per-level handler `(*[DS:0x25EA])()`
        0x2131   hitboxes + hit resolution (see §6) + object pass 0x2594
        0x4F15   stub (empty in this build)
        0x718B   win/time check helper
        0x97E4   keyboard quit/pause handling
0x1959  wait for vertical retrace (IN 0x3DA bit 3)          ; 0x11959
0x1BBA  blit/display frame                                  ; 0x11BBA
0x2AA7  wait `DS:0x16A1 >= 5`                               ; 0x12AA7
0x2AAF  DS:0x1692 = DS:0x16A1; DS:0x1694 = (tick*0x166)>>8  ; 0x12AAF (per-frame time delta)
```
`DS:0x1694` (the scaled delta) is used as the speed scalar for scripted scrolling
(e.g. the story scroller `0x719F`), i.e. the game is frame-rate normalised. **Inferred.**

### 3.6 Round end / winner / HUD

| Addr | Role |
|---|---|
| `0x14573` | round-end dispatcher: compares `DS:0x3BBD` vs `DS:0x3BE0`, sets winner flags, prints `TIME OVER` (`DS:0x2A61`) when carry set, then `JMP CS:[BX+0x45C0]` |
| `CS:0x45C0` table | `{0x4613 (win), 0x45F6 (“DRAW GAME”), 0x4749, 0x45C8}` |
| `0x14613…` | winner path: “PERFECT” (`DS:0x2A59`), “ WINS” (`DS:0x2A9C`), bonus score math with per-char table `DS:0x4332` |
| `0x1474B` | winner display + score/next-round (`0x7FDA`, `0x8382` high-score update, `DS:0x2A3F`) |
| `0x1488D` | fight HUD: builds “ROUND ?”/“BATTLE ??” digit strings (`0x2A46`, `0x2A4F/0x2A50`) and blits |
| `0x14084` | clear effect list (`0x259E`, 16 words) |
| `0x140F9` | match driver (rounds/bouts) |

---

## 4. Fighter / entity state

### 4.1 Fighter struct — `DS:0x3BA9` (P1) and `DS:0x3BCC` (P2), stride `0x23` — observed

Constructor `FUN_1000_7416 (0x17416, AL/AH = chars ±CPU bit)` → `FUN_1000_746E (0x1746E, BP = struct)`,
wall clamp `FUN_1000_7620 (0x17620)`, edge flags from `0x17620`. Field list (offset, size, meaning, evidence):

| Off | Size | Meaning | Evidence |
|---|---|---|---|
| `+0x00` | word | X position (object space) | `MOV DS:[BP+0],BX` (`0x174C7`); `0x04FB` `|x1−x2|` |
| `+0x02` | word | Y position (200 at spawn) | `MOV DS:[BP+2],0xC8` (`0x174CB`); `0x75C5` min(y1,y2) |
| `+0x04` | word | pointer to this fighter’s main **draw object** | `0x174E8`/`0x1752A`; `0x12131` reads `[0x3BA9+4]` |
| `+0x06` | word | unused/unknown (never written by ctor) | — |
| `+0x0A` | word | **flags A**: `0x0001`+`0x0200` at spawn, `0x0400` = CPU, `0x0008` = at left wall, `0x0010` = at right wall, `0x0020` = attack active, `0x0080` = connected, `0x0100` = was hit | `0x1746E`, `0x17620`, `0x121A0`, `0x11080` |
| `+0x0C` | word | **flags B**: bit0 left-wall, bit1 right-wall, bit2 = invulnerable (early-out in `0x121A0`), bit6 cleared by wall clamp | `0x17620`, `0x121A0`, `0x11080` |
| `+0x0B` | byte | 100 at spawn (role unknown) | `0x174A8` |
| `+0x0D` | byte | 2 at spawn | `0x17485` |
| `+0x14` | byte | **ENERGY** (100 = `0x64`) | `0x17480` sets 100; `DS:0x3BBD`/`DS:0x3BE0` = struct+0x14 (key-debug + round-end compare) |
| `+0x15` | byte | 0 at spawn | `0x17499` |
| `+0x16` | byte | 100 at spawn | `0x174AD` |
| `+0x18` | byte | `0xFF` at spawn | `0x174A8` |
| `+0x19` | byte | 0 at spawn | `0x1749E` |
| `+0x1A` | word | 2 at spawn | `0x17485` (as byte `+0x1A`), ctor stores `2` |
| `+0x1B`,`+0x1F`,`+0x21` | byte | 0 at spawn | `0x17494`, `0x1748A`, `0x174A3` |
| `+0x22` | byte | 0 at spawn; used as AI action timer (`10*action/6`) | `0x1748F`, `0x10623` |
| `+0x23` | — | end of stride; **`0x23` bytes total** | P2 base − P1 base = `0x23` |

Player 1 is also flagged `0x0200|0x0001` + `0x0400` when CPU; `DS:0x3C61` = mirror-match flag
(same char for both), `DS:0x3C62/0x3C63` = camera-dirty flags.

### 4.2 Draw object — 32 slots × `0x48` at `DS:0x1B5A` — observed

Allocators: `FUN_1000_3ADC (0x13ADC, flag-only)` and `FUN_1000_3AD9 (0x13AD9, + buffer alloc)`.
Active list: pointer array at `DS:0x245A`, count `DS:0x1B58`. Drawer `FUN_1000_3C7A (0x13C7A)`
(called by `0x13D20`), sprite blit `FUN_1000_1FB3 (0x11FB3)` (nibble-RLE renderer `0x13DEC`).

| Off | Size | Meaning | Evidence |
|---|---|---|---|
| `+0x00` | word | X (world) | `0x169A0`/`0x13C7A` (`*piVar12`) |
| `+0x02` | word | Y | same |
| `+0x04` | word | current **sprite handle** (from `ani` record, `0xF33A`/`-8` = none) | `0x10A7B`, `0x17576` |
| `+0x06` | word | linked “driver” object / parent | `0x17541`, `0x109CF` |
| `+0x08`,`+0x0A` | word | parent-relative offsets added when flag `0x10` set | `0x13C7A` |
| `+0x0C` | word | caller payload pointer (allocator arg, e.g. `0x7665`) | `0x13ADC` |
| `+0x0E` | word | sprite work-buffer segment (allocated by drawer) | `0x13C7A` |
| `+0x10` | word | work-buffer size | `0x13C7A` |
| `+0x12..+0x18` | words | saved draw box x/y/w/h | `0x13C7A` |
| `+0x1A` | word | palette/base offset (P1 `0x3030`, P2 `0x4040`) | `0x174FD`, renderer `DL` |
| `+0x1C` | word | flags: `1` visible, `2` slot in use, `4`/`8` (mirror/immune), `0x10` linked, `0x100`, `0x800` = hidden/immune, `0x1000` = active in physics pass | ctor, `0x13ADC`, `0x2594` |
| `+0x1E` | byte | facing/state flags — **bit2 set for P1** (`TEST …,4`) and used to mirror hitboxes | `0x174F9`, `0x12742` |
| `+0x1F` | byte | shape/state byte copied from driver (`0x20` P1 / `0x28` P2 at spawn) | `0x174EA`, `0x109D2` |
| `+0x20` | word | pointer into the `frm<CHR>` data (hotspot index) | `0x10AB1` |
| `+0x22` | byte | AI timer (from `0x10623`) | `0x1063F`, `0x110B1` |
| `+0x24` | word | X speed (signed; negated for facing) | `0x110ED`, `0x11147` |
| `+0x26`/`+0x28` | 32-bit | fixed-point position accumulator (`+= 0x38000` per frame) | `0x10D8B..0x10D9A` |
| `+0x2C` | word | Y source/target | `0x10D41`, `0x16A38` |
| `+0x2E` | word | 0 (role unknown) | `0x10D39` |
| `+0x30` | word | speed scale (accumulator divisor) | `0x10D20`, `0x10DB7` |
| `+0x32` | byte | **sub-action / frame slot** (added to frm index) | `0x10DE5`, `0x169F4` (`ADD byte [SI+0x32],4`) |
| `+0x33` | byte | `0xFF` at spawn | `0x17508` |
| `+0x34` | byte | mode (`0xFC` = speed mode; `0`/`4` in frame advance) | `0x11151`, `0x10D34` |
| `+0x35` | byte | **animation frame index** (used as `ani[frame*42]`), 125 at spawn | `0x17500`, `0x1273B`, `0x10D88` |
| `+0x36` | byte | player number (0/1) | `0x1750C` |
| `+0x37` | byte | **character = 2×char id** | `0x1750F`, all handle-table lookups |
| `+0x38` | byte | flags: `0x80` action active, `0x40` projectile spawned, low 2 bits copy of sub-action state | `0x10D63`, `0x169BA` |
| `+0x39`,`+0x3A` | byte | sub-action timer / reload (`0xF8`, `0` in idle) | `0x11129`, `0x109DB` |
| `+0x3B`,`+0x3C` | byte | `0xFF` at spawn | `0x1751A` |
| `+0x3D` | byte | **action id** `0x47` at spawn; `0x3A` = stand/idle; `0x34–0x37` = attacks; `0x45/0x46` etc. | `0x17526`, `0x11135`, `0x122D1` |
| `+0x3E..+0x47` | — | unused by observed code | — |

### 4.3 Other gameplay state

| Address | Meaning | Evidence |
|---|---|---|
| `DS:0x15B0` / `DS:0x15D8` | P1 / P2 **hitbox arrays**, 4 × 10 bytes `{valid(0/0xFF), type, left, top, right, bottom}` | `0x12141`/`0x1214A` (`0x2712` output) |
| `DS:0x1576` / `DS:0x157E` | **view structs** `{+0 my player, +2 my hitboxes, +4 my draw obj, +8 opp player, +0xA opp hitboxes, +0xC opp draw obj}` | `0x12150`–`0x1216A`, used by `0x121A0` |
| `DS:0x3BA3` / `DS:0x3BA5` | camera X / Y | `0x75C5`, `0x7620`, `0x1742X` |
| `DS:0x308A` | round timer (u16, 200 → 0) | `0x15316`, `0x1516B` |
| `DS:0x2A3A` / `DS:0x2A38` | HUD “ROUND ?” / “BATTLE ??” counters | `0x1488D`, `0x140F9` |
| `DS:0x3658`/`DS:0x365A` | 32-bit bonus score | `0x14665`, `0x827B` |
| `DS:0x363C`/`0x363E`/`0x365C`/`0x3696`/`0x3716` | 8 projectile slots: frame word, `{reload,count}`, shape pointer, state tables (`DS:0x3624` = coordinate scratch) | `0x6570` |
| `DS:0x15A8..0x15AE`, `0x15A6` | scratch box used by the object pass → projectile collision | `0x12594` |
| `DS:0x25BE` | level/opponent id (0…0x1C; 0x18/0x1C = bonus stages) | `0x3F28`, `0x474B` |
| `DS:0x2646` / `0x25CC` / `0x25EA` / `0x2608` | per-level descriptor ptrs / per-level code ptrs | `0x3F28`, `0x3FAB`, `0x3F88` |
| `DS:0x16A1`,`0x1692`,`0x1694`,`0x1696`,`0x1698`,`0x1690` | frame tick and timing min/max/average scalars | `0x2AA7`, `0x2AAF` |
| `DS:0x5042`,`0x5043` | two device/flag bytes compared against `0xFF` all over the shell (role not proven) | `0x979C`, `0x9CD6`, `0x97E4` |

### 4.4 Debug panel (dead data, useful as field-name hint)

Label table at **`DS:0x385C`**, 16-byte records `[13-char label][00][type(1/2/4)][00]` (two records are
17 bytes), ending at `DS:0x3B8E` followed by `09 09 00 0D 0A 00` and then the live player state at
`0x3BA9`. Labels: `CodeCall, Handle, PlayerData, LastPress, Flags, Movement, FireButs, SpriteX,
SpriteY, SpritePtr, Score, Flags, Flags1, PushDist, SpriteXFrac, SpriteYFrac, Energy, OldEnergy,
DispEnergy, Movement, OldMovement, DizzyTimer, AITimer, WalkTimer, DiffLevel, SpecCount, AIHitMove,
AniTimer, HitCounter, HitRecovery, Recovery, FrameOff, MoveX, MoveY, RealMoveX, RealMoveY, MoveSteps,
FixedPtr, FramePos, HitFrame, JFramePos, Frame, Player, Character, Flags, Action, SubAction,
OldAction, OldSAction, ActionNumb, ActionData, ActionNumbL`.
**Observed**: no instruction anywhere references `DS:0x385C` (byte scan of all immediate operands +
Ghidra xrefs = empty) ⇒ the overlay is not compiled into this build. **Inferred**: the labels name
the intended fields (Energy/AITimer/Action/SubAction/Frame match real fields above); `MoveSteps` and
`FixedPtr` correspond to the allocator payload pointers (`0x7665`, `0x9C1`, …). `Score`/`Flags`/`SpritePtr`
belong to the draw-object group, which matches the 0x48-byte layout in §4.2.

---

## 5. Action / animation execution

### 5.1 Per-character tables (all word-strided, index = `2×char`)

| Table | Handle | Consumer |
|---|---|---|
| `DS:0x60` | `ait<CHR>` | asset preload `0x0944`; action selection `0x10524`/`0x105B1` (`MOV BP,[BX+0x60]`) |
| `DS:0x266` | `ani<CHR>` | frame/hitbox source (`0x2712`, `0x109CF`, `0x10D45` side) |
| `DS:0x27E` | `frm<CHR>` | hotspot/offset lookup (`0x10AB1`) |
| `DS:0x2DE` | byte list → handle indices | preload loop in `0x0944` (`MOV AX,0x2A; MUL BX` over 42-byte records) |
| `DS:0x296` | per-char word (move speed) | `0x110E1` |
| `DS:0x3BF9` | **per-char action handler code ptr** `{0x6993,0x6D1F,0x707B,0x6993,0x70E8,0x7016,0x6AF3,0x6E63,0x70E8,0x70B3,0x6EA4,0x70E8}` | `0x110C1`, `0x10D57` |
| `DS:0x431A` / `0x4332` | win-quote string ptrs / per-char value (3,5,7,3,7,7,7,6,6,4,5,7) | `0x1464F`, `0x14665` |
| `DS:0x1618` | 16 × 2 bytes = (boxW, boxH) nibble-size table (0x10/0x20/0x30/0x40 pairs), ends at `0x1638` | `0x12777`, `0x12780` |
| `DS:0x1638` | 56-byte sound-id table **indexed by action id** (`0x13,0x10,0x14,0x11,0x06…`), `0xFF` = silent | `0x122A9` (action id from `[obj+0x3D]`) |

### 5.2 `ani<CHR>` frame record = **42 bytes**, 200 per character (`8400/42`)

Observed use `0x12712` (`CL=0x2A`, `MUL`, `SI=AX`, `ES:[SI+0x28]`, `ES:[BX+0x10+6i]`) and
`0x109CF`; verified against the decompressed `aniKEN` corpus:

| Off | Meaning |
|---|---|
| `+0x00,+0x04,+0x08` | sprite asset handles (e.g. `sprKENNT152` / `sprKENNB152` / `sprCHLIB0xx`; `0xFFF8`/−8 = none, `0xF33A` = “no change”) |
| `+0x02,+0x06,+0x0A` | signed offsets paired with those sprites (`0x109CF`, `0x10A51`) |
| `+0x0C..+0x0F` | unknown (small ints) |
| `+0x10+6i`, i=0..3 | **hitbox triple** `{xP2, y, xP1}`; `−8` (0xFFF8) in any of the three ⇒ box inactive |
| `+0x28`, `+0x29` | 4 nibbles (boxes 0..3): size index into `DS:0x1618` (per-box w/h) |

### 5.3 Hitbox extraction — `FUN_1000_2712` (`0x12712`) — observed

```
if ([player+0x0A] & 0x400) → clear 4 boxes (CPU has none)
frame = [obj+0x35]; AX = frame*42
acquire(ani[char*2+0x266]);  DX = ([obj+0x1E]&4) ? 4 : 0   ; per-player x mirror
for i in 0..3:
   BX = frame*42 + 0x10 + 6i
   if (ES:[BX]==-8 || ES:[BX+4]==-8 || ES:[BX+2]==-8) → box i invalid else:
       x  = obj.x + ES:[BX+DX]  - 4
       y  = obj.y + ES:[BX+2]   - 4
       w  = table1618[nibble*2]  ; h = table1638-ish pair (table1619)      ; per box
       out = {0xFF, nibble, x, y, x+w+4, y+h+4}
```
Outputs go to `DS:0x15B0` (P1) / `DS:0x15D8` (P2) — 4 × 10-byte entries.

### 5.4 Frame advance / timing — `0x10B7F`, `0x10C32`, `0x10D45`, `0x11080` (unanalyzed region)

```
0x10B7F  computes new frame index from the object's timer/accumulator → AL
0x10D88  MOV [obj+0x35], AL                 ; frame index
0x10D8B  [obj+0x26/+0x28] += 0x38000        ; 32-bit fixed-point accumulator
0x10DA3  if ([obj+0x34]) [obj+0x32] = ((acc*8)/([obj+0x30]))→clamped 0..7 <<2   ; sub-action
0x10D45  BP = char*2; AL = [obj+0x3D] (action); AH = [obj+0x32] (sub-action); BX = 4
0x10D57  CALL SS:[BP+0x3BF9]                ; per-character action handler
0x11080  ... TEST [BX+0xC],0x20 → CALL 0x11114 (return to idle)
         if ([player+0x21]) { [player+0x22] += [player+0x21]; [player+0x21] = 0 }
         else if (action != 0x3A) CALL 0x10623        ; refresh AI timer = 10*action/6
         CALL SS:[BP+0x3BF9]                          ; per-char handler, BX = 6
```
Idle reset `FUN_1000_11114` (`0x11114`): `[obj+0x3D] = 0x3A`, `[obj+0x39] = 0xF8`, `[obj+0x3A] = 0`,
`[obj+0x38] |= 0x80`, `[obj+0x24] = ∓0x0C` (sign = facing), `[obj+0x32] = 0x1C`, `[obj+0x34] = 0xFC`,
`[obj+0x26/+0x28] = 0xFFF0:0`.
Move start `FUN_1000_110CA` (`0x110CA`): speed `CX = DS:0x296[char*2]` (negated when facing left),
`[obj+0x38] |= 0x80`, `[obj+0x34] = 0`, `[obj+0x30] = 8×speed`, `[player+0x0A] &= ~0x100`.

### 5.5 Action selection (AI / scripted) — `0x10524`–`0x10644` (unanalyzed region)

```
0x10524  if ([0x264] == 0xFF) { BP = ait[char*2+0x60]; … }
0x105B1  base = word[ait+6]
         idx = base + distance_index*8 + flags*8 + (rand & 7)
         acquire(ait); AL = ES:[idx]; release
         if (AL in {0x34..0x37} && …) loop again            ; avoid repeating attacks
         [player+0x18] = AL                                  ; pending action id
0x10623  [player+0x22] = (10 * action) / 6                  ; AI commitment timer
```
`distance_index` = `DS:0x4E` (0..7, from `0x04FB` and the threshold table `DS:0x50`),
`flags` = `[player+0x0A] & 3`. **Inferred**: the `ait` record is a 2-level table of action ids
(ids are bytes in `0x00–0x47`; the bytes at `aitKEN[0x3AE…]` include `0x45,0x2C,0x2D,0x12,0x34,0x46,
0x40,0x18,…`). Exact `+0..+0x10` header semantics (including the `+6` base word) still open.

### 5.6 Per-character action handlers (unanalyzed, `0x16993–0x170F8`)

`DS:0x3BF9[char]` → e.g. KEN/RYU = `0x16993`, GUILE = `0x16D1F`, EHONDA = `0x1707B`,
ZANGIEF = `0x170E8`, CHUNLI = `0x17016`, DHALSIM = `0x16AF3`, BLANKA = `0x16E63`,
BALROG = `0x170E8`, VEGA = `0x170B3`, SAGAT = `0x16EA4`, MBISON = `0x170E8`.
Each handler is a **state dispatcher**: `CALL CS:[BX+0x6999]` (`0x16993`), `CS:0x6AF9`,
`CS:0x6D25`, `CS:0x6E69`, `CS:0x6EAA` — i.e. one jump table per character family, indexed by the
action/sub-action, whose cases:
* `ADD byte [SI+0x32],4` — advance the sub-action/frame slot (`0x169F4`, `0x16BBC`, `0x16D5F`, `0x16F07`);
* call `0x170EA` (a shared helper, 5 call sites) — projectile/effect spawner (fireball release);
* `CALL 0x108C6` / `0x19A66` (sound), `0x108D8` (multiply helper);
* allocate a new draw object (`CALL 0x13ADC` with payload `0x6A87`) and initialise it:
  `+0x00 = x`, `+0x1A = palette+0x2020`, `+0x1F = 0x24`, `+0x22 = DX`, `+0x24 = 0x6ABD`,
  `+0x26 = parent obj`, `+0x28 = player`, `+0x1C |= 4|1|0x1000`, `[parent+0x38] |= 0x40` (`0x169FC`–`0x16A5E`);
* per-char tables `DS:0x6999`+ are the **only** place character-specific attack behaviour exists.

---

## 6. Hit detection & resolution

### 6.1 Per-frame pipeline — `FUN_1000_2131` (`0x12131`) — observed

```
0x2712(BP=0x3BA9, DI=0x15B0)      ; P1 hitboxes
0x2712(BP=0x3BCC, DI=0x15D8)      ; P2 hitboxes
build views: [0x1576] = {0x3BA9,0x15B0, obj1, 0, 0x3BCC,0x15D8, obj2}
             [0x157E] = {0x3BCC,0x15D8, obj2, 0, 0x3BA9,0x15B0, obj1}
0x21A0(BP=0x1576) / 0x21A0(BP=0x157E)   ; !directional order flips each frame (0x1685)
0x2594()                                 ; object-list pass: effect/projectile collisions + sparks
```

### 6.2 Overlap test — `FUN_1000_1281E` (`0x1281E`) / `0x128AE` (`0x128AE`)

`0x1281E(AX,BX = attacker left/top, CX,DX = w/h, DI = defender box array, SI = mode)` → carry set
on overlap; used for box1/box3 of the attacker against the defender’s 4 boxes.

### 6.3 Resolution — `FUN_1000_21A0` (`0x121A0`) — observed

Early-outs: `[myObj+0x1C] & 0x800` (hidden), `[oppPlayer+0xC] & 4` (invulnerable),
`[myPlayer+0xA] & 0x100` (already hit) / `& 4`, `[myPlayer+0xC] & 4`.
If `[myPlayer+0xA] & 0x20` (attack active): `0x1232B` check, then
* my box #1 (`myBoxes+0xA`) vs opponent hitbox array (`CALL 0x1281E, SI=5`), and
* my box #3 (`myBoxes+0x1E`, requires `type == 0x0F`) vs opponent
else: `0x1237D` / `0x123EA` / `0x1250D` throw/other paths.
On overlap (`0x12256`):

```
BX = opponent draw object
AX = CS:[char*2 + 0x22B3]          ; per-character block/counter rule
CALL AX  → carry set = hit rejected (0x122CB = RET; 0x122CD = STC;RET; rules 0x122D1/0x122E4/
           0x122F7/0x12302/0x12312 test the opponent's action id 0x34/0x35/0x37 and flags)
else: attacker  [player+0xA] |= 0x0800 | 0x80   (or 0x1000 | 0x80 if [oppPlayer+0xC]&3)
                                [player+0xE] = 0x18
      defender  [player+0xA] |= 0x0100
                [defenderObj+0x1E] = [attackerObj+0x3D]   ; hit-reaction animation id
      sound: AL = DS:0x1638[[attackerObj+0x3D]]; if != 0xFF → CALL 0x9A66
      CALL 0x126B1 → spawn hit-spark draw object at the overlap centre
```
Sprite-object pass `FUN_1000_2594` (`0x12594`): for each object with `+0x1C & 0x1000` and
`[+0x28] & 4 == 0`: `0x3EF9`, `0x24EE` → box in `DS:0x15A8..0x15AE`, `0x6570` (projectile test),
`0x281E` (object overlap) → `0x26B1` (spark).

### 6.4 Projectiles — `FUN_1000_6570` (`0x16570`) — observed/derived

Eight projectile slots: per-slot animation frame at `DS:0x363C` (word), `{reload,count}` bytes at
`DS:0x363E`, per-slot shape pointer at `DS:0x365C` (8 words → `0x3688/0x367A/…`), per-slot state
tables `DS:0x3696` (byte, next-frame select) and `DS:0x3716` (per-state frame); `DS:0x3624` is the
coordinate scratch buffer that gets the shape data + camera offset copied into it each tick.
The updater is gated on `DS:0x25BE == 0x18` (bonus stage) plus `DS:0x375E`; each hit adds 100 to the
32-bit score `DS:0x3658/0x365A` and plays sound `0x13` (`0x165FD`). **Inferred**: the fighters’ own
fireballs are spawned as *draw objects* (§5.6), while `0x6570` is the object/box-vs-projectile
collision tick invoked from the object pass (`0x12594`).

---

## 7. Proposed Rust module structure

| Rust module | Reproduces (addresses) | Key items |
|---|---|---|
| `engine::boot` | `0x104E6`, `0x14F4F`, `0x14F93`, `0x11465` | DS/segment init, asset system init, CONFIG(`0x5F09`)/HIGHSCOR(`0x1831B`), shutdown |
| `engine::main_loop` | `0x14FB9`, `0x14FF0` | screen sequencer + title menu dispatch (`CS:0x5029` → 4 modes), quit flag |
| `engine::mode::attract` | `0x1979C`, `0x19CD6`, `0x197E4`, `0x19A20`, `0x1985D` | demo/attract ticks, bank flags `0x5042/0x5043` |
| `engine::mode::story` | `0x1719F` | ending/story text scroller (asset list `DS:0x3840`, text ptrs `DS:0x3855`+, delta-time scroll `0x1694`) |
| `engine::mode::menus` | `0x15CFC`, `0x15DE8`, `0x15E4A`, `0x2AF2`/`0x2B0E`/`0x933B` | menu framework (item list, cursor, confirm) |
| `fight::match` | `0x140F9`, `0x152FE`, `0x4A18`, `0x4AED`, `0x4B41` | rounds/bouts ladder, `0x2A2C/0x2A32` battle records, CONTINUE/GAME OVER, `0x451A` ladder |
| `fight::round` | `0x15083`, `0x152EE`, `0x152D2`, `0x14573`, `0x1474B`, `0x15031`, `0x1506F`, `0x15076` | round setup/loop, per-frame order, timer `0x308A`, KO/TIME OVER, winner + bonus |
| `fight::fighter` | `0x17416`, `0x1746E`, `0x17466`, `0x10944`, `0x17620`, `0x175C5` | fighter struct + 0x23 layout, spawn, per-char asset preload, wall clamp, camera |
| `fight::entity` (`DrawObject`) | `0x13AD9`, `0x13ADC`, `0x13C7A`, `0x13D20`, `0x13DEC`, `0x11FB3`, `0x11E9B`, `0x11EFE` | 32-slot object pool, active list `0x245A`, nibble-RLE sprite blit, palette |
| `fight::action` | `0x12712` (read side), `0x10B7F`, `0x10C32`, `0x10D45`, `0x10AB1`, `0x10524`, `0x105B1`, `0x10623`, `0x11114`, `0x110CA`, `0x11080` | `ani` frame records (42 B), hitbox build, fixed-point frame advance, `frm` hotspots, AI/action selection, idle/move |
| `fight::char_handlers` | `0x16993`, `0x16AF3`, `0x16D1F`, `0x16E63`, `0x16EA4`, `0x17016`, `0x1707B`, `0x170B3`, `0x170E8`, `0x170EA` | per-character action state machines + projectile spawn |
| `fight::hit` | `0x12131`, `0x121A0`, `0x1281E`, `0x128AE`, `0x1232B`, `0x1237D`, `0x123EA`, `0x1250D`, `0x126B1`, `0x12594`, `0x122B3` rules | hitbox views, block/counter rules, damage/reaction, sparks |
| `fight::projectile` | `0x16570`, `0x3696`, `0x3716`, `0x3624` | 8-slot fireball/projectile tick + scoring |
| `fight::hud` | `0x1488D`, `0x14084`, `0x1401B`, `0x827B`, `0x474B` | ROUND/BATTLE/TIME/VITAL/BONUS rendering, effect list |
| `fight::stage` | `0x13F28`, `0x13FAB`, `0x13F88`, `0x140B0`/`0x140D2`, `0x2646`, `0x25CC/0x25EA/0x2608` | per-level descriptor + per-level handler, backdrop scroll, camera integration |
| `assets::ani` / `assets::ait` / `assets::frm` | `0x0944`, `0x12712`, `0x10AB1`, `0x105B1` | decoders for the three per-char tables (formats in §5.2, §5.5) |
| `platform::{input,time,video}` | `0x14D15`, `0x14CCA`, `0x1933B`, `0x11959`, `0x12AA7`, `0x12AAF`, `0x11BBA`, `0x11B94` | keyboard/joystick, retrace wait, tick pacing, blit |

Suggested porting order: `fight::fighter` + `fight::entity` → `fight::action` (hitbox build) →
`fight::hit` → `fight::round`/`match` → `fight::char_handlers` → `hud`/`stage`.

---

## 8. Confidence and unknowns

**High confidence (observed in code / data)**

* Outer state machine, title dispatch table `CS:0x5029`, match driver `0x140F9`, round loop
  `0x15083`, per-frame `0x152EE`/`0x152D2` ordering, round timer `DS:0x308A`, round result `0x3233`.
* Player struct base/stride (`0x3BA9`/`0x3BCC`, 0x23) and fields X/Y, draw-object ptr, flags,
  energy (`+0x14`), plus the wall-clamp semantics.
* Draw-object pool geometry (32 × 0x48 at `DS:0x1B5A`, active list `0x245A`, count `0x1B58`) and the
  fields listed in §4.2 (each has direct instruction evidence).
* `ani` = 200 × 42-byte records with 4 × 3-word hitboxes at `+0x10` and nibble bytes at `+0x28/29`;
  hitbox build algorithm (`0x12712`) and the P1/P2 x-mirror (`[obj+0x1E]&4`).
* Hit pipeline order and the reaction/block dispatch via `CS:0x22B3` + sound table `0x1638`.
* Debug label table is unreferenced (dead) and sits immediately before the live player state.

**Medium confidence / inferred**

* `ait` = action-id matrix indexed by `base(+6) + distance_index*8 + flags*8 + (rand&7)`; the exact
  header (`0..0x10`) and the meaning of `flags` need one more pass over `0x10524–0x10644`.
* `frm` = hotspot/offset tables addressed by `obj+0x20` + `obj+0x32` (`0x10AB1`), 3 bytes/entry
  (flag/0xFF, dx, dy).
* Fixed-point timing: accumulator `+0x26/+0x28 += 0x38000`, divisor `+0x30`, sub-action derivation
  `0x10DA3–0x10DE5`; the exact tick rate (the loop waits for `DS:0x16A1 >= 5`) not yet measured.
* `[obj+0x1F]`, `[obj+0x39/0x3A]`, `[obj+0x34]`, the remaining flag bits of `+0x1C/+0x38`, player
  fields `+0x0B/+0x0D/+0x15/+0x16/+0x18/+0x19/+0x1A/+0x1B/+0x1F/+0x21` and the low half of the
  player struct’s flag words.
* Stage tables `DS:0x2646`/`0x25CC` indexing (`[0x25BE]` may be pre-doubled like character ids).

**Unknown / to be traced**

* Bodies of the unanalyzed clusters: **`0x10524–0x11160`** (sprite/frame update, action selection),
  **`0x16993–0x170F8`** (per-character action handlers), **`0x17620–0x17E00`** (per-entity updater
  reached from `0x18EC4`/`0x1904B`/`0x148C7`/`0x1D0A2`), `0x1437D`, `0x150xx` mode bodies.
  Ghidra created no functions there (jump-table targets), so these must be decoded with the linear
  disassembler or a forced-function pass.
* `0x170EA` (shared helper called from five action handlers) — probably the projectile/effect spawner
  but not yet decoded.
* `DS:0x5042/0x5043` device flags; `DS:0x322E/0x322F/0x3235/0x3236` shell flags; `0x25B0`, `0x3080`
  tables (look like digit/format tables used by the HUD).
* Exact behaviour of the per-character *block rules* (`0x122CB–0x1232A`) — action-id comparisons only
  (`0x34`, `0x35`, `0x37`) with attack-flag tests; needs one pass per rule.
