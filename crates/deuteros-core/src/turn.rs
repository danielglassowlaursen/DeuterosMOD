use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::command::{Command, CommandError};
use crate::ids::{Day, PlayerId};
use crate::items::ItemType;
use crate::research;
use crate::staff::StaffKind;
use crate::world::{GameData, World};

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
/// player's commands in the order given. An invalid command is skipped and
/// reported; it never aborts the turn.
pub fn resolve_turn(data: &GameData, world: &mut World, orders: &Orders, days: u32) -> TurnReport {
    let mut rejected = Vec::new();
    for (&player, commands) in orders {
        for (index, command) in commands.iter().enumerate() {
            if let Err(error) = command.apply(world, player) {
                rejected.push(RejectedCommand {
                    player,
                    index,
                    error,
                });
            }
        }
    }

    let first_day = world.day + 1;
    let mut events = Vec::new();
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
    // training, production, ships, research, Methanoid drones, alien
    // messages, MTX. Systems are added here in that order as they are ported.
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
            .insert(ItemType::SChassis, ResearchDef::default());
        data.research.insert(
            ItemType::IChassis,
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
            player.research_team = Some(Staff::new("Lead", StaffKind::Research, 100));
            player.unlock_research(ItemType::SChassis, &data.research[&ItemType::SChassis]);
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
        let orders = Orders::from([(ALICE, vec![set_research(ItemType::SChassis)])]);

        let report = resolve_turn(&data, &mut world, &orders, 149);
        assert_eq!((report.first_day, report.last_day), (1, 149));
        assert!(report.events.is_empty());
        assert_eq!(
            world.players[&ALICE].research[&ItemType::SChassis].percent,
            89
        );

        let report = resolve_turn(&data, &mut world, &Orders::new(), 10);
        assert_eq!(
            report.events,
            [Event::ResearchCompleted {
                day: 150,
                player: ALICE,
                item: ItemType::SChassis,
            }]
        );
        // Bob gave no orders, so his team stayed idle.
        assert_eq!(world.players[&BOB].research[&ItemType::SChassis].percent, 1);
    }

    #[test]
    fn invalid_commands_are_reported_and_the_rest_still_apply() {
        let data = data();
        let mut world = world(&data);
        let orders = Orders::from([
            (
                ALICE,
                vec![
                    set_research(ItemType::IChassis),
                    set_research(ItemType::SChassis),
                ],
            ),
            (PlayerId(9), vec![set_research(ItemType::SChassis)]),
        ]);

        let report = resolve_turn(&data, &mut world, &orders, 1);
        assert_eq!(
            report.rejected,
            [
                RejectedCommand {
                    player: ALICE,
                    index: 0,
                    error: CommandError::ResearchUnavailable(ItemType::IChassis),
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
            Some(ItemType::SChassis)
        );
    }

    #[test]
    fn finished_research_cannot_be_selected_again() {
        let data = data();
        let mut world = world(&data);
        let orders = Orders::from([(ALICE, vec![set_research(ItemType::SChassis)])]);
        resolve_turn(&data, &mut world, &orders, 150);

        let report = resolve_turn(&data, &mut world, &orders, 1);
        assert_eq!(
            report.rejected[0].error,
            CommandError::AlreadyResearched(ItemType::SChassis)
        );
    }

    #[test]
    fn a_saved_and_restored_world_resolves_identically() {
        let data = data();
        let mut original = world(&data);
        let orders = Orders::from([
            (ALICE, vec![set_research(ItemType::SChassis)]),
            (BOB, vec![set_research(ItemType::SChassis)]),
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
