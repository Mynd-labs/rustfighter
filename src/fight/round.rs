//! Round state — port of the round setup (`0x152FE`), loop (`0x15083`) and
//! round-end dispatcher (`0x14573`).
//!
//! Verified behaviour:
//!
//! * Setup resets `timer = 200` (`0x308A = 0xC8`), `result = 0xFF` (`0x3233`),
//!   round/battle counters (`0x2A3A`/`0x2A38 = 1`).
//! * While `result == 0xFF` and both fighters have energy, the timer counts
//!   down once per frame; the loop exits when a fighter reaches 0 energy
//!   (KO) or when the timer expires (TIME OVER).
//! * Round end (`0x14573`): the higher energy wins. The winner gets
//!   `flags_a |= 0x8000`, the loser `flags_b |= 8`; equal energy sets the
//!   draw bits on both. TIME OVER is signalled to the caller through carry.
//! * After a round, `[0x3233]` is reset to 0 and the loop returns the result
//!   in `AL` (`0x1534C`).

use crate::fight::fighter::Fighter;

/// Initial round timer value (`0x152FE: mov word [0x308A], 0xC8`).
pub const ROUND_TIME: u16 = 200;

/// `flags_a` bit set on the winner (`0x14589`).
pub const WINNER_FLAG: u16 = 0x8000;
/// `flags_b` bit set on the loser / both fighters on a draw (`0x1458E/0x14594`).
pub const LOSER_FLAG: u16 = 0x0008;

/// Final outcome of a round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundOutcome {
    /// Winner index (0 = P1, 1 = P2).
    Winner {
        /// Index of the winning fighter.
        player: usize,
    },
    /// Equal energy.
    Draw,
    /// Timer expired with a higher-energy winner.
    TimeOver {
        /// Winner index, or `None` for an equal-energy time over.
        winner: Option<usize>,
    },
}

/// Round state (`DS:0x308A` timer, `DS:0x3233` result, `DS:0x322E` KO flag).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Round {
    /// Remaining timer (`DS:0x308A`).
    pub timer: u16,
    /// Round result (`DS:0x3233`); `0xFF` while undecided.
    pub result: u8,
    /// KO latch (`DS:0x322E`, set by the KO check at `0x17810`).
    pub ko: bool,
    /// Outcome once decided.
    pub outcome: Option<RoundOutcome>,
}

impl Default for Round {
    fn default() -> Self {
        Round {
            timer: ROUND_TIME,
            result: 0xFF,
            ko: false,
            outcome: None,
        }
    }
}

impl Round {
    /// New round exactly as `0x152FE` sets it up.
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance one frame of the round loop (`0x15083`).
    ///
    /// `freeze` corresponds to the `0x4CCA()` gate returning true (hit-stop /
    /// event frame) during which the timer does not tick.
    ///
    /// Returns the outcome on the frame it is decided.
    pub fn tick(
        &mut self,
        energy: [u8; 2],
        freeze: bool,
    ) -> Option<RoundOutcome> {
        if self.result != 0xFF {
            return self.outcome;
        }
        if energy[0] == 0 || energy[1] == 0 {
            // A fighter is down: resolve as a KO (not a time over).
            return Some(self.resolve(energy, false));
        }
        if freeze {
            return None;
        }
        if self.timer > 0 {
            self.timer -= 1;
        }
        if self.timer == 0 {
            return Some(self.resolve(energy, true));
        }
        None
    }

    /// Energy comparison and flag effects, port of `0x14573`.
    pub fn resolve(&mut self, energy: [u8; 2], time_over: bool) -> RoundOutcome {
        let e1 = energy[0];
        let e2 = energy[1];
        let outcome = if e1 == e2 {
            RoundOutcome::Draw
        } else if e1 > e2 {
            RoundOutcome::Winner { player: 0 }
        } else {
            RoundOutcome::Winner { player: 1 }
        };
        let outcome = if time_over {
            match outcome {
                RoundOutcome::Draw => RoundOutcome::TimeOver { winner: None },
                RoundOutcome::Winner { player } => RoundOutcome::TimeOver {
                    winner: Some(player),
                },
                o => o,
            }
        } else {
            outcome
        };
        // The dispatcher flags are applied to the structs by the caller via
        // `apply_round_flags`; keep `result` in sync for the loop's exit test.
        self.result = match outcome {
            RoundOutcome::Winner { .. } | RoundOutcome::TimeOver { winner: Some(_) } => 1,
            RoundOutcome::Draw | RoundOutcome::TimeOver { winner: None } => 2,
        };
        self.outcome = Some(outcome);
        outcome
    }

