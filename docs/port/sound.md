# Sound subsystem — port specification

Reverse-engineering of the sound/music subsystem of **Street Fighter II (MS-DOS, U.S. Gold /
Creative Materials, 1995)**, based on the shipped `SF2.EXE` + `GENERAL.SF2` assets.

- Address convention (same as all repo findings): **linear Ghidra coordinates**, image base
  `0x10000`; `file_offset = linear - 0xFFA0`. The game's data segment base is `0x1AA40`
  (`DS:0xNNNN` = linear `0x1AA40 + 0xNNNN`). A second code segment (the built-in OPL2 driver)
  lives at linear `0x20860` (= far segment `0x2086` after relocation; the image contains the
  pre-relocation segment word `0x1086`).
- Evidence classes: **observed** (bytes/instructions in the shipped files), **derived**
  (reproduced over all assets), **inferred** (cross-checks / interpretation).

Companion documents: `docs/findings/general-sf2-formats.md` (assets, LZW, `tun*/voc*/mit*`),
`docs/findings/engine-overview.md` (engine map), `docs/findings/config-highscores.md`.

---

## 1. Summary

1. The game has **five sound back ends**, selected by a config byte, and one common game-facing
   API: a per-device *dispatch table* keyed by a device index stored at `DS:0x4F4E`:
   | device idx | back end | music | SFX |
   |---|---|---|---|
   | 0 | none | — | — |
   | 1 | **Roland LAPC-1 / MPU-401** | `mit*` SMF streamed as MIDI (`0x330/0x331`) | MIDI notes (SFX id → note) |
   | 2 | **AdLib (OPL2)** | `mit*` SMF parsed and re-synthesised on the OPL2 (built-in driver in the 2nd code segment) | OPL note / percussion |
   | 3 | **Sound Blaster (`SBDRIVER.DRV`)** | same OPL2 music engine as (2) | `voc*` samples via the external driver (DMA) |
   | 4 | **PC speaker** | `tunBTUNE*` note tables on PIT ch2 / port 0x61 | `tunBSFX*` beeps |
   | 5 | composite (Roland + SB init) | — | — |
   A few code paths test `[0x4F4E] == 8` for the speaker player (see §11 unknown U3).
2. **Music** is Standard MIDI format 0, 96 ppq (`mitSF2TUNE1..6`, `mitSF2AT1..6`, `mitRFX`),
   loaded by asset handle and played either as MIDI (MPU-401) or as OPL2 voices. The
   `mit*` files are genuinely read at run time — they are **not** vestigial.
3. **SFX** are Creative Voice 1.10 samples (`voc*`, 8-bit unsigned PCM, 4–10 kHz), played
   only on the Sound Blaster path through the external driver `SBDRIVER.DRV`, which is opened
   **by name**, loaded at run time and far-called with a small numeric command in `BX`.
   On the other paths SFX are synthesised (MPU note / OPL note / PIT beep).
4. The archived asset `drvSBDRIVER` (Creative CT-VOICE driver, index 3066) is **unreferenced**:
   handle `0xF405` appears nowhere in `SF2.EXE` — dead asset.
5. All timing hangs off one INT 8 hook (`0x12B14`): the PIT is reprogrammed to ~192 Hz; the
   game's tick (`0x12B74`) runs at ~48 Hz and drives the PC-speaker player; an FM/OPL poll
   runs at the full 192 Hz.

A Rust port does **not** need the DMA driver, MPU-401 or OPL hardware: the natural port is
"decode `voc*` → mix samples; decode `mit*` → MIDI synthesis; decode `tun*` → optional
square-wave emulation", with the same event→asset mapping the DOS code uses (§9, §10).

---

## 2. Game-facing entry points (all in `SF2.EXE`)

| Linear | Symbol (this doc) | Role |
|---|---|---|
| `0x1985D` | `dev_select(BL)` | set/init sound device; writes `[0x4F4E]=BX`, clears `[0x5044]/[0x5045]`, dispatches (`xor bh,bh` — the device is in **BL**; callers set it, e.g. `0x14F85`, `0x1595A`, `0x19837`) |
| `0x1979C` | `dev_resume()` | called from the main loop: if `[0x5042]!=0xFF && [0x5043]!=0xFF` → `FUN_1000_9A20()` + `dev_select()` (BL borrowed from the preceding call, see U6) |
| `0x197E4` | `option_M_S()` | option-screen handler for the `M` / `S` keys (see §5) |
| `0x19843` | `copy_prompt()` | copy `Press 'S'…` or `Press 'M'…` text into `DS:0x508A` |
| `0x1985D` | dispatch table 1 | per-device "init/start" |
| `0x19961` | `dev_stop(BX=[0x4F4E])` | per-device "stop music" |
| `0x199F5` | `dev_engine_on()` | per-device "enable engine" |
| `0x19A20` | `dev_engine_off()` | per-device "disable engine" |
| `0x19A66` | `sfx_play(AL=id)` | **play SFX id** (per-device) |
| `0x19B1A` | `sfx_play_pending()` | play `AL=[0x323B]` (per-device) |
| `0x19C04` | `dev_silence()` | per-device "all notes off" |
| `0x19CD6` | `music_start(AL=track)` | **start music track** (per-device), called each frame with the current id |
| `0x12B14` | `int8_isr` | INT 8 handler (192 Hz): EOI, FM poll, ~48 Hz game/sound tick |
| `0x1A9DF` | `fm_tick` | 192 Hz OPL/music poll (`lcall 0x2086:0x55F`, `FUN_1000_A439`) |
| `0x1A439` | `music_engine()` | per-tick SMF engine: `jmp 0x19E78` (MPU) or `jmp 0x1A06B`/FM parser |
| `0x19E78` | `mpu_player_tick()` | 2-stream SMF→MPU player |
| `0x1A021` | `smf_bind()` | reads SMF header: division at `+0xC`, MTrk length at `+0x12`, first event at `+0x16` |
| `0x1A229` | `smf_vlq()` | MIDI variable-length-quantity reader |

