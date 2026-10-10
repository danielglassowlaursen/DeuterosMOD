//! Random maps: a new net for every game, made from its seed.
//!
//! A random map is built from one corner's share of hosts, turned a quarter
//! at a time round Cortex in the middle, so all four corners are alike and
//! no crew starts better off than another. Within that, everything is new
//! each game: where the hosts sit, which of them link up, and how much of a
//! web the net is. Some maps keep few links and grow long branches with dead
//! ends; others keep most of them and become a dense web.
//!
//! The rules of the standard map hold here too: the Legacy Net holds the
//! middle (Cortex, a ring of grid hosts and its strongholds), the grid is
//! reached only through a stronghold, a hideout links to two free hosts, and
//! the crews' corners always link up through free hosts, not only through
//! the Legacy Net. Hosts get their role from how many links they are from a
//! hideout, and with it their security, ICE and yields.
//!
//! Everything is integer arithmetic on the world's own random numbers, so
//! the server and a browser build the same map from the same seed.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::data::{District, GameData, HostDef, Role, Yields};
use crate::ids::HostId;
use crate::rng::Rng;

/// The map a game is played on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MapSpec {
    /// The standard map, the same in every game.
    #[default]
    Standard,
    /// A map made for the game from a seed, with this many hosts.
    Random { seed: u64, hosts: u32 },
}

/// The fewest and most hosts a random map is asked for.
pub const MIN_HOSTS: u32 = 20;
pub const MAX_HOSTS: u32 = 100;

impl MapSpec {
    /// A random map with about `hosts` hosts: the nearest count that four
    /// equal corners and Cortex make, from 21 to 101.
    pub fn random(seed: u64, hosts: u32) -> Self {
        MapSpec::Random {
            seed,
            hosts: 4 * share(hosts) as u32 + 1,
        }
    }

    pub fn data(self) -> GameData {
        match self {
            MapSpec::Standard => GameData::standard(),
            MapSpec::Random { seed, hosts } => generate(seed, hosts),
        }
    }

    pub fn is_random(self) -> bool {
        matches!(self, MapSpec::Random { .. })
    }
}

/// Hosts in one corner's share for a map of about `hosts` hosts.
fn share(hosts: u32) -> usize {
    ((hosts.clamp(MIN_HOSTS, MAX_HOSTS) + 1) / 4) as usize
}

// ------------------------------------------------------------ names

const HIDEOUT_NAMES: [&str; 12] = [
    "Basement",
    "Loft",
    "Arcade",
    "Garage",
    "Attic",
    "Bunker",
    "Squat",
    "Cellar",
    "Backroom",
    "Boathouse",
    "Darkroom",
    "Crawlspace",
];

const METRO_NAMES: [&str; 44] = [
    "Beacon",
    "Switchboard",
    "Transit",
    "Waterworks",
    "Powergrid",
    "Clinic",
    "Payroll",
    "Lighthouse",
    "Outpost",
    "Census",
    "Registry",
    "Courts",
    "Subway",
    "Tollgate",
    "Depot",
    "Harbor",
    "Stadium",
    "Precinct",
    "Firehouse",
    "Hotel",
    "Pawnshop",
    "Laundromat",
    "Cinema",
    "Carpark",
    "Junction",
    "Tramway",
    "Sewer",
    "Postal",
    "Taxi",
    "Mall",
    "Pier",
    "Dockyard",
    "Hospital",
    "Pharmacy",
    "Ferry",
    "Bridge",
    "Tunnel",
    "Kiosk",
    "Diner",
    "Bodega",
    "Motel",
    "Arena",
    "Museum",
    "Marina",
];

const BANKWIRE_NAMES: [&str; 26] = [
    "Clearinghouse",
    "Vault",
    "Ledger",
    "Treasury",
    "Mint",
    "Exchange",
    "Escrow",
    "Bourse",
    "Teller",
    "Payments",
    "Brokerage",
    "Custody",
    "Settlement",
    "Forex",
    "Futures",
    "Margin",
    "Dividend",
    "Audit",
    "Trustee",
    "Pension",
    "Lottery",
    "Casino",
    "Remittance",
    "Bullion",
    "Coffer",
    "Underwriter",
];

