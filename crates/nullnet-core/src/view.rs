//! What one crew is allowed to see. Clients get a [`CrewView`], never the
//! whole [`World`], so a crew cannot read its rivals' hands.
//!
//! A crew sees its own state in full. Of the rest of the world it sees what
//! is in plain sight on the net: who holds each host and how big the
//! citadel above it is, and the vessels at hosts it holds or out on the
//! open net at the home host. Everything inside a rival's hideout, citadel
//! or held host stays hidden. Scanning with sniffers comes with M4.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{Day, HostId, PlayerId};
use crate::site::Site;
use crate::transport::{Berth, Vessel, VesselId, VesselState};
use crate::turn::{Event, TurnReport};
use crate::world::{Controller, Player, World};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrewView {
    pub day: Day,
    pub turn: u32,
    pub player: PlayerId,
    /// The crew's own state, in full.
    pub me: Player,
    /// Every crew in the game and how many hosts it holds.
    pub crews: Vec<CrewSummary>,
    /// Every host, indexed by [`HostId`].
    pub hosts: Vec<HostView>,
    /// The crew's own vessels, and others' where the crew can see them.
    pub vessels: BTreeMap<VesselId, Vessel>,
    /// Legacy swarms heading for or besieging the crew's hosts.
    pub threats: Vec<Threat>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Threat {
    pub host: HostId,
    pub daemons: u32,
    pub arrives: Option<Day>,
    pub siege_until: Option<Day>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrewSummary {
    pub player: PlayerId,
    pub name: String,
    pub hosts: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostView {
    pub controller: Option<Controller>,
    /// Citadel modules installed; visible from the net.
    pub citadel_modules: u32,
    /// The site in full, only where the crew holds the host.
    pub site: Option<Site>,
}

/// The world as `player` sees it, or `None` if it is not in the game.
pub fn crew_view(world: &World, player: PlayerId, home: HostId) -> Option<CrewView> {
    let me = world.players.get(&player)?.clone();
    let held = |crew: PlayerId| {
        world
            .hosts
            .iter()
            .filter(|h| h.controller == Some(Controller::Crew(crew)))
            .count() as u32
    };
    let mine = |host: HostId| {
        world.hosts[usize::from(host.0)].controller == Some(Controller::Crew(player))
    };
    Some(CrewView {
        day: world.day,
        turn: world.turn,
        player,
        crews: world
            .players
            .iter()
            .map(|(&id, p)| CrewSummary {
                player: id,
                name: p.name.clone(),
                hosts: held(id),
            })
            .collect(),
        hosts: world
            .hosts
            .iter()
            .map(|state| HostView {
                controller: state.controller,
                citadel_modules: state.site.citadel.modules,
                site: (state.controller == Some(Controller::Crew(player)))
                    .then(|| state.site.clone()),
            })
            .collect(),
        vessels: world
            .vessels
            .iter()
            .filter(|(_, v)| {
                v.owner == player
                    || mine(v.host)
                    || (v.host == home
                        && !matches!(
                            v.state,
                            VesselState::At(Berth::Planted) | VesselState::At(Berth::Connected)
                        ))
            })
            .map(|(&id, v)| (id, v.clone()))
            .collect(),
        threats: world
            .legacy
            .fleets
            .iter()
            .filter_map(|f| {
                let host = f.target?;
                mine(host).then_some(Threat {
                    host,
                    daemons: f.daemons,
                    arrives: f.arrives,
                    siege_until: f.siege_until,
                })
            })
            .collect(),
        me,
    })
}

impl Event {
    /// The day the event happened.
    pub fn day(&self) -> Day {
        match *self {
            Event::ResearchCompleted { day, .. }
            | Event::StaffPromoted { day, .. }
            | Event::RecruitsGraduated { day, .. }
            | Event::ItemBuilt { day, .. }
            | Event::HostClaimed { day, .. }
            | Event::Installed { day, .. }
            | Event::VesselArrived { day, .. }
            | Event::VesselStopped { day, .. }
            | Event::VesselBurned { day, .. }
            | Event::Unlocked { day, .. }
            | Event::WarDeclared { day, .. }
            | Event::FleetSighted { day, .. }
            | Event::UnderAttack { day, .. }
            | Event::AttackRepelled { day, .. }
            | Event::HostCaptured { day, .. }
            | Event::HostFreed { day, .. }
            | Event::BattleFought { day, .. }
            | Event::VesselLost { day, .. }
            | Event::CacheFound { day, .. }
            | Event::FragmentFound { day, .. } => day,
        }
    }

    /// The crew the event happened to.
    pub fn player(&self) -> PlayerId {
        match *self {
            Event::ResearchCompleted { player, .. }
            | Event::StaffPromoted { player, .. }
            | Event::RecruitsGraduated { player, .. }
            | Event::ItemBuilt { player, .. }
            | Event::HostClaimed { player, .. }
            | Event::Installed { player, .. }
            | Event::VesselArrived { player, .. }
            | Event::VesselStopped { player, .. }
            | Event::VesselBurned { player, .. }
            | Event::Unlocked { player, .. }
            | Event::WarDeclared { player, .. }
            | Event::FleetSighted { player, .. }
            | Event::UnderAttack { player, .. }
            | Event::AttackRepelled { player, .. }
            | Event::HostCaptured { player, .. }
            | Event::HostFreed { player, .. }
            | Event::BattleFought { player, .. }
            | Event::VesselLost { player, .. }
            | Event::CacheFound { player, .. }
            | Event::FragmentFound { player, .. } => player,
        }
    }

    /// Whether every crew learns of the event: a host changing hands is
    /// seen across the net.
    pub fn is_public(&self) -> bool {
        matches!(
            self,
            Event::HostClaimed { .. } | Event::HostCaptured { .. } | Event::HostFreed { .. }
        )
    }
}

impl TurnReport {
    /// The report as one crew may see it: its own rejected orders, and the
    /// events that happened to it or in plain sight.
    pub fn for_crew(&self, player: PlayerId) -> TurnReport {
        TurnReport {
            first_day: self.first_day,
            last_day: self.last_day,
            rejected: self
                .rejected
                .iter()
                .filter(|r| r.player == player)
                .cloned()
                .collect(),
            events: self
                .events
                .iter()
                .filter(|e| e.player() == player || e.is_public())
                .cloned()
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandError;
    use crate::data::GameData;
    use crate::transport::{Module, VesselKind};
    use crate::turn::{RejectedCommand, resolve_turn};

    const CREW: PlayerId = PlayerId(0);
    const RIVAL: PlayerId = PlayerId(1);

    fn vessel(owner: PlayerId, host: HostId, berth: Berth) -> Vessel {
        Vessel {
            owner,
            kind: VesselKind::Worm,
            host,
            state: VesselState::At(berth),
            fuel: 10,
            pilot: None,
            modules: vec![Module::Empty; 3],
            destination: None,
            exposed_days: 0,
            script: None,
            daemons: 0,
            c2: false,
            cache: None,
        }
    }

    #[test]
    fn a_crew_sees_its_own_sites_and_only_the_outside_of_others() {
        let data = GameData::classic();
        let mut world = World::new_game(&data, 3, &[(CREW, "Crew"), (RIVAL, "Rival")]);
        let home = data.hideout.host;
        let transit = HostId(data.hosts.iter().position(|h| h.name == "Transit").unwrap() as u16);
        let beacon = HostId(data.hosts.iter().position(|h| h.name == "Beacon").unwrap() as u16);
        world.hosts[usize::from(transit.0)].controller = Some(Controller::Crew(RIVAL));
        world.hosts[usize::from(transit.0)].site.citadel.modules = 3;
        world.hosts[usize::from(transit.0)].site.taps = 2;
        world.hosts[usize::from(beacon.0)].controller = Some(Controller::Crew(CREW));
        world
            .vessels
            .insert(VesselId(0), vessel(CREW, home, Berth::Connected));
        world
            .vessels
            .insert(VesselId(1), vessel(RIVAL, home, Berth::Connected));
        world
            .vessels
            .insert(VesselId(2), vessel(RIVAL, home, Berth::Lurking));
        world
            .vessels
            .insert(VesselId(3), vessel(RIVAL, beacon, Berth::Lurking));
        world
            .vessels
            .insert(VesselId(4), vessel(RIVAL, transit, Berth::Lurking));
        resolve_turn(&data, &mut world, &Default::default(), 0);

        let view = crew_view(&world, CREW, home).unwrap();
        assert_eq!(view.turn, 1);
        assert_eq!(view.me.name, "Crew");
        assert_eq!(
            view.crews.iter().map(|c| c.hosts).collect::<Vec<_>>(),
            [1, 1]
        );
        let transit_view = &view.hosts[usize::from(transit.0)];
        assert_eq!(transit_view.controller, Some(Controller::Crew(RIVAL)));
        assert_eq!(transit_view.citadel_modules, 3);
        assert!(transit_view.site.is_none(), "a rival's site stays hidden");
        assert_eq!(
            view.hosts[usize::from(beacon.0)]
                .site
                .as_ref()
                .map(|s| s.taps),
            Some(0)
        );
        // Own, lurking at home, and at a held host: seen. Inside a rival's
        // citadel at home, or at the rival's host: not.
        assert_eq!(
            view.vessels.keys().copied().collect::<Vec<_>>(),
            [VesselId(0), VesselId(2), VesselId(3)]
        );
        assert!(crew_view(&world, PlayerId(7), home).is_none());
    }

    #[test]
    fn reports_are_filtered_per_crew() {
        let report = TurnReport {
            first_day: 1,
            last_day: 10,
            rejected: vec![
                RejectedCommand {
                    player: CREW,
                    index: 0,
                    error: CommandError::NoCoders,
                },
                RejectedCommand {
                    player: RIVAL,
                    index: 2,
                    error: CommandError::NoCoders,
                },
            ],
            events: vec![
                Event::RecruitsGraduated {
                    day: 3,
                    player: RIVAL,
                    kind: crate::staff::StaffKind::Coder,
                    count: 5,
                },
                Event::HostClaimed {
                    day: 4,
                    player: RIVAL,
                    host: HostId(4),
                },
                Event::ResearchCompleted {
                    day: 5,
                    player: CREW,
                    item: crate::items::ItemType::Tap,
                },
            ],
        };
        let mine = report.for_crew(CREW);
        assert_eq!(mine.rejected.len(), 1);
        assert_eq!(mine.rejected[0].player, CREW);
        assert_eq!(mine.events.len(), 2);
        assert!(matches!(mine.events[0], Event::HostClaimed { .. }));
        assert!(matches!(mine.events[1], Event::ResearchCompleted { .. }));
    }
}