The per-device dispatchers are `call word ptr cs:[bx+table]` with `BX=[0x4F4E]`; 6-entry tables
(`bx` 0..5). Tables: `0x9875` (init), `0x996B` (stop), `0x99FF` (engine on), `0x9A2A` (engine
off), `0x9A8E` (SFX play), `0x9B42` (SFX pending), `0x9C13` (silence).

### 2.1 Device selection / config

```
0x14F85  mov  bl,[0x2F36]        ; BF: main init
0x14F89  call 0x1985D            ;   select sound device from CONFIG.SF2 byte +0x2E
...
0x1594F  mov  [0x2F36],bh        ; option screen: save highlighted device value
0x15954  call 0x19961            ;   stop old device
0x1595A  call 0x1985D            ;   select new device
```

`DS:0x2F36` = `CONFIG.SF2` buffer (`DS:0x2F08`) offset **+0x2E**, i.e. **the sound-device
selection** (ships as `06`). The option screen UI itself is icon-based (`sprICONCRD*` sprites,
table at `DS:0x3200`); the strings `NONE.`/`ROLAND LAPC-1.`/`ADLIB.`/`SPEAKER.` at
`DS:0x5046/0x5057/0x5068/0x5079` (pointer list at `DS:0x4F46`) have **no code references**
in this build — the text device list is dead data, only `SBDRIVER.DRV` (path `DS:0x50E2`) and
the two option prompts are used.

### 2.2 Music start (`FUN_1000_9CD6`, AL = track id 0..5)

```
0x19CD6  if [0x4F4E]==0 or not([0x5042]==0xFF or [0x5043]==0xFF): return
         if [0x4F4E]==8:                       ; PC speaker
             [0x4F26]=0xFFFF; 0x19BC6(SI=[0x4F32+2*AL])   ; load tunBTUNE*, pin asset
             [0x4F26]=[0x4F28]=0; [0x5041]=0
         else:
             [0x50F1]=AL;  [0x50F2]=AL+0x0F     ; two indices into the mit* handle table
             0x19D42() -> FUN_1000_9F8A()        ; load mit* (see §8)
             [0x51F1]=0x60
             if [0x50EF]==0: FUN_1000_9E49(); FUN_1000_9FB8()   ; MPU path: reset chans, start SMF
             else:           FUN_1000_9F86()                     ; FM path: OPL instruments + start
```

`FUN_1000_9CD6` is called every main-loop iteration (`0x14FC7`, AL=0 or 5), when a scene
changes music (`0x1584B`: `mov al,[0x323C]; call 0x19CD6`), from the option screen (`0x19840`)
and from the "VS" screen (`0x193E5`). `[0x323C]` = current music id (0..5, set in the option
screen at `0x1551D`); `[0x5043]` = "music off" state (0xFF = off).

### 2.3 SFX trigger

The game plays SFX with a single call: `mov al,<id>; call 0x19A66` (29 call sites, e.g.
`0x1106C` id 6, `0x122AF` id from table `DS:0x1638 + attack_id`, `0x1494F` id 0x0A,
`0x151E0` id 0x12). `FUN_1000_9A66` saves registers and dispatches on `[0x4F4E]`:

| dev | handler | action |
|---|---|---|
| 1 | `0x9A9A` | `mov bx,0x4FCE; xlatb` → MIDI note, `call 0x19DAF` (MPU note-on) |
| 2 | `0x9B0E` | `inc al; lcall 0x2086:0x749` (post to OPL driver queue) |
| 3, 5 | `0x9AA2` | **copy `voc*` to buffer, far-call SB driver cmd 6** |
| 4 | `0x9AED` | PIT beep from `tunBSFX*` (handle table `DS:0x4FEC`) |
| 0 | `0x9A8D` | ret |

### 2.4 Option-screen `Press M` / `Press S` handler (`FUN_1000_97E4`)

