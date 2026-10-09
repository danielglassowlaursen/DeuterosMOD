//! A bot that plays one crew from the state of the world alone.
//!
//! It grows the hideout's economy, builds the citadel above it, staffs and
//! supplies that citadel, builds a worm there and sends it out to claim free
//! hosts in the home network with citadel modules. It then works the best
//! of those hosts as colonies ([`colony`]), mines the cache field with a
//! sniffer, and, once it can build daemons, garrisons its citadels and
//! sends a warship against swarms, Legacy hosts and a runaway rival
//! ([`war`]). It is used to stress the rules over long games and fills
//! empty seats in a game.
//!
//! The bot keeps no memory between turns: it reads the world and gives the
//! orders that move each part of its plan one step on. It plans against
//! copies of the stores it draws from, so that it never gives an order the
//! rules would reject.

mod colony;
mod war;

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
use crate::unlocks::Milestone;
use crate::workshop::{AutoMode, SiteRef, Workshop, WorkshopRef};
use crate::world::{Controller, Player, World};

use colony::cargo_free;

/// Research in the order the bot wants it. Items the crew has not unlocked
/// yet are skipped until it has.
const RESEARCH: [ItemType; 18] = [
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
    ItemType::Sniffer,
    ItemType::BackdoorKit,
    ItemType::LegacyExploit,
    ItemType::Daemon,
    ItemType::C2Controller,
    ItemType::Crawler,
    ItemType::BuildBot,
    ItemType::Patch,
];

/// What a worm is for, read off its equipment: the first plain worm
/// builds citadels and runs supplies, one with a sniffer mines the cache
/// field, one with a C2 controller fights, and any other waits at home to
/// be fitted out.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Builder,
    Miner,
    Warship,
    Spare,
}