const NIMBUS_NAMES: [&str; 25] = [
    "Primary",
    "Replica",
    "Edge",
    "Shard",
    "Cluster",
    "Rack",
    "Blade",
    "Hypervisor",
    "Container",
    "Kernel",
    "Cache",
    "Mirror",
    "Failover",
    "Scheduler",
    "Balancer",
    "Coldstore",
    "Snapshot",
    "Lambda",
    "Compiler",
    "Sandbox",
    "Instance",
    "Partition",
    "Mainframe",
    "Swapfile",
    "Bytecode",
];

const ORBITAL_NAMES: [&str; 25] = [
    "Uplink",
    "Constellation",
    "Relay",
    "Groundstation",
    "Transponder",
    "Dish",
    "Beam",
    "Apogee",
    "Perigee",
    "Telemetry",
    "Downlink",
    "Antenna",
    "Booster",
    "Gantry",
    "Radome",
    "Carrier",
    "Frequency",
    "Sextant",
    "Zenith",
    "Nadir",
    "Horizon",
    "Polaris",
    "Backbone",
    "Skyhook",
    "Launchpad",
];

const FOUNDRY_NAMES: [&str; 25] = [
    "Kiln", "Refinery", "Assembly", "Smelter", "Turbine", "Furnace", "Crane", "Conveyor", "Forge",
    "Mill", "Boiler", "Pump", "Valve", "Reactor", "Press", "Lathe", "Pipeline", "Silo", "Quarry",
    "Tannery", "Dynamo", "Gearbox", "Welder", "Anvil", "Crucible",
];

const MINISTRY_NAMES: [&str; 16] = [
    "Intelligence",
    "Cabinet",
    "Wiretap",
    "Archive",
    "Embassy",
    "Consulate",
    "Senate",
    "Bureau",
    "Customs",
    "Passport",
    "Treaty",
    "Chancery",
    "Directorate",
    "Signals",
    "Tribunal",
    "Parliament",
];

const HELIX_NAMES: [&str; 16] = [
    "Sequencer",
    "Genebank",
    "Clone",
    "Vaccine",
    "Ribosome",
    "Enzyme",
    "Culture",
    "Petri",
    "Incubator",
    "Splicer",
    "Biobank",
    "Plasmid",
    "Marker",
    "Mutagen",
    "Serum",
    "Strain",
];

const CAMPUS_NAMES: [&str; 16] = [
    "Observatory",
    "Library",
    "Registrar",
    "Lecture",
    "Thesis",
    "Lab",
    "Faculty",
    "Dean",
    "Seminar",
    "Dormitory",
    "Quad",
    "Bursar",
    "Syllabus",
    "Alumni",
    "Planetarium",
    "Greenhouse",
];

const LATTICE_NAMES: [&str; 4] = ["Tensor", "Oracle", "Trainer", "Hive"];

/// The names a district's hosts are drawn from.
fn names(district: District) -> &'static [&'static str] {
    match district {
        District::Metro => &METRO_NAMES,
        District::Bankwire => &BANKWIRE_NAMES,
        District::Nimbus => &NIMBUS_NAMES,
        District::Orbital => &ORBITAL_NAMES,
        District::Foundry => &FOUNDRY_NAMES,
        District::Ministry => &MINISTRY_NAMES,
        District::Helix => &HELIX_NAMES,
        District::Campus => &CAMPUS_NAMES,
        District::Lattice => &LATTICE_NAMES,
    }
}

const SIDE_DISTRICTS: [District; 4] = [
    District::Bankwire,
    District::Nimbus,
    District::Orbital,
    District::Foundry,
];
const INNER_DISTRICTS: [District; 3] = [District::Campus, District::Ministry, District::Helix];

/// What a host of this district and role yields, in line with the standard
/// map.
fn yields(district: District, role: Role) -> Yields {
    let [credits, compute, bandwidth, data] = match role {
        Role::Hideout => [5, 2, 5, 0],
        Role::CornerEdge => [3, 0, 0, 0],
        Role::CornerSide => [0, 2, 0, 0],
        Role::CornerBack => [0, 0, 2, 1],
        Role::SideEnd | Role::SideMiddle => {
            let data = u32::from(role == Role::SideMiddle);
            match district {
                District::Bankwire => [4, 0, 0, data],
                District::Nimbus => [0, 3, 0, data],
                District::Orbital => [0, 0, 3, data],
                District::Foundry => [2 - data, 1, data, data],
                District::Metro => [2, 0, 1, data],
                _ => [1, 0, 0, 2 + data],
            }
        }
        Role::Inner => [1, 0, 0, 3],
        Role::Stronghold => [0, 1, 0, 4],
        Role::Grid => [0, 0, 0, 5],
        Role::Cortex => [0, 0, 0, 8],
    };
    Yields::new(credits, compute, bandwidth, data)
}