```
0x197E4  if [0x322F]!=0xFF (option mode) or [0x4F4E]==0 or [0x5042]==0xFF: return
         if [0x16F1]&2: return                    ; modifier
         if [0x5043]==0xFF:                       ; music currently off
             if !([0x16F3]&1): return             ; 'S' key pressed?
             call 0x19961  ; stop music
             call 0x199F5  ; engine on  (per device)
             al=0; call 0x19A66                  ; play SFX 0 (demo beep/sample)
         else:                                    ; music currently on
             if !([0x1706]&1): return             ; 'M' key pressed?
             call 0x19A20  ; engine off
             call 0x1985D  ; re-init current device
             al=0; call 0x19CD6                   ; start track 0
         then copy the prompt string ("Press 'S' for Sound Effects." DS:0x50A7 or
         "Press 'M' for Music." DS:0x50C4) into DS:0x508A (29 bytes, `rep movsb`).
```

`0x197B1`/`0x197D6` are the draw/erase helpers for that 29-byte prompt: `0x197B1` calls
`0x19843` then draws `DS:0x508A` (20 chars) with the text routine `0x115D7`
(`BP=0x4004, CX=0x14, DX=0xA8`); `0x197D6` erases it with `0x1161F` using the handle cached
at `DS:0x4F3E`.

---

## 3. SB driver path (device 3/5) — the only VOC playback path

### 3.1 Load (`0x98B9`, table-1 entry 3)

```
0x98B9  call 0x12FF9 / 0x12FF3          ; asset-system critical section
0x98BF  xor dx,dx ; mov ax,0x2400
0x98C4  call 0x130C8                   ; alloc 0x2400 bytes -> [0x4F52] (VOC staging buffer)
0x98D2  mov dx,0x50E2                  ; "SBDRIVER.DRV"
0x98D5  call 0x11470                   ; DOS open
0x98E9  mov bp,ax                      ; file size (2048)
0x98EB  call 0x130C8                   ; alloc size bytes -> driver image segment
0x98F2  mov word cs:[0x9CD2],0         ; far-call pointer (offset 0)
0x98F9  mov word cs:[0x9CD4],ax        ; ... segment
0x9907  call 0x1147F                   ; DOS read image to seg:0000
0x990E  call 0x11495                   ; close
0x9918  mov bx,3 ; lcall cs:[0x9CD2]   ; driver cmd 3 = init
0x9920  mov di,0x4F54 ; call 0x138A0   ; load+pin the 30 voc assets (table below)
0x9926  [0x50E1]=0 ; [0x50EF]=1 ; [0x50F0]=0 ; [0x5042]=0xFF
```

The driver is **not** patched and **not** passed a config: it uses its compiled defaults
(base 0x220, IRQ 7, DMA ch 1). Note `0x98B9` is the same handler as "device 3 init", so the
driver is (re)loaded when the device is (re)selected.

### 3.2 Play (`0x19AA2` = `0x9AA2`)

```
0x19AA2  push ax                              ; AL = SFX id
0x19AA3  if [0x50E1]==0xFF: bx=8; lcall [0x9CD2]; [0x50E1]=0   ; stop previous voice
0x19AB7  pop bx; xor bh,bh; shl bx,1
0x19ABC  mov si,[bx+0x4F54]                   ; voc asset handle
0x19AC0  mov cx,[bx+0x4F92]                   ; record size in bytes (< 0x2400 asserted)
0x19AD2  mov ax,[0x4F52]; call 0x13974        ; copy whole VOC file to buffer:0
0x19AD8  mov es,[0x4F52]; mov di,0x1A         ; ES:DI -> VOC block start (header is 0x1A)
0x19ADF  mov bx,6; lcall cs:[0x9CD2]          ; driver cmd 6 = play voice at ES:DI
0x19AE7  [0x50E1]=0xFF
```

`FUN_1000_13974`: `acquire(SI) → DS = asset segment; ES = AX (buffer); di=si=0; rep movsb`
(raw 1:1 copy — no conversion).

Stop/cleanup paths: cmd `8` (stop voice) is issued before each new sample; cmd `9` (DSP
reset / flush) and buffer unmapping at `0x19961`→`0x9981/0x998E`, and at device unload.

### 3.3 `SBDRIVER.DRV` ABI (observed from the 2,048-byte file)

```
 0000: EB 2A 90 0D 0A "Street Fighter - SB Driver (c) 1993." 0D 0A 1A
 002C: 9C            pushf          ; === FAR ENTRY (called as buffer:0000) ===
 002D: 50 53 51 52 57 56 55 1E 06   ; push ax,bx,cx,dx,di,si,bp,ds,es
 0036: 50 8C C8 8E D8 58            ; ax=cs; ds=ax  (all state is CS-relative)
 003D: D1 E3 FF 97 4E 00            ; call word ptr [bx+0x004E]   (BX = command*2)
 0043: 07 1F 5D 5E 5F 5A 59 5B 58 9D CB
 004E: 89 00 89 00 89 00 62 00 89 00 89 00 66 00 89 00 81 00 85 00 ...
```

Command table at `CS:0x4E` (only 10 entries; **all registers are preserved**):

