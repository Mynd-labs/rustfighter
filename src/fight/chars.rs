//! Per-character gameplay handlers — port of the mode table at `DS:0x3BF9`
//! (see `docs/port/action-execution.md` §6).
//!
//! Each character has stubs for modes 0/2/4/6/8 (action start, script change,
//! per-frame tick, tail, KO). This build's handlers are mostly **sound hooks**;
//! KEN/RYU additionally spawn the fireball, while GUILE/DHALSIM/SAGAT specials
//! need their sprite tables extracted (tracked as TODO — their conditions are
//! recognised but no object is spawned yet).
//!
//! Character order (engine): KEN, GUILE, E-HONDA, RYU, ZANGIEF, CHUN LI,
//! DHALSIM, BLANKA, BALROG, VEGA, SAGAT, M.BISON.

/// A projectile spawn requested by a mode-4 handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnRequest {
    /// Spawn X (world).
    pub x: i16,
    /// Spawn Y (world, player space).
    pub y: i16,
    /// Horizontal speed.
    pub x_speed: i16,
    /// Palette passed to the projectile (`obj[0x1A] + 0x2020`).
    pub palette: u16,
    /// Spawning action id (becomes the projectile's reaction id).
    pub action: u8,
}

/// Mode-0 hooks (`0x17BC2`): sounds at action start.
pub fn start_hook(char: usize, action: u8, sounds: &mut Vec<u8>) {
    match char {
        0 | 3 => {
            // KEN / RYU
            if action == 0x34 {
                sounds.push(7);
            }
        }
        1 => {
            // GUILE
            if action == 0x35 {
                sounds.push(0x18);
            }
        }
        2 => {
            // E. HONDA
            if action == 0x35 {
                sounds.push(0x17);
            }
        }
        5 => {
            // CHUN LI
            if action == 0x35 {
                sounds.push(1);
            }
        }
        7 => {
            // BLANKA
            if action == 0x2C {
                sounds.push(2);
            }
            if action == 0x34 {
                sounds.push(5);
            }
        }
        10 => {
            // SAGAT
            if (0x34..=0x36).contains(&action) {
                sounds.push(0x19);
            }
        }
        _ => {}
    }
}

/// Mode-2 hooks (`0x10C75`): sounds on script change.
pub fn script_hook(char: usize, action: u8, sounds: &mut Vec<u8>) {
    if char == 6 && matches!(action, 0x34 | 0x35) {
        // DHALSIM
        sounds.push(0x1D);
    }
}

/// Mode-4 hooks (`0x10D57`): per-frame tick; may spawn a projectile.
///
/// `sub_action` is `obj[0x32]`, `flags38` is `obj[0x38]`, `lockout` is
/// `player[0x1F]`, `frame` is `obj[0x35]`.
pub fn tick_hook(
    char: usize,
    action: u8,
    sub_action: u8,
    flags38: u8,
    lockout: u8,
    frame: u8,
    facing_right: bool,
    obj_x: i16,
    player_y: i16,
    palette: u16,
    latches: &mut (bool, bool),
    sounds: &mut Vec<u8>,
) -> Option<SpawnRequest> {
    match char {
        0 | 3 => {
            // KEN / RYU fireball: AX == 0x1035 (action 0x35, slot 0x10).
            if action == 0x35
                && sub_action == 0x10
                && flags38 & 0x40 == 0
                && lockout == 0
            {
                let x = obj_x
                    + if facing_right { 0x1E } else { -0x34 };
                let y = player_y - 0x34;
                let x_speed = if facing_right { 0x1E } else { -0x1E };
                return Some(SpawnRequest {
                    x,
                    y,
                    x_speed,
                    palette: palette.wrapping_add(0x2020),
                    action: 0x35,
                });
            }
        }
        5 => {
            // CHUN LI: sound 0x17 at frame 0x5C of action 0x34 (latch 0x37A7).
            if action == 0x34 && frame == 0x5C && !latches.1 {
                latches.1 = true;
                sounds.push(0x17);
            }
        }
        9 => {
            // VEGA: slot 0x3C of action 0x34 (latch 0x37A6), sound 0x16.
            if action == 0x34 && sub_action == 0x3C && !latches.0 {
                latches.0 = true;
                sounds.push(0x16);
            }
        }
        _ => {}
    }
    // TODO: GUILE (0xC35) / DHALSIM (0x1434, 0x1035) / SAGAT (0x1435, 0xC34/6)
    // spawn objects whose sprite tables are not extracted yet.
    None
}

