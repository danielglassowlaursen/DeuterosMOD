//! The bot's colonies: hosts it works like the hideout, with a backdoor,
//! taps and a dropper running an exfil script, so the rare resources of
//! the home network reach the hideout's citadel. The worm carries out what
//! each colony still lacks, a few loads a trip, and brings back what the
//! taps have found.

use std::cmp::Reverse;

use crate::command::Command;
use crate::data::HostDef;
use crate::exfil::Route;
use crate::ids::HostId;
use crate::items::ItemType;
use crate::site::{STAFF_SLOTS, Site};
use crate::staff::StaffKind;
use crate::store::Store;
use crate::transport::{
    self, Berth, Destination, Module, ModuleKind, Seat, Vessel, VesselId, VesselKind,
};
use crate::workshop::{SiteRef, WorkshopRef};
use crate::world::Controller;

use super::Bot;

/// Resources the hideout's host lacks and the war needs.
pub(super) const RARE: [ItemType; 5] = [
    ItemType::ZeroDays,
    ItemType::Crypto,
    ItemType::SigningKeys,
    ItemType::Certificates,
    ItemType::Firmware,
];
/// Colonies the bot works at once.
pub(super) const MAX_COLONIES: usize = 3;
/// A colony's stock of a rare resource that is worth a trip to collect.
pub(super) const COLLECT_AT: u32 = 200;
/// Anonymisation a colony keeps for its dropper.
const COLONY_FUEL: u32 = 150;
/// Smallest load of a resource the worm bothers to carry.
pub(super) const MIN_LOAD: u32 = 25;
/// Anonymisation the hideout citadel keeps back from colony loads.
const HOME_FUEL_RESERVE: u32 = 50;
/// A colony's anonymisation beyond which the worm brings some home.
const FUEL_SURPLUS: u32 = 400;
/// A stock at home beyond which a rare resource is not worth collecting.
const ENOUGH_AT_HOME: u32 = 5000;

/// Something a colony still has to be brought.
pub(super) enum Need {
    Team(StaffKind),
    Tool(ItemType, u32),
    Resource(ItemType, u32),
}

/// How much a host's resources are worth working: what daemons and C2
/// controllers need first.
pub(super) fn value(def: &HostDef) -> u32 {
    def.resources
        .iter()
        .map(|r| match r {
            ItemType::ZeroDays | ItemType::Crypto => 3,
            ItemType::SigningKeys => 2,
            ItemType::Certificates | ItemType::Firmware => 1,
            _ => 0,
        })
        .sum()
}

/// Whether a module slot can take a new load once this turn's unloads apply.
pub(super) fn cargo_free(module: &Module, slot: usize, freed: &[usize]) -> bool {
    freed.contains(&slot)
        || matches!(
            module,
            Module::Empty
                | Module::DataContainer(None)
                | Module::ToolModule(None)
                | Module::SessionPod(None)
        )
}

impl<'a> Bot<'a> {
    /// The crew's complete citadels beyond the hideout.
    pub(super) fn held_citadels(&self) -> Vec<HostId> {
        self.world
            .hosts
            .iter()
            .enumerate()
            .filter(|(h, state)| {
                HostId(*h as u16) != self.home
                    && state.controller == Some(Controller::Crew(self.crew))
                    && state.site.citadel.complete()
            })
            .map(|(h, _)| HostId(h as u16))
            .collect()
    }

    pub(super) fn dropper_at(&self, host: HostId) -> Option<(VesselId, &'a Vessel)> {
        self.vessels(VesselKind::Dropper)
            .find(|(_, v)| v.host == host)
    }

