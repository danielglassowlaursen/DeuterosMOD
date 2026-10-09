//! Crews against each other: raids and their three goals, the garrisons
//! that defend, heat, the Legacy Net's choice of victim, and how a game
//! ends and is scored.

use nullnet_core::{
    Berth, Cargo, Command, CommandError, Controller, EndReason, Event, GameData, HostId, ItemType,
    Module, Orders, Outcome, PROTECTION_TURNS, PlayerId, RaidGoal, Staff, StaffKind, Vein, Vessel,
    VesselId, VesselKind, VesselState, World, contested_hosts, crew_view, heat, raid, resolve_turn,
    score, scores,
};

const CREW: PlayerId = PlayerId(0);
const RIVAL: PlayerId = PlayerId(1);

fn host(data: &GameData, name: &str) -> HostId {
    HostId(data.hosts.iter().position(|h| h.name == name).unwrap() as u16)
}

fn new_game() -> (GameData, World) {
    let data = GameData::classic();
    let world = World::new_game(&data, 11, &[(CREW, "Crew"), (RIVAL, "Rival")]);
    (data, world)
}

/// Gives the crew a complete citadel on a free host.
fn claim(world: &mut World, host: HostId, crew: PlayerId) {
    let state = &mut world.hosts[usize::from(host.0)];
    state.controller = Some(Controller::Crew(crew));
    state.site.citadel.modules = 8;
}

/// Puts a worm of the crew's at a host with daemons, a C2 and an operator.
fn warship(
    world: &mut World,
    owner: PlayerId,
    host: HostId,
    berth: Berth,
    daemons: u32,
    modules: Vec<Module>,
) -> VesselId {
    let id = VesselId(world.next_vessel);
    world.next_vessel += 1;
    world.vessels.insert(
        id,
        Vessel {
            owner,
            kind: VesselKind::Worm,
            host,
            state: VesselState::At(berth),
            fuel: 200,
            pilot: Some(Staff::new("Runner", StaffKind::Operator, 20)),
            modules,
            destination: None,
            exposed_days: 0,
            script: None,
            daemons,
            c2: true,
            cache: None,
        },
    );
    id
}

fn days(data: &GameData, world: &mut World, n: u32) -> Vec<Event> {
    resolve_turn(data, world, &Orders::new(), n).events
}

/// Runs the protection period out with empty turns.
fn protection_over(data: &GameData, world: &mut World) {
    while world.turn < PROTECTION_TURNS {
        days(data, world, 0);
    }
}

fn order(data: &GameData, world: &mut World, crew: PlayerId, command: Command) -> Vec<Event> {
    let report = resolve_turn(data, world, &Orders::from([(crew, vec![command])]), 0);
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    report.events
}

fn refused(data: &GameData, world: &mut World, crew: PlayerId, command: Command) -> CommandError {
    let report = resolve_turn(data, world, &Orders::from([(crew, vec![command])]), 0);
    assert_eq!(report.rejected.len(), 1, "{:?}", report.rejected);
    report.rejected[0].error.clone()
}

fn raid_event(events: &[Event]) -> Option<&Event> {
    events.iter().find(|e| matches!(e, Event::Raid { .. }))
}

#[test]
fn raids_wait_for_the_protection_to_end_and_need_a_rival_host() {
    let (data, mut world) = new_game();
    let transit = host(&data, "Transit");
    let beacon = host(&data, "Beacon");
    claim(&mut world, transit, RIVAL);
    claim(&mut world, beacon, CREW);
    let worm = warship(
        &mut world,
        CREW,
        transit,
        Berth::Lurking,
        50,
        vec![Module::Empty; 3],
    );
    let raid = Command::Raid {
        vessel: worm,
        goal: RaidGoal::Exfiltrate,
    };
    assert_eq!(
        refused(&data, &mut world, CREW, raid.clone()),
        CommandError::Protected
    );
    protection_over(&data, &mut world);

    // Not a rival's host: own, free, or the Legacy Net's.
    for at in [beacon, host(&data, "Switchboard"), host(&data, "Colossus")] {
        world.vessels.get_mut(&worm).unwrap().host = at;
        assert_eq!(
            refused(&data, &mut world, CREW, raid.clone()),
            CommandError::NothingToRaid,
            "{}",
            data.host(at).name
        );
    }
    world.vessels.get_mut(&worm).unwrap().host = transit;
    let events = order(&data, &mut world, CREW, raid);
    assert!(raid_event(&events).is_some());
}

