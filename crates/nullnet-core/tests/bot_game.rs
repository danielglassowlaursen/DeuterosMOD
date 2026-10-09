//! Long games between bot crews. They play the rules through every system
//! ported so far, over thousands of days, and check after every turn that
//! nothing the rules promise has broken.

use nullnet_core::{
    AbortReason, Citadel, CommandError, Controller, Event, GameData, HostId, Milestone, Module,
    Orders, PlayerId, STAFF_SLOTS, Site, StaffKind, Store, TurnReport, Vessel, World, bot,
    resolve_turn,
};

const TURN: u32 = 10;

fn crews(count: u8) -> Vec<(PlayerId, String)> {
    (0..count)
        .map(|i| (PlayerId(i), format!("Crew {i}")))
        .collect()
}

fn new_game(data: &GameData, seed: u64, count: u8) -> World {
    let crews = crews(count);
    let named: Vec<(PlayerId, &str)> = crews.iter().map(|(id, n)| (*id, n.as_str())).collect();
    World::new_game(data, seed, &named)
}

/// Plays one turn: every crew's bot gives its orders, then the world runs.
fn play_turn(data: &GameData, world: &mut World) -> (Orders, TurnReport) {
    let orders: Orders = world
        .players
        .keys()
        .map(|&crew| (crew, bot::orders(data, world, crew)))
        .collect();
    let report = resolve_turn(data, world, &orders, TURN);
    (orders, report)
}

/// Plays until `days`, checking every turn, and returns every event.
fn play(data: &GameData, world: &mut World, days: u32) -> Vec<Event> {
    let mut events = Vec::new();
    while world.day < days {
        let before = world.clone();
        let (orders, report) = play_turn(data, world);
        check_orders(world, &orders, &report);
        check_world(data, &before, world, &report);
        events.extend(report.events);
    }
    events
}

/// The bot only gives orders the rules accept. The exceptions are races:
/// two crews installing on the same free host in one turn, where the crew
/// whose orders apply first claims it, or a rival's orders changing who
/// holds a host before the bot's apply.
fn check_orders(world: &World, orders: &Orders, report: &TurnReport) {
    for rejected in &report.rejected {
        let command = &orders[&rejected.player][rejected.index];
        let rival = |host| match world.host(host).controller {
            Some(Controller::Crew(crew)) => crew != rejected.player,
            _ => false,
        };
        let race = match rejected.error {
            CommandError::HostTaken(host) | CommandError::NotYourHost(host) => rival(host),
            CommandError::NothingToAttack | CommandError::NothingToRaid => true,
            _ => false,
        };
        assert!(
            race && world.players.len() > 1,
            "day {}: {:?} gave {command:?}, rejected: {}",
            report.first_day - 1,
            rejected.player,
            rejected.error
        );
    }
}

fn check_store(store: &Store, what: &str) {
    for (item, count) in store.iter() {
        assert!(count <= Store::CAP, "{what}: {count} {item:?}");
    }
}

fn check_site(site: &Site, what: &str) {
    check_store(&site.store, what);
    check_store(&site.citadel.store, what);
    assert!(site.taps <= Site::MAX_TAPS, "{what}: {} taps", site.taps);
    assert!(site.backdoor_parts <= Site::BACKDOOR_PARTS, "{what}");
    assert!(site.citadel.modules <= Citadel::MODULES, "{what}");
    assert!(site.staff.len() <= STAFF_SLOTS, "{what}: staff");
    assert!(site.citadel.staff.len() <= STAFF_SLOTS, "{what}: staff");
}

fn check_vessel(vessel: &Vessel, what: &str) {
    assert!(vessel.fuel <= Vessel::FUEL_CAPACITY, "{what}: fuel");
    assert_eq!(vessel.modules.len(), vessel.kind.modules(), "{what}");
    for module in &vessel.modules {
        if let Module::DataContainer(Some(cargo)) = module {
            assert!(cargo.count <= Module::CONTAINER_CAPACITY, "{what}: cargo");
        }
        if let Module::ToolModule(Some(cargo)) = module {
            assert!(cargo.count > 0, "{what}: empty cargo");
        }
    }
}

