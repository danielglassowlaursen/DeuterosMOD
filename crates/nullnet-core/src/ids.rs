use serde::{Deserialize, Serialize};

/// The day the first internet went down and NullNet began.
pub const EPOCH: &str = "ResetN00L";

/// A crew in a game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub u8);

/// A host on the map: index into `GameData::hosts`.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct HostId(pub u16);

impl HostId {
    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}

/// A hacker, on a crew or on offer in a market. Unique within a game.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct HackerId(pub u32);
