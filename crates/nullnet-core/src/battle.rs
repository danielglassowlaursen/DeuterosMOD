//! Battles between swarms of daemons. Port of the battle rules in
//! Godot/Code/Platform/Screens/BattleLogic.cs, run to the end in one go
//! with the world's own generator, so a turn resolves the same way wherever
//! it is replayed.
//!
//! Each side's power is its daemons times its operator's level plus four.
//! Every tick runs two rounds. In a round each side draws, when its counter
//! is empty, how many rounds its next daemon survives: the stronger side
//! draws from the first row of the table, the weaker from the row its
//! power ratio points at, so a swarm twice as strong loses daemons about
//! half as often, and a swarm eight times as strong hardly loses any.

use serde::{Deserialize, Serialize};

use crate::rng::Rng;

/// Rounds a daemon survives, by power ratio (rows of eight) and a die.
const SURVIVAL: [u8; 64] = [
    0x10, 0x1E, 0x0C, 0x0F, 0x05, 0x12, 0x17, 0x1E, //
    0x09, 0x0B, 0x07, 0x09, 0x0E, 0x09, 0x0C, 0x10, //
    0x06, 0x08, 0x32, 0x07, 0x0A, 0x07, 0x09, 0x0D, //
    0x05, 0x06, 0x04, 0x05, 0x08, 0x06, 0x07, 0x0A, //
    0x04, 0x05, 0x03, 0x04, 0x06, 0x04, 0x05, 0x07, //
    0x03, 0x04, 0x02, 0x03, 0x04, 0x03, 0x04, 0x05, //
    0x02, 0x03, 0x01, 0x02, 0x03, 0x02, 0x0C, 0x03, //
    0x01, 0x02, 0x01, 0x02, 0x02, 0x02, 0x0C, 0x04, //
];
/// The table used while the two sides are within a factor of two.
const SURVIVAL_EVEN: [u8; 8] = [0x0A, 0x28, 0x03, 0x05, 0x1A, 0x01, 0x14, 0x01];

/// Ticks between the snapshots a replay is drawn from.
const SNAPSHOT_EVERY: u32 = 4;
/// A guard against a battle that never ends; no real one gets near it.
const MAX_TICKS: u32 = 100_000;

/// One side of a battle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Side {
    pub daemons: u32,
    /// The operator's level, 0 when there is none; the Legacy Net has none.
    pub level: u8,
}

impl Side {
    fn strength(self) -> u32 {
        u32::from(self.level) + 4
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// The defender has no daemons left.
    AttackerWon,
    /// The attacker has no daemons left.
    DefenderWon,
    /// The defender pulled out once it had lost too many.
    DefenderFled,
}

/// How a battle went, with enough of its course to replay it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub attacker: Side,
    pub defender: Side,
    pub outcome: Outcome,
    /// Daemons left on each side every few ticks, ending with the result.
    pub course: Vec<(u32, u32)>,
}

impl Report {
    pub fn attacker_left(&self) -> u32 {
        self.course.last().map_or(0, |c| c.0)
    }

    pub fn defender_left(&self) -> u32 {
        self.course.last().map_or(0, |c| c.1)
    }
}

