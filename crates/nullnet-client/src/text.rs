//! Turning the core's commands and events into the short lines the client
//! shows: order rows, the turn log and toasts; and the tips that explain
//! hackers, tools, upgrades, operations and hosts.

use nullnet_core::{
    Command, Controller, CrewView, Event, GameData, Hacker, HostId, Offer, Operation, Outcome,
    PlayerId, Relic, Upgrade, Weakness, Yields, rules,
};

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
        Event::PieceFound { host, piece, .. } => format!(
            "Found piece {} of the {} on {}",
            piece.number,
            piece.relic.name(),
            h(*host)
        ),
        Event::PieceTaken { from, piece, .. } => format!(
            "Took a piece of the {} from {}",
            piece.relic.name(),
            crew(*from)
        ),
        Event::PieceLost { by, piece, .. } => format!(
            "{} stole your piece of the {}",
            crew(*by),
            piece.relic.name()
        ),
        Event::RelicCompleted { player, relic, .. } => {
            if Some(*player) == session.status.as_ref().map(|s| s.view.player) {
                format!("You completed the {}: {}", relic.name(), relic.power())
            } else {
                format!("{} completed the {}", crew(*player), relic.name())
            }
        }
        Event::Reclaimed { host, lost, .. } => {
            if *lost {
                format!("The Legacy Net won back {}", h(*host))
            } else {
                format!("The Legacy Net struck at {}, but it held", h(*host))
            }
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

// ------------------------------------------------------------ tips

/// What a host pays its holder each turn, as "4 credits, 1 data".
pub fn yields(y: Yields) -> String {
    let parts: Vec<String> = [
        (y.credits, "credits"),
        (y.compute, "compute"),
        (y.bandwidth, "bandwidth"),
        (y.data, "data"),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, what)| format!("{n} {what}"))
    .collect();
    if parts.is_empty() {
        "nothing".to_string()
    } else {
        parts.join(", ")
    }
}

/// A hacker on the crew: what they are good at and what their level does.
pub fn hacker_tip(h: &Hacker, turn: u32) -> String {
    let mut lines = vec![
        format!(
            "{}: {} specialist, level {} of {}.",
            h.handle,
            h.specialty.name(),
            h.level,
            rules::MAX_LEVEL
        ),
        format!(
            "Break-in attack {} (2 per level), +{} on {} hosts.",
            2 * u32::from(h.level),
            rules::SPECIALTY_BONUS,
            h.specialty.name()
        ),
    ];
    match h.xp_to_level() {
        Some(left) => lines.push(format!(
            "{} experience; {} more for level {}. Each break-in, backdoor, theft or sub-net that works gives 1.",
            h.xp,
            left,
            h.level + 1
        )),
        None => lines.push("At the top level.".to_string()),
    }
    lines.push(format!("Wage {} credits a turn (1 per level).", h.wage()));
    if h.out_until > turn {
        lines.push(format!(
            "Caught on a break-in: lying low until turn {}.",
            h.out_until
        ));
    }
    lines.join("\n")
}

/// A hacker for hire.
pub fn offer_tip(offer: &Offer, room: bool) -> String {
    let h = &offer.hacker;
    let mut lines = vec![
        format!(
            "{}: {} specialist, level {}.",
            h.handle,
            h.specialty.name(),
            h.level
        ),
        format!(
            "Break-in attack {}, +{} on {} hosts.",
            2 * u32::from(h.level),
            rules::SPECIALTY_BONUS,
            h.specialty.name()
        ),
        format!(
            "Hiring costs {} credits, then {} a turn in wages.",
            offer.price,
            h.wage()
        ),
    ];
    if !room {
        lines.push("No room: build a Safehouse level or let a hacker go first.".to_string());
    }
    lines.join("\n")
}

pub fn kit_tip(weakness: Weakness) -> String {
    format!(
        "{}: +{} attack on every break-in into a {} host, for the rest of the game. {} credits.",
        weakness.kit(),
        rules::KIT_BONUS,
        weakness.name(),
        rules::KIT_PRICE
    )
}

pub fn zero_day_tip() -> String {
    format!(
        "Zero-day: +{} attack on one break-in into any host, then it is used up (it replaces a kit's bonus). {} credits. Use it with the 0-day button on a host.",
        rules::ZERO_DAY_BONUS,
        rules::ZERO_DAY_PRICE
    )
}

pub fn upgrade_tip(upgrade: Upgrade, level: u8) -> String {
    let (credits, compute) = upgrade.cost(level);
    format!(
        "{}: {} per level. Now level {} of {}. The next level costs {} credits and {} compute.",
        upgrade.name(),
        upgrade.effect(),
        level,
        upgrade.max_level(),
        credits,
        compute
    )
}

