//! Pseudo-random generator — port of `FUN_1000_7F49` (`0x17F49`).
//!
//! The game seeds it from the BIOS tick counter (`INT 1A`, `0x17F3C`):
//! `[0x3CD0] = DX` (ticks), `[0x3CD2] = CX` (midnight rollovers). Each call
//! shuffles the two 16-bit words and returns the new first state in `AX`.
//!
//! With the shipped zero-initialised state the generator returns zero forever,
//! so the boot time seed (or any nonzero seed) is what actually drives it.

/// The generator's two 16-bit state words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rng {
    /// `DS:0x3CD0` (first state word; the returned value).
    pub state1: u16,
    /// `DS:0x3CD2` (second state word).
    pub state2: u16,
}

impl Default for Rng {
    fn default() -> Self {
        Rng {
            state1: 0,
            state2: 0,
        }
    }
}

impl Rng {
    /// Seeded generator (the engine seeds from the BIOS tick counter).
    pub fn seeded(seed1: u16, seed2: u16) -> Self {
        Rng {
            state1: seed1,
            state2: seed2,
        }
    }

    /// Next 16-bit value — port of `0x17F49`.
    pub fn next(&mut self) -> u16 {
        let ax = self.state2;
        let dx0 = self.state1;
        self.state2 = dx0;

        let dl = (dx0 >> 8) as u8; // mov dl, dh
        let dh_orig = (dx0 >> 8) as u8;
        let carry = (dh_orig >> 7) & 1; // shl dh, 1 -> CF
        let dh = (ax & 0xFF) as u8; // mov dh, al
        let ax_rot = (ax << 1) | carry as u16; // rcl ax, 1
        let dx = ((dh as u16) << 8) | dl as u16;
        let result = ax_rot ^ dx;
        self.state1 = result;
        result
    }

    /// Convenience: value in `0..n` (the engine masks/limits manually).
    pub fn below(&mut self, n: u16) -> u16 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_state_stays_zero() {
        let mut rng = Rng::default();
        for _ in 0..10 {
            assert_eq!(rng.next(), 0);
        }
    }

    /// Reference sequence produced by emulating the original instructions
    /// (seed 1,1) — see the analysis session log.
    #[test]
    fn reference_sequence_from_seed_1_1() {
        let mut rng = Rng::seeded(1, 1);
        let expected = [
            0x0102, 0x0103, 0x0005, 0x0106, 0x050B, 0x0409, 0x0112, 0x0113, 0x1025, 0x1136,
        ];
        for want in expected {
            assert_eq!(rng.next(), want);
        }
    }

    #[test]
    fn state_advances_like_the_original() {
        let mut rng = Rng::seeded(0x1234, 0x5678);
        let v = rng.next();
        assert_ne!(v, 0);
        assert_eq!(rng.state2, 0x1234); // [0x3CD2] = old state1
        assert_eq!(rng.state1, v);
    }
}
