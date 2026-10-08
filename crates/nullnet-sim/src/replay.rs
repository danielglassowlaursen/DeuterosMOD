//! A turn-by-turn record of a game: every crew's orders, what happened and
//! where each crew stood after each turn. Written as JSON for replay viewers.

use std::collections::BTreeMap;

use nullnet_core::{
    Command, Controller, Destination, Event, GameData, ItemType, Milestone, Module, Orders,
    PlayerId, Staff, StaffKind, TurnReport, VesselId, VesselKind, VesselState, World,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct Replay {
    pub seed: u64,
    pub turn_days: u32,
    pub home: u16,
    pub hosts: Vec<HostInfo>,
    pub crews: Vec<String>,
    pub turns: Vec<Turn>,
}

#[derive(Serialize)]
pub struct HostInfo {
    pub name: String,
    pub network: u8,
    pub order: u32,
    pub parent: Option<u16>,
    pub legacy: bool,
    pub cache_field: bool,
}

#[derive(Serialize)]
pub struct Turn {
    pub turn: u32,
    pub first_day: u32,
    pub last_day: u32,
    /// Each crew's orders, by crew index.
    pub orders: Vec<Vec<Command>>,
    pub rejected: Vec<Rejected>,
    pub events: Vec<Event>,
    /// Each crew after the turn, by crew index.
    pub crews: Vec<CrewStatus>,
}

#[derive(Serialize)]
pub struct Rejected {
    pub crew: u8,
    pub index: usize,
    pub error: String,
}

#[derive(Serialize)]
pub struct CrewStatus {
    pub taps: u32,
    pub citadel_modules: u32,
    pub recruits_available: u32,
    pub courses: Vec<CourseStatus>,
    pub research: Option<ResearchStatus>,
    pub researched: Vec<ItemType>,
    pub analysts: Option<Team>,
    pub coders: Option<Team>,
    pub citadel_coders: Option<Team>,
    pub hideout_staff: Vec<Team>,
    pub citadel_staff: Vec<Team>,
    pub hideout_job: Option<JobStatus>,
    pub citadel_job: Option<JobStatus>,
    pub hideout_store: BTreeMap<ItemType, u32>,
    pub citadel_store: BTreeMap<ItemType, u32>,
    pub vessels: Vec<VesselStatus>,
    pub hosts: Vec<HeldHost>,
    pub milestones: Vec<Milestone>,
}

#[derive(Serialize)]
pub struct CourseStatus {
    pub kind: StaffKind,
    pub enrolled: u32,
    pub running: bool,
}

#[derive(Serialize)]
pub struct ResearchStatus {
    pub item: ItemType,
    pub percent: u8,
}

#[derive(Serialize)]
pub struct Team {
    pub leader: String,
    pub kind: StaffKind,
    pub count: u32,
    pub level: u8,
}

impl Team {
    fn of(staff: &Staff) -> Team {
        Team {
            leader: staff.leader.clone(),
            kind: staff.kind,
            count: staff.count,
            level: staff.level(),
        }
    }
}

#[derive(Serialize)]
pub struct JobStatus {
    pub item: ItemType,
    pub stage: u32,
}

#[derive(Serialize)]
pub struct VesselStatus {
    pub id: VesselId,
    pub kind: VesselKind,
    pub host: u16,
    pub state: VesselState,
    pub fuel: u32,
    pub pilot: Option<Team>,
    pub modules: Vec<Module>,
    pub destination: Option<Destination>,
    /// Whether the vessel has an exfil script, and whether it is running.
    pub script: Option<bool>,
}

#[derive(Serialize)]
pub struct HeldHost {
    pub host: u16,
    pub citadel_modules: u32,
}

impl Replay {
    pub fn new(data: &GameData, seed: u64, turn_days: u32, crews: &[(PlayerId, &str)]) -> Self {
        Replay {
            seed,
            turn_days,
            home: data.hideout.host.0,
            hosts: data
                .hosts
                .iter()
                .map(|def| HostInfo {
                    name: def.name.clone(),
                    network: def.network.0,
                    order: def.order,
                    parent: def.parent.map(|p| p.0),
                    legacy: def.legacy,
                    cache_field: def.cache_field,
                })
                .collect(),
            crews: crews.iter().map(|(_, name)| name.to_string()).collect(),
            turns: Vec::new(),
        }
    }

    /// Records a resolved turn and the world as it leaves it.
    pub fn record(&mut self, world: &World, orders: &Orders, report: &TurnReport) {
        let crews: Vec<PlayerId> = (0..self.crews.len()).map(|i| PlayerId(i as u8)).collect();
        self.turns.push(Turn {
            turn: self.turns.len() as u32,
            first_day: report.first_day,
            last_day: report.last_day,
            orders: crews
                .iter()
                .map(|id| orders.get(id).cloned().unwrap_or_default())
                .collect(),
            rejected: report
                .rejected
                .iter()
                .map(|r| Rejected {
                    crew: r.player.0,
                    index: r.index,
                    error: r.error.to_string(),
                })
                .collect(),
            events: report.events.clone(),
            crews: crews.iter().map(|&id| crew_status(world, id)).collect(),
        });
    }
}

fn crew_status(world: &World, id: PlayerId) -> CrewStatus {
    let player = &world.players[&id];
    let hideout = &player.hideout;
    let citadel = &hideout.citadel;
    let job = |workshop: &nullnet_core::Workshop| {
        workshop.jobs.iter().find(|j| j.active).map(|j| JobStatus {
            item: j.item,
            stage: j.stage,
        })
    };
    CrewStatus {
        taps: hideout.taps,
        citadel_modules: citadel.modules,
        recruits_available: player.recruitment.available,
        courses: player
            .recruitment
            .courses
            .iter()
            .filter(|(_, c)| c.enrolled > 0)
            .map(|(&kind, c)| CourseStatus {
                kind,
                enrolled: c.enrolled,
                running: c.running(),
            })
            .collect(),
        research: player.current_research.and_then(|item| {
            let progress = player.research.get(&item)?;
            (!progress.researched).then_some(ResearchStatus {
                item,
                percent: progress.percent,
            })
        }),
        researched: player
            .research
            .iter()
            .filter(|(_, p)| p.researched)
            .map(|(&item, _)| item)
            .collect(),
        analysts: player.research_team.as_ref().map(Team::of),
        coders: player.workshop.coders.as_ref().map(Team::of),
        citadel_coders: citadel.workshop.coders.as_ref().map(Team::of),
        hideout_staff: hideout.staff.iter().map(Team::of).collect(),
        citadel_staff: citadel.staff.iter().map(Team::of).collect(),
        hideout_job: job(&player.workshop),
        citadel_job: job(&citadel.workshop),
        hideout_store: hideout.store.iter().collect(),
        citadel_store: citadel.store.iter().collect(),
        vessels: world
            .vessels
            .iter()
            .filter(|(_, v)| v.owner == id)
            .map(|(&vid, v)| VesselStatus {
                id: vid,
                kind: v.kind,
                host: v.host.0,
                state: v.state,
                fuel: v.fuel,
                pilot: v.pilot.as_ref().map(Team::of),
                modules: v.modules.clone(),
                destination: v.destination,
                script: v.script.as_ref().map(|s| s.running()),
            })
            .collect(),
        hosts: world
            .hosts
            .iter()
            .enumerate()
            .filter(|(_, h)| h.controller == Some(Controller::Crew(id)))
            .map(|(index, h)| HeldHost {
                host: index as u16,
                citadel_modules: h.site.citadel.modules,
            })
            .collect(),
        milestones: player.milestones.iter().copied().collect(),
    }
}
