//! A greedy policy that gives a crew a decent turn's orders, for practice
//! games and for balancing. It is deterministic: the same world always gives
//! the same orders, so a bot game replays the same way.

use std::collections::BTreeSet;

use crate::command::{Command, Operation};
use crate::data::{GameData, Upgrade, Weakness, rules};
use crate::ids::{HackerId, HostId, PlayerId};
use crate::turn::{attack, chance, defence};
use crate::world::{Controller, World};

/// Orders for one crew this turn.
pub fn orders(data: &GameData, world: &World, player: PlayerId) -> Vec<Command> {
    let Some(crew) = world.crew(player) else {
        return Vec::new();
    };
    let turn = world.turn;
    let threshold = world.settings.difficulty.sweep_at();
    let mut out = Vec::new();

    // Spend some credits before the turn. Hired hackers wait until next turn.
    let mut credits = crew.credits;
    let mut compute = crew.compute;

    // A kit for a weakness we keep running into near our front.
    let wanted = front_weaknesses(data, world, player);
    for weakness in wanted {
        if credits < rules::KIT_PRICE + 20 {
            break;
        }
        if !crew.kits.contains(&weakness) {
            out.push(Command::BuyKit { weakness });
            credits -= rules::KIT_PRICE;
        }
    }

    // Room and money for another hacker? Take the best-value offer.
    if crew.hackers.len() < crew.slots()
        && let Some(offer) = crew
            .market
            .iter()
            .filter(|o| o.price + 15 <= credits)
            .max_by_key(|o| (o.hacker.level, std::cmp::Reverse(o.price)))
    {
        out.push(Command::Hire {
            hacker: offer.hacker.id,
        });
        credits -= offer.price;
    }

    // Upgrades when flush: more hacker room, then bandwidth, then rigs.
    for upgrade in [
        Upgrade::Safehouse,
        Upgrade::Lines,
        Upgrade::Rigs,
        Upgrade::Firewall,
    ] {
        let level = crew.upgrades.level(upgrade);
        if level >= upgrade.max_level() {
            continue;
        }
        let (c, p) = upgrade.cost(level);
        if credits >= c + 60 && compute >= p {
            out.push(Command::Upgrade { upgrade });
            credits -= c;
            compute -= p;
        }
    }

    // Plan operations within the bandwidth budget, one per ready hacker.
    let mut bandwidth = world.bandwidth(data, player);
    let mut busy: BTreeSet<HackerId> = BTreeSet::new();
    let ready: Vec<HackerId> = crew
        .hackers
        .iter()
        .filter(|h| h.ready(turn))
        .map(|h| h.id)
        .collect();

    // 0. Open the sealed sub-net behind any host we hold, when two ready
    //    hackers fit its lock: steady credits for the rest of the game.
    for host in world.held_by(player).collect::<Vec<_>>() {
        let Some(subnet) = world.host(host).subnet.filter(|s| !s.open) else {
            continue;
        };
        if bandwidth < Operation::OpenSubnet.bandwidth() {
            break;
        }
        let free = |want: Weakness, skip: Option<HackerId>| {
            ready.iter().copied().find(|&id| {
                !busy.contains(&id)
                    && Some(id) != skip
                    && crew.hacker(id).is_some_and(|h| h.specialty == want)
            })
        };
        if let Some(first) = free(subnet.lock[0], None)
            && let Some(second) = free(subnet.lock[1], Some(first))
        {
            out.push(Command::OpenSubnet {
                host,
                hackers: [first, second],
            });
            busy.insert(first);
            busy.insert(second);
            bandwidth -= Operation::OpenSubnet.bandwidth();
        }
    }

    // 1. Backdoor every host we have access to but do not own: that is how
    //    we take hosts, and access lasts only one turn.
    for host in world.held_access(player) {
        if world.controller(host) == Some(Controller::Crew(player)) {
            continue;
        }
        if bandwidth < Operation::Backdoor.bandwidth() {
            break;
        }
        if let Some(hacker) = pick_hacker(world, player, &ready, &busy, None) {
            out.push(Command::Backdoor { hacker, host });
            busy.insert(hacker);
            bandwidth -= Operation::Backdoor.bandwidth();
        }
    }

    // 2. If our trace is getting high, guard our best host instead of making
    //    more noise.
    let loud = crew.trace + 2 >= threshold;
    if loud
        && bandwidth >= Operation::Defend.bandwidth()
        && let Some(host) = world
            .held_by(player)
            .filter(|&h| h != crew.hideout)
            .min_by_key(|&h| (world.defence(h), h.0))
        && let Some(hacker) = pick_hacker(world, player, &ready, &busy, None)
    {
        out.push(Command::Defend { hacker, host });
        busy.insert(hacker);
        bandwidth -= Operation::Defend.bandwidth();
    }

    // 3. Break into the reachable, scanned hosts with the best odds, unless
    //    our trace is already high.
    if !loud {
        let mut targets: Vec<(HostId, HackerId, u32, bool, u32)> = Vec::new();
        for host in data.host_ids() {
            if !world.can_break_in(data, player, host) || !world.knows(player, host) {
                continue;
            }
            // Skip hosts we already have access to (we will backdoor them).
            if world.host(host).has_access(player, turn) {
                continue;
            }
            let zero_day = crew.zero_days > 0 && world.defence(host) >= 8;
            let boost = compute.min(rules::MAX_BOOST);
            if let Some((hacker, odds)) =
                best_hacker(world, player, host, &ready, &busy, zero_day, boost)
            {
                targets.push((host, hacker, odds, zero_day, boost));
            }
        }
        // Best odds first; a free host is worth more than a contested one.
        targets.sort_by_key(|&(host, _, odds, _, _)| {
            let free = world.controller(host).is_none();
            (std::cmp::Reverse(odds), std::cmp::Reverse(free), host.0)
        });
        for (host, hacker, odds, zero_day, boost) in targets {
            if bandwidth < Operation::BreakIn.bandwidth() {
                break;
            }
            if busy.contains(&hacker) || odds < 40 {
                continue;
            }
            let boost = boost.min(compute);
            out.push(Command::BreakIn {
                hacker,
                host,
                zero_day,
                boost,
            });
            busy.insert(hacker);
            compute -= boost;
            bandwidth -= Operation::BreakIn.bandwidth();
        }
    }

    // 4. Steal data from any rival or Legacy host we still have access to.
    for host in world.held_access(player) {
        if bandwidth < Operation::StealData.bandwidth() {
            break;
        }
        if world.controller(host) == Some(Controller::Crew(player)) {
            continue;
        }
        if let Some(hacker) = pick_hacker(world, player, &ready, &busy, None) {
            out.push(Command::StealData { hacker, host });
            busy.insert(hacker);
            bandwidth -= Operation::StealData.bandwidth();
        }
    }

    // 5. Spend any spare hacker and bandwidth scanning the way ahead.
    let scan_range = world.scan_range(data, player);
    let mut to_scan: Vec<HostId> = scan_range
        .into_iter()
        .filter(|&h| !world.knows(player, h))
        .collect();
    // Scan the hosts nearest our front first (lowest id is a rough stand-in).
    to_scan.sort_by_key(|h| h.0);
    for host in to_scan {
        if bandwidth < Operation::Scan.bandwidth() {
            break;
        }
        if let Some(hacker) = pick_hacker(world, player, &ready, &busy, None) {
            out.push(Command::Scan { hacker, host });
            busy.insert(hacker);
            bandwidth -= Operation::Scan.bandwidth();
        } else {
            break;
        }
    }

    out
}

