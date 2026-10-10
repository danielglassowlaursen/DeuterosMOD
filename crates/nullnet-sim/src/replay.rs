//! A turn-by-turn record of a game as JSON: the map, every crew's orders,
//! what happened and where each crew stood after each turn.

use nullnet_core::{Command, Controller, Event, GameData, Orders, PlayerId, TurnReport, World};
use serde::Serialize;

#[derive(Serialize)]
pub struct Replay {
    pub seed: u64,
    pub difficulty: String,
    pub last_turn: u32,
    pub hosts: Vec<HostInfo>,
    pub links: Vec<(u16, u16)>,
    pub crews: Vec<String>,
    pub turns: Vec<Turn>,
}

#[derive(Serialize)]
pub struct HostInfo {
    pub name: String,
    pub district: String,
    pub pos: (i16, i16),
    pub legacy: bool,
}

#[derive(Serialize)]
pub struct Turn {
    pub turn: u32,
    pub orders: Vec<CrewOrders>,
    pub events: Vec<String>,
    pub crews: Vec<CrewStanding>,
}

#[derive(Serialize)]
pub struct CrewOrders {
    pub crew: String,
    pub commands: Vec<Command>,
}

#[derive(Serialize)]
pub struct CrewStanding {
    pub crew: String,
    pub credits: u32,
    pub compute: u32,
    pub data: u32,
    pub trace: u32,
    pub hosts: u32,
    pub hackers: usize,
}

impl Replay {
    pub fn new(data: &GameData, world: &World, names: &[(PlayerId, &str)]) -> Self {
        Replay {
            seed: world.seed,
            difficulty: world.settings.difficulty.name().to_string(),
            last_turn: world.settings.last_turn,
            hosts: data
                .hosts
                .iter()
                .map(|h| HostInfo {
                    name: h.name.to_string(),
                    district: h.district.name().to_string(),
                    pos: h.pos,
                    legacy: h.legacy,
                })
                .collect(),
            links: data.links.iter().map(|&(a, b)| (a.0, b.0)).collect(),
            crews: names.iter().map(|&(_, n)| n.to_string()).collect(),
            turns: Vec::new(),
        }
    }

    pub fn record(
        &mut self,
        world: &World,
        names: &[(PlayerId, &str)],
        orders: &Orders,
        report: &TurnReport,
    ) {
        let crew_orders = names
            .iter()
            .map(|&(p, name)| CrewOrders {
                crew: name.to_string(),
                commands: orders.get(&p).cloned().unwrap_or_default(),
            })
            .collect();
        let standings = names
            .iter()
            .map(|&(p, name)| {
                let crew = world.crew(p);
                CrewStanding {
                    crew: name.to_string(),
                    credits: crew.map_or(0, |c| c.credits),
                    compute: crew.map_or(0, |c| c.compute),
                    data: crew.map_or(0, |c| c.data),
                    trace: crew.map_or(0, |c| c.trace),
                    hosts: world
                        .held_by(p)
                        .filter(|&h| Some(h) != crew.map(|c| c.hideout))
                        .count() as u32,
                    hackers: crew.map_or(0, |c| c.hackers.len()),
                }
            })
            .collect();
        self.turns.push(Turn {
            turn: report.turn,
            orders: crew_orders,
            events: report.events.iter().map(|e| describe(world, e)).collect(),
            crews: standings,
        });
    }
}

/// A short human line for an event, for the timeline.
pub fn describe(world: &World, event: &Event) -> String {
    use Event::*;
    let name = |c: Controller| match c {
        Controller::Crew(p) => world
            .crew(p)
            .map_or_else(|| format!("crew {}", p.0), |c| c.name.clone()),
        Controller::Legacy => "the Legacy Net".to_string(),
    };
    let crew = |p: PlayerId| name(Controller::Crew(p));
    match event {
        Scanned { .. } => "scanned a host".to_string(),
        BrokeIn {
            player,
            chance,
            outcome,
            ..
        } => format!("{}: break-in ({chance}%) {outcome:?}", crew(*player)),
        Intrusion {
            player, intruder, ..
        } => {
            format!("{} broke into {}'s host", crew(*intruder), crew(*player))
        }
        BackdoorPlanted { player, .. } => format!("{} planted a backdoor", crew(*player)),
        BackdoorFailed { player, .. } => format!("{}'s backdoor failed", crew(*player)),
        HostLost { player, taker, .. } => {
            format!("{} lost a host to {}", crew(*player), name(*taker))
        }
        AccessPurged { player, .. } => format!("{} was thrown out", crew(*player)),
        DataStolen { player, amount, .. } => format!("{} stole {amount} data", crew(*player)),
        DataLost { player, amount, .. } => format!("{} lost {amount} data", crew(*player)),
        Income { .. } | Wages { .. } => String::new(),
        HackerQuit { player, handle, .. } => format!("{} lost {handle}", crew(*player)),
        HackerHired { player, handle, .. } => format!("{} hired {handle}", crew(*player)),
        LevelUp {
            player,
            handle,
            level,
            ..
        } => format!("{}'s {handle} reached level {level}", crew(*player)),
        Bought { player, what, .. } => format!("{} bought {what}", crew(*player)),
        Swept { player, lost, .. } => {
            if *lost {
                format!("the Legacy Net swept {} and took a host", crew(*player))
            } else {
                format!("the Legacy Net swept {} but the host held", crew(*player))
            }
        }
        LegacySpread { .. } => "the Legacy Net spread to a free host".to_string(),
        GameEnded { winner, .. } => match winner {
            Some(p) => format!("game over: {} won", crew(*p)),
            None => "game over: a tie".to_string(),
        },
    }
}
