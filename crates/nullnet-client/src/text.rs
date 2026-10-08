//! Words for the things the client shows: items, vessels, orders and
//! events, in plain English and plain ASCII, which is all the built-in
//! font has.

use nullnet_core::{
    Berth, Command, Destination, Event, GameData, ItemType, Module, Staff, Vessel, VesselId,
    VesselKind, VesselState, WorkshopRef,
};
use std::collections::BTreeMap;

/// `DropperCore` as "dropper core".
pub fn item(item: ItemType) -> String {
    spaced(&format!("{item:?}"))
}

pub fn spaced(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push(' ');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

pub fn kind(kind: VesselKind) -> &'static str {
    match kind {
        VesselKind::Dropper => "dropper",
        VesselKind::Worm => "worm",
        VesselKind::Tunneler => "tunneler",
    }
}

pub fn vessel(id: VesselId, vessels: &BTreeMap<VesselId, Vessel>) -> String {
    match vessels.get(&id) {
        Some(v) => format!("{} {}", kind(v.kind), id.0),
        None => format!("vessel {}", id.0),
    }
}

pub fn berth(berth: Berth) -> &'static str {
    match berth {
        Berth::Planted => "inside",
        Berth::Connected => "at the citadel",
        Berth::Lurking => "outside",
    }
}

pub fn state(state: VesselState, data: &GameData) -> String {
    match state {
        VesselState::At(b) => berth(b).to_string(),
        VesselState::Exfiltrating { until } => format!("exfiltrating, out on day {until}"),
        VesselState::Injecting { until } => format!("injecting, inside on day {until}"),
        VesselState::Disconnecting { until } => format!("disconnecting, out on day {until}"),
        VesselState::Connecting => "connecting to the citadel".into(),
        VesselState::Routing { to, until } => {
            format!("routing to {}, there on day {until}", data.host(to).name)
        }
    }
}

pub fn team(staff: &Staff) -> String {
    format!(
        "{} ({} {}s, level {})",
        staff.leader,
        staff.count,
        format!("{:?}", staff.kind).to_lowercase(),
        staff.level()
    )
}

pub fn module(module: &Module) -> String {
    match module {
        Module::Empty => "empty slot".into(),
        Module::DataContainer(None) => "data container, empty".into(),
        Module::DataContainer(Some(c)) => format!("data container: {} {}", c.count, item(c.item)),
        Module::ToolModule(None) => "tool module, empty".into(),
        Module::ToolModule(Some(c)) => format!("tool module: {} {}", c.count, item(c.item)),
        Module::SessionPod(None) => "session pod, empty".into(),
        Module::SessionPod(Some(t)) => format!("session pod: {}", team(t)),
    }
}

fn destination(to: Destination, data: &GameData) -> String {
    format!("{} {}", berth(to.berth), data.host(to.host).name)
}

fn workshop(at: WorkshopRef, data: &GameData) -> String {
    match at {
        WorkshopRef::Hideout => "in the hideout".into(),
        WorkshopRef::Citadel(nullnet_core::SiteRef::Hideout) => "in the hideout citadel".into(),
        WorkshopRef::Citadel(nullnet_core::SiteRef::Host(h)) => {
            format!("in the citadel at {}", data.host(h).name)
        }
    }
}

