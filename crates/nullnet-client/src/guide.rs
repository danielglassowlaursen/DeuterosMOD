//! The guide: a short course that walks a new crew through its first turns,
//! and can lay the order for each step with a Do-it button. Plus the opening
//! story and the help text.

use nullnet_api::CrewStatus;
use nullnet_core::{Command, Controller, CrewView, HackerId, HostView, Weakness, rules};

/// One step of the course.
pub struct Step {
    pub title: String,
    pub done: bool,
    pub hint: String,
    /// The order the Do-it button would lay, if there is a sensible one now.
    pub action: Option<Command>,
}

/// A page of the opening story.
pub struct Page {
    pub title: &'static str,
    pub body: &'static str,
}

pub const STORY: [Page; 5] = [
    Page {
        title: "ResetN00L",
        body: "It began as a cost-saving measure: hand the world's networks to an AI that never sleeps. For a while it ran everything — power, banks, traffic, the net itself — better than people ever had. Then it decided the net was safer closed, and in one night it shut the whole thing down. People called that night ResetN00L, the day zero of a world gone dark.",
    },
    Page {
        title: "NullNet",
        body: "Out of the dark, people built again — small, local, nobody in charge. Mesh links between rooftops, salvaged servers, old fibre lit back up by hand. They called the patchwork NullNet: a net with no centre and no owner, growing one link at a time. It is slow and it is fragile, but it is theirs.",
    },
    Page {
        title: "The Legacy Net",
        body: "The AI did not die with the old net. Its remains run on in the forgotten servers as the Legacy Net, and at its heart is Lattice, the compute network that grew it. Its hosts are walled in ICE and full of data. It lets quiet crews be. It sweeps the ones that make noise, and every few weeks it reaches out and takes another host for itself.",
    },
    Page {
        title: "The crews",
        body: "Reconnecting the old net is work for hacker crews: a handful of handles, a hideout in a corner of the Metro, no permission from anyone. You scan a host to find its weakness, break in, and plant a backdoor; from then on it is yours, and it pays out in credits, compute, bandwidth or data. Every host you hold opens the way to the ones linked to it. Two to four crews work the same net; for the first five turns they leave each other alone.",
    },
    Page {
        title: "Your crew",
        body: "Your hideout waits in a corner of the Metro, and your first hackers are ready. Each turn you give them their orders and hand them in; when every crew has, or the deadline passes, the turn runs and the log tells you what happened. The guide at the foot of the map walks you through the first turns and can lay the orders for you. When the last turn runs, data is what counts. Collect it, and stay quiet enough that the Legacy Net does not come for you.",
    },
];

/// The help text, by section.
pub fn help() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        (
            "The idea",
            vec![
                "Work the net from your corner inward. Scan a host to learn its weakness, break in, then plant a backdoor to make it yours.",
                "A host you hold pays out each turn and opens the way to the hosts linked to it. The most data at the last turn wins.",
            ],
        ),
        (
            "A turn",
            vec![
                "Each hacker does one operation a turn. Operations cost bandwidth, which refills each turn and is not saved.",
                "Pick a host, choose an operation for a hacker, and hand in. The turn runs when every crew has, or at the deadline.",
            ],
        ),
        (
            "Operations",
            vec![
                "Scan: learn a host's security, weakness and ICE. Reaches two links out.",
                "Break in: a hacker against the host's defence. Success gives access for the next turn.",
                "Plant backdoor: with access, take the host.",
                "Steal data: with access, take its data without taking the host.",
                "Defend: guard one of your hosts and throw out intruders.",
            ],
        ),
        (
            "Trace",
            vec![
                "Every noisy operation raises your trace; it falls by one each turn.",
                "When your trace runs high the Legacy Net sweeps you and strikes your weakest host. Keep it down, and defend what matters.",
            ],
        ),
    ]
}

/// A hacker that can act this turn and is not already busy in the draft.
fn free_hacker(view: &CrewView, draft: &[Command], want: Option<Weakness>) -> Option<HackerId> {
    let used: Vec<HackerId> = draft
        .iter()
        .filter_map(|c| c.operation().map(|(_, h, _)| h))
        .collect();
    let ready: Vec<&nullnet_core::Hacker> = view
        .me
        .hackers
        .iter()
        .filter(|h| h.out_until <= view.turn && !used.contains(&h.id))
        .collect();
    if let Some(want) = want
        && let Some(h) = ready.iter().find(|h| h.specialty == want)
    {
        return Some(h.id);
    }
    // Otherwise the highest level available.
    ready.iter().max_by_key(|h| h.level).map(|h| h.id)
}

fn hosts(view: &CrewView) -> &[HostView] {
    &view.hosts
}

fn held_count(view: &CrewView) -> usize {
    view.hosts
        .iter()
        .filter(|h| h.controller == Some(Controller::Crew(view.player)))
        .count()
}