/// Resources the citadel's workshop and worms need from the hideout:
/// what it builds from, anonymisation, and the bandwidth and proxies its
/// workshop refines more anonymisation from.
const SUPPLY: [ItemType; 8] = [
    ItemType::Compute,
    ItemType::Storage,
    ItemType::Memory,
    ItemType::Code,
    ItemType::Credentials,
    ItemType::ProxyChains,
    ItemType::Bandwidth,
    ItemType::Proxies,
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
        taken_above: Vec::new(),
        commands: Vec::new(),
    };
    bot.recruit();
    bot.research();
    bot.install_taps();
    bot.ground_workshop();
    if player.hideout.citadel.complete() {
        bot.citadel_workshop();
        bot.colonies();
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
    /// Teams this turn's orders move out of the hideout citadel's staff
    /// slots, by their index before any of them left.
    taken_above: Vec<usize>,
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

    /// Whether a team of `kind` waits in the hideout citadel's staff slots
    /// that no order this turn has claimed yet.
    fn team_above(&self, kind: StaffKind) -> bool {
        self.player
            .hideout
            .citadel
            .staff
            .iter()
            .enumerate()
            .any(|(i, s)| s.kind == kind && !self.taken_above.contains(&i))
    }

    /// Claims a waiting team of `kind` in the hideout citadel for an order
    /// that moves it out, and returns the index that order must use once
    /// the teams earlier orders this turn moved out are gone.
    fn take_team_above(&mut self, kind: StaffKind) -> Option<usize> {
        let index = self
            .player
            .hideout
            .citadel
            .staff
            .iter()
            .enumerate()
            .position(|(i, s)| s.kind == kind && !self.taken_above.contains(&i))?;
        self.taken_above.push(index);
        Some(index - self.taken_above.iter().filter(|&&t| t < index).count())
    }

    /// Builders the crew wants: one, and a second once there are colonies
    /// to supply.
    fn builders_wanted(&self) -> u32 {
        1 + u32::from(!self.colony_hosts().is_empty())
    }

    /// Worms the crew wants: its builders, a miner once sniffers are
    /// known, a warship once C2 controllers are.
    fn worms_wanted(&self) -> u32 {
        self.builders_wanted()
            + u32::from(self.researched(ItemType::Sniffer))
            + u32::from(self.researched(ItemType::C2Controller))
    }

    /// What the hideout's dropper carries up: the supplies the citadel is
    /// short of, the shortest first; everything when nothing is short.
    fn supply_list(&self) -> Vec<ItemType> {
        let target = |item| match item {
            ItemType::ProxyChains => 300,
            ItemType::Bandwidth | ItemType::Proxies => 200,
            _ => 600,
        };
        let mut short: Vec<ItemType> = SUPPLY
            .into_iter()
            .filter(|&item| self.above.get(item) < target(item) && self.ground.get(item) > 0)
            .collect();
        short.sort_by_key(|&item| self.above.get(item) * 100 / target(item));
        if short.is_empty() {
            SUPPLY.to_vec()
        } else {
            short
        }
    }

    /// Operator teams the crew wants in all: the dropper's pilot, one per
    /// worm and one per colony.
    fn operator_teams_wanted(&self) -> usize {
        1 + self.worms_wanted() as usize + self.colony_hosts().len()
    }

    /// The team waiting in the hideout's slots that the dropper should carry
    /// up next: coders the citadel's workshop or a colony wants, else
    /// operators a worm or a colony wants. `None` when the citadel's slots
    /// are full.
    fn team_to_carry_up(&self) -> Option<usize> {
        let hideout = &self.player.hideout;
        if hideout.citadel.staff.len() >= STAFF_SLOTS {
            return None;
        }
        let waiting = |kind: StaffKind| hideout.staff.iter().position(|s| s.kind == kind);
        if self.coders_wanted_above() > self.staged_above(StaffKind::Coder)
            && let Some(team) = waiting(StaffKind::Coder)
        {
            return Some(team);
        }
        if self.operators_wanted_above() > self.staged_above(StaffKind::Operator)
            && let Some(team) = waiting(StaffKind::Operator)
        {
            return Some(team);
        }
        None
    }

    /// Teams of a kind waiting in the hideout citadel's staff slots.
    fn staged_above(&self, kind: StaffKind) -> usize {
        self.player
            .hideout
            .citadel
            .staff
            .iter()
            .filter(|s| s.kind == kind)
            .count()
    }

    /// Coder teams that should wait in the hideout citadel: one for its own
    /// workshop while coders run it, and the colonies' teams.
    fn coders_wanted_above(&self) -> usize {
        let workshop = &self.player.hideout.citadel.workshop;
        usize::from(!workshop.automated && workshop.coders.is_none())
            + self.teams_needed(StaffKind::Coder)
    }

    /// Operator teams that should wait in the hideout citadel: for worms
    /// without a pilot and for colonies without one.
    fn operators_wanted_above(&self) -> usize {
        let piloted = self
            .vessels(VesselKind::Worm)
            .filter(|(_, v)| v.pilot.is_some())
            .count();
        (self.worms_wanted() as usize).saturating_sub(piloted)
            + self.teams_needed(StaffKind::Operator)
    }

    /// Plans fitting `kind` in a slot of a docked vessel from the bay's
    /// store (planned as `store`); returns whether the slot holds that
    /// kind once this turn's orders apply.
    fn fit_from(
        &mut self,
        id: VesselId,
        vessel: &Vessel,
        slot: usize,
        kind: ModuleKind,
        store: &mut Store,
    ) -> bool {
        let pod = match kind {
            ModuleKind::DataContainer => ItemType::DataContainer,
            ModuleKind::ToolModule => ItemType::ToolModule,
            ModuleKind::SessionPod => ItemType::SessionPod,
        };
        let current = vessel.modules[slot].pod();
        if current == Some(pod) {
            return true;
        }
        if store.get(pod) == 0 || self.swaps_this_turn(id, slot) {
            return false;
        }
        store.take(pod, 1);
        if let Some(old) = current {
            store.add(old, 1);
        }
        self.order(Command::Fit {
            vessel: id,
            slot,
            module: Some(kind),
        });
        true
    }

    /// Whether this turn's orders already refit the slot; a second swap
    /// would put the first pod's cargo in the wrong module.
    fn swaps_this_turn(&self, id: VesselId, slot: usize) -> bool {
        self.commands.iter().any(
            |c| matches!(c, Command::Fit { vessel, slot: s, .. } if *vessel == id && *s == slot),
        )
    }

    /// [`Bot::fit_from`] for a worm at the hideout citadel; `freed` lists
    /// the slots this turn's orders empty first.
    fn fit_above(
        &mut self,
        id: VesselId,
        worm: &Vessel,
        slot: usize,
        kind: ModuleKind,
        freed: &[usize],
    ) -> bool {
        if !cargo_free(&worm.modules[slot], slot, freed) {
            return false;
        }
        let mut above = std::mem::take(&mut self.above);
        let fitted = self.fit_from(id, worm, slot, kind, &mut above);
        self.above = above;
        fitted
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
                // Teams wait in the hideout's slots for the dropper to
                // carry them up; two waiting is plenty.
                StaffKind::Operator
                    if operator_teams < self.operator_teams_wanted()
                        && self
                            .player
                            .hideout
                            .staff
                            .iter()
                            .filter(|s| s.kind == StaffKind::Operator)
                            .count()
                            < 2 =>
                {
                    20
                }
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
        // The hideout's own dropper; the colonies' droppers are theirs.
        let home_dropper = self.dropper_at(self.home);
        let has_dropper = home_dropper.is_some();
        let dropper_has = |item: ItemType| {
            home_dropper.is_some_and(|(_, v)| match item {
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

    /// Puts the wants to a workshop: all of them queued on its build-bot,
    /// daemons and citadel modules on repeat, or the first one its coders
    /// can start. Queued items no longer wanted are dropped.
    fn produce(
        &mut self,
        at: WorkshopRef,
        workshop: &Workshop,
        level: u8,
        ground: bool,
        store: &mut Store,
        wants: &[ItemType],
    ) {
        if workshop.automated {
            for job in &workshop.jobs {
                if !wants.contains(&job.item) && (job.repeat || !job.active) {
                    self.order(Command::Automate {
                        at,
                        item: job.item,
                        mode: AutoMode::Stop,
                    });
                }
            }
            for &item in wants {
                let def = &self.data.items[&item];
                if workshop.jobs.iter().any(|j| j.item == item)
                    || !self.researched(item)
                    || (def.orbit_only && ground)
                    || def.recipe.is_empty()
                {
                    continue;
                }
                let mode = if matches!(item, ItemType::Daemon | ItemType::CitadelModule) {
                    AutoMode::Repeat
                } else {
                    AutoMode::Once
                };
                self.order(Command::Automate { at, item, mode });
            }
            return;
        }
        if workshop.jobs.iter().any(|j| j.active) {
            return;
        }
        if let Some(item) = self.pick_build(workshop, level, ground, store, wants) {
            self.order(Command::Build { at, item });
        }
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

    /// Whether the hideout's dropper runs supplies up by script.
    fn script_running(&self) -> bool {
        self.dropper_at(self.home)
            .is_some_and(|(_, v)| v.script.as_ref().is_some_and(|s| s.running()))
    }

    /// Coders or a build-bot run the citadel's workshop and an operator team
    /// waits there.
    fn citadel_staffed(&self) -> bool {
        let citadel = &self.player.hideout.citadel;
        (citadel.workshop.coders.is_some() || citadel.workshop.automated)
            && (citadel.staff.iter().any(|s| s.kind == StaffKind::Operator)
                || self
                    .vessels(VesselKind::Worm)
                    .any(|(_, v)| v.pilot.is_some()))
    }

    // ------------------------------------------------------------ dropper

    fn dropper(&mut self) {
        let home = self.home;
        let Some((id, dropper)) = self.dropper_at(home) else {
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
        if let Some(route) = dropper.script.as_ref().and_then(|s| s.route.as_ref()) {
            // A team waiting to go up interrupts the supply run; the run
            // starts again once the dropper has carried it.
            if self.team_to_carry_up().is_some() {
                self.order(Command::ConfigureScript {
                    vessel: id,
                    route: None,
                });
                return;
            }
            // Point the running supply run at what the citadel is short of.
            let outbound = self.supply_list();
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
        if dropper.destination.is_some() {
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

        // Staff the citadel and the colonies: teams ride up in a session
        // pod, one a trip.
        if let Some(team) = self.team_to_carry_up() {
            if self.fit(id, dropper, ModuleKind::SessionPod) {
                self.board_pod(id, team, up);
            }
            return;
        }
        // Coders leave the hideout workshop for the citadel once they can
        // build worms, or for a colony at any level; new recruits take over
        // the hideout workshop.
        let coders_short = self.coders_wanted_above() > self.staged_above(StaffKind::Coder)
            && !hideout.staff.iter().any(|s| s.kind == StaffKind::Coder);
        let level_needed = if self.teams_needed(StaffKind::Coder) > 0 {
            1
        } else {
            2
        };
        if coders_short
            && let Some(coders) = &self.player.workshop.coders
            && coders.level() >= level_needed
            && hideout.staff.len() < STAFF_SLOTS
            && !self.player.workshop.jobs.iter().any(|j| j.active)
        {
            self.order(Command::ReleaseCoders {
                at: WorkshopRef::Hideout,
            });
            return;
        }
        if !self.citadel_staffed() {
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
                        outbound: self.supply_list(),
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

    /// Fits the hideout dropper's slot with `kind` from the hideout's store
    /// if it is not already; returns whether the slot holds `kind` once this
    /// turn's orders are applied.
    fn fit(&mut self, id: VesselId, dropper: &Vessel, kind: ModuleKind) -> bool {
        let mut ground = std::mem::take(&mut self.ground);
        let fitted = self.fit_from(id, dropper, 0, kind, &mut ground);
        self.ground = ground;
        fitted
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
        let mut unloaded = false;
        match &dropper.modules[0] {
            Module::DataContainer(Some(_)) | Module::ToolModule(Some(_)) => {
                self.order(Command::Unload {
                    vessel: id,
                    slot: 0,
                });
                unloaded = true;
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
        // An exfil script the citadel built for this dropper goes down with
        // it, to be installed inside.
        if dropper.script.is_none()
            && self.ground.get(ItemType::ExfilScript) == 0
            && self.above.get(ItemType::ExfilScript) > 0
            && (unloaded || dropper.modules[0].pod() != Some(ItemType::SessionPod))
        {
            let mut above = std::mem::take(&mut self.above);
            let fitted = self.fit_from(id, dropper, 0, ModuleKind::ToolModule, &mut above);
            self.above = above;
            if fitted {
                self.above.take(ItemType::ExfilScript, 1);
                self.order(Command::Load {
                    vessel: id,
                    slot: 0,
                    item: ItemType::ExfilScript,
                    count: 1,
                });
            }
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
        let automated = citadel.workshop.automated;
        let level = match &citadel.workshop.coders {
            Some(coders) => coders.level(),
            None if automated => 0,
            None => {
                if let Some(team) = self.take_team_above(StaffKind::Coder) {
                    self.order(Command::AssignCoders { at, team });
                }
                return;
            }
        };
        if !automated && citadel.workshop.jobs.iter().any(|j| j.active) {
            return;
        }
        let worms = self.vessels(VesselKind::Worm).count() as u32;
        let worm_modules: u32 = self
            .vessels(VesselKind::Worm)
            .filter(|(_, v)| self.role(v) == Role::Builder)
            .map(|(_, v)| {
                v.modules
                    .iter()
                    .filter(|m| m.pod() == Some(ItemType::ToolModule))
                    .count() as u32
            })
            .sum();
        let mut wants = Vec::new();
        // A build-bot doubles the workshop's pace and frees its coders.
        if !automated
            && self.researched(ItemType::BuildBot)
            && self.above.get(ItemType::Certificates) > 0
        {
            wants.push(ItemType::BuildBot);
        }
        let cores = self
            .above
            .get(ItemType::WormCore)
            .min(self.above.get(ItemType::WormEngine));
        if worms + cores < self.worms_wanted() {
            for part in [ItemType::WormCore, ItemType::WormEngine] {
                if self.above.get(part) == 0 {
                    wants.push(part);
                }
            }
        }
        if self.researched(ItemType::Sniffer)
            && self.above.get(ItemType::Sniffer) == 0
            && !self.has_role(Role::Miner)
        {
            wants.push(ItemType::Sniffer);
        }
        // The hideout's dropper runs supplies by script; its workshop may
        // not have the hands to build one yet.
        if self
            .dropper_at(self.home)
            .is_some_and(|(_, d)| d.script.is_none())
            && self.ground.get(ItemType::ExfilScript) == 0
            && self.above.get(ItemType::ExfilScript) == 0
        {
            wants.push(ItemType::ExfilScript);
        }
        self.colony_wants(&mut wants);
        self.war_wants(&mut wants);
        if worm_modules + self.above.get(ItemType::ToolModule) < 3 * self.builders_wanted() {
            wants.push(ItemType::ToolModule);
        }
        // A pod of each kind in stock, so the other worms can refit.
        if worms > 1 {
            for pod in [
                ItemType::ToolModule,
                ItemType::DataContainer,
                ItemType::SessionPod,
            ] {
                if self.above.get(pod) == 0 && !wants.contains(&pod) {
                    wants.push(pod);
                }
            }
        }
        // A load of modules for every builder, counting what they carry.
        if worms > 0
            && self.above.get(ItemType::CitadelModule) + self.carried(ItemType::CitadelModule)
                < MODULES_IN_STOCK * self.builders_wanted()
        {
            wants.push(ItemType::CitadelModule);
        }
        let workshop = &citadel.workshop;
        let mut above = std::mem::take(&mut self.above);
        self.produce(at, workshop, level, false, &mut above, &wants);
        self.above = above;
    }

    // ------------------------------------------------------------ worms

    /// A worm's role, from its equipment: see [`Role`]. The plain worms
    /// are builders up to the number wanted, in the order they were built.
    fn role(&self, worm: &Vessel) -> Role {
        if worm.c2 {
            return Role::Warship;
        }
        if carries(worm, ItemType::Sniffer) {
            return Role::Miner;
        }
        let rank = self
            .vessels(VesselKind::Worm)
            .filter(|(_, v)| !v.c2 && !carries(v, ItemType::Sniffer))
            .position(|(_, v)| std::ptr::eq(v, worm));
        match rank {
            Some(rank) if (rank as u32) < self.builders_wanted() => Role::Builder,
            _ => Role::Spare,
        }
    }

    fn has_role(&self, role: Role) -> bool {
        self.vessels(VesselKind::Worm)
            .any(|(_, v)| self.role(v) == role)
    }

    fn worms(&mut self) {
        let home = self.home;
        let worms: Vec<(VesselId, &Vessel)> = self.vessels(VesselKind::Worm).collect();
        // A new worm needs a pilot and anonymisation waiting for it, or it
        // would sit on the port with no way to leave.
        if (worms.len() as u32) < self.worms_wanted()
            && self.above.get(ItemType::WormCore) > 0
            && self.above.get(ItemType::WormEngine) > 0
            && self.team_above(StaffKind::Operator)
            && self.above.get(ItemType::ProxyChains) >= 50
        {
            self.above.take(ItemType::WormCore, 1);
            self.above.take(ItemType::WormEngine, 1);
            self.order(Command::Assemble {
                host: home,
                berth: Berth::Connected,
                kind: VesselKind::Worm,
            });
        }
        // Worms share the citadel's single port. One with nothing to do at
        // home makes way for one that has, and waits outside until it has
        // business there itself.
        let mut port_claimed = false;
        for (id, worm) in worms {
            if worm.destination.is_some() {
                continue;
            }
            let role = self.role(worm);
            match (worm.state, role) {
                (VesselState::At(Berth::Connected), _) if worm.host == home => {
                    let before = self.commands.len();
                    match role {
                        Role::Builder => self.builder_at_home(id, worm),
                        Role::Miner => self.miner_at_home(id, worm),
                        Role::Warship => self.warship_at_home(id, worm),
                        Role::Spare => self.spare_at_home(id, worm),
                    }
                    let idle = !self.commands[before..]
                        .iter()
                        .any(|c| !matches!(c, Command::Refuel { .. }) && c.vessel() == Some(id));
                    if idle && self.port_wanted(id) && worm.fuel + self.refuelled(id) >= 5 {
                        self.dispatch(id, home, Berth::Lurking);
                    }
                }
                (VesselState::At(Berth::Connected), _) => self.worm_at_colony(id, worm),
                (VesselState::At(Berth::Lurking), _) if worm.host == home => {
                    if !port_claimed && worm.fuel > 0 && self.wants_in(role, worm) {
                        port_claimed = true;
                        self.dispatch(id, home, Berth::Connected);
                    }
                }
                (VesselState::At(Berth::Lurking), Role::Miner) => self.miner_outside(id, worm),
                (VesselState::At(Berth::Lurking), Role::Warship) => self.warship_outside(id, worm),
                (VesselState::At(Berth::Lurking), _) => self.worm_outside(id, worm),
                _ => {}
            }
        }
    }

    /// Whether another worm of the crew's waits for the home port: one on
    /// its way in, or one outside with business at the citadel.
    fn port_wanted(&self, except: VesselId) -> bool {
        let home = self.home;
        self.vessels(VesselKind::Worm).any(|(id, v)| {
            id != except
                && match v.state {
                    VesselState::Connecting => v.host == home,
                    VesselState::At(Berth::Lurking) => {
                        v.host == home && self.wants_in(self.role(v), v)
                    }
                    _ => v
                        .destination
                        .is_some_and(|d| d.host == home && d.berth == Berth::Connected),
                }
        })
    }

    /// Whether a worm outside the home citadel has business inside it.
    fn wants_in(&self, role: Role, worm: &Vessel) -> bool {
        if worm.pilot.is_none() {
            return true;
        }
        match role {
            Role::Builder => true,
            Role::Miner => {
                let field = self.cache_field();
                let trip = field.map_or(0, |f| 2 * transport::latency(self.data, self.home, f) + 4);
                worm.fuel < trip || worm.modules.iter().any(|m| {
                    matches!(m, Module::DataContainer(Some(_)))
                        || matches!(m, Module::ToolModule(Some(c)) if c.item != ItemType::Sniffer)
                }) || !self.miner_equipped(worm)
            }
            Role::Warship => {
                worm.fuel < 100
                    || (worm.daemons < Vessel::DAEMON_CAPACITY
                        && self.above.get(ItemType::Daemon) > 0)
            }
            Role::Spare => {
                (!self.has_role(Role::Miner)
                    && self.above.get(ItemType::Sniffer) > 0
                    && (worm.modules[0].pod() == Some(ItemType::ToolModule)
                        || self.above.get(ItemType::ToolModule) > 0))
                    || (!self.has_role(Role::Warship) && self.above.get(ItemType::C2Controller) > 0)
            }
        }
    }

    /// Whether the miner's slots are as it wants them: the sniffer, a tool
    /// module for a source fragment until one is found, containers for the
    /// rest.
    fn miner_equipped(&self, worm: &Vessel) -> bool {
        let fragment_wanted = !self.player.milestones.contains(&Milestone::SourceCode);
        worm.modules.iter().enumerate().all(|(slot, m)| match slot {
            0 => carries(worm, ItemType::Sniffer),
            1 if fragment_wanted => m.pod() == Some(ItemType::ToolModule),
            _ => m.pod() == Some(ItemType::DataContainer),
        })
    }

    /// Boards a pilot and tops up a worm's anonymisation at home to `cap`;
    /// false until it has a pilot.
    fn worm_ready(&mut self, id: VesselId, worm: &Vessel, cap: u32) -> bool {
        if worm.pilot.is_none() {
            if let Some(team) = self.take_team_above(StaffKind::Operator) {
                self.order(Command::Board {
                    vessel: id,
                    seat: Seat::Pilot,
                    team,
                });
            }
            return false;
        }
        if worm.fuel < cap {
            let amount = self.above.get(ItemType::ProxyChains).min(cap - worm.fuel);
            if amount > 0 {
                self.above.take(ItemType::ProxyChains, amount);
                self.order(Command::Refuel { vessel: id, amount });
            }
        }
        true
    }

    /// Empties a worm at home of what it brought back: cargo into the
    /// citadel's store, teams into its staff slots. Returns the slots
    /// freed.
    fn unload_at_home(&mut self, id: VesselId, worm: &Vessel) -> Vec<usize> {
        let mut room = STAFF_SLOTS.saturating_sub(self.player.hideout.citadel.staff.len());
        let mut freed = Vec::new();
        for (slot, module) in worm.modules.iter().enumerate() {
            match module {
                // A store at its cap keeps nothing more; the cargo would
                // stay aboard and the slot would not be free.
                Module::DataContainer(Some(c))
                    if self.above.get(c.item) + c.count <= Store::CAP =>
                {
                    self.above.add(c.item, c.count);
                    self.order(Command::Unload { vessel: id, slot });
                    freed.push(slot);
                }
                // Citadel modules and the sniffer stay aboard: they are
                // the worm's own equipment.
                Module::ToolModule(Some(c))
                    if c.item != ItemType::CitadelModule
                        && c.item != ItemType::Sniffer
                        && self.above.get(c.item) + c.count <= Store::CAP =>
                {
                    self.above.add(c.item, c.count);
                    self.order(Command::Unload { vessel: id, slot });
                    freed.push(slot);
                }
                Module::SessionPod(Some(_)) if room > 0 => {
                    room -= 1;
                    self.order(Command::Disembark {
                        vessel: id,
                        seat: Seat::Pod(slot),
                    });
                    freed.push(slot);
                }
                _ => {}
            }
        }
        freed
    }

    /// The builder at home: garrison runs and colony supplies first, then
    /// collecting what the colonies' taps found, then claiming hosts with
    /// citadel modules.
    fn builder_at_home(&mut self, id: VesselId, worm: &Vessel) {
        if !self.worm_ready(id, worm, 120) {
            return;
        }
        let freed = self.unload_at_home(id, worm);
        if self.garrison_run(id, worm) {
            return;
        }
        let fuel = worm.fuel + self.refuelled(id);
        let round_trip = |bot: &Self, host| 2 * transport::latency(bot.data, bot.home, host) + 4;
        if let Some(host) = self.supply_target()
            && fuel >= round_trip(self, host)
            && self.load_for_colony(id, worm, host, &freed)
        {
            self.dispatch(id, host, Berth::Connected);
            return;
        }
        if let Some(host) = self.collect_target()
            && fuel >= round_trip(self, host)
        {
            let container = (0..worm.modules.len())
                .any(|slot| self.fit_above(id, worm, slot, ModuleKind::DataContainer, &freed));
            if container {
                self.dispatch(id, host, Berth::Connected);
                return;
            }
        }

        let mut carrying = 0;
        let mut capacity = 0;
        for (slot, module) in worm.modules.iter().enumerate() {
            if let Module::ToolModule(Some(_)) = module
                && !freed.contains(&slot)
            {
                capacity += 1;
                carrying += 1;
                continue;
            }
            if !self.fit_above(id, worm, slot, ModuleKind::ToolModule, &freed) {
                continue;
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
        if let Some(target) = self.target() {
            let lacking = Citadel::MODULES - self.world.host(target).site.citadel.modules;
            if carrying > 0 && carrying >= capacity.min(lacking) && fuel >= round_trip(self, target)
            {
                self.dispatch(id, target, Berth::Lurking);
            }
        }
    }

    /// A worm with no role yet is fitted out as the miner or the warship
    /// as soon as the citadel has a sniffer or a C2 controller for it.
    fn spare_at_home(&mut self, id: VesselId, worm: &Vessel) {
        if !self.worm_ready(id, worm, 60) {
            return;
        }
        let freed = self.unload_at_home(id, worm);
        let has_miner = self.has_role(Role::Miner);
        let has_warship = self.has_role(Role::Warship);
        if !has_miner
            && self.above.get(ItemType::Sniffer) > 0
            && self.fit_above(id, worm, 0, ModuleKind::ToolModule, &freed)
        {
            self.above.take(ItemType::Sniffer, 1);
            self.order(Command::Load {
                vessel: id,
                slot: 0,
                item: ItemType::Sniffer,
                count: 1,
            });
        } else if !has_warship && self.above.get(ItemType::C2Controller) > 0 {
            self.above.take(ItemType::C2Controller, 1);
            self.order(Command::InstallC2 { vessel: id });
        }
    }

    /// The nearest cache field in the home network.
    fn cache_field(&self) -> Option<HostId> {
        let network = self.data.host(self.home).network;
        self.data
            .hosts
            .iter()
            .enumerate()
            .filter(|(_, def)| def.network == network && def.cache_field)
            .map(|(h, _)| HostId(h as u16))
            .min_by_key(|&h| transport::latency(self.data, self.home, h))
    }

    /// The miner at home: unloads its finds, keeps its sniffer in slot 0,
    /// an empty tool module for a source fragment until one is found and
    /// data containers in the rest, and goes back out to the cache field.
    fn miner_at_home(&mut self, id: VesselId, worm: &Vessel) {
        if !self.worm_ready(id, worm, 80) {
            return;
        }
        let freed = self.unload_at_home(id, worm);
        let fragment_wanted = !self.player.milestones.contains(&Milestone::SourceCode);
        for slot in 1..worm.modules.len() {
            let kind = if slot == 1 && fragment_wanted {
                ModuleKind::ToolModule
            } else {
                ModuleKind::DataContainer
            };
            self.fit_above(id, worm, slot, kind, &freed);
        }
        let Some(field) = self.cache_field() else {
            return;
        };
        let fuel = worm.fuel + self.refuelled(id);
        if fuel >= 2 * transport::latency(self.data, self.home, field) + 4 {
            self.dispatch(id, field, Berth::Lurking);
        }
    }

    /// The miner out on the net: it scans at the cache field until its
    /// containers are spoken for or it has a fragment, then heads home.
    fn miner_outside(&mut self, id: VesselId, worm: &Vessel) {
        if self.data.host(worm.host).cache_field {
            let containers = worm
                .modules
                .iter()
                .filter(|m| matches!(m, Module::DataContainer(_)))
                .count();
            let filled = worm
                .modules
                .iter()
                .filter(|m| matches!(m, Module::DataContainer(Some(_))))
                .count();
            let full = worm.modules.iter().any(|m| {
                matches!(m, Module::DataContainer(Some(c)) if c.count >= Module::CONTAINER_CAPACITY)
            });
            let fragment = carries(worm, ItemType::SourceFragment);
            if (filled < containers && !full && !fragment)
                || worm.fuel < transport::latency(self.data, worm.host, self.home) + 2
            {
                return;
            }
        }
        if worm.fuel > 0 {
            self.dispatch(id, self.home, Berth::Connected);
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
    /// the nearest free host in the home network, with the ones worth
    /// working as colonies counting as nearer. The citadel that would bring
    /// the Legacy Net down on the crew waits until the crew is armed.
    fn target(&self) -> Option<HostId> {
        let home_network = self.data.host(self.home).network;
        let citadels = self.held_citadels().len() + usize::from(self.citadel_complete());
        if self.player.war.is_none()
            && citadels + 1 >= crate::legacy::CITADELS_FOR_WAR
            && !self.armed_for_war()
        {
            return None;
        }
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
                let latency = transport::latency(self.data, self.home, id);
                (
                    !started,
                    latency.saturating_sub(2 * colony::value(self.data.host(id))),
                    id,
                )
            })
            .map(|(id, _)| id)
    }

    /// The builder (or a spare worm) out on the net: it installs the citadel
    /// modules it carries where it may and heads home.
    fn worm_outside(&mut self, id: VesselId, worm: &Vessel) {
        let state = &self.world.hosts[usize::from(worm.host.0)];
        let ours_or_free = matches!(state.controller, None | Some(Controller::Crew(_)))
            && state
                .controller
                .is_none_or(|c| c == Controller::Crew(self.crew))
            && worm.host != self.home
            && !self.data.host(worm.host).cache_field;
        // Nothing to do at a host under siege but get away from it.
        let besieged = crate::legacy::under_siege(self.world, worm.host);
        if ours_or_free && worm.pilot.is_some() && !besieged {
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
        if worm.fuel > 0 {
            self.dispatch(id, self.home, Berth::Connected);
        }
    }
}

/// Whether a vessel has `tool` loaded in a tool module.
fn carries(vessel: &Vessel, tool: ItemType) -> bool {
    vessel
        .modules
        .iter()
        .any(|m| matches!(m, Module::ToolModule(Some(c)) if c.item == tool))
}
