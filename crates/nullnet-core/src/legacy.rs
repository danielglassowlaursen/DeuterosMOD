//! The Legacy Net: what is left of the AI that took the first internet
//! down. Port of the Methanoids in Godot/Code/Platform/EnemyDroneBuilder.cs
//! and Godot/Code/Objects/EnemyFleets.cs, with the battles of
//! Godot/Code/Platform/Screens/ShipInterior.cs.
//!
//! It holds hosts with complete citadels and garrisons them with daemons.
//! Once any crew is at war with it, it builds daemons on a schedule and
//! gathers a swarm in each network; when a swarm is big enough it picks a
//! citadel of a crew at war, routes there, lays siege for five days and
//! takes it unless a crew's vessel with a C2 controller and daemons drives
//! it off. Crews go to war by holding six citadels, or by attacking first.
//! A crew that destroys a host's garrison frees the host and takes it over
//! with everything the Legacy Net left in it.

use serde::{Deserialize, Serialize};

use crate::battle::{self, Outcome, Side};
use crate::command::CommandError;
use crate::data::GameData;
use crate::ids::{Day, HostId, NetworkId, PlayerId};
use crate::items::ItemType;
use crate::site::Citadel;
use crate::store::Store;
use crate::transport::{self, Berth, VesselId, VesselState};
use crate::turn::Event;
use crate::workshop::Workshop;
use crate::world::{Controller, World};

/// Daemons a Legacy host starts with, and gets back when a host is taken.
pub const GARRISON: u32 = 50;
/// Most daemons a store, a swarm or a vessel holds.
pub const DAEMON_CAP: u32 = 200;
/// Days a swarm besieges a citadel before it falls.
pub const SIEGE_DAYS: u32 = 5;
/// Complete citadels, the hideout's included, that bring the Legacy Net
/// down on a crew.
pub const CITADELS_FOR_WAR: usize = 6;
/// Days between daemon builds, by how many networks the Legacy Net has
/// lost. The original's build frequencies divided by a hundred.
const BUILD_DAYS: [u32; 9] = [7, 10, 9, 9, 9, 8, 7, 7, 8];
const TRIGGER_CAP: u32 = 200;
/// Most days a swarm dawdles before setting off.
const DAWDLE_DAYS: u32 = 63;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Legacy {
    /// One swarm per network, indexed by [`NetworkId`].
    pub fleets: Vec<Fleet>,
    /// The next day daemons are built.
    pub build_day: Day,
    /// The network whose swarm looks for a target next.
    pub next_check: usize,
}

/// A network's swarm of daemons.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fleet {
    /// The Legacy host it gathers at; `None` where the network has none.
    pub host: Option<HostId>,
    pub daemons: u32,
    /// Daemons it needs before it attacks.
    pub attack_trigger: u32,
    pub target: Option<HostId>,
    /// The day it reaches its target.
    pub arrives: Option<Day>,
    /// Attacks left on the current target before it picks another.
    pub attack_count: u32,
    /// The day the besieged citadel falls.
    pub siege_until: Option<Day>,
}

impl Fleet {
    /// The host the swarm is besieging, if any.
    pub fn besieging(&self) -> Option<HostId> {
        self.siege_until.and(self.target)
    }
}

/// Gives every network its swarm and every Legacy host its garrison.
pub(crate) fn setup(data: &GameData, world: &mut World) {
    world.legacy.fleets = data
        .networks
        .iter()
        .enumerate()
        .map(|(index, network)| Fleet {
            host: data
                .hosts
                .iter()
                .enumerate()
                .find(|(h, def)| {
                    def.network == NetworkId(index as u8)
                        && world.hosts[*h].controller == Some(Controller::Legacy)
                })
                .map(|(h, _)| HostId(h as u16)),
            attack_trigger: network.legacy_attack_trigger,
            ..Fleet::default()
        })
        .collect();
    for host in &mut world.hosts {
        if host.controller == Some(Controller::Legacy) {
            host.site.citadel.store.add(ItemType::Daemon, GARRISON);
        }
    }
}

pub fn at_war(world: &World) -> bool {
    world.players.values().any(|p| p.war.is_some())
}

/// Whether a swarm is besieging the host.
pub fn under_siege(world: &World, host: HostId) -> bool {
    world
        .legacy
        .fleets
        .iter()
        .any(|f| f.besieging() == Some(host))
}

fn network_of(data: &GameData, host: HostId) -> usize {
    usize::from(data.host(host).network.0)
}

/// Networks with no Legacy host left.
fn networks_freed(data: &GameData, world: &World) -> usize {
    (0..data.networks.len())
        .filter(|&n| {
            !data.hosts.iter().zip(&world.hosts).any(|(def, state)| {
                usize::from(def.network.0) == n && state.controller == Some(Controller::Legacy)
            })
        })
        .count()
}

