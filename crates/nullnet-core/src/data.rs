//! The map every game is played on, and the fixed catalogs: the four
//! weaknesses, the districts, the upgrades and the prices.
//!
//! The map is a fixed template, so players learn it, but each host's
//! security, ICE and weakness are rolled when a game is created (see
//! [`World::new_game`](crate::World::new_game)).

use std::collections::{BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::ids::HostId;

/// What a host is open to, and what a hacker specialises in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Weakness {
    Web,
    Database,
    Network,
    People,
}

impl Weakness {
    pub const ALL: [Weakness; 4] = [
        Weakness::Web,
        Weakness::Database,
        Weakness::Network,
        Weakness::People,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Weakness::Web => "Web",
            Weakness::Database => "Database",
            Weakness::Network => "Network",
            Weakness::People => "People",
        }
    }

    /// The kit sold for this weakness on the black market.
    pub fn kit(self) -> &'static str {
        match self {
            Weakness::Web => "Web kit",
            Weakness::Database => "Database kit",
            Weakness::Network => "Network kit",
            Weakness::People => "People kit",
        }
    }
}

/// A part of the net with its own character.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum District {
    /// The city net, with a crew hideout in each corner.
    Metro,
    /// Banks and payments: credits.
    Bankwire,
    /// Cloud and data centres: compute.
    Nimbus,
    /// Satellites and ground stations: bandwidth.
    Orbital,
    /// Industrial control systems: a bit of everything.
    Foundry,
    /// University: data.
    Campus,
    /// Government: data.
    Ministry,
    /// Biotech: data.
    Helix,
    /// The AI compute net in the middle, where the Legacy Net was born.
    Lattice,
}

impl District {
    pub fn name(self) -> &'static str {
        match self {
            District::Metro => "Metro",
            District::Bankwire => "Bankwire",
            District::Nimbus => "Nimbus",
            District::Orbital => "Orbital",
            District::Foundry => "Foundry",
            District::Campus => "Campus",
            District::Ministry => "Ministry",
            District::Helix => "Helix",
            District::Lattice => "Lattice",
        }
    }
}

/// A host's place in the template. Hosts with the same role get the same
/// security and ICE in a game, so no corner starts better off than another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Role {
    Hideout,
    /// Next to a hideout, along the top or bottom edge.
    CornerEdge,
    /// Next to a hideout, along the left or right edge.
    CornerSide,
    /// Two links from a hideout, towards the middle.
    CornerBack,
    /// The ends of a side district, next to a corner.
    SideEnd,
    /// The middle of a side district, linked inwards.
    SideMiddle,
    /// The inner ring between the Legacy strongholds.
    Inner,
    /// The Legacy Net's strongholds in the inner ring.
    Stronghold,
    /// The ring of four hosts around Cortex.
    Grid,
    Cortex,
}

impl Role {
    pub const ALL: [Role; 10] = [
        Role::Hideout,
        Role::CornerEdge,
        Role::CornerSide,
        Role::CornerBack,
        Role::SideEnd,
        Role::SideMiddle,
        Role::Inner,
        Role::Stronghold,
        Role::Grid,
        Role::Cortex,
    ];

    /// Security range before the difficulty shift.
    pub fn security(self) -> (u8, u8) {
        match self {
            Role::Hideout | Role::CornerEdge | Role::CornerSide => (1, 2),
            Role::CornerBack | Role::SideEnd => (2, 3),
            Role::SideMiddle | Role::Inner => (3, 4),
            Role::Stronghold | Role::Grid => (4, 5),
            Role::Cortex => (5, 5),
        }
    }

    /// Whether a sealed sub-net can sit behind a host of this role: not on a
    /// hideout or the hosts right next to one (no crew may start beside a
    /// sub-net), and not deep in the Lattice.
    pub fn can_hold_subnet(self) -> bool {
        matches!(
            self,
            Role::CornerBack | Role::SideEnd | Role::SideMiddle | Role::Inner | Role::Stronghold
        )
    }

    pub fn ice(self) -> (u8, u8) {
        match self {
            Role::Hideout | Role::CornerEdge | Role::CornerSide | Role::CornerBack => (0, 1),
            Role::SideEnd | Role::SideMiddle => (1, 2),
            Role::Inner | Role::Stronghold => (2, 3),
            Role::Grid => (3, 4),
            Role::Cortex => (4, 5),
        }
    }
}

