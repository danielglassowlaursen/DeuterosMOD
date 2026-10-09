//! Milestones that open new research. Port of
//! Godot/Code/Platform/Unlocker.cs, per crew.
//!
//! Each milestone is reached once by a crew and makes a set of items
//! available to research. They are checked after the orders of a turn and
//! after every day, from what happened and from what the crew holds.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::data::GameData;
use crate::ids::PlayerId;
use crate::items::ItemType;
use crate::site::Citadel;
use crate::turn::Event;
use crate::world::{Controller, World};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Milestone {
    /// The first citadel module above the hideout. (First_Station_Segment)
    FirstCitadelModule,
    /// The hideout's citadel complete. (Space_Stations)
    HideoutCitadel,
    /// The first worm core built. (IOS_Attachments)
    WormEquipment,
    /// The Legacy exploit researched, or war declared. (D_F_C_C)
    Daemons,
    /// A fragment of the Legacy Net's source code brought into a store;
    /// its exploit can be worked out. (Alien artifact)
    SourceCode,
    /// Holding a citadel with an encrypted link, as on a freed Legacy host.
    /// (Mass_Tranceiver)
    EncryptedLinks,
    /// Holding a citadel with a kill switch. (Self_Destruct)
    KillSwitches,
    /// The home network free of the Legacy Net. Deuteros opened travel to
    /// other stars once Sol was cleared; the Godot remake never did.
    /// (Interstellar_Travel)
    Tunnelers,
}

impl Milestone {
    /// The items the milestone makes available to research.
    pub fn research(self) -> &'static [ItemType] {
        match self {
            Milestone::FirstCitadelModule => &[
                ItemType::WormCore,
                ItemType::WormEngine,
                ItemType::ExfilScript,
            ],
            Milestone::HideoutCitadel => &[],
            Milestone::WormEquipment => &[
                ItemType::Crawler,
                ItemType::BuildBot,
                ItemType::Patch,
                ItemType::Sniffer,
                ItemType::BackdoorKit,
            ],
            Milestone::Daemons => &[ItemType::C2Controller, ItemType::Daemon],
            Milestone::SourceCode => &[ItemType::LegacyExploit],
            Milestone::EncryptedLinks => &[ItemType::EncryptedLink],
            Milestone::KillSwitches => &[ItemType::KillSwitch],
            Milestone::Tunnelers => &[
                ItemType::TunnelCore,
                ItemType::TunnelEngine,
                ItemType::OnionRoutes,
            ],
        }
    }
}

/// Grants every milestone reached through `events` or through what crews
/// now hold, and returns an event for each one granted.
pub(crate) fn check(data: &GameData, world: &mut World, events: &[Event]) -> Vec<Event> {
    let mut reached: Vec<(PlayerId, Milestone)> = Vec::new();
    let wars = crate::legacy::check_war(data, world);
    for event in events.iter().chain(&wars) {
        match *event {
            Event::Installed {
                player,
                host,
                item: ItemType::CitadelModule,
                installed,
                ..
            } if host == data.hideout.host => {
                reached.push((player, Milestone::FirstCitadelModule));
                if installed >= Citadel::MODULES {
                    reached.push((player, Milestone::HideoutCitadel));
                }
            }
            Event::ItemBuilt {
                player,
                item: ItemType::WormCore,
                ..
            } => reached.push((player, Milestone::WormEquipment)),
            Event::ResearchCompleted {
                player,
                item: ItemType::LegacyExploit,
                ..
            }
            | Event::WarDeclared { player, .. } => reached.push((player, Milestone::Daemons)),
            _ => {}
        }
    }

    for (&player, state) in &world.players {
        let fragment = |store: &crate::store::Store| store.get(ItemType::SourceFragment) > 0;
        let held = world
            .hosts
            .iter()
            .filter(|h| h.controller == Some(Controller::Crew(player)))
            .any(|h| fragment(&h.site.store) || fragment(&h.site.citadel.store));
        if held || fragment(&state.hideout.store) || fragment(&state.hideout.citadel.store) {
            reached.push((player, Milestone::SourceCode));
        }
    }

    for host in &world.hosts {
        let Some(Controller::Crew(player)) = host.controller else {
            continue;
        };
        let citadel = &host.site.citadel;
        if citadel.complete() && citadel.encrypted_link {
            reached.push((player, Milestone::EncryptedLinks));
        }
        if citadel.complete() && citadel.kill_switch {
            reached.push((player, Milestone::KillSwitches));
        }
    }

    // Only on a map: an empty test world has no home network to clear.
    if let Some(home) = data.hosts.get(usize::from(data.hideout.host.0))
        && world.hosts.len() == data.hosts.len()
    {
        let legacy_at_home = data.hosts.iter().zip(&world.hosts).any(|(def, state)| {
            def.network == home.network && state.controller == Some(Controller::Legacy)
        });
        if !legacy_at_home {
            reached.extend(
                world
                    .players
                    .keys()
                    .map(|&player| (player, Milestone::Tunnelers)),
            );
        }
    }

    let day = world.day;
    let mut granted = wars;
    for (player, milestone) in reached {
        let Some(state) = world.players.get_mut(&player) else {
            continue;
        };
        if !state.milestones.insert(milestone) {
            continue;
        }
        for &item in milestone.research() {
            if let Some(def) = data.research.get(&item) {
                state.unlock_research(item, def);
            }
        }
        granted.push(Event::Unlocked {
            day,
            player,
            milestone,
        });
    }
    granted
}