/// Fights to the end. The defender flees once it has `flee_at` daemons or
/// fewer left (0 never flees); the attacker never does.
pub fn fight(rng: &mut Rng, attacker: Side, defender: Side, flee_at: u32) -> Report {
    let (a_strength, d_strength) = (attacker.strength(), defender.strength());
    let (mut a, mut d) = (attacker.daemons, defender.daemons);
    let (mut a_counter, mut d_counter) = (0u32, 0u32);
    let mut course = vec![(a, d)];
    // A side with nothing to fight with has lost before it starts.
    let mut outcome = match (a, d) {
        (_, 0) => Some(Outcome::AttackerWon),
        (0, _) => Some(Outcome::DefenderWon),
        _ => None,
    };

    let mut tick = 0;
    while outcome.is_none() && tick < MAX_TICKS {
        tick += 1;
        for _ in 0..2 {
            if outcome.is_some() {
                break;
            }
            let (a_power, d_power) = (a * a_strength, d * d_strength);

            // The weaker side's row is the power ratio, capped at seven; the
            // stronger side draws from the first row. Within a factor of two
            // both draw from the even table.
            let (mut a_row, mut d_row, table): (u32, u32, &[u8]) = if d_power > a_power {
                let ratio = (d_power + 1) / (a_power + 1);
                if ratio == 1 {
                    (0, 0, &SURVIVAL_EVEN)
                } else {
                    (ratio.min(7), 0, &SURVIVAL)
                }
            } else {
                let ratio = (a_power + 1) / (d_power + 1);
                if ratio == 1 {
                    (0, 0, &SURVIVAL_EVEN)
                } else {
                    (0, ratio.min(7), &SURVIVAL)
                }
            };
            a_row = a_row * 8 + rng.below(8);
            d_row = d_row * 8 + rng.below(8);
            if a_counter == 0 {
                a_counter = u32::from(table[a_row as usize]);
                if d_power > a_power {
                    a_counter = a_counter / 2 + 1;
                }
            }
            if d_counter == 0 {
                d_counter = u32::from(table[d_row as usize]);
                if a_power >= d_power {
                    d_counter = d_counter / 2 + 1;
                }
            }

            a_counter -= 1;
            if a_counter == 0 {
                a -= 1;
                if a == 0 {
                    outcome = Some(Outcome::DefenderWon);
                }
            }
            d_counter -= 1;
            if d_counter == 0 {
                d -= 1;
                if d == 0 {
                    outcome = Some(Outcome::AttackerWon);
                } else if flee_at > 0 && d <= flee_at && outcome.is_none() {
                    outcome = Some(Outcome::DefenderFled);
                }
            }
        }
        if tick.is_multiple_of(SNAPSHOT_EVERY) || outcome.is_some() {
            course.push((a, d));
        }
    }
    if course.last() != Some(&(a, d)) {
        course.push((a, d));
    }

    Report {
        attacker,
        defender,
        outcome: outcome.unwrap_or(Outcome::DefenderWon),
        course,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(daemons: u32, level: u8) -> Side {
        Side { daemons, level }
    }

    #[test]
    fn the_stronger_swarm_wins_and_keeps_most_of_its_daemons() {
        let mut rng = Rng::new(1, 1);
        let report = fight(&mut rng, side(200, 3), side(50, 0), 0);
        assert_eq!(report.outcome, Outcome::AttackerWon);
        assert_eq!(report.defender_left(), 0);
        assert!(
            report.attacker_left() > 150,
            "left {}",
            report.attacker_left()
        );

        let report = fight(&mut rng, side(20, 1), side(200, 0), 0);
        assert_eq!(report.outcome, Outcome::DefenderWon);
        assert_eq!(report.attacker_left(), 0);
        assert!(report.defender_left() > 150);
    }

    #[test]
    fn a_defender_flees_at_its_limit_and_even_fights_are_close() {
        let mut rng = Rng::new(2, 1);
        let report = fight(&mut rng, side(100, 1), side(100, 1), 50);
        assert_eq!(report.outcome, Outcome::DefenderFled);
        assert_eq!(report.defender_left(), 50);
        assert!(
            report.attacker_left() >= 20,
            "left {}",
            report.attacker_left()
        );
        assert!(
            report.course.len() > 3,
            "the course has snapshots to replay"
        );
        assert_eq!(report.course[0], (100, 100));
        assert_eq!(
            *report.course.last().unwrap(),
            (report.attacker_left(), report.defender_left())
        );
    }

    #[test]
    fn the_same_seed_fights_the_same_battle() {
        let a = fight(&mut Rng::new(9, 2), side(80, 2), side(120, 0), 0);
        let b = fight(&mut Rng::new(9, 2), side(80, 2), side(120, 0), 0);
        assert_eq!(a, b);
        let c = fight(&mut Rng::new(10, 2), side(80, 2), side(120, 0), 0);
        assert_ne!(a.course, c.course);
    }

    #[test]
    fn an_empty_side_loses_at_once() {
        let report = fight(&mut Rng::new(1, 1), side(10, 1), side(0, 0), 0);
        assert_eq!(report.outcome, Outcome::AttackerWon);
        assert_eq!(report.attacker_left(), 10);
    }
}