| BX | target | meaning | EXE call sites |
|---|---|---|---|
| 3 | `0x062` → `call 0x653` | init: detect DSP, install IRQ, enable DMA voice | `0x1991B` |
| 6 | `0x066` → `0x6BD/0x752/0x726` | play 8-bit sample at `ES:DI` | `0x199E2`, `0x19AE2`, `0x19B92` |
| 8 | `0x081` → `call 0x780` | stop current DMA voice | `0x19984`, `0x19AAD`, `0x19B5D` |
| 9 | `0x085` → `call 0x56C` | flush: unhook IRQ, DSP drain, reset | `0x19991` |
| 0-2,4,5,7 | `0x089` | no-op (`ret`) | — |

Driver state (CS-relative words):

| addr | meaning | file value |
|---|---|---|
| `0xA2` | SB base I/O port | `0x220` |
| `0xA4/A6/A8/AA` | base+6 reset / +0xA read data / +0xC write cmd+data+status / +0xE read status | derived |
| `0xAC` | IRQ number → vector `(8+IRQ)*4` installed at `0x1FD` | `7` |
| `0xAE` | 8-bit DMA channel | `1` |
| `0x94` | DMA page-register low bytes `{0x87,0x83,0x81,0x82}` | ✓ |
| `0xB6/B8` | saved old INT vector (`0x1FD`) | – |
| `0xC6` | IRQ hook installed flag | 0/1 |
| `0xD0+2v` | voice v: buffer offset | – |
| `0xD4+2v` | voice v: buffer segment | – |
| `0xD8+2v` | voice v: remaining length (bytes) | – |
| `0xDC+2v` | voice v: cursor | – |
| `0xE4+2v` | voice v: DSP time constant | – |
| `0xE8+2v` | voice v: state (0 queued, 2 playing, 3 done) | – |
| `0xEC/0xEE` | global voice state: 0 idle, 1, 2 = playing, 3 = finished | – |

Play sequence (`0x066` → `0x6BD` → `0x752` → `0x726` → `0x4EB` → `0x515`):

```
0x066: mov  ah,es:[di+2]        ; VOC bytes ...
0x06A: mov  al,es:[di+3]        ;  -> AX = (mem[DI+2]<<8)|mem[DI+3]
0x06E: push ax / push es / push di / push 0
0x074: call 0x6BD               ; store ptr + AX as length into voice 0 struct
0x752: mov  bl,es:[si+4]        ; TC byte (e.g. 0x38) -> [0xE4]/[0xE6]
0x75C: add  si,6                ; data starts 6 bytes after the block header
0x773: sub  word [0xD8],6
0x726: [0xEE]=3; call 0x4EB     ; if status != 0xFFFF -> [0xEE]=2; call 0x515
0x515: out DSP 0x40; out TC     ; set sample rate
0x54B: ax=0x315; call 0x1FD     ; install IRQ handler
       push [0xDC][0xD8][0xD4][0xD0]; call 0x4C5   ; program DMA + start
0x28C: ax=[0xAE]|4; out 0x0A    ; DMA mask
       out 0x0C,0; out 0x0B,0x49; out dx,al ... ; mode 0x49 = single, read, ch1
```

**VOC handoff detail (important for a port).** `ES:DI = staging_buffer : 0x1A`, i.e. the driver
sees the raw `.VOC` block *header*:

| offset | bytes (vocPUNCH) | driver reads |
|---|---|---|
| `+0` | `01` (block type) | — (implicitly skipped by `si+=6`) |
| `+1..+3` | `C8 04 00` (u24 length, LE) | — (low byte never read) |
| `+2` | `04` | `AH` (cmd 6) |
| `+3` | `00` | `AL` (cmd 6) |
| `+4` | `38` (TC / rate) | DSP time constant |
| `+5` | `00` (codec 0) | unused |
| `+6` | data | DMA source |

So the driver's sample length is `((len>>8) & 0xFFFF) - 6` bytes, i.e. **the u24 block length
rounded down to a 256-byte boundary, minus 6** — the low byte of the length is ignored
(e.g. `vocPUNCH`: len `0x4C8` → 1018 of 1217 data bytes played; `vocKICK`: len `0x600` →
1530 of 1529). This is an observed driver quirk (likely an endian/offset bug in the 1993
driver, or an intentional 256-byte tail trim). A faithful Rust port may simply play the full
`u24_len - 7` bytes; if bit-exact DOS behaviour is ever needed, truncate as above.

---

## 4. PC speaker path (device 4; `[0x4F4E]==8` checks)

Music = `tunBTUNE*`, SFX = `tunBSFX*`, both `u16 (period, duration)` pairs, `0xFFFF`-terminated;
`period` is written **directly** into PIT channel 2 and is therefore a 1.193 MHz divisor
(`f = 1193182 / period`); `duration` is counted in ~48 Hz sound-tick units (§6); `period == 0`
= rest (its duration is the *next* word). Example: `tunBSFX9 = (2290,50) (0,2) END` → 521 Hz
for 50 ticks ≈ 1 s.

