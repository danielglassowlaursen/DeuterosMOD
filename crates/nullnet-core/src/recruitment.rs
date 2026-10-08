//! Recruiting staff in the hideout. Port of `Training.ChildDayTick`
//! (Godot/Code/Objects/Training.cs) and the limits on its screen
//! (Godot/Code/Platform/Screens/Training.cs).
//!
//! Each kind of staff has one course. A crew enrols recruits while the
//! course is idle; the course starts the next day and graduates them when it
//! has run its length. Analysts join the crew's single analyst team, coders
//! join the hideout workshop's team, and each batch of operators becomes a
//! team of its own in one of the hideout's staff slots.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::command::CommandError;
use crate::data::GameData;
use crate::ids::Day;
use crate::site::STAFF_SLOTS;
use crate::staff::{Staff, StaffKind};
use crate::turn::Event;
use crate::world::{Player, World};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recruitment {
    /// Recruits left to draw on for the rest of the game.
    pub available: u32,
    pub courses: BTreeMap<StaffKind, Course>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Course {
    pub enrolled: u32,
    /// The day before the course's first day; `None` while it is idle.
    pub started: Option<Day>,
}

impl Course {
    pub fn running(&self) -> bool {
        self.started.is_some()
    }
}

/// Sets how many recruits the next course of `kind` takes; 0 cancels.
pub(crate) fn enrol(
    data: &GameData,
    player: &mut Player,
    kind: StaffKind,
    count: u32,
) -> Result<(), CommandError> {
    let def = &data.recruitment.courses[&kind];
    let course = player.recruitment.courses.entry(kind).or_default();
    if course.running() {
        return Err(CommandError::CourseRunning(kind));
    }
    let team = match kind {
        StaffKind::Analyst => player.research_team.as_ref(),
        StaffKind::Coder => player.workshop.coders.as_ref(),
        StaffKind::Operator => None,
    }
    .map_or(0, |staff| staff.count);
    let enrolled_elsewhere: u32 = player
        .recruitment
        .courses
        .iter()
        .filter(|&(&other, _)| other != kind)
        .map(|(_, course)| course.enrolled)
        .sum();
    if count > def.batch_max
        || def.team_max.is_some_and(|max| team + count > max)
        || enrolled_elsewhere + count > player.recruitment.available
    {
        return Err(CommandError::TooManyRecruits(kind));
    }
    player.recruitment.courses.entry(kind).or_default().enrolled = count;
    Ok(())
}

pub(crate) fn run_day(data: &GameData, world: &mut World, events: &mut Vec<Event>) {
    let day = world.day;
    let World {
        players,
        next_handle,
        ..
    } = world;
    for (&id, player) in players.iter_mut() {
        for (&kind, def) in &data.recruitment.courses {
            let course = player.recruitment.courses.entry(kind).or_default();
            match course.started {
                None if course.enrolled > 0 => course.started = Some(day - 1),
                Some(start) if day - start >= def.days => {
                    let count = course.enrolled;
                    if graduate(player, kind, count, next_handle) {
                        player.recruitment.available -= count;
                        player.recruitment.courses.insert(kind, Course::default());
                        events.push(Event::RecruitsGraduated {
                            day,
                            player: id,
                            kind,
                            count,
                        });
                    }
                }
                _ => {}
            }
        }
    }
}

/// Places a graduating batch; returns false if it has to wait, as operators
/// do while every staff slot in the hideout is taken.
fn graduate(player: &mut Player, kind: StaffKind, count: u32, next_handle: &mut u32) -> bool {
    let team = match kind {
        StaffKind::Analyst => &mut player.research_team,
        StaffKind::Coder => &mut player.workshop.coders,
        StaffKind::Operator => {
            if player.hideout.staff.len() >= STAFF_SLOTS {
                return false;
            }
            player
                .hideout
                .staff
                .push(Staff::new(handle(next_handle), kind, count));
            return true;
        }
    };
    team.get_or_insert_with(|| Staff::new(handle(next_handle), kind, 0))
        .count += count;
    true
}

const HANDLES: [&str; 32] = [
    "Zer0", "Nyx", "Glitch", "Cipher", "Rook", "Vex", "Kestrel", "Halcyon", "Byte", "Tess", "Echo",
    "Wraith", "Mantis", "Juno", "Static", "Raven", "Hex", "Lumen", "Onyx", "Specter", "Tamsin",
    "Volt", "Quill", "Sable", "Nova", "Drift", "Jinx", "Morrow", "Ash", "Kilo", "Pixel", "Loki",
];

/// The next team leader's handle. Handles are shared by every crew and get
/// a number once the list has been used up.
fn handle(next: &mut u32) -> String {
    let index = *next as usize;
    *next += 1;
    let name = HANDLES[index % HANDLES.len()];
    match index / HANDLES.len() {
        0 => name.to_string(),
        round => format!("{name}-{}", round + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;
    use crate::turn::{Orders, resolve_turn};
    use crate::{Command, World};

    const CREW: PlayerId = PlayerId(0);

    fn new_game() -> (GameData, World) {
        let data = GameData::classic();
        let world = World::new_game(&data, 5, &[(CREW, "Crew")]);
        (data, world)
    }

    fn recruit(kind: StaffKind, count: u32) -> Orders {
        Orders::from([(CREW, vec![Command::Recruit { kind, count }])])
    }

    #[test]
    fn a_course_graduates_on_its_twenty_fourth_day() {
        let (data, mut world) = new_game();
        let report = resolve_turn(&data, &mut world, &recruit(StaffKind::Analyst, 100), 23);
        assert!(report.rejected.is_empty());
        assert!(world.players[&CREW].research_team.is_none());

        let report = resolve_turn(&data, &mut world, &Orders::new(), 1);
        assert_eq!(
            report.events,
            [Event::RecruitsGraduated {
                day: 24,
                player: CREW,
                kind: StaffKind::Analyst,
                count: 100,
            }]
        );
        let player = &world.players[&CREW];
        let team = player.research_team.as_ref().unwrap();
        assert_eq!((team.leader.as_str(), team.count), ("Zer0", 100));
        assert_eq!(player.recruitment.available, 5900);
    }

    #[test]
    fn enrolment_respects_the_course_limits() {
        let (data, mut world) = new_game();
        let player = world.players.get_mut(&CREW).unwrap();
        assert_eq!(
            enrol(&data, player, StaffKind::Operator, 42),
            Err(CommandError::TooManyRecruits(StaffKind::Operator))
        );
        player.research_team = Some(Staff::new("Lead", StaffKind::Analyst, 200));
        assert_eq!(
            enrol(&data, player, StaffKind::Analyst, 51),
            Err(CommandError::TooManyRecruits(StaffKind::Analyst))
        );
        assert_eq!(enrol(&data, player, StaffKind::Analyst, 50), Ok(()));

        player
            .recruitment
            .courses
            .get_mut(&StaffKind::Analyst)
            .unwrap()
            .started = Some(0);
        assert_eq!(
            enrol(&data, player, StaffKind::Analyst, 10),
            Err(CommandError::CourseRunning(StaffKind::Analyst))
        );
    }

    #[test]
    fn coders_join_the_hideout_workshop_and_operators_get_their_own_teams() {
        let (data, mut world) = new_game();
        let orders = Orders::from([(
            CREW,
            vec![
                Command::Recruit {
                    kind: StaffKind::Coder,
                    count: 80,
                },
                Command::Recruit {
                    kind: StaffKind::Operator,
                    count: 41,
                },
            ],
        )]);
        resolve_turn(&data, &mut world, &orders, 24);
        resolve_turn(&data, &mut world, &recruit(StaffKind::Operator, 20), 25);

        let player = &world.players[&CREW];
        assert_eq!(player.workshop.coders.as_ref().unwrap().count, 80);
        let teams: Vec<u32> = player.hideout.staff.iter().map(|s| s.count).collect();
        assert_eq!(teams, [41, 20]);
    }

    #[test]
    fn operators_wait_for_a_free_staff_slot() {
        let (data, mut world) = new_game();
        let player = world.players.get_mut(&CREW).unwrap();
        for _ in 0..STAFF_SLOTS {
            player
                .hideout
                .staff
                .push(Staff::new("Busy", StaffKind::Operator, 1));
        }
        resolve_turn(&data, &mut world, &recruit(StaffKind::Operator, 10), 40);
        let player = &world.players[&CREW];
        assert_eq!(player.hideout.staff.len(), STAFF_SLOTS);
        assert!(player.recruitment.courses[&StaffKind::Operator].running());
    }

    #[test]
    fn handles_run_through_the_list_then_number_themselves() {
        let mut next = 0;
        assert_eq!(handle(&mut next), "Zer0");
        next = HANDLES.len() as u32;
        assert_eq!(handle(&mut next), "Zer0-2");
    }
}
