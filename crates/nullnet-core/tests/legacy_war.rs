//! The Legacy Net at war: swarms, sieges, captures, defence and liberation,
//! and the cache fields crews mine for what the home host lacks.

use nullnet_core::{
    Berth, Cargo, Command, CommandError, Controller, Event, GameData, HostId, ItemType, Milestone,
    Module, Orders, Outcome, PlayerId, Staff, StaffKind, Vessel, VesselId, VesselKind, VesselState,
    World, legacy, resolve_turn,
};

const CREW: PlayerId = PlayerId(0);
const RIVAL: PlayerId = PlayerId(1);

fn host(data: &GameData, name: &str) -> HostId {
    HostId(data.hosts.iter().position(|h| h.name == name).unwrap() as u16)
}

fn new_game() -> (GameData, World) {
    let data = GameData::classic();
    let world = World::new_game(&data, 5, &[(CREW, "Crew"), (RIVAL, "Rival")]);
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
    c2: bool,
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
            modules: vec![Module::Empty; 3],
            destination: None,
            exposed_days: 0,
            script: None,
            daemons,
            c2,
            cache: None,
        },
    );
    id
}

fn days(data: &GameData, world: &mut World, n: u32) -> Vec<Event> {
    resolve_turn(data, world, &Orders::new(), n).events
}

fn order(data: &GameData, world: &mut World, crew: PlayerId, command: Command) -> Vec<Event> {
    let report = resolve_turn(data, world, &Orders::from([(crew, vec![command])]), 0);
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
    report.events
}

#[test]
fn legacy_hosts_start_garrisoned_and_every_network_has_a_swarm() {
    let (data, world) = new_game();
    let colossus = host(&data, "Colossus");
    assert_eq!(
        world.hosts[usize::from(colossus.0)]
            .site
            .citadel
            .store
            .get(ItemType::Daemon),
        legacy::GARRISON
    );
    assert_eq!(world.legacy.fleets.len(), data.networks.len());
    assert_eq!(world.legacy.fleets[0].host, Some(colossus));
    assert_eq!(world.legacy.fleets[0].attack_trigger, 40);
    assert!(!legacy::at_war(&world));
    // Nothing stirs before a war.
    let mut world = world;
    days(&data, &mut world, 60);
    assert_eq!(world.legacy.fleets[0].daemons, 0);
}

#[test]
fn six_citadels_bring_war_and_open_daemons_to_research() {
    let (data, mut world) = new_game();
    world
        .players
        .get_mut(&CREW)
        .unwrap()
        .hideout
        .citadel
        .modules = 8;
    for name in ["Beacon", "Switchboard", "Transit", "Mirror"] {
        claim(&mut world, host(&data, name), CREW);
    }
    let events = days(&data, &mut world, 1);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::WarDeclared { .. })),
        "five citadels keep the peace"
    );
    claim(&mut world, host(&data, "Dispatch"), CREW);
    let events = days(&data, &mut world, 1);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::WarDeclared { player, .. } if *player == CREW))
    );
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Unlocked { player, milestone: Milestone::Daemons, .. } if *player == CREW
    )));
    assert!(world.players[&CREW].war.is_some());
    assert!(world.players[&RIVAL].war.is_none(), "war is per crew");
    assert!(
        world.players[&CREW]
            .research
            .contains_key(&ItemType::Daemon)
    );
}

/// A world where the crew is at war and the home swarm is ready to go.
fn at_war() -> (GameData, World, HostId) {
    let (data, mut world) = new_game();
    let transit = host(&data, "Transit");
    claim(&mut world, transit, CREW);
    world.players.get_mut(&CREW).unwrap().war = Some(0);
    world.legacy.fleets[0].daemons = 60;
    (data, world, transit)
}