fn check_world(data: &GameData, before: &World, world: &World, report: &TurnReport) {
    assert_eq!(world.day, before.day + TURN);
    assert_eq!(world.hosts.len(), data.hosts.len());

    for (id, player) in &world.players {
        let what = format!("{} on day {}", player.name, world.day);
        check_site(&player.hideout, &what);
        let course = |kind| data.recruitment.courses[&kind].team_max.unwrap_or(u32::MAX);
        if let Some(team) = &player.research_team {
            assert!(team.count <= course(StaffKind::Analyst), "{what}");
        }
        if let Some(coders) = &player.workshop.coders {
            assert!(coders.count <= course(StaffKind::Coder), "{what}");
        }
        let previous = &before.players[id];
        assert!(player.recruitment.available <= previous.recruitment.available);
        assert!(
            player.milestones.is_superset(&previous.milestones),
            "{what}"
        );
        for (item, progress) in &previous.research {
            assert!(
                !progress.researched || player.research[item].researched,
                "{what}: forgot {item:?}"
            );
        }
    }

    // A held host changes hands only through a siege, a battle or a raid.
    for (index, (old, new)) in before.hosts.iter().zip(&world.hosts).enumerate() {
        let what = format!("{} on day {}", data.hosts[index].name, world.day);
        check_site(&new.site, &what);
        if old.controller.is_some() && old.controller != new.controller {
            let id = HostId(index as u16);
            let fought = report.events.iter().any(|e| {
                matches!(
                    e,
                    Event::HostCaptured { host, .. }
                        | Event::HostFreed { host, .. }
                        | Event::HostTaken { host, .. }
                    if *host == id
                )
            });
            assert!(fought, "{what}: changed hands without a fight");
        }
        assert!(
            new.site.citadel.modules >= old.site.citadel.modules,
            "{what}"
        );
        if new.controller.is_none() {
            assert_eq!(new.site.citadel.modules, 0, "{what}");
        }
    }

    for (id, vessel) in &world.vessels {
        check_vessel(vessel, &format!("vessel {} on day {}", id.0, world.day));
        assert!(world.players.contains_key(&vessel.owner));
    }

    // A vessel stopped by a siege or a host lost on the way is the war's
    // doing; one burned or stopped for want of anonymisation or a pilot is
    // the bot's.
    for event in &report.events {
        assert!(
            !matches!(
                event,
                Event::VesselBurned { .. }
                    | Event::VesselStopped {
                        reason: AbortReason::NoAnonymisation | AbortReason::NoPilot,
                        ..
                    }
            ),
            "the bot lost control of a vessel: {event:?}"
        );
    }
}

fn milestone_day(events: &[Event], crew: PlayerId, wanted: Milestone) -> Option<u32> {
    events.iter().find_map(|event| match *event {
        Event::Unlocked {
            day,
            player,
            milestone,
        } if player == crew && milestone == wanted => Some(day),
        _ => None,
    })
}

/// Complete citadels a crew holds on hosts it claimed.
fn citadels(world: &World, crew: PlayerId) -> usize {
    world
        .hosts
        .iter()
        .filter(|h| h.controller == Some(Controller::Crew(crew)) && h.site.citadel.complete())
        .count()
}