/// What a host gives its controller each turn. Bandwidth is capacity for
/// the turn, not a stock.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Yields {
    pub credits: u32,
    pub compute: u32,
    pub bandwidth: u32,
    pub data: u32,
}

impl Yields {
    pub const fn new(credits: u32, compute: u32, bandwidth: u32, data: u32) -> Self {
        Yields {
            credits,
            compute,
            bandwidth,
            data,
        }
    }

    pub fn add(&mut self, other: Yields) {
        self.credits += other.credits;
        self.compute += other.compute;
        self.bandwidth += other.bandwidth;
        self.data += other.data;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HostDef {
    pub name: &'static str,
    pub district: District,
    pub role: Role,
    /// Place on the map, x and y from -100 to 100 with y up.
    pub pos: (i16, i16),
    /// The corner (0 north-west, 1 north-east, 2 south-east, 3 south-west)
    /// if this is a hideout.
    pub hideout: Option<u8>,
    pub yields: Yields,
    /// Held by the Legacy Net at the start.
    pub legacy: bool,
}

/// Something a crew can build at its hideout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Upgrade {
    /// More compute every turn.
    Rigs,
    /// More bandwidth every turn.
    Lines,
    /// Every host the crew holds is harder to take.
    Firewall,
    /// Room for another hacker.
    Safehouse,
}

impl Upgrade {
    pub const ALL: [Upgrade; 4] = [
        Upgrade::Rigs,
        Upgrade::Lines,
        Upgrade::Firewall,
        Upgrade::Safehouse,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Upgrade::Rigs => "Rigs",
            Upgrade::Lines => "Lines",
            Upgrade::Firewall => "Firewall",
            Upgrade::Safehouse => "Safehouse",
        }
    }

    pub fn max_level(self) -> u8 {
        match self {
            Upgrade::Safehouse => 2,
            _ => 3,
        }
    }

    /// Credits and compute for the next level, from `level`.
    pub fn cost(self, level: u8) -> (u32, u32) {
        let level = u32::from(level);
        (30 + 20 * level, 5 + 5 * level)
    }

    /// What one level gives.
    pub fn effect(self) -> &'static str {
        match self {
            Upgrade::Rigs => "+2 compute each turn",
            Upgrade::Lines => "+2 bandwidth each turn",
            Upgrade::Firewall => "+1 defence on every host you hold",
            Upgrade::Safehouse => "room for one more hacker",
        }
    }
}

/// Prices and numbers that are the same in every game.
pub mod rules {
    /// Bandwidth each operation takes.
    pub const SCAN_BANDWIDTH: u32 = 1;
    pub const BREAK_IN_BANDWIDTH: u32 = 2;
    pub const BACKDOOR_BANDWIDTH: u32 = 1;
    pub const STEAL_BANDWIDTH: u32 = 2;
    pub const DEFEND_BANDWIDTH: u32 = 1;

    /// How far a scan reaches from the hosts a crew holds.
    pub const SCAN_LINKS: u32 = 2;
    /// Most compute one break-in can use, one point of attack each.
    pub const MAX_BOOST: u32 = 3;

    pub const KIT_PRICE: u32 = 25;
    pub const KIT_BONUS: u32 = 2;
    pub const ZERO_DAY_PRICE: u32 = 35;
    pub const ZERO_DAY_BONUS: u32 = 4;
    pub const SPECIALTY_BONUS: u32 = 2;
    pub const DEFEND_BONUS: u32 = 2;

    /// Trace an operation leaves.
    pub const TRACE_SUCCESS: u32 = 1;
    pub const TRACE_FAILURE: u32 = 2;
    pub const TRACE_CAUGHT: u32 = 4;
    pub const TRACE_STEAL: u32 = 1;
    /// Trace a crew loses after the Legacy Net has swept it.
    pub const TRACE_AFTER_SWEEP: u32 = 3;

    /// Turns at the start when crews cannot break into each other.
    pub const PEACE_TURNS: u32 = 5;
    pub const DEFAULT_LAST_TURN: u32 = 50;

    pub const START_HACKERS: usize = 2;
    pub const HACKER_SLOTS: usize = 4;
    pub const MARKET_OFFERS: usize = 3;
    pub const MAX_LEVEL: u8 = 5;
    /// Experience needed for levels 2, 3, 4 and 5.
    pub const LEVEL_XP: [u32; 4] = [2, 5, 9, 14];

    /// Sealed sub-nets: how many a game has, what opening one takes and what
    /// an open one pays its host's holder each turn.
    pub const SUBNETS: (u32, u32) = (2, 4);
    pub const SUBNET_BANDWIDTH: u32 = 2;
    pub const SUBNET_CREDITS: u32 = 6;
    pub const TRACE_SUBNET: u32 = 2;

