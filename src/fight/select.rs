//! Character-select screen — port of `0x18953` (see
//! `docs/port/menus-select.md` §5).
//!
//! Ported faithfully: the 4×3 grid movement rules (`0x1920E..0x1922F`), the
//! confirm/lock semantics (`0x18A6B`) with the strict-mode used-cell gate, and
//! the CPU auto-pick algorithm (`0x18A9D`) including the boss-first roster
//! rule and the random path with the engine RNG.
//!
//! Not ported (documented): the exact cursor seeding (`0x190FE`), the portrait
//! composition (`0x18F68`) and the V.S. stage-cursor sub-phase.

use crate::fight::rng::Rng;

/// `0x44B4` — cell (0..11) to character `char2`.
pub const CELL_TO_CHAR2: [u8; 12] = [
    0x06, 0x04, 0x0E, 0x02, 0x00, 0x0A, 0x08, 0x0C, 0x10, 0x12, 0x14, 0x16,
];

/// `0x44C0` — character `char2 / 2` to cell (inverse of [`CELL_TO_CHAR2`]).
pub const CHAR_TO_CELL: [u8; 12] = [4, 3, 1, 0, 6, 5, 7, 2, 8, 9, 10, 11];

/// `0x44CC` — 4×3 grid pixel offsets for the icons.
pub const GRID_OFFSETS: [(u8, u8); 12] = [
    (0x00, 0x00),
    (0x20, 0x00),
    (0x40, 0x00),
    (0x60, 0x00),
    (0x00, 0x20),
    (0x20, 0x20),
    (0x40, 0x20),
    (0x60, 0x20),
    (0x00, 0x40),
    (0x20, 0x40),
    (0x40, 0x40),
    (0x60, 0x40),
];

/// `0x44E4` — map marker `{x, y, dx, dy}` per character (scattered flags);
/// `dx`/`dy` are signed bytes stored raw (`as i8` at use).
pub const MARKER_POS: [(u8, u8, u8, u8); 12] = [
    (0xCF, 0x2C, 0x04, 0xE0),
    (0xD7, 0x3C, 0x04, 0xF0),
    (0x80, 0x39, 0x02, 0xE8),
    (0x84, 0x3C, 0xFA, 0x06),
    (0x4B, 0x22, 0xDC, 0xEE),
    (0x6F, 0x2E, 0xF4, 0xDC),
    (0x52, 0x3E, 0xCC, 0x02),
    (0xEF, 0x58, 0xDC, 0xF8),
    (0xCF, 0x35, 0xE0, 0xD8),
    (0x0F, 0x2F, 0xE8, 0xD8),
    (0x65, 0x3E, 0xDC, 0x10),
    (0x61, 0x43, 0xE0, 0x0B),
];

/// `0x4448` — flag sprites in char order (last entry = `sprFLAG_CRO`).
pub const FLAG_SPRITES: [&str; 13] = [
    "sprFLAG_USA",
    "sprFLAG_USA",
    "sprFLAG_JAP",
    "sprFLAG_JAP",
    "sprFLAG_USR",
    "sprFLAG_CHI",
    "sprFLAG_IND",
    "sprFLAG_BRA",
    "sprFLAG_USA",
    "sprFLAG_SPA",
    "sprFLAG_THA",
    "sprFLAG_THA",
    "sprFLAG_CRO",
];

/// Cursor sprites (`sprPLAY1SEL` / `sprPLAY2SEL`).
pub const CURSOR_SPRITES: [&str; 2] = ["sprPLAY1SEL", "sprPLAY2SEL"];

/// A cursor direction from the player's key struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// Bit 1 of the direction nibble.
    Up,
    /// Bit 2.
    Down,
    /// Bit 4.
    Left,
    /// Bit 8.
    Right,
}

