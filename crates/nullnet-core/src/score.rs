//! Points and the end of the game. New to NullNet: Deuteros went on until
//! the player gave up or cleared the galaxy.
//!
//! A crew scores for the citadels and hosts it holds, the Legacy hosts it
//! freed, the hosts it took from rivals and the research it finished. The
//! game ends when a crew holds complete citadels on more than half the
//! hosts of the home network, or on the day limit the game was set up
//! with; the highest score wins, the lower crew id breaking ties.

use serde::{Deserialize, Serialize};

use crate::data::GameData;
use crate::ids::{Day, PlayerId};
use crate::turn::Event;
use crate::world::{Controller, World};

pub const CITADEL_POINTS: u32 = 10;
pub const HOST_POINTS: u32 = 3;
pub const FREED_POINTS: u32 = 15;
pub const TAKEN_POINTS: u32 = 5;
pub const RESEARCH_POINTS: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score {
    pub player: PlayerId,
    /// Complete citadels on hosts the crew holds.
    pub citadels: u32,
    /// Hosts the crew holds without a complete citadel yet.
    pub hosts: u32,
    /// Legacy hosts freed over the game.
    pub freed: u32,
    /// Hosts taken from rivals over the game.
    pub taken: u32,
    /// Items researched.
    pub research: u32,
    pub total: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EndReason {
    /// A crew holds complete citadels on more than half the home network.
    Domination,
    /// The game's last day came.
    DayLimit,
}

/// How the game ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameEnd {
    pub day: Day,
    pub winner: PlayerId,
    pub reason: EndReason,
    /// The final scores, highest first.
    pub scores: Vec<Score>,
}

/// One crew's score, or `None` if it is not in the game.
pub fn score(data: &GameData, world: &World, player: PlayerId) -> Option<Score> {
    let state = world.players.get(&player)?;
    let mut citadels = 0;
    let mut hosts = 0;
    for (h, host) in world.hosts.iter().enumerate() {
        if h == usize::from(data.hideout.host.0)
            || host.controller != Some(Controller::Crew(player))
        {
            continue;
        }
        if host.site.citadel.complete() {
            citadels += 1;
        } else {
            hosts += 1;
        }
    }
    let research = state.research.values().filter(|r| r.researched).count() as u32;
    Some(Score {
        player,
        citadels,
        hosts,
        freed: state.freed,
        taken: state.taken,
        research,
        total: citadels * CITADEL_POINTS
            + hosts * HOST_POINTS
            + state.freed * FREED_POINTS
            + state.taken * TAKEN_POINTS
            + research * RESEARCH_POINTS,
    })
}

/// Every crew's score, highest first; ties go to the lower crew id.
pub fn scores(data: &GameData, world: &World) -> Vec<Score> {
    let mut scores: Vec<Score> = world
        .players
        .keys()
        .filter_map(|&player| score(data, world, player))
        .collect();
    scores.sort_by_key(|s| (std::cmp::Reverse(s.total), s.player));
    scores
}

/// Hosts in the home network a crew can hold: all but the hideout's host
/// and the cache fields.
pub fn contested_hosts(data: &GameData) -> u32 {
    let Some(home) = data.hosts.get(usize::from(data.hideout.host.0)) else {
        return 0;
    };
    let home = home.network;
    data.hosts
        .iter()
        .enumerate()
        .filter(|(h, def)| {
            def.network == home && *h != usize::from(data.hideout.host.0) && !def.cache_field
        })
        .count() as u32
}

/// Ends the game if a crew dominates the home network or the day limit has
/// come, and reports it. Does nothing once the game has ended.
pub(crate) fn check_end(data: &GameData, world: &mut World) -> Option<Event> {
    if world.ended.is_some() || world.players.is_empty() {
        return None;
    }
    let scores = scores(data, world);
    // Only on a map: a test world without hosts has nothing to dominate.
    let contested = contested_hosts(data);
    let dominating = scores
        .iter()
        .filter(|s| contested > 0 && s.citadels > contested / 2)
        .max_by_key(|s| (s.total, std::cmp::Reverse(s.player)));
    let (winner, reason) = match dominating {
        Some(score) => (score.player, EndReason::Domination),
        None if world.end_day.is_some_and(|end| world.day >= end) => {
            (scores[0].player, EndReason::DayLimit)
        }
        None => return None,
    };
    let day = world.day;
    world.ended = Some(GameEnd {
        day,
        winner,
        reason,
        scores,
    });
    Some(Event::GameOver {
        day,
        player: winner,
        reason,
    })
}