    /// Points at the end of the game.
    pub const POINTS_PER_HOST: u32 = 5;
    pub const POINTS_PER_FREED: u32 = 10;

    /// A hacker's price on the market.
    pub fn hacker_price(level: u8) -> u32 {
        15 * u32::from(level) + 5
    }

    /// A hacker's wage each turn.
    pub fn wage(level: u8) -> u32 {
        u32::from(level)
    }
}

/// The map: hosts and the links between them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GameData {
    pub hosts: Vec<HostDef>,
    pub links: Vec<(HostId, HostId)>,
    neighbours: Vec<Vec<HostId>>,
    /// The hideout in each corner.
    pub hideouts: [HostId; 4],
}

type Template = (&'static str, District, Role, i16, i16, [u32; 4], bool);

const HOSTS: [Template; 41] = {
    use District::*;
    use Role::*;
    [
        // Hideouts, one per corner: credits, compute, bandwidth, data.
        ("Basement", Metro, Hideout, -90, 90, [5, 2, 5, 0], false),
        ("Loft", Metro, Hideout, 90, 90, [5, 2, 5, 0], false),
        ("Arcade", Metro, Hideout, 90, -90, [5, 2, 5, 0], false),
        ("Garage", Metro, Hideout, -90, -90, [5, 2, 5, 0], false),
        // North-west corner.
        ("Beacon", Metro, CornerEdge, -60, 92, [3, 0, 0, 0], false),
        (
            "Switchboard",
            Metro,
            CornerSide,
            -92,
            60,
            [0, 2, 0, 0],
            false,
        ),
        ("Transit", Metro, CornerBack, -62, 62, [0, 0, 2, 1], false),
        // North-east.
        ("Waterworks", Metro, CornerEdge, 60, 92, [3, 0, 0, 0], false),
        ("Powergrid", Metro, CornerSide, 92, 60, [0, 2, 0, 0], false),
        ("Clinic", Metro, CornerBack, 62, 62, [0, 0, 2, 1], false),
        // South-east.
        ("Payroll", Metro, CornerEdge, 60, -92, [3, 0, 0, 0], false),
        (
            "Lighthouse",
            Metro,
            CornerSide,
            92,
            -60,
            [0, 2, 0, 0],
            false,
        ),
        ("Outpost", Metro, CornerBack, 62, -62, [0, 0, 2, 1], false),
        // South-west.
        ("Census", Metro, CornerEdge, -60, -92, [3, 0, 0, 0], false),
        ("Registry", Metro, CornerSide, -92, -60, [0, 2, 0, 0], false),
        ("Courts", Metro, CornerBack, -62, -62, [0, 0, 2, 1], false),
        // North side: Bankwire.
        (
            "Clearinghouse",
            Bankwire,
            SideEnd,
            -30,
            86,
            [4, 0, 0, 0],
            false,
        ),
        ("Vault", Bankwire, SideMiddle, 0, 88, [4, 0, 0, 1], false),
        ("Ledger", Bankwire, SideEnd, 30, 86, [4, 0, 0, 0], false),
        // East side: Nimbus.
        ("Primary", Nimbus, SideEnd, 86, 30, [0, 3, 0, 0], false),
        ("Replica", Nimbus, SideMiddle, 88, 0, [0, 3, 0, 1], false),
        ("Edge", Nimbus, SideEnd, 86, -30, [0, 3, 0, 0], false),
        // South side: Orbital.
        ("Uplink", Orbital, SideEnd, 30, -86, [0, 0, 3, 0], false),
        (
            "Constellation",
            Orbital,
            SideMiddle,
            0,
            -88,
            [1, 0, 2, 1],
            false,
        ),
        ("Relay", Orbital, SideEnd, -30, -86, [0, 0, 3, 0], false),
        // West side: Foundry.
        ("Kiln", Foundry, SideEnd, -86, -30, [2, 1, 0, 0], false),
        ("Refinery", Foundry, SideMiddle, -88, 0, [1, 1, 1, 1], false),
        ("Assembly", Foundry, SideEnd, -86, 30, [2, 1, 0, 0], false),
        // Inner ring: strongholds on the axes, free hosts between them.
        (
            "Intelligence",
            Ministry,
            Stronghold,
            0,
            48,
            [0, 1, 0, 4],
            true,
        ),
        ("Cabinet", Ministry, Inner, 34, 34, [1, 0, 0, 3], false),
        ("Sequencer", Helix, Stronghold, 48, 0, [0, 1, 0, 4], true),
        ("Genebank", Helix, Inner, 34, -34, [1, 0, 0, 3], false),
        (
            "Observatory",
            Campus,
            Stronghold,
            0,
            -48,
            [0, 1, 0, 4],
            true,
        ),
        ("Library", Campus, Inner, -34, -34, [1, 0, 0, 3], false),
        ("Wiretap", Ministry, Stronghold, -48, 0, [0, 1, 0, 4], true),
        ("Registrar", Campus, Inner, -34, 34, [1, 0, 0, 3], false),
        // Lattice.
        ("Tensor", Lattice, Grid, 0, 22, [0, 0, 0, 5], true),
        ("Oracle", Lattice, Grid, 22, 0, [0, 0, 0, 5], true),
        ("Trainer", Lattice, Grid, 0, -22, [0, 0, 0, 5], true),
        ("Hive", Lattice, Grid, -22, 0, [0, 0, 0, 5], true),
        ("Cortex", Lattice, Cortex, 0, 0, [0, 0, 0, 8], true),
    ]
};

const LINKS: [(&str, &str); 60] = [
    // Corners: the hideout reaches two hosts, both reach the third.
    ("Basement", "Beacon"),
    ("Basement", "Switchboard"),
    ("Beacon", "Transit"),
    ("Switchboard", "Transit"),
    ("Loft", "Waterworks"),
    ("Loft", "Powergrid"),
    ("Waterworks", "Clinic"),
    ("Powergrid", "Clinic"),
    ("Arcade", "Payroll"),
    ("Arcade", "Lighthouse"),
    ("Payroll", "Outpost"),
    ("Lighthouse", "Outpost"),
    ("Garage", "Census"),
    ("Garage", "Registry"),
    ("Census", "Courts"),
    ("Registry", "Courts"),
    // Sides run from corner to corner.
    ("Beacon", "Clearinghouse"),
    ("Clearinghouse", "Vault"),
    ("Vault", "Ledger"),
    ("Ledger", "Waterworks"),
    ("Powergrid", "Primary"),
    ("Primary", "Replica"),
    ("Replica", "Edge"),
    ("Edge", "Lighthouse"),
    ("Payroll", "Uplink"),
    ("Uplink", "Constellation"),
    ("Constellation", "Relay"),
    ("Relay", "Census"),
    ("Registry", "Kiln"),
    ("Kiln", "Refinery"),
    ("Refinery", "Assembly"),
    ("Assembly", "Switchboard"),
    // The middle of each side leads to a stronghold.
    ("Vault", "Intelligence"),
    ("Replica", "Sequencer"),
    ("Constellation", "Observatory"),
    ("Refinery", "Wiretap"),
    // The inner ring.
    ("Intelligence", "Cabinet"),
    ("Cabinet", "Sequencer"),
    ("Sequencer", "Genebank"),
    ("Genebank", "Observatory"),
    ("Observatory", "Library"),
    ("Library", "Wiretap"),
    ("Wiretap", "Registrar"),
    ("Registrar", "Intelligence"),
    // Each corner leads to the inner ring.
    ("Transit", "Registrar"),
    ("Clinic", "Cabinet"),
    ("Outpost", "Genebank"),
    ("Courts", "Library"),
    // Lattice: a ring around Cortex, reached from the strongholds.
    ("Intelligence", "Tensor"),
    ("Sequencer", "Oracle"),
    ("Observatory", "Trainer"),
    ("Wiretap", "Hive"),
    ("Tensor", "Oracle"),
    ("Oracle", "Trainer"),
    ("Trainer", "Hive"),
    ("Hive", "Tensor"),
    ("Cortex", "Tensor"),
    ("Cortex", "Oracle"),
    ("Cortex", "Trainer"),
    ("Cortex", "Hive"),
];

impl GameData {
    /// The map every game is played on.
    pub fn standard() -> Self {
        let mut hideouts = [HostId(0); 4];
        let mut corner = 0u8;
        let hosts: Vec<HostDef> = HOSTS
            .iter()
            .enumerate()
            .map(|(index, &(name, district, role, x, y, yields, legacy))| {
                let hideout = (role == Role::Hideout).then(|| {
                    hideouts[usize::from(corner)] = HostId(index as u16);
                    corner += 1;
                    corner - 1
                });
                HostDef {
                    name,
                    district,
                    role,
                    pos: (x, y),
                    hideout,
                    yields: Yields::new(yields[0], yields[1], yields[2], yields[3]),
                    legacy,
                }
            })
            .collect();
        let find = |name: &str| {
            let index = hosts
                .iter()
                .position(|h| h.name == name)
                .unwrap_or_else(|| panic!("link to unknown host {name}"));
            HostId(index as u16)
        };
        let links: Vec<(HostId, HostId)> = LINKS.iter().map(|&(a, b)| (find(a), find(b))).collect();
        let mut neighbours = vec![Vec::new(); hosts.len()];
        for &(a, b) in &links {
            neighbours[a.index()].push(b);
            neighbours[b.index()].push(a);
        }
        for list in &mut neighbours {
            list.sort();
        }
        GameData {
            hosts,
            links,
            neighbours,
            hideouts,
        }
    }

    pub fn host(&self, id: HostId) -> &HostDef {
        &self.hosts[id.index()]
    }

    pub fn host_ids(&self) -> impl Iterator<Item = HostId> + '_ {
        (0..self.hosts.len()).map(|i| HostId(i as u16))
    }

    pub fn find(&self, name: &str) -> Option<HostId> {
        self.hosts
            .iter()
            .position(|h| h.name.eq_ignore_ascii_case(name))
            .map(|i| HostId(i as u16))
    }

    pub fn neighbours(&self, id: HostId) -> &[HostId] {
        &self.neighbours[id.index()]
    }

    pub fn linked(&self, a: HostId, b: HostId) -> bool {
        self.neighbours(a).contains(&b)
    }

    /// Every host at most `links` links from one of `from`, `from` included.
    pub fn within(&self, from: impl IntoIterator<Item = HostId>, links: u32) -> BTreeSet<HostId> {
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::new();
        for host in from {
            if seen.insert(host) {
                queue.push_back((host, 0));
            }
        }
        while let Some((host, distance)) = queue.pop_front() {
            if distance == links {
                continue;
            }
            for &next in self.neighbours(host) {
                if seen.insert(next) {
                    queue.push_back((next, distance + 1));
                }
            }
        }
        seen
    }

    /// The corner each crew starts in, by its place in the game: two crews
    /// start in opposite corners.
    pub fn start_corner(place: usize) -> usize {
        [0, 2, 1, 3][place % 4]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_is_whole() {
        let data = GameData::standard();
        assert_eq!(data.hosts.len(), 41);
        let names: BTreeSet<_> = data.hosts.iter().map(|h| h.name).collect();
        assert_eq!(names.len(), data.hosts.len(), "host names are unique");
        let links: BTreeSet<_> = data
            .links
            .iter()
            .map(|&(a, b)| (a.min(b), a.max(b)))
            .collect();
        assert_eq!(links.len(), data.links.len(), "no link twice");
        assert!(data.links.iter().all(|(a, b)| a != b));
        let everything = data.within([data.hideouts[0]], 100);
        assert_eq!(everything.len(), data.hosts.len(), "the map is connected");
    }

    #[test]
    fn corners_mirror_each_other() {
        let data = GameData::standard();
        for (corner, &hideout) in data.hideouts.iter().enumerate() {
            let def = data.host(hideout);
            assert_eq!(def.hideout, Some(corner as u8));
            assert_eq!(data.neighbours(hideout).len(), 2);
            let roles: Vec<Role> = data
                .neighbours(hideout)
                .iter()
                .map(|&h| data.host(h).role)
                .collect();
            assert!(roles.contains(&Role::CornerEdge) && roles.contains(&Role::CornerSide));
            // Every hideout is the same number of links from Cortex.
            let cortex = data.find("Cortex").unwrap();
            let reach = |n| data.within([hideout], n).contains(&cortex);
            assert!(!reach(5) && reach(6));
        }
    }

    #[test]
    fn scans_reach_two_links() {
        let data = GameData::standard();
        let basement = data.find("Basement").unwrap();
        let near: Vec<&str> = data
            .within([basement], 2)
            .into_iter()
            .map(|h| data.host(h).name)
            .collect();
        assert_eq!(
            near,
            [
                "Basement",
                "Beacon",
                "Switchboard",
                "Transit",
                "Clearinghouse",
                "Assembly"
            ]
        );
    }

    #[test]
    fn two_crews_start_in_opposite_corners() {
        assert_eq!(GameData::start_corner(0), 0);
        assert_eq!(GameData::start_corner(1), 2);
    }
}
