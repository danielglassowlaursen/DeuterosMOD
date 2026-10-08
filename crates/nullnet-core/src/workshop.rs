//! Workshops build items from recipes. Port of `Factory.IncrementCurrentProd`
//! (Godot/Code/Factory.cs) and `Production.UpdateProduction`
//! (Godot/Code/Platform/Screens/Production.cs).
//!
//! A crew has one workshop in its hideout and one in every complete citadel
//! it holds. A workshop is either run by a coder team, which builds one item
//! at a time as the crew orders, or by a build-bot, which works through a
//! queue on its own.

use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;

use crate::command::CommandError;
use crate::data::{GameData, ItemCategory};
use crate::ids::{Day, HostId, PlayerId};
use crate::items::ItemType;
use crate::research::ResearchProgress;
use crate::site::Citadel;
use crate::staff::{Staff, StaffKind};
use crate::store::Store;
use crate::turn::Event;
use crate::world::{Controller, Player, World};

type Research = BTreeMap<ItemType, ResearchProgress>;

/// Where a workshop is: in the hideout, or in the citadel on a site.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkshopRef {
    Hideout,
    Citadel(SiteRef),
}

/// A crew's site: its hideout or a host it controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SiteRef {
    Hideout,
    Host(HostId),
}

/// How a build-bot treats a queued item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoMode {
    /// Build it once, then drop it from the queue.
    Once,
    /// Keep building it, taking turns with the other queued items.
    Repeat,
    /// Finish the one in progress, if any, and drop it from the queue.
    Stop,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workshop {
    pub coders: Option<Staff>,
    /// Run by a build-bot instead of coders.
    pub automated: bool,
    pub jobs: Vec<Job>,
}

/// An item being built, or waiting in the queue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub item: ItemType,
    /// Daily accumulator; every time it passes 255 the job moves a stage on.
    pub value: u32,
    /// 1 to [`Job::STAGES`]; the item is built when it reaches the last.
    pub stage: u32,
    pub active: bool,
    /// For build-bots: keep building this item after it is done.
    pub repeat: bool,
}

impl Job {
    pub const STAGES: u32 = 4;

    fn new(data: &GameData, item: ItemType, active: bool, repeat: bool) -> Job {
        Job {
            item,
            value: initial_value(data, item),
            stage: 1,
            active,
            repeat,
        }
    }
}

fn initial_value(data: &GameData, item: ItemType) -> u32 {
    data.research.get(&item).map_or(64, |r| r.initial_value)
}

fn has_recipe(data: &GameData, store: &Store, item: ItemType) -> bool {
    data.items[&item]
        .recipe
        .iter()
        .all(|&(input, count)| store.get(input) >= count)
}

fn consume_recipe(data: &GameData, store: &mut Store, item: ItemType) -> bool {
    if !has_recipe(data, store, item) {
        return false;
    }
    for &(input, count) in &data.items[&item].recipe {
        store.take(input, count);
    }
    true
}

/// A workshop together with the store it builds from and into, and the
/// staff slots of its site.
pub(crate) struct Bench<'a> {
    pub at: WorkshopRef,
    pub workshop: &'a mut Workshop,
    pub store: &'a mut Store,
    pub staff: &'a mut Vec<Staff>,
    /// The hideout workshop cannot build orbit-only items.
    pub ground: bool,
}

/// Finds the workshop a command names, with the crew's research.
pub(crate) fn locate(
    world: &mut World,
    id: PlayerId,
    at: WorkshopRef,
) -> Result<(Bench<'_>, &Research), CommandError> {
    let World { players, hosts, .. } = world;
    let unknown = CommandError::UnknownPlayer(id);
    match at {
        WorkshopRef::Hideout => {
            let Player {
                workshop,
                hideout,
                research,
                ..
            } = players.get_mut(&id).ok_or(unknown)?;
            let bench = Bench {
                at,
                workshop,
                store: &mut hideout.store,
                staff: &mut hideout.staff,
                ground: true,
            };
            Ok((bench, research))
        }
        WorkshopRef::Citadel(SiteRef::Hideout) => {
            let Player {
                hideout, research, ..
            } = players.get_mut(&id).ok_or(unknown)?;
            Ok((citadel_bench(at, &mut hideout.citadel)?, research))
        }
        WorkshopRef::Citadel(SiteRef::Host(host)) => {
            let player = players.get(&id).ok_or(unknown)?;
            let state = hosts
                .get_mut(usize::from(host.0))
                .ok_or(CommandError::UnknownHost(host))?;
            if state.controller != Some(Controller::Crew(id)) {
                return Err(CommandError::NotYourHost(host));
            }
            Ok((
                citadel_bench(at, &mut state.site.citadel)?,
                &player.research,
            ))
        }
    }
}