#[test]
fn an_exfiltration_fills_the_containers_with_the_biggest_stocks() {
    let (data, mut world) = new_game();
    let transit = host(&data, "Transit");
    claim(&mut world, transit, RIVAL);
    {
        let site = &mut world.hosts[usize::from(transit.0)].site;
        site.citadel.store.add(ItemType::Compute, 300);
        site.citadel.store.add(ItemType::Code, 100);
        site.citadel.store.add(ItemType::Tap, 5);
        site.store.add(ItemType::Memory, 50);
    }
    protection_over(&data, &mut world);
    let worm = warship(
        &mut world,
        CREW,
        transit,
        Berth::Lurking,
        50,
        vec![
            Module::DataContainer(None),
            Module::DataContainer(None),
            Module::ToolModule(None),
        ],
    );
    let events = order(
        &data,
        &mut world,
        CREW,
        Command::Raid {
            vessel: worm,
            goal: RaidGoal::Exfiltrate,
        },
    );
    let Some(Event::Raid {
        player,
        defender,
        goal,
        report,
        loot,
        ..
    }) = raid_event(&events)
    else {
        panic!("no raid reported");
    };
    assert_eq!(
        (*player, *defender, *goal),
        (CREW, RIVAL, RaidGoal::Exfiltrate)
    );
    assert_eq!(report.outcome, Outcome::AttackerWon, "nobody defended");
    assert_eq!(loot, &[(ItemType::Compute, 250), (ItemType::Code, 100)]);
    let vessel = &world.vessels[&worm];
    assert_eq!(
        vessel.modules[0],
        Module::DataContainer(Some(Cargo {
            item: ItemType::Compute,
            count: 250
        }))
    );
    let site = &world.hosts[usize::from(transit.0)].site;
    assert_eq!(site.citadel.store.get(ItemType::Compute), 50);
    assert_eq!(site.citadel.store.get(ItemType::Code), 0);
    assert_eq!(site.citadel.store.get(ItemType::Tap), 5, "equipment stays");
    assert_eq!(site.store.get(ItemType::Memory), 50);
    assert_eq!(heat(&world, CREW), raid::RAID_HEAT);
    assert_eq!(heat(&world, RIVAL), raid::CITADEL_HEAT);

    // Both crews see the raid; nobody else does.
    let report = resolve_turn(&data, &mut world, &Orders::new(), 0);
    assert!(report.events.is_empty());
    let turn = nullnet_core::TurnReport {
        first_day: 0,
        last_day: 0,
        rejected: Vec::new(),
        events,
    };
    assert_eq!(turn.for_crew(CREW).events.len(), 1);
    assert_eq!(turn.for_crew(RIVAL).events.len(), 1);
    assert_eq!(turn.for_crew(PlayerId(2)).events.len(), 0);
}

#[test]
fn a_planted_tap_siphons_a_share_of_the_extraction_home() {
    let (data, mut world) = new_game();
    let transit = host(&data, "Transit");
    claim(&mut world, transit, RIVAL);
    {
        let site = &mut world.hosts[usize::from(transit.0)].site;
        site.backdoor_parts = 2;
        site.taps = 4;
        site.veins = vec![Vein {
            resource: ItemType::Code,
            amount: 10_000,
            survey_days: 0,
        }];
    }
    protection_over(&data, &mut world);
    let worm = warship(
        &mut world,
        CREW,
        transit,
        Berth::Lurking,
        50,
        vec![Module::Empty; 3],
    );
    // The hideout mines code of its own; switch its taps off to count only
    // what the planted tap sends home.
    world.players.get_mut(&CREW).unwrap().hideout.taps = 0;
    let before = world.players[&CREW].hideout.store.get(ItemType::Code);
    order(
        &data,
        &mut world,
        CREW,
        Command::Raid {
            vessel: worm,
            goal: RaidGoal::PlantTap,
        },
    );
    let until = world.day + raid::SIPHON_DAYS;
    assert_eq!(
        world.hosts[usize::from(transit.0)].site.siphons,
        vec![nullnet_core::Siphon {
            player: CREW,
            until
        }]
    );
    let view = crew_view(&data, &world, CREW).unwrap();
    assert_eq!(view.taps_planted, vec![(transit, until)]);

    // Four taps extract 8 code a day; a quarter of it goes home.
    days(&data, &mut world, 10);
    let site = &world.hosts[usize::from(transit.0)].site;
    assert_eq!(site.store.get(ItemType::Code), 60);
    assert_eq!(
        world.players[&CREW].hideout.store.get(ItemType::Code) - before,
        20
    );

    days(&data, &mut world, raid::SIPHON_DAYS);
    assert!(
        world.hosts[usize::from(transit.0)].site.siphons.is_empty(),
        "the tap runs its course"
    );
}

