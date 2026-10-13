//! Fight engine core — faithful port of the gameplay subsystem.
//!
//! Sources (see `docs/port/fight-engine.md`):
//!
//! * [`fighter`] — the `0x23`-byte fighter struct (`DS:0x3BA9`/`DS:0x3BCC`),
//!   constructor `0x1746E`, wall clamp `0x17620`.
//! * [`ani`] — the per-character `ani<CHR>` tables: 200 × 42-byte frame records
//!   with three sprite handles, paired offsets, four hitbox triples and size
//!   nibbles (reader `0x12712`).
//!
//! Porting order per the analysis: fighter + entities → action/hitboxes → hit
//! resolution → round/match → per-character handlers → HUD/stage.

pub mod ani;
pub mod fighter;

/// Hitbox size table `DS:0x1618`: 16 `(width, height)` pairs indexed by the
/// nibbles stored in each `ani` frame record (`+0x28`/`+0x29`).
pub const HITBOX_SIZES: [(u8, u8); 16] = [
    (0x10, 0x10),
    (0x20, 0x10),
    (0x30, 0x10),
    (0x40, 0x10),
    (0x10, 0x20),
    (0x10, 0x30),
    (0x10, 0x40),
    (0x20, 0x20),
    (0x30, 0x20),
    (0x40, 0x20),
    (0x20, 0x30),
    (0x20, 0x40),
    (0x30, 0x30),
    (0x40, 0x30),
    (0x30, 0x40),
    (0x40, 0x20),
];
