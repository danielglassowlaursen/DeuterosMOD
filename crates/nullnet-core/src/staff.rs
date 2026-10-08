use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum StaffKind {
    Research,
    Production,
    Marines,
}

impl StaffKind {
    /// Actions needed to reach level 2 and level 3 (Godot/Code/Objects/Staff.cs).
    fn level_thresholds(self) -> [u32; 2] {
        match self {
            StaffKind::Research => [6, 9],
            StaffKind::Production => [6, 12],
            StaffKind::Marines => [10, 40],
        }
    }
}

/// A team led by a named leader. Experience comes from completed actions
/// (research finished, items built, take-offs, deployments) and sets the
/// team's level from 1 to 3.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Staff {
    pub leader: String,
    pub kind: StaffKind,
    pub count: u32,
    actions_taken: u32,
}

impl Staff {
    pub fn new(leader: impl Into<String>, kind: StaffKind, count: u32) -> Self {
        Staff {
            leader: leader.into(),
            kind,
            count,
            actions_taken: 0,
        }
    }

    pub fn level(&self) -> u8 {
        let [second, third] = self.kind.level_thresholds();
        if self.actions_taken >= third {
            3
        } else if self.actions_taken >= second {
            2
        } else {
            1
        }
    }

    /// Records one completed action and returns the new level if the team
    /// was promoted by it.
    pub fn record_action(&mut self) -> Option<u8> {
        let before = self.level();
        self.actions_taken += 1;
        let after = self.level();
        (after != before).then_some(after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn levels_after_each_action(kind: StaffKind, actions: u32) -> Vec<(u32, u8)> {
        let mut staff = Staff::new("Test", kind, 1);
        (1..=actions)
            .filter_map(|n| staff.record_action().map(|level| (n, level)))
            .collect()
    }

    #[test]
    fn promotion_thresholds_match_the_original() {
        assert_eq!(
            levels_after_each_action(StaffKind::Research, 50),
            [(6, 2), (9, 3)]
        );
        assert_eq!(
            levels_after_each_action(StaffKind::Production, 50),
            [(6, 2), (12, 3)]
        );
        assert_eq!(
            levels_after_each_action(StaffKind::Marines, 50),
            [(10, 2), (40, 3)]
        );
    }
}
