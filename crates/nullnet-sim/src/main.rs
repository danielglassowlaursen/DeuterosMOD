//! Plays bot crews against each other on the classic map and prints what
//! happened: a timeline of the game's main events and where each crew ended.
//!
//! ```text
//! cargo run -p nullnet-sim -- --seed 7 --crews 3 --days 3000
//! ```
//!
//! With `--json` it writes the whole game turn by turn instead: every
//! crew's orders, the events and each crew's state after each turn.

mod replay;

use std::process::ExitCode;

use nullnet_core::{
    Command, CommandError, Controller, Event, GameData, Orders, PlayerId, VesselKind, World, bot,
    calendar_date, resolve_turn,
};

const NAMES: [&str; 4] = ["Ghostline", "Blackice", "Nullsector", "Redshift"];

struct Options {
    seed: u64,
    crews: usize,
    days: u32,
    turn: u32,
    verbose: bool,
    json: bool,
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut options = Options {
        seed: 1,
        crews: 2,
        days: 3000,
        turn: 10,
        verbose: false,
        json: false,
    };
    while let Some(arg) = args.next() {
        if arg == "--verbose" {
            options.verbose = true;
            continue;
        }
        if arg == "--json" {
            options.json = true;
            continue;
        }
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        let number = |text: String| text.parse::<u64>().map_err(|e| format!("{arg}: {e}"));
        match arg.as_str() {
            "--seed" => options.seed = number(value()?)?,
            "--crews" => options.crews = number(value()?)? as usize,
            "--days" => options.days = number(value()?)? as u32,
            "--turn" => options.turn = number(value()?)? as u32,
            _ => return Err(format!("unknown argument {arg}")),
        }
    }
    if !(1..=NAMES.len()).contains(&options.crews) {
        return Err(format!("--crews must be 1 to {}", NAMES.len()));
    }
    if options.turn == 0 {
        return Err("--turn must be at least 1".into());
    }
    Ok(options)
}

fn main() -> ExitCode {
    let options = match parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            eprintln!(
                "usage: nullnet-sim [--seed N] [--crews 1-4] [--days N] [--turn N] [--verbose] [--json]"
            );
            return ExitCode::FAILURE;
        }
    };

    let data = GameData::classic();
    let crews: Vec<(PlayerId, &str)> = (0..options.crews)
        .map(|i| (PlayerId(i as u8), NAMES[i]))
        .collect();
    let mut world = World::new_game(&data, options.seed, &crews);
    let name = |id: PlayerId| NAMES[usize::from(id.0)];
    let host = |id: nullnet_core::HostId| data.host(id).classic.as_str();

    let mut replay = options
        .json
        .then(|| replay::Replay::new(&data, options.seed, options.turn, &crews));
    let mut rejected = 0;
    while world.day < options.days {
        let orders: Orders = crews
            .iter()
            .map(|&(id, _)| (id, bot::orders(&data, &world, id)))
            .collect();
        let days = options.turn.min(options.days - world.day);
        let report = resolve_turn(&data, &mut world, &orders, days);
        for r in &report.rejected {
            let command = &orders[&r.player][r.index];
            let day = date(report.first_day - 1);
            match (command, &r.error) {
                // Two crews installing on the same free host in one turn:
                // the first applied claims it. Part of the game, not a bot bug.
                (Command::Deploy { .. }, &CommandError::HostTaken(h)) => {
                    if !options.json
                        && (r.index == 0
                            || !matches!(orders[&r.player][r.index - 1], Command::Deploy { .. }))
                    {
                        println!("{day}  {:<10}  lost {} to a rival", name(r.player), host(h));
                    }
                }
                _ => {
                    rejected += 1;
                    if !options.json {
                        println!(
                            "{day}  {:<10}  REJECTED {command:?}: {}",
                            name(r.player),
                            r.error
                        );
                    }
                }
            }
        }
        if let Some(replay) = &mut replay {
            replay.record(&world, &orders, &report);
            continue;
        }
        for event in &report.events {
            let line = match *event {
                Event::Unlocked {
                    day,
                    player,
                    milestone,
                } => Some((day, player, format!("milestone {milestone:?}"))),
                Event::ResearchCompleted { day, player, item } => {
                    Some((day, player, format!("researched {item:?}")))
                }
                Event::HostClaimed {
                    day,
                    player,
                    host: h,
                } => Some((day, player, format!("claimed {}", host(h)))),
                Event::Installed {
                    day,
                    player,
                    host: h,
                    item,
                    installed,
                } if options.verbose || installed == nullnet_core::Citadel::MODULES => Some((
                    day,
                    player,
                    format!("{item:?} {installed} installed at {}", host(h)),
                )),
                Event::VesselBurned {
                    day,
                    player,
                    vessel,
                    host: h,
                } => Some((
                    day,
                    player,
                    format!("vessel {} burned at {}", vessel.0, host(h)),
                )),
                Event::VesselStopped {
                    day,
                    player,
                    vessel,
                    reason,
                } => Some((
                    day,
                    player,
                    format!("vessel {} stopped: {reason:?}", vessel.0),
                )),
                Event::RecruitsGraduated {
                    day,
                    player,
                    kind,
                    count,
                } if options.verbose => Some((day, player, format!("{count} {kind:?}s graduated"))),
                Event::ItemBuilt {
                    day, player, item, ..
                } if options.verbose => Some((day, player, format!("built {item:?}"))),
                Event::VesselArrived {
                    day,
                    player,
                    vessel,
                    host: h,
                    berth,
                } if options.verbose => Some((
                    day,
                    player,
                    format!("vessel {} {berth:?} at {}", vessel.0, host(h)),
                )),
                _ => None,
            };
            if let Some((day, player, text)) = line {
                println!("{}  {:<10}  {text}", date(day), name(player));
            }
        }
    }

    if let Some(replay) = &replay {
        println!("{}", serde_json::to_string(replay).expect("serializable"));
        return if rejected > 0 {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        };
    }

    println!();
    println!("After {} days ({}):", world.day, date(world.day));
    for &(id, crew) in &crews {
        let player = &world.players[&id];
        let held: Vec<&str> = world
            .hosts
            .iter()
            .enumerate()
            .filter(|(_, h)| h.controller == Some(Controller::Crew(id)))
            .map(|(i, h)| {
                let id = nullnet_core::HostId(i as u16);
                if h.site.citadel.complete() {
                    host(id)
                } else {
                    "(building)"
                }
            })
            .collect();
        let vessels = |kind| {
            world
                .vessels
                .values()
                .filter(|v| v.owner == id && v.kind == kind)
                .count()
        };
        let researched = player.research.values().filter(|r| r.researched).count();
        println!(
            "  {crew:<10}  taps {}  citadel {}/{}  research {researched}  \
             droppers {}  worms {}  hosts {held:?}",
            player.hideout.taps,
            player.hideout.citadel.modules,
            nullnet_core::Citadel::MODULES,
            vessels(VesselKind::Dropper),
            vessels(VesselKind::Worm),
        );
        if options.verbose {
            let store = |store: &nullnet_core::Store| {
                store
                    .iter()
                    .map(|(item, count)| format!("{item:?} {count}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            println!("    hideout: {}", store(&player.hideout.store));
            println!("    citadel: {}", store(&player.hideout.citadel.store));
        }
    }
    if rejected > 0 {
        println!("{rejected} orders were rejected: the bot gave orders the rules refuse");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn date(day: u32) -> String {
    let (year, day) = calendar_date(day);
    format!("{year}.{day:03}")
}