#[test]
fn a_takeover_moves_the_host_and_throws_the_loser_out() {
    let (data, mut world) = new_game();
    let transit = host(&data, "Transit");
    claim(&mut world, transit, RIVAL);
    {
        let site = &mut world.hosts[usize::from(transit.0)].site;
        site.citadel.store.add(ItemType::Daemon, 20);
        site.citadel.store.add(ItemType::Storage, 400);
        site.citadel
            .staff
            .push(Staff::new("Coder", StaffKind::Coder, 10));
        site.staff
            .push(Staff::new("Runner", StaffKind::Operator, 10));
        site.taps = 3;
    }
    let rival_worm = warship(
        &mut world,
        RIVAL,
        transit,
        Berth::Connected,
        0,
        vec![Module::Empty; 3],
    );
    protection_over(&data, &mut world);
    let worm = warship(
        &mut world,
        CREW,
        transit,
        Berth::Lurking,
        200,
        vec![Module::Empty; 3],
    );
    let events = order(
        &data,
        &mut world,
        CREW,
        Command::Raid {
            vessel: worm,
            goal: RaidGoal::TakeOver,
        },
    );
    let Some(Event::Raid { report, .. }) = raid_event(&events) else {
        panic!("no raid reported");
    };
    assert_eq!(report.outcome, Outcome::AttackerWon);
    assert_eq!(report.defender.daemons, 20);
    assert!(events.iter().any(
        |e| matches!(e, Event::HostTaken { player, from, host, .. } if *player == CREW && *from == RIVAL && *host == transit)
    ));
    let state = &world.hosts[usize::from(transit.0)];
    assert_eq!(state.controller, Some(Controller::Crew(CREW)));
    assert_eq!(state.site.taps, 3, "the host comes with its taps");
    assert_eq!(state.site.citadel.store.get(ItemType::Storage), 400);
    assert_eq!(state.site.citadel.store.get(ItemType::Daemon), 0);
    assert!(state.site.staff.is_empty() && state.site.citadel.staff.is_empty());
    assert_eq!(
        world.vessels[&rival_worm].state,
        VesselState::At(Berth::Lurking),
        "the rival's worm is thrown out of the citadel"
    );
    assert_eq!(world.players[&CREW].taken, 1);
    assert_eq!(score(&data, &world, CREW).unwrap().taken, 1);
    let turn = nullnet_core::TurnReport {
        first_day: 0,
        last_day: 0,
        rejected: Vec::new(),
        events,
    };
    assert_eq!(turn.for_crew(RIVAL).events.len(), 2, "raid and loss");
    assert_eq!(
        turn.for_crew(PlayerId(2)).events.len(),
        1,
        "the loss is public"
    );
}

#[test]
fn a_garrison_under_a_good_operator_repels_a_raid_and_the_vessel_is_lost() {
    let (data, mut world) = new_game();
    let transit = host(&data, "Transit");
    claim(&mut world, transit, RIVAL);
    let mut operators = Staff::new("Ghost", StaffKind::Operator, 10);
    for _ in 0..40 {
        operators.record_action();
    }
    assert_eq!(operators.level(), 3);
    {
        let site = &mut world.hosts[usize::from(transit.0)].site;
        site.citadel.store.add(ItemType::Daemon, 250);
        site.citadel.staff.push(operators);
    }
    protection_over(&data, &mut world);
    let worm = warship(
        &mut world,
        CREW,
        transit,
        Berth::Lurking,
        10,
        vec![Module::Empty; 3],
    );
    let events = order(
        &data,
        &mut world,
        CREW,
        Command::Raid {
            vessel: worm,
            goal: RaidGoal::TakeOver,
        },
    );
    let Some(Event::Raid { report, .. }) = raid_event(&events) else {
        panic!("no raid reported");
    };
    assert_eq!(report.outcome, Outcome::DefenderWon);
    assert_eq!(report.defender.daemons, 200, "at most 200 fight at once");
    assert_eq!(report.defender.level, 3);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::VesselLost { vessel, .. } if *vessel == worm))
    );
    assert!(!world.vessels.contains_key(&worm));
    let state = &world.hosts[usize::from(transit.0)];
    assert_eq!(state.controller, Some(Controller::Crew(RIVAL)));
    let left = state.site.citadel.store.get(ItemType::Daemon);
    assert!(left > 200 && left <= 250, "{left} daemons left");
    assert_eq!(heat(&world, CREW), raid::RAID_HEAT, "a lost raid heats too");
}

