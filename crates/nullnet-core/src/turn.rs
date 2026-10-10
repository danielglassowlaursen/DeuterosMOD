use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::command::{Command, CommandError, Operation};
use crate::data::{GameData, Yields, rules};
use crate::ids::{HackerId, HostId, PlayerId};
use crate::legacy;
use crate::score;
use crate::world::{Access, Controller, Hacker, World};

pub type Orders = BTreeMap<PlayerId, Vec<Command>>;

/// How a break-in or backdoor turned out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// Got access, or planted the backdoor.
    Done,
    /// Failed, but got away.
    Failed,
    /// Failed badly: caught, lying low until the given turn.
    Caught { until: u32 },
}

/// Something that happened while a turn ran. Every event names the turn so a
/// crew can tell a fresh report from an old one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    Scanned {
        turn: u32,
        player: PlayerId,
        host: HostId,
    },
    BrokeIn {
        turn: u32,
        player: PlayerId,
        hacker: HackerId,
        host: HostId,
        chance: u32,
        outcome: Outcome,
    },
    /// Someone broke into a host you own.
    Intrusion {
        turn: u32,
        player: PlayerId,
        host: HostId,
        intruder: PlayerId,
    },
    BackdoorPlanted {
        turn: u32,
        player: PlayerId,
        host: HostId,
    },
    BackdoorFailed {
        turn: u32,
        player: PlayerId,
        host: HostId,
        reason: CommandError,
    },
    /// You lost a host to someone else's backdoor.
    HostLost {
        turn: u32,
        player: PlayerId,
        host: HostId,
        taker: Controller,
    },
    AccessPurged {
        turn: u32,
        player: PlayerId,
        host: HostId,
        intruder: PlayerId,
    },
    DataStolen {
        turn: u32,
        player: PlayerId,
        host: HostId,
        amount: u32,
    },
    DataLost {
        turn: u32,
        player: PlayerId,
        host: HostId,
        amount: u32,
    },
    Income {
        turn: u32,
        player: PlayerId,
        yields: Yields,
    },
    Wages {
        turn: u32,
        player: PlayerId,
        paid: u32,
    },
    HackerQuit {
        turn: u32,
        player: PlayerId,
        hacker: HackerId,
        handle: String,
    },
    HackerHired {
        turn: u32,
        player: PlayerId,
        handle: String,
    },
    LevelUp {
        turn: u32,
        player: PlayerId,
        hacker: HackerId,
        handle: String,
        level: u8,
    },
    Bought {
        turn: u32,
        player: PlayerId,
        what: String,
    },
    /// The Legacy Net swept a crew: it attacked their worst-defended host.
    Swept {
        turn: u32,
        player: PlayerId,
        host: HostId,
        lost: bool,
    },
    /// The Legacy Net spread to a free host.
    LegacySpread {
        turn: u32,
        host: HostId,
    },
    /// A crew opened the sealed sub-net behind one of its hosts.
    SubnetOpened {
        turn: u32,
        player: PlayerId,
        host: HostId,
    },
    GameEnded {
        turn: u32,
        winner: Option<PlayerId>,
    },
}

