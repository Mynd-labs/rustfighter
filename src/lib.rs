//! `rustfighter` — native Rust reimplementation of the Street Fighter II (DOS) engine.
//!
//! This crate implements the validated on-disk formats of the original game and a
//! small native runtime around them:
//!
//! * [`index`]   — `INDEX.SF2` asset directory (3749 records).
//! * [`lzw`]     — the game's variable-width LZW codec ("method 3").
//! * [`sprite`]  — 4-bit-per-pixel nibble-RLE sprite frames.
//! * [`palette`] — 6-bit VGA palettes (16-colour and 256-colour).
//! * [`font`]    — 8-bpp glyph-array fonts.
//! * [`canvas`]  — 8-bit indexed framebuffer with blitter/text helpers.
//! * [`gamedata`]— ties it together: loads the original data files and decodes assets.

pub mod audio;
pub mod binfont;
pub mod canvas;
#[cfg(feature = "embedded")]
pub mod embedded;
pub mod fight;
pub mod font;
pub mod gamedata;
pub mod index;
pub mod input;
pub mod lzw;
pub mod palette;
pub mod sound;
pub mod sprite;
