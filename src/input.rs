//! Input layer — faithful port of the game's binding scheme.
//!
//! The DOS game reads **keyboard scancodes** and maps them to six action bits
//! per player. The mapping is stored in `CONFIG.SF2` as twelve `u16` entries
//! (six per player) at offset `0x16`, each `[scancode, action-bit]`
//! (see `docs/findings/config-highscores.md`):
//!
//! | bit  | action |
//! |------|--------|
//! | 0x01 | up     |
//! | 0x02 | down   |
//! | 0x04 | left   |
//! | 0x08 | right  |
//! | 0x20 | punch  |
//! | 0x40 | kick   |
//!
//! Shipped defaults: P1 = arrow keys + Enter/Space; P2 = Q/A/Z/X + 1/2.

use std::path::Path;

/// The six per-player actions of this two-button port.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    /// Up (jump).
    Up,
    /// Down (crouch).
    Down,
    /// Left.
    Left,
    /// Right.
    Right,
    /// Punch.
    Punch,
    /// Kick.
    Kick,
}

/// Action bit for each button (as in `CONFIG.SF2`).
pub const ACTION_BITS: [(Button, u8); 6] = [
    (Button::Up, 0x01),
    (Button::Down, 0x02),
    (Button::Left, 0x04),
    (Button::Right, 0x08),
    (Button::Punch, 0x20),
    (Button::Kick, 0x40),
];

/// One player's six `[scancode, action-bit]` bindings, in `CONFIG.SF2` order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerBindings {
    /// Six `(scancode, action bit)` pairs.
    pub map: [(u8, u8); 6],
}

impl PlayerBindings {
    /// The shipped default bindings.
    pub fn defaults(player: usize) -> Self {
        let map = if player == 0 {
            [
                (0x48, 0x01), // Up arrow
                (0x50, 0x02), // Down arrow
                (0x4B, 0x04), // Left arrow
                (0x4D, 0x08), // Right arrow
                (0x1C, 0x20), // Enter = punch
                (0x39, 0x40), // Space = kick
            ]
        } else {
            [
                (0x10, 0x01), // Q
                (0x1E, 0x02), // A
                (0x2C, 0x04), // Z
                (0x2D, 0x08), // X
                (0x02, 0x20), // 1 = punch
                (0x03, 0x40), // 2 = kick
            ]
        };
        PlayerBindings { map }
    }
}

/// Both players' bindings, loadable from `CONFIG.SF2`.
#[derive(Clone, Debug)]
pub struct InputConfig {
    /// Player 1 and player 2 bindings.
    pub players: [PlayerBindings; 2],
}

impl Default for InputConfig {
    fn default() -> Self {
        InputConfig {
            players: [PlayerBindings::defaults(0), PlayerBindings::defaults(1)],
        }
    }
}

impl InputConfig {
    /// Parse the twelve binding words out of a 47-byte `CONFIG.SF2` image.
    ///
    /// Returns `None` when the file does not have the expected size.
    pub fn from_config_bytes(data: &[u8]) -> Option<Self> {
        if data.len() != 0x2F {
            return None;
        }
        let mut players = [
            PlayerBindings { map: [(0, 0); 6] },
            PlayerBindings { map: [(0, 0); 6] },
        ];
        for p in 0..2 {
            for i in 0..6 {
                let off = 0x16 + (p * 6 + i) * 2;
                let word = u16::from_le_bytes([data[off], data[off + 1]]);
                players[p].map[i] = ((word & 0xFF) as u8, (word >> 8) as u8);
            }
        }
        Some(InputConfig { players })
    }

    /// Load `CONFIG.SF2` from a game folder (case-insensitive), falling back to
    /// the shipped defaults when the file is missing or malformed.
    pub fn from_game_dir(dir: &Path) -> Self {
        let direct = dir.join("CONFIG.SF2");
        let path = if direct.is_file() {
            Some(direct)
        } else {
            std::fs::read_dir(dir).ok().and_then(|entries| {
                entries
                    .flatten()
                    .find(|e| {
                        e.file_name()
                            .to_string_lossy()
                            .eq_ignore_ascii_case("CONFIG.SF2")
                    })
                    .map(|e| e.path())
            })
        };
        path.and_then(|p| std::fs::read(p).ok())
            .and_then(|bytes| Self::from_config_bytes(&bytes))
            .unwrap_or_default()
    }
}

/// Edge-detected per-player button state (held / pressed / released).
#[derive(Clone, Copy, Default, Debug)]
pub struct InputState {
    /// Action bits currently held.
    pub held: [u8; 2],
    /// Action bits that went down this tick.
    pub pressed: [u8; 2],
    /// Action bits that went up this tick.
    pub released: [u8; 2],
}