```
0x19CBF  mov al,0xB6; out 0x43   ; PIT ch2, mode 3, binary
0x19CC3  mov ax,bx; out 0x42,al; out 0x42,ah   ; divisor = period word
0x19CCB  in al,0x61; or al,3; out 0x61         ; speaker gate on
0x19C51  ; tick (from INT8, §6): decrement [0x4F2C]; on 0 read next word from ES:[SI]
0x19C45  ; stop: [0x4F26]=0xFFFF; out 0x61,[0x4F2E] (saved port state)
```

`ES = [0x4F42]`, `SI = [0x4F26]` (cursor), `[0x4F28]` = loop target (set 0 by the start code,
so the tune loops), `[0x4F2C]` = duration counter, `[0x4F2E]` = saved `0x61` value.

The speaker SFX table `DS:0x4FEC` (word per SFX id 0..29) maps nearly every SFX to
`tunBSFX2` (`0xF3FE`), with exceptions: `5→tunBSFX7`, `10→tunBSFX6`, `16,17→tunBSFX7`,
`19,20→tunBSFX8`. The same table continues contiguously at `DS:0x5028` with the 12 entries
`tunBSFX1..C` (all `tunBSFX*` handles are `0xF3FF..0xF3F4`). The music selector `DS:0x4F32`
= `[tunBTUNE1, tunBTUNE5, tunBTUNE2, tunBTUNE3, tunBTUNE4, tunBTUNE5]`.

---

## 5. MPU-401 path (device 1, "Roland LAPC-1")

Direct port I/O, no driver file:

```
0x1A2CE  mp_write: mov dx,0x331; in al,dx; test al,0x40; loopne; mov dx,0x330; out dx,al
0x1A2E2  mp_read : poll 0x331 bit7; in 0x330
0x1A2F4  mp_uart : loop until 0x331 bit6 clear; out 0x331,0x3F   ; enter UART mode
```

`FUN_1000_9E49` (MPU reset) sends, for channels 0xB0..0xB9 (10 MIDI channels):
`B0+ch 79 00` (reset all controllers) and `B0+ch 7B 00` (all notes off).

Per-tick player `0x19E78` maintains two independent streams (`[0x51E7]`/`[0x51EC]` enable,
`[0x51E8]`/`[0x51ED]` cursors, `[0x51EA]`/`[0x51EF]` delta counters). For each stream it
decrements the delta counter once per tick; at zero it reads the next SMF event (status high
bit + per-message byte count from table `DS:0x5121` indexed by `status>>4`: `… 02 02 02 02
01 01 02 …`), decodes the new delta with `0x1A229` (VLQ) and emits the message bytes through
`0x1A2CE`. `0xFF` meta is handled; `0x19DAF` holds the last note-on.

SFX on this device are MIDI notes: `0x19A9A` does `mov bx,0x4FCE; xlatb` (SFX id → note byte
from a table at `DS:0x4FCE`) and `0x19DAF` sends a note-on; `0x1A42`-area helpers send
program changes/volume.

---

## 6. AdLib / OPL2 path (device 2, and music for device 3/5)

The OPL driver is a **second code segment inside `SF2.EXE`**, linear `0x20860`–`0x22A40`
(load offset 0x10860). Far entry points used by the main segment (as `lcall 0x1086:off` before
relocation / linear after):

| far off | linear | role |
|---|---|---|
| `0x052F` | `0x20D8F` | init (reloads patch tables, resets OPL state) |
| `0x0555` | `0x20DB5` | release/stop |
| `0x055F` | `0x20DBF` | **192 Hz tick** (writes OPL reg `0xB0` on 0x388/0x389; per-channel key on/off) |
| `0x0749` | `0x20FA9` | post command `AX` into a 16-entry ring at `CS:0x24C` |

OPL register writes: `0x20DEF: mov dx,0x388; mov al,0xB0; out dx,al; delay; inc dx; out dx,al`
and helper `0x1A9A4` in the main segment (`mov dx,0x388; out dx,al; 6×in al,dx; inc dx; mov
al,ah; out dx,al; delay`).

Music: `FUN_1000_A439` (`0x1A439`) parses the loaded `mit*` SMF stream (`ES:SI =
[0x51C1]/[0x51C3]`), maintains per-channel state at `DS:0x50FF/0x511F/0x521F/0x5227/0x527A`
(voice count 9 or 5), allocates OPL voices for note-on and queues register writes to the CS2
driver. `0x1A42D` = "stop": `[0x51AA]=0; call 0x1A6A7; call 0x1A6C3` (the latter blasts
`0x1A9A4` OPL writes = all-notes-off). Note-number→OPL-FNUM data is copied from tables
`0x5BCF/0x5BDB/0x5BE7/0x5BF3` by `FUN_1000_A397` (`0x1A397`), keyed by `(music_number-0x0F)`.

For device 3/5 the game therefore uses **the same OPL music engine** for music and only uses
`SBDRIVER.DRV` for digitised SFX (this matches the "SB = FM music + DAC voice" design of the
era; there is no separate "SB music file").

---

## 7. Timing infrastructure

