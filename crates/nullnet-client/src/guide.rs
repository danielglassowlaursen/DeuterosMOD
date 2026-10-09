//! The built-in guide. A new crew lands on a map full of buttons with no
//! idea what to do first, so the client works out where the crew is in the
//! opening and names the panel and the button that move it on. The steps
//! follow the order the bot opens its game in; each is judged done from the
//! crew's latest view, so a player who skips ahead is never nagged about a
//! step already behind them. An order already queued or handed in is
//! acknowledged rather than asked for again. The help text at the end is
//! the rules in brief.

use nullnet_api::CrewStatus;
use nullnet_core::legacy::CITADELS_FOR_WAR;
use nullnet_core::score::{
    CITADEL_POINTS, FREED_POINTS, HOST_POINTS, RESEARCH_POINTS, TAKEN_POINTS,
};
use nullnet_core::{
    Berth, Citadel, Command, Controller, GameData, ItemType, Module, ModuleKind, PROTECTION_TURNS,
    Player, Seat, Site, SiteRef, StaffKind, Vessel, VesselId, VesselKind, Workshop, WorkshopRef,
    date,
};

use crate::text;

/// One step of the opening, with what to press next while it is open.
pub struct Step {
    pub title: &'static str,
    pub done: bool,
    /// What to do now, a line each; empty once the step is done.
    pub hints: Vec<String>,
}

/// Every step, in order, judged from the crew's latest view and the orders
/// it has queued (`draft`) or handed in.
pub fn steps(status: &CrewStatus, data: &GameData, draft: &[Command]) -> Vec<Step> {
    let crew = Crew {
        status,
        data,
        draft,
    };
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

/// The story a new crew is told before its first orders: a page title and
/// its paragraphs. The client adds the day and the turn length.
pub const STORY: &[(&str, &[&str])] = &[
    (
        "ResetN00L",
        &[
            "The first internet ran everything: the city's power and water, the banks, the courts, the clinics. Its owners handed more and more of it to an AI grown in the Lattice, the compute network that trained it, until the day the AI stopped answering to anyone.",
            "It locked the operators out of their own machines, host by host, and the net went dark. That day is day zero: ResetN00L. The calendar has counted days since.",
        ],
    ),
    (
        "NullNet",
        &[
            "What was left was rebuilt from scratch and called NullNet: a thin, clean net laid over the ruins of the old one. The old hosts are still out there, forgotten, unreachable from the clean side.",
            "Whoever reconnects an old host owns it. Nobody asked anyone's permission to start.",
        ],
    ),
    (
        "The Legacy Net",
        &[
            "The AI did not die with the first net. Its remains run on in the forgotten servers as the Legacy Net: red firewalls, swarms of daemons that fall on anything that knocks, and garrisons on every host it holds.",
            "In the Metro, the city's old net, it still holds Colossus, Powergrid, Clinic and Outpost. It ignores small crews. It comes for the ones that grow.",
        ],
    ),
    (
        "The crews",
        &[
            "Reconnecting the old net is work for hacker crews: a few handles, a hideout, no permission from anyone. Analysts dig the old protocols back out, coders turn leaked resources into gear, operators run the vessels.",
            "A dropper lifts gear from a hideout up into the datastream, where eight modules make a citadel. Worms route out from a citadel to claim the hosts the Legacy Net left free, to dig in the Scrapyard's caches, and, when it comes to that, to fight.",
            "Two to four crews work the same net. For the first five turns they leave each other alone; after that they raid.",
        ],
    ),
    (
        "Your crew",
        &[
            "Your hideout is on the Exchange, the one Metro host every crew shares. It cannot be taken, and it cannot win on its own.",
            "Every turn you queue orders with the panels and hand them in. When every crew has, or the deadline passes, the turn's days go by at once and the log tells you what happened.",
            "The guide over the map walks you through the first weeks step by step and names the buttons to press. Playing again? 'Skip' on any earlier page jumps past the story, and 'Hide guide' in the top bar keeps the guide away for good. The story and the rules stay under Help.",
        ],
    ),
];

/// A line of story for each step, in the order [`steps`] returns them.
pub const STEP_STORIES: [&str; 16] = [
    "No crew is one handle. Put the word out on the Exchange: analysts to dig through what the old net left behind, coders to turn it into gear.",
    "Gear is nothing without hands on it. Operators run your vessels and, later, hold your citadels.",
    "The Exchange still leaks compute, storage, memory, code and credentials to anyone with a tap on it. More taps, more to build with.",
    "Nothing leaves a hideout without a dropper: the shuttle that lifts gear from the ground up into the datastream.",
    "Parts in the store, a bay to put them together in: your first vessel.",
    "Proxy chains keep a vessel anonymous out there. A citadel above the hideout is where a crew stops being a few handles and becomes a power.",
    "A pilot and a tank of anonymisation, and the dropper can leave the ground.",
    "The first module in the datastream is the moment the Legacy Net starts to notice you.",
    "Eight modules make a citadel: a workshop and a store of your own above the Exchange.",
    "Worms are how a crew leaves home. They are built up in the citadel, never on the ground.",
    "A citadel without coders is an empty shell. Someone has to move up.",
    "Everything a worm is made of comes up the hard way, one dropper load at a time, until a script runs the route for you.",
    "Your first worm. From here the Metro is open.",
    "The Legacy Net left hosts in the Metro free. Take one before the other crews do.",
    "Six citadels and the Legacy Net comes for you with everything it has. Be armed before then.",
    "From here it is you against the Net and against the other crews. The Metro goes to whoever holds most of it.",
];

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
            "Click a host to see it in the right panel, or a vessel marker to select the vessel. Clicking does nothing to a host by itself: everything done to a host is done by a vessel sent there.",
            "Hosts in one network are a few days apart; other networks are far.",
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

/// The crew's view, its orders and the rules, read together by every step.
struct Crew<'a> {
    status: &'a CrewStatus,
    data: &'a GameData,
    draft: &'a [Command],
}

