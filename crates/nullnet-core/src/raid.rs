//! Crew against crew: a vessel with daemons under a C2 controller raids a
//! host another crew holds. New to NullNet; Deuteros had no rival players.
//!
//! The defence is automatic: the daemons stored in the citadel fight,
//! commanded by the best operator team stationed there. A raid that wins
//! does what the crew chose before it set out: exfiltrates from the host's
//! stores into the vessel's data containers, plants a tap that siphons a
//! share of the host's extraction home for a while, or takes the host over
//! with everything on it. Every raid leaves heat on the raiding crew, and
//! the Legacy Net goes for the hottest crew at war.

use serde::{Deserialize, Serialize};

use crate::battle::{self, Outcome, Side};
use crate::command::CommandError;
use crate::data::{GameData, ItemCategory};
use crate::ids::{Day, HostId, PlayerId};
use crate::items::ItemType;
use crate::legacy::{self, DAEMON_CAP};
use crate::links::LinkConfig;
use crate::site::Citadel;
use crate::staff::StaffKind;
use crate::store::Store;
use crate::transport::{Berth, Cargo, Module, VesselId, VesselState};
use crate::turn::Event;
use crate::world::{Controller, World};

/// Turns from the start of the game in which crews cannot raid each other.
pub const PROTECTION_TURNS: u32 = 5;
/// Heat a raid puts on the raiding crew; it cools by one a day.
pub const RAID_HEAT: u32 = 25;
/// Heat every complete citadel a crew holds adds while it holds it.
pub const CITADEL_HEAT: u32 = 10;
/// Days a planted tap siphons for.
pub const SIPHON_DAYS: u32 = 100;
/// A planted tap takes one part in this of each day's extraction, rounded up.
pub const SIPHON_SHARE: u32 = 4;

/// What a raid does if it wins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RaidGoal {
    /// Fill the vessel's data containers from the host's stores.
    Exfiltrate,
    /// Siphon a share of the host's extraction home for [`SIPHON_DAYS`].
    PlantTap,
    /// Take the host, its citadel and everything on it.
    TakeOver,
}

/// A tap another crew planted on a site; it siphons until `until`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Siphon {
    pub player: PlayerId,
    pub until: Day,
}

/// A crew's heat: its raids, cooling by a point a day, plus the citadels it
/// holds. The Legacy Net hunts the hottest crew at war.
pub fn heat(world: &World, player: PlayerId) -> u32 {
    let raids = world.players.get(&player).map_or(0, |p| p.heat);
    let citadels = world
        .hosts
        .iter()
        .filter(|state| {
            state.controller == Some(Controller::Crew(player)) && state.site.citadel.complete()
        })
        .count() as u32;
    raids + citadels * CITADEL_HEAT
}

/// The side a citadel's garrison fights as: its stored daemons, at most
/// [`DAEMON_CAP`] at a time, under the best operator team stationed there.
pub(crate) fn garrison(citadel: &Citadel) -> Side {
    Side {
        daemons: citadel.store.get(ItemType::Daemon).min(DAEMON_CAP),
        level: citadel
            .staff
            .iter()
            .filter(|s| s.kind == StaffKind::Operator)
            .map(|s| s.level())
            .max()
            .unwrap_or(0),
    }
}

/// Puts the garrison's losses into its store.
pub(crate) fn settle_garrison(citadel: &mut Citadel, fought: u32, left: u32) {
    citadel.store.take(ItemType::Daemon, fought);
    citadel.store.add(ItemType::Daemon, left);
}

/// Cools every crew's heat and ends the siphons that have run their course.
pub(crate) fn run_day(world: &mut World) {
    let day = world.day;
    for player in world.players.values_mut() {
        player.heat = player.heat.saturating_sub(1);
    }
    for host in &mut world.hosts {
        host.site.siphons.retain(|s| s.until > day);
    }
}