fn citadel_bench(at: WorkshopRef, citadel: &mut Citadel) -> Result<Bench<'_>, CommandError> {
    if !citadel.complete() {
        return Err(CommandError::NoCitadel);
    }
    let Citadel {
        workshop,
        store,
        staff,
        ..
    } = citadel;
    Ok(Bench {
        at,
        workshop,
        store,
        staff,
        ground: false,
    })
}

/// Runs a day in every crew workshop: each hideout's, then every complete
/// citadel a crew holds.
pub(crate) fn run_all(data: &GameData, world: &mut World, events: &mut Vec<Event>) {
    let day = world.day;
    let World { players, hosts, .. } = world;
    for (&id, player) in players.iter_mut() {
        let Player {
            workshop,
            hideout,
            research,
            ..
        } = player;
        let bench = Bench {
            at: WorkshopRef::Hideout,
            workshop,
            store: &mut hideout.store,
            staff: &mut hideout.staff,
            ground: true,
        };
        run_day(data, research, day, id, bench, events);
        let at = WorkshopRef::Citadel(SiteRef::Hideout);
        if let Ok(bench) = citadel_bench(at, &mut hideout.citadel) {
            run_day(data, research, day, id, bench, events);
        }
    }
    for (index, host) in hosts.iter_mut().enumerate() {
        let Some(Controller::Crew(id)) = host.controller else {
            continue;
        };
        let Some(player) = players.get(&id) else {
            continue;
        };
        let at = WorkshopRef::Citadel(SiteRef::Host(HostId(index as u16)));
        if let Ok(bench) = citadel_bench(at, &mut host.site.citadel) {
            run_day(data, &player.research, day, id, bench, events);
        }
    }
}

/// Puts a coder team from the site's staff slots in charge of a workshop
/// without coders.
pub(crate) fn assign_coders(bench: Bench<'_>, team: usize) -> Result<(), CommandError> {
    if bench.workshop.automated {
        return Err(CommandError::WorkshopAutomated);
    }
    if bench.workshop.coders.is_some() {
        return Err(CommandError::SeatTaken);
    }
    match bench.staff.get(team) {
        None => Err(CommandError::NoSuchTeam),
        Some(staff) if staff.kind != StaffKind::Coder => Err(CommandError::NotCoders),
        Some(_) => {
            bench.workshop.coders = Some(bench.staff.remove(team));
            Ok(())
        }
    }
}

/// Moves a workshop's coders to the site's staff slots, ready to travel.
pub(crate) fn release_coders(bench: Bench<'_>) -> Result<(), CommandError> {
    if bench.staff.len() >= crate::site::STAFF_SLOTS {
        return Err(CommandError::NoStaffSlot);
    }
    let coders = bench.workshop.coders.take().ok_or(CommandError::NoCoders)?;
    bench.staff.push(coders);
    Ok(())
}

/// Checks that `item` is something this crew can build at all.
fn check_buildable(
    data: &GameData,
    research: &Research,
    item: ItemType,
    ground: bool,
) -> Result<(), CommandError> {
    let def = &data.items[&item];
    if def.category == ItemCategory::Resource || def.recipe.is_empty() {
        return Err(CommandError::NotBuildable(item));
    }
    if !research.get(&item).is_some_and(|r| r.researched) {
        return Err(CommandError::NotResearched(item));
    }
    if def.orbit_only && ground {
        return Err(CommandError::OrbitOnly(item));
    }
    Ok(())
}

