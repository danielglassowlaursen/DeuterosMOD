use std::fmt;

use serde::{Deserialize, Serialize};

use crate::data::{Upgrade, Weakness};
use crate::ids::{HackerId, HostId};

/// An order a crew gives for a turn. Buying and hiring happen at once, in
/// the order given, so a hacker hired early in the list can work the same
/// turn; operations are checked when given and run when the turn runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Learn a host's security, weakness and ICE.
    Scan {
        hacker: HackerId,
        host: HostId,
    },
    /// Try to get access to a host linked to one you hold. `boost` spends
    /// that much compute for as many points of attack, up to 3.
    BreakIn {
        hacker: HackerId,
        host: HostId,
        zero_day: bool,
        boost: u32,
    },
    /// Take a host you have access to.
    Backdoor {
        hacker: HackerId,
        host: HostId,
    },
    /// Take data from a host you have access to, without taking the host.
    StealData {
        hacker: HackerId,
        host: HostId,
    },
    /// Guard one of your hosts: harder to break into, and intruders are
    /// thrown out.
    Defend {
        hacker: HackerId,
        host: HostId,
    },
    /// Open the sealed sub-net behind one of your hosts: two hackers whose
    /// specialties match its lock work together for the turn.
    OpenSubnet {
        host: HostId,
        hackers: [HackerId; 2],
    },
    /// Hire a hacker from your market.
    Hire {
        hacker: HackerId,
    },
    /// Let a hacker go.
    Dismiss {
        hacker: HackerId,
    },
    BuyKit {
        weakness: Weakness,
    },
    BuyZeroDay,
    Upgrade {
        upgrade: Upgrade,
    },
}

/// What an operation does, without its hacker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Operation {
    Scan,
    BreakIn,
    Backdoor,
    StealData,
    Defend,
    OpenSubnet,
}

impl Operation {
    pub fn name(self) -> &'static str {
        match self {
            Operation::Scan => "Scan",
            Operation::BreakIn => "Break in",
            Operation::Backdoor => "Plant backdoor",
            Operation::StealData => "Steal data",
            Operation::Defend => "Defend",
            Operation::OpenSubnet => "Open sub-net",
        }
    }

    pub fn bandwidth(self) -> u32 {
        use crate::data::rules::*;
        match self {
            Operation::Scan => SCAN_BANDWIDTH,
            Operation::BreakIn => BREAK_IN_BANDWIDTH,
            Operation::Backdoor => BACKDOOR_BANDWIDTH,
            Operation::StealData => STEAL_BANDWIDTH,
            Operation::Defend => DEFEND_BANDWIDTH,
            Operation::OpenSubnet => SUBNET_BANDWIDTH,
        }
    }
}

impl Command {
    /// The operation, its (first) hacker and host, if this is an operation.
    /// Opening a sub-net takes two hackers; [`Command::hackers`] lists both.
    pub fn operation(&self) -> Option<(Operation, HackerId, HostId)> {
        match *self {
            Command::Scan { hacker, host } => Some((Operation::Scan, hacker, host)),
            Command::BreakIn { hacker, host, .. } => Some((Operation::BreakIn, hacker, host)),
            Command::Backdoor { hacker, host } => Some((Operation::Backdoor, hacker, host)),
            Command::StealData { hacker, host } => Some((Operation::StealData, hacker, host)),
            Command::Defend { hacker, host } => Some((Operation::Defend, hacker, host)),
            Command::OpenSubnet { host, hackers } => {
                Some((Operation::OpenSubnet, hackers[0], host))
            }
            _ => None,
        }
    }

    /// Every hacker the command ties up this turn.
    pub fn hackers(&self) -> Vec<HackerId> {
        match *self {
            Command::OpenSubnet { hackers, .. } => hackers.to_vec(),
            Command::Dismiss { hacker } => vec![hacker],
            _ => self.operation().map(|(_, h, _)| h).into_iter().collect(),
        }
    }

    /// Bandwidth the command takes this turn.
    pub fn bandwidth(&self) -> u32 {
        self.operation().map_or(0, |(op, _, _)| op.bandwidth())
    }

