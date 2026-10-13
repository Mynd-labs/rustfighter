# CONFIG.SF2 and HIGHSCOR.SF2 — field layouts

Analysis target: `/tmp/opencode/rea/game/street-fighter-ii/` (read-only).
Address mapping: `file_offset = linear - 0xFFA0`; primary DS base = linear `0x1AA40`
(so `DS:0x2F08` = linear `0x1D948`). All code addresses are linear Ghidra coordinates.

## 1. Summary

| Item | Value |
|---|---|
| `CONFIG.SF2` | 47 bytes; buffer at `DS:0x2F08`; load `FUN_1000_5F09` (`0x15F09`), save `FUN_1000_5F50` (`0x15F50`) |
| `HIGHSCOR.SF2` | 94 bytes; buffer at `DS:0x41FE`; load `FUN_1000_831B` (`0x1831B`), save `FUN_1000_834B` (`0x1834B`), update `FUN_1000_8382` (`0x18382`) |

Both files are opened **directly by name** by the EXE (they are not in `INDEX.SF2`).

## 2. CONFIG.SF2

### 2.1 Storage protocol

- Load (`FUN_1000_5F09`): open `CONFIG.SF2` read-only (`AX=3D00h`), require size
  **exactly `0x2F` (47)**, read 47 bytes into `DS:0x2F08`, close. It then executes
  `[0x3174] = word[config[0x2E] + 0x3200]` — the last config byte indexes a table
  at `DS:0x3200` (which begins with **negative asset handles** for
  `sprICONCRD1..6`, records 3322–3327 in `GENERAL.SF2`) and caches a handle at
  `DS:0x3174` (UI icon selection; exact selection rule inferred, see §2.3).
- Save (`FUN_1000_5F50`): create/truncate (`AX=3C00h`) and write 47 bytes from
  `DS:0x2F08`, close. The on-disk file is therefore a direct image of the
  in-memory struct.

### 2.2 Field map (as accessed by code)

| Offset | Width | Disk value | Access sites | Notes |
|---|---|---|---|---|
| `0x00` | word | `6` | `0x14B87` (write), load | P1 control parameter (see §2.3) |
| `0x02` | word | `8` | `0x14B8F` (write) | P1 control parameter |
| `0x04` | word | `6` | — (init only) | unidentified scalar |
| `0x06` | word | `6` | `FUN_1000_4B65` | P2 control parameter |
| `0x08` | byte | `0x30` (48) | `0x14D15` (read) | unidentified scalar, `& 0x0F` used with `0x13` |
| `0x0A` | byte | `0x0A` | `0x14B65` (OR bits) | option flag bits (bit 2 and 3 used) |
| `0x0B` | byte | `0x30` (48) | `0x14D5F` | unidentified scalar |
| `0x0C` | byte | `0x30` (48) | — | unidentified scalar |
| `0x0D` | byte | `0` | — | unidentified |
| `0x0E` | word | `0x0800` | `0x14D68` (`0x14`), `FUN_1000_4B65` (word `@0x0F`) | fields overlap: code touches word `@0x0F` = `0x0008` and byte/word at `0x12`–`0x14` |
| `0x10` | word | `0` | `0x14D15` reads byte | — |
| `0x12` | word | `0x0C00` | `0x1573A` (byte `@0x12` = `0x0C`) | option flag bits (`|4` by `FUN_1000_4B65`) |
| `0x14` | word | `0` | `0x14D68` | — |
| `0x13` | byte | `0x0C` | `0x15798` (read) + `FUN_1000_4B65` (`|4`) | option flag bits |
| `0x16`–`0x2D` | 12 × u16 | see §2.4 | config screen | key bindings, two players × 6 actions |
| `0x2E` | byte | `6` | `FUN_1000_5F09` | index into `DS:0x3200` table (`sprICONCRD…` handles) |

Because the struct is written back verbatim, all scalar values are what the last
game session left; defaults for a fresh install are as shown.

### 2.3 Control-type recomputation (`FUN_1000_4B65`, `0x14B65`)

`FUN_1000_4B65` derives the per-player control parameters when the input mode
changes. Joystick presence is read from `DS:0x16D1` bits 0/1; a computed mode
byte is kept at `DS:0x302B` (`FUN_1000_4BE6` later tests its bits 0/1 to answer
"is this player on a joystick" for player 2/4 selectors).

Observed mapping (mode = `[0x302B] & 3`):

| mode | words written (P1 `@0x00/0x02`, P2 `@0x06/0x0F`) | flag bytes OR'd |
|---|---|---|
| 0 (keyboard) | 6 / 8 | — |
| 1 (P1 joystick) | 2 / 8 | `@0x12 \|= 4`, `@0x13 \|= 0` |
| 2 (P2 joystick) | 4 / 8 | `@0x12 \|= 4`, `@0x13 \|= 4` |
| 3 (both joysticks) | 2 / 4 | both `\|= 4` |

Interpretation (inferred): per-player axis threshold/centre parameters for the
analog DOS gameport, with companion flag bits. Field names remain unresolved.

