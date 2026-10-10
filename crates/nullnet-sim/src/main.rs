//! Plays bot crews against each other and prints what happened: a timeline
//! of the game's events and where each crew ended. The standard map unless
//! `--hosts` asks for a random one of about that many hosts.
//!
//! ```text
//! cargo run -p nullnet-sim -- --seed 7 --crews 3 --turns 50 --difficulty normal
//! cargo run -p nullnet-sim -- --seed 7 --hosts 60
//! ```
//!
//! With `--json` it writes the whole game turn by turn instead, for a replay
//! viewer.

mod replay;

use std::process::ExitCode;

use nullnet_core::mapgen::{MAX_HOSTS, MIN_HOSTS};
use nullnet_core::{
    Difficulty, GameData, MapSpec, Orders, PlayerId, Settings, World, bot_orders, resolve_turn,
    scores,
};

use crate::replay::{Replay, describe};

const NAMES: [&str; 4] = ["Ghostline", "Blackice", "Nullset", "Rootkit"];

struct Options {
    seed: u64,
    crews: usize,
    turns: u32,
    difficulty: Difficulty,
    /// About how many hosts a random map has; the standard map if unset.
    hosts: Option<u32>,
    verbose: bool,
    json: bool,
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut options = Options {
        seed: 1,
        crews: 2,
        turns: 50,
        difficulty: Difficulty::Normal,
        hosts: None,
        verbose: false,
        json: false,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--verbose" => {
                options.verbose = true;
                continue;
            }
            "--json" => {
                options.json = true;
                continue;
            }
            _ => {}
        }
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--seed" => options.seed = value()?.parse().map_err(|e| format!("--seed: {e}"))?,
            "--crews" => options.crews = value()?.parse().map_err(|e| format!("--crews: {e}"))?,
            "--turns" => options.turns = value()?.parse().map_err(|e| format!("--turns: {e}"))?,
            "--hosts" => {
                options.hosts = Some(value()?.parse().map_err(|e| format!("--hosts: {e}"))?)
            }
            "--difficulty" => {
                options.difficulty = match value()?.to_lowercase().as_str() {
                    "easy" => Difficulty::Easy,
                    "normal" => Difficulty::Normal,
                    "hard" => Difficulty::Hard,
                    other => return Err(format!("unknown difficulty {other}")),
                }
            }
            _ => return Err(format!("unknown argument {arg}")),
        }
    }
    if !(1..=NAMES.len()).contains(&options.crews) {
        return Err(format!("--crews must be 1 to {}", NAMES.len()));
    }
    if options.turns == 0 {
        return Err("--turns must be at least 1".to_string());
    }
    if options
        .hosts
        .is_some_and(|h| !(MIN_HOSTS..=MAX_HOSTS).contains(&h))
    {
        return Err(format!("--hosts must be {MIN_HOSTS} to {MAX_HOSTS}"));
    }
    Ok(options)
}

fn main() -> ExitCode {
    let options = match parse(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(error) => {
            eprintln!("nullnet-sim: {error}");
            return ExitCode::FAILURE;
        }
    };

    let map = match options.hosts {
        Some(hosts) => MapSpec::random(options.seed, hosts),
        None => MapSpec::Standard,
    };
    let data = GameData::for_map(map);
    let names: Vec<(PlayerId, &str)> = (0..options.crews)
        .map(|i| (PlayerId(i as u8), NAMES[i]))
        .collect();
    let settings = Settings {
        difficulty: options.difficulty,
        last_turn: options.turns,
        map,
    };
    let mut world = World::new_game(&data, options.seed, &names, settings);
    let mut record = options.json.then(|| Replay::new(&data, &world, &names));

    while world.ended.is_none() {
        let turn = world.turn;
        let mut orders = Orders::new();
        for &(player, _) in &names {
            orders.insert(player, bot_orders(&data, &world, player));
        }
        let report = resolve_turn(&data, &mut world, &orders);

        if let Some(record) = record.as_mut() {
            record.record(&world, &names, &orders, &report);
        } else if options.verbose {
            for event in &report.events {
                let line = describe(&world, event);
                if !line.is_empty() {
                    println!("turn {turn:>3}: {line}");
                }
            }
        }
    }

    if let Some(record) = record {
        match serde_json::to_string(&record) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("nullnet-sim: {error}");
                return ExitCode::FAILURE;
            }
        }
        return ExitCode::SUCCESS;
    }

    // The standings.
    println!(
        "\nAfter {} turns ({}):",
        options.turns,
        options.difficulty.name()
    );
    for score in scores(&world) {
        let crew = world.crew(score.player).unwrap();
        println!(
            "  {:<10} {:>3} points  ({} data, {} hosts, {} freed; {} credits, trace {})",
            crew.name, score.total, score.data, score.hosts, score.freed, crew.credits, crew.trace,
        );
    }
    if let Some(end) = &world.ended {
        match end.winner {
            Some(p) => println!(
                "Winner: {}",
                world.crew(p).map_or("?".to_string(), |c| c.name.clone())
            ),
            None => println!("A tie."),
        }
    }
    ExitCode::SUCCESS
}
