use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::items::ItemType;

/// A stockpile of items, as kept on a host, in a citadel or in the hideout.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Store(BTreeMap<ItemType, u32>);

impl Store {
    /// Most units of one item a store holds, as in the original.
    pub const CAP: u32 = 50_000;

    pub fn get(&self, item: ItemType) -> u32 {
        self.0.get(&item).copied().unwrap_or(0)
    }

    /// Adds up to `count` units without passing [`Store::CAP`] and returns
    /// how many fitted.
    pub fn add(&mut self, item: ItemType, count: u32) -> u32 {
        let held = self.get(item);
        let added = count.min(Self::CAP.saturating_sub(held));
        if added > 0 {
            self.0.insert(item, held + added);
        }
        added
    }

    /// Removes `count` units if the store has them all; otherwise leaves it
    /// unchanged and returns false.
    pub fn take(&mut self, item: ItemType, count: u32) -> bool {
        let held = self.get(item);
        if held < count {
            return false;
        }
        if held == count {
            self.0.remove(&item);
        } else {
            self.0.insert(item, held - count);
        }
        true
    }

    pub fn iter(&self) -> impl Iterator<Item = (ItemType, u32)> + '_ {
        self.0.iter().map(|(&item, &count)| (item, count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_stops_at_the_cap() {
        let mut store = Store::default();
        assert_eq!(store.add(ItemType::Compute, 49_990), 49_990);
        assert_eq!(store.add(ItemType::Compute, 25), 10);
        assert_eq!(store.add(ItemType::Compute, 1), 0);
        assert_eq!(store.get(ItemType::Compute), Store::CAP);
    }

    #[test]
    fn take_is_all_or_nothing() {
        let mut store = Store::default();
        store.add(ItemType::Code, 5);
        assert!(!store.take(ItemType::Code, 6));
        assert_eq!(store.get(ItemType::Code), 5);
        assert!(store.take(ItemType::Code, 5));
        assert_eq!(store.iter().count(), 0, "an emptied item leaves no entry");
    }
}