// ------------------------------------------------------------ geometry

/// A place on the map, x and y from -100 to 100 with y up.
type Point = (i32, i32);

/// A point turned a quarter clockwise round the middle `quarters` times:
/// the north-west corner to the north-east, then south-east, south-west.
fn turn(point: Point, quarters: usize) -> Point {
    let mut p = point;
    for _ in 0..quarters % 4 {
        p = (p.1, -p.0);
    }
    p
}

fn distance2(a: Point, b: Point) -> i64 {
    let dx = i64::from(a.0 - b.0);
    let dy = i64::from(a.1 - b.1);
    dx * dx + dy * dy
}

fn isqrt(n: i64) -> i64 {
    let mut root = 0i64;
    while (root + 1) * (root + 1) <= n {
        root += 1;
    }
    root
}

/// Which side of the line from `a` to `b` the point `c` is on: positive,
/// negative, or 0 on the line.
fn side(a: Point, b: Point, c: Point) -> i64 {
    let (abx, aby) = (i64::from(b.0 - a.0), i64::from(b.1 - a.1));
    let (acx, acy) = (i64::from(c.0 - a.0), i64::from(c.1 - a.1));
    (abx * acy - aby * acx).signum()
}

/// Whether two segments cross at a point inside both.
fn cross(a: Point, b: Point, c: Point, d: Point) -> bool {
    side(a, b, c) * side(a, b, d) < 0 && side(c, d, a) * side(c, d, b) < 0
}

/// Sixteen directions round the compass, as hundredths.
const DIRECTIONS: [Point; 16] = [
    (100, 0),
    (92, 38),
    (71, 71),
    (38, 92),
    (0, 100),
    (-38, 92),
    (-71, 71),
    (-92, 38),
    (-100, 0),
    (-92, -38),
    (-71, -71),
    (-38, -92),
    (0, -100),
    (38, -92),
    (71, -71),
    (92, -38),
];

// ------------------------------------------------------------ the net

/// A link that would join up two parts of the net, by preference: whether
/// it crosses a link, is too long, ends a branch; then its length and key.
type Bridge = (bool, bool, bool, i64, (usize, usize));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Hideout,
    Grid,
    Stronghold,
    Free,
    Cortex,
}

impl Kind {
    fn legacy(self) -> bool {
        matches!(self, Kind::Grid | Kind::Stronghold | Kind::Cortex)
    }
}

/// The net as it is built. Host `4 * i + q` is host `i` of the north-west
/// share turned `q` quarters; Cortex comes last.
struct Net {
    share: Vec<(Point, Kind)>,
    links: BTreeSet<(usize, usize)>,
    /// Which hosts are joined up so far, for spanning the net.
    parent: Vec<usize>,
}

impl Net {
    fn new(share: Vec<(Point, Kind)>) -> Self {
        let len = 4 * share.len() + 1;
        Net {
            share,
            links: BTreeSet::new(),
            parent: (0..len).collect(),
        }
    }

    fn len(&self) -> usize {
        4 * self.share.len() + 1
    }

    fn cortex(&self) -> usize {
        4 * self.share.len()
    }

    fn pos(&self, host: usize) -> Point {
        if host == self.cortex() {
            (0, 0)
        } else {
            turn(self.share[host / 4].0, host % 4)
        }
    }

    fn kind(&self, host: usize) -> Kind {
        if host == self.cortex() {
            Kind::Cortex
        } else {
            self.share[host / 4].1
        }
    }

    fn turned(&self, host: usize, quarters: usize) -> usize {
        if host == self.cortex() {
            host
        } else {
            host - host % 4 + (host % 4 + quarters) % 4
        }
    }

    /// A link and its three turned copies, each as (low, high).
    fn orbit(&self, a: usize, b: usize) -> [(usize, usize); 4] {
        std::array::from_fn(|q| {
            let (x, y) = (self.turned(a, q), self.turned(b, q));
            (x.min(y), x.max(y))
        })
    }

    /// The copy that stands for a link's orbit: the lowest.
    fn key(&self, a: usize, b: usize) -> (usize, usize) {
        self.orbit(a, b).into_iter().min().unwrap_or((a, b))
    }