    /// Compute the command spends.
    pub fn compute(&self) -> u32 {
        match *self {
            Command::BreakIn { boost, .. } => boost,
            _ => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandError {
    GameOver,
    NotInGame,
    UnknownHacker,
    UnknownHost,
    /// The hacker already has an operation this turn.
    HackerBusy,
    /// The hacker was caught and is lying low until this turn.
    HackerOut {
        until: u32,
    },
    /// Not linked to a host you hold.
    OutOfReach,
    /// Further than a scan reaches.
    OutOfScanRange,
    OwnHost,
    NotYourHost,
    /// A crew's hideout cannot be broken into.
    Hideout,
    /// Crews leave each other alone until this turn.
    Peace {
        until: u32,
    },
    /// Already scanned.
    AlreadyKnown,
    NoAccess,
    NoBandwidth {
        need: u32,
        left: u32,
    },
    NoCompute {
        need: u32,
        have: u32,
    },
    BoostTooHigh {
        max: u32,
    },
    NoZeroDay,
    NotEnoughCredits {
        need: u32,
        have: u32,
    },
    NoRoom {
        slots: usize,
    },
    NotOnMarket,
    LastHacker,
    AlreadyOwned,
    MaxLevel,
    /// The host has no sealed sub-net.
    NoSubnet,
    /// Its sub-net is already open.
    SubnetOpen,
    /// The two hackers' specialties do not fit the sub-net's lock.
    WrongCrew {
        need: [Weakness; 2],
    },
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::GameOver => write!(f, "the game is over"),
            CommandError::NotInGame => write!(f, "your crew is not in this game"),
            CommandError::UnknownHacker => write!(f, "no such hacker on your crew"),
            CommandError::UnknownHost => write!(f, "no such host"),
            CommandError::HackerBusy => write!(f, "that hacker already has an order this turn"),
            CommandError::HackerOut { until } => {
                write!(f, "that hacker is lying low until turn {until}")
            }
            CommandError::OutOfReach => write!(f, "not linked to a host you hold"),
            CommandError::OutOfScanRange => {
                write!(f, "more than two links from the hosts you hold")
            }
            CommandError::OwnHost => write!(f, "you already hold that host"),
            CommandError::NotYourHost => write!(f, "you do not hold that host"),
            CommandError::Hideout => write!(f, "a crew's hideout cannot be broken into"),
            CommandError::Peace { until } => {
                write!(f, "crews leave each other alone until turn {until}")
            }
            CommandError::AlreadyKnown => write!(f, "you already know that host"),
            CommandError::NoAccess => write!(f, "you have no access to that host"),
            CommandError::NoBandwidth { need, left } => {
                write!(f, "needs {need} bandwidth, {left} left this turn")
            }
            CommandError::NoCompute { need, have } => {
                write!(f, "needs {need} compute, you have {have}")
            }
            CommandError::BoostTooHigh { max } => {
                write!(f, "a break-in can use at most {max} compute")
            }
            CommandError::NoZeroDay => write!(f, "you have no zero-day"),
            CommandError::NotEnoughCredits { need, have } => {
                write!(f, "costs {need} credits, you have {have}")
            }
            CommandError::NoRoom { slots } => write!(f, "your crew is full ({slots} hackers)"),
            CommandError::NotOnMarket => write!(f, "that hacker is not on your market"),
            CommandError::LastHacker => write!(f, "you cannot let your last hacker go"),
            CommandError::AlreadyOwned => write!(f, "you already have that kit"),
            CommandError::MaxLevel => write!(f, "already at the highest level"),
            CommandError::NoSubnet => write!(f, "that host has no sealed sub-net"),
            CommandError::SubnetOpen => write!(f, "that sub-net is already open"),
            CommandError::WrongCrew { need } => write!(
                f,
                "the sub-net opens only for a {} and a {} hacker together",
                need[0].name(),
                need[1].name()
            ),
        }
    }
}

impl std::error::Error for CommandError {}