#[test]
fn a_lone_crew_builds_its_citadel_and_spreads_through_the_home_network() {
    let data = GameData::classic();
    let mut world = new_game(&data, 1, 1);
    let events = play(&data, &mut world, 3000);
    let crew = PlayerId(0);

    for milestone in [
        Milestone::FirstCitadelModule,
        Milestone::HideoutCitadel,
        Milestone::WormEquipment,
    ] {
        let day = milestone_day(&events, crew, milestone);
        assert!(day.is_some(), "{milestone:?} never reached");
    }
    assert!(milestone_day(&events, crew, Milestone::HideoutCitadel) < Some(700));

    let player = &world.players[&crew];
    assert_eq!(player.hideout.taps, Site::MAX_TAPS);
    let workshop = &player.hideout.citadel.workshop;
    assert!(
        workshop.coders.is_some() || workshop.automated,
        "coders or a build-bot run the citadel's workshop"
    );
    assert!(
        world.vessels.values().any(|v| v.script.is_some()),
        "the dropper runs supplies by exfil script"
    );
    // The bot holds back from the sixth citadel until it is armed, mines
    // the cache field for a source fragment, works a colony with a backdoor
    // and taps, and builds daemons and a warship.
    assert!(
        milestone_day(&events, crew, Milestone::SourceCode).is_some(),
        "the sniffer found a fragment"
    );
    assert!(
        milestone_day(&events, crew, Milestone::Daemons).is_some(),
        "the exploit opened daemons"
    );
    assert!(player.war.is_none(), "no war before the crew is ready");
    let held = citadels(&world, crew);
    assert!(held >= 3, "only {held} citadels by day 3000");
    let colony = world.hosts.iter().any(|h| {
        h.controller == Some(Controller::Crew(crew))
            && h.site.backdoor_complete()
            && h.site.taps == Site::MAX_TAPS
    });
    assert!(colony, "a colony with a backdoor and taps");
    let daemons: u32 = world
        .hosts
        .iter()
        .filter(|h| h.controller == Some(Controller::Crew(crew)))
        .map(|h| h.site.citadel.store.get(nullnet_core::ItemType::Daemon))
        .sum::<u32>()
        + player
            .hideout
            .citadel
            .store
            .get(nullnet_core::ItemType::Daemon)
        + world
            .vessels
            .values()
            .filter(|v| v.owner == crew)
            .map(|v| v.daemons)
            .sum::<u32>();
    assert!(daemons >= 10, "only {daemons} daemons by day 3000");
    assert!(
        world.vessels.values().any(|v| v.owner == crew && v.c2),
        "a warship with a C2 controller"
    );
}

#[test]
fn rival_crews_race_for_the_home_network_without_breaking_the_rules() {
    let data = GameData::classic();
    for crews in 2..=4 {
        let mut world = new_game(&data, u64::from(crews), crews);
        let events = play(&data, &mut world, 4000);
        let lost = |crew: PlayerId| {
            events
                .iter()
                .filter(|e| matches!(e, Event::HostCaptured { player, .. } if *player == crew))
                .count()
        };

        let home = data.host(data.hideout.host).network;
        let free = data
            .hosts
            .iter()
            .zip(&world.hosts)
            .filter(|(def, state)| {
                def.network == home
                    && !def.cache_field
                    && state.controller.is_none()
                    && def.name != data.host(data.hideout.host).name
            })
            .count();
        // Every crew builds its five citadels (the sixth waits on being
        // armed), or more where the war has taken some.
        let total: usize = world
            .players
            .keys()
            .map(|&crew| {
                let held = citadels(&world, crew) + lost(crew);
                assert!(held >= 3, "{crews} crews: crew {crew:?} built {held}");
                held
            })
            .sum();
        assert!(
            free == 0 || total >= 3 * crews as usize,
            "{crews} crews: {total} citadels built, {free} hosts still free"
        );
    }
}

#[test]
fn bot_games_are_deterministic_and_survive_save_and_restore() {
    let data = GameData::classic();
    let mut first = new_game(&data, 9, 3);
    let mut second = new_game(&data, 9, 3);
    play(&data, &mut first, 1200);
    play(&data, &mut second, 1200);
    assert_eq!(first, second);

    let saved = serde_json::to_string(&first).unwrap();
    let mut restored: World = serde_json::from_str(&saved).unwrap();
    assert_eq!(restored, first);
    let a = play(&data, &mut first, 2400);
    let b = play(&data, &mut restored, 2400);
    assert_eq!(a, b);
    assert_eq!(first, restored);
}