```
0x12A31  mov al,0x36; out 0x43        ; PIT ch0, mode 3
0x12A3E  ax=[0x1686] (min BIOS counter read in a loop)  -> patched into ISR chain
0x12A1C  dx:ax = 0x00123321; div 0x00C0 -> [0x168A] = 6212   ; 1193182/6212 = 192.08 Hz
0x12A2A  [0x168C] = 0x03D7 (983)
0x12A53  mov ax,0x2508; mov dx,0x2B14; int 0x21              ; install INT 8 -> 0x12B14
0x12B14  int8_isr: push regs; DS=0xAA4; out 0x20,0x20        ; EOI
         call 0x12B59 -> 0x12B60 -> 0x1A9DF                  ; 192 Hz FM/music poll
         ax=[0x168C]-0x100; if borrow: [0x168C]+=0x3D7; call 0x12B74   ; ~48 Hz tick
0x12B74  tick: call 0x136E5 / 0x17140 / 0x14D52 / 0x12BA9
         call 0x19C51                                        ; PC-speaker music step
         (the ISR itself chains the old BIOS handler through a self-patched ljmp
          at 0x12B55 that was written from the saved vector in 0x12A12-0x12A17)
0x1A9DF  fm_tick: if [0x5040]==0xFF: toggle & lcall 0x2086:0x55F
                  if [0x5044]==0 (re-entrancy): 0x5044++; sti; call 0x1A439; cli;
                  [0x4F4C] += elapsed ticks
0x1A439  music_engine: if [0x50EF]==0 -> jmp 0x19E78 (MPU player) or 0x1A06B
                       else -> FM/SMF parser (already described)
```

So: FM music is serviced at 192 Hz with a tick accumulator; the speaker player and the rest
of the "sound tick" run at ≈48.0 Hz (`192.08/4`), which is the unit of `tun*` durations.

---

## 8. Music assets (`mit*`) and their wiring

All 13 `mit*` records decompress to **SMF format 0, 1 track, division 0x0060 (96 ppq)**
(`MThd 00 00 00 06 00 00 00 01 00 60`, then `MTrk`, then at +0x16 the first event
`00 FF 03 08 "Untitled"`). Event census (all files similar): note-on `0x9n`, note-off `0x8n`,
control change `0xBn`, program change `0xCn`, pitch bend `0xEn`, meta `FF 03/51/54/58/2F`.
`mitRFX` is a tiny percussion/metronome track (3 CC + 3 note-offs, 52-byte track).

The player uses the header directly: `FUN_1000_A021` reads **division at +0xC**, **MTrk
length at +0x12**, and sets the cursor to **+0x16**; `FUN_1000_A229` decodes delta times as
standard VLQs. This has been verified byte-for-byte against `mitSF2TUNE1..6`, `mitSF2AT1..6`
and `mitRFX`.

Track selection table `DS:0x5AC2` (word array; two sub-tables 15 words apart):

```
idx 0  0xF40B mitSF2TUNE1   idx 1  0xF40A mitSF2TUNE2   ... idx 5  0xF406 mitSF2TUNE6
idx 6  0xF412 mitRFX        idx 7,8 = 0x0000
idx 15 0xF411 mitSF2AT2     idx 16 0xF410 mitSF2AT3     ... idx 20 0xF40C mitSF2AT1
```

`FUN_1000_9F8A` picks the handle by `[0x50F1]` (= AL, the track id) or `[0x50F2]` (= AL+0x0F)
depending on `[0x50EF]` (device class flag), then `0x19BC6(SI)` loads+pins it via the asset
system and exposes `ES=[0x4F42]`, `SI=0` to the player. `mitRFX` is additionally loaded by the
MPU setup at `0x19D47` (`mov si,0xF412; call 0x19BC6`). Interpretation: the **TUNE** files
are the OPL/AdLib renditions and the **AT** files the Roland/MPU renditions of the same
musical numbers (inferred from the device-dependent selection); `mitRFX` is shared.

---

## 9. Audio asset inventory used by the subsystem

**VOC (30 files, all raw, VOC 1.10, codec 0 = 8-bit unsigned PCM):**