/// Puts the coders of a workshop to work on `item`, as clicking an item in
/// the original's production screen did. A job already in the queue
/// resumes; a new one takes its recipe from the store now. The job it
/// replaces is paused and loses its partial stage.
pub(crate) fn build(
    data: &GameData,
    research: &Research,
    bench: Bench<'_>,
    item: ItemType,
) -> Result<(), CommandError> {
    if bench.workshop.automated {
        return Err(CommandError::WorkshopAutomated);
    }
    let level = match &bench.workshop.coders {
        Some(coders) if coders.count > 0 => coders.level(),
        _ => return Err(CommandError::NoCoders),
    };
    check_buildable(data, research, item, bench.ground)?;
    let needed = data.research[&item].tech_level;
    if needed > level {
        return Err(CommandError::TeamLevelTooLow { item, needed });
    }

    let jobs = &mut bench.workshop.jobs;
    if jobs.iter().any(|job| job.active && job.item == item) {
        return Ok(());
    }
    let queued = jobs.iter().position(|job| job.item == item);
    if queued.is_none() && !consume_recipe(data, bench.store, item) {
        return Err(CommandError::MissingResources(item));
    }
    for job in jobs.iter_mut().filter(|job| job.active) {
        job.active = false;
        job.value = initial_value(data, job.item);
    }
    match queued {
        Some(index) => jobs[index].active = true,
        None => jobs.push(Job::new(data, item, true, false)),
    }
    Ok(())
}

/// Adds, changes or stops an item in a build-bot's queue.
pub(crate) fn automate(
    data: &GameData,
    research: &Research,
    bench: Bench<'_>,
    item: ItemType,
    mode: AutoMode,
) -> Result<(), CommandError> {
    if !bench.workshop.automated {
        return Err(CommandError::WorkshopNotAutomated);
    }
    check_buildable(data, research, item, bench.ground)?;
    let jobs = &mut bench.workshop.jobs;
    let queued = jobs.iter().position(|job| job.item == item);
    match (mode, queued) {
        (AutoMode::Stop, Some(index)) if jobs[index].active => jobs[index].repeat = false,
        (AutoMode::Stop, Some(index)) => {
            jobs.remove(index);
        }
        (AutoMode::Stop, None) => {}
        (_, Some(index)) => jobs[index].repeat = mode == AutoMode::Repeat,
        (_, None) => jobs.push(Job::new(data, item, false, mode == AutoMode::Repeat)),
    }
    Ok(())
}

