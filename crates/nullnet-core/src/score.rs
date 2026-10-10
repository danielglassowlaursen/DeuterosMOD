use serde::{Deserialize, Serialize};

use crate::data::rules;
use crate::ids::PlayerId;
use crate::world::World;

/// A crew's points: one per data, five per host held (the hideout not
/// counted) and ten per host taken from the Legacy Net.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score {
    pub player: PlayerId,
    pub data: u32,
    pub hosts: u32,
    pub freed: u32,
    pub total: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameEnd {
    /// The last turn that ran.
    pub turn: u32,
    /// The crew with the most points, if no other crew has as many.
    pub winner: Option<PlayerId>,
    /// Best first.
    pub scores: Vec<Score>,
}

pub fn score(world: &World, player: PlayerId) -> Option<Score> {
    let crew = world.crew(player)?;
    let hosts = world.held_by(player).filter(|&h| h != crew.hideout).count() as u32;
    let freed = crew.freed.len() as u32;
    Some(Score {
        player,
        data: crew.data,
        hosts,
        freed,
        total: crew.data + rules::POINTS_PER_HOST * hosts + rules::POINTS_PER_FREED * freed,
    })
}

/// Every crew's score, best first.
pub fn scores(world: &World) -> Vec<Score> {
    let mut all: Vec<Score> = world
        .crews
        .keys()
        .filter_map(|&p| score(world, p))
        .collect();
    all.sort_by(|a, b| b.total.cmp(&a.total).then(a.player.cmp(&b.player)));
    all
}

pub(crate) fn game_end(world: &World) -> GameEnd {
    let scores = scores(world);
    let winner = match scores.as_slice() {
        [first, second, ..] if first.total == second.total => None,
        [first, ..] => Some(first.player),
        [] => None,
    };
    GameEnd {
        turn: world.turn,
        winner,
        scores,
    }
}
