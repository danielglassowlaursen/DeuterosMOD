//! A bot that plays one crew from the state of the world alone.
//!
//! It grows the hideout's economy, builds the citadel above it, staffs and
//! supplies that citadel, builds a worm there and sends it out to claim free
//! hosts in the home network with citadel modules. It is used to stress the
//! rules over long games, and can later fill empty seats in a game.
//!
//! The bot keeps no memory between turns: it reads the world and gives the
//! orders that move each part of its plan one step on. It plans against
//! copies of the stores it draws from, so that it never gives an order the
//! rules would reject.

use crate::command::Command;
use crate::data::GameData;
use crate::exfil::Route;
use crate::ids::{HostId, PlayerId};
use crate::items::ItemType;
use crate::site::{Citadel, STAFF_SLOTS, Site};
use crate::staff::StaffKind;
use crate::store::Store;
use crate::transport::{
    self, Berth, Destination, Module, ModuleKind, Seat, Vessel, VesselId, VesselKind, VesselState,
};
use crate::workshop::{SiteRef, Workshop, WorkshopRef};
use crate::world::{Controller, Player, World};

/// Research in the order the bot wants it.
const RESEARCH: [ItemType; 15] = [
    ItemType::DropperCore,
    ItemType::DropperEngine,
    ItemType::ToolModule,
    ItemType::CitadelModule,
    ItemType::ProxyChains,
    ItemType::SessionPod,
    ItemType::DataContainer,
    ItemType::WormCore,
    ItemType::WormEngine,
    ItemType::ExfilScript,
    ItemType::Crawler,
    ItemType::Sniffer,
    ItemType::BuildBot,
    ItemType::Patch,
    ItemType::BackdoorKit,
];

/// Resources the citadel's workshop and worm need from the hideout.
const SUPPLY: [ItemType; 6] = [
    ItemType::Compute,
    ItemType::Storage,
    ItemType::Memory,
    ItemType::Code,
    ItemType::Credentials,
    ItemType::ProxyChains,
];

/// Citadel modules the bot keeps in stock above the hideout for expeditions.
const MODULES_IN_STOCK: u32 = 3;

/// Anonymisation a dropper needs before it leaves the hideout: the longest
/// round trip (out to the citadel and back down) with some to spare, so it
/// never ends up lurking with none and gets traced.
const DROPPER_TRIP_FUEL: u32 = 12;

/// The orders the bot gives for its crew this turn.
pub fn orders(data: &GameData, world: &World, crew: PlayerId) -> Vec<Command> {
    let Some(player) = world.players.get(&crew) else {
        return Vec::new();
    };
    let mut bot = Bot {
        data,
        world,
        crew,
        player,
        home: data.hideout.host,
        taps: player.hideout.taps,
        ground: player.hideout.store.clone(),
        above: player.hideout.citadel.store.clone(),
        commands: Vec::new(),
    };
    bot.recruit();
    bot.research();
    bot.install_taps();
    bot.ground_workshop();
    if player.hideout.citadel.complete() {
        bot.citadel_workshop();
        bot.worms();
    }
    bot.dropper();
    bot.commands
}

struct Bot<'a> {
    data: &'a GameData,
    world: &'a World,
    crew: PlayerId,
    player: &'a Player,
    home: HostId,
    /// The hideout's taps, as the orders so far leave them.
    taps: u32,
    /// The hideout's store, as the orders so far leave it.
    ground: Store,
    /// The hideout citadel's store, as the orders so far leave it.
    above: Store,
    commands: Vec<Command>,
}

impl<'a> Bot<'a> {
    fn order(&mut self, command: Command) {
        self.commands.push(command);
    }

    fn researched(&self, item: ItemType) -> bool {
        self.player
            .research
            .get(&item)
            .is_some_and(|r| r.researched)
    }

