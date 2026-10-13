# Action execution — per-frame fighter update, AI action pick, hit reaction, per-character handlers

Target: `SF2.EXE` (MS-DOS 1995), same conventions as `fight-engine.md` / `character-tables.md`:
**linear = file_offset + 0xFFA0**, image/CS base linear `0x10000` (CS-relative offset `o` ⇔ linear `0x10000+o`),
DS base linear `0x1AA40` (DS-relative `d` ⇔ linear `0x1AA40+d`), all values little-endian.

Evidence classes: **observed** (bytes/instructions read directly), **derived** (reproducible from
observed data), **inferred** (stated as such), **UNKNOWN** (explicitly unresolved).

Everything below was produced with a linear 16-bit disassembler over the regions Ghidra never analysed
plus a byte-scan for `E8/E9` call sites (so the call-site lists are complete for direct near calls).

---

## 1. Summary

1. **The fighter's per-frame logic is not called from the round loop.** The round loop
   (`0x15083` → `0x152EE` → `0x152D2`) only does camera/HUD/stage/hit-detection. The *entity* update+draw
   pass (`0x13B6C`, which contains the object-process pass `0x13C07`) is registered as an entry of the
   global **effect list `DS:0x259E`** by the object-system init `0x13A32`
   (`mov ax,0x1b52; call 0x14093`, and the struct at `DS:0x1B52` = `{fn = 0x3B6C, data = 0x3BA3}`), and
   the effect list is run every frame from `0x13FAB` → `0x140B0` (`call word ptr [di]`).
2. Each fighter owns **four draw objects** (payload/process functions in the `+0x0C` slot, invoked by
   `0x13C07` with `SI` = the object): **driver `0x7665`** (the fighter brain), **layer T `0x9C1`**,
   **layer B `0x9C8`** (sprite staging), and **shadow `0xA8A`** (ground shadow). The pool order equals the
   allocation order, so the driver is processed before its layers in the same frame.
3. The driver's per-frame brain is `0x17665` → `0x176CA` (7 ordered sub-steps) → action commit
   `0x176FC` → `0x10C5D` (script commit) → `0x10D45` (sub-action + per-char hooks + movement/physics).
4. **AI**: an action is picked only when the player's input–device selector word is 0
   (`DS:0x2F0C`/`DS:0x2F15` = a copy of the device mask `DS:0x2F08`/`DS:0x2F0A`; `0` = no device = CPU
   or demo). The pick is `0x10524` → `0x10645`/`0x105B1`/`0x1068D`, all reading per-character `ait`
   tables. The "AI timer" `player[0x22]` is refreshed as `ait[hdr[5]][action] / 6` (**not** `10*action/6`),
   and is *extended* by the hit-stop value `player[0x21]` when the fighter is hit.
