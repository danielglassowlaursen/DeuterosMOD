use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::command::CommandError;
use crate::data::GameData;
use crate::ids::{Day, HostId, NetworkId, PlayerId};
use crate::items::ItemType;
use crate::recruitment::Recruitment;
use crate::research::{ResearchDef, ResearchProgress};
use crate::rng::Rng;
use crate::site::{Citadel, Site};
use crate::staff::Staff;
use crate::transport::{Vessel, VesselId};
use crate::unlocks::Milestones;
use crate::workshop::{SiteRef, Workshop};

/// The complete mutable state of one game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub day: Day,
    /// Turns resolved so far. Which crew's orders go first rotates with it.
    pub turn: u32,
    pub rng: Rng,
    pub players: BTreeMap<PlayerId, Player>,
    /// Indexed by [`HostId`], parallel to `GameData::hosts`.
    pub hosts: Vec<HostState>,
    /// Index of the next team leader's handle.
    pub next_handle: u32,
    pub vessels: BTreeMap<VesselId, Vessel>,
    pub next_vessel: u32,
}

/// Who holds a host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Controller {
    Crew(PlayerId),
    Legacy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostState {
    pub controller: Option<Controller>,
    pub site: Site,
}

impl World {
    /// An empty world with no hosts or players.
    pub fn new(seed: u64) -> Self {
        World {
            day: 0,
            turn: 0,
            rng: Rng::new(seed, 0),
            players: BTreeMap::new(),
            hosts: Vec::new(),
            next_handle: 0,
            vessels: BTreeMap::new(),
            next_vessel: 0,
        }
    }

    /// A new game on the classic map: every crew gets a hideout on the home
    /// host, and the Legacy Net holds its fixed hosts plus a random set in
    /// each other network, all with complete citadels.
    pub fn new_game(data: &GameData, seed: u64, crews: &[(PlayerId, &str)]) -> Self {
        let mut world = World::new(seed);
        world.hosts = data
            .hosts
            .iter()
            .map(|def| HostState {
                controller: def.legacy.then_some(Controller::Legacy),
                site: Site::new(def),
            })
            .collect();

        for (index, network) in data.networks.iter().enumerate() {
            let Some(count) = network.random_legacy_hosts else {
                continue;
            };
            let id = NetworkId(index as u8);
            let mut free: Vec<usize> = (0..data.hosts.len())
                .filter(|&h| data.hosts[h].network == id && world.hosts[h].controller.is_none())
                .collect();
            for _ in 0..count.min(free.len() as u32) {
                // The original's Next(Count - 1) could never pick the last host.
                let pick = free.remove(world.rng.below(free.len() as u32) as usize);
                world.hosts[pick].controller = Some(Controller::Legacy);
            }
        }
        for host in &mut world.hosts {
            if host.controller == Some(Controller::Legacy) {
                host.site.backdoor_parts = Site::BACKDOOR_PARTS;
                host.site.citadel = Citadel {
                    modules: Citadel::MODULES,
                    encrypted_link: true,
                    kill_switch: true,
                    workshop: Workshop {
                        automated: true,
                        ..Workshop::default()
                    },
                    ..Citadel::default()
                };
            }
        }

        for &(id, name) in crews {
            world.players.insert(id, Player::new_crew(data, name));
        }
        world
    }

    pub fn host(&self, id: HostId) -> &HostState {
        &self.hosts[usize::from(id.0)]
    }