/// Puts a crew at war with the Legacy Net.
pub(crate) fn declare_war(world: &mut World, player: PlayerId, events: &mut Vec<Event>) {
    let day = world.day;
    if let Some(state) = world.players.get_mut(&player)
        && state.war.is_none()
    {
        state.war = Some(day);
        events.push(Event::WarDeclared { day, player });
    }
}

/// Declares war on every crew that now holds enough citadels.
pub(crate) fn check_war(data: &GameData, world: &mut World) -> Vec<Event> {
    let mut events = Vec::new();
    let crews: Vec<PlayerId> = world.players.keys().copied().collect();
    for crew in crews {
        let player = &world.players[&crew];
        if player.war.is_some() {
            continue;
        }
        let held = world
            .hosts
            .iter()
            .enumerate()
            .filter(|(h, state)| {
                HostId(*h as u16) != data.hideout.host
                    && state.controller == Some(Controller::Crew(crew))
                    && state.site.citadel.complete()
            })
            .count()
            + usize::from(player.hideout.citadel.complete());
        if held >= CITADELS_FOR_WAR {
            declare_war(world, crew, &mut events);
        }
    }
    events
}

/// One day of the Legacy Net: building daemons, moving swarms and picking
/// targets. Nothing stirs until a crew is at war.
pub(crate) fn run_day(data: &GameData, world: &mut World, events: &mut Vec<Event>) {
    if !at_war(world) || world.legacy.fleets.is_empty() {
        return;
    }
    let day = world.day;

    if day >= world.legacy.build_day {
        let freed = networks_freed(data, world).min(BUILD_DAYS.len() - 1);
        world.legacy.build_day = day + BUILD_DAYS[freed];
        for index in 0..data.networks.len() {
            let held: Vec<usize> = (0..data.hosts.len())
                .filter(|&h| {
                    usize::from(data.hosts[h].network.0) == index
                        && world.hosts[h].controller == Some(Controller::Legacy)
                })
                .collect();
            let mut remaining = held.len() as u32 * 2;
            for h in held {
                let store = &mut world.hosts[h].site.citadel.store;
                if store.get(ItemType::Daemon) < DAEMON_CAP {
                    store.add(ItemType::Daemon, 2);
                    remaining -= 2;
                }
            }
            let gain = if remaining == 0 { 1 } else { 2 };
            let fleet = &mut world.legacy.fleets[index];
            if fleet.host.is_some() {
                fleet.daemons = (fleet.daemons + gain).min(DAEMON_CAP);
            }
        }
    }

    for index in 0..world.legacy.fleets.len() {
        process_fleet(data, world, index, events);
    }

    let index = world.legacy.next_check % world.legacy.fleets.len();
    world.legacy.next_check = index + 1;
    check_trigger(data, world, index, events);
}

/// Moves a swarm on: arrival at its target, or the fall of the citadel it
/// besieges.
fn process_fleet(data: &GameData, world: &mut World, index: usize, events: &mut Vec<Event>) {
    let day = world.day;
    let fleet = &world.legacy.fleets[index];
    if let Some(until) = fleet.siege_until {
        if day >= until {
            capture(data, world, index, events);
        }
        return;
    }
    let (Some(target), Some(arrives)) = (fleet.target, fleet.arrives) else {
        return;
    };
    if day < arrives {
        return;
    }

    let fleet = &mut world.legacy.fleets[index];
    fleet.host = Some(target);
    fleet.arrives = None;
    let state = &mut world.hosts[usize::from(target.0)];
    state.attacked += 1;
    let crew = match state.controller {
        Some(Controller::Crew(crew)) if state.site.citadel.complete() => crew,
        _ => {
            cancel_attack(world, index);
            return;
        }
    };

    // A vessel of the holder's with daemons under command meets the swarm.
    if let Some(defender) = best_defender(world, target, crew) {
        let repelled = fleet_battle(world, index, defender, events);
        if repelled {
            return;
        }
    }
    world.legacy.fleets[index].siege_until = Some(day + SIEGE_DAYS);
    events.push(Event::UnderAttack {
        day,
        player: crew,
        host: target,
        captured_on: day + SIEGE_DAYS,
    });
}

/// The crew's vessel at the host best placed to fight: the most daemons
/// under a C2 controller with an operator aboard.
fn best_defender(world: &World, host: HostId, crew: PlayerId) -> Option<VesselId> {
    world
        .vessels
        .iter()
        .filter(|(_, v)| {
            v.owner == crew
                && v.host == host
                && v.c2
                && v.daemons > 0
                && v.pilot.is_some()
                && matches!(v.state, VesselState::At(Berth::Connected | Berth::Lurking))
        })
        .max_by_key(|(id, v)| (v.daemons, std::cmp::Reverse(**id)))
        .map(|(&id, _)| id)
}