    fn linked(&self, a: usize, b: usize) -> bool {
        self.links.contains(&(a.min(b), a.max(b)))
    }

    fn degree(&self, host: usize) -> usize {
        self.links
            .iter()
            .filter(|&&(a, b)| a == host || b == host)
            .count()
    }

    /// Whether a link may run here: a hideout links only to free hosts,
    /// Cortex only to the grid, and the grid only to Cortex, itself and the
    /// strongholds. A link from a host to its own copy across the middle is
    /// left out, as it would have only two copies.
    fn allowed(&self, a: usize, b: usize) -> bool {
        if a == b || (self.turned(a, 2) == b && self.turned(b, 2) == a) {
            return false;
        }
        match (self.kind(a), self.kind(b)) {
            (Kind::Hideout, other) | (other, Kind::Hideout) => other == Kind::Free,
            (Kind::Cortex, other) | (other, Kind::Cortex) => other == Kind::Grid,
            (Kind::Grid, other) | (other, Kind::Grid) => {
                matches!(other, Kind::Grid | Kind::Stronghold)
            }
            _ => true,
        }
    }

    fn find(&mut self, host: usize) -> usize {
        let mut root = host;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        let mut at = host;
        while self.parent[at] != root {
            let next = self.parent[at];
            self.parent[at] = root;
            at = next;
        }
        root
    }

    /// Links `a` and `b` and the turned copies of that link.
    fn link(&mut self, a: usize, b: usize) {
        for (x, y) in self.orbit(a, b) {
            self.links.insert((x, y));
            let (rx, ry) = (self.find(x), self.find(y));
            if rx != ry {
                self.parent[rx.max(ry)] = rx.min(ry);
            }
        }
    }

    fn joined(&mut self, a: usize, b: usize) -> bool {
        self.find(a) == self.find(b)
    }

    /// Whether a link from `a` to `b` would cross a link already drawn
    /// (sharing an end is not crossing).
    fn crosses(&self, a: usize, b: usize) -> bool {
        let (p, q) = (self.pos(a), self.pos(b));
        self.links.iter().any(|&(x, y)| {
            x != a && x != b && y != a && y != b && cross(p, q, self.pos(x), self.pos(y))
        })
    }

    /// The end of a branch: a free host with one link.
    fn tip(&self, host: usize) -> bool {
        self.kind(host) == Kind::Free && self.degree(host) == 1
    }

    /// Links the shortest allowed pair, among the hosts `keep` accepts, that
    /// is not joined up yet: one that crosses no link if there is one, then
    /// one no longer than `reach`, then one that leaves the ends of branches
    /// be. False if there is no pair at all.
    fn bridge(&mut self, reach: i64, keep: impl Fn(Kind) -> bool) -> bool {
        let mut best: Option<Bridge> = None;
        for a in 0..self.len() {
            for b in a + 1..self.len() {
                if !keep(self.kind(a)) || !keep(self.kind(b)) || !self.allowed(a, b) {
                    continue;
                }
                if self.joined(a, b) {
                    continue;
                }
                let length = distance2(self.pos(a), self.pos(b));
                let candidate = (
                    self.crosses(a, b),
                    length > reach * reach,
                    self.tip(a) || self.tip(b),
                    length,
                    self.key(a, b),
                );
                if best.is_none_or(|b| candidate < b) {
                    best = Some(candidate);
                }
            }
        }
        match best {
            Some((_, _, _, _, (a, b))) => {
                self.link(a, b);
                true
            }
            None => false,
        }
    }

    /// Links from every host's neighbours, in host order.
    fn neighbours(&self) -> Vec<Vec<usize>> {
        let mut list = vec![Vec::new(); self.len()];
        for &(a, b) in &self.links {
            list[a].push(b);
            list[b].push(a);
        }
        list
    }
}

// ------------------------------------------------------------ the generator