#[test]
fn the_daemons_stored_in_a_citadel_defend_it_against_a_swarm() {
    let (data, mut world) = new_game();
    let transit = host(&data, "Transit");
    claim(&mut world, transit, CREW);
    world.players.get_mut(&CREW).unwrap().war = Some(0);
    world.hosts[usize::from(transit.0)]
        .site
        .citadel
        .store
        .add(ItemType::Daemon, 200);
    let fleet = &mut world.legacy.fleets[0];
    fleet.daemons = 40;
    fleet.target = Some(transit);
    fleet.arrives = Some(world.day + 1);

    let events = days(&data, &mut world, 2);
    let battle = events
        .iter()
        .find_map(|e| match e {
            Event::BattleFought { vessel, report, .. } => Some((*vessel, report.clone())),
            _ => None,
        })
        .expect("the garrison fought");
    assert_eq!(battle.0, None, "no vessel: the citadel's own daemons");
    assert_eq!(battle.1.attacker.daemons, 200);
    assert_ne!(battle.1.outcome, Outcome::DefenderWon);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::AttackRepelled { host, .. } if *host == transit))
    );
    let state = &world.hosts[usize::from(transit.0)];
    assert_eq!(state.controller, Some(Controller::Crew(CREW)));
    let left = state.site.citadel.store.get(ItemType::Daemon);
    assert!(left > 100 && left < 200, "{left} daemons left");
}

#[test]
fn swarms_go_for_the_hottest_crew_at_war() {
    let (data, mut world) = new_game();
    let beacon = host(&data, "Beacon");
    let switchboard = host(&data, "Switchboard");
    claim(&mut world, beacon, CREW);
    claim(&mut world, switchboard, RIVAL);
    for crew in [CREW, RIVAL] {
        world.players.get_mut(&crew).unwrap().war = Some(0);
    }
    world.players.get_mut(&RIVAL).unwrap().heat = 100;
    let fleet = &mut world.legacy.fleets[0];
    fleet.daemons = 60;
    fleet.attack_trigger = 40;

    let mut target = None;
    for _ in 0..20 {
        let events = days(&data, &mut world, 1);
        target = events.iter().find_map(|e| match e {
            Event::FleetSighted { host, player, .. } => Some((*host, *player)),
            _ => None,
        });
        if target.is_some() {
            break;
        }
    }
    assert_eq!(
        target,
        Some((switchboard, RIVAL)),
        "the hotter crew's citadel"
    );
}

#[test]
fn the_game_ends_on_its_last_day_and_the_top_score_wins() {
    let (data, mut world) = new_game();
    claim(&mut world, host(&data, "Transit"), CREW);
    claim(&mut world, host(&data, "Beacon"), CREW);
    world.hosts[usize::from(host(&data, "Switchboard").0)].controller =
        Some(Controller::Crew(RIVAL));
    world.end_day = Some(10);

    let events = days(&data, &mut world, 5);
    assert!(!events.iter().any(|e| matches!(e, Event::GameOver { .. })));
    assert!(world.ended.is_none());
    let events = days(&data, &mut world, 5);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::GameOver { player, reason: EndReason::DayLimit, .. } if *player == CREW
    )));
    let end = world.ended.clone().expect("over");
    assert_eq!((end.day, end.winner), (10, CREW));
    assert_eq!(end.scores[0].player, CREW);
    assert_eq!(end.scores[0].citadels, 2);
    assert_eq!(end.scores[1].hosts, 1);
    let researched = world.players[&CREW]
        .research
        .values()
        .filter(|r| r.researched)
        .count() as u32;
    assert_eq!(end.scores[0].total, 20 + researched * 2);
    assert_eq!(scores(&data, &world), end.scores);

    // Nothing moves after the end.
    let frozen = world.clone();
    let report = resolve_turn(&data, &mut world, &Orders::new(), 10);
    assert!(report.events.is_empty());
    assert_eq!(world, frozen);
    let view = crew_view(&data, &world, RIVAL).unwrap();
    assert_eq!(view.ended, Some(end));
}

#[test]
fn holding_most_of_the_home_network_wins_at_once() {
    let (data, mut world) = new_game();
    let contested = contested_hosts(&data);
    assert_eq!(contested, 40);
    let home = data.host(data.hideout.host).network;
    let free: Vec<HostId> = (0..data.hosts.len())
        .filter(|&h| {
            data.hosts[h].network == home
                && world.hosts[h].controller.is_none()
                && !data.hosts[h].cache_field
                && HostId(h as u16) != data.hideout.host
        })
        .map(|h| HostId(h as u16))
        .collect();
    for &h in free.iter().take(20) {
        claim(&mut world, h, CREW);
    }
    days(&data, &mut world, 1);
    assert!(world.ended.is_none(), "half is not enough");
    claim(&mut world, free[20], CREW);
    let events = days(&data, &mut world, 1);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::GameOver { player, reason: EndReason::Domination, .. } if *player == CREW
    )));
    assert_eq!(world.ended.as_ref().map(|e| e.winner), Some(CREW));
}
