//! Fighter rendering — port of the layer draw at `0x10A10`–`0x10A7B` and the
//! blitter (`0x13C7A` / nibble-RLE renderer `0x13DEC`).
//!
//! Each `ani` record draws **two layers** of the pose:
//!
//! ```text
//! T layer: Y = obj.y + T_y - h(T) + 0x10
//! B layer: Y = obj.y + T_y - B_y - h(B) + 0x10
//! X      = obj.x + (flipped ? x_flip : x)
//! ```
//!
//! where `h()` is the sprite record's height byte. The engine selects colours
//! through a **palette base** added to each pixel index (draw object `+0x1A`):
//! `0x30` for player 1 and `0x40` for player 2, matching the two 16-colour
//! character palettes loaded into one 256-colour table.

use crate::canvas::Canvas;
use crate::fight::ani::{AniFrame, SPRITE_BLANK};
use crate::palette::Palette;
use crate::sprite::Frame;

/// Palette index base for player 1 (`0x3030` draw-object field, low byte).
pub const PALETTE_BASE_P1: u8 = 0x30;
/// Palette index base for player 2 (`0x4040`).
pub const PALETTE_BASE_P2: u8 = 0x40;

/// Draw one sprite layer honouring transparency and the palette base.
///
/// Returns the number of pixels drawn.
pub fn draw_layer(
    canvas: &mut Canvas,
    sprite: &Frame,
    x: i32,
    y: i32,
    palette_base: u8,
    flipped: bool,
) -> usize {
    let w = sprite.width as i32;
    let mut drawn = 0;
    for fy in 0..sprite.height as i32 {
        for fx in 0..w {
            // The engine mirrors the sprite box when the draw flip is set
            // (`obj[0x1c] & 4`): the X offsets (`T_x_flip`/`B_x_flip`) and the
            // pixel mirror belong together - drawing unmirrored at the flip
            // offsets shifts the two body layers apart.
            let sx = if flipped { (w - 1 - fx) as usize } else { fx as usize };
            let px = sprite.pixels[fy as usize * sprite.width as usize + sx];
            if px != 0 {
                canvas.set(x + fx, y + fy, palette_base.wrapping_add(px));
                drawn += 1;
            }
        }
    }
    drawn
}

/// Which layers a frame drew.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DrawStats {
    /// T-layer pixels drawn.
    pub t_pixels: usize,
    /// B-layer pixels drawn.
    pub b_pixels: usize,
}

/// True when a handle actually names a sprite to draw.
pub fn has_sprite(handle: u16) -> bool {
    handle != 0 && handle != SPRITE_BLANK
}

/// Draw a whole `ani` frame at `(obj_x, obj_y)` using `lookup` to resolve
/// sprite handles, remapping pixel indices through `palette_base`.
///
/// The layer ordering (B first, then T on top) matches the engine's two
/// layer processes spawned at `0x109C1`/`0x109C8`.
pub fn draw_ani_frame(
    canvas: &mut Canvas,
    frame: &AniFrame,
    obj_x: i16,
    obj_y: i16,
    flipped: bool,
    palette_base: u8,
    lookup: &mut dyn FnMut(u16) -> Option<Frame>,
) -> DrawStats {
    let mut stats = DrawStats::default();

    // B layer first (drawn beneath the top layer).
    if has_sprite(frame.b_handle) {
        if let Some(sprite) = lookup(frame.b_handle) {
            let x = obj_x as i32 + frame.b_x_for(flipped) as i32;
            let y = obj_y as i32 + frame.t_y as i32 - frame.b_y as i32
                - sprite.height as i32
                + 0x10;
            stats.b_pixels = draw_layer(canvas, &sprite, x, y, palette_base, flipped);
        }
    }
    // T layer on top.
    if has_sprite(frame.t_handle) {
        if let Some(sprite) = lookup(frame.t_handle) {
            let x = obj_x as i32 + frame.t_x_for(flipped) as i32;
            // Layer geometry: the T layer top is `T_y - h(spr T) + 0x10` and
            // the B layer top is `T_y - B_y - h(spr B) + 0x10` (the pair the
            // reference footage matches: the feet land on the floor line and
            // the waist overlaps by the art's own padding). The earlier
            // half-separation came from the missing mirror, not from this.
            let y = obj_y as i32 + frame.t_y as i32 - sprite.height as i32 + 0x10;
            stats.t_pixels = draw_layer(canvas, &sprite, x, y, palette_base, flipped);
        }
    }
    stats
}

