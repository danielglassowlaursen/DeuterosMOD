use std::fmt;

use serde::{Deserialize, Serialize};

use crate::data::GameData;
use crate::ids::{HostId, PlayerId};
use crate::items::ItemType;
use crate::recruitment;
use crate::site::Site;
use crate::staff::StaffKind;
use crate::workshop::{self, AutoMode, SiteRef, WorkshopRef};
use crate::world::World;

/// An order a player gives for the coming turn.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Points the analyst team at an item. Progress on the previous item is
    /// kept and resumes if the team is pointed back at it.
    SetResearch { item: ItemType },
    /// Sets how many recruits the next course of `kind` takes; 0 cancels.
    /// Only possible while that course is idle.
    Recruit { kind: StaffKind, count: u32 },
    /// Puts a workshop's coders to work on `item`, pausing their current job.
    Build { at: WorkshopRef, item: ItemType },
    /// Puts taps from a site's store into service there, up to
    /// [`Site::MAX_TAPS`](crate::Site::MAX_TAPS).
    InstallTaps { site: SiteRef, count: u32 },
    /// Adds, changes or stops an item in a build-bot's queue.
    Automate {
        at: WorkshopRef,
        item: ItemType,
        mode: AutoMode,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandError {
    UnknownPlayer(PlayerId),
    UnknownHost(HostId),
    NotYourHost(HostId),
    NoCitadel,
    ResearchUnavailable(ItemType),
    AlreadyResearched(ItemType),
    CourseRunning(StaffKind),
    TooManyRecruits(StaffKind),
    NotBuildable(ItemType),
    NotResearched(ItemType),
    OrbitOnly(ItemType),
    TeamLevelTooLow { item: ItemType, needed: u8 },
    MissingResources(ItemType),
    TooManyTaps,
    NoCoders,
    WorkshopAutomated,
    WorkshopNotAutomated,
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::UnknownPlayer(id) => write!(f, "player {} is not in this game", id.0),
            CommandError::UnknownHost(id) => write!(f, "host {} does not exist", id.0),
            CommandError::NotYourHost(id) => write!(f, "host {} is not held by your crew", id.0),
            CommandError::NoCitadel => write!(f, "there is no complete citadel there"),
            CommandError::ResearchUnavailable(item) => {
                write!(f, "{item:?} is not available for research")
            }
            CommandError::AlreadyResearched(item) => write!(f, "{item:?} is already researched"),
            CommandError::CourseRunning(kind) => {
                write!(f, "the {kind:?} course is running and cannot change")
            }
            CommandError::TooManyRecruits(kind) => {
                write!(
                    f,
                    "that many recruits would break the {kind:?} course limits"
                )
            }
            CommandError::NotBuildable(item) => write!(f, "{item:?} cannot be built"),
            CommandError::NotResearched(item) => write!(f, "{item:?} has not been researched"),
            CommandError::OrbitOnly(item) => {
                write!(f, "{item:?} can only be built in a citadel")
            }
            CommandError::TeamLevelTooLow { item, needed } => {
                write!(f, "{item:?} needs a level {needed} team leader")
            }
            CommandError::MissingResources(item) => {
                write!(f, "the store lacks the resources for {item:?}")
            }
            CommandError::TooManyTaps => {
                write!(f, "a site takes at most {} taps", Site::MAX_TAPS)
            }
            CommandError::NoCoders => write!(f, "the workshop has no coders"),
            CommandError::WorkshopAutomated => {
                write!(f, "a build-bot runs this workshop; queue items instead")
            }
            CommandError::WorkshopNotAutomated => {
                write!(f, "this workshop has no build-bot to queue items for")
            }
        }
    }
}

impl std::error::Error for CommandError {}

impl Command {
    /// Validates the command against the current state and applies it.
    /// On error the world is left unchanged.
    pub(crate) fn apply(
        &self,
        data: &GameData,
        world: &mut World,
        player: PlayerId,
    ) -> Result<(), CommandError> {
        match *self {
            Command::SetResearch { item } => {
                let state = player_mut(world, player)?;
                let progress = state
                    .research
                    .get(&item)
                    .ok_or(CommandError::ResearchUnavailable(item))?;
                if progress.researched {
                    return Err(CommandError::AlreadyResearched(item));
                }
                state.current_research = Some(item);
                Ok(())
            }
            Command::Recruit { kind, count } => {
                recruitment::enrol(data, player_mut(world, player)?, kind, count)
            }
            Command::Build { at, item } => {
                let (bench, research) = workshop::locate(world, player, at)?;
                workshop::build(data, research, bench, item)
            }
            Command::InstallTaps { site, count } => {
                let site = world.crew_site_mut(player, site)?;
                if site.taps + count > Site::MAX_TAPS {
                    return Err(CommandError::TooManyTaps);
                }
                if !site.store.take(ItemType::Tap, count) {
                    return Err(CommandError::MissingResources(ItemType::Tap));
                }
                site.taps += count;
                Ok(())
            }
            Command::Automate { at, item, mode } => {
                let (bench, research) = workshop::locate(world, player, at)?;
                workshop::automate(data, research, bench, item, mode)
            }
        }
    }
}

fn player_mut(world: &mut World, player: PlayerId) -> Result<&mut crate::Player, CommandError> {
    world
        .players
        .get_mut(&player)
        .ok_or(CommandError::UnknownPlayer(player))
}
