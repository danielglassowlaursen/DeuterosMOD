//! What one crew is allowed to see: its own crew in full, the map with fog
//! of war (security, weakness and ICE only for hosts it has scanned or holds)
//! and a summary of its rivals.

use serde::{Deserialize, Serialize};

use crate::data::{GameData, Weakness, Yields};
use crate::ids::{HackerId, HostId, PlayerId};
use crate::legacy;
use crate::mapgen::MapSpec;
use crate::score::{Score, scores};
use crate::world::{Controller, Crew, Difficulty, Subnet, World};

/// A host as a crew sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostView {
    pub host: HostId,
    pub controller: Option<Controller>,
    /// Security, weakness, ICE and defence, if the crew has scanned or holds
    /// the host.
    pub intel: Option<Intel>,
    /// The crew has a usable way in from an earlier break-in.
    pub access: bool,
    /// The crew may break into it this turn.
    pub can_break_in: bool,
    /// The crew may scan it this turn.
    pub can_scan: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intel {
    pub security: u8,
    pub weakness: Weakness,
    pub ice: u8,
    /// The defence a break-in faces, firewall included.
    pub defence: u32,
    /// A sealed sub-net behind the host, which a scan reveals.
    #[serde(default)]
    pub subnet: Option<Subnet>,
}

/// What a crew knows about a rival.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrewSummary {
    pub player: PlayerId,
    pub name: String,
    pub hosts: u32,
    pub trace: u32,
    pub hackers: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrewView {
    pub turn: u32,
    pub last_turn: u32,
    pub difficulty: Difficulty,
    /// The map the game is played on, which the client builds from this.
    #[serde(default)]
    pub map: MapSpec,
    pub player: PlayerId,
    /// The crew in full.
    pub me: Crew,
    /// Bandwidth to spend this turn.
    pub bandwidth: u32,
    /// What the crew's hosts and rigs give it each turn.
    pub income: Yields,
    /// The trace at which the Legacy Net sweeps.
    pub sweep_at: u32,
    pub hosts: Vec<HostView>,
    pub crews: Vec<CrewSummary>,
    pub scores: Vec<Score>,
    pub over: bool,
}

impl CrewView {
    pub fn host(&self, host: HostId) -> Option<&HostView> {
        self.hosts.get(host.index())
    }

    /// The crew's hacker, by id.
    pub fn hacker(&self, id: HackerId) -> Option<&crate::world::Hacker> {
        self.me.hacker(id)
    }
}

pub fn crew_view(data: &GameData, world: &World, player: PlayerId) -> Option<CrewView> {
    let me = world.crew(player)?.clone();
    let scan_range = world.scan_range(data, player);
    let front = legacy::spread_front(data, world);
    let _ = front;

    let hosts = data
        .host_ids()
        .map(|host| {
            let state = world.host(host);
            let knows = world.knows(player, host);
            let intel = knows.then(|| Intel {
                security: state.security,
                weakness: state.weakness,
                ice: state.ice,
                defence: world.defence(host),
                subnet: state.subnet,
            });
            HostView {
                host,
                controller: state.controller,
                intel,
                access: state.has_access(player, world.turn),
                can_break_in: world.can_break_in(data, player, host),
                can_scan: !knows && scan_range.contains(&host),
            }
        })
        .collect();

    let crews = world
        .crews
        .iter()
        .filter(|&(&p, _)| p != player)
        .map(|(&p, crew)| CrewSummary {
            player: p,
            name: crew.name.clone(),
            hosts: world.held_by(p).filter(|&h| h != crew.hideout).count() as u32,
            trace: crew.trace,
            hackers: crew.hackers.len(),
        })
        .collect();

    Some(CrewView {
        turn: world.turn,
        last_turn: world.settings.last_turn,
        difficulty: world.settings.difficulty,
        map: world.settings.map,
        player,
        bandwidth: world.bandwidth(data, player),
        income: world.income(data, player),
        sweep_at: world.settings.difficulty.sweep_at(),
        me,
        hosts,
        crews,
        scores: scores(world),
        over: world.ended.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Settings;

    #[test]
    fn fog_hides_unscanned_intel() {
        let data = GameData::standard();
        let world = World::new_game(
            &data,
            2,
            &[(PlayerId(0), "A"), (PlayerId(1), "B")],
            Settings::default(),
        );
        let view = crew_view(&data, &world, PlayerId(0)).unwrap();
        let hideout = view.me.hideout;
        // The crew knows its own hideout but not a stronghold far away.
        assert!(view.host(hideout).unwrap().intel.is_some());
        let cortex = data.find("Cortex").unwrap();
        assert!(view.host(cortex).unwrap().intel.is_none());
        // It can break into a neighbour but not scan what it already knows.
        let neighbour = data.neighbours(hideout)[0];
        assert!(view.host(neighbour).unwrap().can_break_in);
        assert!(view.host(neighbour).unwrap().can_scan);
        assert!(!view.host(hideout).unwrap().can_scan);
        // It sees one rival, not itself.
        assert_eq!(view.crews.len(), 1);
    }
}