fn generate(seed: u64, hosts: u32) -> GameData {
    let mut rng = Rng::new(seed, 0x006d_6170_6765_6e00);
    let share_len = share(hosts);
    let strongholds = if share_len >= 16 { 2 } else { 1 };
    // How many of the extra links the map keeps, in percent: few leave the
    // branches and dead ends, many make a web.
    let style = rng.below(101);

    // The north-west share: the hideout in the corner, a grid host next to
    // Cortex and the strongholds halfway in.
    let jitter = |rng: &mut Rng| rng.range_inclusive(0, 10) as i32 - 5;
    let mut share: Vec<(Point, Kind)> = vec![((-90, 90), Kind::Hideout), ((-16, 16), Kind::Grid)];
    let spots: &[Point] = if strongholds == 1 {
        &[(-32, 32)]
    } else {
        &[(-17, 42), (-42, 17)]
    };
    for &(x, y) in spots {
        let at = (x + jitter(&mut rng), y + jitter(&mut rng));
        share.push((at, Kind::Stronghold));
    }

    // The free hosts grow as branches from the hideout: each new one a
    // little way off a host already placed, at least `spacing` from every
    // host and its turned copies, its link crossing no other. Closer only if
    // the share gets crowded.
    let free = share_len - share.len();
    let room: i64 = 91 * 91 - 700;
    let nominal = isqrt(room / (free + strongholds + 1) as i64) * 4 / 5;
    let mut spacing = nominal;
    let mut branches: Vec<(usize, usize)> = Vec::new();
    let mut children = vec![0usize; share_len];
    let copies = |branches: &[(usize, usize)], share: &[(Point, Kind)]| -> Vec<(Point, Point)> {
        branches
            .iter()
            .flat_map(|&(a, b)| (0..4).map(move |q| (a, b, q)))
            .map(|(a, b, q)| (turn(share[a].0, q), turn(share[b].0, q)))
            .collect()
    };
    while share.len() < share_len {
        let drawn = copies(&branches, &share);
        let mut placed = false;
        for _ in 0..300 {
            let tree: Vec<usize> = (0..share.len())
                .filter(|&i| match share[i].1 {
                    Kind::Hideout => children[i] < 2,
                    Kind::Free => children[i] < 3,
                    _ => false,
                })
                .collect();
            let from = tree[rng.below(tree.len() as u32) as usize];
            let (dx, dy) = DIRECTIONS[rng.below(16) as usize];
            let length = spacing + i64::from(rng.below((spacing / 3 + 1) as u32));
            let origin = share[from].0;
            // Branches grow away from the corner, out over the share.
            let (ox, oy) = (origin.0 + 90 + 1, origin.1 - 90 - 1);
            if dx * ox + dy * oy < 0 {
                continue;
            }
            let p = (
                origin.0 + (i64::from(dx) * length / 100) as i32,
                origin.1 + (i64::from(dy) * length / 100) as i32,
            );
            if !(-96..=-6).contains(&p.0) || !(6..=96).contains(&p.1) {
                continue;
            }
            if distance2(p, (0, 0)) < 30 * 30 {
                continue;
            }
            let clear = share.iter().all(|&(q, _)| {
                (0..4).all(|quarters| distance2(p, turn(q, quarters)) >= spacing * spacing)
            });
            let tangled = spacing > 4
                && drawn
                    .iter()
                    .any(|&(c, d)| c != origin && d != origin && cross(origin, p, c, d));
            if clear && !tangled {
                children[from] += 1;
                branches.push((from, share.len()));
                share.push((p, Kind::Free));
                placed = true;
                break;
            }
        }
        if !placed {
            spacing -= 1;
        }
    }

    // How long a link may be, short of joining up what would otherwise not be.
    let reach = 2 * nominal;
    let mut net = Net::new(share);
    let n = net.len();
    for &(a, b) in &branches {
        net.link(4 * a, 4 * b);
    }

    // The other links a map can draw on: those between near neighbours, with
    // no host inside the circle across them, one per orbit.
    let mut candidates: BTreeMap<(usize, usize), i64> = BTreeMap::new();
    for a in 0..n {
        for b in a + 1..n {
            if !net.allowed(a, b) || net.linked(a, b) {
                continue;
            }
            let (pa, pb) = (net.pos(a), net.pos(b));
            let length = distance2(pa, pb);
            let open = (0..n).all(|c| {
                c == a || c == b || distance2(pa, net.pos(c)) + distance2(pb, net.pos(c)) >= length
            });
            if open {
                candidates.insert(net.key(a, b), length);
            }
        }
    }
    let mut candidates: Vec<(i64, (usize, usize))> = candidates
        .into_iter()
        .map(|(key, length)| (length, key))
        .collect();
    candidates.sort();

    // The corners' branches meet through free hosts, so crews can always
    // reach each other without the Legacy Net.
    while !net.joined(0, 1) && net.bridge(reach, |kind| !kind.legacy()) {}

    // The Legacy Net's middle: Cortex to the grid, each stronghold to the
    // grid host nearest it, then the shortest way from the middle out (not
    // through the end of a branch, if it can be helped).
    let cortex = net.cortex();
    net.link(cortex, 4);
    for index in 0..net.share.len() {
        if net.share[index].1 == Kind::Stronghold {
            let at = net.share[index].0;
            let grid = (4..8)
                .min_by_key(|&g| distance2(at, net.pos(g)))
                .unwrap_or(4);
            net.link(4 * index, grid);
        }
    }
    for spare_tips in [true, false] {
        for &(_, (a, b)) in &candidates {
            if spare_tips && (net.tip(a) || net.tip(b)) {
                continue;
            }
            if !net.joined(a, b) && !net.crosses(a, b) {
                net.link(a, b);
            }
        }
    }
    while (1..n).any(|h| !net.joined(0, h)) && net.bridge(reach, |_| true) {}

    // A hideout always has two ways out.
    while net.degree(0) < 2 {
        let next = candidates
            .iter()
            .map(|&(_, key)| key)
            .find(|&(a, b)| (a == 0 || b == 0) && !net.linked(a, b))
            .or_else(|| {
                (0..n)
                    .filter(|&h| net.kind(h) == Kind::Free && !net.linked(0, h))
                    .min_by_key(|&h| (distance2(net.pos(0), net.pos(h)), h))
                    .map(|h| (0, h))
            });
        match next {
            Some((a, b)) => net.link(a, b),
            None => break,
        }
    }

    // Then the extra links, each kept by the style's odds: short ones that
    // cross nothing, and none that would give a hideout a third way out.
    for &(length, (a, b)) in &candidates {
        if net.linked(a, b) || length > reach * reach || net.crosses(a, b) {
            continue;
        }
        let at_hideout = net.kind(a) == Kind::Hideout || net.kind(b) == Kind::Hideout;
        if at_hideout && net.degree(0) >= 2 {
            continue;
        }
        if rng.below(100) < style {
            net.link(a, b);
        }
    }

    build(&net, &mut rng)
}