/// A lurking vessel with daemons under a C2 controller raids the host it
/// is at, which another crew must hold.
pub(crate) fn raid(
    data: &GameData,
    world: &mut World,
    owner: PlayerId,
    id: VesselId,
    goal: RaidGoal,
    events: &mut Vec<Event>,
) -> Result<(), CommandError> {
    let vessel = world
        .vessels
        .get(&id)
        .filter(|v| v.owner == owner)
        .ok_or(CommandError::UnknownVessel(id))?;
    if vessel.berth() != Some(Berth::Lurking) {
        return Err(CommandError::WrongBerth);
    }
    if vessel.pilot.is_none() {
        return Err(CommandError::NoPilot);
    }
    if !vessel.c2 {
        return Err(CommandError::NoC2);
    }
    if vessel.daemons == 0 {
        return Err(CommandError::NoDaemons);
    }
    let host = vessel.host;
    let defender = match world.hosts[usize::from(host.0)].controller {
        Some(Controller::Crew(crew)) if crew != owner => crew,
        _ => return Err(CommandError::NothingToRaid),
    };
    if world.turn < PROTECTION_TURNS {
        return Err(CommandError::Protected);
    }
    let attacker = Side {
        daemons: vessel.daemons,
        level: vessel.pilot.as_ref().map_or(0, |p| p.level()),
    };
    let day = world.day;

    let defence = garrison(&world.hosts[usize::from(host.0)].site.citadel);
    let report = battle::fight(&mut world.rng, attacker, defence, 0);
    settle_garrison(
        &mut world.hosts[usize::from(host.0)].site.citadel,
        defence.daemons,
        report.defender_left(),
    );
    legacy::settle_vessel(world, id, &report, events);
    if let Some(player) = world.players.get_mut(&owner) {
        player.heat += RAID_HEAT;
    }

    let mut loot = Vec::new();
    if report.outcome == Outcome::AttackerWon {
        match goal {
            RaidGoal::Exfiltrate => loot = exfiltrate(data, world, id, host),
            RaidGoal::PlantTap => plant_tap(world, owner, host),
            RaidGoal::TakeOver => take_over(world, owner, defender, host, events),
        }
    }
    events.push(Event::Raid {
        day,
        player: owner,
        defender,
        host,
        vessel: id,
        goal,
        report,
        loot,
    });
    Ok(())
}

/// Fills the vessel's data containers from the host's citadel store, then
/// its inside store: the biggest stocks first, one resource per container.
fn exfiltrate(
    data: &GameData,
    world: &mut World,
    id: VesselId,
    host: HostId,
) -> Vec<(ItemType, u32)> {
    let mut loot = Vec::new();
    let Some(vessel) = world.vessels.get_mut(&id) else {
        return loot;
    };
    let site = &mut world.hosts[usize::from(host.0)].site;
    for module in &mut vessel.modules {
        let Module::DataContainer(held) = module else {
            continue;
        };
        let room = Module::CONTAINER_CAPACITY - held.map_or(0, |c| c.count);
        if room == 0 {
            continue;
        }
        let wanted = match held {
            Some(cargo) => Some(cargo.item),
            None => richest(data, &site.citadel.store).or_else(|| richest(data, &site.store)),
        };
        let Some(item) = wanted else {
            break;
        };
        let mut taken = 0;
        for store in [&mut site.citadel.store, &mut site.store] {
            let count = store.get(item).min(room - taken);
            if count > 0 && store.take(item, count) {
                taken += count;
            }
        }
        if taken == 0 {
            continue;
        }
        *held = Some(Cargo {
            item,
            count: held.map_or(0, |c| c.count) + taken,
        });
        loot.push((item, taken));
    }
    loot
}

/// The resource a store holds most of.
fn richest(data: &GameData, store: &Store) -> Option<ItemType> {
    store
        .iter()
        .filter(|(item, count)| *count > 0 && data.items[item].category == ItemCategory::Resource)
        .max_by_key(|&(item, count)| (count, std::cmp::Reverse(item)))
        .map(|(item, _)| item)
}

/// Plants the raider's tap on the host, or renews one it has there.
fn plant_tap(world: &mut World, owner: PlayerId, host: HostId) {
    let until = world.day + SIPHON_DAYS;
    let site = &mut world.hosts[usize::from(host.0)].site;
    match site.siphons.iter_mut().find(|s| s.player == owner) {
        Some(siphon) => siphon.until = until,
        None => site.siphons.push(Siphon {
            player: owner,
            until,
        }),
    }
}

/// The host changes hands. The loser's teams there are gone, its vessels
/// are thrown out onto the net, and the citadel's link and taps planted by
/// others are reset.
fn take_over(
    world: &mut World,
    owner: PlayerId,
    from: PlayerId,
    host: HostId,
    events: &mut Vec<Event>,
) {
    let day = world.day;
    let state = &mut world.hosts[usize::from(host.0)];
    state.controller = Some(Controller::Crew(owner));
    state.attacked = 0;
    let site = &mut state.site;
    site.staff.clear();
    site.citadel.staff.clear();
    site.citadel.workshop.coders = None;
    site.citadel.workshop.jobs.clear();
    site.citadel.link = LinkConfig::default();
    site.siphons.clear();
    for vessel in world.vessels.values_mut() {
        if vessel.owner == from
            && vessel.host == host
            && matches!(
                vessel.state,
                VesselState::At(Berth::Planted | Berth::Connected) | VesselState::Connecting
            )
        {
            vessel.state = VesselState::At(Berth::Lurking);
            vessel.destination = None;
            if let Some(script) = &mut vessel.script {
                script.route = None;
            }
        }
    }
    if let Some(player) = world.players.get_mut(&owner) {
        player.taken += 1;
    }
    events.push(Event::HostTaken {
        day,
        player: owner,
        from,
        host,
    });
}
