//! The built-in guide. A new crew lands on a map full of buttons with no
//! idea what to do first, so the client works out where the crew is in the
//! opening and names the panel and the button that move it on. The steps
//! follow the order the bot opens its game in; each is judged done from the
//! crew's latest view, so a player who skips ahead is never nagged about a
//! step already behind them. The help text at the end is the rules in brief.

use nullnet_api::CrewStatus;
use nullnet_core::legacy::CITADELS_FOR_WAR;
use nullnet_core::score::{
    CITADEL_POINTS, FREED_POINTS, HOST_POINTS, RESEARCH_POINTS, TAKEN_POINTS,
};
use nullnet_core::{
    Berth, Citadel, Controller, GameData, ItemType, Module, PROTECTION_TURNS, Player, Site,
    StaffKind, Vessel, VesselKind, Workshop,
};

use crate::text;

/// One step of the opening, with what to press next while it is open.
pub struct Step {
    pub title: &'static str,
    pub done: bool,
    /// What to do now, a line each; empty once the step is done.
    pub hints: Vec<String>,
}

/// Every step, in order, judged from the crew's latest view.
pub fn steps(status: &CrewStatus, data: &GameData) -> Vec<Step> {
    let crew = Crew { status, data };
    vec![
        crew.recruit_teams(),
        crew.recruit_operators(),
        crew.tap_hideout(),
        crew.research_dropper(),
        crew.assemble_dropper(),
        crew.research_citadel(),
        crew.crew_dropper(),
        crew.first_module(),
        crew.complete_citadel(),
        crew.research_worm(),
        crew.staff_citadel(),
        crew.supply_citadel(),
        crew.assemble_worm(),
        crew.claim_host(),
        crew.arm(),
        crew.compete(),
    ]
}

/// The step the guide shows: the first one still open.
pub fn current(steps: &[Step]) -> usize {
    steps
        .iter()
        .position(|s| !s.done)
        .unwrap_or(steps.len().saturating_sub(1))
}

/// The rules in brief, for the help overlay: a heading and its lines.
pub const HELP: &[(&str, &[&str])] = &[
    (
        "Turns",
        &[
            "Every button queues an order in the ORDERS panel. Press 'Hand in' to send the orders; 'Withdraw' takes them back while the turn is still open.",
            "The turn runs when every crew has handed in or the deadline passes. The days of the turn then play out, orders first, and the LOG shows what happened.",
            "Orders the server would refuse are marked in the ORDERS panel before the turn runs, so nothing is lost by trying.",
        ],
    ),
    (
        "The map",
        &[
            "Every dot is a host. Your hosts carry your colour, rivals theirs, free hosts a dim ring and the Legacy Net's hosts a red ring. The map pulses where a swarm is coming.",
            "Click a host to see it in the right panel, or a vessel marker to select the vessel. Hosts in one network are a few days apart; other networks are far.",
        ],
    ),
    (
        "The hideout",
        &[
            "Your base on the host every crew shares; it cannot be taken. Taps extract the host's resources into its store, coders turn them into items in its workshop, and analysts research new items.",
            "Items marked for the citadel can only be built up there, in a citadel's own workshop from its own store.",
        ],
    ),
    (
        "Staff",
        &[
            "Analysts research, coders build, operators pilot and defend. A team is promoted from level 1 to 3 by finishing enough actions; worm parts need level 2 analysts, most advanced items level 3.",
            "Recruits are limited for the whole game, so courses should not be wasted.",
        ],
    ),
    (
        "Vessels",
        &[
            "A dropper shuttles on one host between three berths: inside, the citadel above, and outside. A worm routes between hosts, carries three pods and is assembled at a citadel.",
            "Every vessel needs a pilot and anonymisation (proxy chains). Outside with none left, it is burned after five days.",
            "Pods: a tool module carries one citadel module or backdoor kit and installs it; a data container carries 250 units of one resource; a session pod carries a team.",
        ],
    ),
    (
        "Citadels",
        &[
            "Eight citadel modules installed from outside make a citadel. It has its own store and workshop, builds the big items and is where worms are assembled.",
            "Installing a module on a free host claims the host. Two backdoor kits and taps make a site extract; an exfil script lets a dropper run a supply route on its own.",
        ],
    ),
    (
        "The Legacy Net",
        &[
            "The old network's automated defences hold the red hosts. A crew with six complete citadels is at war: swarms of daemons besiege the hottest crew's hosts and take them when the siege runs out.",
            "Up to 200 daemons in a citadel store fight as its garrison under its best operator team. A worm with a C2 controller and daemons fights swarms and frees Legacy hosts.",
        ],
    ),
    (
        "Rivals",
        &[
            "From turn 5 a worm with a C2 controller and daemons outside a rival's host can raid it: exfiltrate the store, plant a tap that siphons its extraction for 100 days, or take the host. Each raid adds 25 heat, cooling one a day.",
        ],
    ),
    (
        "Points and the end",
        &[
            "10 points per complete citadel, 3 per other host, 15 per Legacy host freed, 5 per host taken from a rival, 2 per item researched.",
            "The game ends when a crew holds more than half of the home network's contested hosts, or on the last day. Most points wins.",
        ],
    ),
];

