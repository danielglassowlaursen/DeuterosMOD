use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::PlayerId;
use crate::items::ItemType;
use crate::world::World;

/// An order a player gives for the coming turn.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Points the research team at an item. Progress on the previous item is
    /// kept and resumes if the team is pointed back at it.
    SetResearch { item: ItemType },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandError {
    UnknownPlayer(PlayerId),
    ResearchUnavailable(ItemType),
    AlreadyResearched(ItemType),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::UnknownPlayer(id) => write!(f, "player {} is not in this game", id.0),
            CommandError::ResearchUnavailable(item) => {
                write!(f, "{item:?} is not available for research")
            }
            CommandError::AlreadyResearched(item) => write!(f, "{item:?} is already researched"),
        }
    }
}

impl std::error::Error for CommandError {}

impl Command {
    /// Validates the command against the current state and applies it.
    /// On error the world is left unchanged.
    pub(crate) fn apply(&self, world: &mut World, player: PlayerId) -> Result<(), CommandError> {
        let state = world
            .players
            .get_mut(&player)
            .ok_or(CommandError::UnknownPlayer(player))?;
        match *self {
            Command::SetResearch { item } => {
                let progress = state
                    .research
                    .get(&item)
                    .ok_or(CommandError::ResearchUnavailable(item))?;
                if progress.researched {
                    return Err(CommandError::AlreadyResearched(item));
                }
                state.current_research = Some(item);
            }
        }
        Ok(())
    }
}