/// Gives every host its role, district, name and yields, and makes the map.
fn build(net: &Net, rng: &mut Rng) -> GameData {
    let n = net.len();
    let neighbours = net.neighbours();

    // Links from the nearest hideout.
    let mut depth = vec![usize::MAX; n];
    let mut queue = VecDeque::new();
    for (hideout, d) in depth.iter_mut().enumerate().take(4) {
        *d = 0;
        queue.push_back(hideout);
    }
    while let Some(host) = queue.pop_front() {
        for &next in &neighbours[host] {
            if depth[next] == usize::MAX {
                depth[next] = depth[host] + 1;
                queue.push_back(next);
            }
        }
    }
    let deepest = (0..net.share.len())
        .filter(|&i| net.share[i].1 == Kind::Free)
        .map(|i| depth[4 * i])
        .max()
        .unwrap_or(1);
    let span = deepest.saturating_sub(2).max(1);

    let mut pools: BTreeMap<District, Vec<&'static str>> = BTreeMap::new();
    for district in [
        District::Metro,
        District::Bankwire,
        District::Nimbus,
        District::Orbital,
        District::Foundry,
        District::Campus,
        District::Ministry,
        District::Helix,
        District::Lattice,
    ] {
        pools.insert(district, names(district).to_vec());
    }
    let mut hideout_names = HIDEOUT_NAMES.to_vec();

    let mut hosts: Vec<Option<HostDef>> = vec![None; n];
    for (index, &(point, kind)) in net.share.iter().enumerate() {
        let role = match kind {
            Kind::Hideout => Role::Hideout,
            Kind::Grid => Role::Grid,
            Kind::Stronghold => Role::Stronghold,
            Kind::Cortex => Role::Cortex,
            Kind::Free => match depth[4 * index] {
                1 if point.1 >= -point.0 => Role::CornerEdge,
                1 => Role::CornerSide,
                2 => Role::CornerBack,
                d => [Role::SideEnd, Role::SideMiddle, Role::Inner][((d - 3) * 3 / span).min(2)],
            },
        };
        let preferred: &[District] = match role {
            Role::Hideout | Role::CornerEdge | Role::CornerSide | Role::CornerBack => {
                &[District::Metro]
            }
            Role::SideEnd | Role::SideMiddle => &SIDE_DISTRICTS,
            Role::Inner | Role::Stronghold => &INNER_DISTRICTS,
            Role::Grid | Role::Cortex => &[District::Lattice],
        };
        let (district, four) = if role == Role::Hideout {
            (District::Metro, draw(&mut hideout_names, rng))
        } else {
            let district = pick(&pools, preferred, rng);
            let pool = pools.get_mut(&district).expect("every district has a pool");
            (district, draw(pool, rng))
        };
        for quarter in 0..4 {
            let (x, y) = turn(point, quarter);
            hosts[4 * index + quarter] = Some(HostDef {
                name: four[quarter],
                district,
                role,
                pos: (x as i16, y as i16),
                hideout: (role == Role::Hideout).then_some(quarter as u8),
                yields: yields(district, role),
                legacy: kind.legacy(),
            });
        }
    }
    hosts[net.cortex()] = Some(HostDef {
        name: "Cortex",
        district: District::Lattice,
        role: Role::Cortex,
        pos: (0, 0),
        hideout: None,
        yields: yields(District::Lattice, Role::Cortex),
        legacy: true,
    });

    let hosts: Vec<HostDef> = hosts.into_iter().flatten().collect();
    let links: Vec<(HostId, HostId)> = net
        .links
        .iter()
        .map(|&(a, b)| (HostId(a as u16), HostId(b as u16)))
        .collect();
    GameData::from_parts(hosts, links)
}