#[test]
fn a_swarm_sets_out_besieges_and_takes_an_undefended_citadel() {
    let (data, mut world, transit) = at_war();
    let mut events = Vec::new();
    let mut day = 0;
    while world.legacy.fleets[0].target.is_none() && day < 20 {
        events.extend(days(&data, &mut world, 1));
        day += 1;
    }
    let sighted = events.iter().find_map(|e| match e {
        Event::FleetSighted { host, arrives, .. } => Some((*host, *arrives)),
        _ => None,
    });
    let (target, arrives) = sighted.expect("the swarm picked a target");
    assert_eq!(target, transit, "the crew's only citadel at war");
    assert!(arrives > world.day);

    let remaining = arrives - world.day;
    let events = days(&data, &mut world, remaining);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::UnderAttack { host, captured_on, .. } if *host == transit && *captured_on == arrives + legacy::SIEGE_DAYS
    )));
    assert!(legacy::under_siege(&world, transit));

    // A vessel cannot connect to a citadel under siege.
    let worm = warship(&mut world, CREW, transit, Berth::Lurking, 0, false);
    let events = order(
        &data,
        &mut world,
        CREW,
        Command::Dispatch {
            vessel: worm,
            to: nullnet_core::Destination {
                host: transit,
                berth: Berth::Connected,
            },
        },
    );
    assert!(events.iter().any(|e| matches!(
        e,
        Event::VesselStopped {
            reason: nullnet_core::AbortReason::UnderAttack,
            ..
        }
    )));

    let events = days(&data, &mut world, legacy::SIEGE_DAYS);
    assert!(events.iter().any(|e| matches!(e, Event::HostCaptured { host, player, .. } if *host == transit && *player == CREW)));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::VesselLost { vessel, .. } if *vessel == worm))
    );
    let state = &world.hosts[usize::from(transit.0)];
    assert_eq!(state.controller, Some(Controller::Legacy));
    assert!(state.site.citadel.encrypted_link && state.site.citadel.kill_switch);
    assert!(state.site.citadel.workshop.automated);
    assert_eq!(
        state.site.citadel.store.get(ItemType::Daemon),
        legacy::GARRISON
    );
    assert!(state.site.citadel.store.get(ItemType::Certificates) >= 100);
    assert!(!world.vessels.contains_key(&worm));
    assert_eq!(world.legacy.fleets[0].host, Some(transit));
}

#[test]
fn a_defender_with_daemons_drives_the_swarm_off() {
    let (data, mut world, transit) = at_war();
    let worm = warship(&mut world, CREW, transit, Berth::Connected, 200, true);
    let mut events = Vec::new();
    for _ in 0..120 {
        events.extend(days(&data, &mut world, 1));
        if events
            .iter()
            .any(|e| matches!(e, Event::AttackRepelled { .. }))
        {
            break;
        }
    }
    let battle = events
        .iter()
        .find_map(|e| match e {
            Event::BattleFought { report, vessel, .. } => Some((report.clone(), *vessel)),
            _ => None,
        })
        .expect("a battle was fought");
    assert_eq!(battle.1, worm);
    assert_eq!(battle.0.outcome, Outcome::DefenderFled);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::AttackRepelled { host, .. } if *host == transit))
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::HostCaptured { .. }))
    );
    assert_eq!(
        world.hosts[usize::from(transit.0)].controller,
        Some(Controller::Crew(CREW))
    );
    assert_eq!(
        world.legacy.fleets[0].attack_trigger, 80,
        "the swarm grows warier"
    );
    assert!(
        world.vessels[&worm].daemons < 200,
        "the defence cost daemons"
    );
    assert!(world.vessels[&worm].daemons > 100);
}