    /// The crew's vessels of one kind.
    fn vessels(&self, kind: VesselKind) -> impl Iterator<Item = (VesselId, &'a Vessel)> + use<'a> {
        let crew = self.crew;
        self.world
            .vessels
            .iter()
            .filter(move |(_, v)| v.owner == crew && v.kind == kind)
            .map(|(&id, v)| (id, v))
    }

    fn citadel_complete(&self) -> bool {
        self.player.hideout.citadel.complete()
    }

    // ------------------------------------------------------------ people

    fn recruit(&mut self) {
        let recruitment = &self.player.recruitment;
        let enrolled: u32 = recruitment.courses.values().map(|c| c.enrolled).sum();
        let mut budget = recruitment.available.saturating_sub(enrolled);
        let operator_teams = self.operator_teams();
        for kind in [StaffKind::Analyst, StaffKind::Coder, StaffKind::Operator] {
            let course = recruitment.courses.get(&kind);
            if course.is_some_and(|c| c.running() || c.enrolled > 0) {
                continue;
            }
            let def = &self.data.recruitment.courses[&kind];
            let wanted = match kind {
                StaffKind::Analyst => {
                    let team = self.player.research_team.as_ref().map_or(0, |s| s.count);
                    def.team_max.unwrap_or(0).saturating_sub(team)
                }
                StaffKind::Coder => {
                    let team = self.player.workshop.coders.as_ref().map_or(0, |s| s.count);
                    def.team_max.unwrap_or(0).saturating_sub(team)
                }
                StaffKind::Operator if operator_teams < 3 => 20,
                StaffKind::Operator => 0,
            };
            let count = wanted.min(def.batch_max).min(budget);
            if count > 0 {
                budget -= count;
                self.order(Command::Recruit { kind, count });
            }
        }
    }

    /// Operator teams the crew has anywhere: stationed, piloting or in transit.
    fn operator_teams(&self) -> usize {
        let hideout = &self.player.hideout;
        let stationed = hideout
            .staff
            .iter()
            .chain(&hideout.citadel.staff)
            .filter(|s| s.kind == StaffKind::Operator)
            .count();
        let aboard: usize = self
            .world
            .vessels
            .values()
            .filter(|v| v.owner == self.crew)
            .map(|v| {
                let pods = v
                    .modules
                    .iter()
                    .filter(|m| match m {
                        Module::SessionPod(Some(team)) => team.kind == StaffKind::Operator,
                        _ => false,
                    })
                    .count();
                pods + usize::from(v.pilot.is_some())
            })
            .sum();
        stationed + aboard
    }

    fn research(&mut self) {
        let Some(team) = &self.player.research_team else {
            return;
        };
        let current = self.player.current_research;
        if current.is_some_and(|item| !self.researched(item)) {
            return;
        }
        let next = RESEARCH.into_iter().find(|item| {
            self.player
                .research
                .get(item)
                .is_some_and(|p| !p.researched)
                && self.data.research[item].tech_level <= team.level()
        });
        if let Some(item) = next
            && Some(item) != current
        {
            self.order(Command::SetResearch { item });
        }
    }

    // ------------------------------------------------------------ hideout

    fn install_taps(&mut self) {
        let count = self
            .ground
            .get(ItemType::Tap)
            .min(Site::MAX_TAPS - self.taps);
        if count > 0 {
            self.taps += count;
            self.ground.take(ItemType::Tap, count);
            self.order(Command::InstallTaps {
                site: SiteRef::Hideout,
                count,
            });
        }
    }

    fn ground_workshop(&mut self) {
        let workshop = &self.player.workshop;
        let Some(coders) = &workshop.coders else {
            return;
        };
        if workshop.jobs.iter().any(|j| j.active) {
            return;
        }
        let level = coders.level();
        let has_dropper = self.vessels(VesselKind::Dropper).next().is_some();
        let dropper_has = |item: ItemType| {
            self.vessels(VesselKind::Dropper).any(|(_, v)| match item {
                ItemType::ExfilScript => v.script.is_some(),
                pod => v.modules.iter().any(|m| m.pod() == Some(pod)),
            })
        };
        let hideout = &self.player.hideout;
        let carried = self.carried(ItemType::CitadelModule);
        let modules_wanted = if self.citadel_complete() {
            if self.script_running() {
                0
            } else {
                MODULES_IN_STOCK
            }
        } else {
            Citadel::MODULES - hideout.citadel.modules
        };

        let mut wants = Vec::new();
        if self.taps + self.ground.get(ItemType::Tap) < Site::MAX_TAPS {
            wants.push(ItemType::Tap);
        }
        if !has_dropper {
            for part in [ItemType::DropperCore, ItemType::DropperEngine] {
                if self.ground.get(part) == 0 {
                    wants.push(part);
                }
            }
        }
        for pod in [
            ItemType::ToolModule,
            ItemType::SessionPod,
            ItemType::DataContainer,
        ] {
            if self.ground.get(pod) == 0 && !dropper_has(pod) {
                wants.push(pod);
            }
        }
        if self.ground.get(ItemType::CitadelModule)
            + self.above.get(ItemType::CitadelModule)
            + carried
            < modules_wanted
        {
            wants.push(ItemType::CitadelModule);
        }
        if self.citadel_staffed()
            && !dropper_has(ItemType::ExfilScript)
            && self.ground.get(ItemType::ExfilScript) == 0
        {
            wants.push(ItemType::ExfilScript);
        }

        let mut ground = self.ground.clone();
        if let Some(item) = self.pick_build(workshop, level, true, &mut ground, &wants) {
            self.ground = ground;
            self.order(Command::Build {
                at: WorkshopRef::Hideout,
                item,
            });
        }
    }

    /// The first wanted item the workshop can start: a paused job resumes for
    /// free, a new one needs research, the team's level and its recipe.
    fn pick_build(
        &self,
        workshop: &Workshop,
        level: u8,
        ground: bool,
        store: &mut Store,
        wants: &[ItemType],
    ) -> Option<ItemType> {
        wants.iter().copied().find(|&item| {
            if self.data.research[&item].tech_level > level {
                return false;
            }
            if workshop.jobs.iter().any(|j| j.item == item) {
                return true;
            }
            let def = &self.data.items[&item];
            let affordable = def.recipe.iter().all(|&(input, n)| store.get(input) >= n);
            if !self.researched(item) || (def.orbit_only && ground) || !affordable {
                return false;
            }
            for &(input, n) in &def.recipe {
                store.take(input, n);
            }
            true
        })
    }

    /// Units of an item aboard the crew's vessels.
    fn carried(&self, item: ItemType) -> u32 {
        self.world
            .vessels
            .values()
            .filter(|v| v.owner == self.crew)
            .flat_map(|v| &v.modules)
            .map(|m| match m {
                Module::DataContainer(Some(c)) | Module::ToolModule(Some(c)) if c.item == item => {
                    c.count
                }
                _ => 0,
            })
            .sum()
    }

    fn script_running(&self) -> bool {
        self.vessels(VesselKind::Dropper)
            .any(|(_, v)| v.script.as_ref().is_some_and(|s| s.running()))
    }

    /// Coders run the citadel's workshop and an operator team waits there.
    fn citadel_staffed(&self) -> bool {
        let citadel = &self.player.hideout.citadel;
        citadel.workshop.coders.is_some()
            && (citadel.staff.iter().any(|s| s.kind == StaffKind::Operator)
                || self
                    .vessels(VesselKind::Worm)
                    .any(|(_, v)| v.pilot.is_some()))
    }

    // ------------------------------------------------------------ dropper

    fn dropper(&mut self) {
        let home = self.home;
        let Some((id, dropper)) = self.vessels(VesselKind::Dropper).next() else {
            if self.ground.get(ItemType::DropperCore) > 0
                && self.ground.get(ItemType::DropperEngine) > 0
            {
                self.ground.take(ItemType::DropperCore, 1);
                self.ground.take(ItemType::DropperEngine, 1);
                self.order(Command::Assemble {
                    host: home,
                    berth: Berth::Planted,
                    kind: VesselKind::Dropper,
                });
            }
            return;
        };
        if dropper.script.as_ref().is_some_and(|s| s.running()) || dropper.destination.is_some() {
            return;
        }
        let VesselState::At(berth) = dropper.state else {
            return;
        };
        match berth {
            Berth::Planted => self.dropper_inside(id, dropper),
            Berth::Connected => self.dropper_above(id, dropper),
            Berth::Lurking => self.dropper_outside(id, dropper),
        }
    }

    fn dropper_inside(&mut self, id: VesselId, dropper: &Vessel) {
        let home = self.home;
        let hideout = &self.player.hideout;
        if dropper.pilot.is_none() {
            if let Some(team) = hideout
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
        if dropper.fuel < 30 {
            let amount = self
                .ground
                .get(ItemType::ProxyChains)
                .min(100 - dropper.fuel);
            if amount > 0 {
                self.ground.take(ItemType::ProxyChains, amount);
                self.order(Command::Refuel { vessel: id, amount });
            }
        }
        if dropper.fuel + self.refuelled(id) < DROPPER_TRIP_FUEL {
            return;
        }
        // Empty whatever came back down.
        if let Module::DataContainer(Some(_)) | Module::ToolModule(Some(_)) = dropper.modules[0] {
            self.order(Command::Unload {
                vessel: id,
                slot: 0,
            });
            return;
        }
        if let Module::SessionPod(Some(_)) = dropper.modules[0] {
            if hideout.staff.len() < STAFF_SLOTS {
                self.order(Command::Disembark {
                    vessel: id,
                    seat: Seat::Pod(0),
                });
            }
            return;
        }

        let up = Destination {
            host: home,
            berth: Berth::Connected,
        };
        if !self.citadel_complete() {
            if self.fit(id, dropper, ModuleKind::ToolModule)
                && self.ground.get(ItemType::CitadelModule) > 0
            {
                self.ground.take(ItemType::CitadelModule, 1);
                self.load(id, ItemType::CitadelModule, 1);
                self.dispatch(id, home, Berth::Lurking);
            }
            return;
        }

        // Staff the citadel: its coders first, then an operator team.
        let citadel = &hideout.citadel;
        let waiting_coders = hideout
            .staff
            .iter()
            .position(|s| s.kind == StaffKind::Coder);
        if citadel.workshop.coders.is_none()
            && !citadel.staff.iter().any(|s| s.kind == StaffKind::Coder)
        {
            match (waiting_coders, &self.player.workshop.coders) {
                (Some(team), _) => {
                    if self.fit(id, dropper, ModuleKind::SessionPod) {
                        self.board_pod(id, team, up);
                    }
                }
                // Coders who can build worms leave for the citadel; new
                // recruits take over the hideout workshop.
                (None, Some(coders))
                    if coders.level() >= 2
                        && hideout.staff.len() < STAFF_SLOTS
                        && !self.player.workshop.jobs.iter().any(|j| j.active) =>
                {
                    self.order(Command::ReleaseCoders {
                        at: WorkshopRef::Hideout,
                    });
                }
                _ => {}
            }
            return;
        }
        if !self.citadel_staffed() {
            // Keep one operator team at home for the dropper's next pilot.
            let operators: Vec<usize> = hideout
                .staff
                .iter()
                .enumerate()
                .filter(|(_, s)| s.kind == StaffKind::Operator)
                .map(|(i, _)| i)
                .collect();
            if let Some(&team) = operators.first()
                && citadel.staff.len() < STAFF_SLOTS
                && self.fit(id, dropper, ModuleKind::SessionPod)
            {
                self.board_pod(id, team, up);
            }
            return;
        }

        // Supplied by script from here on, once one is built.
        if dropper.script.is_some() || self.ground.get(ItemType::ExfilScript) > 0 {
            if self.fit(id, dropper, ModuleKind::DataContainer) {
                if dropper.script.is_none() {
                    self.ground.take(ItemType::ExfilScript, 1);
                    self.order(Command::InstallScript { vessel: id });
                }
                self.order(Command::ConfigureScript {
                    vessel: id,
                    route: Some(Route {
                        from: Destination {
                            host: home,
                            berth: Berth::Planted,
                        },
                        to: up,
                        outbound: SUPPLY.to_vec(),
                        inbound: Vec::new(),
                    }),
                });
            }
            return;
        }

        // Otherwise carry what the citadel lacks most, one load per trip.
        if self.above.get(ItemType::CitadelModule) + self.carried(ItemType::CitadelModule)
            < MODULES_IN_STOCK
            && self.ground.get(ItemType::CitadelModule) > 0
        {
            if self.fit(id, dropper, ModuleKind::ToolModule) {
                self.ground.take(ItemType::CitadelModule, 1);
                self.load(id, ItemType::CitadelModule, 1);
                self.dispatch(id, home, Berth::Connected);
            }
            return;
        }
        if let Some((item, count)) = self.most_needed_above()
            && self.fit(id, dropper, ModuleKind::DataContainer)
        {
            self.ground.take(item, count);
            self.load(id, item, count);
            self.dispatch(id, home, Berth::Connected);
        }
    }

    /// Fits slot 0 with `kind` if it is not already; returns whether the
    /// slot holds `kind` once this turn's orders are applied.
    fn fit(&mut self, id: VesselId, dropper: &Vessel, kind: ModuleKind) -> bool {
        let pod = match kind {
            ModuleKind::DataContainer => ItemType::DataContainer,
            ModuleKind::ToolModule => ItemType::ToolModule,
            ModuleKind::SessionPod => ItemType::SessionPod,
        };
        let current = dropper.modules[0].pod();
        if current == Some(pod) {
            return true;
        }
        if self.ground.get(pod) == 0 {
            return false;
        }
        self.ground.take(pod, 1);
        if let Some(old) = current {
            self.ground.add(old, 1);
        }
        self.order(Command::Fit {
            vessel: id,
            slot: 0,
            module: Some(kind),
        });
        true
    }

    fn load(&mut self, id: VesselId, item: ItemType, count: u32) {
        self.order(Command::Load {
            vessel: id,
            slot: 0,
            item,
            count,
        });
    }

    fn board_pod(&mut self, id: VesselId, team: usize, to: Destination) {
        self.order(Command::Board {
            vessel: id,
            seat: Seat::Pod(0),
            team,
        });
        self.order(Command::Dispatch { vessel: id, to });
    }

    fn dispatch(&mut self, id: VesselId, host: HostId, berth: Berth) {
        self.order(Command::Dispatch {
            vessel: id,
            to: Destination { host, berth },
        });
    }

    /// The resource the citadel lacks most for its next builds and the
    /// worm's anonymisation, with how much of it one container can carry.
    fn most_needed_above(&self) -> Option<(ItemType, u32)> {
        let mut need = Store::default();
        for item in [
            ItemType::WormCore,
            ItemType::WormEngine,
            ItemType::ToolModule,
        ] {
            for &(input, n) in &self.data.items[&item].recipe {
                need.add(input, n);
            }
        }
        need.add(ItemType::ProxyChains, 250);
        SUPPLY
            .into_iter()
            .map(|item| (item, need.get(item).saturating_sub(self.above.get(item))))
            .filter(|&(item, short)| short > 0 && self.ground.get(item) > 0)
            .max_by_key(|&(_, short)| short)
            .map(|(item, _)| (item, self.ground.get(item).min(Module::CONTAINER_CAPACITY)))
    }

    fn dropper_above(&mut self, id: VesselId, dropper: &Vessel) {
        let citadel = &self.player.hideout.citadel;
        match &dropper.modules[0] {
            Module::DataContainer(Some(_)) | Module::ToolModule(Some(_)) => {
                self.order(Command::Unload {
                    vessel: id,
                    slot: 0,
                });
            }
            Module::SessionPod(Some(_)) if citadel.staff.len() < STAFF_SLOTS => {
                self.order(Command::Disembark {
                    vessel: id,
                    seat: Seat::Pod(0),
                });
            }
            Module::SessionPod(Some(_)) => return,
            _ => {}
        }
        if dropper.fuel >= 3 {
            self.dispatch(id, self.home, Berth::Planted);
        }
    }

    fn dropper_outside(&mut self, id: VesselId, dropper: &Vessel) {
        if let Module::ToolModule(Some(cargo)) = &dropper.modules[0]
            && cargo.item == ItemType::CitadelModule
            && dropper.host == self.home
            && !self.citadel_complete()
        {
            self.order(Command::Deploy {
                vessel: id,
                slot: 0,
            });
        }
        if dropper.fuel >= 2 {
            self.dispatch(id, self.home, Berth::Planted);
        }
    }

    // ------------------------------------------------------------ citadel

    fn citadel_workshop(&mut self) {
        let citadel = &self.player.hideout.citadel;
        let at = WorkshopRef::Citadel(SiteRef::Hideout);
        let Some(coders) = &citadel.workshop.coders else {
            if let Some(team) = citadel
                .staff
                .iter()
                .position(|s| s.kind == StaffKind::Coder)
            {
                self.order(Command::AssignCoders { at, team });
            }
            return;
        };
        if citadel.workshop.jobs.iter().any(|j| j.active) {
            return;
        }
        let worms = self.vessels(VesselKind::Worm).count() as u32;
        let worm_modules: u32 = self
            .vessels(VesselKind::Worm)
            .map(|(_, v)| {
                v.modules
                    .iter()
                    .filter(|m| m.pod() == Some(ItemType::ToolModule))
                    .count() as u32
            })
            .sum();
        let mut wants = Vec::new();
        if worms == 0 {
            for part in [ItemType::WormCore, ItemType::WormEngine] {
                if self.above.get(part) == 0 {
                    wants.push(part);
                }
            }
        }
        if worm_modules + self.above.get(ItemType::ToolModule) < 3 {
            wants.push(ItemType::ToolModule);
        }
        if worms > 0
            && self.above.get(ItemType::CitadelModule) + self.carried(ItemType::CitadelModule)
                < MODULES_IN_STOCK
        {
            wants.push(ItemType::CitadelModule);
        }
        let level = coders.level();
        let workshop = &citadel.workshop;
        let mut above = self.above.clone();
        if let Some(item) = self.pick_build(workshop, level, false, &mut above, &wants) {
            self.above = above;
            self.order(Command::Build { at, item });
        }
    }

    // ------------------------------------------------------------ worms

    fn worms(&mut self) {
        let home = self.home;
        if self.vessels(VesselKind::Worm).next().is_none() {
            if self.above.get(ItemType::WormCore) > 0 && self.above.get(ItemType::WormEngine) > 0 {
                self.above.take(ItemType::WormCore, 1);
                self.above.take(ItemType::WormEngine, 1);
                self.order(Command::Assemble {
                    host: home,
                    berth: Berth::Connected,
                    kind: VesselKind::Worm,
                });
            }
            return;
        }
        let worms: Vec<(VesselId, &Vessel)> = self.vessels(VesselKind::Worm).collect();
        for (id, worm) in worms {
            if worm.destination.is_some() {
                continue;
            }
            match worm.state {
                VesselState::At(Berth::Connected) if worm.host == home => {
                    self.worm_at_home(id, worm)
                }
                VesselState::At(Berth::Lurking) => self.worm_outside(id, worm),
                _ => {}
            }
        }
    }

    fn worm_at_home(&mut self, id: VesselId, worm: &Vessel) {
        let citadel = &self.player.hideout.citadel;
        if worm.pilot.is_none() {
            if let Some(team) = citadel
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
        if worm.fuel < transport::Vessel::FUEL_CAPACITY {
            let amount = self
                .above
                .get(ItemType::ProxyChains)
                .min(Vessel::FUEL_CAPACITY - worm.fuel);
            if amount > 0 {
                self.above.take(ItemType::ProxyChains, amount);
                self.order(Command::Refuel { vessel: id, amount });
            }
        }
        let mut carrying = 0;
        let mut capacity = 0;
        for (slot, module) in worm.modules.iter().enumerate() {
            match module {
                Module::Empty if self.above.get(ItemType::ToolModule) > 0 => {
                    self.above.take(ItemType::ToolModule, 1);
                    self.order(Command::Fit {
                        vessel: id,
                        slot,
                        module: Some(ModuleKind::ToolModule),
                    });
                }
                Module::ToolModule(None) => {}
                Module::ToolModule(Some(_)) => {
                    capacity += 1;
                    carrying += 1;
                    continue;
                }
                _ => continue,
            }
            capacity += 1;
            if self.above.get(ItemType::CitadelModule) > 0 {
                self.above.take(ItemType::CitadelModule, 1);
                self.order(Command::Load {
                    vessel: id,
                    slot,
                    item: ItemType::CitadelModule,
                    count: 1,
                });
                carrying += 1;
            }
        }
        // Leave with a full load, or with all the target still lacks.
        let fuel = worm.fuel + self.refuelled(id);
        if let Some(target) = self.target() {
            let lacking = Citadel::MODULES - self.world.host(target).site.citadel.modules;
            if carrying > 0
                && carrying >= capacity.min(lacking)
                && fuel >= 2 * transport::latency(self.data, self.home, target) + 4
            {
                self.dispatch(id, target, Berth::Lurking);
            }
        }
    }

    /// Anonymisation this turn's orders add to a vessel.
    fn refuelled(&self, id: VesselId) -> u32 {
        self.commands
            .iter()
            .map(|c| match *c {
                Command::Refuel { vessel, amount } if vessel == id => amount,
                _ => 0,
            })
            .sum()
    }

    /// The host to build a citadel on next: one the crew has started, else
    /// the nearest free host in the home network.
    fn target(&self) -> Option<HostId> {
        let home_network = self.data.host(self.home).network;
        let candidates = self
            .data
            .hosts
            .iter()
            .enumerate()
            .filter_map(|(index, def)| {
                let id = HostId(index as u16);
                let state = &self.world.hosts[index];
                let usable = def.network == home_network
                    && id != self.home
                    && !def.cache_field
                    && !state.site.citadel.complete()
                    && match state.controller {
                        None => true,
                        Some(Controller::Crew(crew)) => crew == self.crew,
                        Some(Controller::Legacy) => false,
                    };
                usable.then_some((id, state.controller.is_some()))
            });
        candidates
            .min_by_key(|&(id, started)| {
                (!started, transport::latency(self.data, self.home, id), id)
            })
            .map(|(id, _)| id)
    }

    fn worm_outside(&mut self, id: VesselId, worm: &Vessel) {
        let state = &self.world.hosts[usize::from(worm.host.0)];
        let ours_or_free = matches!(state.controller, None | Some(Controller::Crew(_)))
            && state
                .controller
                .is_none_or(|c| c == Controller::Crew(self.crew))
            && worm.host != self.home
            && !self.data.host(worm.host).cache_field;
        if ours_or_free && worm.pilot.is_some() {
            let mut room = Citadel::MODULES.saturating_sub(state.site.citadel.modules);
            for (slot, module) in worm.modules.iter().enumerate() {
                if room == 0 {
                    break;
                }
                if let Module::ToolModule(Some(cargo)) = module
                    && cargo.item == ItemType::CitadelModule
                {
                    self.order(Command::Deploy { vessel: id, slot });
                    room -= 1;
                }
            }
        }
        self.dispatch(id, self.home, Berth::Connected);
    }
}
