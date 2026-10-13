//! `ait<CHAR>` — per-character AI / action-selection tables.
//!
//! Port of the readers at `0x105EF`, `0x106E8`, `0x10721`, `0x17BE1`
//! (see `docs/port/character-tables.md` §4).
//!
//! Layout: an 11-word header of absolute section offsets, then the sections.
//! `hdr[1]` (always 22) is the sparse **action → script id** table used by the
//! action dispatcher (`0x17BE1`), which the port implements in
//! [`AitTable::script_for_action`].

/// Number of header words.
pub const HEADER_WORDS: usize = 11;

/// A character's AI table.
#[derive(Debug, Clone)]
pub struct AitTable {
    /// Asset name (e.g. `aitKEN`).
    pub name: String,
    /// The 11 section offsets.
    pub header: [u16; HEADER_WORDS],
    /// Raw record bytes.
    data: Vec<u8>,
}

impl AitTable {
    /// Parse a decompressed `ait*` record.
    pub fn parse(name: &str, data: &[u8]) -> Result<Self, String> {
        if data.len() < HEADER_WORDS * 2 {
            return Err(format!("{}: shorter than the {HEADER_WORDS}-word header", name));
        }
        let mut header = [0u16; HEADER_WORDS];
        for (i, word) in header.iter_mut().enumerate() {
            *word = u16::from_le_bytes([data[i * 2], data[i * 2 + 1]]);
        }
        Ok(AitTable {
            name: name.to_string(),
            header,
            data: data.to_vec(),
        })
    }

    /// Byte slice of section `k`, when the offsets make sense.
    pub fn section(&self, k: usize) -> Option<&[u8]> {
        let start = *self.header.get(k)? as usize;
        let mut next = self.data.len();
        for (j, &off) in self.header.iter().enumerate() {
            if j == k {
                continue;
            }
            let off = off as usize;
            if off > start && off < next {
                next = off;
            }
        }
        if start >= self.data.len() || start >= next {
            return None;
        }
        Some(&self.data[start..next])
    }

    /// Per-action **damage/power** byte table — header word 5
    /// (`0x1857C` looks it up with `AX = 0x0A`; the helper dereferences the
    /// header word at that byte offset and indexes it by the reaction id).
    pub fn power_table(&self) -> Option<&[u8]> {
        self.section(5)
    }

    /// Per-reaction **score** byte table — header word 4 (`0x1836B` with `AX = 8`).
    pub fn score_table(&self) -> Option<&[u8]> {
        self.section(4)
    }

    /// Per-action **lockout** byte table — header word 6 (`0x17C9E` stores it
    /// into `player[0x20]`).
    pub fn lockout_table(&self) -> Option<&[u8]> {
        self.section(6)
    }

    /// Per-action AI **approach flag** table — header word 0 (`0x106E8`).
    pub fn approach_table(&self) -> Option<&[u8]> {
        self.section(0)
    }

    /// Sparse **action → script** map — header word 1 (`0x17BE1`).
    pub fn action_map(&self) -> Option<&[u8]> {
        self.section(1)
    }

    /// Port of `FUN_1000_17BE1` (`0x17BE1`): map an action id to its script.
    ///
    /// Scans the `hdr[1]` section downward from `action` while the `u16` at
    /// `base + index * 2` is zero; the first non-zero pair yields
    /// `(script_id, param)` from its two bytes. For `action >= 0x40` the
    /// parameter is forced to 0.
    pub fn script_for_action(&self, action: u8) -> Option<(u8, u8)> {
        let base = self.header[1] as usize;
        let mut index = action as usize;
        loop {
            let pos = base + index * 2;
            let pair = self.data.get(pos..pos + 2)?;
            if pair != [0, 0] {
                let script = pair[0];
                let param = if action >= 0x40 { 0 } else { pair[1] };
                return Some((script, param));
            }
            index = index.checked_sub(1)?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_action_scan_steps_down() {
        let mut data = vec![0u8; 22];
        data[2..4].copy_from_slice(&22u16.to_le_bytes()); // hdr[1] = action-map base
        data.extend_from_slice(&[
            0x00, 0x00, // action 0
            0x00, 0x00, // action 1
            0x00, 0x00, // action 2
            0x23, 0x00, // action 3 -> script 0x23
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // actions 4..7
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // actions 8..10
            0x24, 0x00, // action 11 -> script 0x24
        ]);
        let table = AitTable::parse("t", &data).unwrap();
        assert_eq!(table.header[1], 22);
        assert_eq!(table.script_for_action(0x0B), Some((0x24, 0)));
        // Entry at 3 applies up to (not including) entry 11.
        assert_eq!(table.script_for_action(0x0A), Some((0x23, 0)));
        assert_eq!(table.script_for_action(0x03), Some((0x23, 0)));
        // Below the first entry: all zeros -> none.
        assert_eq!(table.script_for_action(0x02), None);
    }

    #[test]
    fn high_actions_force_param_zero() {
        let mut data = vec![0u8; 22];
        data[2..4].copy_from_slice(&22u16.to_le_bytes()); // hdr[1] = action-map base
        data.extend_from_slice(&[0u8; 128]); // actions 0..63 zero
        data.extend_from_slice(&[0x2A, 0x80]); // action 64 (0x40) -> script 0x2A, param 0x80
        let table = AitTable::parse("t", &data).unwrap();
        assert_eq!(table.script_for_action(0x40), Some((0x2A, 0)));
    }
}