/// Character-select state (`0x4514`..`0x452B` block).
#[derive(Debug, Clone)]
pub struct Select {
    /// `0x4514`: `true` = V.S. BATTLE, `false` = GAME START (ladder).
    pub vs_mode: bool,
    /// `0x4515`: cursor may not rest on a used cell.
    pub strict: bool,
    /// `0x451A`: roster size (8 = base, 0xB / 0xC = expanded).
    pub roster: u8,
    /// `0x451B`: accepted confirm mask (1 = P1 only, 3 = both).
    pub confirm_mask: u8,
    /// `0x4523`/`0x4527`: cursor cells per player.
    pub cells: [u8; 2],
    /// Selected `char2` per player (`0xFF` = undecided).
    pub char2: [u8; 2],
    /// Locked flags per player (cursor `+3` == 0).
    pub locked: [bool; 2],
    /// `0x452B`: used-cell flags (`0` free, `0xFF` used).
    pub used: [u8; 12],
    /// `0x451F`: cell resolved for the CPU player.
    pub cpu_cell: u8,
    /// `0x4520`: V.S. stage cursor (`char2`).
    pub stage_cursor: u8,
}

impl Select {
    /// New select screen. Cursor seeding (`0x190FE`) is not decoded; KEN's
    /// cell is used for P1 and CHUN LI's for P2 as a stand-in.
    pub fn new(vs_mode: bool, roster: u8) -> Self {
        let cells = [CHAR_TO_CELL[0], CHAR_TO_CELL[5]];
        Select {
            vs_mode,
            strict: false,
            roster,
            confirm_mask: if vs_mode { 3 } else { 1 },
            cells,
            char2: [0xFF, 0xFF],
            locked: [false, false],
            used: [0; 12],
            cpu_cell: 0,
            stage_cursor: 0,
        }
    }

    /// `0x1920E..0x1922F` — one direction step.
    ///
    /// UP moves the row up; DOWN only works from the top row (as shipped);
    /// LEFT/RIGHT stay within the 4-column row.
    pub fn move_cursor(&mut self, player: usize, dir: Dir) {
        let cell = self.cells[player];
        let next = match dir {
            Dir::Up => {
                if cell >= 4 {
                    cell - 4
                } else {
                    cell
                }
            }
            Dir::Down => {
                if cell < 4 {
                    cell + 4
                } else {
                    cell
                }
            }
            Dir::Left => {
                if cell % 4 != 0 {
                    cell - 1
                } else {
                    cell
                }
            }
            Dir::Right => {
                if cell % 4 != 3 {
                    cell + 1
                } else {
                    cell
                }
            }
        };
        self.cells[player] = next;
    }

    /// `0x18A6B` — confirm. Returns false when the cell is rejected.
    pub fn confirm(&mut self, player: usize) -> bool {
        if self.locked[player] {
            return false;
        }
        let cell = self.cells[player];
        // strict mode rejects cells already used by someone.
        if self.strict && self.used[cell as usize] == 0xFF {
            return false;
        }
        self.locked[player] = true;
        self.confirm_mask |= 1 << player;
        self.char2[player] = CELL_TO_CHAR2[cell as usize];
        true
    }

    /// Whether the select loop is done (`0x14CE2` mask test in 1P; both locks
    /// in V.S.).
    pub fn finished(&self) -> bool {
        if self.vs_mode {
            self.locked[0] && self.locked[1]
        } else {
            self.locked[0]
        }
    }

    /// `0x18A9D` — CPU player resolution. Returns the picked `char2`.
    pub fn cpu_pick(&mut self, rng: &mut Rng) -> u8 {
        let taken = |s: &Self, c: u8| c == s.char2[0] || c == s.char2[1];
        let cell = if self.roster != 8 {
            // Boss-first: first free cell from 8 upward.
            let mut idx = 8u8;
            loop {
                if idx >= 12 {
                    break 8;
                }
                if self.used[idx as usize] != 0xFF {
                    break idx;
                }
                idx += 1;
            }
        } else {
            loop {
                let mut a = (rng.next() & 0xF) as u8;
                if self.roster == 8 {
                    a &= 7;
                }
                if a >= self.roster || a >= 12 {
                    continue;
                }
                if self.used[a as usize] == 0xFF {
                    continue;
                }
                let c = CELL_TO_CHAR2[a as usize];
                if taken(self, c) {
                    continue;
                }
                break a;
            }
        };
        self.cpu_cell = cell;
        let c = CELL_TO_CHAR2[cell as usize];
        if self.char2[0] == 0xFF {
            self.char2[0] = c;
        } else if self.char2[1] == 0xFF {
            self.char2[1] = c;
        }
        c
    }