/// The crew's view and the rules, read together by every step.
struct Crew<'a> {
    status: &'a CrewStatus,
    data: &'a GameData,
}

fn cap(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

impl<'a> Crew<'a> {
    fn me(&self) -> &'a Player {
        &self.status.view.me
    }

    fn hideout(&self) -> &'a Site {
        &self.me().hideout
    }

    fn home(&self) -> &'a str {
        &self.data.host(self.data.hideout.host).name
    }

    fn researched(&self, item: ItemType) -> bool {
        self.me().research.get(&item).is_some_and(|r| r.researched)
    }

    /// Whether the item can be researched at all yet.
    fn open(&self, item: ItemType) -> bool {
        self.me().research.contains_key(&item)
    }

    fn enrolled(&self, kind: StaffKind) -> u32 {
        self.me()
            .recruitment
            .courses
            .get(&kind)
            .map_or(0, |c| c.enrolled)
    }

    fn mine(&self) -> impl Iterator<Item = &'a Vessel> {
        let player = self.status.player;
        self.status
            .view
            .vessels
            .values()
            .filter(move |v| v.owner == player)
    }

    /// The crew's dropper at home, if it has one.
    fn dropper(&self) -> Option<&'a Vessel> {
        self.mine()
            .find(|v| v.kind == VesselKind::Dropper && v.host == self.data.hideout.host)
    }

    fn worm(&self) -> Option<&'a Vessel> {
        self.mine().find(|v| v.kind == VesselKind::Worm)
    }

    /// Whether any operator team exists: waiting, stationed, piloting or in a pod.
    fn operators_anywhere(&self) -> bool {
        let hideout = self.hideout();
        hideout
            .staff
            .iter()
            .chain(&hideout.citadel.staff)
            .any(|s| s.kind == StaffKind::Operator)
            || self.mine().any(|v| {
                v.pilot.is_some()
                    || v.modules.iter().any(|m| {
                        matches!(m, Module::SessionPod(Some(t)) if t.kind == StaffKind::Operator)
                    })
            })
    }

    fn recipe(&self, item: ItemType) -> String {
        self.data.items[&item]
            .recipe
            .iter()
            .map(|(i, n)| format!("{n} {}", text::item(*i)))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The line that gets `item` built: in the hideout, or `above` in the
    /// citadel.
    fn build_hint(&self, item: ItemType, above: bool) -> String {
        let name = text::item(item);
        let workshop: &Workshop = if above {
            &self.hideout().citadel.workshop
        } else {
            &self.me().workshop
        };
        if let Some(job) = workshop.jobs.iter().find(|j| j.item == item) {
            return if job.active {
                format!("{} is being built (stage {}/4).", cap(&name), job.stage)
            } else {
                format!(
                    "{} is paused in the workshop; press its button again to resume it.",
                    cap(&name)
                )
            };
        }
        if !self.researched(item) {
            return format!("RESEARCH: '{name}' has to be researched before it can be built.");
        }
        if workshop.coders.is_none() && !workshop.automated {
            return if above {
                "The citadel workshop has no coders yet.".into()
            } else {
                "The hideout workshop has no coders yet.".into()
            };
        }
        let place = if above {
            format!("CITADEL (hideout panel): press 'Build {name}'")
        } else {
            format!("BUILD IN THE HIDEOUT: press '{name}'")
        };
        format!("{place}. It takes {}.", self.recipe(item))
    }

    /// A step done when every item is researched, hinting at the open ones.
    fn research_step(&self, title: &'static str, items: &[(ItemType, &str)]) -> Step {
        let me = self.me();
        let done = items.iter().all(|(item, _)| self.researched(*item));
        let mut hints = Vec::new();
        if !done {
            if me.research_team.is_none() {
                hints.push("Research needs analysts: recruit them first.".into());
            }
            let team = me.research_team.as_ref().map_or(1, |t| t.level());
            for &(item, what) in items {
                if self.researched(item) {
                    continue;
                }
                let name = text::item(item);
                match me.research.get(&item) {
                    None => hints.push(format!("{} is not open to research yet.", cap(&name))),
                    Some(progress) if me.current_research == Some(item) => hints.push(format!(
                        "Researching {name} ({}%): {what}. Pick the next item when it reaches 100%.",
                        progress.percent
                    )),
                    Some(_) => {
                        let needed = self.data.research[&item].tech_level;
                        if needed > team {
                            hints.push(format!(
                                "{} needs a level {needed} research team; yours is level {team}. Teams are promoted after enough finished research, so keep the analysts busy on other items.",
                                cap(&name)
                            ));
                        } else {
                            hints.push(format!("RESEARCH: press '{name}': {what}."));
                        }
                    }
                }
            }
        }
        Step { title, done, hints }
    }

    // ------------------------------------------------------------ the steps

    fn recruit_teams(&self) -> Step {
        let me = self.me();
        let done = me.research_team.is_some() && me.workshop.coders.is_some();
        let mut hints = Vec::new();
        if !done {
            for (kind, have, label, job) in [
                (
                    StaffKind::Analyst,
                    me.research_team.is_some(),
                    "analysts",
                    "research new items",
                ),
                (
                    StaffKind::Coder,
                    me.workshop.coders.is_some(),
                    "coders",
                    "build items in the hideout workshop",
                ),
            ] {
                if have {
                    continue;
                }
                let enrolled = self.enrolled(kind);
                if enrolled > 0 {
                    hints.push(format!(
                        "{enrolled} {label} are in training; a course takes {} days.",
                        self.data.recruitment.courses[&kind].days
                    ));
                } else {
                    hints.push(format!(
                        "Hideout panel, RECRUIT: press '+100 {label}'. They {job}."
                    ));
                }
            }
            if !self.status.submitted {
                hints.push(format!(
                    "Every button queues an order in the ORDERS panel; press 'Hand in' to send them. The turn runs when every crew has handed in or the deadline passes, and {} days go by.",
                    self.status.game.turn_days
                ));
            }
        }
        Step {
            title: "Recruit analysts and coders",
            done,
            hints,
        }
    }

    fn recruit_operators(&self) -> Step {
        let done = self.operators_anywhere();
        let mut hints = Vec::new();
        if !done {
            hints.push(if self.enrolled(StaffKind::Operator) > 0 {
                "An operator team is in training; it waits in the hideout once it graduates.".into()
            } else {
                "RECRUIT: press '+20 operators'. Operators pilot vessels: nothing moves without a team waiting in the hideout.".into()
            });
        }
        Step {
            title: "Recruit an operator team",
            done,
            hints,
        }
    }

    fn tap_hideout(&self) -> Step {
        let hideout = self.hideout();
        let done = hideout.taps >= Site::MAX_TAPS;
        let mut hints = Vec::new();
        if !done {
            hints.push(format!(
                "Taps {}/{}. Each tap adds a day's extraction of every resource on {}; everything you build comes from them.",
                hideout.taps,
                Site::MAX_TAPS,
                self.home()
            ));
            if hideout.store.get(ItemType::Tap) > 0 {
                hints.push("Hideout panel: press 'Install a tap'.".into());
            } else if self.me().workshop.coders.is_none() {
                hints.push("Taps are built by coders: recruit them first.".into());
            } else {
                hints.push(self.build_hint(ItemType::Tap, false));
                hints.push(
                    "The workshop builds one item at a time; starting another pauses the current one.".into(),
                );
            }
        }
        Step {
            title: "Tap the hideout",
            done,
            hints,
        }
    }

    fn research_dropper(&self) -> Step {
        self.research_step(
            "Research the dropper",
            &[
                (
                    ItemType::DropperCore,
                    "the hull of the shuttle that lifts things from the hideout to the citadel above it",
                ),
                (ItemType::DropperEngine, "its engine"),
            ],
        )
    }

    fn assemble_dropper(&self) -> Step {
        let done = self.mine().any(|v| v.kind == VesselKind::Dropper);
        let mut hints = Vec::new();
        if !done {
            let store = &self.hideout().store;
            let parts = [ItemType::DropperCore, ItemType::DropperEngine];
            if !parts.iter().all(|&p| self.researched(p)) {
                hints.push("Research the dropper first.".into());
            } else if parts.iter().all(|&p| store.get(p) > 0) {
                hints.push(format!(
                    "Click {} on the map (the host your hideout is on). ASSEMBLE: press 'Assemble a dropper inside'.",
                    self.home()
                ));
            } else {
                for part in parts {
                    if store.get(part) == 0 {
                        hints.push(self.build_hint(part, false));
                    }
                }
            }
        }
        Step {
            title: "Build and assemble a dropper",
            done,
            hints,
        }
    }

    fn research_citadel(&self) -> Step {
        self.research_step(
            "Research anonymisation and the citadel",
            &[
                (
                    ItemType::ProxyChains,
                    "the anonymisation vessels burn; once researched, every workshop refines it on its own from bandwidth and proxies",
                ),
                (
                    ItemType::ToolModule,
                    "a pod that carries one citadel module or kit and installs it",
                ),
                (
                    ItemType::CitadelModule,
                    "eight of them make a citadel above the hideout",
                ),
            ],
        )
    }

    fn crew_dropper(&self) -> Step {
        let dropper = self.dropper();
        let done = dropper.is_some_and(|d| d.pilot.is_some() && d.fuel > 0);
        let mut hints = Vec::new();
        if !done {
            match dropper {
                None => hints.push("Assemble the dropper first.".into()),
                Some(d) => {
                    hints.push(format!(
                        "Click {} on the map, then 'Select' next to the dropper under VESSELS HERE.",
                        self.home()
                    ));
                    if d.pilot.is_none() {
                        match self
                            .hideout()
                            .staff
                            .iter()
                            .find(|s| s.kind == StaffKind::Operator)
                        {
                            Some(team) => {
                                hints.push(format!("CREW: press 'Pilot: {}'.", team.leader))
                            }
                            None => hints.push(
                                "No operator team is waiting in the hideout: RECRUIT '+20 operators'.".into(),
                            ),
                        }
                    }
                    if d.fuel == 0 {
                        if self.hideout().store.get(ItemType::ProxyChains) > 0 {
                            hints.push(
                                "CREW: press 'Refuel'. A trip outside and back burns anonymisation, so refuel between trips.".into(),
                            );
                        } else if !self.researched(ItemType::ProxyChains) {
                            hints.push(
                                "No anonymisation yet: research 'proxy chains'; the hideout refines them on its own afterwards.".into(),
                            );
                        } else {
                            hints.push(
                                "Proxy chains refine from bandwidth and proxies every other day; wait a turn.".into(),
                            );
                        }
                    }
                }
            }
        }
        Step {
            title: "Crew and fuel the dropper",
            done,
            hints,
        }
    }

    /// Where the dropper is on its run with a citadel module, and what to press.
    fn module_run(&self, hints: &mut Vec<String>) {
        let Some(d) = self.dropper() else {
            hints.push("Assemble and crew the dropper first.".into());
            return;
        };
        if d.pilot.is_none() || d.fuel == 0 {
            hints.push("Crew and fuel the dropper first (CREW: 'Pilot', 'Refuel').".into());
        }
        let store = &self.hideout().store;
        let berth = d.berth();
        match d.modules.first() {
            Some(Module::ToolModule(Some(cargo))) if cargo.item == ItemType::CitadelModule => {
                match berth {
                    Some(Berth::Lurking) => hints.push(
                        "Select the dropper. OUTSIDE: press 'Install citadel module (slot 1)', then SEND 'Go inside' for the next one.".into(),
                    ),
                    Some(_) => hints.push(
                        "Select the dropper. SEND: press 'Go outside' and hand in; next turn it can install the module from there.".into(),
                    ),
                    None => hints.push("The dropper is on its way; orders wait until it arrives.".into()),
                }
            }
            Some(Module::ToolModule(Some(_))) => hints.push(
                "Select the dropper. SLOT 1: press 'Unload' to make room for a citadel module.".into(),
            ),
            Some(Module::ToolModule(None)) => {
                if berth != Some(Berth::Planted) {
                    hints.push(
                        "Select the dropper. SEND: press 'Go inside' to load from the hideout store.".into(),
                    );
                } else if store.get(ItemType::CitadelModule) > 0 {
                    hints.push("Select the dropper. SLOT 1: press 'Load 1 citadel module'.".into());
                } else {
                    hints.push(self.build_hint(ItemType::CitadelModule, false));
                }
            }
            Some(Module::Empty) => {
                if store.get(ItemType::ToolModule) == 0 {
                    hints.push(self.build_hint(ItemType::ToolModule, false));
                } else if berth == Some(Berth::Planted) {
                    hints.push("Select the dropper. SLOT 1: press 'Fit tool module'.".into());
                } else {
                    hints.push(
                        "Select the dropper. SEND: 'Go inside', then fit the tool module from the store.".into(),
                    );
                }
                if store.get(ItemType::CitadelModule) == 0 {
                    hints.push(self.build_hint(ItemType::CitadelModule, false));
                }
            }
            _ => hints.push(
                "Select the dropper. SLOT 1: press 'Remove pod' (after 'Unload' or 'Disembark'), then 'Fit tool module'.".into(),
            ),
        }
    }

    fn first_module(&self) -> Step {
        let done = self.hideout().citadel.modules >= 1;
        let mut hints = Vec::new();
        if !done {
            hints.push(
                "A citadel module rides up in the dropper's tool module and is installed from outside. The first one opens worm research.".into(),
            );
            self.module_run(&mut hints);
        }
        Step {
            title: "Lift the first citadel module",
            done,
            hints,
        }
    }

    fn complete_citadel(&self) -> Step {
        let citadel = &self.hideout().citadel;
        let done = citadel.complete();
        let mut hints = Vec::new();
        if !done {
            hints.push(format!(
                "Citadel {}/{} modules. Repeat the run: build a module, go inside, load it, go outside, install, refuel. Research session pod and data container meanwhile.",
                citadel.modules,
                Citadel::MODULES
            ));
            self.module_run(&mut hints);
        }
        Step {
            title: "Complete the hideout's citadel",
            done,
            hints,
        }
    }

    fn research_worm(&self) -> Step {
        let mut step = self.research_step(
            "Research the worm",
            &[
                (
                    ItemType::WormCore,
                    "the hull of a vessel that routes between hosts and carries three pods",
                ),
                (ItemType::WormEngine, "its engine"),
            ],
        );
        if !step.done && !self.open(ItemType::WormCore) {
            step.hints = vec!["Worm research opens with your first citadel module.".into()];
        }
        step
    }

    fn staff_citadel(&self) -> Step {
        let hideout = self.hideout();
        let citadel = &hideout.citadel;
        let done = citadel.workshop.coders.is_some() || citadel.workshop.automated;
        let mut hints = Vec::new();
        if !done {
            let pod_coders = |v: &'a Vessel| {
                v.modules.iter().find_map(|m| match m {
                    Module::SessionPod(Some(team)) if team.kind == StaffKind::Coder => Some(team),
                    _ => None,
                })
            };
            if !citadel.complete() {
                hints.push(
                    "Finish the citadel first. Worm parts, daemons and most advanced items can only be built in a citadel's workshop.".into(),
                );
            } else if let Some(team) = citadel.staff.iter().find(|s| s.kind == StaffKind::Coder) {
                hints.push(format!(
                    "CITADEL (hideout panel): press 'Put the coders to work' so {} runs the citadel workshop.",
                    team.leader
                ));
            } else if let Some(d) = self.dropper()
                && let Some(team) = pod_coders(d)
            {
                match d.berth() {
                    Some(Berth::Connected) => hints.push(format!(
                        "Select the dropper. SLOT 1: press 'Disembark' so {} joins the citadel, then 'Put the coders to work'.",
                        team.leader
                    )),
                    Some(_) => {
                        hints.push("Select the dropper. SEND: press 'Go to the citadel'.".into())
                    }
                    None => hints.push("The dropper is on its way to the citadel.".into()),
                }
            } else if let Some(team) = hideout.staff.iter().find(|s| s.kind == StaffKind::Coder) {
                match self.dropper() {
                    None => hints.push("Assemble a dropper to carry the coders up.".into()),
                    Some(d) => match d.modules.first() {
                        Some(Module::SessionPod(None)) if d.berth() == Some(Berth::Planted) => {
                            hints.push(format!(
                                "Select the dropper. SLOT 1: press 'Board: {}', then SEND 'Go to the citadel'.",
                                team.leader
                            ))
                        }
                        Some(Module::SessionPod(None)) => hints.push(
                            "Select the dropper. SEND: 'Go inside' to pick the coders up.".into(),
                        ),
                        _ if hideout.store.get(ItemType::SessionPod) > 0 => hints.push(
                            "Select the dropper. SLOT 1: 'Remove pod' if one is fitted, then 'Fit session pod'.".into(),
                        ),
                        _ => hints.push(self.build_hint(ItemType::SessionPod, false)),
                    },
                }
            } else {
                hints.push(
                    "Coders have to move up to the citadel's workshop. CITADEL (hideout panel): press 'Release the hideout coders'; they wait in the hideout for the dropper. Then RECRUIT '+100 coders' as the new hideout team.".into(),
                );
                if self.me().workshop.jobs.iter().any(|j| j.active) {
                    hints.push("Coders can only be released while nothing is being built.".into());
                }
            }
        }
        Step {
            title: "Staff the citadel with coders",
            done,
            hints,
        }
    }

    /// Resources the citadel store is short of for a worm core and engine.
    fn worm_shortfall(&self) -> Vec<(ItemType, u32)> {
        let store = &self.hideout().citadel.store;
        let mut need: Vec<(ItemType, u32)> = Vec::new();
        for part in [ItemType::WormCore, ItemType::WormEngine] {
            if store.get(part) > 0 {
                continue;
            }
            for &(item, count) in &self.data.items[&part].recipe {
                match need.iter_mut().find(|(i, _)| *i == item) {
                    Some((_, n)) => *n += count,
                    None => need.push((item, count)),
                }
            }
        }
        need.retain_mut(|(item, n)| {
            *n = n.saturating_sub(store.get(*item));
            *n > 0
        });
        need
    }

    fn supply_citadel(&self) -> Step {
        let citadel = &self.hideout().citadel;
        let short = self.worm_shortfall();
        let done = self.worm().is_some() || short.is_empty();
        let mut hints = Vec::new();
        if !done {
            if !citadel.complete() {
                hints.push(
                    "Finish the citadel first: it has its own store, and worm parts are built from it.".into(),
                );
            } else {
                let list = short
                    .iter()
                    .map(|(i, n)| format!("{n} {}", text::item(*i)))
                    .collect::<Vec<_>>()
                    .join(", ");
                hints.push(format!(
                    "The citadel store is short of {list} for a worm core and engine. Send bandwidth and proxies up as well, so the citadel refines its own anonymisation."
                ));
                match self.dropper() {
                    None => {
                        hints.push("A dropper carries resources up: assemble one first.".into())
                    }
                    Some(d) => self.supply_run(d, &short, &mut hints),
                }
            }
        }
        Step {
            title: "Supply the citadel",
            done,
            hints,
        }
    }

    /// What to press to get resources from the hideout store up to the citadel.
    fn supply_run(&self, d: &Vessel, short: &[(ItemType, u32)], hints: &mut Vec<String>) {
        let store = &self.hideout().store;
        match &d.script {
            Some(script) if script.running() => {
                hints.push(
                    "The dropper's exfil script is running supplies up on its own; wait for it."
                        .into(),
                );
                return;
            }
            Some(_) => {
                hints.push(
                    "Select the dropper. EXFIL SCRIPT: press 'Run supplies inside -> citadel' and it shuttles everything on its own.".into(),
                );
                return;
            }
            None if store.get(ItemType::ExfilScript) > 0 && d.berth() == Some(Berth::Planted) => {
                hints.push(
                    "Select the dropper. EXFIL SCRIPT: press 'Install an exfil script'.".into(),
                );
                return;
            }
            None => {}
        }
        let wanted = short.iter().max_by_key(|(_, n)| *n).map(|(i, _)| *i);
        match d.modules.first() {
            Some(Module::DataContainer(Some(_))) => match d.berth() {
                Some(Berth::Connected) => hints.push(
                    "Select the dropper. SLOT 1: press 'Unload' into the citadel store, then SEND 'Go inside' for the next load.".into(),
                ),
                Some(_) => hints.push("Select the dropper. SEND: press 'Go to the citadel'.".into()),
                None => hints.push("The dropper is on its way.".into()),
            },
            Some(Module::DataContainer(None)) if d.berth() == Some(Berth::Planted) => match wanted {
                Some(item) if store.get(item) > 0 => hints.push(format!(
                    "Select the dropper. SLOT 1: press 'Load {} {}', then SEND 'Go to the citadel'.",
                    store.get(item).min(Module::CONTAINER_CAPACITY),
                    text::item(item)
                )),
                Some(item) => hints.push(format!(
                    "The hideout store has no {} yet; the taps bring more every day.",
                    text::item(item)
                )),
                None => {}
            },
            Some(Module::DataContainer(None)) => hints.push(
                "Select the dropper. SEND: press 'Go inside' to load from the hideout store.".into(),
            ),
            _ if store.get(ItemType::DataContainer) > 0 => hints.push(
                "Select the dropper. SLOT 1: 'Remove pod' if one is fitted, then 'Fit data container' (it holds 250 units of one resource).".into(),
            ),
            _ => hints.push(self.build_hint(ItemType::DataContainer, false)),
        }
        if self.open(ItemType::ExfilScript) {
            hints.push(
                "Faster: an exfil script (research it, build it, 'Install an exfil script') lets the dropper run the route on its own.".into(),
            );
        }
    }

    fn assemble_worm(&self) -> Step {
        let citadel = &self.hideout().citadel;
        let done = self.worm().is_some();
        let mut hints = Vec::new();
        if !done {
            let parts = [ItemType::WormCore, ItemType::WormEngine];
            if !citadel.complete() {
                hints.push("Finish the citadel first.".into());
            } else if citadel.workshop.coders.is_none() && !citadel.workshop.automated {
                hints.push("Staff the citadel with coders first.".into());
            } else if parts.iter().all(|&p| citadel.store.get(p) > 0) {
                hints.push(format!(
                    "Click {} on the map. ASSEMBLE: press 'Assemble a worm at the citadel'.",
                    self.home()
                ));
            } else {
                for part in parts {
                    if citadel.store.get(part) == 0 {
                        hints.push(self.build_hint(part, true));
                    }
                }
            }
            hints.push(
                "The worm then needs a pilot and anonymisation from the citadel store: carry an operator team up in a session pod, and bandwidth and proxies for the citadel to refine.".into(),
            );
        }
        Step {
            title: "Build and assemble a worm",
            done,
            hints,
        }
    }

    fn claim_host(&self) -> Step {
        let me = self.status.player;
        let home = self.data.hideout.host;
        let done = self.status.view.hosts.iter().enumerate().any(|(h, host)| {
            h != usize::from(home.0) && host.controller == Some(Controller::Crew(me))
        });
        let mut hints = Vec::new();
        if !done {
            match self.worm() {
                None => hints.push("Build a worm first.".into()),
                Some(w) => {
                    let citadel = &self.hideout().citadel;
                    hints.push(format!(
                        "Select the worm: click it on the map, or click {} and press 'Select'.",
                        self.data.host(w.host).name
                    ));
                    let loaded = w.modules.iter().any(
                        |m| matches!(m, Module::ToolModule(Some(c)) if c.item == ItemType::CitadelModule),
                    );
                    if w.pilot.is_none() {
                        match citadel.staff.iter().find(|s| s.kind == StaffKind::Operator) {
                            Some(team) => {
                                hints.push(format!("CREW: press 'Pilot: {}'.", team.leader))
                            }
                            None => hints.push(
                                "CREW: the worm needs an operator team in the citadel: carry one up in the dropper's session pod ('Board', 'Go to the citadel', 'Disembark').".into(),
                            ),
                        }
                    }
                    if w.fuel < 50 {
                        hints.push(
                            "CREW: press 'Refuel'; the citadel refines proxy chains from the bandwidth and proxies in its store.".into(),
                        );
                    }
                    if !loaded {
                        if w.modules
                            .iter()
                            .any(|m| matches!(m, Module::ToolModule(None)))
                        {
                            if citadel.store.get(ItemType::CitadelModule) > 0 {
                                hints.push("SLOT: press 'Load 1 citadel module'.".into());
                            } else {
                                hints.push(
                                    "Get a citadel module into the citadel store: CITADEL 'Build citadel module', or carry one up in the dropper.".into(),
                                );
                            }
                        } else if citadel.store.get(ItemType::ToolModule) > 0 {
                            hints.push("SLOT 1: press 'Fit tool module'.".into());
                        } else {
                            hints.push(
                                "The citadel store needs a tool module: CITADEL 'Build tool module', or carry one up in the dropper.".into(),
                            );
                        }
                    } else if w.berth() == Some(Berth::Lurking) && w.host != home {
                        hints.push(
                            "OUTSIDE: press 'Install citadel module'. The host is yours from then on.".into(),
                        );
                    } else if w.pilot.is_some() && w.fuel > 0 {
                        hints.push(
                            "SEND: press 'Route to another host...' and click a free host (dim ring) on the map, ideally in the same network. The worm arrives outside it; press 'Install citadel module' there.".into(),
                        );
                    }
                    hints.push(
                        "Eight modules make a citadel there; two backdoor kits and taps make it extract. Red rings are the Legacy Net's hosts, which only force can take.".into(),
                    );
                }
            }
        }
        Step {
            title: "Claim a second host",
            done,
            hints,
        }
    }

    fn arm(&self) -> Step {
        let me = self.me();
        let player = self.status.player;
        let home = usize::from(self.data.hideout.host.0);
        let armed = self.mine().any(|v| v.c2 && v.daemons > 0)
            || self.hideout().citadel.store.get(ItemType::Daemon) > 0
            || self.status.view.hosts.iter().any(|h| {
                h.site
                    .as_ref()
                    .is_some_and(|s| s.citadel.store.get(ItemType::Daemon) > 0)
            });
        let done = me.war.is_some() || armed;
        let mut hints = Vec::new();
        if !done {
            let citadels = usize::from(self.hideout().citadel.complete())
                + self
                    .status
                    .view
                    .hosts
                    .iter()
                    .enumerate()
                    .filter(|(h, host)| {
                        *h != home
                            && host.controller == Some(Controller::Crew(player))
                            && host.site.as_ref().is_some_and(|s| s.citadel.complete())
                    })
                    .count();
            hints.push(format!(
                "The Legacy Net (red rings) declares war on a crew holding {CITADELS_FOR_WAR} complete citadels; you hold {citadels}. Its swarms then besiege the hottest crew's hosts and take them when the siege runs out."
            ));
            if !self.open(ItemType::Daemon) {
                hints.push(
                    "Daemons and C2 controllers open with the Legacy exploit: research it from a source fragment (a worm with a sniffer digs them up at a cache field), or wait for war to open them.".into(),
                );
            } else {
                for item in [ItemType::Daemon, ItemType::C2Controller] {
                    if !self.researched(item) {
                        hints.push(format!("RESEARCH: press '{}'.", text::item(item)));
                    }
                }
                if self.researched(ItemType::Daemon) {
                    hints.push(
                        "Build daemons in a citadel: up to 200 in its store fight as a garrison. Fit a worm with a C2 controller ('Install a C2 controller') and 'Load 50 daemons' to meet swarms; outside a red host, 'Attack the garrison' frees it.".into(),
                    );
                }
            }
        }
        Step {
            title: "Arm against the Legacy Net",
            done,
            hints,
        }
    }

    fn compete(&self) -> Step {
        Step {
            title: "Expand, defend and raid",
            done: false,
            hints: vec![
                format!(
                    "From turn {PROTECTION_TURNS} a worm with a C2 controller and daemons outside a rival's host can raid it: exfiltrate its store, plant a tap that siphons its extraction, or take the host. Every raid adds heat, and swarms go for the hottest crew."
                ),
                format!(
                    "Points: {CITADEL_POINTS} per complete citadel, {HOST_POINTS} per other host, {FREED_POINTS} per Legacy host freed, {TAKEN_POINTS} per host taken from a rival, {RESEARCH_POINTS} per item researched."
                ),
                "The game ends when one crew holds more than half of the home network's contested hosts, or on the last day. Most points wins.".into(),
            ],
        }
    }
}