/// One day in one workshop.
pub(crate) fn run_day(
    data: &GameData,
    research: &Research,
    day: Day,
    player: PlayerId,
    bench: Bench<'_>,
    events: &mut Vec<Event>,
) {
    let Bench {
        at,
        workshop,
        store,
        staff,
        ground,
    } = bench;

    if workshop.automated
        && !workshop.jobs.iter().any(|job| job.active)
        && let Some(job) = workshop
            .jobs
            .iter_mut()
            .find(|job| has_recipe(data, store, job.item))
    {
        consume_recipe(data, store, job.item);
        job.active = true;
    }

    if let Some(index) = workshop.jobs.iter().position(|job| job.active) {
        let item = workshop.jobs[index].item;
        // Coders scale with team size and level; a build-bot works at a fixed pace.
        let gain = if workshop.automated {
            128
        } else {
            workshop.coders.as_ref().map_or(0, |coders| {
                (coders.count << coders.level()) * data.research[&item].multiplier / 801
            })
        };
        let job = &mut workshop.jobs[index];
        let sum = job.value + gain;
        if sum > 255 {
            job.value = sum & 0xFF;
            job.stage = (job.stage + 1).min(Job::STAGES);
        } else {
            job.value = sum;
        }

        if job.stage == Job::STAGES {
            store.add(item, 1);
            events.push(Event::ItemBuilt {
                day,
                player,
                at,
                item,
            });
            finish_job(
                data, day, player, workshop, store, staff, ground, index, events,
            );
        }
    }

    // The fuels refine on their own in every workshop, every other day.
    if day.is_multiple_of(2) {
        for (&item, def) in &data.items {
            if def.auto_produce
                && research.get(&item).is_some_and(|r| r.researched)
                && consume_recipe(data, store, item)
            {
                store.add(item, 3);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn finish_job(
    data: &GameData,
    day: Day,
    player: PlayerId,
    workshop: &mut Workshop,
    store: &mut Store,
    staff: &mut Vec<Staff>,
    ground: bool,
    index: usize,
    events: &mut Vec<Event>,
) {
    let item = workshop.jobs[index].item;
    if !workshop.automated {
        workshop.jobs.remove(index);
        if let Some(coders) = &mut workshop.coders
            && let Some(level) = coders.record_action()
        {
            events.push(Event::StaffPromoted {
                day,
                player,
                kind: coders.kind,
                level,
            });
        }
        // A build-bot built by a citadel's coders takes over the workshop,
        // and the coders move to the citadel's staff slots if one is free.
        if item == ItemType::BuildBot && !ground {
            workshop.automated = true;
            if staff.len() < crate::site::STAFF_SLOTS
                && let Some(coders) = workshop.coders.take()
            {
                staff.push(coders);
            }
        }
        return;
    }

    if !workshop.jobs[index].repeat {
        workshop.jobs.remove(index);
        return;
    }
    let job = &mut workshop.jobs[index];
    job.stage = 1;
    job.value = initial_value(data, item);
    // Take turns with the next queued item that can be afforded. Unlike the
    // original, a repeat with nothing else to do pays for itself again.
    let next = (1..workshop.jobs.len())
        .map(|offset| (index + offset) % workshop.jobs.len())
        .find(|&other| has_recipe(data, store, workshop.jobs[other].item));
    match next {
        Some(other) => {
            workshop.jobs[index].active = false;
            consume_recipe(data, store, workshop.jobs[other].item);
            workshop.jobs[other].active = true;
        }
        None => workshop.jobs[index].active = consume_recipe(data, store, item),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        data: GameData,
        research: BTreeMap<ItemType, ResearchProgress>,
        workshop: Workshop,
        store: Store,
        staff: Vec<Staff>,
        events: Vec<Event>,
    }

    impl Fixture {
        fn new(coders: Option<Staff>) -> Fixture {
            let data = GameData::classic();
            let research = data
                .research
                .iter()
                .map(|(&item, def)| {
                    let mut progress = ResearchProgress::new(def);
                    progress.researched = true;
                    (item, progress)
                })
                .collect();
            Fixture {
                data,
                research,
                workshop: Workshop {
                    coders,
                    ..Workshop::default()
                },
                store: Store::default(),
                staff: Vec::new(),
                events: Vec::new(),
            }
        }

        fn bench(&mut self, ground: bool) -> Bench<'_> {
            Bench {
                at: WorkshopRef::Hideout,
                workshop: &mut self.workshop,
                store: &mut self.store,
                staff: &mut self.staff,
                ground,
            }
        }

        fn build(&mut self, item: ItemType, ground: bool) -> Result<(), CommandError> {
            let data = self.data.clone();
            let research = self.research.clone();
            build(&data, &research, self.bench(ground), item)
        }

        fn day(&mut self, day: Day, ground: bool) {
            let data = self.data.clone();
            let research = self.research.clone();
            let mut events = Vec::new();
            run_day(
                &data,
                &research,
                day,
                PlayerId(0),
                self.bench(ground),
                &mut events,
            );
            self.events.extend(events);
        }

        fn stock(&mut self, item: ItemType, times: u32) {
            for (input, count) in self.data.items[&item].recipe.clone() {
                self.store.add(input, count * times);
            }
        }
    }

    fn coders(count: u32) -> Option<Staff> {
        Some(Staff::new("Lead", StaffKind::Coder, count))
    }

    #[test]
    fn a_hundred_junior_coders_build_a_tap_on_day_47() {
        // (100 << 1) * 64 / 801 = 15 a day from 64: stages at days 13, 30, 47.
        let mut f = Fixture::new(coders(100));
        f.stock(ItemType::Tap, 1);
        f.build(ItemType::Tap, true).unwrap();
        assert_eq!(
            f.store.get(ItemType::Compute),
            0,
            "the recipe is paid up front"
        );

        for day in 1..47 {
            f.day(day, true);
        }
        assert_eq!(f.store.get(ItemType::Tap), 0);
        f.day(47, true);
        assert_eq!(f.store.get(ItemType::Tap), 1);
        assert!(f.workshop.jobs.is_empty());
        assert!(matches!(
            f.events[0],
            Event::ItemBuilt {
                day: 47,
                item: ItemType::Tap,
                ..
            }
        ));
    }

    #[test]
    fn building_needs_coders_research_resources_and_level() {
        let mut f = Fixture::new(None);
        assert_eq!(f.build(ItemType::Tap, true), Err(CommandError::NoCoders));

        let mut f = Fixture::new(coders(10));
        assert_eq!(
            f.build(ItemType::Tap, true),
            Err(CommandError::MissingResources(ItemType::Tap))
        );
        assert_eq!(
            f.build(ItemType::WormCore, true),
            Err(CommandError::OrbitOnly(ItemType::WormCore))
        );
        assert_eq!(
            f.build(ItemType::WormCore, false),
            Err(CommandError::TeamLevelTooLow {
                item: ItemType::WormCore,
                needed: 2
            })
        );
        assert_eq!(
            f.build(ItemType::Compute, true),
            Err(CommandError::NotBuildable(ItemType::Compute))
        );
        f.research.get_mut(&ItemType::Tap).unwrap().researched = false;
        assert_eq!(
            f.build(ItemType::Tap, true),
            Err(CommandError::NotResearched(ItemType::Tap))
        );
    }

    #[test]
    fn switching_pauses_the_old_job_and_resuming_costs_nothing() {
        let mut f = Fixture::new(coders(100));
        f.stock(ItemType::Tap, 1);
        f.stock(ItemType::DataContainer, 1);
        f.build(ItemType::Tap, true).unwrap();
        for day in 1..=14 {
            f.day(day, true);
        }
        assert_eq!(f.workshop.jobs[0].stage, 2);

        f.build(ItemType::DataContainer, true).unwrap();
        let tap = &f.workshop.jobs[0];
        assert!(!tap.active);
        assert_eq!(
            (tap.stage, tap.value),
            (2, 64),
            "keeps its stage, loses the rest"
        );

        f.build(ItemType::Tap, true).unwrap();
        assert!(f.workshop.jobs[0].active);
        assert!(!f.workshop.jobs[1].active);
    }

    #[test]
    fn a_build_bot_works_through_its_queue() {
        let mut f = Fixture::new(None);
        f.workshop.automated = true;
        f.stock(ItemType::Tap, 2);
        f.stock(ItemType::DataContainer, 1);
        let data = f.data.clone();
        let research = f.research.clone();
        automate(
            &data,
            &research,
            f.bench(false),
            ItemType::Tap,
            AutoMode::Repeat,
        )
        .unwrap();
        automate(
            &data,
            &research,
            f.bench(false),
            ItemType::DataContainer,
            AutoMode::Once,
        )
        .unwrap();

        // 128 a day from 64: a stage every two days, an item every six.
        for day in 1..=18 {
            f.day(day, false);
        }
        let built: Vec<ItemType> = f
            .events
            .iter()
            .map(|e| match e {
                Event::ItemBuilt { item, .. } => *item,
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(
            built,
            [ItemType::Tap, ItemType::DataContainer, ItemType::Tap]
        );
        assert_eq!(f.workshop.jobs.len(), 1, "the one-off left the queue");
        assert!(!f.workshop.jobs[0].active, "the repeat waits for resources");
    }

    #[test]
    fn coders_who_build_a_build_bot_hand_the_workshop_over() {
        let mut f = Fixture::new(Some({
            let mut lead = Staff::new("Lead", StaffKind::Coder, 200);
            for _ in 0..12 {
                lead.record_action();
            }
            lead
        }));
        f.stock(ItemType::BuildBot, 1);
        f.build(ItemType::BuildBot, false).unwrap();
        for day in 1..=40 {
            f.day(day, false);
        }
        assert!(f.workshop.automated);
        assert!(f.workshop.coders.is_none());
        assert_eq!(f.staff.len(), 1);
        assert_eq!(f.store.get(ItemType::BuildBot), 1);
    }

    #[test]
    fn researched_fuels_refine_every_other_day() {
        let mut f = Fixture::new(None);
        f.store.add(ItemType::Bandwidth, 4);
        f.store.add(ItemType::Proxies, 4);
        f.day(1, true);
        assert_eq!(f.store.get(ItemType::ProxyChains), 0);
        f.day(2, true);
        assert_eq!(f.store.get(ItemType::ProxyChains), 3);
        f.day(4, true);
        assert_eq!(f.store.get(ItemType::ProxyChains), 6);
        assert_eq!(f.store.get(ItemType::Bandwidth), 0);
    }
}