    /// The hosts the bot works as colonies: those it has started on first,
    /// then the most valuable, then the nearest.
    pub(super) fn colony_hosts(&self) -> Vec<HostId> {
        let network = self.data.host(self.home).network;
        let mut hosts: Vec<(HostId, bool, u32, u32)> = self
            .held_citadels()
            .into_iter()
            .filter(|&h| self.data.host(h).network == network && value(self.data.host(h)) > 0)
            .map(|h| {
                let site = &self.world.host(h).site;
                let started = site.backdoor_parts > 0
                    || site.citadel.workshop.coders.is_some()
                    || !site.citadel.staff.is_empty()
                    || self.dropper_at(h).is_some();
                (
                    h,
                    started,
                    value(self.data.host(h)),
                    transport::latency(self.data, self.home, h),
                )
            })
            .collect();
        hosts.sort_by_key(|&(h, started, value, latency)| (!started, Reverse(value), latency, h));
        hosts
            .into_iter()
            .take(MAX_COLONIES)
            .map(|(h, ..)| h)
            .collect()
    }

    /// The crew's worms bound for or at a host.
    fn worms_bound_for(&self, host: HostId) -> impl Iterator<Item = &'a Vessel> + use<'a, '_> {
        self.vessels(VesselKind::Worm)
            .filter(move |(_, v)| v.destination.map_or(v.host == host, |d| d.host == host))
            .map(|(_, v)| v)
    }

    /// Units of an item aboard worms bound for the host.
    fn in_transit(&self, host: HostId, item: ItemType) -> u32 {
        self.worms_bound_for(host)
            .flat_map(|v| &v.modules)
            .map(|m| match m {
                Module::DataContainer(Some(c)) | Module::ToolModule(Some(c)) if c.item == item => {
                    c.count
                }
                _ => 0,
            })
            .sum()
    }

    fn team_in_transit(&self, host: HostId, kind: StaffKind) -> bool {
        self.worms_bound_for(host)
            .flat_map(|v| &v.modules)
            .any(|m| matches!(m, Module::SessionPod(Some(t)) if t.kind == kind))
    }

    /// What the colony still lacks, in the order it should come.
    pub(super) fn colony_needs(&self, host: HostId) -> Vec<Need> {
        let site = &self.world.host(host).site;
        let citadel = &site.citadel;
        let dropper = self.dropper_at(host);
        let has = |item: ItemType| citadel.store.get(item) + self.in_transit(host, item);
        let aboard = |item: ItemType| {
            dropper.map_or(0, |(_, d)| {
                d.modules
                    .iter()
                    .map(|m| match m {
                        Module::ToolModule(Some(c)) if c.item == item => c.count,
                        _ => 0,
                    })
                    .sum()
            })
        };
        let staffed = |kind| citadel.staff.iter().any(|s| s.kind == kind);
        let mut needs = Vec::new();

        if citadel.workshop.coders.is_none()
            && !citadel.workshop.automated
            && !staffed(StaffKind::Coder)
            && !self.team_in_transit(host, StaffKind::Coder)
        {
            needs.push(Need::Team(StaffKind::Coder));
        }
        if !dropper.is_some_and(|(_, d)| d.pilot.is_some())
            && !staffed(StaffKind::Operator)
            && !self.team_in_transit(host, StaffKind::Operator)
        {
            needs.push(Need::Team(StaffKind::Operator));
        }

        // The dropper's anonymisation comes before anything it would carry.
        if has(ItemType::ProxyChains) < COLONY_FUEL / 3 {
            needs.push(Need::Resource(ItemType::ProxyChains, COLONY_FUEL));
        }
        let kits = Site::BACKDOOR_PARTS.saturating_sub(
            site.backdoor_parts + has(ItemType::BackdoorKit) + aboard(ItemType::BackdoorKit),
        );
        if kits > 0 {
            needs.push(Need::Tool(ItemType::BackdoorKit, kits));
        }
        if dropper.is_none_or(|(_, d)| d.script.is_none()) && has(ItemType::ExfilScript) == 0 {
            needs.push(Need::Tool(ItemType::ExfilScript, 1));
        }

        // Resources for what the colony's own workshop builds: the dropper's
        // parts and pods, and its taps.
        let queued = |item| citadel.workshop.jobs.iter().any(|j| j.item == item);
        let mut builds = Vec::new();
        if dropper.is_none() {
            for part in [ItemType::DropperCore, ItemType::DropperEngine] {
                if has(part) == 0 && !queued(part) {
                    builds.push(part);
                }
            }
        }
        let fitted = |pod| dropper.is_some_and(|(_, d)| d.modules[0].pod() == Some(pod));
        for pod in [ItemType::ToolModule, ItemType::DataContainer] {
            if has(pod) == 0 && !fitted(pod) && !queued(pod) {
                builds.push(pod);
            }
        }
        for _ in 0..self.colony_taps_wanted(host) {
            builds.push(ItemType::Tap);
        }
        let mut need = Store::default();
        for item in builds {
            for &(input, n) in &self.data.items[&item].recipe {
                need.add(input, n);
            }
        }
        for (item, n) in need.iter() {
            let short = n.saturating_sub(has(item));
            if short > 0 {
                needs.push(Need::Resource(item, short.max(MIN_LOAD)));
            }
        }
        // Bandwidth and proxies, so the colony's workshop refines its own
        // anonymisation; a certificate for a build-bot to run it.
        if citadel.workshop.coders.is_some() || citadel.workshop.automated {
            for input in [ItemType::Bandwidth, ItemType::Proxies] {
                if has(input) < 50 {
                    needs.push(Need::Resource(input, Module::CONTAINER_CAPACITY));
                }
            }
        }
        if citadel.workshop.coders.is_some()
            && !citadel.workshop.automated
            && self.researched(ItemType::BuildBot)
            && has(ItemType::Certificates) == 0
        {
            needs.push(Need::Resource(ItemType::Certificates, MIN_LOAD));
        }
        needs
    }

    /// Taps the colony's workshop still has to build.
    fn colony_taps_wanted(&self, host: HostId) -> u32 {
        let site = &self.world.host(host).site;
        let aboard = self.dropper_at(host).map_or(0, |(_, d)| {
            d.modules
                .iter()
                .map(|m| match m {
                    Module::ToolModule(Some(c)) if c.item == ItemType::Tap => c.count,
                    _ => 0,
                })
                .sum()
        });
        let queued = site
            .citadel
            .workshop
            .jobs
            .iter()
            .filter(|j| j.item == ItemType::Tap)
            .count() as u32;
        Site::MAX_TAPS.saturating_sub(
            site.taps
                + site.store.get(ItemType::Tap)
                + site.citadel.store.get(ItemType::Tap)
                + aboard
                + queued,
        )
    }

    /// Items the home citadel builds so the colonies can be supplied.
    pub(super) fn colony_wants(&self, wants: &mut Vec<ItemType>) {
        let colonies = self.colony_hosts();
        if colonies.is_empty() {
            return;
        }
        let mut tools = Store::default();
        for &host in &colonies {
            for need in self.colony_needs(host) {
                if let Need::Tool(item, n) = need {
                    tools.add(item, n);
                }
            }
        }
        for (item, n) in tools.iter() {
            if self.above.get(item) < n {
                wants.push(item);
            }
        }
        // Spare pods, so the worm can refit for any load.
        for pod in [ItemType::DataContainer, ItemType::SessionPod] {
            if self.above.get(pod) == 0 {
                wants.push(pod);
            }
        }
    }

    /// Teams of a kind the colonies still have to be brought.
    pub(super) fn teams_needed(&self, kind: StaffKind) -> usize {
        self.colony_hosts()
            .iter()
            .filter(|&&h| {
                self.colony_needs(h)
                    .iter()
                    .any(|n| matches!(n, Need::Team(k) if *k == kind))
            })
            .count()
    }

    /// Whether the hideout citadel can fill a need right now.
    fn can_supply(&self, need: &Need) -> bool {
        match *need {
            Need::Team(kind) => self.team_above(kind),
            Need::Tool(item, _) => self.above.get(item) > 0,
            Need::Resource(ItemType::ProxyChains, _) => {
                self.above.get(ItemType::ProxyChains) >= HOME_FUEL_RESERVE / 2 + MIN_LOAD
            }
            Need::Resource(item, count) => self.above.get(item) >= MIN_LOAD.min(count),
        }
    }

    /// The first colony with a need the worm can carry out now.
    pub(super) fn supply_target(&self) -> Option<HostId> {
        self.colony_hosts()
            .into_iter()
            .filter(|&h| !self.threatened(h))
            .find(|&h| self.colony_needs(h).iter().any(|n| self.can_supply(n)))
    }

    /// Whether a swarm is heading for or besieging the host.
    pub(super) fn threatened(&self, host: HostId) -> bool {
        self.world
            .legacy
            .fleets
            .iter()
            .any(|f| f.target == Some(host))
    }

    /// The nearest colony with a stock of a rare resource worth collecting,
    /// or with anonymisation to spare while the hideout citadel is short.
    pub(super) fn collect_target(&self) -> Option<HostId> {
        let fuel_short = self.above.get(ItemType::ProxyChains) < HOME_FUEL_RESERVE * 3;
        self.colony_hosts()
            .into_iter()
            .filter(|&h| !self.threatened(h))
            .filter(|&h| {
                let store = &self.world.host(h).site.citadel.store;
                RARE.iter()
                    .any(|&r| store.get(r) >= COLLECT_AT && self.above.get(r) < ENOUGH_AT_HOME)
                    || (fuel_short && store.get(ItemType::ProxyChains) > FUEL_SURPLUS + MIN_LOAD)
            })
            .min_by_key(|&h| transport::latency(self.data, self.home, h))
    }

    /// Loads the worm, resting at home, with what the colony needs; returns
    /// whether it now carries anything for it. One team a trip.
    pub(super) fn load_for_colony(
        &mut self,
        id: VesselId,
        worm: &Vessel,
        host: HostId,
        freed: &[usize],
    ) -> bool {
        let needs = self.colony_needs(host);
        let mut used: Vec<usize> = Vec::new();
        let mut loaded = false;
        let mut boarded = false;
        let free_slot = |used: &[usize], pod: ItemType| -> Option<usize> {
            worm.modules
                .iter()
                .enumerate()
                .filter(|(s, m)| !used.contains(s) && cargo_free(m, *s, freed))
                .min_by_key(|(_, m)| m.pod() != Some(pod))
                .map(|(s, _)| s)
        };
        for need in needs {
            if used.len() == worm.modules.len() {
                break;
            }
            match need {
                Need::Team(kind) => {
                    if boarded || !self.team_above(kind) {
                        continue;
                    }
                    let Some(slot) = free_slot(&used, ItemType::SessionPod) else {
                        continue;
                    };
                    if !self.fit_above(id, worm, slot, ModuleKind::SessionPod, freed) {
                        continue;
                    }
                    let Some(team) = self.take_team_above(kind) else {
                        continue;
                    };
                    self.order(Command::Board {
                        vessel: id,
                        seat: Seat::Pod(slot),
                        team,
                    });
                    used.push(slot);
                    boarded = true;
                    loaded = true;
                }
                Need::Tool(item, count) => {
                    let single = self.data.items[&item].tool_module_single;
                    let mut left = count;
                    while left > 0 && used.len() < worm.modules.len() {
                        let available = self.above.get(item).min(if single { 1 } else { left });
                        if available == 0 {
                            break;
                        }
                        let Some(slot) = free_slot(&used, ItemType::ToolModule) else {
                            break;
                        };
                        if !self.fit_above(id, worm, slot, ModuleKind::ToolModule, freed) {
                            break;
                        }
                        self.above.take(item, available);
                        self.order(Command::Load {
                            vessel: id,
                            slot,
                            item,
                            count: available,
                        });
                        used.push(slot);
                        left -= available;
                        loaded = true;
                    }
                }
                Need::Resource(item, count) => {
                    let spare = if item == ItemType::ProxyChains {
                        self.above.get(item).saturating_sub(HOME_FUEL_RESERVE / 2)
                    } else {
                        self.above.get(item)
                    };
                    let available = spare.min(count).min(Module::CONTAINER_CAPACITY);
                    if available < MIN_LOAD.min(count) {
                        continue;
                    }
                    let Some(slot) = free_slot(&used, ItemType::DataContainer) else {
                        continue;
                    };
                    if !self.fit_above(id, worm, slot, ModuleKind::DataContainer, freed) {
                        continue;
                    }
                    self.above.take(item, available);
                    self.order(Command::Load {
                        vessel: id,
                        slot,
                        item,
                        count: available,
                    });
                    used.push(slot);
                    loaded = true;
                }
            }
        }
        loaded
    }

    /// A worm connected at one of the crew's citadels away from home: it
    /// hands over what it brought and loads what the taps have found.
    pub(super) fn worm_at_colony(&mut self, id: VesselId, worm: &Vessel) {
        let host = worm.host;
        if !self.holds(host) {
            if worm.fuel > 0 {
                self.dispatch(id, self.home, Berth::Connected);
            }
            return;
        }
        let site = &self.world.host(host).site;
        let mut room = STAFF_SLOTS.saturating_sub(site.citadel.staff.len());
        let mut store = site.citadel.store.clone();
        if worm.daemons > 0 {
            self.order(Command::UnloadDaemons {
                vessel: id,
                count: worm.daemons,
            });
        }
        for (slot, module) in worm.modules.iter().enumerate() {
            match module {
                // Citadel modules ride along for the next claim; a store at
                // its cap keeps nothing more.
                Module::ToolModule(Some(c)) if c.item == ItemType::CitadelModule => {}
                Module::DataContainer(Some(c)) | Module::ToolModule(Some(c))
                    if store.get(c.item) + c.count <= Store::CAP =>
                {
                    store.add(c.item, c.count);
                    self.order(Command::Unload { vessel: id, slot });
                }
                Module::SessionPod(Some(_)) if room > 0 => {
                    room -= 1;
                    self.order(Command::Disembark {
                        vessel: id,
                        seat: Seat::Pod(slot),
                    });
                }
                _ => {}
            }
        }
        // Bring home what the taps found, the biggest stocks first, and
        // anonymisation the colony has to spare. Small stocks stay: they
        // may be what the worm just brought.
        let mut picked = Vec::new();
        for (slot, module) in worm.modules.iter().enumerate() {
            // Only a container emptied by the orders above, or empty already.
            match module {
                Module::DataContainer(None) => {}
                Module::DataContainer(Some(c)) if store.get(c.item) >= c.count => {}
                _ => continue,
            }
            let spare_fuel = store
                .get(ItemType::ProxyChains)
                .saturating_sub(FUEL_SURPLUS);
            let Some((item, count)) = RARE
                .iter()
                .copied()
                .filter(|r| {
                    !picked.contains(r)
                        && store.get(*r) >= COLLECT_AT
                        && self.above.get(*r) < ENOUGH_AT_HOME
                })
                .map(|r| (r, store.get(r)))
                .chain(
                    (spare_fuel >= MIN_LOAD && !picked.contains(&ItemType::ProxyChains))
                        .then_some((ItemType::ProxyChains, spare_fuel)),
                )
                .max_by_key(|&(_, count)| count)
            else {
                break;
            };
            let count = count.min(Module::CONTAINER_CAPACITY);
            store.take(item, count);
            picked.push(item);
            self.order(Command::Load {
                vessel: id,
                slot,
                item,
                count,
            });
        }
        if worm.fuel >= transport::latency(self.data, host, self.home) + 2 {
            self.dispatch(id, self.home, Berth::Connected);
        }
    }

    // ------------------------------------------------------------ colonies

    /// Orders for every colony: its workshop, its dropper and its taps.
    pub(super) fn colonies(&mut self) {
        for host in self.colony_hosts() {
            self.colony(host);
        }
    }

    fn colony(&mut self, host: HostId) {
        let site = &self.world.host(host).site;
        let citadel = &site.citadel;
        let at = WorkshopRef::Citadel(SiteRef::Host(host));
        let mut store = citadel.store.clone();
        let dropper = self.dropper_at(host);

        let automated = citadel.workshop.automated;
        match &citadel.workshop.coders {
            None if !automated => {
                if let Some(team) = citadel
                    .staff
                    .iter()
                    .position(|s| s.kind == StaffKind::Coder)
                {
                    self.order(Command::AssignCoders { at, team });
                }
            }
            coders => {
                let level = coders.as_ref().map_or(0, |c| c.level());
                let mut wants = Vec::new();
                if !automated
                    && self.researched(ItemType::BuildBot)
                    && store.get(ItemType::Certificates) > 0
                {
                    wants.push(ItemType::BuildBot);
                }
                if dropper.is_none() {
                    for part in [ItemType::DropperCore, ItemType::DropperEngine] {
                        if store.get(part) == 0 {
                            wants.push(part);
                        }
                    }
                }
                let fitted = |pod| dropper.is_some_and(|(_, d)| d.modules[0].pod() == Some(pod));
                for pod in [ItemType::ToolModule, ItemType::DataContainer] {
                    if store.get(pod) == 0 && !fitted(pod) {
                        wants.push(pod);
                    }
                }
                if self.colony_taps_wanted(host) > 0 {
                    wants.push(ItemType::Tap);
                }
                self.produce(at, &citadel.workshop, level, false, &mut store, &wants);
            }
        }

        match dropper {
            None => {
                if store.get(ItemType::DropperCore) > 0 && store.get(ItemType::DropperEngine) > 0 {
                    self.order(Command::Assemble {
                        host,
                        berth: Berth::Connected,
                        kind: VesselKind::Dropper,
                    });
                }
            }
            Some((id, dropper)) => self.colony_dropper(host, id, dropper, &mut store),
        }
    }

    /// The colony's dropper: it installs the backdoor kits and taps the
    /// worm brings, then shuttles the taps' finds up by script.
    /// What the colony's dropper shuttles up: the rare finds the hideout
    /// is shortest of, and the makings of anonymisation while the colony's
    /// citadel runs low on it.
    fn colony_outbound(&self, host: HostId) -> Vec<ItemType> {
        let def = self.data.host(host);
        let store = &self.world.host(host).site.citadel.store;
        let mut rare: Vec<ItemType> = def
            .resources
            .iter()
            .copied()
            .filter(|r| RARE.contains(r) && self.above.get(*r) < 2000)
            .collect();
        rare.sort_by_key(|&r| self.above.get(r));
        if store.get(ItemType::ProxyChains) < COLONY_FUEL {
            rare.extend(
                def.resources
                    .iter()
                    .copied()
                    .filter(|r| matches!(r, ItemType::Bandwidth | ItemType::Proxies)),
            );
        }
        if rare.is_empty() {
            rare.extend(def.resources.iter().copied().filter(|r| RARE.contains(r)));
        }
        rare
    }

    fn colony_dropper(&mut self, host: HostId, id: VesselId, dropper: &Vessel, store: &mut Store) {
        if dropper.destination.is_some() {
            return;
        }
        if let Some(route) = dropper.script.as_ref().and_then(|s| s.route.as_ref()) {
            // Point the running shuttle at what is wanted now.
            let outbound = self.colony_outbound(host);
            if route.outbound != outbound {
                self.order(Command::ConfigureScript {
                    vessel: id,
                    route: Some(Route {
                        outbound,
                        ..route.clone()
                    }),
                });
            }
            return;
        }
        let Some(berth) = dropper.berth() else {
            return;
        };
        let site = &self.world.host(host).site;
        let down = Destination {
            host,
            berth: Berth::Planted,
        };
        let up = Destination {
            host,
            berth: Berth::Connected,
        };
        match berth {
            Berth::Lurking => {
                if dropper.fuel > 0 {
                    self.order(Command::Dispatch { vessel: id, to: up });
                }
            }
            Berth::Planted => match &dropper.modules[0] {
                Module::ToolModule(Some(cargo))
                    if cargo.item == ItemType::BackdoorKit && !site.backdoor_complete() =>
                {
                    self.order(Command::Deploy {
                        vessel: id,
                        slot: 0,
                    });
                }
                Module::ToolModule(Some(cargo)) if cargo.item == ItemType::Tap => {
                    self.order(Command::Unload {
                        vessel: id,
                        slot: 0,
                    });
                    let count = cargo.count.min(Site::MAX_TAPS - site.taps);
                    if count > 0 {
                        self.order(Command::InstallTaps {
                            site: SiteRef::Host(host),
                            count,
                        });
                    }
                }
                Module::ToolModule(Some(_)) | Module::DataContainer(Some(_)) => {
                    self.order(Command::Unload {
                        vessel: id,
                        slot: 0,
                    });
                }
                _ => {
                    if dropper.fuel >= 5 {
                        self.order(Command::Dispatch { vessel: id, to: up });
                    }
                }
            },
            Berth::Connected => {
                if dropper.pilot.is_none() {
                    if let Some(team) = site
                        .citadel
                        .staff
                        .iter()
                        .position(|s| s.kind == StaffKind::Operator)
                    {
                        self.order(Command::Board {
                            vessel: id,
                            seat: Seat::Pilot,
                            team,
                        });
                    }
                    return;
                }
                let mut fuel = dropper.fuel;
                if fuel < 50 {
                    let amount = store.get(ItemType::ProxyChains).min(100 - fuel);
                    if amount > 0 {
                        store.take(ItemType::ProxyChains, amount);
                        fuel += amount;
                        self.order(Command::Refuel { vessel: id, amount });
                    }
                }
                match &dropper.modules[0] {
                    Module::ToolModule(Some(cargo))
                        if (cargo.item == ItemType::BackdoorKit && !site.backdoor_complete())
                            || (cargo.item == ItemType::Tap && site.taps < Site::MAX_TAPS) =>
                    {
                        if fuel >= 10 {
                            self.order(Command::Dispatch {
                                vessel: id,
                                to: down,
                            });
                        }
                        return;
                    }
                    Module::ToolModule(Some(_)) | Module::DataContainer(Some(_)) => {
                        self.order(Command::Unload {
                            vessel: id,
                            slot: 0,
                        });
                        return;
                    }
                    _ => {}
                }
                if fuel < 10 {
                    return;
                }
                if !site.backdoor_complete() {
                    if store.get(ItemType::BackdoorKit) > 0
                        && self.fit_from(id, dropper, 0, ModuleKind::ToolModule, store)
                    {
                        store.take(ItemType::BackdoorKit, 1);
                        self.order(Command::Load {
                            vessel: id,
                            slot: 0,
                            item: ItemType::BackdoorKit,
                            count: 1,
                        });
                        self.order(Command::Dispatch {
                            vessel: id,
                            to: down,
                        });
                    }
                    return;
                }
                let taps_wanted = Site::MAX_TAPS - site.taps - site.store.get(ItemType::Tap);
                if taps_wanted > 0 {
                    if store.get(ItemType::Tap) > 0
                        && self.fit_from(id, dropper, 0, ModuleKind::ToolModule, store)
                    {
                        let count = store.get(ItemType::Tap).min(taps_wanted);
                        store.take(ItemType::Tap, count);
                        self.order(Command::Load {
                            vessel: id,
                            slot: 0,
                            item: ItemType::Tap,
                            count,
                        });
                        self.order(Command::Dispatch {
                            vessel: id,
                            to: down,
                        });
                    }
                    return;
                }
                if site.taps < Site::MAX_TAPS {
                    return;
                }
                // Everything is in; shuttle the finds up by script.
                if dropper.script.is_none() {
                    if store.get(ItemType::ExfilScript) == 0 {
                        return;
                    }
                    store.take(ItemType::ExfilScript, 1);
                    self.order(Command::InstallScript { vessel: id });
                }
                if self.fit_from(id, dropper, 0, ModuleKind::DataContainer, store) {
                    let outbound = self.colony_outbound(host);
                    self.order(Command::ConfigureScript {
                        vessel: id,
                        route: Some(Route {
                            from: down,
                            to: up,
                            outbound,
                            inbound: Vec::new(),
                        }),
                    });
                }
            }
        }
    }
}
