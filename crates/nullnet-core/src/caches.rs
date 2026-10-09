//! Fields of abandoned data caches, the asteroid belt of Deuteros. Port of
//! the scanning in Godot/Code/Objects/Asteroid.cs and the mining in
//! Godot/Code/Platform/Screens/ModuleScenes/AMA.cs.
//!
//! A vessel lurking at a cache field with a crawler or a sniffer aboard
//! scans every day. A sniffer grabs a small cache whole into a data
//! container, as the original's grapple towed small asteroids home; a
//! crawler mines a large cache it has found, a few dozen units every five
//! days into a data container; and a sniffer now and then turns up a
//! fragment of the Legacy Net's source code.

use serde::{Deserialize, Serialize};

use crate::data::GameData;
use crate::items::ItemType;
use crate::transport::{Berth, Cargo, Module, VesselId, VesselState};
use crate::turn::Event;
use crate::world::World;

/// A cache a vessel's scanner is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cache {
    pub resource: ItemType,
    /// 1 to 8; only [`Cache::LARGE`] and up can be mined.
    pub size: u8,
    /// Days the scanner has stayed on it.
    pub days: u32,
}

impl Cache {
    /// The biggest cache a sniffer can grab whole.
    pub const SMALL: u8 = 3;
    /// The smallest cache a crawler can mine.
    pub const LARGE: u8 = 6;

    pub fn small(&self) -> bool {
        self.size <= Self::SMALL
    }

    pub fn large(&self) -> bool {
        self.size >= Self::LARGE
    }

    /// Units in a cache a sniffer grabs, by size.
    pub fn units(&self) -> u32 {
        match self.size {
            1 => 50,
            2 => 100,
            _ => 250,
        }
    }
}

/// Days between a crawler's hauls.
pub const MINING_DAYS: u32 = 5;
/// A sniffer's daily chance of a fragment, one in this.
const FRAGMENT_ODDS: u32 = 40;

fn carries(modules: &[Module], tool: ItemType) -> bool {
    modules
        .iter()
        .any(|m| matches!(m, Module::ToolModule(Some(c)) if c.item == tool))
}

pub(crate) fn run_day(data: &GameData, world: &mut World, events: &mut Vec<Event>) {
    let day = world.day;
    let ids: Vec<VesselId> = world
        .vessels
        .iter()
        .filter(|(_, v)| {
            data.host(v.host).cache_field
                && v.state == VesselState::At(Berth::Lurking)
                && v.pilot.is_some()
                && (carries(&v.modules, ItemType::Crawler)
                    || carries(&v.modules, ItemType::Sniffer))
        })
        .map(|(&id, _)| id)
        .collect();

    for id in ids {
        let vessel = &world.vessels[&id];
        let (owner, host) = (vessel.owner, vessel.host);
        let crawler = carries(&vessel.modules, ItemType::Crawler);
        let sniffer = carries(&vessel.modules, ItemType::Sniffer);
        let resources = &data.host(host).resources;

        // Scan: find a cache, or lose the one being tracked for another.
        let found = match vessel.cache {
            None => world.rng.below(5) == 0,
            Some(cache) => world.rng.below(10 - cache.days.min(9)) == 0,
        };
        let cache = if found && !resources.is_empty() {
            let cache = Cache {
                resource: resources[world.rng.below(resources.len() as u32) as usize],
                size: world.rng.below(8) as u8 + 1,
                days: 0,
            };
            events.push(Event::CacheFound {
                day,
                player: owner,
                vessel: id,
                resource: cache.resource,
                size: cache.size,
            });
            Some(cache)
        } else {
            vessel.cache.map(|c| Cache {
                days: c.days + 1,
                ..c
            })
        };

        // A grabbed cache is gone; a mined one stays and is mined again.
        let (haul, cache) = match cache {
            Some(c) if sniffer && c.small() && found => (Some((c.resource, c.units())), None),
            Some(c) if crawler && c.large() && c.days > 0 && c.days.is_multiple_of(MINING_DAYS) => {
                (Some((c.resource, 16 + world.rng.below(20))), cache)
            }
            _ => (None, cache),
        };
        let fragment = sniffer && world.rng.below(FRAGMENT_ODDS) == 0;

        let vessel = world.vessels.get_mut(&id).expect("listed");
        vessel.cache = cache;
        if let Some((resource, amount)) = haul {
            let container = vessel.modules.iter_mut().find(|m| match m {
                Module::DataContainer(None) => true,
                Module::DataContainer(Some(c)) => {
                    c.item == resource && c.count < Module::CONTAINER_CAPACITY
                }
                _ => false,
            });
            if let Some(Module::DataContainer(held)) = container {
                let count = held.map_or(0, |c| c.count);
                *held = Some(Cargo {
                    item: resource,
                    count: (count + amount).min(Module::CONTAINER_CAPACITY),
                });
            }
        }
        if fragment
            && let Some(slot) = vessel
                .modules
                .iter_mut()
                .find(|m| matches!(m, Module::ToolModule(None)))
        {
            *slot = Module::ToolModule(Some(Cargo {
                item: ItemType::SourceFragment,
                count: 1,
            }));
            events.push(Event::FragmentFound {
                day,
                player: owner,
                vessel: id,
            });
        }
    }
}
