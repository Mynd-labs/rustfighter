//! `ani<CHR>` — per-character animation records (port of the readers at
//! `0x109CF`, `0x12712`, `0x10A10`; see `docs/port/character-tables.md` §3).
//!
//! 200 records per character, exactly 42 bytes each:
//!
//! ```text
//! +0x00 u16 T_handle   spr<CODE>T###   (0xF33A = sprBLANKSPR, "draw nothing")
//! +0x02 i16 T_x        X offset when obj[0x1C] bit 2 == 0
//! +0x04 i16 T_y        Y offset
//! +0x06 i16 T_x_flip   X offset when obj[0x1C] bit 2 == 1
//! +0x08 u16 B_handle   spr<CODE>B###   (0xF33A = blank)
//! +0x0A i16 B_x
//! +0x0C i16 B_y
//! +0x0E i16 B_x_flip
//! +0x10 4 x 6 B       hitbox candidate {i16 x_a, i16 y, i16 x_b}; -8 = empty
//! +0x28 u8            size nibbles for boxes 0,1 (hi, lo)
//! +0x29 u8            size nibbles for boxes 2,3 (hi, lo)
//! ```
//!
//! `T` and `B` are the two *layers* of one pose (both halves are drawn from the
//! same record; they are not facings — B/T artwork differs per pose).

use crate::fight::HITBOX_SIZES;

/// Sprite handle that means "draw nothing" (`sprBLANKSPR`).
pub const SPRITE_BLANK: u16 = 0xF33A;
/// Hitbox coordinate meaning "slot empty" (`0xFFF8` = -8).
pub const HITBOX_INACTIVE: i16 = -8;

/// One 42-byte animation record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AniFrame {
    /// Top-layer sprite handle (`+0x00`).
    pub t_handle: u16,
    /// Top-layer X for facing 0 (`+0x02`).
    pub t_x: i16,
    /// Top-layer Y (`+0x04`).
    pub t_y: i16,
    /// Top-layer X for facing 1 (`+0x06`).
    pub t_x_flip: i16,
    /// Bottom-layer sprite handle (`+0x08`).
    pub b_handle: u16,
    /// Bottom-layer X for facing 0 (`+0x0A`).
    pub b_x: i16,
    /// Bottom-layer Y (`+0x0C`).
    pub b_y: i16,
    /// Bottom-layer X for facing 1 (`+0x0E`).
    pub b_x_flip: i16,
    /// Four hitbox candidates `{x_a, y, x_b}` (`+0x10 + 6k`).
    pub boxes: [[i16; 3]; 4],
    /// Size nibbles for boxes 0..3 (`+0x28`/`+0x29`).
    pub size_nibbles: [u8; 4],
}

impl AniFrame {
    /// Parse one 42-byte record.
    pub fn parse(record: &[u8]) -> Option<Self> {
        if record.len() < 42 {
            return None;
        }
        let word = |o: usize| u16::from_le_bytes([record[o], record[o + 1]]);
        let sword = |o: usize| i16::from_le_bytes([record[o], record[o + 1]]);
        let mut boxes = [[0i16; 3]; 4];
        for (i, bx) in boxes.iter_mut().enumerate() {
            for (j, v) in bx.iter_mut().enumerate() {
                *v = sword(0x10 + i * 6 + j * 2);
            }
        }
        Some(AniFrame {
            t_handle: word(0x00),
            t_x: sword(0x02),
            t_y: sword(0x04),
            t_x_flip: sword(0x06),
            b_handle: word(0x08),
            b_x: sword(0x0A),
            b_y: sword(0x0C),
            b_x_flip: sword(0x0E),
            boxes,
            size_nibbles: [
                record[0x28] >> 4,
                record[0x28] & 0x0F,
                record[0x29] >> 4,
                record[0x29] & 0x0F,
            ],
        })
    }

    /// Top-layer X for the given facing (`0xA54`/`0x10A5F`).
    pub fn t_x_for(&self, flipped: bool) -> i16 {
        if flipped {
            self.t_x_flip
        } else {
            self.t_x
        }
    }

    /// Bottom-layer X for the given facing.
    pub fn b_x_for(&self, flipped: bool) -> i16 {
        if flipped {
            self.b_x_flip
        } else {
            self.b_x
        }
    }

    /// Whether hitbox `k` is active (no `-8` in its triple).
    pub fn box_active(&self, k: usize) -> bool {
        self.boxes[k].iter().all(|&v| v != HITBOX_INACTIVE)
    }
}

/// A character's full `ani` table (200 frames).
#[derive(Debug, Clone)]
pub struct AniTable {
    /// Asset name the table came from (e.g. `aniKEN`).
    pub name: String,
    /// The 200 frame records.
    pub frames: Vec<AniFrame>,
}