/// One of the preferred districts with four names left, at random; any
/// other with names left if those have run out.
fn pick(
    pools: &BTreeMap<District, Vec<&'static str>>,
    preferred: &[District],
    rng: &mut Rng,
) -> District {
    let open = |district: &District| pools.get(district).is_some_and(|p| p.len() >= 4);
    let mut choice: Vec<District> = preferred.iter().copied().filter(open).collect();
    if choice.is_empty() {
        choice = pools
            .keys()
            .copied()
            .filter(|d| *d != District::Lattice && open(d))
            .collect();
    }
    choice[rng.below(choice.len() as u32) as usize]
}

/// Four names from a pool, at random.
fn draw(pool: &mut Vec<&'static str>, rng: &mut Rng) -> [&'static str; 4] {
    std::array::from_fn(|_| pool.swap_remove(rng.below(pool.len() as u32) as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn maps() -> Vec<GameData> {
        let mut maps = Vec::new();
        for seed in 1..=12u64 {
            for hosts in [20, 33, 41, 60, 80, 100] {
                maps.push(MapSpec::random(seed, hosts).data());
            }
        }
        maps
    }

    #[test]
    fn sizes_round_to_four_corners_and_cortex() {
        assert_eq!(
            MapSpec::random(1, 20),
            MapSpec::Random { seed: 1, hosts: 21 }
        );
        assert_eq!(
            MapSpec::random(1, 41),
            MapSpec::Random { seed: 1, hosts: 41 }
        );
        assert_eq!(
            MapSpec::random(1, 100),
            MapSpec::Random {
                seed: 1,
                hosts: 101
            }
        );
        assert_eq!(
            MapSpec::random(1, 5),
            MapSpec::Random { seed: 1, hosts: 21 }
        );
        for hosts in [20, 41, 77, 100] {
            let MapSpec::Random { hosts: rounded, .. } = MapSpec::random(3, hosts) else {
                unreachable!()
            };
            assert_eq!(MapSpec::random(3, hosts).data().hosts.len() as u32, rounded);
        }
    }

    #[test]
    fn name_pools_do_not_overlap() {
        let mut seen = BTreeSet::new();
        let all = HIDEOUT_NAMES
            .iter()
            .chain(METRO_NAMES.iter())
            .chain(BANKWIRE_NAMES.iter())
            .chain(NIMBUS_NAMES.iter())
            .chain(ORBITAL_NAMES.iter())
            .chain(FOUNDRY_NAMES.iter())
            .chain(MINISTRY_NAMES.iter())
            .chain(HELIX_NAMES.iter())
            .chain(CAMPUS_NAMES.iter())
            .chain(LATTICE_NAMES.iter())
            .chain(["Cortex"].iter());
        for name in all {
            assert!(seen.insert(name.to_lowercase()), "{name} is in two pools");
        }
    }

    #[test]
    fn the_same_seed_makes_the_same_map() {
        let a = MapSpec::random(77, 60).data();
        let b = MapSpec::random(77, 60).data();
        assert_eq!(a, b);
        assert_ne!(a, MapSpec::random(78, 60).data());
    }

    #[test]
    fn random_maps_keep_the_rules_of_the_map() {
        for data in maps() {
            let n = data.hosts.len();
            let names: BTreeSet<_> = data.hosts.iter().map(|h| h.name).collect();
            assert_eq!(names.len(), n, "names are unique");
            assert!(
                data.hosts
                    .iter()
                    .all(|h| h.pos.0.abs() <= 100 && h.pos.1.abs() <= 100)
            );
            let places: BTreeSet<_> = data.hosts.iter().map(|h| h.pos).collect();
            assert_eq!(places.len(), n, "no two hosts in one place");
            assert!(data.links.iter().all(|(a, b)| a != b));

            let everything = data.within([data.hideouts[0]], 1000);
            assert_eq!(everything.len(), n, "the map is connected");

            let cortex = data.find("Cortex").unwrap();
            for (corner, &hideout) in data.hideouts.iter().enumerate() {
                assert_eq!(data.host(hideout).hideout, Some(corner as u8));
                assert_eq!(data.host(hideout).role, Role::Hideout);
                assert!(data.neighbours(hideout).len() >= 2, "two ways out");
                for &next in data.neighbours(hideout) {
                    let def = data.host(next);
                    assert!(!def.legacy && def.role != Role::Hideout);
                }
                assert!(!data.within([hideout], 2).contains(&cortex));
            }
            // The grid is reached only through a stronghold.
            for (index, def) in data.hosts.iter().enumerate() {
                if def.role == Role::Grid {
                    for &next in data.neighbours(HostId(index as u16)) {
                        assert!(data.host(next).legacy);
                    }
                }
            }
            // Crews can meet without going through the Legacy Net.
            let mut seen = BTreeSet::from([data.hideouts[0]]);
            let mut queue = VecDeque::from([data.hideouts[0]]);
            while let Some(host) = queue.pop_front() {
                for &next in data.neighbours(host) {
                    if !data.host(next).legacy && seen.insert(next) {
                        queue.push_back(next);
                    }
                }
            }
            for hideout in data.hideouts {
                assert!(seen.contains(&hideout));
            }
        }
    }

    #[test]
    fn the_four_corners_are_alike() {
        for data in maps() {
            let n = data.hosts.len();
            let cortex = n - 1;
            let turned = |h: usize| {
                if h == cortex {
                    h
                } else {
                    h - h % 4 + (h % 4 + 1) % 4
                }
            };
            let links: BTreeSet<(usize, usize)> = data
                .links
                .iter()
                .map(|&(a, b)| (a.index().min(b.index()), a.index().max(b.index())))
                .collect();
            for &(a, b) in &links {
                let (x, y) = (turned(a), turned(b));
                assert!(
                    links.contains(&(x.min(y), x.max(y))),
                    "every link has its copies"
                );
            }
            for h in 0..n {
                let (a, b) = (data.hosts[h].clone(), data.hosts[turned(h)].clone());
                assert_eq!(
                    (a.role, a.district, a.yields, a.legacy),
                    (b.role, b.district, b.yields, b.legacy)
                );
                assert_eq!(
                    (i32::from(a.pos.1), -i32::from(a.pos.0)),
                    (i32::from(b.pos.0), i32::from(b.pos.1))
                );
            }
        }
    }

    #[test]
    fn some_maps_branch_and_some_are_webs() {
        let mut dead_ends = Vec::new();
        let mut degrees = Vec::new();
        for seed in 1..=40u64 {
            let data = MapSpec::random(seed, 41).data();
            let ends = data
                .host_ids()
                .filter(|&h| data.neighbours(h).len() == 1)
                .count();
            dead_ends.push(ends);
            degrees.push(200 * data.links.len() / data.hosts.len());
        }
        assert!(
            dead_ends.iter().any(|&e| e >= 8),
            "branching maps: {dead_ends:?}"
        );
        assert!(dead_ends.contains(&0), "web maps: {dead_ends:?}");
        assert!(degrees.iter().any(|&d| d >= 300), "dense maps: {degrees:?}");
    }
}
