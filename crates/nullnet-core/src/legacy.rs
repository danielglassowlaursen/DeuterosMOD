//! The Legacy Net: the remains of the old AI. It holds Lattice and a few
//! strongholds, sweeps the loudest crew when their trace runs high, spreads
//! slowly to the free hosts next to its own, and searches out from its hosts
//! for the ones crews took from it, to win them back.

use std::collections::BTreeSet;

use crate::data::{GameData, rules};
use crate::ids::HostId;
use crate::turn::{Event, TurnReport, chance};
use crate::world::{Controller, World};

/// After the turn's operations: sweep, reclaim, then spread. `defended` are
/// the hosts a hacker guarded this turn.
pub(crate) fn run(
    data: &GameData,
    world: &mut World,
    report: &mut TurnReport,
    defended: &BTreeSet<HostId>,
) {
    sweep(world, report);
    reclaim(data, world, report, defended);
    spread(data, world, report);
}

/// The crew with the most trace over the threshold gets a visit: the Legacy
/// Net hits their worst-defended host. If it falls, it becomes a Legacy
/// host. Either way the crew's trace drops, so it is not hit every turn.
fn sweep(world: &mut World, report: &mut TurnReport) {
    let turn = world.turn;
    let threshold = world.settings.difficulty.sweep_at();
    let target = world
        .crews
        .iter()
        .filter(|(_, c)| c.trace >= threshold)
        .max_by_key(|(p, c)| (c.trace, std::cmp::Reverse(**p)))
        .map(|(&p, _)| p);
    let Some(player) = target else {
        return;
    };

    // The crew's worst-defended host, the hideout last of all.
    let hideout = world.crew(player).map(|c| c.hideout);
    let host = world
        .held_by(player)
        .filter(|&h| Some(h) != hideout)
        .min_by_key(|&h| (world.defence(h), h.0));
    let host = match host {
        Some(h) => h,
        // Only the hideout left: strike it, but it cannot be taken.
        None => {
            if let Some(crew) = world.crews.get_mut(&player) {
                crew.trace = crew.trace.saturating_sub(rules::TRACE_AFTER_SWEEP);
            }
            return;
        }
    };

    let strength = rules::legacy_strength(turn);
    let defence = world.defence(host);
    let odds = chance(strength, defence);
    let roll = world.rng.below(100);
    let lost = roll < odds;
    if lost {
        let state = &mut world.hosts[host.index()];
        state.controller = Some(Controller::Legacy);
        state.access.clear();
        // A freed-host credit is void once the Legacy Net takes it back.
        if let Some(crew) = world.crews.get_mut(&player) {
            crew.freed.remove(&host);
        }
    }
    if let Some(crew) = world.crews.get_mut(&player) {
        crew.trace = crew.trace.saturating_sub(rules::TRACE_AFTER_SWEEP);
    }
    report.events.push(Event::Swept {
        turn,
        player,
        host,
        lost,
    });
}

/// Hosts crews took from the Legacy Net that it can see from its own: within
/// a few links of a Legacy host, by difficulty. It strikes the weakest of
/// them on its reclaim turns.
pub fn reclaim_targets(data: &GameData, world: &World) -> BTreeSet<HostId> {
    let reach = world.settings.difficulty.reclaim().0;
    let legacy = data
        .host_ids()
        .filter(|&h| world.controller(h) == Some(Controller::Legacy));
    data.within(legacy, reach)
        .into_iter()
        .filter(|&h| match world.controller(h) {
            Some(Controller::Crew(p)) => world.crew(p).is_some_and(|c| c.freed.contains(&h)),
            _ => false,
        })
        .collect()
}

/// Whether the Legacy Net reclaims this turn: from the turn it starts to
/// spread, every so many turns, halfway between its spreads.
pub fn reclaims_on(world: &World, turn: u32) -> bool {
    let first = world.settings.difficulty.spread().0;
    let every = world.settings.difficulty.reclaim().1;
    turn >= first && (turn - first) % every == every / 2
}

/// The Legacy Net strikes the weakest host it lost to a crew that it can
/// see. If it falls, it is a Legacy host again and the crew no longer counts
/// it as freed. A hacker defending the host helps hold it.
fn reclaim(
    data: &GameData,
    world: &mut World,
    report: &mut TurnReport,
    defended: &BTreeSet<HostId>,
) {
    let turn = world.turn;
    if !reclaims_on(world, turn) {
        return;
    }
    let guard = |h: &HostId| {
        if defended.contains(h) {
            rules::DEFEND_BONUS
        } else {
            0
        }
    };
    let Some(host) = reclaim_targets(data, world)
        .into_iter()
        .min_by_key(|h| (world.defence(*h) + guard(h), h.0))
    else {
        return;
    };
    let Some(Controller::Crew(player)) = world.controller(host) else {
        return;
    };
    let odds = chance(
        rules::legacy_strength(turn),
        world.defence(host) + guard(&host),
    );
    let lost = world.rng.below(100) < odds;
    if lost {
        let state = &mut world.hosts[host.index()];
        state.controller = Some(Controller::Legacy);
        state.access.clear();
        if let Some(crew) = world.crews.get_mut(&player) {
            crew.freed.remove(&host);
        }
    }
    report.events.push(Event::Reclaimed {
        turn,
        player,
        host,
        lost,
    });
}

