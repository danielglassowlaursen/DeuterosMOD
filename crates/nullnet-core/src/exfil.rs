//! Exfil scripts: automatic cargo runs. Port of the ACC
//! (Godot/Code/Objects/ACC.cs).
//!
//! A vessel with an installed script and at least one data container can be
//! given a route between two bays. At each end it unloads its containers,
//! tops up its anonymisation, loads the resources chosen for the other end
//! in turn, and sets off again. A dropper runs between a host's inside and
//! its citadel; worms and tunnelers run between two citadels.

use serde::{Deserialize, Serialize};

use crate::command::CommandError;
use crate::data::{GameData, ItemCategory};
use crate::ids::PlayerId;
use crate::items::ItemType;
use crate::store::Store;
use crate::transport::{self, Berth, Destination, Module, VesselId, VesselKind};
use crate::turn::Event;
use crate::world::World;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExfilScript {
    pub route: Option<Route>,
    /// Where the round-robin over each list continues from.
    pub outbound_next: usize,
    pub inbound_next: usize,
}

impl ExfilScript {
    pub fn running(&self) -> bool {
        self.route.is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Route {
    pub from: Destination,
    pub to: Destination,
    /// Resources carried from `from` to `to`.
    pub outbound: Vec<ItemType>,
    /// Resources carried back.
    pub inbound: Vec<ItemType>,
}

/// Anonymisation levels for a script's refuelling: below the first it
/// refuels, filling up to the second if the store allows.
fn refuel_policy(kind: VesselKind) -> (u32, u32) {
    match kind {
        VesselKind::Dropper => (50, 100),
        VesselKind::Worm | VesselKind::Tunneler => (200, 250),
    }
}

/// Installs an exfil script from the bay's store into a docked vessel.
pub(crate) fn install(
    data: &GameData,
    world: &mut World,
    owner: PlayerId,
    id: VesselId,
) -> Result<(), CommandError> {
    let (vessel, store, _) = transport::docked(data, world, owner, id)?;
    if vessel.script.is_some() {
        return Err(CommandError::AlreadyComplete);
    }
    if !store.take(ItemType::ExfilScript, 1) {
        return Err(CommandError::MissingResources(ItemType::ExfilScript));
    }
    vessel.script = Some(ExfilScript::default());
    Ok(())
}

/// Starts, changes or (with `None`) stops a vessel's script. A vessel
/// resting at either end starts at once; elsewhere it heads for `from`.
pub(crate) fn configure(
    data: &GameData,
    world: &mut World,
    owner: PlayerId,
    id: VesselId,
    route: Option<Route>,
    events: &mut Vec<Event>,
) -> Result<(), CommandError> {
    let vessel = world
        .vessels
        .get(&id)
        .filter(|v| v.owner == owner)
        .ok_or(CommandError::UnknownVessel(id))?;
    if vessel.script.is_none() {
        return Err(CommandError::NoScript);
    }
    if let Some(route) = &route {
        validate(data, world, owner, vessel.kind, vessel.host, route)?;
        if !vessel
            .modules
            .iter()
            .any(|m| matches!(m, Module::DataContainer(_)))
        {
            return Err(CommandError::NoDataContainer);
        }
    }

    let vessel = world.vessels.get_mut(&id).expect("checked above");
    let script = vessel.script.as_mut().expect("checked above");
    *script = ExfilScript {
        route: route.clone(),
        ..ExfilScript::default()
    };
    let Some(route) = route else {
        return Ok(());
    };
    run(data, world, id);
    let vessel = world.vessels.get_mut(&id).expect("checked above");
    if vessel.destination.is_none() && !at_end(vessel, &route) {
        vessel.destination = Some(route.from);
    }
    let day = world.day;
    transport::start_step(data, world, id, day, events);
    Ok(())
}

fn validate(
    data: &GameData,
    world: &World,
    owner: PlayerId,
    kind: VesselKind,
    host: crate::ids::HostId,
    route: &Route,
) -> Result<(), CommandError> {
    for end in [route.from, route.to] {
        if end.berth == Berth::Lurking
            || (end.berth == Berth::Planted && kind != VesselKind::Dropper)
        {
            return Err(CommandError::WrongBerth);
        }
        if !transport::owns(data, world, owner, end.host) {
            return Err(CommandError::NotYourHost(end.host));
        }
    }
    if route.from == route.to {
        return Err(CommandError::WrongBerth);
    }
    let reachable = |a: crate::ids::HostId, b: crate::ids::HostId| match kind {
        VesselKind::Dropper => a == host && b == host,
        VesselKind::Worm => data.host(a).network == data.host(b).network,
        VesselKind::Tunneler => true,
    };
    if !reachable(route.from.host, route.to.host) {
        return Err(CommandError::OutOfRange(route.to.host));
    }
    if let Some(&item) = route
        .outbound
        .iter()
        .chain(&route.inbound)
        .find(|item| data.items[item].category != ItemCategory::Resource)
    {
        return Err(CommandError::WrongModule(item));
    }
    Ok(())
}

fn at_end(vessel: &transport::Vessel, route: &Route) -> bool {
    [route.from, route.to]
        .iter()
        .any(|end| vessel.host == end.host && vessel.berth() == Some(end.berth))
}

/// The turnaround at either end of a running route: unload, refuel, load
/// for the other end and set off. A vessel short of anonymisation waits at
/// the end and tries again each day.
pub(crate) fn run(data: &GameData, world: &mut World, id: VesselId) {
    let vessel = &world.vessels[&id];
    let Some(route) = vessel.script.as_ref().and_then(|s| s.route.clone()) else {
        return;
    };
    if vessel.destination.is_some() || !at_end(vessel, &route) {
        return;
    }
    let outbound = vessel.host == route.from.host && vessel.berth() == Some(route.from.berth);
    let owner = vessel.owner;
    let Ok((vessel, store, _)) = transport::docked(data, world, owner, id) else {
        return;
    };

    for module in &mut vessel.modules {
        if let Module::DataContainer(held) = module
            && let Some(cargo) = held
        {
            cargo.count -= store.add(cargo.item, cargo.count);
            if cargo.count == 0 {
                *held = None;
            }
        }
    }

    let (refuel_below, fill_to) = refuel_policy(vessel.kind);
    if vessel.fuel < refuel_below {
        let fuel = vessel.kind.fuel();
        let available = store.get(fuel);
        let take = if available >= fill_to - vessel.fuel {
            fill_to - vessel.fuel
        } else if available >= refuel_below - vessel.fuel {
            available
        } else {
            return;
        };
        store.take(fuel, take);
        vessel.fuel += take;
    }

    let script = vessel.script.as_mut().expect("route came from it");
    let (items, next) = if outbound {
        (&route.outbound, &mut script.outbound_next)
    } else {
        (&route.inbound, &mut script.inbound_next)
    };
    for module in &mut vessel.modules {
        if let Module::DataContainer(held @ None) = module {
            *held = load_next(store, items, next);
        }
    }
    vessel.destination = Some(if outbound { route.to } else { route.from });
}

/// Loads the next listed resource that the store holds, round-robin, as
/// the original's cursor over item types did.
fn load_next(store: &mut Store, items: &[ItemType], next: &mut usize) -> Option<transport::Cargo> {
    for offset in 0..items.len() {
        let index = (*next + offset) % items.len();
        let item = items[index];
        let count = store.get(item).min(Module::CONTAINER_CAPACITY);
        if count > 0 {
            store.take(item, count);
            *next = index + 1;
            return Some(transport::Cargo { item, count });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::ids::HostId;
    use crate::staff::{Staff, StaffKind};
    use crate::transport::{ModuleKind, Seat, VesselState};
    use crate::turn::{Orders, TurnReport, resolve_turn};

    const CREW: PlayerId = PlayerId(0);

    fn orders(data: &GameData, world: &mut World, commands: Vec<Command>) -> TurnReport {
        resolve_turn(data, world, &Orders::from([(CREW, commands)]), 0)
    }

    /// A crew whose hideout has a citadel and a dropper planted inside it
    /// with a pilot, a data container and an exfil script.
    fn setup() -> (GameData, World, VesselId) {
        let data = GameData::classic();
        let mut world = World::new_game(&data, 4, &[(CREW, "Crew")]);
        let hideout = &mut world.players.get_mut(&CREW).unwrap().hideout;
        // No mining, so only the script moves resources.
        hideout.taps = 0;
        hideout.citadel.modules = 8;
        hideout
            .staff
            .push(Staff::new("Runner", StaffKind::Operator, 5));
        for (item, count) in [
            (ItemType::DropperCore, 1),
            (ItemType::DropperEngine, 1),
            (ItemType::DataContainer, 1),
            (ItemType::ExfilScript, 1),
            (ItemType::ProxyChains, 300),
            (ItemType::Compute, 600),
            (ItemType::Storage, 100),
        ] {
            hideout.store.add(item, count);
        }
        hideout.citadel.store.add(ItemType::Code, 300);

        let home = data.hideout.host;
        let dropper = VesselId(0);
        let report = orders(
            &data,
            &mut world,
            vec![
                Command::Assemble {
                    host: home,
                    berth: Berth::Planted,
                    kind: VesselKind::Dropper,
                },
                Command::Board {
                    vessel: dropper,
                    seat: Seat::Pilot,
                    team: 0,
                },
                Command::Fit {
                    vessel: dropper,
                    slot: 0,
                    module: Some(ModuleKind::DataContainer),
                },
                Command::InstallScript { vessel: dropper },
            ],
        );
        assert!(report.rejected.is_empty(), "{:?}", report.rejected);
        (data, world, dropper)
    }

    fn up_and_down(data: &GameData) -> Route {
        let home = data.hideout.host;
        Route {
            from: Destination {
                host: home,
                berth: Berth::Planted,
            },
            to: Destination {
                host: home,
                berth: Berth::Connected,
            },
            outbound: vec![ItemType::Compute, ItemType::Storage],
            inbound: vec![ItemType::Code],
        }
    }

    #[test]
    fn a_dropper_shuttles_resources_between_hideout_and_citadel() {
        let (data, mut world, dropper) = setup();
        let route = up_and_down(&data);
        let report = orders(
            &data,
            &mut world,
            vec![Command::ConfigureScript {
                vessel: dropper,
                route: Some(route),
            }],
        );
        assert!(report.rejected.is_empty(), "{:?}", report.rejected);
        // It refuels to 100 from the hideout, loads 250 compute and leaves.
        let vessel = &world.vessels[&dropper];
        assert_eq!(vessel.fuel, 100);
        assert!(matches!(vessel.state, VesselState::Exfiltrating { .. }));

        // 5 days out, 1 to connect, 1 to disconnect, 2 to get back in.
        let report = resolve_turn(&data, &mut world, &Orders::new(), 9);
        assert!(
            report.events.is_empty(),
            "scripted runs are quiet: {:?}",
            report.events
        );
        let hideout = &world.players[&CREW].hideout;
        assert_eq!(hideout.citadel.store.get(ItemType::Compute), 250);
        assert_eq!(hideout.citadel.store.get(ItemType::Code), 50);
        assert_eq!(hideout.store.get(ItemType::Code), 250);
        // Back inside, it has already loaded the next outbound item in turn.
        assert_eq!(
            world.vessels[&dropper].modules[0],
            Module::DataContainer(Some(transport::Cargo {
                item: ItemType::Storage,
                count: 100,
            }))
        );
    }

    #[test]
    fn a_script_waits_at_the_end_until_it_can_refuel() {
        let (data, mut world, dropper) = setup();
        let hideout = &mut world.players.get_mut(&CREW).unwrap().hideout;
        hideout.store.take(ItemType::ProxyChains, 300);
        hideout.store.add(ItemType::ProxyChains, 10);
        let route = up_and_down(&data);
        orders(
            &data,
            &mut world,
            vec![Command::ConfigureScript {
                vessel: dropper,
                route: Some(route),
            }],
        );
        resolve_turn(&data, &mut world, &Orders::new(), 3);
        assert_eq!(
            world.vessels[&dropper].state,
            VesselState::At(Berth::Planted)
        );
        assert_eq!(world.vessels[&dropper].fuel, 0);

        // 40 more is enough to reach the 50 threshold, so it takes all 50
        // and leaves the same day; the first unit burns the next day.
        let hideout = &mut world.players.get_mut(&CREW).unwrap().hideout;
        hideout.store.add(ItemType::ProxyChains, 40);
        resolve_turn(&data, &mut world, &Orders::new(), 1);
        assert_eq!(world.vessels[&dropper].fuel, 50);
        assert!(matches!(
            world.vessels[&dropper].state,
            VesselState::Exfiltrating { .. }
        ));
    }

    #[test]
    fn routes_must_fit_the_vessel() {
        let (data, mut world, dropper) = setup();
        let transit = HostId(data.hosts.iter().position(|h| h.name == "Transit").unwrap() as u16);
        let with = |change: &dyn Fn(&mut Route)| {
            let mut route = up_and_down(&data);
            change(&mut route);
            Command::ConfigureScript {
                vessel: dropper,
                route: Some(route),
            }
        };
        let report = orders(
            &data,
            &mut world,
            vec![
                with(&|r| r.to.berth = Berth::Lurking),
                with(&|r| r.to = r.from),
                with(&|r| r.outbound.push(ItemType::Tap)),
                with(&|r| r.to.host = transit),
            ],
        );
        let errors: Vec<_> = report.rejected.iter().map(|r| r.error.clone()).collect();
        assert_eq!(
            errors,
            [
                CommandError::WrongBerth,
                CommandError::WrongBerth,
                CommandError::WrongModule(ItemType::Tap),
                CommandError::NotYourHost(transit),
            ]
        );

        world.vessels.get_mut(&dropper).unwrap().modules[0] = Module::Empty;
        world.vessels.get_mut(&dropper).unwrap().script = None;
        let report = orders(&data, &mut world, vec![with(&|_| {})]);
        assert_eq!(report.rejected[0].error, CommandError::NoScript);
        world.vessels.get_mut(&dropper).unwrap().script = Some(ExfilScript::default());
        let report = orders(&data, &mut world, vec![with(&|_| {})]);
        assert_eq!(report.rejected[0].error, CommandError::NoDataContainer);
    }

    #[test]
    fn stopping_a_script_leaves_the_vessel_where_it_is() {
        let (data, mut world, dropper) = setup();
        let route = up_and_down(&data);
        orders(
            &data,
            &mut world,
            vec![Command::ConfigureScript {
                vessel: dropper,
                route: Some(route),
            }],
        );
        orders(
            &data,
            &mut world,
            vec![Command::ConfigureScript {
                vessel: dropper,
                route: None,
            }],
        );
        // The current leg finishes; then it stays.
        resolve_turn(&data, &mut world, &Orders::new(), 20);
        assert!(!world.vessels[&dropper].script.as_ref().unwrap().running());
        assert_eq!(
            world.vessels[&dropper].state,
            VesselState::At(Berth::Connected)
        );
    }
}