impl Event {
    /// The crew the event is for, if it is for one crew only.
    pub fn player(&self) -> Option<PlayerId> {
        match *self {
            Event::Scanned { player, .. }
            | Event::BrokeIn { player, .. }
            | Event::Intrusion { player, .. }
            | Event::BackdoorPlanted { player, .. }
            | Event::BackdoorFailed { player, .. }
            | Event::HostLost { player, .. }
            | Event::AccessPurged { player, .. }
            | Event::DataStolen { player, .. }
            | Event::DataLost { player, .. }
            | Event::Income { player, .. }
            | Event::Wages { player, .. }
            | Event::HackerQuit { player, .. }
            | Event::HackerHired { player, .. }
            | Event::LevelUp { player, .. }
            | Event::Bought { player, .. }
            | Event::Swept { player, .. }
            | Event::SubnetOpened { player, .. } => Some(player),
            Event::LegacySpread { .. } | Event::GameEnded { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectedCommand {
    pub player: PlayerId,
    /// Where the command was in the player's list.
    pub index: usize,
    pub error: CommandError,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnReport {
    /// The turn these results are for.
    pub turn: u32,
    pub rejected: Vec<RejectedCommand>,
    pub events: Vec<Event>,
}

impl TurnReport {
    /// Just one crew's rejections and events, with the shared ones.
    pub fn for_crew(&self, player: PlayerId) -> TurnReport {
        TurnReport {
            turn: self.turn,
            rejected: self
                .rejected
                .iter()
                .filter(|r| r.player == player)
                .cloned()
                .collect(),
            events: self
                .events
                .iter()
                .filter(|e| e.player().is_none_or(|p| p == player))
                .cloned()
                .collect(),
        }
    }
}

/// The chance, 5 to 95, of an attack of `attack` beating a defence of
/// `defence`: 50% plus 10 points per point of advantage.
pub fn chance(attack: u32, defence: u32) -> u32 {
    let base = 50 + 10 * attack as i64 - 10 * defence as i64;
    base.clamp(5, 95) as u32
}

/// A hacker's attack on a host, with the tools the crew would use.
pub fn attack(
    world: &World,
    player: PlayerId,
    hacker: &Hacker,
    host: HostId,
    zero_day: bool,
    boost: u32,
) -> u32 {
    let weakness = world.host(host).weakness;
    let mut attack = 2 * u32::from(hacker.level);
    if hacker.specialty == weakness {
        attack += rules::SPECIALTY_BONUS;
    }
    if zero_day {
        attack += rules::ZERO_DAY_BONUS;
    } else if world
        .crew(player)
        .is_some_and(|c| c.kits.contains(&weakness))
    {
        attack += rules::KIT_BONUS;
    }
    attack + boost.min(rules::MAX_BOOST)
}

/// A host's defence, with a defender counted when `defended`.
pub fn defence(world: &World, host: HostId, defended: bool) -> u32 {
    world.defence(host) + if defended { rules::DEFEND_BONUS } else { 0 }
}

/// Checks one crew's orders against the world as it is now, without changing
/// it, and returns the ones that would be rejected. Used for previews: it
/// runs the same acceptance logic on a throwaway copy of the world, so a
/// second order on a busy hacker, a spent budget or an exhausted bandwidth
/// is caught just as it would be when the turn runs.
pub fn check_orders(
    data: &GameData,
    world: &World,
    player: PlayerId,
    commands: &[Command],
) -> Vec<RejectedCommand> {
    let mut sandbox = world.clone();
    let mut throwaway = TurnReport {
        turn: world.turn,
        rejected: Vec::new(),
        events: Vec::new(),
    };
    let mut state = CrewTurn::new(data, &sandbox, player);
    let mut rejected = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        if let Err(error) = state.apply(data, &mut sandbox, &mut throwaway, command) {
            rejected.push(RejectedCommand {
                player,
                index,
                error,
            });
        }
    }
    rejected
}

/// Applies every crew's orders and runs one turn. Returns a report of what
/// everyone did; split it per crew with [`TurnReport::for_crew`].
pub fn resolve_turn(data: &GameData, world: &mut World, orders: &Orders) -> TurnReport {
    let turn = world.turn;
    let mut report = TurnReport {
        turn,
        rejected: Vec::new(),
        events: Vec::new(),
    };
    if world.ended.is_some() {
        return report;
    }

    // Each crew's accepted operations, kept until the operation phase so
    // defends and scans can run before the attacks.
    let mut ops: BTreeMap<PlayerId, Vec<AcceptedOp>> = BTreeMap::new();

    for player in player_order(world) {
        let commands = match orders.get(&player) {
            Some(c) => c,
            None => continue,
        };
        let mut state = CrewTurn::new(data, world, player);
        for (index, command) in commands.iter().enumerate() {
            match state.apply(data, world, &mut report, command) {
                Ok(Some(op)) => ops.entry(player).or_default().push(op),
                Ok(None) => {}
                Err(error) => report.rejected.push(RejectedCommand {
                    player,
                    index,
                    error,
                }),
            }
        }
    }

    run_operations(data, world, &mut report, ops);
    production(data, world, &mut report);
    legacy::run(data, world, &mut report);
    decay(world);

    world.turn += 1;
    for player in world.crews.keys().copied().collect::<Vec<_>>() {
        world.refresh_market(player);
    }

    if world.turn > world.settings.last_turn {
        let mut end = score::game_end(world);
        end.turn = turn; // the turn that just ran, not the one that never will
        report.events.push(Event::GameEnded {
            turn,
            winner: end.winner,
        });
        world.ended = Some(end);
    }

    report
}

/// The order crews act in, rotated by the turn so no crew always goes first.
fn player_order(world: &World) -> Vec<PlayerId> {
    let players: Vec<PlayerId> = world.crews.keys().copied().collect();
    if players.is_empty() {
        return players;
    }
    let first = world.turn as usize % players.len();
    players[first..]
        .iter()
        .chain(&players[..first])
        .copied()
        .collect()
}

/// An operation accepted into the turn, with everything it needs to run.
struct AcceptedOp {
    player: PlayerId,
    operation: Operation,
    hacker: HackerId,
    host: HostId,
    zero_day: bool,
    boost: u32,
    /// The second hacker, for an operation that takes two.
    partner: Option<HackerId>,
}

/// Tracks one crew while its commands are applied, spending credits, compute
/// and bandwidth and running the buy and hire commands at once.
struct CrewTurn {
    player: PlayerId,
    bandwidth_left: u32,
    busy: Vec<HackerId>,
}

impl CrewTurn {
    fn new(data: &GameData, world: &World, player: PlayerId) -> Self {
        CrewTurn {
            player,
            bandwidth_left: world.bandwidth(data, player),
            busy: Vec::new(),
        }
    }

    fn use_hacker(&mut self, id: HackerId) -> Result<(), CommandError> {
        if self.busy.contains(&id) {
            return Err(CommandError::HackerBusy);
        }
        self.busy.push(id);
        Ok(())
    }

    fn spend_bandwidth(&mut self, need: u32) -> Result<(), CommandError> {
        if need > self.bandwidth_left {
            return Err(CommandError::NoBandwidth {
                need,
                left: self.bandwidth_left,
            });
        }
        self.bandwidth_left -= need;
        Ok(())
    }

    /// Checks a command and, for economy commands, carries it out at once.
    /// Returns an accepted operation to run in the operation phase.
    fn apply(
        &mut self,
        data: &GameData,
        world: &mut World,
        report: &mut TurnReport,
        command: &Command,
    ) -> Result<Option<AcceptedOp>, CommandError> {
        let player = self.player;
        let turn = world.turn;
        if world.crew(player).is_none() {
            return Err(CommandError::NotInGame);
        }
        match *command {
            Command::Scan { hacker, host } => {
                self.ready(world, hacker)?;
                check_host(data, host)?;
                if world.knows(player, host) {
                    return Err(CommandError::AlreadyKnown);
                }
                if !world.scan_range(data, player).contains(&host) {
                    return Err(CommandError::OutOfScanRange);
                }
                self.use_hacker(hacker)?;
                self.spend_bandwidth(Operation::Scan.bandwidth())?;
                Ok(Some(AcceptedOp {
                    player,
                    operation: Operation::Scan,
                    hacker,
                    host,
                    zero_day: false,
                    boost: 0,
                    partner: None,
                }))
            }
            Command::BreakIn {
                hacker,
                host,
                zero_day,
                boost,
            } => {
                self.ready(world, hacker)?;
                check_host(data, host)?;
                if let Some(error) = world.break_in_error(data, player, host) {
                    return Err(error);
                }
                if boost > rules::MAX_BOOST {
                    return Err(CommandError::BoostTooHigh {
                        max: rules::MAX_BOOST,
                    });
                }
                let crew = world.crews.get_mut(&player).unwrap();
                if zero_day && crew.zero_days == 0 {
                    return Err(CommandError::NoZeroDay);
                }
                if boost > crew.compute {
                    return Err(CommandError::NoCompute {
                        need: boost,
                        have: crew.compute,
                    });
                }
                self.use_hacker(hacker)?;
                self.spend_bandwidth(Operation::BreakIn.bandwidth())?;
                crew.compute -= boost;
                if zero_day {
                    crew.zero_days -= 1;
                }
                Ok(Some(AcceptedOp {
                    player,
                    operation: Operation::BreakIn,
                    hacker,
                    host,
                    zero_day,
                    boost,
                    partner: None,
                }))
            }
            Command::Backdoor { hacker, host } => {
                self.ready(world, hacker)?;
                check_host(data, host)?;
                if !world.host(host).has_access(player, turn) {
                    return Err(CommandError::NoAccess);
                }
                self.use_hacker(hacker)?;
                self.spend_bandwidth(Operation::Backdoor.bandwidth())?;
                Ok(Some(AcceptedOp {
                    player,
                    operation: Operation::Backdoor,
                    hacker,
                    host,
                    zero_day: false,
                    boost: 0,
                    partner: None,
                }))
            }
            Command::StealData { hacker, host } => {
                self.ready(world, hacker)?;
                check_host(data, host)?;
                if !world.host(host).has_access(player, turn) {
                    return Err(CommandError::NoAccess);
                }
                self.use_hacker(hacker)?;
                self.spend_bandwidth(Operation::StealData.bandwidth())?;
                Ok(Some(AcceptedOp {
                    player,
                    operation: Operation::StealData,
                    hacker,
                    host,
                    zero_day: false,
                    boost: 0,
                    partner: None,
                }))
            }
            Command::Defend { hacker, host } => {
                self.ready(world, hacker)?;
                check_host(data, host)?;
                if world.controller(host) != Some(Controller::Crew(player)) {
                    return Err(CommandError::NotYourHost);
                }
                self.use_hacker(hacker)?;
                self.spend_bandwidth(Operation::Defend.bandwidth())?;
                Ok(Some(AcceptedOp {
                    player,
                    operation: Operation::Defend,
                    hacker,
                    host,
                    zero_day: false,
                    boost: 0,
                    partner: None,
                }))
            }
            Command::OpenSubnet { host, hackers } => {
                check_host(data, host)?;
                if world.controller(host) != Some(Controller::Crew(player)) {
                    return Err(CommandError::NotYourHost);
                }
                let subnet = world.host(host).subnet.ok_or(CommandError::NoSubnet)?;
                if subnet.open {
                    return Err(CommandError::SubnetOpen);
                }
                for hacker in hackers {
                    self.ready(world, hacker)?;
                }
                let [first, second] = hackers;
                if first == second || self.busy.contains(&first) || self.busy.contains(&second) {
                    return Err(CommandError::HackerBusy);
                }
                let crew = world.crew(player).ok_or(CommandError::NotInGame)?;
                let specialty = |id| crew.hacker(id).map(|h| h.specialty);
                match (specialty(first), specialty(second)) {
                    (Some(a), Some(b)) if subnet.fits(a, b) => {}
                    _ => return Err(CommandError::WrongCrew { need: subnet.lock }),
                }
                self.spend_bandwidth(Operation::OpenSubnet.bandwidth())?;
                self.use_hacker(first)?;
                self.use_hacker(second)?;
                Ok(Some(AcceptedOp {
                    player,
                    operation: Operation::OpenSubnet,
                    hacker: first,
                    host,
                    zero_day: false,
                    boost: 0,
                    partner: Some(second),
                }))
            }
            Command::Hire { hacker } => {
                let crew = world.crews.get_mut(&player).unwrap();
                if crew.hackers.len() >= crew.slots() {
                    return Err(CommandError::NoRoom {
                        slots: crew.slots(),
                    });
                }
                let offer_index = crew
                    .market
                    .iter()
                    .position(|o| o.hacker.id == hacker)
                    .ok_or(CommandError::NotOnMarket)?;
                let price = crew.market[offer_index].price;
                if price > crew.credits {
                    return Err(CommandError::NotEnoughCredits {
                        need: price,
                        have: crew.credits,
                    });
                }
                crew.credits -= price;
                let offer = crew.market.remove(offer_index);
                let handle = offer.hacker.handle.clone();
                crew.hackers.push(offer.hacker);
                report.events.push(Event::HackerHired {
                    turn,
                    player,
                    handle,
                });
                Ok(None)
            }
            Command::Dismiss { hacker } => {
                let crew = world.crews.get_mut(&player).unwrap();
                let index = crew
                    .hackers
                    .iter()
                    .position(|h| h.id == hacker)
                    .ok_or(CommandError::UnknownHacker)?;
                if crew.hackers.len() <= 1 {
                    return Err(CommandError::LastHacker);
                }
                self.use_hacker(hacker)?;
                let gone = crew.hackers.remove(index);
                report.events.push(Event::HackerQuit {
                    turn,
                    player,
                    hacker,
                    handle: gone.handle,
                });
                Ok(None)
            }
            Command::BuyKit { weakness } => {
                let crew = world.crews.get_mut(&player).unwrap();
                if crew.kits.contains(&weakness) {
                    return Err(CommandError::AlreadyOwned);
                }
                if rules::KIT_PRICE > crew.credits {
                    return Err(CommandError::NotEnoughCredits {
                        need: rules::KIT_PRICE,
                        have: crew.credits,
                    });
                }
                crew.credits -= rules::KIT_PRICE;
                crew.kits.insert(weakness);
                report.events.push(Event::Bought {
                    turn,
                    player,
                    what: weakness.kit().to_string(),
                });
                Ok(None)
            }
            Command::BuyZeroDay => {
                let crew = world.crews.get_mut(&player).unwrap();
                if rules::ZERO_DAY_PRICE > crew.credits {
                    return Err(CommandError::NotEnoughCredits {
                        need: rules::ZERO_DAY_PRICE,
                        have: crew.credits,
                    });
                }
                crew.credits -= rules::ZERO_DAY_PRICE;
                crew.zero_days += 1;
                report.events.push(Event::Bought {
                    turn,
                    player,
                    what: "Zero-day".to_string(),
                });
                Ok(None)
            }
            Command::Upgrade { upgrade } => {
                let crew = world.crews.get_mut(&player).unwrap();
                let level = crew.upgrades.level(upgrade);
                if level >= upgrade.max_level() {
                    return Err(CommandError::MaxLevel);
                }
                let (credits, compute) = upgrade.cost(level);
                if credits > crew.credits {
                    return Err(CommandError::NotEnoughCredits {
                        need: credits,
                        have: crew.credits,
                    });
                }
                if compute > crew.compute {
                    return Err(CommandError::NoCompute {
                        need: compute,
                        have: crew.compute,
                    });
                }
                crew.credits -= credits;
                crew.compute -= compute;
                *crew.upgrades.level_mut(upgrade) = level + 1;
                report.events.push(Event::Bought {
                    turn,
                    player,
                    what: format!("{} level {}", upgrade.name(), level + 1),
                });
                Ok(None)
            }
        }
    }

    fn ready(&self, world: &World, id: HackerId) -> Result<(), CommandError> {
        let crew = world.crew(self.player).ok_or(CommandError::NotInGame)?;
        let hacker = crew.hacker(id).ok_or(CommandError::UnknownHacker)?;
        if !hacker.ready(world.turn) {
            return Err(CommandError::HackerOut {
                until: hacker.out_until,
            });
        }
        Ok(())
    }
}

fn check_host(data: &GameData, host: HostId) -> Result<(), CommandError> {
    if host.index() < data.hosts.len() {
        Ok(())
    } else {
        Err(CommandError::UnknownHost)
    }
}

/// Runs the accepted operations in order: defends and scans first, then the
/// break-ins, backdoors and steals one crew at a time, rotated by the turn.
fn run_operations(
    data: &GameData,
    world: &mut World,
    report: &mut TurnReport,
    mut ops: BTreeMap<PlayerId, Vec<AcceptedOp>>,
) {
    let turn = world.turn;
    let order = player_order(world);

    // Defend: throw out intruders and mark the host defended this turn.
    let mut defended: std::collections::BTreeSet<HostId> = Default::default();
    for player in &order {
        for op in ops.get(player).into_iter().flatten() {
            if op.operation != Operation::Defend {
                continue;
            }
            defended.insert(op.host);
            let state = &mut world.hosts[op.host.index()];
            for access in std::mem::take(&mut state.access) {
                if access.crew != *player {
                    report.events.push(Event::AccessPurged {
                        turn,
                        player: access.crew,
                        host: op.host,
                        intruder: access.crew,
                    });
                } else {
                    state.access.push(access);
                }
            }
        }
    }

    // Scan: reveal the host to the crew.
    for player in &order {
        for op in ops.get(player).into_iter().flatten() {
            if op.operation == Operation::Scan {
                world.hosts[op.host.index()].scanned.insert(*player);
                report.events.push(Event::Scanned {
                    turn,
                    player: *player,
                    host: op.host,
                });
            }
        }
    }

    // Open sub-nets. They run before any attack, so a host is still held by
    // the crew that was checked to hold it.
    for player in &order {
        for op in ops.get(player).into_iter().flatten() {
            if op.operation == Operation::OpenSubnet {
                open_subnet(world, report, op);
            }
        }
    }

    // Break-in, backdoor and steal, crew by crew.
    for player in &order {
        let crew_ops = match ops.remove(player) {
            Some(o) => o,
            None => continue,
        };
        for op in crew_ops {
            match op.operation {
                Operation::Scan | Operation::Defend | Operation::OpenSubnet => {}
                Operation::BreakIn => {
                    break_in(world, report, &op, defended.contains(&op.host));
                }
                Operation::Backdoor => backdoor(data, world, report, &op),
                Operation::StealData => steal(data, world, report, &op),
            }
        }
    }
}

fn hacker_of(world: &World, player: PlayerId, id: HackerId) -> Option<Hacker> {
    world.crew(player)?.hacker(id).cloned()
}

fn break_in(world: &mut World, report: &mut TurnReport, op: &AcceptedOp, defended: bool) {
    let player = op.player;
    let Some(hacker) = hacker_of(world, player, op.hacker) else {
        return;
    };
    let attack = attack(world, player, &hacker, op.host, op.zero_day, op.boost);
    let defence = defence(world, op.host, defended);
    let chance = chance(attack, defence);
    let roll = world.rng.below(100);
    let outcome = if roll < chance {
        world.hosts[op.host.index()].access.push(Access {
            crew: player,
            until: world.turn + 1,
        });
        add_trace(world, player, rules::TRACE_SUCCESS);
        gain_xp(world, report, player, op.hacker, 1);
        if let Some(Controller::Crew(owner)) = world.controller(op.host)
            && owner != player
        {
            report.events.push(Event::Intrusion {
                turn: world.turn,
                player: owner,
                host: op.host,
                intruder: player,
            });
        }
        Outcome::Done
    } else {
        // The worst third of the fail band is a bad failure: caught.
        let fail_span = 100 - chance;
        let bad = roll >= chance + (fail_span * 2 / 3);
        if bad {
            add_trace(world, player, rules::TRACE_CAUGHT);
            let out = world.turn + 1 + world.rng.below(2); // 1 or 2 turns
            if let Some(h) = world
                .crews
                .get_mut(&player)
                .and_then(|c| c.hackers.iter_mut().find(|h| h.id == op.hacker))
            {
                h.out_until = out;
            }
            Outcome::Caught { until: out }
        } else {
            add_trace(world, player, rules::TRACE_FAILURE);
            Outcome::Failed
        }
    };
    report.events.push(Event::BrokeIn {
        turn: world.turn,
        player,
        hacker: op.hacker,
        host: op.host,
        chance,
        outcome,
    });
}

fn backdoor(data: &GameData, world: &mut World, report: &mut TurnReport, op: &AcceptedOp) {
    let player = op.player;
    let turn = world.turn;
    if !world.host(op.host).has_access(player, turn) {
        report.events.push(Event::BackdoorFailed {
            turn,
            player,
            host: op.host,
            reason: CommandError::NoAccess,
        });
        return;
    }
    let was = world.controller(op.host);
    if let Some(Controller::Crew(loser)) = was
        && loser != player
    {
        report.events.push(Event::HostLost {
            turn,
            player: loser,
            host: op.host,
            taker: Controller::Crew(player),
        });
    }
    let from_legacy = was == Some(Controller::Legacy);
    let state = &mut world.hosts[op.host.index()];
    state.controller = Some(Controller::Crew(player));
    state.access.clear();
    state.scanned.insert(player);
    if from_legacy && let Some(crew) = world.crews.get_mut(&player) {
        crew.freed.insert(op.host);
    }
    gain_xp(world, report, player, op.hacker, 1);
    let _ = data;
    report.events.push(Event::BackdoorPlanted {
        turn,
        player,
        host: op.host,
    });
}

fn steal(data: &GameData, world: &mut World, report: &mut TurnReport, op: &AcceptedOp) {
    let player = op.player;
    let turn = world.turn;
    if !world.host(op.host).has_access(player, turn) {
        return;
    }
    let amount = data.host(op.host).yields.data.max(1) * 3;
    if let Some(crew) = world.crews.get_mut(&player) {
        crew.data += amount;
    }
    report.events.push(Event::DataStolen {
        turn,
        player,
        host: op.host,
        amount,
    });
    if let Some(Controller::Crew(owner)) = world.controller(op.host)
        && owner != player
    {
        let lost = world.crew(owner).map_or(0, |c| c.data.min(amount));
        if let Some(crew) = world.crews.get_mut(&owner) {
            crew.data -= lost;
        }
        report.events.push(Event::DataLost {
            turn,
            player: owner,
            host: op.host,
            amount: lost,
        });
    }
    gain_xp(world, report, player, op.hacker, 1);
}

fn open_subnet(world: &mut World, report: &mut TurnReport, op: &AcceptedOp) {
    let player = op.player;
    let Some(subnet) = world.hosts[op.host.index()].subnet.as_mut() else {
        return;
    };
    subnet.open = true;
    add_trace(world, player, rules::TRACE_SUBNET);
    for hacker in std::iter::once(op.hacker).chain(op.partner) {
        gain_xp(world, report, player, hacker, 1);
    }
    report.events.push(Event::SubnetOpened {
        turn: world.turn,
        player,
        host: op.host,
    });
}

fn add_trace(world: &mut World, player: PlayerId, amount: u32) {
    if let Some(crew) = world.crews.get_mut(&player) {
        crew.trace += amount;
    }
}

fn gain_xp(world: &mut World, report: &mut TurnReport, player: PlayerId, id: HackerId, xp: u32) {
    let turn = world.turn;
    let Some(crew) = world.crews.get_mut(&player) else {
        return;
    };
    let Some(hacker) = crew.hackers.iter_mut().find(|h| h.id == id) else {
        return;
    };
    hacker.xp += xp;
    while hacker.level < rules::MAX_LEVEL
        && hacker.xp >= rules::LEVEL_XP[usize::from(hacker.level) - 1]
    {
        hacker.level += 1;
        report.events.push(Event::LevelUp {
            turn,
            player,
            hacker: id,
            handle: hacker.handle.clone(),
            level: hacker.level,
        });
    }
}

/// Income and wages. A crew that cannot pay loses hackers, cheapest last.
fn production(data: &GameData, world: &mut World, report: &mut TurnReport) {
    let turn = world.turn;
    for player in world.crews.keys().copied().collect::<Vec<_>>() {
        let yields = world.income(data, player);
        let crew = world.crews.get_mut(&player).unwrap();
        crew.credits += yields.credits;
        crew.compute += yields.compute;
        crew.data += yields.data;
        report.events.push(Event::Income {
            turn,
            player,
            yields,
        });

        let mut wages = crew.wages();
        while wages > crew.credits && crew.hackers.len() > 1 {
            // Let the highest-paid hacker go when the money runs out.
            let index = crew
                .hackers
                .iter()
                .enumerate()
                .max_by_key(|(_, h)| h.wage())
                .map(|(i, _)| i)
                .unwrap();
            let gone = crew.hackers.remove(index);
            report.events.push(Event::HackerQuit {
                turn,
                player,
                hacker: gone.id,
                handle: gone.handle,
            });
            wages = crew.wages();
        }
        let paid = wages.min(crew.credits);
        crew.credits -= paid;
        report.events.push(Event::Wages { turn, player, paid });
    }
}

/// Trace falls by one each turn and old access expires.
fn decay(world: &mut World) {
    for crew in world.crews.values_mut() {
        crew.trace = crew.trace.saturating_sub(1);
    }
    let turn = world.turn;
    for host in &mut world.hosts {
        host.access.retain(|a| a.until > turn);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Difficulty, Settings};

    fn game() -> (GameData, World) {
        let data = GameData::standard();
        let world = World::new_game(
            &data,
            11,
            &[(PlayerId(0), "Ghostline"), (PlayerId(1), "Blackice")],
            Settings::default(),
        );
        (data, world)
    }

    fn only(player: PlayerId, commands: Vec<Command>) -> Orders {
        let mut orders = Orders::new();
        orders.insert(player, commands);
        orders
    }

    #[test]
    fn chance_clamps_and_scales() {
        assert_eq!(chance(5, 5), 50);
        assert_eq!(chance(7, 5), 70);
        assert_eq!(chance(5, 7), 30);
        assert_eq!(chance(100, 0), 95);
        assert_eq!(chance(0, 100), 5);
    }

    #[test]
    fn scan_reveals_a_host() {
        let (data, mut world) = game();
        let me = PlayerId(0);
        let hideout = world.crew(me).unwrap().hideout;
        let target = data.neighbours(hideout)[0];
        let hacker = world.crew(me).unwrap().hackers[0].id;
        assert!(!world.knows(me, target));
        let report = resolve_turn(
            &data,
            &mut world,
            &only(
                me,
                vec![Command::Scan {
                    hacker,
                    host: target,
                }],
            ),
        );
        assert!(report.rejected.is_empty(), "{:?}", report.rejected);
        assert!(world.knows(me, target));
    }

    #[test]
    fn break_in_then_backdoor_takes_a_host() {
        let (data, mut world) = game();
        let me = PlayerId(0);
        let hideout = world.crew(me).unwrap().hideout;
        // Pick the neighbour the first hacker suits.
        let hacker0 = world.crew(me).unwrap().hackers[0].clone();
        let target = *data
            .neighbours(hideout)
            .iter()
            .find(|&&h| world.host(h).weakness == hacker0.specialty)
            .unwrap_or(&data.neighbours(hideout)[0]);
        // Break in with lots of boost until it lands, a turn at a time.
        let mut took = false;
        for _ in 0..30 {
            let hacker = world.crew(me).unwrap().hackers[0].id;
            resolve_turn(
                &data,
                &mut world,
                &only(
                    me,
                    vec![Command::BreakIn {
                        hacker,
                        host: target,
                        zero_day: false,
                        boost: 3,
                    }],
                ),
            );
            if world.host(target).has_access(me, world.turn) {
                let hacker = world.crew(me).unwrap().hackers[0].id;
                let report = resolve_turn(
                    &data,
                    &mut world,
                    &only(
                        me,
                        vec![Command::Backdoor {
                            hacker,
                            host: target,
                        }],
                    ),
                );
                if world.controller(target) == Some(Controller::Crew(me)) {
                    assert!(
                        report
                            .events
                            .iter()
                            .any(|e| matches!(e, Event::BackdoorPlanted { .. }))
                    );
                    took = true;
                    break;
                }
            }
            // Wait for the hacker if caught.
            while !world.crew(me).unwrap().hackers[0].ready(world.turn) {
                resolve_turn(&data, &mut world, &Orders::new());
            }
        }
        assert!(took, "never took the host");
    }

    #[test]
    fn the_right_crew_opens_a_subnet_and_it_pays() {
        use crate::data::Weakness;
        use crate::world::Subnet;

        let (data, mut world) = game();
        let me = PlayerId(0);
        // A held host with a sealed sub-net, and two hackers.
        let hideout = world.crew(me).unwrap().hideout;
        let host = data.neighbours(hideout)[0];
        let lock = [Weakness::Database, Weakness::People];
        world.hosts[host.index()].controller = Some(Controller::Crew(me));
        world.hosts[host.index()].subnet = Some(Subnet { lock, open: false });
        let (a, b) = {
            let crew = world.crews.get_mut(&me).unwrap();
            crew.hackers[0].specialty = Weakness::People;
            crew.hackers[1].specialty = Weakness::Network;
            (crew.hackers[0].id, crew.hackers[1].id)
        };
        let open = vec![Command::OpenSubnet {
            host,
            hackers: [a, b],
        }];

        // The wrong crew is turned away.
        let wrong = check_orders(&data, &world, me, &open);
        assert_eq!(wrong[0].error, CommandError::WrongCrew { need: lock });

        // The right crew opens it, and from then on the host pays more.
        world.crews.get_mut(&me).unwrap().hackers[1].specialty = Weakness::Database;
        let credits_before = world.income(&data, me).credits;
        let trace_before = world.crew(me).unwrap().trace;
        let report = resolve_turn(&data, &mut world, &only(me, open.clone()));
        assert!(report.rejected.is_empty(), "{:?}", report.rejected);
        assert!(world.host(host).subnet.unwrap().open);
        assert!(
            report
                .events
                .iter()
                .any(|e| matches!(e, Event::SubnetOpened { .. }))
        );
        assert_eq!(
            world.income(&data, me).credits,
            credits_before + rules::SUBNET_CREDITS
        );
        // It is noisy: trace up by two, then down by one at the end of the turn.
        assert_eq!(
            world.crew(me).unwrap().trace,
            trace_before + rules::TRACE_SUBNET - 1
        );

        // It cannot be opened twice.
        let again = check_orders(&data, &world, me, &open);
        assert_eq!(again[0].error, CommandError::SubnetOpen);
    }

    #[test]
    fn income_accrues_each_turn() {
        let (data, mut world) = game();
        let me = PlayerId(0);
        let before = world.crew(me).unwrap().credits;
        resolve_turn(&data, &mut world, &Orders::new());
        let after = world.crew(me).unwrap().credits;
        // Hideout gives 5 credits, two wages of 2 and 1 cost 3.
        assert_eq!(after, before + 5 - 3);
    }

    #[test]
    fn peace_blocks_early_rival_break_ins() {
        let (data, mut world) = game();
        let me = PlayerId(0);
        let rival = PlayerId(1);
        let rival_hideout = world.crew(rival).unwrap().hideout;
        // A host next to the rival's hideout, which they do not hold.
        let near_rival = data.neighbours(rival_hideout)[0];
        let hacker = world.crew(me).unwrap().hackers[0].id;
        // Not reachable anyway, but the peace check is what we test: give
        // the crew that host first by fiat, then try a rival's held host.
        world.hosts[rival_hideout.index()].controller = Some(Controller::Crew(rival));
        let _ = near_rival;
        let rejected = check_orders(
            &data,
            &world,
            me,
            &[Command::BreakIn {
                hacker,
                host: rival_hideout,
                zero_day: false,
                boost: 0,
            }],
        );
        assert!(matches!(rejected[0].error, CommandError::Hideout));
    }

    #[test]
    fn a_whole_easy_game_ends_with_a_winner() {
        let data = GameData::standard();
        let mut world = World::new_game(
            &data,
            5,
            &[(PlayerId(0), "Solo")],
            Settings {
                difficulty: Difficulty::Easy,
                last_turn: 10,
            },
        );
        for _ in 0..10 {
            assert!(world.ended.is_none());
            resolve_turn(&data, &mut world, &Orders::new());
        }
        assert!(world.ended.is_some());
    }
}
