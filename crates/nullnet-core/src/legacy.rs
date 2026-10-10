//! The Legacy Net: the remains of the old AI. It holds Lattice and a few
//! strongholds, sweeps the loudest crew when their trace runs high, and
//! spreads slowly to the free hosts next to its own.

use crate::data::{GameData, rules};
use crate::ids::HostId;
use crate::turn::{Event, TurnReport, chance};
use crate::world::{Controller, World};

/// After the turn's operations: sweep, then spread.
pub(crate) fn run(data: &GameData, world: &mut World, report: &mut TurnReport) {
    sweep(world, report);
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

    let strength = 4 + turn / 5;
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