/// Milestones a crew has reached.
pub type Milestones = BTreeSet<Milestone>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::staff::{Staff, StaffKind};
    use crate::transport::{Berth, Destination, ModuleKind, Seat, VesselId, VesselKind};
    use crate::turn::{Orders, resolve_turn};

    const CREW: PlayerId = PlayerId(0);
    const RIVAL: PlayerId = PlayerId(1);

    fn new_game() -> (GameData, World) {
        let data = GameData::classic();
        let world = World::new_game(&data, 6, &[(CREW, "Crew"), (RIVAL, "Rival")]);
        (data, world)
    }

    fn unlocked(events: &[Event]) -> Vec<(PlayerId, Milestone)> {
        events
            .iter()
            .filter_map(|e| match *e {
                Event::Unlocked {
                    player, milestone, ..
                } => Some((player, milestone)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_first_citadel_module_opens_worm_research_once() {
        let (data, mut world) = new_game();
        let installed = |installed| Event::Installed {
            day: 1,
            player: CREW,
            host: data.hideout.host,
            item: ItemType::CitadelModule,
            installed,
        };
        let granted = check(&data, &mut world, &[installed(1), installed(2)]);
        assert_eq!(unlocked(&granted), [(CREW, Milestone::FirstCitadelModule)]);
        let player = &world.players[&CREW];
        for item in [
            ItemType::WormCore,
            ItemType::WormEngine,
            ItemType::ExfilScript,
        ] {
            assert!(player.research.contains_key(&item), "{item:?}");
        }
        assert!(
            !world.players[&RIVAL]
                .research
                .contains_key(&ItemType::WormCore)
        );
        assert!(check(&data, &mut world, &[installed(3)]).is_empty());

        let granted = check(&data, &mut world, &[installed(8)]);
        assert_eq!(unlocked(&granted), [(CREW, Milestone::HideoutCitadel)]);
    }

    #[test]
    fn a_built_worm_core_and_the_legacy_exploit_open_equipment_and_daemons() {
        let (data, mut world) = new_game();
        let events = [
            Event::ItemBuilt {
                day: 3,
                player: CREW,
                at: crate::workshop::WorkshopRef::Citadel(crate::workshop::SiteRef::Hideout),
                item: ItemType::WormCore,
            },
            Event::ResearchCompleted {
                day: 3,
                player: RIVAL,
                item: ItemType::LegacyExploit,
            },
        ];
        let granted = check(&data, &mut world, &events);
        assert_eq!(
            unlocked(&granted),
            [
                (CREW, Milestone::WormEquipment),
                (RIVAL, Milestone::Daemons)
            ]
        );
        assert!(
            world.players[&CREW]
                .research
                .contains_key(&ItemType::BuildBot)
        );
        assert!(
            world.players[&RIVAL]
                .research
                .contains_key(&ItemType::Daemon)
        );
    }

    #[test]
    fn holding_a_freed_legacy_citadel_reveals_its_technology() {
        let (data, mut world) = new_game();
        let colossus = data
            .hosts
            .iter()
            .position(|h| h.name == "Colossus")
            .unwrap();
        world.hosts[colossus].controller = Some(Controller::Crew(CREW));
        let granted = check(&data, &mut world, &[]);
        assert_eq!(
            unlocked(&granted),
            [
                (CREW, Milestone::EncryptedLinks),
                (CREW, Milestone::KillSwitches)
            ]
        );
        assert!(
            world.players[&CREW]
                .research
                .contains_key(&ItemType::EncryptedLink)
        );
    }

    #[test]
    fn a_home_network_free_of_the_legacy_net_opens_tunnelers_for_everyone() {
        let (data, mut world) = new_game();
        let home = data.host(data.hideout.host).network;
        for (def, state) in data.hosts.iter().zip(world.hosts.iter_mut()) {
            if def.network == home && state.controller == Some(Controller::Legacy) {
                state.controller = None;
            }
        }
        let granted = check(&data, &mut world, &[]);
        assert_eq!(
            unlocked(&granted),
            [(CREW, Milestone::Tunnelers), (RIVAL, Milestone::Tunnelers)]
        );
        assert!(
            world.players[&RIVAL]
                .research
                .contains_key(&ItemType::OnionRoutes)
        );
    }

    #[test]
    fn turns_grant_milestones_from_orders_and_days() {
        let (data, mut world) = new_game();
        let hideout = &mut world.players.get_mut(&CREW).unwrap().hideout;
        hideout
            .staff
            .push(Staff::new("Runner", StaffKind::Operator, 3));
        for (item, count) in [
            (ItemType::DropperCore, 1),
            (ItemType::DropperEngine, 1),
            (ItemType::ProxyChains, 20),
            (ItemType::ToolModule, 1),
            (ItemType::CitadelModule, 1),
        ] {
            hideout.store.add(item, count);
        }
        let home = data.hideout.host;
        let dropper = VesselId(0);
        let setup = vec![
            Command::Assemble {
                host: home,
                berth: Berth::Planted,
                kind: VesselKind::Dropper,
            },
            Command::Board {
                vessel: dropper,
                seat: Seat::Pilot,
                team: 0,
            },
            Command::Refuel {
                vessel: dropper,
                amount: 20,
            },
            Command::Fit {
                vessel: dropper,
                slot: 0,
                module: Some(ModuleKind::ToolModule),
            },
            Command::Load {
                vessel: dropper,
                slot: 0,
                item: ItemType::CitadelModule,
                count: 1,
            },
            Command::Dispatch {
                vessel: dropper,
                to: Destination {
                    host: home,
                    berth: Berth::Lurking,
                },
            },
        ];
        resolve_turn(&data, &mut world, &Orders::from([(CREW, setup)]), 5);
        let deploy = vec![Command::Deploy {
            vessel: dropper,
            slot: 0,
        }];
        let report = resolve_turn(&data, &mut world, &Orders::from([(CREW, deploy)]), 0);
        assert_eq!(
            unlocked(&report.events),
            [(CREW, Milestone::FirstCitadelModule)]
        );
    }
}
