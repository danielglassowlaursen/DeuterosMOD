use std::fmt;

use serde::{Deserialize, Serialize};

use crate::data::GameData;
use crate::ids::{HostId, PlayerId};
use crate::items::ItemType;
use crate::recruitment;
use crate::site::Site;
use crate::staff::StaffKind;
use crate::transport::{self, Berth, Destination, ModuleKind, Seat, VesselId, VesselKind};
use crate::turn::Event;
use crate::workshop::{self, AutoMode, SiteRef, WorkshopRef};
use crate::world::World;

/// An order a player gives for the coming turn.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Points the analyst team at an item. Progress on the previous item is
    /// kept and resumes if the team is pointed back at it.
    SetResearch {
        item: ItemType,
    },
    /// Sets how many recruits the next course of `kind` takes; 0 cancels.
    /// Only possible while that course is idle.
    Recruit {
        kind: StaffKind,
        count: u32,
    },
    /// Puts a workshop's coders to work on `item`, pausing their current job.
    Build {
        at: WorkshopRef,
        item: ItemType,
    },
    /// Puts taps from a site's store into service there, up to
    /// [`Site::MAX_TAPS`](crate::Site::MAX_TAPS).
    InstallTaps {
        site: SiteRef,
        count: u32,
    },
    /// Adds, changes or stops an item in a build-bot's queue.
    Automate {
        at: WorkshopRef,
        item: ItemType,
        mode: AutoMode,
    },
    /// Puts a coder team from the site's staff slots in charge of a
    /// workshop that has none.
    AssignCoders {
        at: WorkshopRef,
        team: usize,
    },
    /// Moves a workshop's coders to the site's staff slots.
    ReleaseCoders {
        at: WorkshopRef,
    },
    /// Builds a vessel from its core and engine in a bay at a crew's host:
    /// inside it (`Planted`, droppers only) or in its citadel (`Connected`).
    Assemble {
        host: HostId,
        berth: Berth,
        kind: VesselKind,
    },
    Refuel {
        vessel: VesselId,
        amount: u32,
    },
    /// Fits a module slot with a pod, or empties it with `None`.
    Fit {
        vessel: VesselId,
        slot: usize,
        module: Option<ModuleKind>,
    },
    Load {
        vessel: VesselId,
        slot: usize,
        item: ItemType,
        count: u32,
    },
    Unload {
        vessel: VesselId,
        slot: usize,
    },
    Board {
        vessel: VesselId,
        seat: Seat,
        team: usize,
    },
    Disembark {
        vessel: VesselId,
        seat: Seat,
    },
    /// Sends a vessel on its way; it keeps going over the following turns.
    Dispatch {
        vessel: VesselId,
        to: Destination,
    },
    /// Installs the citadel module or backdoor kit in a tool module.
    Deploy {
        vessel: VesselId,
        slot: usize,
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
    NotCoders,
    WorkshopAutomated,
    WorkshopNotAutomated,
    UnknownVessel(VesselId),
    VesselNotDocked,
    WrongBerth,
    DropperAlreadyHere,
    NoSuchModule,
    ModuleNotEmpty,
    WrongModule(ItemType),
    ModuleFull,
    NoSuchTeam,
    SeatTaken,
    NoStaffSlot,
    PilotMustBeOperator,
    NoPilot,
    OutOfRange(HostId),
    HostTaken(HostId),
    NothingToDeploy,
    NotDeployable(ItemType),
    AlreadyComplete,
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
            CommandError::NotCoders => write!(f, "only a coder team can run a workshop"),
            CommandError::UnknownVessel(id) => write!(f, "vessel {} is not yours", id.0),
            CommandError::VesselNotDocked => {
                write!(f, "the vessel must be planted or connected for that")
            }
            CommandError::WrongBerth => write!(f, "the vessel cannot be there for that"),
            CommandError::DropperAlreadyHere => write!(f, "your crew already has a dropper here"),
            CommandError::NoSuchModule => write!(f, "the vessel has no such module"),
            CommandError::ModuleNotEmpty => write!(f, "the module holds something else"),
            CommandError::WrongModule(item) => write!(f, "that module cannot take {item:?}"),
            CommandError::ModuleFull => write!(f, "the module cannot hold that much"),
            CommandError::NoSuchTeam => write!(f, "there is no such team"),
            CommandError::SeatTaken => write!(f, "that seat is taken"),
            CommandError::NoStaffSlot => write!(f, "every staff slot here is taken"),
            CommandError::PilotMustBeOperator => write!(f, "only operators can run a vessel"),
            CommandError::NoPilot => write!(f, "the vessel has no operator"),
            CommandError::OutOfRange(id) => write!(f, "this vessel cannot reach host {}", id.0),
            CommandError::HostTaken(id) => write!(f, "host {} is held by someone else", id.0),
            CommandError::NothingToDeploy => write!(f, "that module holds nothing to install"),
            CommandError::NotDeployable(item) => write!(f, "{item:?} cannot be installed"),
            CommandError::AlreadyComplete => write!(f, "that is already complete"),
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
        events: &mut Vec<Event>,
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
            Command::AssignCoders { at, team } => {
                workshop::assign_coders(workshop::locate(world, player, at)?.0, team)
            }
            Command::ReleaseCoders { at } => {
                workshop::release_coders(workshop::locate(world, player, at)?.0)
            }
            Command::Assemble { host, berth, kind } => {
                transport::assemble(data, world, player, host, berth, kind).map(|_| ())
            }
            Command::Refuel { vessel, amount } => {
                transport::refuel(data, world, player, vessel, amount)
            }
            Command::Fit {
                vessel,
                slot,
                module,
            } => transport::fit(data, world, player, vessel, slot, module),
            Command::Load {
                vessel,
                slot,
                item,
                count,
            } => transport::load(data, world, player, vessel, slot, item, count),
            Command::Unload { vessel, slot } => {
                transport::unload(data, world, player, vessel, slot)
            }
            Command::Board { vessel, seat, team } => {
                transport::board(data, world, player, vessel, seat, team)
            }
            Command::Disembark { vessel, seat } => {
                transport::disembark(data, world, player, vessel, seat)
            }
            Command::Dispatch { vessel, to } => {
                transport::dispatch(data, world, player, vessel, to, events)
            }
            Command::Deploy { vessel, slot } => {
                transport::deploy(data, world, player, vessel, slot, events)
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