pub fn command(cmd: &Command, data: &GameData, vessels: &BTreeMap<VesselId, Vessel>) -> String {
    let v = |id: VesselId| vessel(id, vessels);
    match cmd {
        Command::SetResearch { item: i } => format!("Research {}", item(*i)),
        Command::Recruit { kind, count } => {
            format!("Recruit {count} {}s", format!("{kind:?}").to_lowercase())
        }
        Command::Build { at, item: i } => format!("Build {} {}", item(*i), workshop(*at, data)),
        Command::InstallTaps { site, count } => match site {
            nullnet_core::SiteRef::Hideout => format!("Install {count} tap(s) in the hideout"),
            nullnet_core::SiteRef::Host(h) => {
                format!("Install {count} tap(s) at {}", data.host(*h).name)
            }
        },
        Command::Automate { item: i, mode, .. } => {
            format!("Queue {} on the build-bot ({mode:?})", item(*i))
        }
        Command::AssignCoders { at, .. } => format!("Put coders to work {}", workshop(*at, data)),
        Command::ReleaseCoders { at } => format!("Release the coders {}", workshop(*at, data)),
        Command::Assemble {
            host,
            berth: b,
            kind: k,
        } => format!(
            "Assemble a {} {} {}",
            kind(*k),
            berth(*b),
            data.host(*host).name
        ),
        Command::Refuel { vessel, amount } => format!("Refuel {} with {amount}", v(*vessel)),
        Command::Fit {
            vessel,
            slot,
            module: Some(m),
        } => format!(
            "Fit {} in slot {} of {}",
            spaced(&format!("{m:?}")),
            slot + 1,
            v(*vessel)
        ),
        Command::Fit {
            vessel,
            slot,
            module: None,
        } => format!("Clear slot {} of {}", slot + 1, v(*vessel)),
        Command::Load {
            vessel,
            slot,
            item: i,
            count,
        } => format!(
            "Load {count} {} into slot {} of {}",
            item(*i),
            slot + 1,
            v(*vessel)
        ),
        Command::Unload { vessel, slot } => format!("Unload slot {} of {}", slot + 1, v(*vessel)),
        Command::Board { vessel, seat, team } => match seat {
            nullnet_core::Seat::Pilot => format!("Team {} pilots {}", team + 1, v(*vessel)),
            nullnet_core::Seat::Pod(slot) => {
                format!(
                    "Team {} boards pod {} of {}",
                    team + 1,
                    slot + 1,
                    v(*vessel)
                )
            }
        },
        Command::Disembark { vessel, seat } => match seat {
            nullnet_core::Seat::Pilot => format!("The pilot leaves {}", v(*vessel)),
            nullnet_core::Seat::Pod(slot) => {
                format!("Pod {} of {} disembarks", slot + 1, v(*vessel))
            }
        },
        Command::Dispatch { vessel, to } => {
            format!("Send {} {}", v(*vessel), destination(*to, data))
        }
        Command::Deploy { vessel, slot } => format!("Install slot {} of {}", slot + 1, v(*vessel)),
        Command::InstallScript { vessel } => format!("Install an exfil script in {}", v(*vessel)),
        Command::ConfigureScript {
            vessel,
            route: Some(r),
        } => format!(
            "Run {} on a route {} -> {}",
            v(*vessel),
            destination(r.from, data),
            destination(r.to, data)
        ),
        Command::ConfigureScript {
            vessel,
            route: None,
        } => {
            format!("Stop the route of {}", v(*vessel))
        }
        Command::InstallLink { host } => {
            format!("Install an encrypted link at {}", data.host(*host).name)
        }
        Command::ConfigureLink { host, target, .. } => match target {
            Some(target) => format!(
                "Link {} to {}",
                data.host(*host).name,
                data.host(*target).name
            ),
            None => format!("Unlink {}", data.host(*host).name),
        },
    }
}

pub fn event(ev: &Event, data: &GameData, vessels: &BTreeMap<VesselId, Vessel>) -> String {
    let host = |h: nullnet_core::HostId| data.host(h).name.clone();
    match ev {
        Event::ResearchCompleted { item: i, .. } => format!("Researched {}", item(*i)),
        Event::StaffPromoted { kind, level, .. } => {
            format!(
                "A {} team reached level {level}",
                format!("{kind:?}").to_lowercase()
            )
        }
        Event::RecruitsGraduated { kind, count, .. } => {
            format!("{count} {}s graduated", format!("{kind:?}").to_lowercase())
        }
        Event::ItemBuilt { item: i, at, .. } => {
            format!("Built {} {}", item(*i), workshop(*at, data))
        }
        Event::HostClaimed {
            player, host: h, ..
        } => {
            format!("Crew {} claimed {}", player.0 + 1, host(*h))
        }
        Event::Installed {
            host: h,
            item: i,
            installed,
            ..
        } => format!("{} {installed} installed at {}", item(*i), host(*h)),
        Event::VesselArrived {
            vessel: id,
            host: h,
            berth: b,
            ..
        } => format!("{} is {} {}", vessel(*id, vessels), berth(*b), host(*h)),
        Event::VesselStopped {
            vessel: id, reason, ..
        } => format!(
            "{} stopped: {}",
            vessel(*id, vessels),
            spaced(&format!("{reason:?}"))
        ),
        Event::VesselBurned {
            vessel: id,
            host: h,
            ..
        } => {
            format!(
                "{} was traced and burned at {}",
                vessel(*id, vessels),
                host(*h)
            )
        }
        Event::Unlocked { milestone, .. } => {
            format!("Milestone: {}", spaced(&format!("{milestone:?}")))
        }
    }
}