/// A crew's vessel fights the swarm. Returns whether the swarm was driven
/// off. The swarm pulls out once it has lost half its daemons.
fn fleet_battle(
    world: &mut World,
    index: usize,
    vessel_id: VesselId,
    events: &mut Vec<Event>,
) -> bool {
    let day = world.day;
    let fleet = &world.legacy.fleets[index];
    let vessel = &world.vessels[&vessel_id];
    let attacker = Side {
        daemons: vessel.daemons,
        level: vessel.pilot.as_ref().map_or(0, |p| p.level()),
    };
    let defender = Side {
        daemons: fleet.daemons,
        level: 0,
    };
    let flee_at = if fleet.daemons / 2 > 2 {
        fleet.daemons / 2
    } else {
        0
    };
    let report = battle::fight(&mut world.rng, attacker, defender, flee_at);
    let host = vessel.host;
    let owner = vessel.owner;

    world.legacy.fleets[index].daemons = report.defender_left();
    let repelled = report.outcome != Outcome::DefenderWon;
    settle_vessel(world, vessel_id, &report, events);
    events.push(Event::BattleFought {
        day,
        player: owner,
        host,
        vessel: vessel_id,
        report,
    });
    if repelled {
        cancel_attack(world, index);
        events.push(Event::AttackRepelled {
            day,
            player: owner,
            host,
        });
    }
    repelled
}

/// Applies a battle to the crew's vessel: its losses, its operator's
/// experience, and its end if nothing is left.
fn settle_vessel(
    world: &mut World,
    id: VesselId,
    report: &battle::Report,
    events: &mut Vec<Event>,
) {
    let day = world.day;
    let vessel = world.vessels.get_mut(&id).expect("fought");
    vessel.daemons = report.attacker_left();
    if let Some(pilot) = &mut vessel.pilot
        && let Some(level) = pilot.record_action()
    {
        events.push(Event::StaffPromoted {
            day,
            player: vessel.owner,
            kind: pilot.kind,
            level,
        });
    }
    if vessel.daemons == 0 {
        let vessel = world.vessels.remove(&id).expect("fought");
        events.push(Event::VesselLost {
            day,
            player: vessel.owner,
            vessel: id,
            host: vessel.host,
        });
    }
}

/// The swarm gives up on its target and grows warier.
fn cancel_attack(world: &mut World, index: usize) {
    let fleet = &mut world.legacy.fleets[index];
    fleet.target = None;
    fleet.arrives = None;
    fleet.siege_until = None;
    fleet.attack_trigger = (fleet.attack_trigger * 2).min(TRIGGER_CAP);
}

/// The besieged citadel falls to the Legacy Net.
fn capture(data: &GameData, world: &mut World, index: usize, events: &mut Vec<Event>) {
    let day = world.day;
    let fleet = &mut world.legacy.fleets[index];
    let Some(host) = fleet.target else {
        return;
    };
    fleet.siege_until = None;
    fleet.target = None;
    fleet.attack_count = 0;
    fleet.host = Some(host);

    let state = &mut world.hosts[usize::from(host.0)];
    let victim = match state.controller {
        Some(Controller::Crew(crew)) => crew,
        _ => return,
    };
    state.controller = Some(Controller::Legacy);
    state.attacked = 0;
    let site = &mut state.site;
    let mut store = Store::default();
    store.add(ItemType::Daemon, GARRISON);
    for &resource in &data.host(host).resources {
        store.add(resource, 100 + world.rng.below(1024));
    }
    site.citadel = Citadel {
        modules: Citadel::MODULES,
        encrypted_link: true,
        kill_switch: true,
        workshop: Workshop {
            automated: true,
            ..Workshop::default()
        },
        store,
        ..Citadel::default()
    };
    site.store = Store::default();
    site.staff.clear();
    site.taps = world.rng.below(8);
    site.backdoor_parts = crate::site::Site::BACKDOOR_PARTS;
    site.backdoor_damaged = false;

    let lost: Vec<VesselId> = world
        .vessels
        .iter()
        .filter(|(_, v)| v.host == host && !matches!(v.state, VesselState::Routing { .. }))
        .map(|(&id, _)| id)
        .collect();
    for id in lost {
        let vessel = world.vessels.remove(&id).expect("listed");
        events.push(Event::VesselLost {
            day,
            player: vessel.owner,
            vessel: id,
            host,
        });
    }
    events.push(Event::HostCaptured {
        day,
        player: victim,
        host,
    });
}

