use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{Day, PlayerId};
use crate::items::ItemType;
use crate::research::{ResearchDef, ResearchProgress};
use crate::rng::Rng;
use crate::staff::Staff;

/// The complete mutable state of one game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub day: Day,
    pub rng: Rng,
    pub players: BTreeMap<PlayerId, Player>,
}

impl World {
    pub fn new(seed: u64) -> Self {
        World {
            day: 0,
            rng: Rng::new(seed, 0),
            players: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub name: String,
    /// The single research team on Earth; `None` until trainees graduate.
    pub research_team: Option<Staff>,
    pub current_research: Option<ItemType>,
    /// Research the player has access to, in progress or finished.
    pub research: BTreeMap<ItemType, ResearchProgress>,
}

impl Player {
    pub fn new(name: impl Into<String>) -> Self {
        Player {
            name: name.into(),
            research_team: None,
            current_research: None,
            research: BTreeMap::new(),
        }
    }

    /// Makes an item available for research. Does nothing if it already is,
    /// so earlier progress is never reset.
    pub fn unlock_research(&mut self, item: ItemType, def: &ResearchDef) {
        self.research
            .entry(item)
            .or_insert_with(|| ResearchProgress::new(def));
    }
}
