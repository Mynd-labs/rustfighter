//! Input motion recogniser — port of `0x14D72` (history ring) and `0x14E1B`
//! (recogniser), with the block data from `DS:0x78` tables.
//!
//! Verified against a byte-exact simulator of the original code (see
//! `docs/port/input-recogniser.md` §8): the unit tests below reproduce the
//! simulator's vectors.
//!
//! Deviation (documented): elements with `f == 0` would hit the original's
//! fatal assert; the shipped data only reaches that state in the four bosses'
//! stub blocks, which are unreachable in the original (they are not playable).
//! The port treats such blocks as non-matching.

/// `DS:0x2FFF` — direction identity map used by the history update.
pub const MIRROR_IDENTITY: [u8; 16] = [
    0x00, 0x01, 0x02, 0x00, 0x04, 0x05, 0x06, 0x00, 0x08, 0x09, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// `DS:0x2FEF` — left-facing mirror applied by the recogniser.
pub const MIRROR_303F: [u8; 16] = [
    0x00, 0x01, 0x02, 0x00, 0x08, 0x09, 0x0A, 0x00, 0x04, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// One motion block: flags plus `(v, f, btn)` elements.
#[derive(Debug)]
pub struct Block {
    /// Block flags (`0x8000` clear ⇒ quiet runs ≤ 6 frames are skipped).
    pub flags: u16,
    /// Match elements; `btn & 0x80` selects the mask comparison.
    pub elems: &'static [(u8, u8, u8)],
}


// Block data (DS 0xB8..0x23B) — see the analysis table.
const B_QCB_K: Block = Block { flags: 0x0000, elems: &[(0x1A, 0x02, 0x02), (0x1A, 0x02, 0x06), (0x1A, 0x02, 0x04), (0x1A, 0x02, 0xC0)] };
const B_GUILE_FLASH: Block = Block { flags: 0x0000, elems: &[(0x64, 0x01, 0x02), (0x1A, 0x02, 0x01), (0x1A, 0x02, 0xC0)] };
const B_HONDA_HEADBUTT: Block = Block { flags: 0x0000, elems: &[(0x64, 0x01, 0x04), (0x1A, 0x02, 0x08), (0x1A, 0x02, 0xA0)] };
const B_MASH_P: Block = Block { flags: 0x8000, elems: &[(0x06, 0x02, 0xA0), (0x06, 0x02, 0x00), (0x06, 0x02, 0xA0), (0x06, 0x02, 0x00), (0x06, 0x02, 0xA0)] };
const B_MASH_K: Block = Block { flags: 0x8000, elems: &[(0x06, 0x02, 0xC0), (0x06, 0x02, 0x00), (0x06, 0x02, 0xC0), (0x06, 0x02, 0x00), (0x06, 0x02, 0xC0)] };
const B_DHAL_34: Block = Block { flags: 0x0000, elems: &[(0x1A, 0x02, 0x06), (0x1A, 0x02, 0x02), (0x1A, 0x02, 0x0A), (0x1A, 0x02, 0x08), (0x1A, 0x02, 0xA0)] };
const B_HADOUKEN: Block = Block { flags: 0x0000, elems: &[(0x1A, 0x02, 0x02), (0x1A, 0x02, 0x0A), (0x1A, 0x02, 0x08), (0x1A, 0x02, 0xA0)] };
const B_GUILE_SONIC: Block = Block { flags: 0x0000, elems: &[(0x64, 0x01, 0x04), (0x1A, 0x02, 0x08), (0x1A, 0x02, 0xA0)] };
const B_ZANG_35: Block = Block { flags: 0x0000, elems: &[(0x1A, 0x02, 0x06), (0x1A, 0x02, 0x02), (0x1A, 0x02, 0x0A), (0x1A, 0x02, 0x08), (0x1A, 0x02, 0xA0)] };
const B_CHUN_35: Block = Block { flags: 0x0000, elems: &[(0x64, 0x01, 0x02), (0x1A, 0x02, 0x01), (0x1A, 0x02, 0xC0)] };
const B_DHAL_35: Block = Block { flags: 0x0000, elems: &[(0x1A, 0x02, 0x02), (0x1A, 0x02, 0x0A), (0x1A, 0x02, 0x08), (0x1A, 0x02, 0xA0)] };
const B_BLANKA_35: Block = Block { flags: 0x0000, elems: &[(0x64, 0x01, 0x04), (0x1A, 0x02, 0x08), (0x1A, 0x02, 0xA0)] };
const B_KEN_36: Block = Block { flags: 0x0000, elems: &[(0x1A, 0x02, 0x08), (0x1A, 0x02, 0x02), (0x1A, 0x02, 0x08), (0x1A, 0x02, 0xA0)] };
const B_SAGAT_34: Block = Block { flags: 0x0006, elems: &[(0x1A, 0x02, 0x04), (0x1A, 0x02, 0xC0)] };
const B_SAGAT_3: Block = Block { flags: 0x00A0, elems: &[(0x06, 0x02, 0x00), (0x06, 0x02, 0xA0)] };
const B_SHORYU: Block = Block { flags: 0x0000, elems: &[(0x1A, 0x02, 0x08), (0x1A, 0x02, 0x02), (0x1A, 0x02, 0x0A), (0x1A, 0x02, 0xA0)] };
const B_EMPTY: Block = Block { flags: 0x0000, elems: &[] };

/// The four motion blocks per character (engine order).
pub fn char_blocks(char: usize) -> [Option<&'static Block>; 4] {
    match char {
        0 | 3 => [
            Some(&B_QCB_K),
            Some(&B_HADOUKEN),
            Some(&B_KEN_36),
            Some(&B_SHORYU),
        ],
        1 => [
            Some(&B_GUILE_FLASH),
            Some(&B_GUILE_SONIC),
            Some(&B_EMPTY),
            Some(&B_EMPTY),
        ],
        2 => [
            Some(&B_HONDA_HEADBUTT),
            Some(&B_MASH_P),
            Some(&B_EMPTY),
            Some(&B_EMPTY),
        ],
        4 => [
            Some(&B_MASH_P),
            Some(&B_ZANG_35),
            Some(&B_EMPTY),
            Some(&B_EMPTY),
        ],
        5 => [
            Some(&B_MASH_K),
            Some(&B_CHUN_35),
            Some(&B_EMPTY),
            Some(&B_EMPTY),
        ],
        6 => [
            Some(&B_DHAL_34),
            Some(&B_DHAL_35),
            Some(&B_EMPTY),
            Some(&B_EMPTY),
        ],
        7 => [
            Some(&B_MASH_P),
            Some(&B_BLANKA_35),
            Some(&B_EMPTY),
            Some(&B_EMPTY),
        ],
        10 => [Some(&B_SAGAT_34), None, None, Some(&B_SAGAT_3)],
        // Bosses: no working special data in the shipped build.
        _ => [None, None, None, None],
    }
}

/// Per-player history ring (`0x2F4B` state + `0x2F55` buffer, 16 entries).
#[derive(Debug, Clone)]
pub struct Hist {
    /// `[counter, byte]` per entry.
    buf: [[u16; 2]; 16],
    idx: usize,
    wptr: usize,
    cnt: u16,
    last: u8,
}

impl Default for Hist {
    fn default() -> Self {
        Hist {
            buf: [[0; 2]; 16],
            idx: 0,
            wptr: 0,
            cnt: 0,
            last: 0,
        }
    }
}

impl Hist {
    /// Port of `0x14D72` — record one input byte, including the original's
    /// stale-byte quirk on change frames.
    pub fn update(&mut self, al: u8) {
        let bl = al & 0x0F;
        let al = (al & 0xF0) | MIRROR_IDENTITY[bl as usize];
        let ah = self.last;
        if al != ah {
            let cx = self.cnt;
            self.buf[self.wptr][0] = cx;
            self.buf[self.wptr][1] = ah as u16;
            let mut cl = self.idx + 1;
            if cl >= 16 {
                cl = 0;
                self.wptr = 0;
            } else {
                self.wptr += 1;
            }
            self.idx = cl;
            self.last = al;
            self.cnt = 0;
        }
        self.cnt += 1;
        self.buf[self.wptr][0] = self.cnt;
        self.buf[self.wptr][1] = ah as u16; // stale byte on change frames
    }

    fn clear(&mut self) {
        self.buf = [[0; 2]; 16];
    }

    /// Byte of the newest (open) run.
    pub fn newest_byte(&self) -> u8 {
        self.buf[self.wptr][1] as u8
    }

    /// Debug view of the ring (`(counter, byte)` per entry).
    pub fn entries(&self) -> [(u16, u8); 16] {
        let mut out = [(0u16, 0u8); 16];
        for (i, e) in self.buf.iter().enumerate() {
            out[i] = (e[0], e[1] as u8);
        }
        out
    }
}

/// Port of `0x14E1B` — returns the matching block index (the caller maps it to
/// an action `0x34 + CL`).
pub fn recognise(hist: &mut Hist, char: usize, facing_right: bool) -> Option<u8> {
    for (outer, block) in char_blocks(char).iter().enumerate() {
        let Some(block) = block else { continue };
        let f3028: u8 = if block.flags & 0x8000 == 0 { 0xFF } else { 0x00 };
        if block.elems.is_empty() {
            continue;
        }
        if block.elems.iter().any(|&(_, f, _)| f == 0) {
            // Would hit the original's fatal assert; unreachable there.
            continue;
        }
        for s in 0..0x40usize {
            if s % 4 != 0 {
                continue;
            }
            let mut e = 0usize;
            let mut pos = s;
            let mut guard = 0;
            loop {
                guard += 1;
                if guard > 200 {
                    break;
                }
                let entry = hist.buf[(pos / 4) % 16];
                let (hcnt, hbyte) = (entry[0], entry[1]);
                let (v, f, btn) = block.elems[e];
                if f3028 == 0xFF && hbyte == 0 {
                    if hcnt > 6 {
                        break;
                    }
                    pos = (pos + 4) & 0x3C;
                    if pos == s {
                        break;
                    }
                    if v == 0 && f == 0 {
                        return Some(outer as u8);
                    }
                    continue;
                }
                let mut al = hbyte as u8;
                if !facing_right {
                    al = (al & 0xF0) | MIRROR_303F[(al & 0x0F) as usize];
                }
                if btn & 0x80 == 0 {
                    if al != btn {
                        break;
                    }
                } else if (btn & 0x7F) & al == 0 {
                    break;
                }
                if f & 0x01 != 0 {
                    if hcnt <= v as u16 {
                        break;
                    }
                } else if f & 0x02 != 0 {
                    if hcnt > v as u16 {
                        break;
                    }
                } else {
                    break;
                }
                e += 1;
                pos = (pos + 4) & 0x3C;
                if pos == s {
                    break;
                }
                if e == block.elems.len() {
                    // Match: clear the ring like the original's REP STOSB.
                    hist.clear();
                    return Some(outer as u8);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(char: usize, facing_right: bool, script: &[(u8, u32)]) -> Option<u8> {
        let mut hist = Hist::default();
        for &(byte, dur) in script {
            for _ in 0..dur {
                hist.update(byte);
                if let Some(cl) = recognise(&mut hist, char, facing_right) {
                    return Some(cl);
                }
            }
        }
        None
    }

    #[test]
    fn ken_qcf_punch_is_hadouken() {
        // 0x02 (down) -> 0x0A (down-right) -> 0x08 (right) + punch (0xA0).
        let cl = run(0, true, &[(0x00, 10), (0x02, 3), (0x0A, 3), (0x08, 3), (0x28, 5)]);
        assert_eq!(cl, Some(1), "QCF+P -> block 1 -> action 0x35");
    }

    #[test]
    fn ken_qcb_kick_is_0x34() {
        let cl = run(0, true, &[(0x00, 10), (0x02, 3), (0x06, 3), (0x04, 3), (0x44, 5)]);
        assert_eq!(cl, Some(0));
    }

    #[test]
    fn ken_dragon_punch_is_0x37() {
        let cl = run(0, true, &[(0x00, 10), (0x08, 3), (0x02, 3), (0x0A, 3), (0x28, 5)]);
        assert_eq!(cl, Some(3));
    }

    #[test]
    fn guile_charge_moves() {
        let cl = run(1, true, &[(0x00, 10), (0x04, 120), (0x08, 3), (0x28, 5)]);
        assert_eq!(cl, Some(1), "back >100f, forward+P -> sonic");
        let cl = run(1, true, &[(0x00, 10), (0x02, 120), (0x01, 3), (0x48, 5)]);
        assert_eq!(cl, Some(0), "down >100f, up+K -> flash kick");
    }

    #[test]
    fn facing_left_mirrors_the_motion() {
        let cl = run(0, false, &[(0x00, 10), (0x02, 3), (0x06, 3), (0x04, 3), (0x24, 5)]);
        assert_eq!(cl, Some(1));
    }

    #[test]
    fn slow_motion_does_not_match() {
        let cl = run(0, true, &[(0x00, 10), (0x02, 30), (0x0A, 30), (0x08, 30), (0x28, 5)]);
        assert_eq!(cl, None);
    }
}