    /// Final outcome: `(human mask, [char2 P1, char2 P2])`.
    pub fn outcome(&self) -> (u8, [u8; 2]) {
        (self.confirm_mask, self.char2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_match_the_binary() {
        // cell -> char2 (RYU, HONDA, BLANKA, GUILE, KEN, CHUNLI, ZANGIEF,
        // DHALSIM, BALROG, VEGA, SAGAT, BISON).
        assert_eq!(CELL_TO_CHAR2[0], 6); // RYU
        assert_eq!(CELL_TO_CHAR2[4], 0); // KEN
        assert_eq!(CHAR_TO_CELL[0], 4); // KEN's cell
        assert_eq!(GRID_OFFSETS[11], (0x60, 0x40));
        assert_eq!(MARKER_POS[0], (0xCF, 0x2C, 4, 0xE0));
        assert_eq!(FLAG_SPRITES[2], "sprFLAG_JAP");
    }

    #[test]
    fn movement_rules_match_the_handlers() {
        let mut s = Select::new(false, 8);
        s.cells[0] = 5; // row 1, column 1
        s.move_cursor(0, Dir::Up);
        assert_eq!(s.cells[0], 1);
        s.move_cursor(0, Dir::Up); // top row: no-op
        assert_eq!(s.cells[0], 1);
        s.move_cursor(0, Dir::Down); // from the top row: works
        assert_eq!(s.cells[0], 5);
        s.move_cursor(0, Dir::Down); // mid rows: no-op (as shipped)
        assert_eq!(s.cells[0], 5);
        s.move_cursor(0, Dir::Right);
        assert_eq!(s.cells[0], 6);
        s.move_cursor(0, Dir::Right);
        assert_eq!(s.cells[0], 7);
        s.move_cursor(0, Dir::Right); // column edge: no-op
        assert_eq!(s.cells[0], 7);
        s.move_cursor(0, Dir::Left);
        assert_eq!(s.cells[0], 6);
    }

    #[test]
    fn confirm_locks_and_maps_the_cell() {
        let mut s = Select::new(false, 8);
        s.cells[0] = CHAR_TO_CELL[0];
        assert!(s.confirm(0));
        assert_eq!(s.char2[0], 0);
        assert_eq!(s.confirm_mask & 1, 1);
        assert!(!s.confirm(0), "already locked");
        assert!(s.finished());
        // strict mode rejects used cells.
        let mut s = Select::new(false, 8);
        s.strict = true;
        s.used[CHAR_TO_CELL[0] as usize] = 0xFF;
        s.cells[0] = CHAR_TO_CELL[0];
        assert!(!s.confirm(0));
    }

    #[test]
    fn cpu_pick_roster_and_used_rules() {
        // Expanded roster: first free boss cell (8..11).
        let mut s = Select::new(false, 0xC);
        s.char2[0] = 0; // KEN picked by P1
        let mut rng = Rng::seeded(0x1234, 0x5678);
        let c = s.cpu_pick(&mut rng);
        assert_eq!(c, CELL_TO_CHAR2[8], "first boss cell = BALROG char2 0x10");
        assert_eq!(s.char2[1], 0x10);
        // Roster 8: random path stays inside 0..8, unused, and != picks.
        let mut s = Select::new(false, 8);
        s.char2[0] = CELL_TO_CHAR2[4]; // KEN
        let mut rng = Rng::seeded(1, 1);
        for _ in 0..50 {
            let c = s.cpu_pick(&mut rng);
            assert!((0..=0x0E).contains(&c));
            assert_ne!(c, s.char2[0]);
            assert!(CELL_TO_CHAR2[..8].contains(&c), "must be in the base roster");
        }
    }
}