    /// Apply the winner/loser flag effects to both fighters (`0x14573`).
    pub fn apply_flags(&self, fighters: [&mut Fighter; 2]) {
        let [p1, p2] = fighters;
        match self.outcome {
            Some(RoundOutcome::Winner { player }) | Some(RoundOutcome::TimeOver { winner: Some(player) }) => {
                let (winner, loser) = if player == 0 { (p1, p2) } else { (p2, p1) };
                winner.flags_a |= WINNER_FLAG;
                loser.flags_b |= LOSER_FLAG;
            }
            Some(RoundOutcome::Draw) | Some(RoundOutcome::TimeOver { winner: None }) => {
                p1.flags_b |= LOSER_FLAG;
                p2.flags_b |= LOSER_FLAG;
            }
            None => {}
        }
    }
}

/// AI distance bucket (`FUN_1000_04FB`, thresholds at `DS:0x50`):
/// `(0x30, 0x40, 0x50, 0x60, 0x70, 0x90, 0xC0, 0x8000)`.
///
/// Returns the bucket index 0..=7 for an absolute X distance.
pub fn distance_bucket(dx: u16) -> u8 {
    const THRESHOLDS: [u16; 8] = [0x30, 0x40, 0x50, 0x60, 0x70, 0x90, 0xC0, 0x8000];
    THRESHOLDS
        .iter()
        .position(|&t| dx < t)
        .unwrap_or(7) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_matches_0x152fe() {
        let round = Round::new();
        assert_eq!(round.timer, 200);
        assert_eq!(round.result, 0xFF);
        assert!(!round.ko);
    }

    #[test]
    fn timer_counts_down_and_times_over() {
        let mut round = Round::new();
        round.timer = 2;
        assert!(round.tick([100, 100], false).is_none());
        assert_eq!(round.timer, 1);
        let out = round.tick([100, 100], false).unwrap();
        assert_eq!(out, RoundOutcome::TimeOver { winner: None });
        assert_ne!(round.result, 0xFF);
        // Idempotent after decision.
        assert_eq!(round.tick([100, 100], false), Some(out));
    }

    #[test]
    fn freeze_pauses_timer() {
        let mut round = Round::new();
        round.timer = 5;
        assert!(round.tick([100, 100], true).is_none());
        assert_eq!(round.timer, 5);
    }

    #[test]
    fn ko_resolves_immediately() {
        let mut round = Round::new();
        let out = round.tick([100, 0], false).unwrap();
        assert_eq!(out, RoundOutcome::Winner { player: 0 });
    }

    #[test]
    fn equal_energy_is_draw() {
        let mut round = Round::new();
        let out = round.tick([50, 50], false);
        // both alive: no resolution yet
        assert!(out.is_none());
        let out = round.resolve([50, 50], true);
        assert_eq!(out, RoundOutcome::TimeOver { winner: None });
    }

    #[test]
    fn flags_applied_like_dispatcher() {
        let mut round = Round::new();
        round.resolve([80, 30], false);
        let mut p1 = Fighter::spawn(0, false);
        let mut p2 = Fighter::spawn(0, true);
        round.apply_flags([&mut p1, &mut p2]);
        assert!(p1.flags_a & WINNER_FLAG != 0);
        assert!(p2.flags_b & LOSER_FLAG != 0);
        // Draw path.
        let mut round = Round::new();
        round.resolve([50, 50], false);
        let mut p1 = Fighter::spawn(0, false);
        let mut p2 = Fighter::spawn(0, true);
        round.apply_flags([&mut p1, &mut p2]);
        assert!(p1.flags_b & LOSER_FLAG != 0);
        assert!(p2.flags_b & LOSER_FLAG != 0);
    }

    #[test]
    fn distance_buckets_match_thresholds() {
        assert_eq!(distance_bucket(0x00), 0);
        assert_eq!(distance_bucket(0x2F), 0);
        assert_eq!(distance_bucket(0x30), 1);
        assert_eq!(distance_bucket(0x3F), 1);
        assert_eq!(distance_bucket(0x40), 2);
        assert_eq!(distance_bucket(0xBF), 6);
        assert_eq!(distance_bucket(0xC0), 7);
        assert_eq!(distance_bucket(0x7FFF), 7);
    }
}
