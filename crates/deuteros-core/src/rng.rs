use serde::{Deserialize, Serialize};

/// PCG32 (XSH-RR) random number generator.
///
/// Implemented here rather than taken from `rand` so the sequence for a given
/// seed can never change under a dependency upgrade: the state is saved with
/// the world, and a turn must resolve the same way when it is replayed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rng {
    state: u64,
    inc: u64,
}

impl Rng {
    const MULTIPLIER: u64 = 6_364_136_223_846_793_005;

    /// Seeds the generator the same way as the reference `pcg32_srandom_r`.
    pub fn new(seed: u64, stream: u64) -> Self {
        let mut rng = Rng {
            state: 0,
            inc: (stream << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(Self::MULTIPLIER).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform value in `0..bound`, without modulo bias.
    ///
    /// # Panics
    /// If `bound` is 0.
    pub fn below(&mut self, bound: u32) -> u32 {
        assert!(bound > 0, "Rng::below called with an empty range");
        // Reject the low values that would make some results more likely.
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let r = self.next_u32();
            if r >= threshold {
                return r % bound;
            }
        }
    }

    /// Uniform value in `lo..=hi`.
    ///
    /// # Panics
    /// If `lo > hi`.
    pub fn range_inclusive(&mut self, lo: u32, hi: u32) -> u32 {
        assert!(lo <= hi, "Rng::range_inclusive called with lo > hi");
        match (hi - lo).checked_add(1) {
            Some(span) => lo + self.below(span),
            None => self.next_u32(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_reference_pcg32_sequence() {
        // First outputs of the PCG reference demo, pcg32_srandom_r(42, 54).
        let mut rng = Rng::new(42, 54);
        let expected = [
            0xa15c_02b7,
            0x7b47_f409,
            0xba1d_3330,
            0x83d2_f293,
            0xbfa4_784b,
            0xcbed_606e,
        ];
        for value in expected {
            assert_eq!(rng.next_u32(), value);
        }
    }

    #[test]
    fn below_stays_in_range() {
        let mut rng = Rng::new(1, 1);
        for bound in [1, 2, 3, 7, 1000, u32::MAX] {
            for _ in 0..1000 {
                assert!(rng.below(bound) < bound);
            }
        }
    }

    #[test]
    fn range_inclusive_covers_both_ends() {
        let mut rng = Rng::new(7, 3);
        let mut seen = [false; 4];
        for _ in 0..1000 {
            let v = rng.range_inclusive(5, 8);
            seen[(v - 5) as usize] = true;
        }
        assert_eq!(seen, [true; 4]);
        rng.range_inclusive(0, u32::MAX);
    }

    #[test]
    fn serde_round_trip_continues_the_same_sequence() {
        let mut rng = Rng::new(123, 9);
        rng.next_u32();
        let mut restored: Rng =
            serde_json::from_str(&serde_json::to_string(&rng).unwrap()).unwrap();
        for _ in 0..10 {
            assert_eq!(rng.next_u32(), restored.next_u32());
        }
    }
}
