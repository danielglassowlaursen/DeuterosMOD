use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::command::{Command, CommandError};
use crate::data::GameData;
use crate::ids::{Day, HostId, PlayerId};
use crate::items::ItemType;
use crate::staff::StaffKind;
use crate::transport::{self, AbortReason, Berth, VesselId};
use crate::workshop::{self, WorkshopRef};
use crate::world::World;
use crate::{mining, recruitment, research};

/// Every player's orders for one turn.
pub type Orders = BTreeMap<PlayerId, Vec<Command>>;

/// Something that happened while a turn was resolved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    ResearchCompleted {
        day: Day,
        player: PlayerId,
        item: ItemType,
    },
    StaffPromoted {
        day: Day,
        player: PlayerId,
        kind: StaffKind,
        level: u8,
    },
    RecruitsGraduated {
        day: Day,
        player: PlayerId,
        kind: StaffKind,
        count: u32,
    },
    ItemBuilt {
        day: Day,
        player: PlayerId,
        at: WorkshopRef,
        item: ItemType,
    },
    /// A crew installed its first citadel module or backdoor kit on a free
    /// host and now holds it.
    HostClaimed {
        day: Day,
        player: PlayerId,
        host: HostId,
    },
    /// A citadel module or backdoor kit went in; `installed` is the new count.
    Installed {
        day: Day,
        player: PlayerId,
        host: HostId,
        item: ItemType,
        installed: u32,
    },
    VesselArrived {
        day: Day,
        player: PlayerId,
        vessel: VesselId,
        host: HostId,
        berth: Berth,
    },
    /// A vessel gave up on its destination.
    VesselStopped {
        day: Day,
        player: PlayerId,
        vessel: VesselId,
        reason: AbortReason,
    },
    /// A vessel lurked too long without anonymisation and was traced.
    VesselBurned {
        day: Day,
        player: PlayerId,
        vessel: VesselId,
        host: HostId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectedCommand {
    pub player: PlayerId,
    /// Position of the command in the player's order list.
    pub index: usize,
    pub error: CommandError,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnReport {
    pub first_day: Day,
    pub last_day: Day,
    pub rejected: Vec<RejectedCommand>,
    pub events: Vec<Event>,
}

/// Applies every player's orders, then advances the world `days` days.
///
/// Orders are applied before the first day, in player-id order and each
/// player's commands in the order given; what they do at once (a host
/// claimed, a vessel setting off) is reported on the day they were given.
/// An invalid command is skipped and reported; it never aborts the turn.
pub fn resolve_turn(data: &GameData, world: &mut World, orders: &Orders, days: u32) -> TurnReport {
    let mut rejected = Vec::new();
    let mut events = Vec::new();
    for (&player, commands) in orders {
        for (index, command) in commands.iter().enumerate() {
            if let Err(error) = command.apply(data, world, player, &mut events) {
                rejected.push(RejectedCommand {
                    player,
                    index,
                    error,
                });
            }
        }
    }

    let first_day = world.day + 1;
    for _ in 0..days {
        step_day(data, world, &mut events);
    }

    TurnReport {
        first_day,
        last_day: world.day,
        rejected,
        events,
    }
}

fn step_day(data: &GameData, world: &mut World, events: &mut Vec<Event>) {
    world.day += 1;
    // The original runs its daily systems in this order: unlocks, mining,
    // training, production, ships, research, Legacy daemons, Legacy
    // transmissions, encrypted links. Systems are added here in that order
    // as they are ported.
    mining::run_day(data, world);
    recruitment::run_day(data, world, events);
    workshop::run_all(data, world, events);
    transport::run_day(data, world, events);
    research::run_day(data, world, events);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::ResearchDef;
    use crate::staff::Staff;
    use crate::world::Player;

    const ALICE: PlayerId = PlayerId(0);
    const BOB: PlayerId = PlayerId(1);

    fn data() -> GameData {
        let mut data = GameData::default();
        data.research
            .insert(ItemType::DropperCore, ResearchDef::default());
        data.research.insert(
            ItemType::WormCore,
            ResearchDef {
                tech_level: 2,
                ..ResearchDef::default()
            },
        );
        data
    }

    fn world(data: &GameData) -> World {
        let mut world = World::new(2026);
        for (id, name) in [(ALICE, "Alice"), (BOB, "Bob")] {
            let mut player = Player::new(name);
            player.research_team = Some(Staff::new("Lead", StaffKind::Analyst, 100));
            player.unlock_research(
                ItemType::DropperCore,
                &data.research[&ItemType::DropperCore],
            );
            world.players.insert(id, player);
        }
        world
    }

    fn set_research(item: ItemType) -> Command {
        Command::SetResearch { item }
    }

    #[test]
    fn research_completes_on_the_day_the_rules_predict() {
        let data = data();
        let mut world = world(&data);
        let orders = Orders::from([(ALICE, vec![set_research(ItemType::DropperCore)])]);

        let report = resolve_turn(&data, &mut world, &orders, 149);
        assert_eq!((report.first_day, report.last_day), (1, 149));
        assert!(report.events.is_empty());
        assert_eq!(
            world.players[&ALICE].research[&ItemType::DropperCore].percent,
            89
        );

        let report = resolve_turn(&data, &mut world, &Orders::new(), 10);
        assert_eq!(
            report.events,
            [Event::ResearchCompleted {
                day: 150,
                player: ALICE,
                item: ItemType::DropperCore,
            }]
        );
        // Bob gave no orders, so his team stayed idle.
        assert_eq!(
            world.players[&BOB].research[&ItemType::DropperCore].percent,
            1
        );
    }

    #[test]
    fn invalid_commands_are_reported_and_the_rest_still_apply() {
        let data = data();
        let mut world = world(&data);
        let orders = Orders::from([
            (
                ALICE,
                vec![
                    set_research(ItemType::WormCore),
                    set_research(ItemType::DropperCore),
                ],
            ),
            (PlayerId(9), vec![set_research(ItemType::DropperCore)]),
        ]);

        let report = resolve_turn(&data, &mut world, &orders, 1);
        assert_eq!(
            report.rejected,
            [
                RejectedCommand {
                    player: ALICE,
                    index: 0,
                    error: CommandError::ResearchUnavailable(ItemType::WormCore),
                },
                RejectedCommand {
                    player: PlayerId(9),
                    index: 0,
                    error: CommandError::UnknownPlayer(PlayerId(9)),
                },
            ]
        );
        assert_eq!(
            world.players[&ALICE].current_research,
            Some(ItemType::DropperCore)
        );
    }

    #[test]
    fn finished_research_cannot_be_selected_again() {
        let data = data();
        let mut world = world(&data);
        let orders = Orders::from([(ALICE, vec![set_research(ItemType::DropperCore)])]);
        resolve_turn(&data, &mut world, &orders, 150);

        let report = resolve_turn(&data, &mut world, &orders, 1);
        assert_eq!(
            report.rejected[0].error,
            CommandError::AlreadyResearched(ItemType::DropperCore)
        );
    }

    #[test]
    fn hideouts_mine_on_even_days() {
        let data = GameData::classic();
        let mut world = World::new_game(&data, 3, &[(ALICE, "Alice")]);
        let mined = |world: &World| -> u32 {
            world.players[&ALICE]
                .hideout
                .store
                .iter()
                .map(|(_, n)| n)
                .sum()
        };

        // Day 2 finds the first veins; day 4 is the first extraction.
        resolve_turn(&data, &mut world, &Orders::new(), 3);
        assert_eq!(mined(&world), 0);
        resolve_turn(&data, &mut world, &Orders::new(), 1);
        let first = mined(&world);
        assert!(first > 0);
        resolve_turn(&data, &mut world, &Orders::new(), 1);
        assert_eq!(mined(&world), first, "nothing on odd day 5");
    }

    #[test]
    fn a_crew_mines_recruits_builds_and_installs_a_second_tap() {
        use crate::workshop::{SiteRef, WorkshopRef};

        let data = GameData::classic();
        let mut world = World::new_game(&data, 3, &[(ALICE, "Alice")]);
        let recruit = Command::Recruit {
            kind: StaffKind::Coder,
            count: 100,
        };
        resolve_turn(
            &data,
            &mut world,
            &Orders::from([(ALICE, vec![recruit])]),
            30,
        );
        let hideout = &world.players[&ALICE].hideout;
        assert!(
            hideout.store.get(ItemType::Compute) >= 3,
            "{:?}",
            hideout.store
        );

        let build = Command::Build {
            at: WorkshopRef::Hideout,
            item: ItemType::Tap,
        };
        let report = resolve_turn(&data, &mut world, &Orders::from([(ALICE, vec![build])]), 50);
        assert!(report.rejected.is_empty(), "{:?}", report.rejected);
        assert!(report.events.iter().any(|e| matches!(
            e,
            Event::ItemBuilt {
                item: ItemType::Tap,
                ..
            }
        )));

        let install = Command::InstallTaps {
            site: SiteRef::Hideout,
            count: 1,
        };
        let report = resolve_turn(
            &data,
            &mut world,
            &Orders::from([(ALICE, vec![install])]),
            1,
        );
        assert!(report.rejected.is_empty(), "{:?}", report.rejected);
        let hideout = &world.players[&ALICE].hideout;
        assert_eq!((hideout.taps, hideout.store.get(ItemType::Tap)), (2, 0));

        let again = Command::InstallTaps {
            site: SiteRef::Hideout,
            count: 1,
        };
        let report = resolve_turn(&data, &mut world, &Orders::from([(ALICE, vec![again])]), 1);
        assert_eq!(
            report.rejected[0].error,
            CommandError::MissingResources(ItemType::Tap)
        );
    }

    #[test]
    fn a_new_game_resolves_identically_after_save_and_restore() {
        let data = GameData::classic();
        let mut original = World::new_game(&data, 11, &[(ALICE, "Alice"), (BOB, "Bob")]);
        resolve_turn(&data, &mut original, &Orders::new(), 30);

        let saved = serde_json::to_string(&original).unwrap();
        let mut restored: World = serde_json::from_str(&saved).unwrap();
        let a = resolve_turn(&data, &mut original, &Orders::new(), 100);
        let b = resolve_turn(&data, &mut restored, &Orders::new(), 100);
        assert_eq!(a, b);
        assert_eq!(original, restored);
    }

    #[test]
    fn a_saved_and_restored_world_resolves_identically() {
        let data = data();
        let mut original = world(&data);
        let orders = Orders::from([
            (ALICE, vec![set_research(ItemType::DropperCore)]),
            (BOB, vec![set_research(ItemType::DropperCore)]),
        ]);
        resolve_turn(&data, &mut original, &orders, 40);

        let saved = serde_json::to_string(&original).unwrap();
        let mut restored: World = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored, original);

        let a = resolve_turn(&data, &mut original, &Orders::new(), 200);
        let b = resolve_turn(&data, &mut restored, &Orders::new(), 200);
        assert_eq!(a, b);
        assert_eq!(original, restored);
    }
}