/// The first unused ready hacker, preferring one whose specialty matches.
fn pick_hacker(
    world: &World,
    player: PlayerId,
    ready: &[HackerId],
    busy: &BTreeSet<HackerId>,
    want: Option<Weakness>,
) -> Option<HackerId> {
    let crew = world.crew(player)?;
    let mut best: Option<(HackerId, i32)> = None;
    for &id in ready {
        if busy.contains(&id) {
            continue;
        }
        let hacker = crew.hacker(id)?;
        let fit = match want {
            Some(w) if hacker.specialty == w => 10,
            Some(_) => 0,
            None => 0,
        };
        let score = fit + i32::from(hacker.level);
        if best.is_none_or(|(_, b)| score > b) {
            best = Some((id, score));
        }
    }
    best.map(|(id, _)| id)
}

/// The ready hacker with the best odds against a host, and those odds.
fn best_hacker(
    world: &World,
    player: PlayerId,
    host: HostId,
    ready: &[HackerId],
    busy: &BTreeSet<HackerId>,
    zero_day: bool,
    boost: u32,
) -> Option<(HackerId, u32)> {
    let crew = world.crew(player)?;
    let defence = defence(world, host, false);
    let mut best: Option<(HackerId, u32)> = None;
    for &id in ready {
        if busy.contains(&id) {
            continue;
        }
        let hacker = crew.hacker(id)?;
        let odds = chance(
            attack(world, player, hacker, host, zero_day, boost),
            defence,
        );
        if best.is_none_or(|(_, b)| odds > b) {
            best = Some((id, odds));
        }
    }
    best
}

/// The weaknesses of the scanned hosts just outside our border: the kits
/// worth buying.
fn front_weaknesses(data: &GameData, world: &World, player: PlayerId) -> Vec<Weakness> {
    let mut seen = Vec::new();
    for host in data.host_ids() {
        if world.can_break_in(data, player, host) && world.knows(player, host) {
            let weakness = world.host(host).weakness;
            if !seen.contains(&weakness) {
                seen.push(weakness);
            }
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn::{Orders, resolve_turn};
    use crate::world::Settings;

    #[test]
    fn a_bot_plays_a_sensible_opening() {
        let data = GameData::standard();
        let mut world = World::new_game(&data, 9, &[(PlayerId(0), "Bot")], Settings::default());
        let me = PlayerId(0);
        let turn = orders(&data, &world, me);
        // It should not go over bandwidth or repeat a hacker.
        let used: u32 = turn.iter().map(Command::bandwidth).sum();
        assert!(used <= world.bandwidth(&data, me));
        let mut hackers = BTreeSet::new();
        for command in &turn {
            for hacker in command.hackers() {
                assert!(hackers.insert(hacker), "a hacker acted twice");
            }
        }
        // Nothing it issues is rejected.
        let mut all = Orders::new();
        all.insert(me, turn);
        let report = resolve_turn(&data, &mut world, &all);
        assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    }

    #[test]
    fn a_bot_takes_hosts_over_a_game() {
        let data = GameData::standard();
        let mut world = World::new_game(
            &data,
            4,
            &[(PlayerId(0), "Bot")],
            Settings {
                last_turn: 40,
                ..Settings::default()
            },
        );
        let me = PlayerId(0);
        while world.ended.is_none() {
            let mut all = Orders::new();
            all.insert(me, orders(&data, &world, me));
            resolve_turn(&data, &mut world, &all);
        }
        let end = world.ended.unwrap();
        let score = &end.scores[0];
        // By the end it should hold a few hosts and have some data.
        assert!(score.hosts >= 2, "only {} hosts", score.hosts);
        assert!(score.total > 0);
    }
}