#[test]
fn attacking_a_garrison_frees_the_host_and_declares_war() {
    let (data, mut world) = new_game();
    let colossus = host(&data, "Colossus");
    let worm = warship(&mut world, CREW, colossus, Berth::Lurking, 200, true);
    assert!(world.players[&CREW].war.is_none());

    let events = order(&data, &mut world, CREW, Command::Attack { vessel: worm });
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::WarDeclared { player, .. } if *player == CREW))
    );
    let report = events
        .iter()
        .find_map(|e| match e {
            Event::BattleFought { report, .. } => Some(report),
            _ => None,
        })
        .unwrap();
    assert_eq!(report.defender.daemons, legacy::GARRISON);
    assert_eq!(report.outcome, Outcome::AttackerWon);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::HostFreed { host, .. } if *host == colossus))
    );
    let state = &world.hosts[usize::from(colossus.0)];
    assert_eq!(state.controller, Some(Controller::Crew(CREW)));
    assert!(
        state.site.citadel.encrypted_link,
        "the Legacy Net's gear stays"
    );
    assert_eq!(state.site.citadel.store.get(ItemType::Daemon), 0);
    assert!(
        world.players[&CREW]
            .milestones
            .contains(&Milestone::EncryptedLinks)
    );
    // The home swarm regroups at another Legacy host.
    assert_ne!(world.legacy.fleets[0].host, Some(colossus));
    assert!(world.legacy.fleets[0].host.is_some());

    // Attacking needs daemons under a C2 at a Legacy host.
    let empty = warship(
        &mut world,
        CREW,
        host(&data, "Clinic"),
        Berth::Lurking,
        0,
        true,
    );
    let report = resolve_turn(
        &data,
        &mut world,
        &Orders::from([(CREW, vec![Command::Attack { vessel: empty }])]),
        0,
    );
    assert_eq!(report.rejected[0].error, CommandError::NoDaemons);
    let unarmed = warship(
        &mut world,
        CREW,
        host(&data, "Clinic"),
        Berth::Lurking,
        10,
        false,
    );
    let report = resolve_turn(
        &data,
        &mut world,
        &Orders::from([(CREW, vec![Command::Attack { vessel: unarmed }])]),
        0,
    );
    assert_eq!(report.rejected[0].error, CommandError::NoC2);
    let peaceful = warship(
        &mut world,
        CREW,
        host(&data, "Beacon"),
        Berth::Lurking,
        10,
        true,
    );
    let report = resolve_turn(
        &data,
        &mut world,
        &Orders::from([(CREW, vec![Command::Attack { vessel: peaceful }])]),
        0,
    );
    assert_eq!(report.rejected[0].error, CommandError::NothingToAttack);
}

#[test]
fn a_weak_attack_loses_the_vessel_and_leaves_the_garrison() {
    let (data, mut world) = new_game();
    let colossus = host(&data, "Colossus");
    let worm = warship(&mut world, CREW, colossus, Berth::Lurking, 5, true);
    let events = order(&data, &mut world, CREW, Command::Attack { vessel: worm });
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::VesselLost { vessel, .. } if *vessel == worm))
    );
    assert!(!world.vessels.contains_key(&worm));
    let state = &world.hosts[usize::from(colossus.0)];
    assert_eq!(state.controller, Some(Controller::Legacy));
    assert!(state.site.citadel.store.get(ItemType::Daemon) > 40);
}

#[test]
fn daemons_and_c2_move_between_a_citadel_and_a_docked_vessel() {
    let (data, mut world) = new_game();
    let transit = host(&data, "Transit");
    claim(&mut world, transit, CREW);
    let citadel = &mut world.hosts[usize::from(transit.0)].site.citadel;
    citadel.store.add(ItemType::Daemon, 250);
    citadel.store.add(ItemType::C2Controller, 1);
    let worm = warship(&mut world, CREW, transit, Berth::Connected, 0, false);

    order(&data, &mut world, CREW, Command::InstallC2 { vessel: worm });
    assert!(world.vessels[&worm].c2);
    let report = resolve_turn(
        &data,
        &mut world,
        &Orders::from([(
            CREW,
            vec![
                Command::LoadDaemons {
                    vessel: worm,
                    count: 200,
                },
                Command::LoadDaemons {
                    vessel: worm,
                    count: 1,
                },
                Command::InstallC2 { vessel: worm },
            ],
        )]),
        0,
    );
    assert_eq!(world.vessels[&worm].daemons, 200);
    assert_eq!(report.rejected.len(), 2);
    assert_eq!(report.rejected[0].error, CommandError::TooManyDaemons);
    assert_eq!(report.rejected[1].error, CommandError::AlreadyComplete);
    order(
        &data,
        &mut world,
        CREW,
        Command::UnloadDaemons {
            vessel: worm,
            count: 150,
        },
    );
    assert_eq!(world.vessels[&worm].daemons, 50);
    assert_eq!(
        world.hosts[usize::from(transit.0)]
            .site
            .citadel
            .store
            .get(ItemType::Daemon),
        200
    );
}