### 2.4 Key binding table (`0x16`–`0x2D`, 12 × u16)

Each entry is `u16 = [scancode (low byte)][action bit (high byte)]`:

| Entry | Offset | Disk u16 | Scancode | Action bit |
|---|---|---|---|---|
| P1 up | `0x16` | `0x0148` | `0x48` (Up) | `0x01` |
| P1 down | `0x18` | `0x0250` | `0x50` (Down) | `0x02` |
| P1 left | `0x1A` | `0x044B` | `0x4B` (Left) | `0x04` |
| P1 right | `0x1C` | `0x084D` | `0x4D` (Right) | `0x08` |
| P1 punch | `0x1E` | `0x201C` | `0x1C` (Enter) | `0x20` |
| P1 kick | `0x20` | `0x4039` | `0x39` (Space) | `0x40` |
| P2 up | `0x22` | `0x0110` | `0x10` (Q) | `0x01` |
| P2 down | `0x24` | `0x021E` | `0x1E` (A) | `0x02` |
| P2 left | `0x26` | `0x042C` | `0x2C` (Z) | `0x04` |
| P2 right | `0x28` | `0x082D` | `0x2D` (X) | `0x08` |
| P2 punch | `0x2A` | `0x2002` | `0x02` (1) | `0x20` |
| P2 kick | `0x2C` | `0x4003` | `0x03` (2) | `0x40` |

Action bits are consistent with the state fields written by the recompute routine
and with the `DEFINE CONTROLS` / `Press ESC to abort define` strings. The game is
a **two-button** port (punch/kick) in this DOS version.

## 3. HIGHSCOR.SF2

### 3.1 Layout — 5 entries × 18 bytes + trailing u32

| Offset | Size | Field | Disk content |
|---|---|---|---|
| `0x00` | u32 LE | score | `74,904` (`0x124F8`) |
| `0x04` | 4 B ASCIIZ | rank label | `"1ST"` |
| `0x08` | 10 B string | name | `"Jazzie..."` (9 chars max; `.` padding; NUL at +9) |
| `0x12` | 18 B | entry 2 | `74,904` / `"2ND"` / `"Jody...."` |
| `0x24` | 18 B | entry 3 | `50,000` / `"3RD"` / `"Dave....."` |
| `0x36` | 18 B | entry 4 | `30,000` / `"4TH"` / `"Nick....."` |
| `0x48` | 18 B | entry 5 | `20,000` / `"5TH"` / `"Shelagh.."` |
| `0x5A` | u32 LE | trailing constant | `10,000` (`0x2710`) — inferred floor/qualifying score, 6th place label never stored |

Scores are plain binary u32 (not BCD). Default names match the credits'
"Thanks to" list (e.g. `Jazzie` / `Jody` / `Dave` / `Nick` / `Shelagh`).

### 3.2 Runtime behaviour (code evidence)

- Load (`0x1831B`): open, require size **exactly `0x5E` (94)**, read 94 bytes to
  `DS:0x41FE`, close. A wrong size leaves defaults.
- Save (`0x1834B`): create/truncate, write 94 bytes from `DS:0x41FE`, close.
- Update (`FUN_1000_8382`, `0x18382`): caller passes a candidate entry pointer in
  `DI` and a score delta in `AX:DX`; the routine adds the delta to a running
  u32 at `DI+6`, compares against the top score at `DS:0x41FE/0x4200`, and on a
  new record copies the new score into the top slot and sets
  `DS:0x42DA = 0xFF` ("new record" flag).
- `FUN_1000_827B` (`0x1827B`): if `DS:0x42DA == 0xFF`, performs the record
  celebration/name-entry path and clears the flag to 0.

The top score lives at the very start of the buffer (`DS:0x41FE`), i.e. entry 1's
score field is the live "best score" slot.

## 4. Confidence and unknowns

**High confidence**

- File sizes and access buffers (`0x2F08`/`0x41FE`), load/save functions, exact
  size checks (`0x2F`, `0x5E`), and the 18-byte high-score entry layout.
- Key binding table encoding (scancode + action bit) and all 12 default bindings.
- Input-mode plumbing: `DS:0x16D1` joystick presence → `DS:0x302B` mode byte →
  per-player control parameters.

**Partially resolved**

- CONFIG scalar fields `@0x04/0x08/0x0B/0x0C/0x0D/0x0E/0x10/0x12/0x14`: values are
  known from disk and write sites, but their game-meaning labels (difficulty,
  timing, etc.) were not traced through the option-screen code.
- The `0x2E` icon byte → `DS:0x3200` table: table begins with `sprICONCRD1..6`
  handles and is cached to `DS:0x3174`; the exact selection/scaling is inferred.
- Control parameter words (6/8, 2/4, …): plausible gameport centre/threshold
  values; semantics inferred (joystick handling code not traced end-to-end).
- HIGHSCOR trailing u32 `10,000`: floor/qualifier inferred; no code path was
  traced that reads offset `0x5A` explicitly.
