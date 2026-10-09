//! Vessels: droppers, worms and tunnelers. Port of the ship rules in
//! Godot/Code/Objects/Ship.cs, Shuttle.cs, InterStellarShip.cs,
//! Godot/Code/Platform/Screens/ShipBay.cs (assembly and outfitting) and
//! ShipInterior.cs (deployment and `UpdateShips`).
//!
//! A vessel is always at one host, in one of three resting positions:
//! planted inside the host through its backdoor (droppers only), connected
//! to the citadel above it, or lurking outside on the network. Moving
//! between them, or routing to another host, takes days and burns a unit of
//! anonymisation (proxy chains or onion routes) per day. A vessel left
//! lurking without anonymisation is traced and burned after five days.
//!
//! Crews give a vessel a destination and it works its way there on its own,
//! one step at a time, as the original's player did by hand.

use serde::{Deserialize, Serialize};

use crate::command::CommandError;
use crate::data::GameData;
use crate::exfil::{self, ExfilScript};
use crate::ids::{Day, HostId, PlayerId};
use crate::items::ItemType;
use crate::site::{STAFF_SLOTS, Site};
use crate::staff::{Staff, StaffKind};
use crate::store::Store;
use crate::turn::Event;
use crate::world::{Controller, Player, World};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct VesselId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VesselKind {
    /// Moves between a host's inside, its citadel and the network outside
    /// it, but never to another host. A shuttle in Deuteros.
    Dropper,
    /// Routes between hosts within one network. An IOS in Deuteros.
    Worm,
    /// Routes between networks. An SCG in Deuteros.
    Tunneler,
}

impl VesselKind {
    pub fn modules(self) -> usize {
        match self {
            VesselKind::Dropper => 1,
            VesselKind::Worm => 3,
            VesselKind::Tunneler => 5,
        }
    }

    pub fn core(self) -> ItemType {
        match self {
            VesselKind::Dropper => ItemType::DropperCore,
            VesselKind::Worm => ItemType::WormCore,
            VesselKind::Tunneler => ItemType::TunnelCore,
        }
    }

    pub fn engine(self) -> ItemType {
        match self {
            VesselKind::Dropper => ItemType::DropperEngine,
            VesselKind::Worm => ItemType::WormEngine,
            VesselKind::Tunneler => ItemType::TunnelEngine,
        }
    }

    /// The anonymisation the vessel burns while it moves.
    pub fn fuel(self) -> ItemType {
        match self {
            VesselKind::Dropper | VesselKind::Worm => ItemType::ProxyChains,
            VesselKind::Tunneler => ItemType::OnionRoutes,
        }
    }
}

/// What fills one of a vessel's module slots.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Module {
    Empty,
    /// Up to [`Module::CONTAINER_CAPACITY`] units of one resource.
    DataContainer(Option<Cargo>),
    /// Equipment: citadel modules, backdoor kits, taps, sniffers and so on.
    ToolModule(Option<Cargo>),
    /// A team in transit.
    SessionPod(Option<Staff>),
}

impl Module {
    pub const CONTAINER_CAPACITY: u32 = 250;

    /// The pod item a module slot is fitted with.
    pub fn pod(&self) -> Option<ItemType> {
        match self {
            Module::Empty => None,
            Module::DataContainer(_) => Some(ItemType::DataContainer),
            Module::ToolModule(_) => Some(ItemType::ToolModule),
            Module::SessionPod(_) => Some(ItemType::SessionPod),
        }
    }

