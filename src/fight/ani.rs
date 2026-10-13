//! `ani<CHR>` — per-character animation/frame tables (port of `0x12712` and
//! friends).
//!
//! Each record is exactly **42 bytes**; every character has **200** of them
//! (decompressed size 8,400). Layout (observed):

use crate::fight::HITBOX_SIZES;

/// Sprite handle meaning "no sprite" (`0xFFF8` = −8).
pub const SPRITE_NONE: u16 = 0xFFF8;
/// Sprite handle meaning "keep previous" (`0xF33A` in the analysis).
pub const SPRITE_NO_CHANGE: u16 = 0xF33A;
/// Hitbox coordinate meaning "box inactive" (`0xFFF8` = −8).
pub const HITBOX_INACTIVE: i16 = -8;

/// One 42-byte animation frame record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AniFrame {
    /// Three `(sprite handle, signed offset)` pairs (`+0x00..+0x0B`).
    pub sprites: [(u16, i16); 3],
    /// Unknown small integers (`+0x0C..+0x0F`).
    pub unknown: [u8; 4],
    /// Four hitbox triples `(x_p2, y, x_p1)` (`+0x10 + 6i`).
    pub boxes: [[i16; 3]; 4],
    /// Size nibbles for boxes 0..3 (`+0x28`, `+0x29`).
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
        let mut sprites = [(0u16, 0i16); 3];
        for (i, sprite) in sprites.iter_mut().enumerate() {
            *sprite = (word(i * 4), sword(i * 4 + 2));
        }
        let mut unknown = [0u8; 4];
        unknown.copy_from_slice(&record[0x0C..0x10]);
        let mut boxes = [[0i16; 3]; 4];
        for (i, bx) in boxes.iter_mut().enumerate() {
            for (j, v) in bx.iter_mut().enumerate() {
                *v = sword(0x10 + i * 6 + j * 2);
            }
        }
        let size_nibbles = [
            record[0x28] >> 4,
            record[0x28] & 0x0F,
            record[0x29] >> 4,
            record[0x29] & 0x0F,
        ];
        Some(AniFrame {
            sprites,
            unknown,
            boxes,
            size_nibbles,
        })
    }

    /// Whether hitbox `i` is active (all three coordinates present).
    pub fn box_active(&self, i: usize) -> bool {
        self.boxes[i]
            .iter()
            .all(|&v| v != HITBOX_INACTIVE)
    }
}

/// A character's full `ani` table (200 frames).
#[derive(Debug, Clone)]
pub struct AniTable {
    /// Character name the table belongs to (asset name, e.g. `aniKEN`).
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

    /// Frame by index (the engine indexes `ani[frame * 42]`).
    pub fn frame(&self, index: usize) -> Option<&AniFrame> {
        self.frames.get(index)
    }
}

/// A computed hitbox (`{valid, type, left, top, right, bottom}` as in
/// `DS:0x15B0`/`DS:0x15D8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitBox {
    /// Box type: 0 = inactive, otherwise the size-nibble index.
    pub kind: u8,
    /// Left edge in world coordinates.
    pub left: i16,
    /// Top edge.
    pub top: i16,
    /// Right edge (exclusive-ish; engine adds +4 to size).
    pub right: i16,
    /// Bottom edge.
    pub bottom: i16,
}

/// Build the (up to four) active hitboxes for a frame — port of `0x12712`.
///
/// * `object_x`/`object_y` — the draw object's world position.
/// * `p1_side` — true when this is player 1 (tests bit 2 of the object's
///   facing/state byte, selecting the `x_p1` coordinate instead of `x_p2`).
pub fn build_hitboxes(
    frame: &AniFrame,
    object_x: i16,
    object_y: i16,
    p1_side: bool,
) -> [Option<HitBox>; 4] {
    let mut out = [None; 4];
    for i in 0..4 {
        if !frame.box_active(i) {
            continue;
        }
        let triple = frame.boxes[i];
        let x_source = if p1_side { triple[2] } else { triple[0] };
        let y_source = triple[1];
        let left = object_x.wrapping_add(x_source).wrapping_sub(4);
        let top = object_y.wrapping_add(y_source).wrapping_sub(4);
        let nibble = frame.size_nibbles[i] as usize & 0x0F;
        let (w, h) = HITBOX_SIZES[nibble];
        out[i] = Some(HitBox {
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

    #[test]
    fn parses_synthetic_record() {
        let mut rec = [0u8; 42];
        rec[0..2].copy_from_slice(&0xF4B5u16.to_le_bytes());
        rec[2..4].copy_from_slice(&(-43i16).to_le_bytes());
        // Box 0 triple: x_p2 @0x10, y @0x12, x_p1 @0x14.
        rec[0x10..0x12].copy_from_slice(&(-36i16).to_le_bytes());
        rec[0x12..0x14].copy_from_slice(&(-45i16).to_le_bytes());
        rec[0x14..0x16].copy_from_slice(&20i16.to_le_bytes());
        // Box 1: first word is the inactive sentinel.
        rec[0x16..0x18].copy_from_slice(&HITBOX_INACTIVE.to_le_bytes());
        rec[0x28] = 0x0A;
        rec[0x29] = 0xAB;
        let frame = AniFrame::parse(&rec).unwrap();
        assert_eq!(frame.sprites[0], (0xF4B5, -43));
        assert_eq!(frame.boxes[0], [-36, -45, 20]);
        assert!(frame.box_active(0));
        assert!(!frame.box_active(1)); // contains -8
        assert_eq!(frame.size_nibbles, [0x0, 0xA, 0xA, 0xB]);
    }

    #[test]
    fn hitbox_geometry_matches_engine_formula() {
        let mut rec = [0u8; 42];
        rec[0x10..0x12].copy_from_slice(&10i16.to_le_bytes()); // x_p2
        rec[0x12..0x14].copy_from_slice(&20i16.to_le_bytes()); // y
        rec[0x14..0x16].copy_from_slice(&30i16.to_le_bytes()); // x_p1
        rec[0x28] = 0x00; // box 0 nibble = 0 -> size (0x10, 0x10)
        let frame = AniFrame::parse(&rec).unwrap();
        // Player 2 side: left = x + x_p2 - 4, top = y + 20 - 4.
        let boxes = build_hitboxes(&frame, 100, 200, false);
        let b = boxes[0].unwrap();
        assert_eq!(b.left, 106);
        assert_eq!(b.top, 216);
        assert_eq!(b.right, 106 + 0x10 + 4);
        assert_eq!(b.bottom, 216 + 0x10 + 4);
        // Player 1 side uses x_p1.
        let boxes = build_hitboxes(&frame, 100, 200, true);
        assert_eq!(boxes[0].unwrap().left, 126);
    }
}
