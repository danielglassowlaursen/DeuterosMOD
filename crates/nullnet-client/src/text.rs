//! Turning the core's commands and events into the short lines the client
//! shows: order rows, the turn log and toasts.

use nullnet_core::{Command, Event, GameData, HostId, Outcome, PlayerId};

use crate::net::Session;

/// A host's name.
pub fn host(data: &GameData, host: HostId) -> &str {
    data.hosts.get(host.index()).map(|h| h.name).unwrap_or("?")
}

/// A one-line label for an order in the draft list.
pub fn command(cmd: &Command, data: &GameData) -> String {
    match cmd {
        Command::Scan { host: h, .. } => format!("Scan {}", host(data, *h)),
        Command::BreakIn {
            host: h,
            zero_day,
            boost,
            ..
        } => {
            let mut extra = String::new();
            if *boost > 0 {
                extra.push_str(&format!(" +{boost}"));
            }
            if *zero_day {
                extra.push_str(" (0-day)");
            }
            format!("Break into {}{extra}", host(data, *h))
        }
        Command::Backdoor { host: h, .. } => format!("Backdoor {}", host(data, *h)),
        Command::StealData { host: h, .. } => format!("Steal from {}", host(data, *h)),
        Command::Defend { host: h, .. } => format!("Defend {}", host(data, *h)),
        Command::OpenSubnet { host: h, .. } => format!("Open the sub-net on {}", host(data, *h)),
        Command::Hire { .. } => "Hire a hacker".to_string(),
        Command::Dismiss { .. } => "Let a hacker go".to_string(),
        Command::BuyKit { weakness } => format!("Buy {}", weakness.kit()),
        Command::BuyZeroDay => "Buy a zero-day".to_string(),
        Command::Upgrade { upgrade } => format!("Upgrade {}", upgrade.name()),
    }
}

/// How a break-in or backdoor turned out.
pub fn outcome(outcome: Outcome) -> String {
    match outcome {
        Outcome::Done => "got access".to_string(),
        Outcome::Failed => "failed".to_string(),
        Outcome::Caught { until } => format!("caught, out until turn {until}"),
    }
}

/// A line for an event in the turn log or a toast.
pub fn event(ev: &Event, data: &GameData, session: &Session) -> String {
    let h = |host_id: HostId| host(data, host_id).to_string();
    let crew = |p: PlayerId| crew(session, p);
    match ev {
        Event::Scanned { host, .. } => format!("Scanned {}", h(*host)),
        Event::BrokeIn {
            host,
            chance,
            outcome: o,
            ..
        } => format!("Break-in on {} ({chance}%): {}", h(*host), outcome(*o)),
        Event::Intrusion { host, intruder, .. } => {
            format!("{} broke into your {}", crew(*intruder), h(*host))
        }
        Event::BackdoorPlanted { host, .. } => format!("Took {}", h(*host)),
        Event::BackdoorFailed { host, .. } => format!("Backdoor on {} failed", h(*host)),
        Event::HostLost { host, taker, .. } => {
            format!("Lost {} to {}", h(*host), controller(session, *taker))
        }
        Event::AccessPurged { host, .. } => format!("Thrown out of {}", h(*host)),
        Event::DataStolen { host, amount, .. } => {
            format!("Stole {amount} data from {}", h(*host))
        }
        Event::DataLost { host, amount, .. } => format!("Lost {amount} data from {}", h(*host)),
        Event::Income { .. } => String::new(),
        Event::Wages { paid, .. } => {
            if *paid > 0 {
                format!("Paid {paid} in wages")
            } else {
                String::new()
            }
        }
        Event::HackerQuit { handle, .. } => format!("{handle} left the crew"),
        Event::HackerHired { handle, .. } => format!("Hired {handle}"),
        Event::LevelUp { handle, level, .. } => format!("{handle} reached level {level}"),
        Event::Bought { what, .. } => format!("Bought {what}"),
        Event::Swept { host, lost, .. } => {
            if *lost {
                format!("The Legacy Net swept you and took {}", h(*host))
            } else {
                format!("The Legacy Net swept you, but {} held", h(*host))
            }
        }
        Event::SubnetOpened { host, .. } => format!(
            "Opened the sub-net on {}: +{} credits a turn",
            h(*host),
            nullnet_core::rules::SUBNET_CREDITS
        ),
        Event::LegacySpread { host, .. } => {
            format!("The Legacy Net spread to {}", h(*host))
        }
        Event::GameEnded { winner, .. } => match winner {
            Some(p) => format!("Game over: {} wins", crew(*p)),
            None => "Game over: a tie".to_string(),
        },
    }
}

/// A crew's name, from the status's crew list.
pub fn crew(session: &Session, player: PlayerId) -> String {
    session
        .status
        .as_ref()
        .and_then(|s| s.crews.iter().find(|c| c.player == player))
        .map(|c| c.name.clone())
        .unwrap_or_else(|| format!("crew {}", player.0))
}

fn controller(session: &Session, controller: nullnet_core::Controller) -> String {
    match controller {
        nullnet_core::Controller::Crew(p) => crew(session, p),
        nullnet_core::Controller::Legacy => "the Legacy Net".to_string(),
    }
}