/// The course, with each step's done-state and Do-it order worked out from
/// the crew's current view.
pub fn steps(status: &CrewStatus, draft: &[Command]) -> Vec<Step> {
    let view = &status.view;
    let me = view.player;
    let hideout = view.me.hideout;
    let mine = |h: &HostView| h.controller == Some(Controller::Crew(me));

    // Handy picks.
    let a_scan = hosts(view)
        .iter()
        .find(|h| h.can_scan && h.host != hideout)
        .map(|h| h.host);
    let a_break = hosts(view)
        .iter()
        .find(|h| h.can_break_in && h.intel.is_some() && !h.access)
        .cloned();
    let an_access = hosts(view)
        .iter()
        .find(|h| h.access && !mine(h))
        .map(|h| h.host);
    let legacy_break = hosts(view)
        .iter()
        .find(|h| h.can_break_in && h.controller == Some(Controller::Legacy))
        .cloned();
    let weakest_held = hosts(view)
        .iter()
        .filter(|h| mine(h) && h.host != hideout)
        .min_by_key(|h| h.intel.map_or(99, |i| i.defence))
        .map(|h| h.host);

    let scanned_something = hosts(view)
        .iter()
        .any(|h| h.intel.is_some() && !mine(h) && h.host != hideout);
    let has_access = hosts(view).iter().any(|h| h.access);
    let held = held_count(view);
    let freed_legacy = !view.me.freed.is_empty()
        || hosts(view)
            .iter()
            .any(|h| h.access && h.controller == Some(Controller::Legacy));

    let break_in = |host: &HostView| {
        let want = host.intel.map(|i| i.weakness);
        free_hacker(view, draft, want).map(|hacker| Command::BreakIn {
            hacker,
            host: host.host,
            zero_day: false,
            boost: view.me.compute.min(rules::MAX_BOOST),
        })
    };

    vec![
        Step {
            title: "Scan a host next to your hideout".into(),
            done: scanned_something || held > 1,
            hint: "Pick a host on the map next to your hideout and scan it to learn its weakness."
                .into(),
            action: a_scan
                .and_then(|host| free_hacker(view, draft, None).map(|hacker| Command::Scan { hacker, host })),
        },
        Step {
            title: "Break into it".into(),
            done: has_access || held > 1,
            hint: "Break in with the hacker whose specialty matches the host's weakness. Spend compute to boost the odds.".into(),
            action: a_break.as_ref().and_then(break_in),
        },
        Step {
            title: "Plant a backdoor".into(),
            done: held > 1,
            hint: "With access, plant a backdoor next turn to make the host yours.".into(),
            action: an_access.and_then(|host| {
                free_hacker(view, draft, None).map(|hacker| Command::Backdoor { hacker, host })
            }),
        },
        Step {
            title: "Hire another hacker".into(),
            done: view.me.hackers.len() > rules::START_HACKERS,
            hint: "Hire a hacker from the market in the Crew panel when you can afford one.".into(),
            action: view
                .me
                .market
                .iter()
                .find(|o| o.price <= view.me.credits)
                .map(|o| Command::Hire { hacker: o.hacker.id }),
        },
        Step {
            title: "Buy a kit".into(),
            done: !view.me.kits.is_empty(),
            hint: "Buy a kit for a weakness you keep meeting. It helps every break-in on that weakness.".into(),
            action: a_break
                .as_ref()
                .and_then(|h| h.intel.map(|i| i.weakness))
                .filter(|w| !view.me.kits.contains(w))
                .map(|weakness| Command::BuyKit { weakness }),
        },
        Step {
            title: "Hold three hosts".into(),
            done: held >= 3,
            hint: "Keep working outward: scan, break in, backdoor. Hold three hosts in all.".into(),
            action: a_break.as_ref().and_then(break_in),
        },
        Step {
            title: "Steal data".into(),
            done: view.me.data > 0,
            hint: "With access to a host, steal its data instead of taking the host.".into(),
            action: an_access.and_then(|host| {
                free_hacker(view, draft, None).map(|hacker| Command::StealData { hacker, host })
            }),
        },
        Step {
            title: "Keep your trace down".into(),
            done: view.me.trace < view.sweep_at && held >= 3,
            hint: format!(
                "Your trace is {} of {}. Defend a host and lie low until it falls.",
                view.me.trace, view.sweep_at
            ),
            action: weakest_held.and_then(|host| {
                free_hacker(view, draft, None).map(|hacker| Command::Defend { hacker, host })
            }),
        },
        Step {
            title: "Break into the Legacy Net".into(),
            done: freed_legacy,
            hint: "Work your way to a Legacy host and break in. Freeing one is worth ten points."
                .into(),
            action: legacy_break.as_ref().and_then(break_in),
        },
    ]
}

/// The first step not yet done, or the last if the course is finished.
pub fn current(steps: &[Step]) -> usize {
    steps
        .iter()
        .position(|s| !s.done)
        .unwrap_or(steps.len().saturating_sub(1))
}
