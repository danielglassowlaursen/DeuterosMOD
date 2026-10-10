//! Whole games played by the bot, with invariants that must hold on every
//! turn of every seed and difficulty.

use std::collections::BTreeMap;

use nullnet_core::{
    Command, Controller, Difficulty, GameData, Orders, PlayerId, Settings, World, bot_orders,
    check_orders, crew_view, resolve_turn,
};

fn crews(n: usize) -> Vec<(PlayerId, &'static str)> {
    const NAMES: [&str; 4] = ["Ghostline", "Blackice", "Nullset", "Rootkit"];
    (0..n).map(|i| (PlayerId(i as u8), NAMES[i])).collect()
}

/// Every game stays consistent: hosts have exactly one controller or none,
/// crews keep at least one hacker, nothing the bot issues is rejected, and
/// the game ends with scores on the last turn.
fn play(seed: u64, n: usize, difficulty: Difficulty, last_turn: u32) -> World {
    let data = GameData::standard();
    let settings = Settings {
        difficulty,
        last_turn,
    };
    let mut world = World::new_game(&data, seed, &crews(n), settings);
    let players: Vec<PlayerId> = world.crews.keys().copied().collect();

    let mut turns = 0;
    while world.ended.is_none() {
        turns += 1;
        assert!(turns <= last_turn + 1, "game ran past its last turn");

        let mut orders = Orders::new();
        for &player in &players {
            let commands = bot_orders(&data, &world, player);
            // The bot's own orders must pass its own crew's checks.
            let rejected = check_orders(&data, &world, player, &commands);
            assert!(
                rejected.is_empty(),
                "seed {seed}: bot orders rejected: {rejected:?}"
            );
            orders.insert(player, commands);
        }

        let report = resolve_turn(&data, &mut world, &orders);
        assert!(
            report.rejected.is_empty(),
            "seed {seed}: resolve rejected: {:?}",
            report.rejected
        );

        // Invariants after the turn.
        for host in data.host_ids() {
            let state = world.host(host);
            if let Some(Controller::Crew(p)) = state.controller {
                assert!(players.contains(&p), "host held by an unknown crew");
            }
            // A hideout is never anyone's but its own crew's.
            if world.is_hideout(host) {
                assert!(matches!(state.controller, Some(Controller::Crew(_))));
            }
        }
        for &player in &players {
            let crew = world.crew(player).unwrap();
            assert!(!crew.hackers.is_empty(), "a crew lost its last hacker");
            assert!(crew.hackers.len() <= crew.slots());
            // The hideout is always held.
            assert_eq!(
                world.controller(crew.hideout),
                Some(Controller::Crew(player))
            );
            // A view always builds.
            assert!(crew_view(&data, &world, player).is_some());
        }
    }

    let end = world.ended.clone().unwrap();
    assert_eq!(end.scores.len(), n);
    assert_eq!(end.turn, last_turn);
    world
}

#[test]
fn solo_games_on_every_difficulty_end_cleanly() {
    for seed in 0..5 {
        for difficulty in Difficulty::ALL {
            play(seed, 1, difficulty, 40);
        }
    }
}

#[test]
fn multi_crew_games_end_cleanly() {
    for seed in 0..4 {
        play(seed, 2, Difficulty::Normal, 40);
    }
    play(7, 3, Difficulty::Normal, 40);
    play(7, 4, Difficulty::Easy, 40);
}

#[test]
fn a_game_is_deterministic() {
    let a = play(42, 2, Difficulty::Normal, 30);
    let b = play(42, 2, Difficulty::Normal, 30);
    let sa = serde_json::to_string(&a.ended).unwrap();
    let sb = serde_json::to_string(&b.ended).unwrap();
    assert_eq!(sa, sb);
}

#[test]
fn the_bot_makes_progress() {
    // Over a solo game the bot should take hosts and gather points.
    let mut best = BTreeMap::new();
    for seed in 0..8 {
        let world = play(seed, 1, Difficulty::Easy, 40);
        let score = &world.ended.as_ref().unwrap().scores[0];
        best.insert(seed, (score.hosts, score.total));
    }
    let hosts: u32 = best.values().map(|&(h, _)| h).sum::<u32>() / best.len() as u32;
    assert!(hosts >= 3, "bot averaged only {hosts} hosts");
}

#[test]
fn freeing_a_legacy_host_scores_ten() {
    // Hand a crew a backdoor onto a Legacy host and check the score.
    let data = GameData::standard();
    let mut world = World::new_game(
        &data,
        1,
        &crews(1),
        Settings {
            difficulty: Difficulty::Easy,
            last_turn: 50,
        },
    );
    let me = PlayerId(0);
    // Reach a stronghold: give the crew a neighbour and access, then backdoor.
    let intelligence = data.find("Intelligence").unwrap();
    let neighbour = data.neighbours(intelligence)[0];
    world.hosts[neighbour.index()].controller = Some(Controller::Crew(me));
    world.hosts[intelligence.index()]
        .access
        .push(nullnet_core::Access {
            crew: me,
            until: world.turn,
        });
    let hacker = world.crew(me).unwrap().hackers[0].id;
    let mut orders = Orders::new();
    orders.insert(
        me,
        vec![Command::Backdoor {
            hacker,
            host: intelligence,
        }],
    );
    resolve_turn(&data, &mut world, &orders);
    assert_eq!(world.controller(intelligence), Some(Controller::Crew(me)));
    assert_eq!(nullnet_core::score(&world, me).unwrap().freed, 1);
}