pub fn operation_tip(op: Operation) -> String {
    let bandwidth = op.bandwidth();
    match op {
        Operation::Scan => format!(
            "Scan: learn the host's security, weakness and ICE, and whether it hides a sub-net. Reaches two links from your hosts. {bandwidth} bandwidth, no trace."
        ),
        Operation::BreakIn => format!(
            "Break in: your hacker's attack against the host's defence; each point either way is 10% on the odds. Success gives access next turn and 1 experience. A failure leaves trace, and a bad one gets the hacker caught for a turn or two. {bandwidth} bandwidth."
        ),
        Operation::Backdoor => format!(
            "Plant backdoor: with access, take the host. From then on it pays you every turn and opens the way to the hosts linked to it. {bandwidth} bandwidth."
        ),
        Operation::StealData => format!(
            "Steal data: with access, take three times the host's data (at least 3) as points, without taking the host. {bandwidth} bandwidth."
        ),
        Operation::Defend => format!(
            "Defend: +{} defence on this host this turn, and throw out anyone who has access to it. {bandwidth} bandwidth.",
            rules::DEFEND_BONUS
        ),
        Operation::OpenSubnet => format!(
            "Open sub-net: two hackers whose specialties match the lock open it together. {bandwidth} bandwidth, {} trace.",
            rules::TRACE_SUBNET
        ),
    }
}

/// A host on the map, as the crew knows it.
pub fn host_tip(data: &GameData, view: &CrewView, session: &Session, id: HostId) -> String {
    let Some(def) = data.hosts.get(id.index()) else {
        return String::new();
    };
    let Some(hv) = view.hosts.get(id.index()) else {
        return String::new();
    };
    let holder = match hv.controller {
        None => "free".to_string(),
        Some(Controller::Legacy) => "held by the Legacy Net".to_string(),
        Some(Controller::Crew(p)) if p == view.player => "yours".to_string(),
        Some(Controller::Crew(p)) => format!("held by {}", crew(session, p)),
    };
    let mut lines = vec![
        format!(
            "{}  ·  {}  ·  {}",
            def.name.to_uppercase(),
            def.district.name(),
            holder
        ),
        format!("Pays its holder each turn: {}.", yields(def.yields)),
    ];
    match hv.intel {
        Some(intel) => {
            lines.push(format!(
                "Security {}, {} weakness, ICE {}: defence {}.",
                intel.security,
                intel.weakness.name(),
                intel.ice,
                intel.defence
            ));
            if let Some(piece) = intel.piece {
                lines.push(format!(
                    "Hides piece {} of the {} ({}). Plant a backdoor or steal data here to take it.",
                    piece.number,
                    piece.relic.name(),
                    piece.relic.power()
                ));
            }
            if let Some(subnet) = intel.subnet {
                lines.push(if subnet.open {
                    format!("Open sub-net: +{} credits a turn.", rules::SUBNET_CREDITS)
                } else {
                    format!(
                        "Sealed sub-net: a {} and a {} hacker open it for +{} credits a turn.",
                        subnet.lock[0].name(),
                        subnet.lock[1].name(),
                        rules::SUBNET_CREDITS
                    )
                });
            }
        }
        None => lines.push("Not scanned: its defence is unknown.".to_string()),
    }
    if hv.reclaim {
        lines.push(
            "You took this from the Legacy Net, and it can see it: it may strike to win it back. A hacker defending it helps."
                .to_string(),
        );
    }
    lines.join("\n")
}

/// A relic: its power, how far the crew has got, and who else is close.
pub fn relic_tip(view: &CrewView, session: &Session, relic: Relic) -> String {
    let size = usize::from(view.relic_size);
    let mine = view.me.pieces_of(relic);
    let mut lines = vec![
        format!("{}: {}.", relic.name(), relic.power()),
        format!(
            "You hold {mine} of its {size} pieces. Scan hosts to find the rest, then plant a backdoor or steal data there to take them."
        ),
    ];
    for rival in &view.crews {
        let theirs = rival.pieces.iter().filter(|p| p.relic == relic).count();
        if theirs > 0 {
            lines.push(format!("{} holds {theirs}.", crew(session, rival.player)));
        }
    }
    if mine < size {
        lines.push(
            "Steal data from a rival's host to take one of their loose pieces; a finished relic is safe."
                .to_string(),
        );
    }
    lines.join("\n")
}
