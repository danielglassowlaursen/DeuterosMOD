//! Rules data that never changes during a game.
//!
//! The classic tables are the numbers of Deuteros, extracted from the Godot
//! remake by `tools/extract_coredata.py` into `data/classic.json` and embedded
//! in the crate, so every server and client resolves turns from the same
//! data.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::{HostId, NetworkId};
use crate::items::ItemType;
use crate::research::ResearchDef;
use crate::staff::StaffKind;

const CLASSIC_JSON: &str = include_str!("../data/classic.json");

/// The item, research, network and host tables. Kept apart from
/// [`World`](crate::World) so saves hold only what changes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameData {
    pub items: BTreeMap<ItemType, ItemDef>,
    pub research: BTreeMap<ItemType, ResearchDef>,
    /// Indexed by [`NetworkId`].
    pub networks: Vec<NetworkDef>,
    /// Indexed by [`HostId`].
    pub hosts: Vec<HostDef>,
    /// Scales how long a survey takes and how rich the next vein is.
    pub survey_multiplier: BTreeMap<ItemType, u32>,
    /// Units one tap extracts per day.
    pub tap_rate: BTreeMap<ItemType, u32>,
    pub hideout: HideoutDef,
    pub recruitment: RecruitmentDef,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemCategory {
    Resource,
    Item,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDef {
    pub category: ItemCategory,
    pub mass: u32,
    /// Can only be built in a citadel's workshop, not in the hideout.
    pub orbit_only: bool,
    /// Carried in a tool module rather than a data container.
    pub tool_module: bool,
    /// A tool module takes one unit instead of the whole stock.
    pub tool_module_single: bool,
    /// Refined automatically by every workshop (the two transit fuels).
    pub auto_produce: bool,
    pub recipe: Vec<(ItemType, u32)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkDef {
    /// The star this network was in Deuteros.
    pub classic: String,
    /// How many hosts the Legacy Net holds here at random from the start;
    /// `None` for the home network, whose Legacy hosts are fixed.
    pub random_legacy_hosts: Option<u32>,
    /// Strength the network's Legacy fleet must reach before it attacks.
    pub legacy_attack_trigger: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostDef {
    /// The planet or moon this host was in Deuteros.
    pub classic: String,
    pub network: NetworkId,
    /// Position in the network, which sets travel time.
    pub order: u32,
    /// The host a subsystem belongs to; `None` for a top-level host.
    pub parent: Option<HostId>,
    /// Resources that can be tapped here.
    pub resources: Vec<ItemType>,
    /// A segment site from the original, not yet used by any rule.
    pub segment: bool,
    /// Held by the Legacy Net from the start.
    pub legacy: bool,
    /// A field of abandoned data caches (the asteroid belt in Deuteros):
    /// vessels can visit, but no one can build a citadel or backdoor there.
    #[serde(default)]
    pub cache_field: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HideoutDef {
    /// The host every crew's hideout sits on.
    pub host: HostId,
    /// Taps each hideout starts with.
    pub taps: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecruitmentDef {
    pub recruits_available: u32,
    pub courses: BTreeMap<StaffKind, CourseDef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CourseDef {
    pub days: u32,
    /// Most recruits in one course.
    pub batch_max: u32,
    /// Most staff of this kind a crew can have in one team, if limited.
    pub team_max: Option<u32>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DataError(String);

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid game data: {}", self.0)
    }
}

impl std::error::Error for DataError {}

impl GameData {
    /// The Deuteros tables embedded in the crate.
    pub fn classic() -> GameData {
        GameData::from_json(CLASSIC_JSON).expect("embedded classic data is valid")
    }

    /// Parses and validates a data file in the format the extractor writes.
    pub fn from_json(json: &str) -> Result<GameData, DataError> {
        let file: DataFile =
            serde_json::from_str(json).map_err(|e| DataError(format!("JSON: {e}")))?;
        let data = file.into_game_data();
        data.validate()?;
        Ok(data)
    }

    pub fn host(&self, id: HostId) -> &HostDef {
        &self.hosts[usize::from(id.0)]
    }

    pub fn network(&self, id: NetworkId) -> &NetworkDef {
        &self.networks[usize::from(id.0)]
    }

    fn validate(&self) -> Result<(), DataError> {
        let fail = |message: String| Err(DataError(message));
        for (index, host) in self.hosts.iter().enumerate() {
            if usize::from(host.network.0) >= self.networks.len() {
                return fail(format!(
                    "host {index} is in unknown network {}",
                    host.network.0
                ));
            }
            if let Some(parent) = host.parent {
                let Some(parent_def) = self.hosts.get(usize::from(parent.0)) else {
                    return fail(format!("host {index} has unknown parent {}", parent.0));
                };
                if parent_def.parent.is_some() || parent_def.network != host.network {
                    return fail(format!(
                        "host {index} must hang under a top-level host in its network"
                    ));
                }
            }
            for resource in &host.resources {
                if !self.survey_multiplier.contains_key(resource)
                    || !self.tap_rate.contains_key(resource)
                {
                    return fail(format!(
                        "host {index} has {resource:?}, which has no survey or tap rate"
                    ));
                }
            }
        }
        if usize::from(self.hideout.host.0) >= self.hosts.len() {
            return fail("the hideout host does not exist".into());
        }
        for (item, def) in &self.items {
            if let Some((input, _)) = def
                .recipe
                .iter()
                .find(|(input, _)| !self.items.contains_key(input))
            {
                return fail(format!(
                    "{item:?} needs {input:?}, which is not in the item table"
                ));
            }
        }
        Ok(())
    }
}

/// The on-disk format written by `tools/extract_coredata.py`.
#[derive(Deserialize)]
struct DataFile {
    items: Vec<ItemEntry>,
    networks: Vec<NetworkDef>,
    hosts: Vec<HostDef>,
    survey_multiplier: BTreeMap<ItemType, u32>,
    tap_rate: BTreeMap<ItemType, u32>,
    hideout: HideoutDef,
    recruitment: RecruitmentDef,
}

#[derive(Deserialize)]
struct ItemEntry {
    item: ItemType,
    #[serde(flatten)]
    def: ItemDef,
    research: Option<ResearchDef>,
}

impl DataFile {
    fn into_game_data(self) -> GameData {
        let mut items = BTreeMap::new();
        let mut research = BTreeMap::new();
        for entry in self.items {
            items.insert(entry.item, entry.def);
            if let Some(def) = entry.research {
                research.insert(entry.item, def);
            }
        }
        GameData {
            items,
            research,
            networks: self.networks,
            hosts: self.hosts,
            survey_multiplier: self.survey_multiplier,
            tap_rate: self.tap_rate,
            hideout: self.hideout,
            recruitment: self.recruitment,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_tables_have_the_original_shape() {
        let data = GameData::classic();
        assert_eq!(data.items.len(), 46);
        assert_eq!(data.networks.len(), 9);
        assert_eq!(data.hosts.len(), 160);
        let top_level = data.hosts.iter().filter(|h| h.parent.is_none()).count();
        assert_eq!((top_level, data.hosts.len() - top_level), (44, 116));
    }

    #[test]
    fn classic_values_match_core_data() {
        let data = GameData::classic();
        // derrick: 3 iron, 4 titanium, 1 carbon (CoreData.cs).
        assert_eq!(
            data.items[&ItemType::Tap].recipe,
            [
                (ItemType::Compute, 3),
                (ItemType::Storage, 4),
                (ItemType::Code, 1)
            ]
        );
        // S drive overrides the research defaults.
        let drive = &data.research[&ItemType::DropperEngine];
        assert_eq!((drive.multiplier, drive.initial_value), (96, 32));
        assert_eq!(data.research[&ItemType::WormCore].tech_level, 2);
        assert_eq!(data.survey_multiplier[&ItemType::Keys], 4);
        assert_eq!(data.tap_rate[&ItemType::Compute], 2);

        let home = data.host(data.hideout.host);
        assert_eq!(home.classic, "earth");
        assert_eq!(data.hideout.taps, 1);

        let legacy: Vec<&str> = data
            .hosts
            .iter()
            .filter(|h| h.legacy)
            .map(|h| h.classic.as_str())
            .collect();
        assert_eq!(
            legacy,
            ["jupiter", "uranus", "titania", "neptune", "triton", "pluto"]
        );
    }

    #[test]
    fn research_open_at_start_is_the_tech_one_starter_set() {
        let data = GameData::classic();
        let open: Vec<ItemType> = data
            .research
            .iter()
            .filter(|(_, def)| def.available_at_start)
            .map(|(&item, _)| item)
            .collect();
        assert_eq!(
            open,
            [
                ItemType::ProxyChains,
                ItemType::Tap,
                ItemType::DropperCore,
                ItemType::DropperEngine,
                ItemType::CitadelModule,
                ItemType::DataContainer,
                ItemType::ToolModule,
                ItemType::SessionPod,
            ]
        );
    }

    #[test]
    fn subsystems_hang_under_hosts_in_their_own_network() {
        let data = GameData::classic();
        for host in data.hosts.iter().filter(|h| h.parent.is_some()) {
            let parent = data.host(host.parent.unwrap());
            assert_eq!(parent.network, host.network, "{}", host.classic);
        }
    }

    #[test]
    fn invalid_data_is_rejected() {
        let mut file: serde_json::Value = serde_json::from_str(CLASSIC_JSON).unwrap();
        file["hosts"][0]["parent"] = serde_json::json!(999);
        let error = GameData::from_json(&file.to_string()).unwrap_err();
        assert!(error.to_string().contains("unknown parent 999"), "{error}");
    }
}
