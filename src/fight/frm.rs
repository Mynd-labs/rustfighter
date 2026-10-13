//! `frm<CHAR>` — per-character script ("frame") tables.
//!
//! Port of the readers `0x10AB1`, `0x10B7F`, `0x10C32`, `0x10C5D` (see
//! `docs/port/character-tables.md` §2):
//!
//! ```text
//! +0x000  64 x 4-byte script headers:
//!           +0  u16  frame_list_offset (first list at 0x100)
//!           +2  u8   flags   (loop/keep-last/mirror bits, see below)
//!           +3  u8   event_ani (ANI index that triggers the impact event)
//! +0x100  frame lists: 4-byte entries {u8 ani_index, i8 dx, i8 dy, u8 flags};
//!         a frame whose ani_index == 0xFF terminates the list.
//! ```

/// Script header flags (`+2`) with the effects traced at `0x10B20`–`0x10BC0`.
pub mod script_flag {
    /// At list end: restart list and set `obj[0x38] |= 0x10`.
    pub const LOOP_AND_FLAG: u8 = 0x04;
    /// At list end: restart list silently.
    pub const LOOP: u8 = 0x08;
    /// At list end: keep the last frame and set `obj[0x38] |= 0x10`.
    pub const HOLD_LAST: u8 = 0x10;
    /// Mirrors into `player[0x0A]` bit 5 (attack flag).
    pub const ATTACK: u8 = 0x20;
    /// Sets `obj[0x38] |= 0x10`.
    pub const FLAG_80: u8 = 0x80;
}

/// One 4-byte script header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptHeader {
    /// Byte offset of the 0xFF-terminated frame list.
    pub list_offset: u16,
    /// Header flags (see [`script_flag`]).
    pub flags: u8,
    /// ANI record index that triggers the impact/hit event.
    pub event_ani: u8,
}

/// One frame-list entry (4 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptFrame {
    /// ANI record index (0..=199).
    pub ani_index: u8,
    /// Signed X delta applied to the layer accumulator each frame.
    pub dx: i8,
    /// Signed Y delta applied to the layer accumulator each frame.
    pub dy: i8,
    /// Frame flags: `0x04` XORs mirror select, `0x08` mirrors into `obj[0x1C]` bit 3.
    pub flags: u8,
}

/// A character's `frm` table.
#[derive(Debug, Clone)]
pub struct FrmTable {
    /// Asset name (e.g. `frmKEN`).
    pub name: String,
    /// The 64 script headers.
    pub scripts: Vec<ScriptHeader>,
    /// Raw record bytes (frame lists are read from here).
    data: Vec<u8>,
}

impl FrmTable {
    /// Parse a decompressed `frm*` record.
    pub fn parse(name: &str, data: &[u8]) -> Result<Self, String> {
        if data.len() < 0x100 {
            return Err(format!("{}: shorter than the 64-script header", name));
        }
        let mut scripts = Vec::with_capacity(64);
        for i in 0..64 {
            let o = i * 4;
            scripts.push(ScriptHeader {
                list_offset: u16::from_le_bytes([data[o], data[o + 1]]),
                flags: data[o + 2],
                event_ani: data[o + 3],
            });
        }
        Ok(FrmTable {
            name: name.to_string(),
            scripts,
            data: data.to_vec(),
        })
    }

    /// Header for a script id (`0..=63`); the engine keeps `script_id * 4` in
    /// `player[0x20]` and dereferences it directly.
    pub fn script(&self, script_id: usize) -> Option<&ScriptHeader> {
        self.scripts.get(script_id)
    }

    /// Walk a script's frame list until the `0xFF` terminator.
    ///
    /// Returns `(frames, terminated)`. A list that ends exactly at the record
    /// end (the final list's terminator may be a lone `0xFF` byte with no room
    /// for the rest of the slot) counts as terminated; a list that runs into a
    /// truncated entry mid-record does not.
    pub fn frame_list(&self, script_id: usize) -> (Vec<ScriptFrame>, bool) {
        let mut out = Vec::new();
        let Some(script) = self.script(script_id) else {
            return (out, false);
        };
        let mut pos = script.list_offset as usize;
        loop {
            let Some(&first) = self.data.get(pos) else {
                return (out, true); // end of record
            };
            if first == 0xFF {
                return (out, true);
            }
            let Some(entry) = self.data.get(pos..pos + 4) else {
                return (out, false);
            };
            out.push(ScriptFrame {
                ani_index: entry[0],
                dx: entry[1] as i8,
                dy: entry[2] as i8,
                flags: entry[3],
            });
            pos += 4;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_synthetic_script() {
        let mut data = vec![0u8; 0x100];
        // Script 36 header -> list at 0x100, flags 0x25.
        let o = 36 * 4;
        data[o..o + 2].copy_from_slice(&0x100u16.to_le_bytes());
        data[o + 2] = 0x25;
        data[o + 3] = 0x07;
        data.extend_from_slice(&[
            0x6F, 0x00, 0x00, 0x00, // ani 111
            0x70, 0x01, 0x02, 0x04, // ani 112, dx 1, dy 2, flags 4
            0xFF, 0x00, 0x00, 0x00, // terminator
        ]);
        let table = FrmTable::parse("t", &data).unwrap();
        let script = table.script(36).unwrap();
        assert_eq!(script.list_offset, 0x100);
        assert_eq!(script.flags, 0x25);
        assert_eq!(script.event_ani, 0x07);
        let (frames, terminated) = table.frame_list(36);
        assert!(terminated);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].ani_index, 0x6F);
        assert_eq!(frames[1].dx, 1);
        assert_eq!(frames[1].dy, 2);
        assert_eq!(frames[1].flags, 4);
    }
}