impl AniTable {
    /// Parse a decompressed `ani*` record (8,400 bytes expected).
    pub fn parse(name: &str, data: &[u8]) -> Result<Self, String> {
        if data.len() % 42 != 0 {
            return Err(format!("{}: size {} not a multiple of 42", name, data.len()));
        }
        let mut frames = Vec::with_capacity(data.len() / 42);
        for chunk in data.chunks_exact(42) {
            frames.push(AniFrame::parse(chunk).ok_or("short frame record")?);
        }
        Ok(AniTable {
            name: name.to_string(),
            frames,
        })
    }

    /// Frame by index (the engine computes `ani[index * 42]`, bounded to `< 200`).
    pub fn frame(&self, index: usize) -> Option<&AniFrame> {
        self.frames.get(index)
    }
}

/// A computed hitbox (`{valid, type, left, top, right, bottom}`, as in
/// `DS:0x15B0`/`DS:0x15D8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitBox {
    /// Box type 1..=16 (size-nibble index + 1); 0 means inactive.
    pub kind: u8,
    /// Left edge in world coordinates.
    pub left: i16,
    /// Top edge.
    pub top: i16,
    /// Right edge.
    pub right: i16,
    /// Bottom edge.
    pub bottom: i16,
}

/// Build the (up to four) active hitboxes for a frame — port of `0x12712`.
///
/// * `player_x`/`player_y` — the player's world position.
/// * `flipped` — `obj[0x1C] & 4` (set for player 1): selects the `x_b`
///   coordinate instead of `x_a`.
pub fn build_hitboxes(
    frame: &AniFrame,
    player_x: i16,
    player_y: i16,
    flipped: bool,
) -> [Option<HitBox>; 4] {
    let mut out = [None; 4];
    for k in 0..4 {
        if !frame.box_active(k) {
            continue;
        }
        let [x_a, y, x_b] = frame.boxes[k];
        let x_source = if flipped { x_b } else { x_a };
        let left = player_x.wrapping_add(x_source).wrapping_sub(4);
        let top = player_y.wrapping_add(y).wrapping_sub(4);
        let nibble = frame.size_nibbles[k] as usize & 0x0F;
        let (w, h) = HITBOX_SIZES[nibble];
        out[k] = Some(HitBox {
            kind: (nibble + 1) as u8,
            left,
            top,
            right: left.wrapping_add(w as i16).wrapping_add(4),
            bottom: top.wrapping_add(h as i16).wrapping_add(4),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes of `aniKEN` record 111 (from the worked example in the analysis).
    fn ken_frame_111() -> [u8; 42] {
        [
            0x52, 0xf4, 0xdc, 0xff, 0xb6, 0xff, 0xf4, 0xff, 0xe6, 0xf4, 0xe2, 0xff, 0xda, 0xff,
            0xe4, 0xff, 0xdd, 0xff, 0xb8, 0xff, 0xf3, 0xff, 0xf8, 0xff, 0x00, 0x00, 0xf8, 0xff,
            0xe9, 0xff, 0xd1, 0xff, 0xf7, 0xff, 0xeb, 0xff, 0xbd, 0xff, 0xf5, 0xff, 0x80, 0xbb,
        ]
    }

    #[test]
    fn parses_real_ken_record() {
        let frame = AniFrame::parse(&ken_frame_111()).unwrap();
        // handles from the worked example: sprKENNT112 / sprKENNB112
        assert_eq!(frame.t_handle, 0xF452);
        assert_eq!(frame.t_x, -36);
        assert_eq!(frame.t_y, -74);
        assert_eq!(frame.t_x_flip, -12);
        assert_eq!(frame.b_handle, 0xF4E6);
        assert_eq!(frame.b_x, -30);
        assert_eq!(frame.b_y, -38);
        assert_eq!(frame.b_x_flip, -28);
        assert_eq!(frame.boxes[0], [-35, -72, -13]);
        assert!(!frame.box_active(1)); // (-8, 0, -8)
        assert_eq!(frame.boxes[2], [-23, -47, -9]);
        assert_eq!(frame.size_nibbles, [8, 0, 0xB, 0xB]);
        assert!(frame.box_active(0));
        assert!(frame.box_active(2));
        assert!(frame.box_active(3));
    }

    #[test]
    fn hitbox_geometry_matches_engine_formula() {
        let mut rec = [0u8; 42];
        rec[0x10..0x12].copy_from_slice(&10i16.to_le_bytes()); // x_a
        rec[0x12..0x14].copy_from_slice(&20i16.to_le_bytes()); // y
        rec[0x14..0x16].copy_from_slice(&30i16.to_le_bytes()); // x_b
        rec[0x28] = 0x00; // box 0 nibble 0 -> size (0x10, 0x10)
        let frame = AniFrame::parse(&rec).unwrap();
        let boxes = build_hitboxes(&frame, 100, 200, false);
        let b = boxes[0].unwrap();
        assert_eq!(b.left, 106);
        assert_eq!(b.top, 216);
        assert_eq!(b.right, 106 + 0x10 + 4);
        assert_eq!(b.bottom, 216 + 0x10 + 4);
        let boxes = build_hitboxes(&frame, 100, 200, true);
        assert_eq!(boxes[0].unwrap().left, 126); // uses x_b
    }
}