#[test]
fn the_legacy_net_builds_daemons_while_at_war() {
    let (data, mut world) = new_game();
    world.players.get_mut(&CREW).unwrap().war = Some(0);
    let colossus = host(&data, "Colossus");
    let before = world.hosts[usize::from(colossus.0)]
        .site
        .citadel
        .store
        .get(ItemType::Daemon);
    days(&data, &mut world, 30);
    let after = world.hosts[usize::from(colossus.0)]
        .site
        .citadel
        .store
        .get(ItemType::Daemon);
    assert!(after > before, "{after} > {before}");
    assert!(world.legacy.fleets[0].daemons > 0);
    assert!(
        world.legacy.fleets[1].daemons > 0,
        "every network's swarm grows"
    );
}

#[test]
fn a_sniffer_grabs_small_caches_and_finds_fragments() {
    let (data, mut world) = new_game();
    let scrapyard = host(&data, "Scrapyard");
    let worm = warship(&mut world, CREW, scrapyard, Berth::Lurking, 0, false);
    {
        let vessel = world.vessels.get_mut(&worm).unwrap();
        vessel.modules[0] = Module::ToolModule(Some(Cargo {
            item: ItemType::Sniffer,
            count: 1,
        }));
        vessel.modules[1] = Module::ToolModule(None);
        vessel.modules[2] = Module::DataContainer(None);
    }
    // Lurking burns no anonymisation, so the worm can scan for a long time.
    let events = days(&data, &mut world, 400);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::CacheFound { vessel, .. } if *vessel == worm))
    );
    let grabbed = match &world.vessels[&worm].modules[2] {
        Module::DataContainer(Some(cargo)) => Some(*cargo),
        _ => None,
    };
    let grabbed = grabbed.expect("a small cache was grabbed in 400 days");
    assert!(data.host(scrapyard).resources.contains(&grabbed.item));
    assert!(grabbed.count >= 50);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::FragmentFound { vessel, .. } if *vessel == worm))
    );
    assert!(matches!(
        world.vessels[&worm].modules[1],
        Module::ToolModule(Some(Cargo {
            item: ItemType::SourceFragment,
            ..
        }))
    ));

    // A fragment brought home opens the Legacy exploit.
    world
        .players
        .get_mut(&CREW)
        .unwrap()
        .hideout
        .store
        .add(ItemType::SourceFragment, 1);
    let events = days(&data, &mut world, 1);
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Unlocked { milestone: Milestone::SourceCode, player, .. } if *player == CREW
    )));
    assert!(
        world.players[&CREW]
            .research
            .contains_key(&ItemType::LegacyExploit)
    );
}

#[test]
fn a_crawler_mines_large_caches_into_a_container() {
    let (data, mut world) = new_game();
    let scrapyard = host(&data, "Scrapyard");
    let worm = warship(&mut world, CREW, scrapyard, Berth::Lurking, 0, false);
    {
        let vessel = world.vessels.get_mut(&worm).unwrap();
        vessel.modules[0] = Module::ToolModule(Some(Cargo {
            item: ItemType::Crawler,
            count: 1,
        }));
        vessel.modules[1] = Module::DataContainer(None);
    }
    days(&data, &mut world, 400);
    let mined = match &world.vessels[&worm].modules[1] {
        Module::DataContainer(Some(cargo)) => Some(*cargo),
        _ => None,
    };
    let mined = mined.expect("a large cache was mined in 400 days");
    assert!(data.host(scrapyard).resources.contains(&mined.item));
    assert!(mined.count >= 16, "{}", mined.count);
    assert!(
        world.vessels[&worm].modules[2] == Module::Empty,
        "a crawler never grabs a cache whole"
    );
}