/// A swarm with enough daemons picks a citadel of a crew at war in its
/// network: the least attacked first, then the farthest out.
fn check_trigger(data: &GameData, world: &mut World, index: usize, events: &mut Vec<Event>) {
    let day = world.day;
    let fleet = &world.legacy.fleets[index];
    let Some(from) = fleet.host else {
        return;
    };
    if fleet.arrives.is_some()
        || fleet.siege_until.is_some()
        || fleet.daemons < fleet.attack_trigger
    {
        return;
    }
    let mut attack_count = fleet.attack_count.saturating_sub(1);
    let mut target = fleet.target;
    if attack_count == 0 {
        target = data
            .hosts
            .iter()
            .enumerate()
            .filter(|(h, def)| {
                let id = HostId(*h as u16);
                let state = &world.hosts[*h];
                usize::from(def.network.0) == index
                    && id != data.hideout.host
                    && state.site.citadel.complete()
                    && matches!(state.controller, Some(Controller::Crew(crew))
                        if world.players.get(&crew).is_some_and(|p| p.war.is_some()))
            })
            .min_by_key(|(h, _)| (world.hosts[*h].attacked, std::cmp::Reverse(*h)))
            .map(|(h, _)| HostId(h as u16));
    }
    let Some(target) = target else {
        world.legacy.fleets[index].attack_count = attack_count;
        return;
    };
    let roll = world.rng.below(3);
    attack_count = if roll == 0 { 1 } else { roll };
    let arrives = day + transport::latency(data, from, target) + world.rng.below(DAWDLE_DAYS) + 1;
    let fleet = &mut world.legacy.fleets[index];
    fleet.target = Some(target);
    fleet.attack_count = attack_count;
    fleet.arrives = Some(arrives);
    if let Some(Controller::Crew(crew)) = world.hosts[usize::from(target.0)].controller {
        events.push(Event::FleetSighted {
            day,
            player: crew,
            host: target,
            arrives,
            daemons: fleet.daemons,
        });
    }
}

/// A crew's vessel, lurking with daemons under a C2 controller, attacks
/// the Legacy Net at its host: the garrison of a Legacy host, or the swarm
/// besieging a host. Attacking is a declaration of war. A garrison
/// destroyed frees the host for the crew, with everything in it.
pub(crate) fn attack(
    data: &GameData,
    world: &mut World,
    owner: PlayerId,
    id: VesselId,
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
    let attacker = Side {
        daemons: vessel.daemons,
        level: vessel.pilot.as_ref().map_or(0, |p| p.level()),
    };
    let legacy_host = world.hosts[usize::from(host.0)].controller == Some(Controller::Legacy);
    let besieging = world
        .legacy
        .fleets
        .iter()
        .position(|f| f.besieging() == Some(host));
    if !legacy_host && besieging.is_none() {
        return Err(CommandError::NothingToAttack);
    }
    declare_war(world, owner, events);
    let day = world.day;

    if let Some(index) = besieging {
        fleet_battle(world, index, id, events);
        return Ok(());
    }

    let store = &mut world.hosts[usize::from(host.0)].site.citadel.store;
    let garrison = store.get(ItemType::Daemon).min(DAEMON_CAP);
    store.take(ItemType::Daemon, garrison);
    let report = battle::fight(
        &mut world.rng,
        attacker,
        Side {
            daemons: garrison,
            level: 0,
        },
        0,
    );
    let store = &mut world.hosts[usize::from(host.0)].site.citadel.store;
    store.add(ItemType::Daemon, report.defender_left());
    let freed = store.get(ItemType::Daemon) == 0;
    settle_vessel(world, id, &report, events);
    events.push(Event::BattleFought {
        day,
        player: owner,
        host,
        vessel: id,
        report,
    });
    if freed {
        let state = &mut world.hosts[usize::from(host.0)];
        state.controller = Some(Controller::Crew(owner));
        state.attacked = 0;
        // The network's swarm has lost its base here; it regroups at
        // another Legacy host in the network, or nowhere.
        let network = network_of(data, host);
        if let Some(fleet) = world.legacy.fleets.get_mut(network)
            && fleet.host == Some(host)
        {
            fleet.host = data
                .hosts
                .iter()
                .enumerate()
                .find(|(h, def)| {
                    usize::from(def.network.0) == network
                        && world.hosts[*h].controller == Some(Controller::Legacy)
                })
                .map(|(h, _)| HostId(h as u16));
        }
        events.push(Event::HostFreed {
            day,
            player: owner,
            host,
        });
    }
    Ok(())
}