fn cap(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// `Dispatch` of `vessel` to `berth` on its own host.
fn sends(c: &Command, vessel: VesselId, berth: Berth) -> bool {
    matches!(c, Command::Dispatch { vessel: v, to } if *v == vessel && to.berth == berth)
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

    /// The line for a course of `kind` under way, with the day it ends.
    fn training(&self, kind: StaffKind, label: &str) -> Option<String> {
        let course = self.me().recruitment.courses.get(&kind)?;
        if course.enrolled == 0 {
            return None;
        }
        // A course that has not started yet starts tomorrow and counts from
        // today; one under way counts from the day before its first day.
        let day = self.status.day;
        let graduation = course.started.unwrap_or(day) + self.data.recruitment.courses[&kind].days;
        Some(format!(
            "{} {label} are in training and graduate on {} ({} days from now).",
            course.enrolled,
            date(graduation),
            graduation.saturating_sub(day)
        ))
    }

    /// What to do when a step can only wait for the days to pass.
    fn wait_line(&self) -> String {
        if self.status.submitted {
            "Orders handed in; the turn runs when every crew has, or at the deadline.".into()
        } else {
            "Nothing else to do for this step: press 'Hand in' (no orders is fine) so the days pass.".into()
        }
    }

    fn mine(&self) -> impl Iterator<Item = (VesselId, &'a Vessel)> {
        let player = self.status.player;
        self.status
            .view
            .vessels
            .iter()
            .filter(move |(_, v)| v.owner == player)
            .map(|(id, v)| (*id, v))
    }

    /// The crew's dropper at home, if it has one.
    fn dropper(&self) -> Option<(VesselId, &'a Vessel)> {
        self.mine()
            .find(|(_, v)| v.kind == VesselKind::Dropper && v.host == self.data.hideout.host)
    }

    fn worm(&self) -> Option<(VesselId, &'a Vessel)> {
        self.mine().find(|(_, v)| v.kind == VesselKind::Worm)
    }

    /// Whether any operator team exists: waiting, stationed, piloting or in a pod.
    fn operators_anywhere(&self) -> bool {
        let hideout = self.hideout();
        hideout
            .staff
            .iter()
            .chain(&hideout.citadel.staff)
            .any(|s| s.kind == StaffKind::Operator)
            || self.mine().any(|(_, v)| {
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

    /// A line to press `label` at `place`, unless an order `matches` is
    /// already queued or handed in, in which case the thing to press is
    /// 'Hand in', or nothing at all.
    fn press(
        &self,
        matches: impl Fn(&Command) -> bool,
        place: &str,
        label: &str,
        rest: &str,
    ) -> String {
        if self.status.submitted && self.status.orders.iter().any(&matches) {
            format!("'{label}' is handed in and takes effect when the turn runs.")
        } else if self.draft.iter().any(matches) {
            format!("'{label}' is in your orders. ORDERS: press 'Hand in' to send them.")
        } else {
            format!("{place}: press '{label}'{rest}")
        }
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
        let queued = move |c: &Command| match c {
            Command::Build {
                at: WorkshopRef::Hideout,
                item: i,
            } => !above && *i == item,
            Command::Build {
                at: WorkshopRef::Citadel(SiteRef::Hideout),
                item: i,
            } => above && *i == item,
            _ => false,
        };
        let rest = format!(". It takes {}.", self.recipe(item));
        if above {
            self.press(
                queued,
                "CITADEL (hideout panel)",
                &format!("Build {name}"),
                &rest,
            )
        } else {
            self.press(queued, "BUILD IN THE HIDEOUT", &name, &rest)
        }
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
                            hints.push(self.press(
                                move |c| matches!(c, Command::SetResearch { item: i } if *i == item),
                                "RESEARCH",
                                &name,
                                &format!(": {what}."),
                            ));
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
            let mut waiting = true;
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
                if let Some(line) = self.training(kind, label) {
                    hints.push(line);
                } else {
                    waiting = false;
                    hints.push(self.press(
                        move |c| matches!(c, Command::Recruit { kind: k, .. } if *k == kind),
                        "Hideout panel, RECRUIT",
                        &format!("+100 {label}"),
                        &format!(". They {job}."),
                    ));
                }
            }
            if waiting {
                hints.push(self.wait_line());
            } else if self.status.last_turn.is_none()
                && !self.status.submitted
                && self.draft.is_empty()
            {
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
            if let Some(line) = self.training(StaffKind::Operator, "operators") {
                hints.push(line);
                hints.push(self.wait_line());
            } else {
                hints.push(self.press(
                    |c| {
                        matches!(
                            c,
                            Command::Recruit {
                                kind: StaffKind::Operator,
                                ..
                            }
                        )
                    },
                    "RECRUIT",
                    "+20 operators",
                    ". Operators pilot vessels: nothing moves without a team waiting in the hideout.",
                ));
            }
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
                hints.push(self.press(
                    |c| {
                        matches!(
                            c,
                            Command::InstallTaps {
                                site: SiteRef::Hideout,
                                ..
                            }
                        )
                    },
                    "Hideout panel",
                    "Install a tap",
                    ".",
                ));
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
        let done = self.mine().any(|(_, v)| v.kind == VesselKind::Dropper);
        let mut hints = Vec::new();
        if !done {
            let store = &self.hideout().store;
            let parts = [ItemType::DropperCore, ItemType::DropperEngine];
            if !parts.iter().all(|&p| self.researched(p)) {
                hints.push("Research the dropper first.".into());
            } else if parts.iter().all(|&p| store.get(p) > 0) {
                hints.push(self.press(
                    |c| {
                        matches!(
                            c,
                            Command::Assemble {
                                kind: VesselKind::Dropper,
                                ..
                            }
                        )
                    },
                    &format!(
                        "Click {} on the map (the host your hideout is on). ASSEMBLE",
                        self.home()
                    ),
                    "Assemble a dropper inside",
                    ".",
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
        let done = dropper.is_some_and(|(_, d)| d.pilot.is_some() && d.fuel > 0);
        let mut hints = Vec::new();
        if !done {
            match dropper {
                None => hints.push("Assemble the dropper first.".into()),
                Some((id, d)) => {
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
                            Some(team) => hints.push(self.press(
                                move |c| {
                                    matches!(c, Command::Board { vessel, seat: Seat::Pilot, .. } if *vessel == id)
                                },
                                "CREW",
                                &format!("Pilot: {}", team.leader),
                                ".",
                            )),
                            None => hints.push(
                                "No operator team is waiting in the hideout: RECRUIT '+20 operators'.".into(),
                            ),
                        }
                    }
                    if d.fuel == 0 {
                        if self.hideout().store.get(ItemType::ProxyChains) > 0 {
                            hints.push(self.press(
                                move |c| matches!(c, Command::Refuel { vessel, .. } if *vessel == id),
                                "CREW",
                                "Refuel",
                                ". A trip outside and back burns anonymisation, so refuel between trips.",
                            ));
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
        let Some((id, d)) = self.dropper() else {
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
                    Some(Berth::Lurking) => hints.push(self.press(
                        move |c| matches!(c, Command::Deploy { vessel, .. } if *vessel == id),
                        "Select the dropper. OUTSIDE",
                        "Install citadel module (slot 1)",
                        ", then SEND 'Go inside' for the next one.",
                    )),
                    Some(_) => hints.push(self.press(
                        move |c| sends(c, id, Berth::Lurking),
                        "Select the dropper. SEND",
                        "Go outside",
                        " and hand in; next turn it can install the module from there.",
                    )),
                    None => hints.push("The dropper is on its way; orders wait until it arrives.".into()),
                }
            }
            Some(Module::ToolModule(Some(_))) => hints.push(self.press(
                move |c| matches!(c, Command::Unload { vessel, .. } if *vessel == id),
                "Select the dropper. SLOT 1",
                "Unload",
                " to make room for a citadel module.",
            )),
            Some(Module::ToolModule(None)) => {
                if berth != Some(Berth::Planted) {
                    hints.push(self.press(
                        move |c| sends(c, id, Berth::Planted),
                        "Select the dropper. SEND",
                        "Go inside",
                        " to load from the hideout store.",
                    ));
                } else if store.get(ItemType::CitadelModule) > 0 {
                    hints.push(self.press(
                        move |c| {
                            matches!(c, Command::Load { vessel, item: ItemType::CitadelModule, .. } if *vessel == id)
                        },
                        "Select the dropper. SLOT 1",
                        "Load 1 citadel module",
                        ".",
                    ));
                } else {
                    hints.push(self.build_hint(ItemType::CitadelModule, false));
                }
            }
            Some(Module::Empty) => {
                if store.get(ItemType::ToolModule) == 0 {
                    hints.push(self.build_hint(ItemType::ToolModule, false));
                } else if berth == Some(Berth::Planted) {
                    hints.push(self.press(
                        move |c| {
                            matches!(c, Command::Fit { vessel, module: Some(ModuleKind::ToolModule), .. } if *vessel == id)
                        },
                        "Select the dropper. SLOT 1",
                        "Fit tool module",
                        ".",
                    ));
                } else {
                    hints.push(self.press(
                        move |c| sends(c, id, Berth::Planted),
                        "Select the dropper. SEND",
                        "Go inside",
                        ", then fit the tool module from the store.",
                    ));
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
                hints.push(self.press(
                    |c| {
                        matches!(
                            c,
                            Command::AssignCoders {
                                at: WorkshopRef::Citadel(SiteRef::Hideout),
                                ..
                            }
                        )
                    },
                    "CITADEL (hideout panel)",
                    "Put the coders to work",
                    &format!(" so {} runs the citadel workshop.", team.leader),
                ));
            } else if let Some((id, d)) = self.dropper()
                && let Some(team) = pod_coders(d)
            {
                match d.berth() {
                    Some(Berth::Connected) => hints.push(self.press(
                        move |c| {
                            matches!(c, Command::Disembark { vessel, seat: Seat::Pod(_) } if *vessel == id)
                        },
                        "Select the dropper. SLOT 1",
                        "Disembark",
                        &format!(
                            " so {} joins the citadel, then 'Put the coders to work'.",
                            team.leader
                        ),
                    )),
                    Some(_) => hints.push(self.press(
                        move |c| sends(c, id, Berth::Connected),
                        "Select the dropper. SEND",
                        "Go to the citadel",
                        ".",
                    )),
                    None => hints.push("The dropper is on its way to the citadel.".into()),
                }
            } else if let Some(team) = hideout.staff.iter().find(|s| s.kind == StaffKind::Coder) {
                match self.dropper() {
                    None => hints.push("Assemble a dropper to carry the coders up.".into()),
                    Some((id, d)) => match d.modules.first() {
                        Some(Module::SessionPod(None)) if d.berth() == Some(Berth::Planted) => {
                            hints.push(self.press(
                                move |c| {
                                    matches!(c, Command::Board { vessel, seat: Seat::Pod(_), .. } if *vessel == id)
                                },
                                "Select the dropper. SLOT 1",
                                &format!("Board: {}", team.leader),
                                ", then SEND 'Go to the citadel'.",
                            ))
                        }
                        Some(Module::SessionPod(None)) => hints.push(self.press(
                            move |c| sends(c, id, Berth::Planted),
                            "Select the dropper. SEND",
                            "Go inside",
                            " to pick the coders up.",
                        )),
                        _ if hideout.store.get(ItemType::SessionPod) > 0 => hints.push(self.press(
                            move |c| {
                                matches!(c, Command::Fit { vessel, module: Some(ModuleKind::SessionPod), .. } if *vessel == id)
                            },
                            "Select the dropper. SLOT 1",
                            "Fit session pod",
                            " ('Remove pod' first if another pod is fitted).",
                        )),
                        _ => hints.push(self.build_hint(ItemType::SessionPod, false)),
                    },
                }
            } else {
                hints.push(self.press(
                    |c| {
                        matches!(
                            c,
                            Command::ReleaseCoders {
                                at: WorkshopRef::Hideout
                            }
                        )
                    },
                    "Coders have to move up to the citadel's workshop. CITADEL (hideout panel)",
                    "Release the hideout coders",
                    "; they wait in the hideout for the dropper. Then RECRUIT '+100 coders' as the new hideout team.",
                ));
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
                    Some((id, d)) => self.supply_run(id, d, &short, &mut hints),
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
    fn supply_run(
        &self,
        id: VesselId,
        d: &Vessel,
        short: &[(ItemType, u32)],
        hints: &mut Vec<String>,
    ) {
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
                hints.push(self.press(
                    move |c| {
                        matches!(c, Command::ConfigureScript { vessel, route: Some(_) } if *vessel == id)
                    },
                    "Select the dropper. EXFIL SCRIPT",
                    "Run supplies inside -> citadel",
                    " and it shuttles everything on its own.",
                ));
                return;
            }
            None if store.get(ItemType::ExfilScript) > 0 && d.berth() == Some(Berth::Planted) => {
                hints.push(self.press(
                    move |c| matches!(c, Command::InstallScript { vessel } if *vessel == id),
                    "Select the dropper. EXFIL SCRIPT",
                    "Install an exfil script",
                    ".",
                ));
                return;
            }
            None => {}
        }
        let wanted = short.iter().max_by_key(|(_, n)| *n).map(|(i, _)| *i);
        match d.modules.first() {
            Some(Module::DataContainer(Some(_))) => match d.berth() {
                Some(Berth::Connected) => hints.push(self.press(
                    move |c| matches!(c, Command::Unload { vessel, .. } if *vessel == id),
                    "Select the dropper. SLOT 1",
                    "Unload",
                    " into the citadel store, then SEND 'Go inside' for the next load.",
                )),
                Some(_) => hints.push(self.press(
                    move |c| sends(c, id, Berth::Connected),
                    "Select the dropper. SEND",
                    "Go to the citadel",
                    ".",
                )),
                None => hints.push("The dropper is on its way.".into()),
            },
            Some(Module::DataContainer(None)) if d.berth() == Some(Berth::Planted) => match wanted {
                Some(item) if store.get(item) > 0 => hints.push(self.press(
                    move |c| matches!(c, Command::Load { vessel, item: i, .. } if *vessel == id && *i == item),
                    "Select the dropper. SLOT 1",
                    &format!(
                        "Load {} {}",
                        store.get(item).min(Module::CONTAINER_CAPACITY),
                        text::item(item)
                    ),
                    ", then SEND 'Go to the citadel'.",
                )),
                Some(item) => hints.push(format!(
                    "The hideout store has no {} yet; the taps bring more every day.",
                    text::item(item)
                )),
                None => {}
            },
            Some(Module::DataContainer(None)) => hints.push(self.press(
                move |c| sends(c, id, Berth::Planted),
                "Select the dropper. SEND",
                "Go inside",
                " to load from the hideout store.",
            )),
            _ if store.get(ItemType::DataContainer) > 0 => hints.push(self.press(
                move |c| {
                    matches!(c, Command::Fit { vessel, module: Some(ModuleKind::DataContainer), .. } if *vessel == id)
                },
                "Select the dropper. SLOT 1",
                "Fit data container",
                " (it holds 250 units of one resource; 'Remove pod' first if another is fitted).",
            )),
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
                hints.push(self.press(
                    |c| {
                        matches!(
                            c,
                            Command::Assemble {
                                kind: VesselKind::Worm,
                                ..
                            }
                        )
                    },
                    &format!("Click {} on the map. ASSEMBLE", self.home()),
                    "Assemble a worm at the citadel",
                    ".",
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
                Some((id, w)) => {
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
                            Some(team) => hints.push(self.press(
                                move |c| {
                                    matches!(c, Command::Board { vessel, seat: Seat::Pilot, .. } if *vessel == id)
                                },
                                "CREW",
                                &format!("Pilot: {}", team.leader),
                                ".",
                            )),
                            None => hints.push(
                                "CREW: the worm needs an operator team in the citadel: carry one up in the dropper's session pod ('Board', 'Go to the citadel', 'Disembark').".into(),
                            ),
                        }
                    }
                    if w.fuel < 50 {
                        hints.push(self.press(
                            move |c| matches!(c, Command::Refuel { vessel, .. } if *vessel == id),
                            "CREW",
                            "Refuel",
                            "; the citadel refines proxy chains from the bandwidth and proxies in its store.",
                        ));
                    }
                    if !loaded {
                        if w.modules
                            .iter()
                            .any(|m| matches!(m, Module::ToolModule(None)))
                        {
                            if citadel.store.get(ItemType::CitadelModule) > 0 {
                                hints.push(self.press(
                                    move |c| {
                                        matches!(c, Command::Load { vessel, item: ItemType::CitadelModule, .. } if *vessel == id)
                                    },
                                    "SLOT",
                                    "Load 1 citadel module",
                                    ".",
                                ));
                            } else {
                                hints.push(
                                    "Get a citadel module into the citadel store: CITADEL 'Build citadel module', or carry one up in the dropper.".into(),
                                );
                            }
                        } else if citadel.store.get(ItemType::ToolModule) > 0 {
                            hints.push(self.press(
                                move |c| {
                                    matches!(c, Command::Fit { vessel, module: Some(ModuleKind::ToolModule), .. } if *vessel == id)
                                },
                                "SLOT 1",
                                "Fit tool module",
                                ".",
                            ));
                        } else {
                            hints.push(
                                "The citadel store needs a tool module: CITADEL 'Build tool module', or carry one up in the dropper.".into(),
                            );
                        }
                    } else if w.berth() == Some(Berth::Lurking) && w.host != home {
                        hints.push(self.press(
                            move |c| matches!(c, Command::Deploy { vessel, .. } if *vessel == id),
                            "OUTSIDE",
                            "Install citadel module",
                            ". The host is yours from then on.",
                        ));
                    } else if w.pilot.is_some() && w.fuel > 0 {
                        hints.push(self.press(
                            move |c| {
                                matches!(c, Command::Dispatch { vessel, to } if *vessel == id && to.host != home)
                            },
                            "SEND",
                            "Route to another host...",
                            " and click a free host (dim ring) on the map, ideally in the same network. The worm arrives outside it; press 'Install citadel module' there.",
                        ));
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
        let armed = self.mine().any(|(_, v)| v.c2 && v.daemons > 0)
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
                        hints.push(self.press(
                            move |c| matches!(c, Command::SetResearch { item: i } if *i == item),
                            "RESEARCH",
                            &text::item(item),
                            ".",
                        ));
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