| id | asset | bytes | TC | rate | id | asset | bytes | TC | rate |
|---|---|---|---|---|---|---|---|---|---|
| 0 | vocBALL | 2603 | 6 | 4000 | 15 | vocHOWL | 5127 | 131 | 8000 |
| 1 | vocBIRD | 7447 | 113 | 6993 | 16 | vocKICK | 1567 | 131 | 8000 |
| 2 | vocBITE | 1511 | 89 | 5988 | 17 | vocKICK1 | 1567 | 6 | 4000 |
| 3 | vocBLIP | 5107 | 113 | 6993 | 18 | vocPAUSE | 8299 | 6 | 4000 |
| 4 | vocBLOCK | 1637 | 113 | 6993 | 19 | vocPUNCH | 1255 | 56 | 5000 |
| 5 | vocBUZZ | 7771 | 113 | 6993 | 20 | vocPUNCH1 | 1255 | 6 | 4000 |
| 6 | vocCHUCK | 1287 | 6 | 4000 | 21 | vocQUAKE | 2235 | 6 | 4000 |
| 7 | vocCYCLONE | 3901 | 6 | 4000 | 22 | vocSCREAM | 2411 | 56 | 5000 |
| 8 | vocDEDF | 5463 | 113 | 6993 | 23 | vocSLAP | 1125 | 156 | 10000 |
| 9 | vocDRAGON | 6603 | 131 | 8000 | 24 | vocSONIC | 3235 | 6 | 4000 |
| 10 | vocFIGHT | 1181 | 6 | 4000 | 25 | vocTIGER | 1763 | 6 | 4000 |
| 11 | vocFIRE | 1291 | 6 | 4000 | 26 | vocUCUT | 2165 | 6 | 4000 |
| 12 | vocFLAME | 2905 | 113 | 6993 | 27 | vocWHOOSH | 1181 | 131 | 8000 |
| 13 | vocGETDAT | 3449 | 113 | 6993 | 28 | vocYAP | 1437 | 56 | 5000 |
| 14 | vocHAHA | 7703 | 131 | 8000 | 29 | vocYOGA | 1093 | 6 | 4000 |

Note `vocBUZZ` contains **8 consecutive type-1 blocks** (964 bytes each) — a stutter/buzz
effect; the DOS driver only ever plays the first block. All other files are single-block.

**Handles (DS-relative tables, used by the code):**

| table | contents |
|---|---|
| `DS:0x4F54` (30×u16 + `0x0000` terminator) | voc handles, id 0..29 = `0xF3F3` (vocBALL) descending to `0xF3D6` (vocYOGA) |
| `DS:0x4F92` (30×u16) | record sizes in bytes, matching the table above |
| `DS:0x4FEC` (30×u16, continues at `0x5028`) | speaker-SFX `tun*` handles per SFX id (`tunBSFX2` mostly; exceptions in §4), then `tunBSFX1..C` |
| `DS:0x4F32` (6×u16) | speaker music: TUNE1, TUNE5, TUNE2, TUNE3, TUNE4, TUNE5 |
| `DS:0x5028` (12×u16) | `tunBSFX1..C` (alternate SFX set) |
| `DS:0x5AC2` (word array) | music handles: TUNE1..6, RFX, then AT2..AT6,AT1 at +15 words |
| `DS:0x4FCE` (byte table) | SFX id → MIDI note (Roland device) |
| `DS:0x5121` (16 bytes) | SMF message data-byte count by `status>>4` (`8..E` = 02 02 02 02 01 01 02) |

Handle encoding in this build: `handle = -(record_index + 1) mod 0x10000` (asset system).

---

## 10. Rust port plan (prioritised)

Goal: reproduce the *experience*, not the 1993 hardware plumbing. The asset side
(`src/index.rs`, `src/lzw.rs`, `src/gamedata.rs`) already resolves records; add a `sound`
module with the following stages.

### Stage 1 — VOC sample playback (highest value, ~everything needed for SFX)
- Decode VOC 1.10: header `"Creative Voice File\x1A"`, header size @0x14, version @0x16,
  walk blocks (`01` = sound data: u24 length, TC byte, codec byte, payload; `00` = end;
  tolerate `02` continuation by appending).
- Convert TC → rate: `rate = 1_000_000 / (256 - TC)`; payload = unsigned 8-bit mono PCM.
- Resample to the output device rate (linear interpolation is fine) and play via
  `rodio`/`cpal` (add `rodio` dependency) or a small `cpal` mixer.
- Mixer: one SFX voice is enough (DOS is single-voice on SB: a new SFX **cuts** the previous
  sample — cf. driver cmd 8 before cmd 6). Keep a list of 30 decoded samples and expose
  `play_sfx(id: u8)` with the same id table as §9 (id 0..29 alphabetical).
- Play the full `u24_len - 7` data bytes (skip the DOS truncation quirk, §3.3); if bit-exact
  behaviour is wanted, add a compatibility flag.
- Route: `game event -> SFX id (0..29) -> play_sfx` (mirrors `0x19A66`).

### Stage 2 — Music via SMF (needed for any music)
- The `mit*` files are already standard format-0 SMF: hand them to a synthesizer.
  Simplest robust option: a small built-in MIDI synth (sine/square/triangle voices + drum
  noise) fed by an SMF parser (96 ppq, tempo meta 0x51). Avoid OPL2 emulation unless the
  goal is bit-accuracy.
- Reuse the game's track selection tables: `mitSF2TUNE1..6` (ids 0..5) and `mitSF2AT1..6`;
  play `mitRFX` on a separate percussion channel alongside (as `0x19D47` does).
- Menu/attract wiring: `id -> [0x5AC2]` mapping (§8); option screen `M` starts track 0.

### Stage 3 — PC-speaker tunes (optional, distinctive)
- Decode `tun*` as `(u16 period, u16 duration)`; `0xFFFF` = end, `period==0` = rest.
- Emulate the 1-bit speaker as a square wave at `1_193_182 / period` Hz with a ~48 Hz tick
  for durations, mixed at low volume; or make it a selectable "PC speaker" audio mode
  (`tunBTUNE1..5` music, `tunBSFX*` SFX). Music selection uses `DS:0x4F32` order.