5. **Hit reaction is deferred by one frame:** the hit resolution (`0x12256` / `0x123EA`) only writes
   `defender[0xA] |= 0x100` and `defender[0x1E] = attacker's action`; the *next* frame `0x17849` consumes
   it: damage/score/hit-stop (`0x1857C`, `0x1836B`), then a hard-coded reaction **script id** is selected
   (`0x3D` generic hit-stun, `0x32` Blanka's bite, `0x0D` fall, …) and committed as action `0x3A`.
6. **Projectiles** (`0x16993–0x170F8`) are draw objects: the per-character handler's **mode 4** tick
   spawns them when `(frame_slot<<8)|action` matches, and `0x123EA` (called per defender from `0x121A0`)
   tests every object with `+0x1C & 0x400` against the defender's hitboxes using the object's *sprite*
   box — the projectile's `+0x2C` (the spawning action) becomes the defender's reaction id.

---

## 2. Where the fighter update sits in one frame

### 2.1 Fight frame pipeline — `FUN_1000_52D2` (`0x152D2`, called from `0x152EE`) — observed

```
0x152D2  CALL 0x12A9D          ; frame-tick bookkeeping (DS:0x1690/0x1692/0x1694…)
0x152D5  CALL 0x104FB          ; |X1-X2| → DS:0x4C, distance bucket DS:0x4E / DS:0x265
0x152D8  CALL 0x175C5          ; camera + wall clamp (0x17620 per player), camera Y
0x152DB  CALL 0x1827B          ; HUD/score panel
0x152DE  CALL 0x13FAB          ; stage: backdrop + level handler + << effect list >> ← fighters update+draw
0x152E1  CALL 0x12131          ; hitboxes (0x12712 ×2) + hit resolution (0x121A0 ×2) + object pass (0x12594)
0x152E4  CALL 0x14F15          ; stub (RET) in this build
0x152E7  CALL 0x1718B          ; pause helper
0x152EA  CALL 0x197E4          ; keyboard quit/pause handling
```

`0x13FAB` (observed):

```
0x13FAB  if camera == [0x25C6]  → 0x140D2 (run 16-entry list at DS:0x25BC, entries {fn,data})
         else                   → 0x11B94, 0x1401B (backdrop redraw), 0x13BF3 (clear obj[+0x1C] bit 0x20)
0x13FD2  CALL word ptr [bx+0x25EA]      ; per-level scroll handler (RET stub for normal levels)
0x13FDA  CALL 0x140B0                   ; run effect list DS:0x259E → entry {fn,data} pairs
```
`0x140B0`: `for 16 slots of DS:0x259E: if ptr != 0 and [ptr] != 0 → call [ptr]`.
`0x13A32` (object-system init, called from the round setup `0x150B3` = `0x13ACB`) does
`mov ax,0x1b52; call 0x14093`; `DS:0x1B52 = 0x3B6C` (`0x13B6C`) and `DS:0x1B54 = 0x3BA3` (camera X) —
so the entity pass is effect-list entry #1 and runs **before** hit detection each frame.
**Inferred consequence:** a hit registered in frame *N* is reacted to in frame *N+1*.

### 2.2 Entity pass — `0x13B6C` / `0x13C07` — observed

```
0x13B6C  CALL 0x13C07      ; process every in-use object
0x13B77  CALL 0x13C46      ; sort active list DS:0x245A by object[+0x1F] ascending (draw order key)
0x13B7A  CALL 0x13C7A      ; blit every active object (0x13DEC nibble sprite, mirrored if [obj+0x1C]&4)
0x13B7D  CALL 0x13D29      ; overlay pass (sprite blit + optional 0x13E5B/0x13E94)
0x13C07: for SI = DS:0x1B5A .. +32*0x48:
            if (!([si+0x1C] & 2)) continue                       ; slot free
            if ( [si+0x1C] & 0x800)  continue                     ; hidden  ← see §8 (CPU question)
            CALL word ptr [si+0x0C]                                ; ← the object's PROCESS
            if ([si+0x1C] & 1) append SI to DS:0x245A
```
Draw-object `+0x0C` = process entry (CS offset), set by the allocator `0x13ADC` from `DI`
(`mov word ptr [si+0xC],di` at `0x13AF5`) — i.e. the `DI` argument of every `CALL 0x13AD9/0x13ADC`
spawn site (see the object table above and the projectile spawns in §6).
`0x13C07` skips objects whose `+0x1C` bit 0x800 is set and objects not marked `2` (in-use).

Fighter object inventory (created by `FUN_1000_746E` `0x1746E`, observed):

| # | payload | spawn site | role | key fields at spawn |
|---|---|---|---|---|
| 1 | `0x7665` | `0x174E2` | **driver** / fighter brain | `+0x1F` = 0x20/0x28, `+0x35` = 0x7D, `+0x36` = player#, `+0x37` = char*2, `+0x38` = 1, `+0x39` = 0x18, `+0x3D` = 0x47, `+0x1C` |= 0x800 when the player has the 0x400 flag |
| 2 | `0x9C1` | `0x1753B` | sprite layer T (`BP=0`) | `+0x06` = driver, `+0x1C` |= 0x10 (linked) |
| 3 | `0x9C8` | `0x1754B` | sprite layer B (`BP=8`) | `+0x06` = driver, `+0x1C` |= 0x10 |
| 4 | `0xA8A` | `0x1755B` | ground shadow | `+0x1F` = 0x70, `+0x1A` = 0x1010, `+0x04` = 0xF2D7, `+0x1C` |= 0x10\|0x100 |

`0x10A8A` (shadow, observed): `X = -0x26`, `Y = 0xC8 + [0x25C8] - player[2] + 8`, visible, no physics.

---

## 3. One frame of a fighter — step list (addresses + evidence)

### 3.1 Driver process `FUN_1000_7665` → the entry point of everything (observed)

```
0x17665  CMP byte [0x322F],0xFF ; JZ 0x176C9 (RET)      ; global freeze (pause / hit-stop) → no update
0x1766C  DI = 0x3BA9 (P1), BX = 0x3BCC (P2), BP = 0x2F0C
0x17675  if (obj[0x36] != 0) { xchg DI,BX; BP = 0x2F15 } ; pick my player struct by the object's player #
0x17680  TEST word [DI+0xA],0x400 ; JNZ RET              ; ← see §8, UNKNOWN
0x17687  if ((DI[0xC] & 4) && !(BX[0xC] & 0x40))         ; I am frozen & the opponent not yet done
0x17695     push SI… SI = [BX+4]; CALL 0x17665  (recursion → opponent first)
0x176A5  if (DI[0xC] & 0x40) RET                         ; already updated this frame
0x176AC  CALL 0x176CA            ; ── THE FIGHTER LOGIC (CF = "action already final, skip commit")
0x176B1  if (!CF) CALL 0x176FC   ; ── script/action commit + sub-action + movement
0x176B4  obj[0x00] = DI[0x00] - camera[0x3BA3]           ; place the driver object (world→screen X)
0x176BC  obj[0x02] = [0x25C8] + DI[0x02]                 ; … and Y (ground line + player Y)
0x176C5  DI[0xC] |= 0x40                                 ; "done this frame"
```
Registers inside `0x176CA` (and everything it calls): **SI** = my driver draw object,
**DI** = my player struct, **BX** = opponent player struct, **BP** = my input record (`0x2F0C`/`0x2F15`).
This register convention is stable through `0x176CA`, `0x17929`, `0x179B7`, `0x1075A`, `0x10524`,
`0x10C5D`, `0x10D45`, `0x10D7C…0x110C9`.

### 3.2 The ordered sub-steps of `FUN_1000_76CA` (`0x176CA`) — observed

| order | call | role |
|---|---|---|
| 1 | `0x17737` | dizzy / stun timer `DI[0x19]`: skipped while `DI[0xA] & 0x2000`; if `DI[0x19] >= 0x32` and `obj[0x38] & 0x10`: if (`obj[0x39]` in {8,0x34,0xF4} or `DI[0x1E]` in 0x28..0x33) → `DI[0x19] = 0x31`, `DI[0xC] |= 0x10`, `CALL 0x12990` (effect), script 0x30, else `DI[0x19] = 0x33` + script 0x0D; while `DI[0x19] < 0x32 && != 0` (and `CS:[0xC0E3] == 0xFF`): decrement, and once below `DS:0x3C59[char]` clear `DI[0xC] & 0x10` with script 6 |
| 2 | `0x177B3` | KO / round-lost: `DI[0xA] & 0x8000` → `DI[0xA] |= 0x2000`, clear `DI[0xC]` bits 4/0x80/0x20, script `0x0E` (+1 if battle-record `+1 != 0`) via `0x17BD7`, mode-8 handler `[bp+0x3BF9]` with `BX=8`, then suppress |
| 3 | `0x17803` | `DI[0x14] == 0` (energy) → `[0x322E] = 0xFF`, `DI[0xA] |= 0x2000`, script `0x0D` via `0x17BD7` |
| 4 | `0x17830` | hit-connected timer: `if (DI[0xA] & 0x80 && DI[0x20] != 0) { if (--DI[0x20]) DI[0xA] &= ~0x80; }` |
| 5 | `0x17849` | **hit reaction** (`DI[0xA] & 0x100`) — §5 |
| 6 | `0x17929` | **input / AI → requested action in AL** — §4 |
| 7 | `0x179B7` | **action start** (AL = requested action) — §3.3 |

### 3.3 Action start / commit — observed

```
0x179B7  if (AL == 0x38) AL = 0x43 ; if (AL == 0x39) AL = 0x44     ; "jump" aliases
0x179C3  AH = obj[0x38]; CL = obj[0x3D]
0x179C9  if ((AH & 4) && !(AH & 8)) RET                            ; animation lock (frm flags 0x04/0x80)
0x179D6  if (DI[0x1F] == 0) CALL 0x11306                           ; facing update (unless locked)
0x179DF  if (AL in {0x3E,0x3F,0x40,0x41}) → 0x17A7C                 ; throws/grab bypass the filters
0x179FB  if (CL in {0x42,0x43,0x44} && !(obj[0x38]&0x10) && AL not in 0x18..0x2B) RET  ; no actions in a jump
0x17A1D  if (AL in {0x42,0x43,0x44} && AL == CL) { obj[0x3B] = 0xFF; CALL 0x11317 }
0x17A34  if (DI[0xA] & 4) { …attack/counter filters, then 0x108C6 (idle)… }
0x17A7C  if (AL == 0x28) { grab: needs opp[0xA]&3 == 2, opp action ∉ 0x34..0x37, |dx| < 0x50, |dy| < 0x40 }
0x17ACC  if (AL in 0x2C..0x33) { throw: needs !(opp[0xA]&0x400), opp[0xA]&3 != 2, |dx| < 0x50, |dy| < 0x40,
                                 DS:0x3C64[char*4 + (AL-0x2C)] == 1, then optional position swap 0x17B80 }
0x17BB3  CALL 0x108C6                         ; reject → idle (0x47/0x48 by DI[0xA]&3)
0x17BB6  CALL [bp+0x3BF9] with BX=0           ; mode-0 hook (action-start; KEN sound, DHALSIM cancel,
                                              ;                       HONDA/VEGA/CHUNLI/BLANKA/SAGAT sounds)
0x17BE1  AL = action → scan ait[hdr[1]] pairs downward: CH = script id, CL = param (CL=0 if action>=0x40)
0x17C31  if (AL == 0x3E/0x3F) { CH = 0x0A + (AL-0x3E); CL = 0; DI[0xA] |= 0x40 }   ; the two throws
0x17C4C  commit:
           0x17C51  if (AL in {0x40,0x41}) { DI[0x1A] = 0; DI[0x1B] = (rand&7)+2 }
           0x17C69  obj[0x33] = 0xFF
           0x17C6D  if (AL != 0x3A && AL < 0x40) {
                       0x17C79 if (AL == 0x28) obj[0x38] &= 0x7F
                       0x17C7D CALL 0x17C9E: DI[0x20] = ait[hdr[6]][AL]; DI[0xA] &= ~0x80
                       0x17C80 DI[0x22] = 0 }
           0x17C84  obj[0x3D] = AL            ; action id
           0x17C87  CH <<= 2                  ; script id → frm header offset
           0x17C8B  CALL 0x176E0              ; DS:0x3BEF {action,pad,handler} (0x40/0x41 → 0x108FA)
           0x17C8E  obj[0x39] = CH (script*4) ; obj[0x3A] = CL (param)
           0x17C94  if (DI[0x1F] == 0) CALL 0x11306
```
`0x17BD7` (used by KO/dizzy/hit/recovery): `AL = 0x3A; CL = 0; obj[0x38] &= 0x7F; jmp 0x17C4C` — i.e.
"return to stand with the script id passed in **CH**" (CH is *not* looked up from `ait`).

### 3.4 Script commit + tick + movement — `0x176FC`, `0x10C5D`, `0x10D45` — observed

```
0x176FC  CH = obj[0x39]; CL = obj[0x3A]; CALL 0x10C5D        ; commit the pending script
0x17709  obj[0x3B] = obj[0x39]; obj[0x3C] = obj[0x3A]        ; remember old script/param
0x17715  if (!(DI[0xA] & 0x400) && [0x25BE] >= 0x18)         ; bonus stages: fix up X on a new action
             BX[0x00] ±= 0x4F  (sign = sign(getX - myX))
```
```
0x10C5D  if (CH == obj[0x3B]) JMP 0x10D45                    ; same script → no re-init (frame carries on)
0x10C65  CALL [bp+0x3BF9] with BX=2 (AL = obj[0x3D])         ; mode-2 hook (script change / attack sounds)
0x10C7D  obj[0x38] = (obj[0x38] | 4) & ~0x08 & ~0x10
0x10C8D  obj[0x20] = AX (= CH = script*4)                    ; frm header offset
0x10C92  obj[0x32] = 0                                        ; frame-list byte offset
0x10C95  DI[0xA] &= ~0x20                                     ; attack inactive
0x10C99  DI[0xC] &= ~0x100
0x10C9E  if (CH < 0x0C) DI[0xC] |= 0x100                      ; scripts 0..2 = "attack-enabled" set?
0x10CA8  AX = header flags (0x10C32) → if bit 0x20: DI[0xA] |= 0x20 ; if bit 0x80: obj[0x38] |= 8
0x10CBD  if (action in 0x42..0x44) {                          ; jumps
             obj[0x38] |= 0x80
             CX = (action==0x42) ? 0 : ±DS:0x2C6[char*2]      ; X speed (sign vs facing)
             obj[0x24] = CX; obj[0x32] = 0x1C or 0; obj[0x34] = 0xFC or 4
             obj[0x28]/obj[0x26] = 0; obj[0x30] = speed*8 }
         else if (action in 0x18..0x27) { obj[0x32] = 0; obj[0x34] = 4 }
         else { obj[0x2E] = 0; obj[0x2C] = DI[0x02] }          ; Y target
0x10D45  CALL [bp+0x3BF9] with BX=4, AX = (obj[0x32]<<8)|obj[0x3D]   ; ← MODE 4 = per-frame tick hook
0x10D5E  AX = header flags (0x10C32)
0x10D63  obj[0x38] = (obj[0x38] & 0xFC) | (flags & 3)
0x10D74  DI[0xA]   = (DI[0xA]   & 0xFFFC) | (flags & 3)
0x10D7C  if (!(obj[0x38] & 0x80)) JMP 0x10E4F                 ; not action-active → follow/throw path
```
**Movement / physics path** (obj[0x38] & 0x80 set, i.e. a script with speed):

```
0x10D85  CALL 0x10B7F → AL = CURRENT frame ani_index; obj[0x35] = AL   ; note: does NOT advance
                                                                      ; (also maps frm flags: bit0x20→DI[0xA]|=0x20, bit0x80→obj[0x38]|=8)
0x10D8B  obj[0x26/+0x28] += 0x038000                          ; 32-bit fixed-point accumulator
0x10D9D  if (obj[0x34] != 0) {                                ; sub-action from the accumulator
             obj[0x32] = 4 * clamp( 4 - ((acc*8) / (2*obj[0x30])) , 0..7 )     ; (7-x if obj[0x34]==4)
         }
0x10DEA  CX = obj[0x24]; CALL 0x112B0                         ; "step": tests X+CX / Y+DX bounds,
                                                              ;  CF=1 → step finished (ground/wall/hit box)
0x10DF0  if (!CF) → 0x10E42: if (DI[0xA] & 0x1000) → 0x11002 (pushback!) else 0x1101E
0x10DF2  if (CF) {                                            ; step finished
            if (obj[0x34] != 0) { obj[0x38] = (obj[0x38]&0xFC)|1; DI[0xA] = (DI[0xA]&~3)|1; obj[0x38] |= 0x10 }
            else { DI[0xC] |= 0x100; sound 4; CALL 0x1857C/0x1836B on the opponent (landing on the opponent!) }
            obj[0x2E] = 0; DX = obj[0x2C] - DI[0x02] }
0x10E4F  else-branch (obj[0x38] & 0x80 clear):
           0x10E4F if (DI[0xC] & 0x400) {                     ; I am being thrown → follow the thrower
                      BX = [BX+4] (opp obj); if (oppobj[0x32] != obj[0x32]) { obj[0x32] = oppobj[0x32]-4;
                      CALL 0x10AB1; obj[0x24] += CX; obj[0x28] += DX }   ; advance my frame from theirs +
                      CX = obj[0x24]; DX = obj[0x28]; if (oppobj[0x1E] & 4) CX = -CX    ; frame dx/dy
                      DI[0x00] = BX[0x00] + CX; DI[0x02] = BX[0x02] + DX  ; my pos = thrower pos + offset
                      → 0x11034 }
           0x10E8C if (obj[0x26] & 0x40) { … ballistic path: DI[0x02] = obj[0x2C];
                      CALL 0x10AB1 → AL = frame; sign of CX from frame bits 0/1 or facing;
                      CALL 0x17F64 (trajectory solver) → obj[0x24], obj[0x22](word), obj[0x28], obj[0x26],
                      obj[0x2E] = step count; obj[0x2C] = Y target + DI[0x02]; … 0x10EE1 (step loop, BP=0x20
                      iterations max) → 0x1101E }
           0x10F31 else { if (DI[0xC] & 4) { 0x10F4A block/parry release: if the opponent is in action 0x2D
                      (0x24 for KEN) and my frame slot == 0x20 → CALL 0x110CA (start walk), clear bits }
                     0x10F81  DI[0x1F] = max(0, 5 +2*(opp[0xC]&4) -1*(opp[0xC]&0x20==0)
                                              -1*(my script != 0x18) -1*(my script != 0x10) - 5)
                      …frame advance: CALL 0x10AB1 (the advancer); if the opponent is 0x400:
                          sync my frame slot to the opponent's; if the new frame == header event ANI
                          (0x10BFD) and the opponent is frozen → CALL 0x1857C/0x1836B on the opponent
                      → 0x11034 }
0x1101E  if (DI[0xA] & 0x800) { CX = -CX; DI[0xA] &= ~0x800 }   ; reverse the pushback (attack hit)
0x1102C  CALL 0x11172                                            ; wall clamp + camera + wall pushing
0x1102F  DI[0x00] += CX; DI[0x02] += DX                          ; ← the position update
0x11034  if (!(obj[0x38] & 0x10) || obj[0x2E] != 0) → RET        ; (obj[0x38] bit 0x10 = "list ended/loop")
0x11047  obj[0x38] &= ~4                                         ; clear ... 
0x1104C  if (DI[0xC] & 0x100) { DI[0x1A] = 0; DI[0x1B] = 0 }
0x1105B  if (BX[0xC] & 0x80) {                                   ; opponent is in "hit/throw reaction"
             if (BX[0xC] & 0x400) { sound 6; XCHG DI,BX; SI = [BX+4]; CALL 0x1857C; CALL 0x1836B }
                                                     ; throw confirmed: damage the opponent (DI = opp)
             BX[0xC] &= ~0x400; BX[0xC] &= ~4         ; release the grab/invulnerable bits
             if (BX[0xC] & 0x20) CALL 0x11114         ; opponent → idle reset (0x3A) }
0x11087/0x1109A  then: BP = char*2; BX = 6 (mode-6 hook), AL = obj[0x3D], AH = DI[0x21]
0x110AD  if (AH != 0) { DI[0x22] += AH; DI[0x21] = 0 }            ; AI timer += hit-stop
0x110BA  else if (obj[0x3D] != 0x3A) CALL 0x10623                 ; DI[0x22] = ait[hdr[5]][action]/6
0x110C1  CALL [bp+0x3BF9] with BX=6 (AL = action, AH = hit-stop) ; mode-6 hook (all RET in this build?)
```

### 3.5 Layer processes — `0x109C1` (BP=0, T) / `0x109C8` (BP=8, B) — observed

```
0x109C1  BP = 0 ; CALL 0x109CF      0x109C8  BP = 8 ; CALL 0x109CF
0x109CF  BX = obj[0x06] (driver); obj[0x1F] = BX[0x1F] (draw priority)
         AL = BX[0x35] (frame); if (AL >= 0xC8) fatal
         DI = frame*42 (ani record); obj[0x1C] bits 2/3 = BX[0x1C] bits 2/3
         if T/B handle == 0xF33A (blank) → Y = previous X/Y path, else:
           bp=0:  Y = ani.T_y − h(spr T) + 0x10
           bp=8:  Y = ani.T_y − ani.B_y − h(spr B) + 0x10
         X = (obj[0x1C] & 4) ? ani.x_flip : ani.x        (word at +0x02+BP / +0x06+BP)
         obj[0x1C] &= ~1; if the handle != 0xF33A and the parent is not hidden → obj[0x04] = handle, visible
```

### 3.6 Frame advance / staging helpers — observed

| Addr | Role |
|---|---|
| `0x10AB1` | **advance** one frame: `bx = list_base + obj[0x32] + 4`; if `es:[bx] == 0xFF` apply header flags (0x04/0x08/0x10 → restart, `obj[0x38] |= 0x10`), else `obj[0x32] = bx - list_base`, `obj[0x35] = ani_index`, map frame flags bit 0x04→`obj[0x1C]` bit 2, 0x08→bit 3; **returns CX = frame dx, DX = frame dy** |
| `0x10B7F` | **read** the current frame (no advance) + map header flags (`0x20`→`player[0xA] |= 0x20`, `0x80`→`obj[0x38] |= 8`); returns AL = ani_index |
| `0x10C32` | returns the committed script's header flags byte (`frm[obj[0x20]+2]`) |
| `0x10C26` | same but returns `0xFFFF` if bit 0x20 set |
| `0x10BFD` | returns the header's `event_ani` byte (`frm[obj[0x20]+3]`) |

---

## 4. Action selection: input vs AI

### 4.1 `FUN_1000_7929` (`0x17929`) — observed

```
0x17929  if (DI[0x22] != 0) { --DI[0x22]; AL = 0x108C6() (= 0x47/0x48 "stand"); goto tail }  ; AI timer
0x17937  if (DI[0xC] & 0x200) { AL = 0x108C6(); goto tail }
0x1793E  if (DI[0xC] & 4) RET                           ; frozen → keep the current action
0x17945  if (word [BP] == 0) goto 0x17975               ; ← no input device ⇒ AI
   ── human path ──
0x1794C  if (DI[0xC] & 8) { AL = 0x108C6(); goto tail }
0x17953  AL = [BP+4]; cli; AL |= [BP+8]; [BP+8] = 0; sti; and byte [BP+6],0xF7
0x17967  DI[0x17] = AL (raw input byte); DI[0x18] = AH
0x1796A  CALL 0x17D74            ; bonus stages: move X by ±0x4F if input bits 4/8
0x1796D  CALL 0x1075A            ; AL = action from input  (see §4.4)
         goto tail
   ── AI path ──
0x17975  if (DI[0x1B] != 0) { --DI[0x1B]; goto 0x1799A }
0x1797C  if (DI[0x1A] == 0) { CALL 0x10524; goto 0x179A8 }       ; ← the AI decision (§4.2)
0x17983  --DI[0x1A]
0x17986  if ((DI[0xA] & 3) == 2) goto 0x179A2 (RET)
0x17990  CALL 0x10645 → CF clear ⇒ 0x179A8 ; CF set ⇒ 0x108C6()  ; counter-attack attempt
0x1799A  --DI[0x1B]; CALL 0x10645 → …
0x179A8  DI[0x17] = 0
tail 0x179AC: DI[0xC] &= ~(0x800|0x1000); DI[0xA] &= ~0x4000; RET   ; AL = action request
```
Tail semantics: `AL` is then consumed by `0x179B7` (§3.3). `DI[0x1E]`/`DI[0x1F]`/`DI[0x20]`/`DI[0x21]`
/`DI[0x22]` are the reaction/hit-stop/AI timers (see §5).

### 4.2 The AI decision — `FUN_1000_0524` (`0x10524`) — observed

```
0x10524  AL = 0x47
0x10527  if ([0x264] != 0xFF) goto 0x1059F              ; ← global gate, semantics UNKNOWN (see §8)
0x1052E  if ((opp[0xA] & 0x20) && (DI[0xA]&3) != 2)     ; opponent is attacking me
0x1054B      CALL 0x1068D; if (CF == 0 /*decided*/) goto 0x10553   ; counter/defend (see below)
0x10550  CALL 0x105B1                                   ; ← distance-bucket action pick (§4.3)
0x10553  ; AL = action
0x10554  DI[0x1A] = clamp(((obj[0x1C] ^ 7) >> 1) - ((obj[0x37] - 0x12) >> 1), 0, …)   ; decision delay
0x10571  DI[0x1B] = 0
0x10576  if (obj[0x3D] in 0x18..0x2B) AL = 0x47          ; mid-"walk" actions → stand instead
0x10585  if (AL == 0x43 && (obj[0x1E] & 4)) AL = 0x44    ; jump direction mirrored by facing
0x10593  if (AL == 0x44 && (obj[0x1E] & 4)) AL = 0x43
0x1059F  if (AL in {0x42,0x43,0x44}) DI[0x1A] = 0
0x105AF  RET  (AL = the requested action)
```

### 4.3 The three AI sub-decisions

**(a) distance-bucket pick `0x105B1`** (the fallback / default):

```
0x105B1  bucket = [0x4E] (0..7, from 0x104FB thresholds DS:0x50)
         idx = bucket*24 + (DI[0xA] & 3)*8 + (rand() & 7)
         acquire(ait); base = es:[6] (= hdr[3]); AL = es:[base + idx]; release
0x105FE  if (AL in 0x34..0x37 && [obj+0x1C] < 3 && DI[0x18] in 0x34..0x37) re-roll   ; don't repeat attacks
0x1061B  DI[0x18] = AL                                  ; "pending/previous action"
```
`hdr[3]` = the 4th word of the `ait` header (942 for the 8 world warriors, 958 for the 4 bosses);
section length 192 bytes ⇒ the formula can read up to 7 bytes past the end for bucket 7 + flags 3 + rand 7
(a quirk of the original — clamp in the port).

**(b) `0x1068D` — "attack/approach decision"** (called when the opponent is attacking):

```
0x1068D  if ((DI[0xA]&3) == 2) → fail (CF)
         if (DI[0xC] & 0x800) && !(DI[0xA] & 0x4000) → fail
         if (obj[0x3D] in {0x3E,0x3F}) → pass (CF clear, AL unchanged!)
         CALL 0x1073A → CF set → { ah = opp[0xA]&3; AL = 0x3E or 0x3F; → pass }
                                     ; AL = 0x3E if ah == 1 else 0x3F   (the two throws)
         else: acquire ait; BX = es:[0] (= hdr[0]); bl = es:[hdr[0] + obj[0x3D]]   ; per-action byte
               release; if (bl == 0) → fail
               AL = 0; BX <<= 4; BX += AL; BX += [0x4E]; BX += (rand() & 8)
               acquire; base = es:[4] (= hdr[2]); AL = es:[base + BX]; release
               if (AL == 0) → fail else CF clear (AL = action)
```
`0x1073A` = a small "interrupt/space" roll: `BL = char*2; BL = BL*8 + DI[0x1C]*2; AX = rand() & 0x7FFF;
CF = (AX >= word[DS:0x3432 + BX])` — a per-character/per-state probability table (`DS:0x3432`,
`8*char + 2*player[0x1C]` ⇒ 2 bytes per (char, state)).

**(c) `0x10645` — "counter/punish attempt"** (called on the AI-timer expiry):

```
0x10645  if (!(opp[0xA] & 0x20)) fail ; if ((DI[0xA]&3) == 2) fail
         if ((DI[0xC] & 0x800) && !(DI[0xA] & 0x4000)) fail
         AL = obj[0x3D]; if (AL in {0x3E,0x3F}) pass
         CALL 0x1073A → CF → { AL = 0x3E/0x3F by opp[0xA]&3 } else pass with AL = obj[0x3D]
```
So `0x10645`/`0x1068D` mostly yield the actions `0x3E`/`0x3F` (the throw/grab actions) as a
counter-move when the opponent is attacking — the *actual* moves come from `0x105B1`.

**(d) AI timing (`0x10623`, the mode-6 tail)** — observed:

```
0x10623  AH = AL (the action); AL = DI[0x22]
0x1062A  if (AH >= 0x40) → store unchanged
0x1062F  BX = AH; AX = 0x0A → AL = ait[hdr[5]][action]  (section = header word at asset+0x0A)
0x10639  AL /= 6 → DI[0x22] = AL
```
So **AI timer = `ait[hdr[5]][action] / 6`** (frames), and `player[0x21]` (set by `0x1857C` = `ait[hdr[5]][hit
action] / 5 | 1`) is *added* to `player[0x22]` at `0x110B1` when a hit lands on me — i.e. **the same
per-action byte drives both the commitment time (`/6`) and the hit-stun/AI freeze (`/5`)**.

### 4.4 Human input decoder — `FUN_1000_075A` (`0x1075A`) — observed (summary)

```
0x1075A  AL = input byte (DI[0x17]); AH = opp[0xA]
0x10763  BL = AL & 0xF → BL = DS:0x2FDF[BL]      ; 16-byte table: nibble → action/class
0x1076A  if (BL == 8) → 0x108C6 (stand 0x47/0x48) ; if (BL == 0xFF) BL = 0
0x10776  if ([0x302A] == 0xFF) → 0x107D9         ; "special moves enabled"
0x1077D  AL &= 0x7F; if (AL == 0) → 0x107D4 (AL = 0x47) ; if (AL & 0x10) → 0x107D9
0x1078A  if ((opp[0xA] & 0x20) && (DI[0xA]&3) != 2) {          ; opponent attacking me, not "level 2"
             facing right (obj[0x1E]&4): if (AL & 4) → 0x107AA
             facing left:                if (AL & 8) → 0x107AA
             0x107AA: AL = 0x3E if (DI[0xA]&3) == 1 else 0x3F → 0x108AD (tail) }
0x107B9  AL = DS:0x23C[BL + 8*(DI[0xA]&3)]        ; per-input/per-state action table
0x107D9  special/attack path:
            BL = AL & 0xF; AL = (AL & 0xF0) | DS:0x254[BL]; BL = DS:0x2FDF[AL & 0xF]
            SI = obj; BX = 0x78 + char*8; AH = obj[0x36]; [0x3027] = facing?0:0xFF
            CALL 0x14E1B                        ; *** motion/hold recogniser (8-byte per-char data at DS:0x78) ***
            if (CF) { AL = CL + 0x34            ; ← attack action 0x34..0x37 by motion strength
                      if (char == 8 && AL == 0x35) AL = 0x33 }   ; BLANKA special-case
tail 0x108AD: if (obj[0x1E] & 4) swap AL 0x40 ↔ 0x41            ; jump direction by facing
```
`DS:0x2FDF` = 16 bytes (input-nibble → class), `DS:0x254` = 16 bytes (remap), `DS:0x23C` = the
per-(input-class, level) action table, `DS:0x78 + char*8` = per-character 8-byte motion patterns.
(These tables were not dumped in this pass; the *reading* code is exact.)

---

## 5. Hit reaction & recovery path

### 5.1 What the hit resolution writes (frame N) — observed

From `0x121A0` → `0x12256` (`bp` = the view struct `{0 myPlayer, +2 myBoxes, +4 myObj, +8 oppPlayer,
+0xA oppBoxes, +0xC oppObj}`):

```
0x12256  SI = [bp] (attacker struct), DI = [bp+8] (defender struct), BX = [bp+0xC] (defender obj)
0x12262  AX = CS:[0x22B3 + defObj[0x37]] ; BX = [DI+4] (defender driver obj); CLC; CALL AX
         if (CF) RET                                  ; per-char block/counter rule rejected it
0x12274  CALL 0x126B1                                 ; hit spark
0x1227D  BX = [SI+4] (attacker driver obj)
0x12280  AX = 0x800; if (defPlayer[0xC] & 3) { AX = 0x1000; attacker[0xE] = 0x18 }
0x12292  AX |= 0x80;  attacker[0xA] |= AX             ; 0x80 = "hit connected" (+0x800 / +0x1000)
0x12298  defender[0xA] |= 0x100                       ; *** "was hit" ***
0x1229D  defender[0x1E] = [BX+0x3D]                   ; *** the attacker's ACTION = reaction id ***
0x122A7  sound DS:0x1638[[BX+0x3D]] (0xFF = silent)
```
Projectile hits are resolved by `0x123EA` (called from `0x121A0`'s flow, per defender):
for each active object with `+0x1C & 0x400` and not `+0x28 & 4`, the object's **sprite box**
(`0x13EF9` size + `0x124EE` position, honouring the `+0x10` parent link) is tested against the
defender's hitbox array (`0x1281E`, SI=5). On overlap:

```
0x12442  if (obj[0x1C] & 0x1000) {                     ; a projectile draw object
0x1244D     if ((obj[0x28] & 1) == defenderObj[0x36]) skip     ; don't hit your own thrower
0x1245B     ownerChar = [obj[0x26]][0x37]              ; parent object's char
0x12463     if (ownerChar == 2 /*HONDA*/ && defender char in {0,KEN 6,DHALSIM} && def action == 0x34) skip
0x1247D     per-char block rule (CS:0x22B3) with STC (projectile flag); if (CF) skip
0x124A1     defender[0x1E] = obj[0x2C]                 ; *** projectile's stored action = reaction id ***
0x124AB     defender[0xA] |= 0x4000 }
0x124B0  CALL 0x126B1 (spark)
0x124B7  if (obj[0x1C] & 0x2000) defender[0xC] |= 0x1000
0x124C3  defender[0xA] |= 0x100                        ; "was hit"
0x124C8  obj[0x0C] = obj[0x24]                         ; projectile switches to its hit process (0x6ABD…)
0x124D2  obj[0x28] |= 4; obj[0x1C] &= ~0x400 ; STC
```
Pushback (body-vs-body) — `0x1250D` (called at the end of `0x121A0`): my boxes 0 and 2 vs the
opponent's hitboxes tests touching (`[0x1684] = 0xFF`), and box 3 vs the opponent's boxes with SI=8
sets `[player+0x0E] = 0x18` and `[player+0xA] |= 0x1000` ("apply pushback"); the movement stage then
consumes it at `0x11002` (`CX = player[0x0E]`, clear 0x1000) and negates CX if `player[0xA] & 0x800`.

### 5.2 The reaction (frame N+1) — `FUN_1000_7849` (`0x17849`) — observed

```
0x17849  if (!(DI[0xA] & 0x100)) RET                 ; nothing to do
0x17853  DI[0xC] |= 0x800                            ; "reacting"
0x17858  DI[0xA] &= ~0x100
0x1785D  if (DI[0xC] & 0x1000) → 0x178F9 (reaction script = 0x3D)
0x17867  AL = DI[0x1E] (the action that hit me)
         if (AL in 0x34..0x37 && (DI[0xA] & 0x40)) {  ; I was throwing (bit 0x40) when hit
             ah = DI[0xA]&3; ch = opp[0xA]&3
             if (ch == 2) { if (ah == 1) RET } else if (ah == ch) RET }   ; no reaction (trade/interrupt)
0x1789B  PUSH SI; SI = [BX+4] (opponent driver obj); CALL 0x1857C; CALL 0x1836B; POP SI
         ;  ^ damage+stun for ME (DI), score for ME, using the OPPONENT's ait/hdr[4..6]
0x178A6  DI[0x1A] = 0; DI[0x22] = 0; DI[0x1B] = 0; DI[0x21] = 0      ; drop AI/decision timers
0x178B6  if (DI[0xC] & 0x10) { DI[0x19] = 0; DI[0xC] &= ~0x10 }
0x178C5  AL = DI[0x1E]; BX = opp char index
         if (AL == DS:0x3CC4[oppChar]) { CH = 0x0D (script 13); → commit }   ; NOTE: table = {6,4,5,6,4,4,4,6,…}
         if (AL in 0x34..0x37) {
             if (DI[0xA] & 0x40) RET                        ; I'm throwing → keep it
             if (oppChar == 11 (M.BISON)) → CH = 0x3D (script 61)
             if (oppChar == 6  (DHALSIM)) → CH = 0x3D
             if (oppChar == 7  (BLANKA) && AL == 0x34) → CH = 0x32 (script 50) }
0x1790F  CALL 0x11317 (facing); if ((DI[0xA]&3) != 2) DI[0x02] = 0xC8     ; snap to ground
0x17922  CALL 0x17BD7                          ; commit action 0x3A with script CH (below)
0x17925  → 0x17BD4 (pop; CLC; RET)
```
Anomaly (observed): `DS:0x3CC4` = `06 04 05 06 04 04 04 06 FF FF FF FF` — values 4/5/6 can never
equal `DI[0x1E]` (0x34..0x37), so that comparison is **dead in the shipped data** unless `DI[0x1E]`
holds a small id. Documented as-is; do not "fix" it in the port without a runtime check.

### 5.3 Damage / stun / score — `FUN_1000_857C` (`0x1857C`) and `FUN_1000_836B` (`0x1836B`) — observed

```
0x1857C  (DI = the player to configure, SI = the attacker's driver object; BX = scratch)
  if ([0x25BE] >= 0x18) → 0x1864A (bonus stages skip)
  AL/AX = 0x0A → AL = ait_attacker[hdr[5]][DI[0x1E]]     ; section = header word at asset+0x0A
  [0x43BA] = DI[0x1E]                                    ; the attack action
  DI[0x21] = max(1, (value / 5) | 1)                     ; *** hit-stop / extra AI freeze ***
  AL = value * 2                                         ; damage base
  BX = the defender's input-device selector ([0x2F0C] / [0x2F15] by player #)
      if (BX == 0) AX = (AX * 0xD9) >> 8                 ; no-device (CPU) scaling
  CL = [BX+0x1C] * 3                                     ; (small selector value → byte read at DS:0x1C+sel)
  [0x43BB] = 0
  BX = [BX+4] → if ([BX+0x37] in {4,0xC,0xE} = ZANGIEF/DHALSIM/BLANKA) [0x43BB] = 0xFF
                and if (DI[0x1E] == 0x2C) CL >>= 1
  AH += CL ; if (AH <= 0) AH = 1
  if ((DI[0xA] & 0x40) && DI[0x1E] in 0x34..0x37) AH = 6          ; attacks during a throw do 6
  AH >>= 1
  if (!(DI[0xC] & 0x10)) {
      if ([0x43BB] == 0xFF) AH >>= 1
      DI[0x19] += AH                                  ; *** dizzy timer += damage
      DI[0x14] = max(0, DI[0x14] - AH)                 ; *** energy -= damage
      if (DI[0x14] < 0x19) [0x43B9] = 0xFF             ; "low energy" flag (used by 0x1870C) }
0x1836B  (score)  AX = 8 → AX = ait_attacker[hdr[4]][DI[0x1E]]   ; section = header word at asset+8
  CALL 0x18382: DI[0x06]/DI[0x08] (32-bit) += AX
                if > [0x41FE]/[0x4200] → store as the new high score, [0x42DA] = 0xFF
                DI[0xA] |= 0x200                       ; "score changed" → HUD redraw (0x182CA)
```
So the per-action byte in `ait[hdr[4]]` = score, `ait[hdr[5]]` = power (÷6 → AI timer, ÷5 → hit-stop,
×2 → damage), and `ait[hdr[6]]` = `player[0x20]` (a per-action word/byte used by `0x17C9E`).

### 5.4 Recovery / lockout timers (player struct) — observed

| field | meaning | evidence |
|---|---|---|
| `+0x0E` | pushback delta (0x18) | `0x12589`, consumed at `0x11007` |
| `+0x0F`..`+0x13` | (not touched by this code) | — |
| `+0x14` | energy | `0x18635` |
| `+0x17` | raw input byte (last press) | `0x17967` |
| `+0x18` | previous/decided action (AI re-roll guard) | `0x1061B`, `0x105FE` |
| `+0x19` | dizzy timer (`>= 0x32` ⇒ dizzy) | `0x1773E`, `0x18631` |
| `+0x1A` | AI decision delay (frames) | `0x1056E`, `0x1797C` |
| `+0x1B` | AI secondary delay | `0x10571`, `0x17975` |
| `+0x1E` | **the action id of the attack that hit me** | `0x122A0`, `0x124A1` |
| `+0x1F` | movement lockout (frames; set 1..2 on block/interrupt, decremented at `0x10F3F`) | `0x10FA3`, `0x169E8`, `0x16AD6` |
| `+0x20` | hit-connected timer (decrements, clears `+0xA` bit 0x80) | `0x17830` |
| `+0x21` | pending hit-stop (added to `+0x22`) | `0x185A2`, `0x110A7` |
| `+0x22` | **AI/action commitment timer** | `0x1063F`, `0x1792F` |
| `+0xC` bits | 4 frozen/invulnerable, 0x10 recovery? , 0x20 , 0x40 updated-this-frame, 0x80 reaction, 0x100 "attack set", 0x200 , **0x400 grabbed/being thrown**, 0x800 reaction, 0x1000 projectile-hit/low-energy-set, 0x8000 | §3.3/§5 |
| `+0xA` bits | 0x01 (spawn), 0x0008 left wall, 0x0010 right wall, 0x20 attack active, 0x40 in a throw, 0x80 hit connected, 0x100 was hit, 0x200 score-dirty, **0x400 CPU**, 0x800 reverse pushback, 0x1000 apply pushback, 0x2000 KO'd, 0x4000 projectile-hit flag, 0x8000 KO/lost | §3/§5 |

Recovery is therefore: (a) hit-stun reaction action `0x3A` with a hard-coded script (18..29 frames,
flags `0x51`), (b) `player[0x22]` (AI timer) freezing the next action pick, (c) `player[0x1F]`
movement lockout, (d) `player[0x0E]` pushback.

---

## 6. Per-character handlers — inventory (all 12) — observed

`DS:0x3BF9[char*2]` → stub `CALL word ptr cs:[bx+T]; RET`; `BX` = the *mode*, `AX` = `(frame_slot<<8)|action`,
`SI` = the driver object, `DI` = my player struct, `BX`→opponent player struct (reused as the argument). 
Call sites: mode 0 = `0x17BC2` (action start), mode 2 = `0x10C75` (script change), mode 4 = `0x10D57`
(per-frame tick), mode 6 = `0x110C1` (per-frame tail, `AH` = hit-stop), mode 8 = `0x177F5`
(KO/round end, `CH` = `0x0E` or `0x0F`).

| char | stub (mode table) | mode 0 (start) | mode 2 | mode 4 (tick) | mode 6 | mode 8 |
|---|---|---|---|---|---|---|
| KEN / RYU | `0x16993` (`0x16999`) | `0x169A3` sound 7 for action 0x34 | RET | **`0x169D7` fireball** (§6.1) | RET | RET |
| GUILE | `0x16D1F` (`0x16D25`) | `0x16D2F` sound 0x18 for 0x35 | RET | **`0x16D42` sonic boom** (`cmp ax,0xC35`) | RET | RET |
| E.HONDA | `0x1707B` (`0x17081`) | `0x1708B` sound 0x17 for 0x35 | RET | RET | RET | `0x170A5` KO sound 0x15 (`CH==0x0E`) |
| ZANGIEF | `0x170E8` | RET | RET | RET | RET | RET |
| CHUN-LI | `0x17016` (`0x1701C`) | `0x17026` sound 1 for 0x35 | RET | `0x17039` sound 0x17 at frame `0x5C` of action 0x34 (latch `[0x37A7]`) | RET | `0x1705F` KO sounds 0x0D/0x0E |
| DHALSIM | `0x16AF3` (`0x16AF9`) | `0x16B03` cancel rule (`[0x1684]!=0xFF` blocks action 0x0C) | `0x16B12` sounds 0x1D for 0x34/0x35 | **`0x16B28` yoga fire 0x34@0x14 / yoga flame 0x35@0x10** | RET | RET |
| BLANKA | `0x16E63` (`0x16E69`) | `0x16E73` sounds 2 (0x2C) / 5 (0x34) | RET | RET | RET | `0x16E91` KO sound 0x0F (`CH==0x0E/0x0F`) |
| BALROG | `0x170E8` | RET | RET | RET | RET | RET |
| VEGA | `0x170B3` (`0x170B9`) | `0x170C3` clear `[0x37A6]` for 0x34 | RET | `0x170CE` `ax==0x3C34` (action 0x34 slot 0x3C) → latch `[0x37A6]`, sound 0x16 | RET | RET |
| SAGAT | `0x16EA4` (`0x16EAA`) | `0x16EB4` sounds 0x19 (0x34..0x36) | RET | **`0x16ED4` tiger shot 0x35@0x14, and 0x34/0x36 @0x0C** | RET | RET |
| M.BISON | `0x170E8` | RET | RET | RET | RET | RET |

`0x170E8`/`0x170E9` = `RET; RET` — so ZANGIEF, BALROG and M.BISON have **no** character code in this
build; their specials are pure data (scripts/ani), driven by the generic movement code.
`0x170EA` (5 call sites: `0x169F1`, `0x16B36`, `0x16BB9`, `0x16D5C`, `0x16F04`), observed:

```
0x170EA  PUSH BX; BX = [BX+4] (the OPPONENT's driver object)
0x170EE  obj[0x1F] = 0x20        ; my draw priority
0x170F2  oppobj[0x1F] = 0x28     ; opponent drawn after me (list sorted ascending at 0x13C46)
```
It is a **draw-order helper** called before spawning the projectile, not the spawner.

### 6.1 KEN / RYU fireball — `0x169D7` (mode 4) — observed

```
0x169D7  if ((AX != 0x1035)) RET              ; action 0x35, frame slot 0x10
0x169DF  if (obj[0x38] & 0x40) RET            ; one projectile per action
0x169E8  if (DI[0x1F] != 0) RET               ; movement lockout
0x169F1  CALL 0x170EA                         ; draw-order bump
0x169F4  obj[0x32] += 4                       ; advance one animation step (skip the "charge" frame)
0x169F8  CX = obj[0x00] + (facingRight ? 0x1E : -0x34)
         DX = DI[0x02] - 0x34                 ; spawn position
0x16A16  BP = obj (parent); obj[0x38] |= 0x40
0x16A1C  AX = obj[0x1A] + 0x2020              ; palette for the projectile
0x16A25  DI = 0x6A87 → CALL 0x13ADC           ; *** allocate a draw object with process 0x16A87 ***
0x16A2B  new[0x00] = CX (X); new[0x1F] = 0x24 (priority); new[0x1A] = AX (palette)
0x16A35  AX = 0x6ABD (the "hit" process, installed later by 0x123EA)
0x16A38  new[0x22] = DX (Y); new[0x29] = BL (±0x1E X speed); new[0x2A] = 0; new[0x2B] = 0
0x16A46  new[0x24] = 0x6ABD ; new[0x26] = BP (parent obj) ; new[0x2C] = CL (= 0x35, action!)
0x16A4F  new[0x28] = BH (player #)
0x16A56  new[0x1C] |= (4 if facing right) | 1 | 0x1000 | 0x100 | 0x400
0x16A71  DI[0x1F] = 4 * max(1, 4 - [0x1690])  ; my movement lockout from the frame-rate scalar
```
The projectile's own process `0x16A87` (observed):

```
0x16A87  if ([0x322F] == 0xFF) RET                  ; freeze
0x16A8E  X += new[0x29]; if (X <= -0x30 || X >= 0x150) → remove
0x16AA3  Y = [0x25C8] + new[0x22]; new[0x04] = DS:0x376C[new[0x2B]]; new[0x2B] ^= 2   ; 2-frame flicker
remove (0x16AC1/0x16ABD → 0x16AC3):
         parent[0x38] &= ~0x40; the parent's player DI[0x1F] = 0
         if (CF /*was hit*/ && parent[0x3D] == 0x35 && parent[0x32] <= 0x10) parent[0x32] += 4
         CALL 0x13B4A (free the object)
```

### 6.2 DHALSIM — `0x16B28` (mode 4) — observed
* `AX == 0x1434` (yoga fire): `0x16B36` → `0x170EA`, sound 0x0C, spawn an **attached** effect:
  `0x13ADC`, `new[0x06] = parent`, `new[0x1C] |= AX` where `AX = 0x1411 | 4` (0x10 = linked!),
  `new[0x20]/[0x22]` = ox/oy, `new[0x24] = 0x6C4F` (sprite-offset table), `new[0x26] = parent`,
  `new[0x28] = player#`, `new[0x2C] = action`; then `0x16C4F` (the effect process) picks the sprite from
  `DS:0x3770` by the parent's frame slot and self-destructs when the table entry is 0 or
  `parent[0x38] & 0x40` clears.
* `AX == 0x1035` (yoga flame): `0x16BB9` (sound 0x0B) → a second effect with payload `0x6CB3`, table `DS:0x3796`.

### 6.3 Others
* GUILE `0x16D42`: `AX == 0xC35` → `0x16D5C` (payload `0x6DF3`, sprite table `0x6E2D`, Y offset `-0x34`, X offset ±0x1E).
* SAGAT `0x16ED4`: `AX == 0x1435` → sound 0x1A + latch `[0x37A6]`; `AX == 0xC34/0xC36` → `0x16F04`
  (payload + `0x170EA`).
* CHUN-LI `0x17039`, VEGA `0x170CE`: **sound-only** hooks (no object is spawned); their specials must be
  fully data-driven (`frm`/`ani` + the generic movement/physics).
* All "latch" globals `[0x37A6]`/`[0x37A7]` are one-shot sound gates reset by the matching mode-0 handler.

---

## 7. Step-by-step pseudo-code ("one frame of a fighter", port-ready)

```rust
// called from the entity pass (0x13C07) with the fighter's DRIVER object
fn fighter_process(obj: &mut DrawObject, g: &mut Game) {           // 0x17665
    if g.freeze_322f == 0xFF { return; }                           // 0x17665
    let (my, opp, inp) = match obj.player_no {                     // 0x17675..0x1767D
        0 => (P1, P2, 0x2F0C),
        _ => (P2, P1, 0x2F15),
    };
    if my.flags_a & 0x400 != 0 { return; }                         // 0x17680  (see §8)
    if my.flags_b & 4 != 0 && opp.flags_b & 0x40 == 0 {            // 0x17687
        fighter_process(&mut g.objects[opp.draw_obj]);             // 0x17695 (opponent first, throw case)
    }
    if my.flags_b & 0x40 != 0 { return; }                          // 0x176A5
    let committed = fighter_logic(obj, my, opp, inp, g);           // 0x176CA
    if !committed { script_commit(obj, my, opp, inp, g); }         // 0x176FC (0x10C5D -> 0x10D45 -> movement)
    obj.x = my.x - g.camera_x;                                     // 0x176B4
    obj.y = g.ground_y_25c8 + my.y;                                // 0x176BC
    my.flags_b |= 0x40;                                            // 0x176C5
}

fn fighter_logic(obj, my, opp, inp, g) -> bool {                    // 0x176CA
    dizzy_step(my, obj, g);                                         // 0x17737
    ko_step(my, obj, g);                                            // 0x177B3
    if my.energy == 0 { energy_zero_step(my, obj, g); }              // 0x17803
    hit_connect_timer(my, obj);                                     // 0x17830
    hit_reaction(my, opp, obj, g);                                   // 0x17849  (§5.2)
    let req = choose_action(obj, my, opp, inp, g);                  // 0x17929  (§4)
    try_start_action(req, obj, my, opp, g);                          // 0x179B7  (§3.3)
    return false;                                                    // (CF clear → script_commit runs)
}

fn script_commit(obj, my, opp, inp, g) {                             // 0x176FC
    let (script, param) = (obj.sub_action_39, obj.sub_action_3a);     // = player[0x39]/[0x3A]
    if script != obj.old_script_3b {                                  // 0x10C5D
        char_hook(obj, 2, obj.action);                                // 0x10C75 (mode 2)
        obj.flags_38 = (obj.flags_38 | 4) & !0x08 & !0x10;
        obj.frm_ptr_20 = script;                                      // script*4 = frm header offset
        obj.frame_slot_32 = 0;
        my.flags_a &= !0x20;  my.flags_b &= !0x100;
        if script < 0x0C { my.flags_b |= 0x100; }
        let hdr = frm_header(obj.frm_ptr_20);
        if hdr.flags & 0x20 != 0 { my.flags_a |= 0x20; }
        if hdr.flags & 0x80 != 0 { obj.flags_38 |= 8; }
        match obj.action {                                            // 0x10CBD
            a @ 0x42..=0x44 => { obj.flags_38 |= 0x80;
                obj.speed_24 = if a == 0x42 {0} else {jump_speed(g, obj, a)};
                obj.frame_slot_32 = if a == 0x42 {0} else {0x1C};
                obj.mode_34 = if a == 0x42 {4} else {0xFC};
                obj.acc_26_28 = 0; obj.scale_30 = obj.speed_24 * 8; }
            a @ 0x18..=0x27 => { obj.frame_slot_32 = 0; obj.mode_34 = 4; }
            _ => { obj.steps_2e = 0; obj.y_target_2c = my.y; }
        }
    }
    obj.old_script_3b = script;  obj.old_param_3c = param;             // 0x17709
    char_hook(obj, 4, (obj.frame_slot_32 as u16) << 8 | obj.action as u16);  // 0x10D57 (spawns projectiles)
    let f = frm_header(obj.frm_ptr_20).flags;                          // 0x10D5E
    obj.flags_38 = (obj.flags_38 & 0xFC) | (f & 3);
    my.flags_a   = (my.flags_a   & 0xFFFC) | (f & 3);
    if obj.flags_38 & 0x80 != 0 { action_active_step(obj, my, opp, g); } else { idle_step(obj, my, opp, g); }
}
```
`action_active_step` = the fixed-point accumulator + `0x112B0` step + `0x11172` wall clamp + `player.x/y += (cx,dy)`
(§3.4), followed by the common tail `0x11034`: mode-6 hook, AI-timer refresh (`0x10623`), hit-apply to the
opponent when `obj.flags_38 & 0x10` and `obj.steps_2e == 0`.

---

## 8. Confidence and unknowns

**High confidence (byte-exact, all addresses re-read from the image):**
* The effect-list discovery: `DS:0x1B52 = 0x3B6C`, registered by `0x13A32` → `0x14093`, executed by `0x140B0`
  from `0x13FAB`; therefore the entity pass (`0x13C07` → object `+0x0C` processes) runs inside the gameplay
  frame *before* `0x12131`.
* `0x17665 → 0x176CA → 0x176FC → 0x10C5D → 0x10D45 → 0x10D7C…0x110C9` order, the mode 0/2/4/6/8 hooks and
  their call sites, the register convention (SI = driver obj, DI = my player, BX = opponent, BP = input record).
* The AI: device-selector gate (`[0x2F0C]/[0x2F15] == 0`), `0x10524`'s decision flow, the three
  sub-decisions, the `ait` header words used (`hdr[0]`, `hdr[1]`, `hdr[2]`, `hdr[3]`, `hdr[4]`, `hdr[5]`,
  `hdr[6]`), the RNG (`0x17F49` = 32-bit LFSR seeded by `int 1Ah` at `0x17F3C`), and
  **AI timer = `ait[hdr[5]][action]/6`** (correcting `fight-engine.md` §5.5's `10*action/6`).
* The hit-reaction chain `0x12256`/`0x123EA` (write) → `0x17849` (consume on the next frame) → `0x1857C`
  (damage/hit-stop/dizzy) → `0x1836B` (score) → `0x17BD7` (hard-coded reaction script), and the `+0x19/+0x1F/
  +0x20/+0x21/+0x22` timers.
* The per-character handler table (all 12 characters, all 5 modes) and every projectile spawn site with its
  arguments; `0x170EA` is a draw-order helper, not a spawner.

**Medium confidence / inferred:**
* Names of `player[0x1F]` ("movement lockout"), `player[0x20]` ("hit-connected window"), `player[0x21]`
  ("hit-stop"), `player[0xC]`/`player[0xA]` bit *names* — behaviours are verified, names are inferred.
* `0x10524`'s `[0x264] == 0xFF` gate, `0x1073A`'s probability table `DS:0x3432`, the meaning of
  `player[0xA] & 3` ("level/stance index", 2 = special), and the `0x3C64` throw-cross table semantics.
* `0x1857C`'s `[BX+0x1C]` term: `BX` = the *device selector* (0/2/4/6/8) so this reads DS:0x1C+selector —
  probably a per-device damage modifier; not verified. The `*0xD9>>8` scaling applies to the player with no
  device (the AI side).
* `0x12131` calls `0x121A0` in alternating order (`[0x1685]`), so on even/odd frames the two fighters trade
  priority for melee hits — the port must reproduce this for faithful trades.

**UNKNOWN / must be resolved before/while porting:**
1. **The CPU-player gating (highest priority).** `0x1746E` sets `player[0xA] |= 0x400` when the character
   byte's bit 7 is set, marks the *driver object* `+0x1C |= 0x800` (`0x1752E–0x17536`), and `0x13C07`
   skips every `0x800` object (`0x13C19`). `0x17665` likewise returns for a `0x400` player (`0x17680`), and
   the only other route into `0x176CA` is the opponent-first recursion at `0x17695`, which hits the same
   guard. Since the AI code is only reachable through `0x176CA`, **no code path found statically runs the AI
   for a 0x400-flagged fighter.** All direct call sites of `0x176CA`, `0x10524`, `0x179B7` are accounted for
   (byte-scan for `E8/E9`). Candidate explanations: (a) 0x400/0x800 is used only in the attract demo and/or
   the bonus stages (where the "player" is a target object), and normal 1P CPU opponents are *not* flagged
   0x400 (they only have no input device, which is what the AI gate checks); or (b) a runtime path clears
   0x800 that does not exist as an immediate-encoded write. **Porting recommendation:** implement the AI as
   "device selector == 0" (which is what `0x17929` tests) and keep driving CPU fighters' driver objects;
   treat 0x400/0x800 as a *special-mode* marker. Verify with a runtime trace (watch `DS:0x3BCC+0xA` bit 0x400
   in a 1P match).
2. `DS:0x3CC4`'s dead comparison (§5.2) and whether any character has an action-reaction override.
3. The `ait` header sections `hdr[7]..hdr[10]` (192-byte blocks) and `hdr[0]`/`hdr[2]` value semantics:
   `hdr[0][action]` is a 0/nonzero *gate* (attack allowed) and `hdr[2]` is indexed
   `16*x + bucket + (rand & 8)` with an unknown `x` (likely `player[0xC]`-derived); the *ranges* are verified,
   the meaning is not.
4. `0x14E1B` (the motion/hold recogniser, 8 bytes per character at `DS:0x78`) — not decoded here; it is
   what turns joystick motions into actions 0x34..0x37.
5. `DS:0x23C`/`0x254`/`0x2FDF` (input tables), `DS:0x2C6` (jump speeds), `DS:0x2AE` (ballistic constant),
   `DS:0x376C/0x3770/0x3796` (projectile sprite/animation tables) were read only as *readers*; the data
   itself should be dumped next.
6. `[0x322E]` vs `[0x322F]`: `0x322F` is the global freeze tested by `0x17665`/fireballs; `0x322E` is set
   on KO (`0x17810`). Their exact roles in the round loop (pause vs KO freeze) were not traced.