/// From a set turn, every so many turns, the Legacy Net takes one free host
/// next to its own. It never takes a crew's host this way.
fn spread(data: &GameData, world: &mut World, report: &mut TurnReport) {
    let turn = world.turn;
    let (first, every) = world.settings.difficulty.spread();
    if turn < first || !(turn - first).is_multiple_of(every) {
        return;
    }
    let mut candidates: Vec<HostId> = data
        .host_ids()
        .filter(|&h| world.controller(h).is_none() && !world.is_hideout(h))
        .filter(|&h| {
            data.neighbours(h)
                .iter()
                .any(|&n| world.controller(n) == Some(Controller::Legacy))
        })
        .collect();
    if candidates.is_empty() {
        return;
    }
    // The one closest to the middle (highest security), ties by id.
    candidates.sort_by_key(|&h| (std::cmp::Reverse(world.host(h).security), h.0));
    let host = candidates[0];
    let state = &mut world.hosts[host.index()];
    state.controller = Some(Controller::Legacy);
    state.access.clear();
    report.events.push(Event::LegacySpread { turn, host });
}

/// Hosts the Legacy Net could spread to next, for the view.
pub fn spread_front(data: &GameData, world: &World) -> Vec<HostId> {
    data.host_ids()
        .filter(|&h| world.controller(h).is_none() && !world.is_hideout(h))
        .filter(|&h| {
            data.neighbours(h)
                .iter()
                .any(|&n| world.controller(n) == Some(Controller::Legacy))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;
    use crate::world::{Difficulty, Settings};

    fn loud_game() -> (GameData, World) {
        let data = GameData::standard();
        let world = World::new_game(
            &data,
            3,
            &[(PlayerId(0), "Loud")],
            Settings {
                difficulty: Difficulty::Hard,
                last_turn: 50,
                ..Settings::default()
            },
        );
        (data, world)
    }

    #[test]
    fn sweep_hits_the_loudest_crew_and_lowers_trace() {
        let (data, mut world) = loud_game();
        let me = PlayerId(0);
        // Give the crew a held host and high trace.
        let hideout = world.crew(me).unwrap().hideout;
        let host = data.neighbours(hideout)[0];
        world.hosts[host.index()].controller = Some(Controller::Crew(me));
        world.crews.get_mut(&me).unwrap().trace = 20;
        let mut report = TurnReport {
            turn: world.turn,
            rejected: Vec::new(),
            events: Vec::new(),
        };
        sweep(&mut world, &mut report);
        assert!(
            report
                .events
                .iter()
                .any(|e| matches!(e, Event::Swept { .. }))
        );
        assert_eq!(world.crew(me).unwrap().trace, 20 - rules::TRACE_AFTER_SWEEP);
    }

    #[test]
    fn reclaim_strikes_a_freed_host_it_can_see() {
        let (data, mut world) = loud_game();
        let me = PlayerId(0);
        // The crew has taken a stronghold from the Legacy Net.
        let stronghold = data
            .host_ids()
            .find(|&h| data.host(h).role == crate::data::Role::Stronghold)
            .unwrap();
        world.hosts[stronghold.index()].controller = Some(Controller::Crew(me));
        world.crews.get_mut(&me).unwrap().freed.insert(stronghold);
        assert!(reclaim_targets(&data, &world).contains(&stronghold));

        // A host the crew took that was never the Legacy Net's is left alone.
        let hideout = world.crew(me).unwrap().hideout;
        let near = data.neighbours(hideout)[0];
        world.hosts[near.index()].controller = Some(Controller::Crew(me));
        assert!(!reclaim_targets(&data, &world).contains(&near));

        // On a reclaim turn, late in the game, it strikes.
        world.turn = (40..60).find(|&t| reclaims_on(&world, t)).unwrap();
        let mut report = TurnReport {
            turn: world.turn,
            rejected: Vec::new(),
            events: Vec::new(),
        };
        reclaim(&data, &mut world, &mut report, &BTreeSet::new());
        let lost = match report.events.first() {
            Some(Event::Reclaimed { host, lost, .. }) => {
                assert_eq!(*host, stronghold);
                *lost
            }
            other => panic!("expected a reclaim, got {other:?}"),
        };
        assert_eq!(world.crew(me).unwrap().freed.contains(&stronghold), !lost);
        if lost {
            assert_eq!(world.controller(stronghold), Some(Controller::Legacy));
        }

        // Never on an off turn.
        world.turn += 1;
        let mut report = TurnReport {
            turn: world.turn,
            rejected: Vec::new(),
            events: Vec::new(),
        };
        reclaim(&data, &mut world, &mut report, &BTreeSet::new());
        assert!(report.events.is_empty());
    }

    #[test]
    fn spread_takes_a_free_host_next_to_legacy() {
        let (data, mut world) = loud_game();
        let legacy_count = |w: &World| {
            w.hosts
                .iter()
                .filter(|h| h.controller == Some(Controller::Legacy))
                .count()
        };
        let before = legacy_count(&world);
        // The front: free hosts next to a Legacy host, such as the inner ring.
        let front = spread_front(&data, &world);
        assert!(!front.is_empty());
        world.turn = 6; // Hard spreads from turn 6.
        let mut report = TurnReport {
            turn: world.turn,
            rejected: Vec::new(),
            events: Vec::new(),
        };
        spread(&data, &mut world, &mut report);
        // One free host on the front is now the Legacy Net's.
        assert_eq!(legacy_count(&world), before + 1);
        let taken = match report.events.first() {
            Some(Event::LegacySpread { host, .. }) => *host,
            other => panic!("expected a spread event, got {other:?}"),
        };
        assert!(front.contains(&taken));
        assert_eq!(world.controller(taken), Some(Controller::Legacy));

        // It does not spread on an off turn.
        world.turn = 7;
        let mut report = TurnReport {
            turn: world.turn,
            rejected: Vec::new(),
            events: Vec::new(),
        };
        spread(&data, &mut world, &mut report);
        assert!(report.events.is_empty());
    }
}