- This also gives a working `SPEAKER.` configuration without any MIDI synth.

### Stage 4 — Menu/plumbing parity
- Implement the option-screen `M`/`S` toggles: `M` restarts music (track 0), `S` plays a demo
  SFX; store the device selection in the config byte (CONFIG+0x2E) exactly as `0x1594F`.
- Keep the device enum (`None/Roland/AdLib/SB/Speaker`) so the port can expose the same
  choices; map everything onto stages 1–3 internally (e.g. SB → VOC + SMF, Roland → SMF,
  Speaker → tun*).
- Volume/state bytes to mirror: `[0x5042]` (SFX enable), `[0x5043]` (music off=0xFF),
  `[0x323B]` (pending SFX), `[0x323C]` (music id 0..5).

### Explicit non-goals
- Reimplementing `SBDRIVER.DRV` (DMA/PIC/DSP programming) — replace with an OS audio sink.
- MPU-401 port I/O and OPL2 register-level synthesis — a MIDI synth supersedes them.
- `drvSBDRIVER` (CT-VOICE) — unreferenced by the build; ignore.

---

## 11. Confidence and unknowns

**High confidence (observed / derived):**

- Entry points, dispatch tables and call flow of §2 (tables verified byte-by-byte; callers
  enumerated by scanning all `e8/e9` rel16 targets).
- `SBDRIVER.DRV` = 2,048-byte DMA driver; entry at `0x2C` is a jump table dispatcher on
  `BX` (`0x2C: pushf; …; call word [bx+0x4E]`), with commands 3/6/8/9 used by the EXE; state
  map, port defaults (0x220/IRQ7/DMA1), DSP-command sequence and the `ES:DI = voc+0x1A`
  hand-off (all read from the driver file and the EXE call sites).
- VOC census: 30 files, VOC 1.10, codec 0, single-block except `vocBUZZ` (8 blocks);
  TC/rate table above (parsed from the actual records).
- `mit*` = SMF format 0, 1 track, 96 ppq, event census; player binds division at +0xC and
  events at +0x16; track handle tables at `DS:0x5AC2`/`0x5B46`.
- PIT/INT8 architecture: `mov ax,0x2508; mov dx,0x2B14; int 0x21`; divisor 6212 → 192.08 Hz;
  `[0x168C]=0x3D7` divider → ~48.0 Hz sound tick; speaker player call at `0x12B9C`.
- `drvSBDRIVER` (handle 0xF405) unreferenced → dead asset.

**Medium confidence (inferred):**

- Device-id ↔ option mapping (table entries 0..5 vs. the `==8` speaker tests) — see U3.
- TUNE = FM rendition / AT = Roland rendition (device-dependent selection observed; musical
  intent inferred).
- The speaker SFX table semantics (`0x4FEC` entries chosen by SFX id; several ids map to the
  same `tunBSFX*` beep).
- `DS:0x4FCE` SFX→note byte table for the Roland path (bytes verified, note semantics not).

**Unknowns:**

- **U1.** Why the SB driver computes the sample length from VOC bytes `+2/+3`
  (`(mem[+2]<<8)|mem[+3]`, i.e. `len & 0xFF00`) instead of the u24 length — looks like an
  off-by-one register/endian bug or an intentional 256-byte trim. Port recommendation: play
  the full length.
- **U2.** Whether the option screen ever displays the text device names
  (`NONE./ROLAND/ADLIB/SPEAKER.`): they are unreferenced by code in this build; the UI is
  icon-only (`sprICONCRD*`).
- **U3.** The exact device id used for the PC speaker: most dispatch tables key off
  `[0x4F4E]` 0..5 (4 = speaker handlers), but `FUN_1000_9CD6`/the tick test `[0x4F4E]==8`.
  The config byte (`CONFIG+0x2E`, ships as 6) is passed straight through, so the mapping
  menu-value → device id is not 1:1. A port can side-step this by keying off the user's
  device choice semantically.
- **U4.** Whether any `mit*` file is *exclusively* used by the attract mode (the AT set) or
  whether the game re-uses ids across scenes (scene → `[0x323C]` writers are mostly
  indirect).
- **U5.** Exact tempo handling of `FUN_1000_A439` (the engine consumes 96-ppq deltas against
  its 192 Hz tick with an accumulator at `[0x4F4C]`; tempo meta `FF 51` is present in the
  files but its consumption was not traced to the byte). A Rust port can use the SMF tempo
  directly.
- **U6.** `FUN_1000_979C` (`0x197AA`/`0x197AD`) calls `FUN_1000_9A20` and then `dev_select`
  **without loading BL**; the device index must survive in BL from `FUN_1000_9A20`'s
  `mov bx,[0x4F4E]` dispatch (handlers preserve registers). This works in practice but is an
  implicit contract, not an explicit argument pass — worth remembering when tracing bugs.
