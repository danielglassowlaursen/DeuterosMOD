use serde::{Deserialize, Serialize};

/// Game day. Day 0 is the start of the game.
pub type Day = u32;

/// Calendar date as the original shows it: year 3100 + day / 1000, and the
/// day within that year (0-999).
pub fn calendar_date(day: Day) -> (u32, u32) {
    (3100 + day / 1000, day % 1000)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub u8);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_matches_the_original() {
        assert_eq!(calendar_date(0), (3100, 0));
        assert_eq!(calendar_date(999), (3100, 999));
        assert_eq!(calendar_date(1000), (3101, 0));
        assert_eq!(calendar_date(2345), (3102, 345));
    }
}
