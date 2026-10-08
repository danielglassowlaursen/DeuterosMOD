use serde::{Deserialize, Serialize};

use crate::data::GameData;
use crate::staff::Staff;
use crate::turn::Event;
use crate::world::World;

/// Static research parameters of one item, from the item table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchDef {
    /// Minimum research team level (1-3) able to work on the item.
    pub tech_level: u8,
    /// Speed factor; 64 for most items.
    pub multiplier: u32,
    /// Starting value of the daily accumulator; 64 for most items.
    pub initial_value: u32,
    /// Highest percentage research can reach. Source fragments start at 0
    /// and rise as fragments are recovered.
    pub limit: u8,
    /// Open to research when a crew starts.
    #[serde(default)]
    pub available_at_start: bool,
    /// Already researched when a crew starts.
    #[serde(default)]
    pub researched_at_start: bool,
}

impl Default for ResearchDef {
    fn default() -> Self {
        ResearchDef {
            tech_level: 1,
            multiplier: 64,
            initial_value: 64,
            limit: 100,
            available_at_start: false,
            researched_at_start: false,
        }
    }
}

/// One player's progress on one item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchProgress {
    /// Accumulator; every time it passes 255 the item gains 11%.
    pub value: u32,
    pub percent: u8,
    pub researched: bool,
}

impl ResearchProgress {
    pub fn new(def: &ResearchDef) -> Self {
        ResearchProgress {
            value: def.initial_value,
            percent: 1,
            researched: false,
        }
    }
}

/// Runs one day of research by `team` and returns true if the item was
/// completed that day.
///
/// Port of `Research.UpdateResearch` (Godot/Code/Platform/Screens/Research.cs):
/// the team adds `(count << level) * multiplier / 801` to the accumulator, and
/// each wrap past 255 adds 11%, so an item needs nine wraps to go from 1% to
/// 100%.
pub fn research_day(def: &ResearchDef, progress: &mut ResearchProgress, team: &Staff) -> bool {
    if progress.researched {
        return false;
    }
    let level = team.level();
    if level < def.tech_level || progress.percent >= def.limit {
        return false;
    }

    let gain = (team.count << level) * def.multiplier / 801;
    let sum = progress.value + gain;
    if sum > 255 {
        progress.value = sum & 0xFF;
        progress.percent = (progress.percent + 11).min(def.limit).min(100);
    } else {
        progress.value = sum;
    }

    if progress.percent == 100 {
        progress.researched = true;
        return true;
    }
    false
}

/// Daily research system: every player's team works on its current item.
pub(crate) fn run_day(data: &GameData, world: &mut World, events: &mut Vec<Event>) {
    let day = world.day;
    for (&player, state) in &mut world.players {
        let (Some(team), Some(item)) = (&mut state.research_team, state.current_research) else {
            continue;
        };
        let (Some(def), Some(progress)) = (data.research.get(&item), state.research.get_mut(&item))
        else {
            continue;
        };
        if research_day(def, progress, team) {
            events.push(Event::ResearchCompleted { day, player, item });
            if let Some(level) = team.record_action() {
                events.push(Event::StaffPromoted {
                    day,
                    player,
                    kind: team.kind,
                    level,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::staff::StaffKind;

    fn days_until_done(def: &ResearchDef, team: &Staff) -> Vec<(u32, u8)> {
        let mut progress = ResearchProgress::new(def);
        let mut steps = Vec::new();
        for day in 1..=1000 {
            let before = progress.percent;
            let done = research_day(def, &mut progress, team);
            if progress.percent != before {
                steps.push((day, progress.percent));
            }
            if done {
                break;
            }
        }
        steps
    }

    #[test]
    fn hundred_technicians_finish_a_standard_item_in_150_days() {
        // 100 technicians gain (100 << 1) * 64 / 801 = 15 a day from 64.
        let team = Staff::new("Lead", StaffKind::Analyst, 100);
        let steps = days_until_done(&ResearchDef::default(), &team);
        let days: Vec<u32> = steps.iter().map(|&(day, _)| day).collect();
        assert_eq!(days, [13, 30, 47, 64, 82, 99, 116, 133, 150]);
        assert_eq!(steps.first(), Some(&(13, 12)));
        assert_eq!(steps.last(), Some(&(150, 100)));
    }

    #[test]
    fn team_below_tech_level_makes_no_progress() {
        let def = ResearchDef {
            tech_level: 2,
            ..ResearchDef::default()
        };
        let team = Staff::new("Lead", StaffKind::Analyst, 250);
        let mut progress = ResearchProgress::new(&def);
        for _ in 0..100 {
            assert!(!research_day(&def, &mut progress, &team));
        }
        assert_eq!(progress, ResearchProgress::new(&def));
    }

    #[test]
    fn progress_stops_at_the_limit() {
        let def = ResearchDef {
            limit: 23,
            ..ResearchDef::default()
        };
        let team = Staff::new("Lead", StaffKind::Analyst, 250);
        let mut progress = ResearchProgress::new(&def);
        for _ in 0..1000 {
            assert!(!research_day(&def, &mut progress, &team));
        }
        assert_eq!(progress.percent, 23);
    }
}