/// Install both characters' 16-colour palettes into a 256-entry canvas
/// palette at [`PALETTE_BASE_P1`]/[`PALETTE_BASE_P2`].
pub fn install_character_palettes(canvas: &mut Canvas, p1: &Palette, p2: &Palette) {
    if canvas.palette.colors.len() < 256 {
        canvas.palette.colors.resize(256, [0, 0, 0]);
    }
    for (i, c) in p1.colors.iter().take(16).enumerate() {
        canvas.palette.colors[PALETTE_BASE_P1 as usize + i] = *c;
    }
    for (i, c) in p2.colors.iter().take(16).enumerate() {
        canvas.palette.colors[PALETTE_BASE_P2 as usize + i] = *c;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sprite::Frame;

    fn solid(width: u8, height: u8, index: u8) -> Frame {
        Frame {
            width,
            height,
            pixels: vec![index; width as usize * height as usize],
        }
    }

    fn blank_frame() -> crate::fight::ani::AniFrame {
        crate::fight::ani::AniFrame::parse(&[0u8; 42]).unwrap()
    }

    #[test]
    fn layer_geometry_matches_engine_formulas() {
        let mut canvas = Canvas::new(320, 200);
        let mut frame = blank_frame();
        // T sprite 4x8 at T_x=10, T_y=20; B sprite 4x4 at B_x=-6, B_y=2.
        frame.t_handle = 0x1111;
        frame.t_x = 10;
        frame.t_x_flip = -10;
        frame.t_y = 20;
        frame.b_handle = 0x2222;
        frame.b_x = -6;
        frame.b_y = 2;
        let mut lookup = |h: u16| -> Option<Frame> {
            match h {
                0x1111 => Some(solid(4, 8, 1)),
                0x2222 => Some(solid(4, 4, 2)),
                _ => None,
            }
        };
        let stats = draw_ani_frame(&mut canvas, &frame, 100, 50, false, 0x30, &mut lookup);
        assert_eq!(stats.t_pixels, 32);
        assert_eq!(stats.b_pixels, 16);
        // B: x = 100 + (-6) = 94, y = 50 + 20 - 2 - 4 + 16 = 80.
        assert_eq!(canvas.get(94, 80), 0x32);
        // T: x = 110, y = 50 + 20 - 8 + 16 = 78.
        assert_eq!(canvas.get(110, 78), 0x31);
        // Flipped X uses x_flip.
        let mut canvas2 = Canvas::new(320, 200);
        draw_ani_frame(&mut canvas2, &frame, 100, 50, true, 0x40, &mut lookup);
        assert_eq!(canvas2.get(100 + (-10), 78), 0x41);
    }

    #[test]
    fn blank_handles_draw_nothing() {
        let mut canvas = Canvas::new(64, 64);
        let frame = blank_frame();
        let mut lookup = |_h: u16| -> Option<Frame> { panic!("must not be called") };
        let stats = draw_ani_frame(&mut canvas, &frame, 0, 0, false, 0x30, &mut lookup);
        assert_eq!(stats, DrawStats::default());
    }

    #[test]
    fn palette_bases_install_side_by_side() {
        let mut canvas = Canvas::new(8, 8);
        let p1 = Palette {
            colors: vec![[255, 0, 0]; 16],
        };
        let p2 = Palette {
            colors: vec![[0, 255, 0]; 16],
        };
        install_character_palettes(&mut canvas, &p1, &p2);
        assert_eq!(canvas.palette.get(0x31), [255, 0, 0]);
        assert_eq!(canvas.palette.get(0x40 + 7), [0, 255, 0]);
    }
}
