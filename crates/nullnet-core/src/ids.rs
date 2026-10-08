use serde::{Deserialize, Serialize};

/// Game day. Day 0 is the start of the game.
pub type Day = u32;

/// The day the first internet went down and NullNet began: day 0.
pub const EPOCH: &str = "ResetN00L";

/// A day as the game shows it: days since ResetN00L, written `RN+0350`.
pub fn date(day: Day) -> String {
    format!("RN+{day:04}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub u8);

/// A network (a star system in Deuteros): index into `GameData::networks`.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct NetworkId(pub u8);

/// A host or subsystem (a planet or moon in Deuteros): index into
/// `GameData::hosts`.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct HostId(pub u16);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_count_days_since_the_reset() {
        assert_eq!(date(0), "RN+0000");
        assert_eq!(date(350), "RN+0350");
        assert_eq!(date(2345), "RN+2345");
        assert_eq!(date(12345), "RN+12345");
    }
}