    fn is_empty(&self) -> bool {
        matches!(
            self,
            Module::Empty
                | Module::DataContainer(None)
                | Module::ToolModule(None)
                | Module::SessionPod(None)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cargo {
    pub item: ItemType,
    pub count: u32,
}

/// Where a vessel rests at its host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Berth {
    /// Inside the host through its backdoor. Droppers only.
    Planted,
    /// Connected to the citadel above the host.
    Connected,
    /// Outside the host, on the network.
    Lurking,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VesselState {
    At(Berth),
    /// Leaving the inside of a host for the network: 5 days.
    Exfiltrating {
        until: Day,
    },
    /// Getting inside a host: 2 days.
    Injecting {
        until: Day,
    },
    /// Leaving the citadel: 1 day.
    Disconnecting {
        until: Day,
    },
    /// Connecting to the citadel: the next day, once a port is free.
    Connecting,
    Routing {
        to: HostId,
        until: Day,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Destination {
    pub host: HostId,
    pub berth: Berth,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vessel {
    pub owner: PlayerId,
    pub kind: VesselKind,
    /// The host the vessel is at, or left from while routing.
    pub host: HostId,
    pub state: VesselState,
    /// Units of anonymisation aboard, up to [`Vessel::FUEL_CAPACITY`].
    pub fuel: u32,
    /// The operator running the vessel.
    pub pilot: Option<Staff>,
    pub modules: Vec<Module>,
    pub destination: Option<Destination>,
    /// Days spent lurking with no anonymisation left.
    pub exposed_days: u32,
    /// An installed exfil script that can run the vessel on a cargo route.
    pub script: Option<ExfilScript>,
    /// Daemons aboard, up to [`Vessel::DAEMON_CAPACITY`], for battles.
    pub daemons: u32,
    /// A C2 controller installed, so the vessel can command its daemons.
    pub c2: bool,
    /// What its scanner is on at a cache field.
    pub cache: Option<crate::caches::Cache>,
}

impl Vessel {
    pub const FUEL_CAPACITY: u32 = 250;
    pub const DAEMON_CAPACITY: u32 = 200;
    /// Days a vessel can lurk without anonymisation before it is burned.
    pub const EXPOSURE_LIMIT: u32 = 5;

    pub fn berth(&self) -> Option<Berth> {
        match self.state {
            VesselState::At(berth) => Some(berth),
            _ => None,
        }
    }
}

/// Days spent routing between two hosts.
///
/// Within a network this is the original's rule: between a host and its own
/// subsystems, the difference in their positions (at least 1); otherwise four
/// days per position between the two top-level hosts. Deuteros never
/// defined travel between stars, so between networks a tunneler routes to
/// the backbone and out again, plus [`NETWORK_HOP_DAYS`] per network crossed.
pub fn latency(data: &GameData, from: HostId, to: HostId) -> u32 {
    let (a, b) = (data.host(from), data.host(to));
    let top = |id: HostId| data.host(id).parent.unwrap_or(id);
    let (top_a, top_b) = (top(from), top(to));
    if a.network == b.network && top_a == top_b {
        a.order.abs_diff(b.order).max(1)
    } else if a.network == b.network {
        data.host(top_a).order.abs_diff(data.host(top_b).order) * 4
    } else {
        let (top_a, top_b) = (data.host(top_a), data.host(top_b));
        let crossed = u32::from(a.network.0.abs_diff(b.network.0));
        (top_a.order + 1) * 4 + (top_b.order + 1) * 4 + crossed * NETWORK_HOP_DAYS
    }
}

/// Days added per network a tunneler crosses. A placeholder to balance.
pub const NETWORK_HOP_DAYS: u32 = 20;

/// Where a vessel's resting position puts it: the store, staff slots and
/// citadel it can reach.
pub(crate) fn site_mut<'a>(
    data: &GameData,
    players: &'a mut std::collections::BTreeMap<PlayerId, Player>,
    hosts: &'a mut [crate::world::HostState],
    owner: PlayerId,
    host: HostId,
) -> Option<&'a mut Site> {
    if host == data.hideout.host {
        players.get_mut(&owner).map(|player| &mut player.hideout)
    } else {
        hosts
            .get_mut(usize::from(host.0))
            .map(|state| &mut state.site)
    }
}

/// The store and staff slots a vessel at rest can reach: the host's inside
/// when planted, the citadel when connected.
pub(crate) fn bay(site: &mut Site, berth: Berth) -> Option<(&mut Store, &mut Vec<Staff>)> {
    match berth {
        Berth::Planted => Some((&mut site.store, &mut site.staff)),
        Berth::Connected => Some((&mut site.citadel.store, &mut site.citadel.staff)),
        Berth::Lurking => None,
    }
}

/// Whether `owner` may use the host's inside or citadel: its own hideout,
/// a host it holds, or (to claim it) a free one.
pub(crate) fn may_use(
    data: &GameData,
    world_hosts: &[crate::world::HostState],
    owner: PlayerId,
    host: HostId,
) -> bool {
    host == data.hideout.host
        || match world_hosts[usize::from(host.0)].controller {
            None => !data.host(host).cache_field,
            Some(Controller::Crew(crew)) => crew == owner,
            Some(Controller::Legacy) => false,
        }
}

// ---------------------------------------------------------------- commands

/// Builds a vessel from its core and engine in a crew's bay. Droppers can
/// be built inside a site or in its citadel, one per host; worms and
/// tunnelers only in a citadel. Unlike the original, both parts are used up.
pub(crate) fn assemble(
    data: &GameData,
    world: &mut World,
    owner: PlayerId,
    host: HostId,
    berth: Berth,
    kind: VesselKind,
) -> Result<VesselId, CommandError> {
    if berth == Berth::Lurking || (berth == Berth::Planted && kind != VesselKind::Dropper) {
        return Err(CommandError::WrongBerth);
    }
    if !owns(data, world, owner, host) {
        return Err(CommandError::NotYourHost(host));
    }
    if kind == VesselKind::Dropper
        && world
            .vessels
            .values()
            .any(|v| v.owner == owner && v.host == host && v.kind == VesselKind::Dropper)
    {
        return Err(CommandError::DropperAlreadyHere);
    }
    let World { players, hosts, .. } = &mut *world;
    let site =
        site_mut(data, players, hosts, owner, host).ok_or(CommandError::UnknownHost(host))?;
    if berth == Berth::Connected && !site.citadel.complete() {
        return Err(CommandError::NoCitadel);
    }
    let (store, _) = bay(site, berth).expect("not lurking");
    for part in [kind.core(), kind.engine()] {
        if store.get(part) == 0 {
            return Err(CommandError::MissingResources(part));
        }
    }
    store.take(kind.core(), 1);
    store.take(kind.engine(), 1);

    let id = VesselId(world.next_vessel);
    world.next_vessel += 1;
    world.vessels.insert(
        id,
        Vessel {
            owner,
            kind,
            host,
            state: VesselState::At(berth),
            fuel: 0,
            pilot: None,
            modules: vec![Module::Empty; kind.modules()],
            destination: None,
            exposed_days: 0,
            script: None,
            daemons: 0,
            c2: false,
            cache: None,
        },
    );
    Ok(id)
}

/// Whether the crew holds the host, counting its own hideout.
pub(crate) fn owns(data: &GameData, world: &World, owner: PlayerId, host: HostId) -> bool {
    host == data.hideout.host
        || world
            .hosts
            .get(usize::from(host.0))
            .is_some_and(|h| h.controller == Some(Controller::Crew(owner)))
}

/// Everything a command on a docked vessel needs: the vessel and the store
/// and staff slots of the bay it rests in.
pub(crate) fn docked<'a>(
    data: &GameData,
    world: &'a mut World,
    owner: PlayerId,
    id: VesselId,
) -> Result<(&'a mut Vessel, &'a mut Store, &'a mut Vec<Staff>), CommandError> {
    let World {
        players,
        hosts,
        vessels,
        ..
    } = world;
    let vessel = vessels
        .get_mut(&id)
        .filter(|v| v.owner == owner)
        .ok_or(CommandError::UnknownVessel(id))?;
    let berth = vessel
        .berth()
        .filter(|&b| b != Berth::Lurking)
        .ok_or(CommandError::VesselNotDocked)?;
    if !may_use(data, hosts, owner, vessel.host) {
        return Err(CommandError::HostTaken(vessel.host));
    }
    let site = site_mut(data, players, hosts, owner, vessel.host)
        .ok_or(CommandError::UnknownHost(vessel.host))?;
    let (store, staff) = bay(site, berth).expect("not lurking");
    Ok((vessel, store, staff))
}

/// Tops up a docked vessel's anonymisation from the bay's store.
pub(crate) fn refuel(
    data: &GameData,
    world: &mut World,
    owner: PlayerId,
    id: VesselId,
    amount: u32,
) -> Result<(), CommandError> {
    let (vessel, store, _) = docked(data, world, owner, id)?;
    let amount = amount.min(Vessel::FUEL_CAPACITY - vessel.fuel);
    let fuel = vessel.kind.fuel();
    if !store.take(fuel, amount) {
        return Err(CommandError::MissingResources(fuel));
    }
    vessel.fuel += amount;
    Ok(())
}

/// The pod a module slot can be fitted with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModuleKind {
    DataContainer,
    ToolModule,
    SessionPod,
}

/// Fits an empty module slot with a pod from the bay's store, or removes
/// its pod (`None`), which goes back to the store.
pub(crate) fn fit(
    data: &GameData,
    world: &mut World,
    owner: PlayerId,
    id: VesselId,
    slot: usize,
    kind: Option<ModuleKind>,
) -> Result<(), CommandError> {
    let (vessel, store, _) = docked(data, world, owner, id)?;
    let module = vessel
        .modules
        .get_mut(slot)
        .ok_or(CommandError::NoSuchModule)?;
    if !module.is_empty() {
        return Err(CommandError::ModuleNotEmpty);
    }
    let new = match kind {
        None => Module::Empty,
        Some(ModuleKind::DataContainer) => Module::DataContainer(None),
        Some(ModuleKind::ToolModule) => Module::ToolModule(None),
        Some(ModuleKind::SessionPod) => Module::SessionPod(None),
    };
    if let Some(pod) = new.pod()
        && new.pod() != module.pod()
        && !store.take(pod, 1)
    {
        return Err(CommandError::MissingResources(pod));
    }
    if let Some(old) = module.pod()
        && new.pod() != Some(old)
    {
        store.add(old, 1);
    }
    *module = new;
    Ok(())
}

/// Loads cargo from the bay's store into a module: resources into a data
/// container, equipment into a tool module.
pub(crate) fn load(
    data: &GameData,
    world: &mut World,
    owner: PlayerId,
    id: VesselId,
    slot: usize,
    item: ItemType,
    count: u32,
) -> Result<(), CommandError> {
    let (vessel, store, _) = docked(data, world, owner, id)?;
    let module = vessel
        .modules
        .get_mut(slot)
        .ok_or(CommandError::NoSuchModule)?;
    let def = &data.items[&item];
    let (held, limit) = match module {
        Module::DataContainer(held) if def.category == crate::data::ItemCategory::Resource => {
            (held, Module::CONTAINER_CAPACITY)
        }
        Module::ToolModule(held) if def.tool_module => (
            held,
            if def.tool_module_single {
                1
            } else {
                Store::CAP
            },
        ),
        _ => return Err(CommandError::WrongModule(item)),
    };
    let already = match held {
        Some(cargo) if cargo.item != item => return Err(CommandError::ModuleNotEmpty),
        Some(cargo) => cargo.count,
        None => 0,
    };
    if already + count > limit {
        return Err(CommandError::ModuleFull);
    }
    if !store.take(item, count) {
        return Err(CommandError::MissingResources(item));
    }
    *held = Some(Cargo {
        item,
        count: already + count,
    });
    Ok(())
}

/// Empties a module's cargo into the bay's store. Whatever does not fit
/// under the store's cap stays aboard.
pub(crate) fn unload(
    data: &GameData,
    world: &mut World,
    owner: PlayerId,
    id: VesselId,
    slot: usize,
) -> Result<(), CommandError> {
    let (vessel, store, _) = docked(data, world, owner, id)?;
    let held = match vessel
        .modules
        .get_mut(slot)
        .ok_or(CommandError::NoSuchModule)?
    {
        Module::DataContainer(held) | Module::ToolModule(held) => held,
        _ => return Err(CommandError::WrongModule(ItemType::DataContainer)),
    };
    if let Some(cargo) = held {
        cargo.count -= store.add(cargo.item, cargo.count);
        if cargo.count == 0 {
            *held = None;
        }
    }
    Ok(())
}

/// A seat aboard a vessel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Seat {
    Pilot,
    /// A session pod in this module slot.
    Pod(usize),
}

/// Moves a team from the bay's staff slots into a seat. The pilot must be
/// an operator team.
pub(crate) fn board(
    data: &GameData,
    world: &mut World,
    owner: PlayerId,
    id: VesselId,
    seat: Seat,
    team: usize,
) -> Result<(), CommandError> {
    let (vessel, _, staff) = docked(data, world, owner, id)?;
    let candidate = staff.get(team).ok_or(CommandError::NoSuchTeam)?;
    let place = match seat {
        Seat::Pilot if candidate.kind != StaffKind::Operator => {
            return Err(CommandError::PilotMustBeOperator);
        }
        Seat::Pilot => &mut vessel.pilot,
        Seat::Pod(slot) => match vessel.modules.get_mut(slot) {
            Some(Module::SessionPod(place)) => place,
            Some(_) => return Err(CommandError::WrongModule(ItemType::SessionPod)),
            None => return Err(CommandError::NoSuchModule),
        },
    };
    if place.is_some() {
        return Err(CommandError::SeatTaken);
    }
    *place = Some(staff.remove(team));
    Ok(())
}

/// Moves a team from a seat to the bay's staff slots.
pub(crate) fn disembark(
    data: &GameData,
    world: &mut World,
    owner: PlayerId,
    id: VesselId,
    seat: Seat,
) -> Result<(), CommandError> {
    let (vessel, _, staff) = docked(data, world, owner, id)?;
    if staff.len() >= STAFF_SLOTS {
        return Err(CommandError::NoStaffSlot);
    }
    let place = match seat {
        Seat::Pilot => &mut vessel.pilot,
        Seat::Pod(slot) => match vessel.modules.get_mut(slot) {
            Some(Module::SessionPod(place)) => place,
            _ => return Err(CommandError::NoSuchModule),
        },
    };
    staff.push(place.take().ok_or(CommandError::NoSuchTeam)?);
    Ok(())
}

/// Sends a vessel to rest at a host. It sets off at once and keeps going
/// turn after turn until it gets there or a step becomes impossible.
pub(crate) fn dispatch(
    data: &GameData,
    world: &mut World,
    owner: PlayerId,
    id: VesselId,
    to: Destination,
    events: &mut Vec<Event>,
) -> Result<(), CommandError> {
    let vessel = world
        .vessels
        .get(&id)
        .filter(|v| v.owner == owner)
        .ok_or(CommandError::UnknownVessel(id))?;
    if usize::from(to.host.0) >= data.hosts.len() {
        return Err(CommandError::UnknownHost(to.host));
    }
    let reachable = match vessel.kind {
        VesselKind::Dropper => to.host == vessel.host,
        VesselKind::Worm => data.host(to.host).network == data.host(vessel.host).network,
        VesselKind::Tunneler => true,
    };
    if !reachable {
        return Err(CommandError::OutOfRange(to.host));
    }
    if to.berth == Berth::Planted && vessel.kind != VesselKind::Dropper {
        return Err(CommandError::WrongBerth);
    }
    if to.berth != Berth::Lurking && !may_use(data, &world.hosts, owner, to.host) {
        return Err(CommandError::HostTaken(to.host));
    }
    world
        .vessels
        .get_mut(&id)
        .expect("checked above")
        .destination = Some(to);
    // The first step starts the day the order is given, as a player's click
    // did in the original.
    let day = world.day;
    start_step(data, world, id, day, events);
    Ok(())
}

/// Installs a citadel module or backdoor kit from a tool module at the
/// vessel's host. Citadel modules go in from outside (lurking), backdoor
/// kits from inside (a planted dropper); both need the pilot. The first
/// module or kit on a free host claims it for the crew.
pub(crate) fn deploy(
    data: &GameData,
    world: &mut World,
    owner: PlayerId,
    id: VesselId,
    slot: usize,
    events: &mut Vec<Event>,
) -> Result<(), CommandError> {
    let day = world.day;
    let World {
        players,
        hosts,
        vessels,
        ..
    } = world;
    let vessel = vessels
        .get_mut(&id)
        .filter(|v| v.owner == owner)
        .ok_or(CommandError::UnknownVessel(id))?;
    let host = vessel.host;
    let cargo = match vessel.modules.get(slot) {
        Some(Module::ToolModule(Some(cargo))) => *cargo,
        Some(_) => return Err(CommandError::NothingToDeploy),
        None => return Err(CommandError::NoSuchModule),
    };
    let needed = match cargo.item {
        ItemType::CitadelModule => Berth::Lurking,
        ItemType::BackdoorKit => Berth::Planted,
        other => return Err(CommandError::NotDeployable(other)),
    };
    if vessel.berth() != Some(needed) {
        return Err(CommandError::WrongBerth);
    }
    if vessel.pilot.is_none() {
        return Err(CommandError::NoPilot);
    }
    if !may_use(data, hosts, owner, host) {
        return Err(CommandError::HostTaken(host));
    }
    let site =
        site_mut(data, players, hosts, owner, host).ok_or(CommandError::UnknownHost(host))?;
    let installed = match cargo.item {
        ItemType::CitadelModule if site.citadel.complete() => {
            return Err(CommandError::AlreadyComplete);
        }
        ItemType::CitadelModule => {
            site.citadel.modules += 1;
            site.citadel.modules
        }
        _ if site.backdoor_complete() => return Err(CommandError::AlreadyComplete),
        _ => {
            site.backdoor_parts += 1;
            site.backdoor_parts
        }
    };

    if host != data.hideout.host {
        let state = &mut hosts[usize::from(host.0)];
        if state.controller.is_none() {
            state.controller = Some(Controller::Crew(owner));
            events.push(Event::HostClaimed {
                day,
                player: owner,
                host,
            });
        }
    }
    events.push(Event::Installed {
        day,
        player: owner,
        host,
        item: cargo.item,
        installed,
    });
    vessel.modules[slot] = Module::ToolModule(None);
    if let Some(pilot) = &mut vessel.pilot
        && let Some(level) = pilot.record_action()
    {
        events.push(Event::StaffPromoted {
            day,
            player: owner,
            kind: pilot.kind,
            level,
        });
    }
    Ok(())
}

// ------------------------------------------------------------------- daily

/// Moves every vessel one day along its way. Port of `UpdateShips`.
pub(crate) fn run_day(data: &GameData, world: &mut World, events: &mut Vec<Event>) {
    let day = world.day;
    let ids: Vec<VesselId> = world.vessels.keys().copied().collect();
    let mut burned = Vec::new();
    for id in ids {
        burn_fuel(world.vessels.get_mut(&id).expect("listed"));
        finish_step(world, id, day, events);
        exfil::run(data, world, id);
        start_step(data, world, id, day, events);

        let vessel = &world.vessels[&id];
        if vessel.exposed_days >= Vessel::EXPOSURE_LIMIT {
            burned.push(id);
        }
    }
    for id in burned {
        let vessel = world.vessels.remove(&id).expect("listed");
        events.push(Event::VesselBurned {
            day,
            player: vessel.owner,
            vessel: id,
            host: vessel.host,
        });
    }
}

/// Moving costs a unit a day; lurking with none left counts toward being
/// traced. A move under way carries on even when the tank runs dry.
fn burn_fuel(vessel: &mut Vessel) {
    match vessel.state {
        VesselState::At(Berth::Lurking) if vessel.fuel == 0 => vessel.exposed_days += 1,
        VesselState::At(_) => vessel.exposed_days = 0,
        _ => {
            vessel.fuel = vessel.fuel.saturating_sub(1);
            vessel.exposed_days = 0;
        }
    }
}

fn finish_step(world: &mut World, id: VesselId, day: Day, events: &mut Vec<Event>) {
    let vessel = &world.vessels[&id];
    let next = match vessel.state {
        VesselState::Exfiltrating { until } | VesselState::Disconnecting { until }
            if day >= until =>
        {
            Some((vessel.host, Berth::Lurking))
        }
        VesselState::Injecting { until } if day >= until => Some((vessel.host, Berth::Planted)),
        VesselState::Routing { to, until } if day >= until => Some((to, Berth::Lurking)),
        // Worms and tunnelers share a citadel's single port; droppers have their own.
        VesselState::Connecting
            if vessel.kind == VesselKind::Dropper
                || !world.vessels.iter().any(|(&other, v)| {
                    other != id
                        && v.host == vessel.host
                        && v.owner == vessel.owner
                        && v.kind != VesselKind::Dropper
                        && v.state == VesselState::At(Berth::Connected)
                }) =>
        {
            Some((vessel.host, Berth::Connected))
        }
        _ => None,
    };
    let Some((host, berth)) = next else {
        return;
    };
    let vessel = world.vessels.get_mut(&id).expect("listed");
    vessel.host = host;
    vessel.state = VesselState::At(berth);
    if vessel.destination == Some(Destination { host, berth }) {
        vessel.destination = None;
        // A scripted run arrives at its ends every few days; only report
        // trips a crew ordered.
        if vessel.script.as_ref().is_some_and(ExfilScript::running) {
            return;
        }
        events.push(Event::VesselArrived {
            day,
            player: vessel.owner,
            vessel: id,
            host,
            berth,
        });
    }
}

/// Starts the next step toward the vessel's destination, if it is resting.
pub(crate) fn start_step(
    data: &GameData,
    world: &mut World,
    id: VesselId,
    day: Day,
    events: &mut Vec<Event>,
) {
    let vessel = &world.vessels[&id];
    let (Some(berth), Some(to)) = (vessel.berth(), vessel.destination) else {
        return;
    };
    if to.host == vessel.host && to.berth == berth {
        let vessel = world.vessels.get_mut(&id).expect("listed");
        vessel.destination = None;
        return;
    }

    // Leave the host's inside or citadel first; route only from outside.
    let step = match (berth, to.host == vessel.host, to.berth) {
        (Berth::Planted, _, _) => VesselState::Exfiltrating { until: day + 5 },
        (Berth::Connected, _, _) => VesselState::Disconnecting { until: day + 1 },
        (Berth::Lurking, false, _) => VesselState::Routing {
            to: to.host,
            until: day + latency(data, vessel.host, to.host),
        },
        (Berth::Lurking, true, Berth::Planted) => VesselState::Injecting { until: day + 2 },
        (Berth::Lurking, true, Berth::Connected) => {
            let ready = may_use(data, &world.hosts, vessel.owner, to.host)
                && citadel_complete(data, world, vessel.owner, to.host);
            if !ready {
                abort(world, id, day, AbortReason::NoCitadel, events);
                return;
            }
            if crate::legacy::under_siege(world, to.host) {
                abort(world, id, day, AbortReason::UnderAttack, events);
                return;
            }
            VesselState::Connecting
        }
        (Berth::Lurking, true, Berth::Lurking) => unreachable!("handled above"),
    };
    if vessel.fuel == 0 {
        abort(world, id, day, AbortReason::NoAnonymisation, events);
        return;
    }
    if vessel.pilot.is_none() {
        abort(world, id, day, AbortReason::NoPilot, events);
        return;
    }

    let vessel = world.vessels.get_mut(&id).expect("listed");
    vessel.state = step;
    // Pilots gain experience whenever they leave a host's inside or citadel
    // and whenever they route, as in the original's TakeOff and EngageEngine.
    if matches!(
        step,
        VesselState::Exfiltrating { .. }
            | VesselState::Disconnecting { .. }
            | VesselState::Routing { .. }
    ) && let Some(pilot) = &mut vessel.pilot
        && let Some(level) = pilot.record_action()
    {
        events.push(Event::StaffPromoted {
            day,
            player: vessel.owner,
            kind: pilot.kind,
            level,
        });
    }
}

fn citadel_complete(data: &GameData, world: &World, owner: PlayerId, host: HostId) -> bool {
    if host == data.hideout.host {
        world
            .players
            .get(&owner)
            .is_some_and(|p| p.hideout.citadel.complete())
    } else {
        world.hosts[usize::from(host.0)].site.citadel.complete()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbortReason {
    NoAnonymisation,
    NoPilot,
    NoCitadel,
    /// A Legacy swarm is besieging the citadel.
    UnderAttack,
}

/// Moves daemons between a docked vessel and its bay's store.
pub(crate) fn transfer_daemons(
    data: &GameData,
    world: &mut World,
    owner: PlayerId,
    id: VesselId,
    count: u32,
    aboard: bool,
) -> Result<(), CommandError> {
    let (vessel, store, _) = docked(data, world, owner, id)?;
    if aboard {
        if vessel.daemons + count > Vessel::DAEMON_CAPACITY {
            return Err(CommandError::TooManyDaemons);
        }
        if !store.take(ItemType::Daemon, count) {
            return Err(CommandError::MissingResources(ItemType::Daemon));
        }
        vessel.daemons += count;
    } else {
        if vessel.daemons < count {
            return Err(CommandError::NoDaemons);
        }
        vessel.daemons -= store.add(ItemType::Daemon, count);
    }
    Ok(())
}

/// Installs a C2 controller from the bay's store into a docked vessel.
pub(crate) fn install_c2(
    data: &GameData,
    world: &mut World,
    owner: PlayerId,
    id: VesselId,
) -> Result<(), CommandError> {
    let (vessel, store, _) = docked(data, world, owner, id)?;
    if vessel.c2 {
        return Err(CommandError::AlreadyComplete);
    }
    if !store.take(ItemType::C2Controller, 1) {
        return Err(CommandError::MissingResources(ItemType::C2Controller));
    }
    vessel.c2 = true;
    Ok(())
}

fn abort(world: &mut World, id: VesselId, day: Day, reason: AbortReason, events: &mut Vec<Event>) {
    let vessel = world.vessels.get_mut(&id).expect("listed");
    vessel.destination = None;
    events.push(Event::VesselStopped {
        day,
        player: vessel.owner,
        vessel: id,
        reason,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::turn::{Orders, TurnReport, resolve_turn};

    const CREW: PlayerId = PlayerId(0);
    const RIVAL: PlayerId = PlayerId(1);

    fn host(data: &GameData, name: &str) -> HostId {
        HostId(data.hosts.iter().position(|h| h.name == name).unwrap() as u16)
    }

    /// A new game where the crew's hideout already has a citadel, operator
    /// teams inside and in the citadel, and parts for a dropper and a worm.
    fn setup() -> (GameData, World) {
        let data = GameData::classic();
        let mut world = World::new_game(&data, 9, &[(CREW, "Crew"), (RIVAL, "Rival")]);
        let hideout = &mut world.players.get_mut(&CREW).unwrap().hideout;
        hideout.citadel.modules = 8;
        hideout
            .staff
            .push(Staff::new("Inside", StaffKind::Operator, 10));
        hideout
            .citadel
            .staff
            .push(Staff::new("Above", StaffKind::Operator, 10));
        for (item, count) in [
            (ItemType::DropperCore, 1),
            (ItemType::DropperEngine, 1),
            (ItemType::ProxyChains, 100),
            (ItemType::ToolModule, 2),
            (ItemType::CitadelModule, 2),
        ] {
            hideout.store.add(item, count);
        }
        for (item, count) in [
            (ItemType::WormCore, 2),
            (ItemType::WormEngine, 2),
            (ItemType::ProxyChains, 100),
            (ItemType::ToolModule, 2),
            (ItemType::DataContainer, 1),
            (ItemType::CitadelModule, 2),
            (ItemType::Compute, 400),
        ] {
            hideout.citadel.store.add(item, count);
        }
        (data, world)
    }

    fn orders(data: &GameData, world: &mut World, commands: Vec<Command>) -> TurnReport {
        let report = resolve_turn(data, world, &Orders::from([(CREW, commands)]), 0);
        assert!(report.rejected.is_empty(), "{:?}", report.rejected);
        report
    }

    fn days(data: &GameData, world: &mut World, days: u32) -> TurnReport {
        resolve_turn(data, world, &Orders::new(), days)
    }

    /// A worm in the hideout citadel with a pilot, fuel and a citadel module.
    fn ready_worm(data: &GameData, world: &mut World) -> VesselId {
        let home = data.hideout.host;
        let worm = VesselId(world.next_vessel);
        orders(
            data,
            world,
            vec![
                Command::Assemble {
                    host: home,
                    berth: Berth::Connected,
                    kind: VesselKind::Worm,
                },
                Command::Board {
                    vessel: worm,
                    seat: Seat::Pilot,
                    team: 0,
                },
                Command::Refuel {
                    vessel: worm,
                    amount: 50,
                },
                Command::Fit {
                    vessel: worm,
                    slot: 0,
                    module: Some(ModuleKind::ToolModule),
                },
                Command::Load {
                    vessel: worm,
                    slot: 0,
                    item: ItemType::CitadelModule,
                    count: 1,
                },
            ],
        );
        worm
    }

    #[test]
    fn latency_follows_the_original_within_a_network() {
        let data = GameData::classic();
        let exchange = host(&data, "Exchange");
        assert_eq!(latency(&data, exchange, host(&data, "Mirror")), 2);
        assert_eq!(latency(&data, exchange, host(&data, "Transit")), 4);
        assert_eq!(latency(&data, exchange, host(&data, "Colossus")), 12);
        assert_eq!(latency(&data, host(&data, "Transit"), exchange), 4);

        let elsewhere = data.hosts.iter().position(|h| h.network.0 == 1).unwrap();
        let elsewhere = HostId(elsewhere as u16);
        let order = data.host(elsewhere).order;
        assert_eq!(
            latency(&data, exchange, elsewhere),
            3 * 4 + (order + 1) * 4 + NETWORK_HOP_DAYS
        );
    }

    #[test]
    fn assembling_uses_both_parts_and_respects_the_bays() {
        let (data, mut world) = setup();
        let home = data.hideout.host;
        let assemble = |berth, kind| Command::Assemble {
            host: home,
            berth,
            kind,
        };
        orders(
            &data,
            &mut world,
            vec![assemble(Berth::Planted, VesselKind::Dropper)],
        );
        let hideout = &world.players[&CREW].hideout;
        assert_eq!(hideout.store.get(ItemType::DropperCore), 0);
        assert_eq!(hideout.store.get(ItemType::DropperEngine), 0);
        assert_eq!(
            world.vessels[&VesselId(0)].state,
            VesselState::At(Berth::Planted)
        );

        let report = resolve_turn(
            &data,
            &mut world,
            &Orders::from([(
                CREW,
                vec![
                    assemble(Berth::Connected, VesselKind::Dropper),
                    assemble(Berth::Planted, VesselKind::Worm),
                ],
            )]),
            0,
        );
        let errors: Vec<_> = report.rejected.iter().map(|r| r.error.clone()).collect();
        assert_eq!(
            errors,
            [CommandError::DropperAlreadyHere, CommandError::WrongBerth]
        );
    }

    #[test]
    fn a_dropper_leaves_the_hideout_in_five_days_and_installs_a_citadel_module() {
        let (data, mut world) = setup();
        let home = data.hideout.host;
        world
            .players
            .get_mut(&CREW)
            .unwrap()
            .hideout
            .citadel
            .modules = 0;
        let dropper = VesselId(0);
        orders(
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
                Command::Refuel {
                    vessel: dropper,
                    amount: 20,
                },
                Command::Fit {
                    vessel: dropper,
                    slot: 0,
                    module: Some(ModuleKind::ToolModule),
                },
                Command::Load {
                    vessel: dropper,
                    slot: 0,
                    item: ItemType::CitadelModule,
                    count: 1,
                },
                Command::Dispatch {
                    vessel: dropper,
                    to: Destination {
                        host: home,
                        berth: Berth::Lurking,
                    },
                },
            ],
        );

        assert!(days(&data, &mut world, 4).events.is_empty());
        let report = days(&data, &mut world, 1);
        assert!(matches!(
            report.events[..],
            [Event::VesselArrived {
                day: 5,
                berth: Berth::Lurking,
                ..
            }]
        ));
        assert_eq!(world.vessels[&dropper].fuel, 15);

        let report = orders(
            &data,
            &mut world,
            vec![Command::Deploy {
                vessel: dropper,
                slot: 0,
            }],
        );
        // The first module above the hideout also opens worm research.
        assert!(matches!(
            report.events[..],
            [
                Event::Installed {
                    item: ItemType::CitadelModule,
                    installed: 1,
                    ..
                },
                Event::Unlocked {
                    milestone: crate::unlocks::Milestone::FirstCitadelModule,
                    ..
                }
            ]
        ));
        assert_eq!(world.players[&CREW].hideout.citadel.modules, 1);
        assert_eq!(world.vessels[&dropper].modules[0], Module::ToolModule(None));
    }

    #[test]
    fn a_worm_routes_to_transit_and_claims_it() {
        let (data, mut world) = setup();
        let transit = host(&data, "Transit");
        let worm = ready_worm(&data, &mut world);
        orders(
            &data,
            &mut world,
            vec![Command::Dispatch {
                vessel: worm,
                to: Destination {
                    host: transit,
                    berth: Berth::Lurking,
                },
            }],
        );

        // One day to disconnect, then four to route from Earth to Mars.
        assert!(days(&data, &mut world, 4).events.is_empty());
        let report = days(&data, &mut world, 1);
        assert!(matches!(
            report.events[..],
            [Event::VesselArrived { day: 5, .. }]
        ));
        assert_eq!(world.vessels[&worm].fuel, 45);

        let report = orders(
            &data,
            &mut world,
            vec![Command::Deploy {
                vessel: worm,
                slot: 0,
            }],
        );
        assert!(matches!(
            report.events[..],
            [
                Event::HostClaimed { .. },
                Event::Installed { installed: 1, .. }
            ]
        ));
        assert_eq!(world.host(transit).controller, Some(Controller::Crew(CREW)));
        assert_eq!(world.host(transit).site.citadel.modules, 1);
    }

    #[test]
    fn vessels_respect_range_and_other_crews() {
        let (data, mut world) = setup();
        let worm = ready_worm(&data, &mut world);
        let dispatch = |host, berth| Command::Dispatch {
            vessel: worm,
            to: Destination { host, berth },
        };
        let other_network =
            HostId(data.hosts.iter().position(|h| h.network.0 == 1).unwrap() as u16);
        let colossus = host(&data, "Colossus");
        let report = resolve_turn(
            &data,
            &mut world,
            &Orders::from([(
                CREW,
                vec![
                    dispatch(other_network, Berth::Lurking),
                    dispatch(colossus, Berth::Connected),
                    dispatch(colossus, Berth::Planted),
                ],
            )]),
            0,
        );
        let errors: Vec<_> = report.rejected.iter().map(|r| r.error.clone()).collect();
        assert_eq!(
            errors,
            [
                CommandError::OutOfRange(other_network),
                CommandError::HostTaken(colossus),
                CommandError::WrongBerth,
            ]
        );
        // Lurking outside the Legacy Net's host is allowed.
        orders(&data, &mut world, vec![dispatch(colossus, Berth::Lurking)]);
    }

    #[test]
    fn a_vessel_without_anonymisation_stops_and_is_burned_while_lurking() {
        let (data, mut world) = setup();
        let worm = ready_worm(&data, &mut world);
        let transit = host(&data, "Transit");
        // Exactly enough for the trip: 1 to disconnect, 4 to route.
        world.vessels.get_mut(&worm).unwrap().fuel = 5;
        orders(
            &data,
            &mut world,
            vec![Command::Dispatch {
                vessel: worm,
                to: Destination {
                    host: transit,
                    berth: Berth::Lurking,
                },
            }],
        );
        days(&data, &mut world, 5);
        assert_eq!(world.vessels[&worm].fuel, 0);

        let report = resolve_turn(
            &data,
            &mut world,
            &Orders::from([(
                CREW,
                vec![Command::Dispatch {
                    vessel: worm,
                    to: Destination {
                        host: data.hideout.host,
                        berth: Berth::Lurking,
                    },
                }],
            )]),
            4,
        );
        assert!(matches!(
            report.events[..],
            [Event::VesselStopped {
                reason: AbortReason::NoAnonymisation,
                ..
            }]
        ));
        assert!(world.vessels.contains_key(&worm));
        let report = days(&data, &mut world, 1);
        assert!(matches!(
            report.events[..],
            [Event::VesselBurned { day: 10, .. }]
        ));
        assert!(!world.vessels.contains_key(&worm));
    }

    #[test]
    fn worms_wait_for_the_citadel_port() {
        let (data, mut world) = setup();
        let home = data.hideout.host;
        let first = ready_worm(&data, &mut world);
        world
            .players
            .get_mut(&CREW)
            .unwrap()
            .hideout
            .citadel
            .staff
            .push(Staff::new("Second", StaffKind::Operator, 5));
        let second = ready_worm(&data, &mut world);
        let to = |berth| Destination { host: home, berth };
        orders(
            &data,
            &mut world,
            vec![Command::Dispatch {
                vessel: second,
                to: to(Berth::Lurking),
            }],
        );
        days(&data, &mut world, 1);
        orders(
            &data,
            &mut world,
            vec![Command::Dispatch {
                vessel: second,
                to: to(Berth::Connected),
            }],
        );
        days(&data, &mut world, 3);
        assert_eq!(world.vessels[&second].state, VesselState::Connecting);
        assert_eq!(world.vessels[&second].fuel, 50 - 1 - 3);

        orders(
            &data,
            &mut world,
            vec![Command::Dispatch {
                vessel: first,
                to: to(Berth::Lurking),
            }],
        );
        let report = days(&data, &mut world, 2);
        assert!(report.events.iter().any(|e| matches!(
            e,
            Event::VesselArrived {
                vessel,
                berth: Berth::Connected,
                ..
            } if *vessel == second
        )));
    }

    #[test]
    fn modules_hold_what_they_are_made_for() {
        let (data, mut world) = setup();
        let worm = ready_worm(&data, &mut world);
        let load = |slot, item, count| Command::Load {
            vessel: worm,
            slot,
            item,
            count,
        };
        let report = resolve_turn(
            &data,
            &mut world,
            &Orders::from([(
                CREW,
                vec![
                    Command::Fit {
                        vessel: worm,
                        slot: 1,
                        module: Some(ModuleKind::DataContainer),
                    },
                    load(1, ItemType::Compute, 250),
                    load(1, ItemType::Compute, 1),
                    load(1, ItemType::CitadelModule, 1),
                    load(0, ItemType::CitadelModule, 1),
                    load(2, ItemType::Compute, 1),
                    Command::Unload {
                        vessel: worm,
                        slot: 1,
                    },
                ],
            )]),
            0,
        );
        let errors: Vec<_> = report
            .rejected
            .iter()
            .map(|r| (r.index, r.error.clone()))
            .collect();
        assert_eq!(
            errors,
            [
                (2, CommandError::ModuleFull),
                (3, CommandError::WrongModule(ItemType::CitadelModule)),
                (4, CommandError::ModuleFull),
                (5, CommandError::WrongModule(ItemType::Compute)),
            ]
        );
        assert_eq!(
            world.players[&CREW]
                .hideout
                .citadel
                .store
                .get(ItemType::Compute),
            400
        );
    }

    #[test]
    fn coders_can_leave_and_rejoin_a_workshop() {
        let (data, mut world) = setup();
        let player = world.players.get_mut(&CREW).unwrap();
        player.workshop.coders = Some(Staff::new("Builder", StaffKind::Coder, 30));
        let at = crate::workshop::WorkshopRef::Hideout;
        orders(&data, &mut world, vec![Command::ReleaseCoders { at }]);
        let player = &world.players[&CREW];
        assert!(player.workshop.coders.is_none());
        assert_eq!(player.hideout.staff.len(), 2);

        let report = resolve_turn(
            &data,
            &mut world,
            &Orders::from([(
                CREW,
                vec![
                    Command::AssignCoders { at, team: 0 },
                    Command::AssignCoders { at, team: 1 },
                ],
            )]),
            0,
        );
        assert_eq!(report.rejected[0].error, CommandError::NotCoders);
        assert_eq!(
            world.players[&CREW].workshop.coders.as_ref().unwrap().count,
            30
        );
    }

    #[test]
    fn worlds_with_vessels_in_flight_resolve_identically_after_save_and_restore() {
        let (data, mut world) = setup();
        let worm = ready_worm(&data, &mut world);
        orders(
            &data,
            &mut world,
            vec![Command::Dispatch {
                vessel: worm,
                to: Destination {
                    host: host(&data, "Waterworks"),
                    berth: Berth::Lurking,
                },
            }],
        );
        days(&data, &mut world, 3);
        let mut restored: World =
            serde_json::from_str(&serde_json::to_string(&world).unwrap()).unwrap();
        assert_eq!(days(&data, &mut world, 30), days(&data, &mut restored, 30));
        assert_eq!(world, restored);
    }
}