impl InputState {
    /// Update from a raw per-scancode "is down" predicate.
    ///
    /// This is the faithful core: the DOS game sees scancode set 1 edges; this
    /// produces the same action-bit edges from any host key source.
    pub fn update(&mut self, config: &InputConfig, mut scancode_down: impl FnMut(u8) -> bool) {
        for (p, bindings) in config.players.iter().enumerate() {
            let mut held = 0u8;
            for &(scancode, bit) in &bindings.map {
                if scancode_down(scancode) {
                    held |= bit;
                }
            }
            let pressed = held & !self.held[p];
            let released = !held & self.held[p];
            self.held[p] = held;
            self.pressed[p] = pressed;
            self.released[p] = released;
        }
    }

    /// Whether a button is held for a player.
    pub fn is_held(&self, player: usize, button: Button) -> bool {
        let bit = bit_of(button);
        self.held[player] & bit != 0
    }

    /// Whether a button went down this tick for a player.
    pub fn just_pressed(&self, player: usize, button: Button) -> bool {
        let bit = bit_of(button);
        self.pressed[player] & bit != 0
    }

    /// Whether a button went up this tick for a player.
    pub fn just_released(&self, player: usize, button: Button) -> bool {
        let bit = bit_of(button);
        self.released[player] & bit != 0
    }
}

fn bit_of(button: Button) -> u8 {
    ACTION_BITS
        .iter()
        .find(|(b, _)| *b == button)
        .map(|(_, bit)| *bit)
        .unwrap_or(0)
}

/// Map a DOS scancode (set 1) to a `minifb::Key`. Covers the keyboard keys a
/// config can reasonably assign (letters, digits, arrows, common controls).
#[cfg(feature = "window")]
pub fn minifb_key_for_scancode(scancode: u8) -> Option<minifb::Key> {
    use minifb::Key;
    Some(match scancode {
        0x01 => Key::Escape,
        0x0E => Key::Backspace,
        0x0F => Key::Tab,
        0x1C => Key::Enter,
        0x1D => Key::LeftCtrl,
        0x2A => Key::LeftShift,
        0x36 => Key::RightShift,
        0x38 => Key::LeftAlt,
        0x39 => Key::Space,
        0x3B => Key::F1,
        0x3C => Key::F2,
        0x3D => Key::F3,
        0x3E => Key::F4,
        0x3F => Key::F5,
        0x40 => Key::F6,
        0x41 => Key::F7,
        0x42 => Key::F8,
        0x43 => Key::F9,
        0x44 => Key::F10,
        0x47 => Key::NumPad7,
        0x48 => Key::Up,
        0x49 => Key::NumPad9,
        0x4A => Key::NumPadMinus,
        0x4B => Key::Left,
        0x4C => Key::NumPad5,
        0x4D => Key::Right,
        0x4F => Key::NumPad1,
        0x50 => Key::Down,
        0x51 => Key::NumPad3,
        0x52 => Key::NumPad0,
        0x53 => Key::NumPadDot,
        // digits 1..0
        0x02 => Key::Key1,
        0x03 => Key::Key2,
        0x04 => Key::Key3,
        0x05 => Key::Key4,
        0x06 => Key::Key5,
        0x07 => Key::Key6,
        0x08 => Key::Key7,
        0x09 => Key::Key8,
        0x0A => Key::Key9,
        0x0B => Key::Key0,
        // letters (scancode order on QWERTY)
        0x10 => Key::Q,
        0x11 => Key::W,
        0x12 => Key::E,
        0x13 => Key::R,
        0x14 => Key::T,
        0x15 => Key::Y,
        0x16 => Key::U,
        0x17 => Key::I,
        0x18 => Key::O,
        0x19 => Key::P,
        0x1E => Key::A,
        0x1F => Key::S,
        0x20 => Key::D,
        0x21 => Key::F,
        0x22 => Key::G,
        0x23 => Key::H,
        0x24 => Key::J,
        0x25 => Key::K,
        0x26 => Key::L,
        0x2C => Key::Z,
        0x2D => Key::X,
        0x2E => Key::C,
        0x2F => Key::V,
        0x30 => Key::B,
        0x31 => Key::N,
        0x32 => Key::M,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_shipped_config() {
        // Exactly the CONFIG.SF2 defaults decoded from the original data.
        let cfg = InputConfig::default();
        assert_eq!(cfg.players[0].map[0], (0x48, 0x01));
        assert_eq!(cfg.players[0].map[4], (0x1C, 0x20));
        assert_eq!(cfg.players[0].map[5], (0x39, 0x40));
        assert_eq!(cfg.players[1].map[0], (0x10, 0x01));
        assert_eq!(cfg.players[1].map[4], (0x02, 0x20));
        assert_eq!(cfg.players[1].map[5], (0x03, 0x40));
    }

    #[test]
    fn edges_are_detected() {
        let cfg = InputConfig::default();
        let mut state = InputState::default();
        // Press Up (0x48) then release.
        state.update(&cfg, |s| s == 0x48);
        assert!(state.just_pressed(0, Button::Up));
        assert!(state.is_held(0, Button::Up));
        state.update(&cfg, |_| false);
        assert!(state.just_released(0, Button::Up));
        assert!(!state.is_held(0, Button::Up));
    }
}
