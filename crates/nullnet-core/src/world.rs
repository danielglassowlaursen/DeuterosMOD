use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::data::{GameData, Role, Upgrade, Weakness, Yields, rules};
use crate::ids::{HackerId, HostId, PlayerId};
use crate::rng::Rng;
use crate::score::GameEnd;

/// How hard the Legacy Net and the hosts are, chosen when a game is created.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Difficulty {
    Easy,
    #[default]
    Normal,
    Hard,
}

impl Difficulty {
    pub const ALL: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard];

    pub fn name(self) -> &'static str {
        match self {
            Difficulty::Easy => "Easy",
            Difficulty::Normal => "Normal",
            Difficulty::Hard => "Hard",
        }
    }

    /// Trace at which the Legacy Net sweeps a crew.
    pub fn sweep_at(self) -> u32 {
        match self {
            Difficulty::Easy => 9,
            Difficulty::Normal => 6,
            Difficulty::Hard => 4,
        }
    }

    /// The first turn the Legacy Net spreads, and how many turns apart.
    pub fn spread(self) -> (u32, u32) {
        match self {
            Difficulty::Easy => (15, 6),
            Difficulty::Normal => (10, 4),
            Difficulty::Hard => (6, 3),
        }
    }

    /// Added to every host's rolled security, which stays within 1 to 5.
    pub fn security_shift(self) -> i8 {
        match self {
            Difficulty::Easy => -1,
            Difficulty::Normal => 0,
            Difficulty::Hard => 1,
        }
    }

    pub fn start_credits(self) -> u32 {
        match self {
            Difficulty::Easy => 60,
            Difficulty::Normal | Difficulty::Hard => 40,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Settings {
    pub difficulty: Difficulty,
    /// The game ends when this turn has run.
    pub last_turn: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            difficulty: Difficulty::Normal,
            last_turn: rules::DEFAULT_LAST_TURN,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Controller {
    Crew(PlayerId),
    Legacy,
}

/// A crew's way into a host it broke into, good for the next turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Access {
    pub crew: PlayerId,
    /// The last turn it can be used.
    pub until: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostState {
    pub controller: Option<Controller>,
    pub security: u8,
    pub weakness: Weakness,
    pub ice: u8,
    pub access: Vec<Access>,
    /// Crews that know this host's security, weakness and ICE.
    pub scanned: BTreeSet<PlayerId>,
}

impl HostState {
    pub fn has_access(&self, crew: PlayerId, turn: u32) -> bool {
        self.access
            .iter()
            .any(|a| a.crew == crew && a.until >= turn)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hacker {
    pub id: HackerId,
    pub handle: String,
    pub specialty: Weakness,
    pub level: u8,
    pub xp: u32,
    /// The first turn the hacker can work again after being caught.
    pub out_until: u32,
}

impl Hacker {
    pub fn ready(&self, turn: u32) -> bool {
        turn >= self.out_until
    }

    pub fn wage(&self) -> u32 {
        rules::wage(self.level)
    }

    /// Experience still needed for the next level, if there is one.
    pub fn xp_to_level(&self) -> Option<u32> {
        let next = rules::LEVEL_XP.get(usize::from(self.level).checked_sub(1)?)?;
        Some(next.saturating_sub(self.xp))
    }
}

/// A hacker for hire.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offer {
    pub hacker: Hacker,
    pub price: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Upgrades {
    pub rigs: u8,
    pub lines: u8,
    pub firewall: u8,
    pub safehouse: u8,
}

impl Upgrades {
    pub fn level(&self, upgrade: Upgrade) -> u8 {
        match upgrade {
            Upgrade::Rigs => self.rigs,
            Upgrade::Lines => self.lines,
            Upgrade::Firewall => self.firewall,
            Upgrade::Safehouse => self.safehouse,
        }
    }

    pub fn level_mut(&mut self, upgrade: Upgrade) -> &mut u8 {
        match upgrade {
            Upgrade::Rigs => &mut self.rigs,
            Upgrade::Lines => &mut self.lines,
            Upgrade::Firewall => &mut self.firewall,
            Upgrade::Safehouse => &mut self.safehouse,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crew {
    pub name: String,
    pub hideout: HostId,
    pub credits: u32,
    pub compute: u32,
    /// Points.
    pub data: u32,
    pub trace: u32,
    pub hackers: Vec<Hacker>,
    pub kits: BTreeSet<Weakness>,
    pub zero_days: u32,
    pub upgrades: Upgrades,
    pub market: Vec<Offer>,
    /// Hosts the crew has taken from the Legacy Net, each counted once.
    pub freed: BTreeSet<HostId>,
}

impl Crew {
    pub fn hacker(&self, id: HackerId) -> Option<&Hacker> {
        self.hackers.iter().find(|h| h.id == id)
    }

    pub fn slots(&self) -> usize {
        rules::HACKER_SLOTS + usize::from(self.upgrades.safehouse)
    }

    pub fn wages(&self) -> u32 {
        self.hackers.iter().map(Hacker::wage).sum()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub seed: u64,
    pub rng: Rng,
    pub settings: Settings,
    /// The turn being played, from 1.
    pub turn: u32,
    pub hosts: Vec<HostState>,
    pub crews: BTreeMap<PlayerId, Crew>,
    pub next_hacker: u32,
    pub ended: Option<GameEnd>,
}

/// Handles for hackers, in the order they are dealt out.
pub const HANDLES: [&str; 32] = [
    "Zer0", "Nyx", "Glitch", "Cipher", "Rook", "Vex", "Kestrel", "Halcyon", "Byte", "Tess", "Echo",
    "Wraith", "Mantis", "Juno", "Static", "Raven", "Hex", "Lumen", "Onyx", "Specter", "Tamsin",
    "Volt", "Quill", "Sable", "Nova", "Drift", "Jinx", "Morrow", "Ash", "Kilo", "Pixel", "Loki",
];

impl World {
    /// A new game: rolls every host's security, ICE and weakness, puts each
    /// crew in its corner with two hackers suited to the hosts next door,
    /// and opens the first market.
    pub fn new_game(
        data: &GameData,
        seed: u64,
        crews: &[(PlayerId, &str)],
        settings: Settings,
    ) -> Self {
        let mut rng = Rng::new(seed, 0x004e_756c_6c4e_6574);
        let shift = settings.difficulty.security_shift();
        let mut rolled = BTreeMap::new();
        for role in Role::ALL {
            let (lo, hi) = role.security();
            let security = rng.range_inclusive(lo.into(), hi.into()) as i8 + shift;
            let (lo, hi) = role.ice();
            let ice = rng.range_inclusive(lo.into(), hi.into()) as u8;
            rolled.insert(role, (security.clamp(1, 5) as u8, ice));
        }
        let hosts = data
            .hosts
            .iter()
            .map(|def| {
                let (security, ice) = rolled[&def.role];
                HostState {
                    controller: def.legacy.then_some(Controller::Legacy),
                    security,
                    weakness: Weakness::ALL[rng.below(4) as usize],
                    ice: ice + u8::from(def.legacy),
                    access: Vec::new(),
                    scanned: BTreeSet::new(),
                }
            })
            .collect();
        let mut world = World {
            seed,
            rng,
            settings,
            turn: 1,
            hosts,
            crews: BTreeMap::new(),
            next_hacker: 0,
            ended: None,
        };

        for (place, &(player, name)) in crews.iter().enumerate() {
            let hideout = data.hideouts[GameData::start_corner(place)];
            world.hosts[hideout.index()].controller = Some(Controller::Crew(player));
            let mut crew = Crew {
                name: name.to_string(),
                hideout,
                credits: settings.difficulty.start_credits(),
                compute: 5,
                data: 0,
                trace: 0,
                hackers: Vec::new(),
                kits: BTreeSet::new(),
                zero_days: 0,
                upgrades: Upgrades::default(),
                market: Vec::new(),
                freed: BTreeSet::new(),
            };
            // One hacker for each host next door, the better one for the
            // first, so the first break-in has a good chance.
            let mut specialties: Vec<Weakness> = data
                .neighbours(hideout)
                .iter()
                .map(|&h| world.hosts[h.index()].weakness)
                .collect();
            specialties.dedup();
            for (index, level) in [2, 1].into_iter().enumerate() {
                let specialty = match specialties.get(index) {
                    Some(&weakness) => weakness,
                    None => world.other_specialty(&specialties),
                };
                let hacker = world.new_hacker(specialty, level);
                crew.hackers.push(hacker);
            }
            world.crews.insert(player, crew);
        }
        for player in world.crews.keys().copied().collect::<Vec<_>>() {
            world.refresh_market(player);
        }
        world
    }

    pub fn host(&self, id: HostId) -> &HostState {
        &self.hosts[id.index()]
    }

    pub fn controller(&self, id: HostId) -> Option<Controller> {
        self.host(id).controller
    }

    pub fn crew(&self, player: PlayerId) -> Option<&Crew> {
        self.crews.get(&player)
    }

    /// A hideout a crew plays from; nobody can break into it.
    pub fn is_hideout(&self, host: HostId) -> bool {
        self.crews.values().any(|c| c.hideout == host)
    }

    /// Hosts a crew has a usable way into this turn (from a break-in last
    /// turn), whoever holds them.
    pub fn held_access(&self, player: PlayerId) -> Vec<HostId> {
        self.hosts
            .iter()
            .enumerate()
            .filter(|(_, h)| h.has_access(player, self.turn))
            .map(|(i, _)| HostId(i as u16))
            .collect()
    }

    /// Hosts a crew holds, its hideout included.
    pub fn held_by(&self, player: PlayerId) -> impl Iterator<Item = HostId> + '_ {
        self.hosts
            .iter()
            .enumerate()
            .filter(move |(_, h)| h.controller == Some(Controller::Crew(player)))
            .map(|(i, _)| HostId(i as u16))
    }

    /// What a crew's hosts and rigs give it each turn.
    pub fn income(&self, data: &GameData, player: PlayerId) -> Yields {
        let mut total = Yields::default();
        for host in self.held_by(player) {
            total.add(data.host(host).yields);
        }
        if let Some(crew) = self.crew(player) {
            total.compute += 2 * u32::from(crew.upgrades.rigs);
            total.bandwidth += 2 * u32::from(crew.upgrades.lines);
        }
        total
    }

    /// Bandwidth a crew can use this turn.
    pub fn bandwidth(&self, data: &GameData, player: PlayerId) -> u32 {
        self.income(data, player).bandwidth
    }

    /// Whether `player` may break into `host` this turn: linked to a host
    /// it holds, not its own, not a hideout, and not a rival's in the first
    /// turns.
    pub fn can_break_in(&self, data: &GameData, player: PlayerId, host: HostId) -> bool {
        self.break_in_error(data, player, host).is_none()
    }

    pub(crate) fn break_in_error(
        &self,
        data: &GameData,
        player: PlayerId,
        host: HostId,
    ) -> Option<crate::CommandError> {
        use crate::CommandError;
        let state = self.host(host);
        if state.controller == Some(Controller::Crew(player)) {
            return Some(CommandError::OwnHost);
        }
        if self.is_hideout(host) {
            return Some(CommandError::Hideout);
        }
        if let Some(Controller::Crew(_)) = state.controller
            && self.turn <= rules::PEACE_TURNS
        {
            return Some(CommandError::Peace {
                until: rules::PEACE_TURNS + 1,
            });
        }
        let linked = data
            .neighbours(host)
            .iter()
            .any(|&n| self.controller(n) == Some(Controller::Crew(player)));
        if !linked {
            return Some(CommandError::OutOfReach);
        }
        None
    }

    /// Hosts a crew can scan this turn.
    pub fn scan_range(&self, data: &GameData, player: PlayerId) -> BTreeSet<HostId> {
        data.within(self.held_by(player), rules::SCAN_LINKS)
    }

    /// Whether a crew knows a host's security, weakness and ICE.
    pub fn knows(&self, player: PlayerId, host: HostId) -> bool {
        let state = self.host(host);
        state.controller == Some(Controller::Crew(player)) || state.scanned.contains(&player)
    }

    /// A host's defence against a break-in, before any defender.
    pub fn defence(&self, host: HostId) -> u32 {
        let state = self.host(host);
        let firewall = match state.controller {
            Some(Controller::Crew(owner)) => self
                .crew(owner)
                .map_or(0, |c| u32::from(c.upgrades.firewall)),
            _ => 0,
        };
        2 * u32::from(state.security) + u32::from(state.ice) + firewall
    }

    pub(crate) fn new_hacker(&mut self, specialty: Weakness, level: u8) -> Hacker {
        let id = HackerId(self.next_hacker);
        self.next_hacker += 1;
        let handle = self.free_handle();
        Hacker {
            id,
            handle,
            specialty,
            level,
            xp: rules::LEVEL_XP
                .get(usize::from(level).wrapping_sub(2))
                .copied()
                .unwrap_or(0),
            out_until: 0,
        }
    }

    fn other_specialty(&mut self, taken: &[Weakness]) -> Weakness {
        let free: Vec<Weakness> = Weakness::ALL
            .into_iter()
            .filter(|w| !taken.contains(w))
            .collect();
        free[self.rng.below(free.len() as u32) as usize]
    }

    /// A handle nobody in the game has, or a numbered one when they run out.
    fn free_handle(&mut self) -> String {
        let used: BTreeSet<&str> = self
            .crews
            .values()
            .flat_map(|c| c.hackers.iter().chain(c.market.iter().map(|o| &o.hacker)))
            .map(|h| h.handle.as_str())
            .collect();
        let free: Vec<&str> = HANDLES
            .iter()
            .copied()
            .filter(|h| !used.contains(h))
            .collect();
        if free.is_empty() {
            let base = HANDLES[self.rng.below(HANDLES.len() as u32) as usize];
            return format!("{base}-{}", self.next_hacker);
        }
        free[self.rng.below(free.len() as u32) as usize].to_string()
    }

    /// New offers on a crew's market; better hackers show up later on.
    pub(crate) fn refresh_market(&mut self, player: PlayerId) {
        let Some(crew) = self.crews.get_mut(&player) else {
            return;
        };
        crew.market.clear();
        let later = (self.turn / 20) as u8;
        for _ in 0..rules::MARKET_OFFERS {
            let roll = self.rng.below(100);
            let level = match roll {
                0..50 => 1,
                50..85 => 2,
                _ => 3,
            };
            let level = (level + later).min(rules::MAX_LEVEL);
            let specialty = Weakness::ALL[self.rng.below(4) as usize];
            let hacker = self.new_hacker(specialty, level);
            let price = rules::hacker_price(level);
            if let Some(crew) = self.crews.get_mut(&player) {
                crew.market.push(Offer { hacker, price });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(crews: usize) -> (GameData, World) {
        let data = GameData::standard();
        let names: Vec<(PlayerId, &str)> = (0..crews)
            .map(|i| {
                (
                    PlayerId(i as u8),
                    ["Ghostline", "Blackice", "Nullset", "Rootkit"][i],
                )
            })
            .collect();
        let world = World::new_game(&data, 7, &names, Settings::default());
        (data, world)
    }

    #[test]
    fn crews_start_in_their_corners_with_two_hackers() {
        let (data, world) = game(2);
        assert_eq!(world.crews[&PlayerId(0)].hideout, data.hideouts[0]);
        assert_eq!(world.crews[&PlayerId(1)].hideout, data.hideouts[2]);
        for (&player, crew) in &world.crews {
            assert_eq!(crew.hackers.len(), rules::START_HACKERS);
            assert_eq!(crew.hackers[0].level, 2);
            assert_eq!(crew.market.len(), rules::MARKET_OFFERS);
            assert_eq!(world.held_by(player).collect::<Vec<_>>(), [crew.hideout]);
            // The first hacker fits a host next door.
            let next_door: Vec<Weakness> = data
                .neighbours(crew.hideout)
                .iter()
                .map(|&h| world.host(h).weakness)
                .collect();
            assert!(next_door.contains(&crew.hackers[0].specialty));
        }
        // Empty corners are free hosts.
        assert_eq!(world.controller(data.hideouts[1]), None);
        assert!(!world.is_hideout(data.hideouts[1]));
    }

    #[test]
    fn handles_are_unique() {
        let (_, world) = game(4);
        let handles: Vec<&str> = world
            .crews
            .values()
            .flat_map(|c| c.hackers.iter().chain(c.market.iter().map(|o| &o.hacker)))
            .map(|h| h.handle.as_str())
            .collect();
        let unique: BTreeSet<&str> = handles.iter().copied().collect();
        assert_eq!(unique.len(), handles.len());
    }

    #[test]
    fn same_roles_get_the_same_security() {
        let (data, world) = game(4);
        for role in Role::ALL {
            let values: BTreeSet<u8> = data
                .host_ids()
                .filter(|&h| data.host(h).role == role)
                .map(|h| world.host(h).security)
                .collect();
            assert_eq!(values.len(), 1, "{role:?}");
        }
        let cortex = data.find("Cortex").unwrap();
        assert_eq!(world.controller(cortex), Some(Controller::Legacy));
        assert_eq!(world.host(cortex).security, 5);
    }

    #[test]
    fn difficulty_shifts_security() {
        let data = GameData::standard();
        let crews = [(PlayerId(0), "A")];
        let normal = World::new_game(&data, 3, &crews, Settings::default());
        let hard = World::new_game(
            &data,
            3,
            &crews,
            Settings {
                difficulty: Difficulty::Hard,
                ..Settings::default()
            },
        );
        for host in data.host_ids() {
            let expected = (normal.host(host).security + 1).min(5);
            assert_eq!(hard.host(host).security, expected);
        }
    }

    #[test]
    fn reach_is_one_link_from_a_held_host() {
        let (data, world) = game(2);
        let me = PlayerId(0);
        let beacon = data.find("Beacon").unwrap();
        let transit = data.find("Transit").unwrap();
        let basement = data.find("Basement").unwrap();
        assert!(world.can_break_in(&data, me, beacon));
        assert!(!world.can_break_in(&data, me, transit));
        assert!(!world.can_break_in(&data, me, basement));
        assert!(world.scan_range(&data, me).contains(&transit));
    }
}