    /// A crew's own site: its hideout, or a host it controls.
    pub(crate) fn crew_site_mut(
        &mut self,
        crew: PlayerId,
        site: SiteRef,
    ) -> Result<&mut Site, CommandError> {
        match site {
            SiteRef::Hideout => self
                .players
                .get_mut(&crew)
                .map(|player| &mut player.hideout)
                .ok_or(CommandError::UnknownPlayer(crew)),
            SiteRef::Host(id) => {
                let host = self
                    .hosts
                    .get_mut(usize::from(id.0))
                    .ok_or(CommandError::UnknownHost(id))?;
                if host.controller != Some(Controller::Crew(crew)) {
                    return Err(CommandError::NotYourHost(id));
                }
                Ok(&mut host.site)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub name: String,
    /// The crew's own site on the home host. It cannot be taken.
    pub hideout: Site,
    /// The hideout's own workshop, apart from any citadel above it.
    pub workshop: Workshop,
    pub recruitment: Recruitment,
    /// The single analyst team in the hideout; `None` until recruits graduate.
    pub research_team: Option<Staff>,
    pub current_research: Option<ItemType>,
    /// Research the player has access to, in progress or finished.
    pub research: BTreeMap<ItemType, ResearchProgress>,
    pub milestones: Milestones,
}

impl Player {
    pub fn new(name: impl Into<String>) -> Self {
        Player {
            name: name.into(),
            hideout: Site::default(),
            workshop: Workshop::default(),
            recruitment: Recruitment::default(),
            research_team: None,
            current_research: None,
            research: BTreeMap::new(),
            milestones: Milestones::new(),
        }
    }

    /// A crew as the game starts it: a working backdoor and the starting
    /// taps in the hideout, and the starter research open.
    pub fn new_crew(data: &GameData, name: impl Into<String>) -> Self {
        let mut player = Player::new(name);
        player.hideout = Site {
            backdoor_parts: Site::BACKDOOR_PARTS,
            taps: data.hideout.taps,
            ..Site::new(data.host(data.hideout.host))
        };
        player.recruitment.available = data.recruitment.recruits_available;
        for (&item, def) in &data.research {
            if def.available_at_start || def.researched_at_start {
                player.unlock_research(item, def);
            }
            if def.researched_at_start {
                let progress = player.research.get_mut(&item).expect("just unlocked");
                progress.percent = 100;
                progress.researched = true;
            }
        }
        player
    }

    /// Makes an item available for research. Does nothing if it already is,
    /// so earlier progress is never reset.
    pub fn unlock_research(&mut self, item: ItemType, def: &ResearchDef) {
        self.research
            .entry(item)
            .or_insert_with(|| ResearchProgress::new(def));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CREWS: [(PlayerId, &str); 2] = [(PlayerId(0), "Zer0"), (PlayerId(1), "Kestrel")];

    #[test]
    fn the_legacy_net_holds_its_fixed_and_random_hosts() {
        let data = GameData::classic();
        let world = World::new_game(&data, 42, &CREWS);

        for (index, network) in data.networks.iter().enumerate() {
            let held = (0..data.hosts.len())
                .filter(|&h| {
                    data.hosts[h].network == NetworkId(index as u8)
                        && world.hosts[h].controller == Some(Controller::Legacy)
                })
                .count() as u32;
            let fixed = data
                .hosts
                .iter()
                .filter(|h| h.network == NetworkId(index as u8) && h.legacy)
                .count() as u32;
            assert_eq!(
                held,
                network.random_legacy_hosts.unwrap_or(0) + fixed,
                "{}",
                network.classic
            );
        }
        let legacy = world.hosts.iter().filter(|h| h.controller.is_some());
        assert!(
            legacy
                .clone()
                .all(|h| h.site.citadel.complete() && h.site.backdoor_complete())
        );
        assert_eq!(legacy.count(), 81);
    }

    #[test]
    fn crews_start_with_a_working_hideout_and_the_starter_research() {
        let data = GameData::classic();
        let world = World::new_game(&data, 42, &CREWS);

        for player in world.players.values() {
            let hideout = &player.hideout;
            assert!(hideout.backdoor_complete());
            assert_eq!(hideout.taps, 1);
            assert_eq!(
                hideout.veins.len(),
                data.host(data.hideout.host).resources.len()
            );
            assert!(player.research[&ItemType::Tap].researched);
            assert!(!player.research[&ItemType::DropperCore].researched);
            assert!(!player.research.contains_key(&ItemType::WormCore));
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_map() {
        let data = GameData::classic();
        assert_eq!(
            World::new_game(&data, 7, &CREWS),
            World::new_game(&data, 7, &CREWS)
        );
        assert_ne!(
            World::new_game(&data, 7, &CREWS).hosts,
            World::new_game(&data, 8, &CREWS).hosts
        );
    }
}