/// Mode-0 reset of the one-shot sound latches.
pub fn clear_latches(char: usize, action: u8, latches: &mut (bool, bool)) {
    match char {
        5 if action == 0x34 => latches.1 = false,
        9 if action == 0x34 => latches.0 = false,
        _ => {}
    }
}

/// Mode-8 hooks (`0x177F5`): KO/round-end sounds; `ch` is 0x0E (or 0x0F for
/// the second variant).
pub fn ko_hook(char: usize, ch: u8, sounds: &mut Vec<u8>) {
    match char {
        2 if ch == 0x0E => sounds.push(0x15), // E. HONDA
        5 => {
            // CHUN LI
            if ch == 0x0D {
                sounds.push(0x0D);
            }
            if ch == 0x0E {
                sounds.push(0x0E);
            }
        }
        7 if matches!(ch, 0x0E | 0x0F) => sounds.push(0x0F), // BLANKA
        _ => {}
    }
}

/// Fireball sprite handles (flicker table `DS:0x376C[0..2]`).
pub const FIREBALL_SPRITES: [&str; 2] = ["sprFIREBAL0", "sprFIREBAL1"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fireball_spawn_requires_slot_10_and_clear_flags() {
        let mut latches = (false, false);
        let mut sounds = Vec::new();
        let ok = tick_hook(0, 0x35, 0x10, 0, 0, 0, true, 100, 200, 0x3030, &mut latches, &mut sounds);
        assert!(ok.is_some());
        let s = ok.unwrap();
        assert_eq!(s.x, 100 + 0x1E);
        assert_eq!(s.y, 200 - 0x34);
        assert_eq!(s.x_speed, 0x1E);
        assert_eq!(s.palette, 0x3030 + 0x2020);
        // Wrong slot -> no spawn.
        let no = tick_hook(0, 0x35, 0x0C, 0, 0, 0, true, 100, 200, 0x3030, &mut latches, &mut sounds);
        assert!(no.is_none());
        // Already spawned this action.
        let no = tick_hook(0, 0x35, 0x10, 0x40, 0, 0, true, 100, 200, 0x3030, &mut latches, &mut sounds);
        assert!(no.is_none());
        // Facing left mirrors the spawn.
        let left = tick_hook(3, 0x35, 0x10, 0, 0, 0, false, 100, 200, 0x4040, &mut latches, &mut sounds);
        assert_eq!(left.unwrap().x, 100 - 0x34);
    }

    #[test]
    fn sounds_match_the_table() {
        let mut s = Vec::new();
        start_hook(0, 0x34, &mut s);
        assert_eq!(s, vec![7]);
        s.clear();
        start_hook(10, 0x35, &mut s);
        assert_eq!(s, vec![0x19]);
        s.clear();
        script_hook(6, 0x34, &mut s);
        assert_eq!(s, vec![0x1D]);
        s.clear();
        ko_hook(7, 0x0E, &mut s);
        assert_eq!(s, vec![0x0F]);
    }

    #[test]
    fn chunli_latch_plays_once_per_action() {
        let mut latches = (false, false);
        let mut sounds = Vec::new();
        tick_hook(5, 0x34, 0, 0, 0, 0x5C, true, 0, 0, 0, &mut latches, &mut sounds);
        assert_eq!(sounds, vec![0x17]);
        tick_hook(5, 0x34, 0, 0, 0, 0x5C, true, 0, 0, 0, &mut latches, &mut sounds);
        assert_eq!(sounds.len(), 1, "latch prevents a second sound");
        clear_latches(5, 0x34, &mut latches);
        tick_hook(5, 0x34, 0, 0, 0, 0x5C, true, 0, 0, 0, &mut latches, &mut sounds);
        assert_eq!(sounds.len(), 2);
    }
}
