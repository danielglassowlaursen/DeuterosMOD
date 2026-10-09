//! The client's line to the server: the crew's token, the status it polls
//! for, and the orders it hands in. Requests run off the main thread (in
//! the browser, on its event loop) and answer through a channel that a
//! system drains every frame.

use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};

use bevy::prelude::*;
use nullnet_api::{CrewStatus, OrdersReceipt, crew_path};
use nullnet_core::{Citadel, Command, Event, ItemType, PlayerId};

use crate::Rules;
use crate::notify;
use crate::sound::{Cue, Play};
use crate::text;
use crate::ui::Toast;

/// Seconds between status polls while nothing else is going on.
const POLL_SECONDS: f32 = 15.0;

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = channel();
        app.insert_resource(Api::from_environment())
            .insert_resource(Inbox {
                tx,
                rx: Mutex::new(rx),
            })
            .init_resource::<Session>()
            .init_resource::<Clock>()
            .add_systems(Startup, connect)
            .add_systems(Update, (receive, poll));
    }
}

/// Where the server is and who we are.
#[derive(Resource, Clone, Debug)]
pub struct Api {
    /// The server's origin, or empty for the page's own.
    pub base: String,
    pub token: Option<String>,
}

impl Api {
    /// In the browser the token comes from the invite link the page was
    /// opened with (`/join/<token>`, or `#<token>`); natively from
    /// `--server URL --token TOKEN`.
    fn from_environment() -> Api {
        #[cfg(target_arch = "wasm32")]
        {
            let location = web_sys::window().map(|w| w.location());
            let path = location
                .as_ref()
                .and_then(|l| l.pathname().ok())
                .unwrap_or_default();
            let hash = location
                .as_ref()
                .and_then(|l| l.hash().ok())
                .unwrap_or_default();
            let token = path
                .strip_prefix("/join/")
                .map(str::to_string)
                .or_else(|| hash.strip_prefix('#').map(str::to_string))
                .filter(|t| !t.is_empty());
            Api {
                base: String::new(),
                token,
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut api = Api {
                base: "http://localhost:8080".into(),
                token: None,
            };
            let mut args = std::env::args().skip(1);
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--server" => api.base = args.next().unwrap_or_default(),
                    "--token" => api.token = args.next(),
                    _ => {}
                }
            }
            api
        }
    }

    fn crew_url(&self, suffix: &str) -> Option<String> {
        let token = self.token.as_ref()?;
        Some(format!("{}{}{suffix}", self.base, crew_path(token)))
    }
}

/// What the client knows about its crew's game.
#[derive(Resource, Default)]
pub struct Session {
    pub status: Option<CrewStatus>,
    /// Orders being put together for the running turn.
    pub draft: Vec<Command>,
    /// What the server said about the last hand-in.
    pub receipt: Option<OrdersReceipt>,
    /// A line for the player: a turn ran, orders went in, and so on.
    pub notice: Option<String>,
    pub error: Option<String>,
    /// A request is in flight.
    pub busy: bool,
}

impl Session {
    /// Seconds until the running turn's deadline, from the last status.
    pub fn seconds_left(&self, clock: &Clock) -> Option<i64> {
        let status = self.status.as_ref()?;
        Some(status.deadline - status.now - clock.since_fetch as i64)
    }
}

/// Time since the last status arrived and since the last poll, kept apart
/// from [`Session`] so ticking does not count as a change.
#[derive(Resource, Default)]
pub struct Clock {
    pub since_fetch: f32,
    since_poll: f32,
}

enum Reply {
    Status(Result<Box<CrewStatus>, String>),
    Receipt(Result<OrdersReceipt, String>),
    Withdrawn(Result<(), String>),
}

#[derive(Resource)]
pub struct Inbox {
    tx: Sender<Reply>,
    rx: Mutex<Receiver<Reply>>,
}

fn error_text(response: &ehttp::Response) -> String {
    response
        .json::<nullnet_api::ApiError>()
        .map(|e| e.error)
        .unwrap_or_else(|_| format!("{} {}", response.status, response.status_text))
}

fn fetch<T, F>(request: ehttp::Request, tx: Sender<Reply>, wrap: F)
where
    T: serde::de::DeserializeOwned + Send + 'static,
    F: Fn(Result<T, String>) -> Reply + Send + 'static,
{
    ehttp::fetch(request, move |result| {
        let reply = match result {
            Ok(response) if response.ok => response
                .json::<T>()
                .map_err(|e| format!("bad answer from the server: {e}")),
            Ok(response) => Err(error_text(&response)),
            Err(error) => Err(error),
        };
        let _ = tx.send(wrap(reply));
    });
}

pub fn fetch_status(api: &Api, inbox: &Inbox, session: &mut Session) {
    let Some(url) = api.crew_url("") else {
        return;
    };
    session.busy = true;
    fetch(ehttp::Request::get(url), inbox.tx.clone(), |r| {
        Reply::Status(r.map(Box::new))
    });
}

pub fn hand_in(api: &Api, inbox: &Inbox, session: &mut Session, orders: &[Command]) {
    let Some(url) = api.crew_url("/orders") else {
        return;
    };
    let Ok(request) = ehttp::Request::put_json(url, &orders) else {
        session.error = Some("the orders could not be encoded".into());
        return;
    };
    session.busy = true;
    session.error = None;
    fetch(request, inbox.tx.clone(), Reply::Receipt);
}

pub fn withdraw(api: &Api, inbox: &Inbox, session: &mut Session) {
    let Some(url) = api.crew_url("/orders") else {
        return;
    };
    session.busy = true;
    session.error = None;
    let tx = inbox.tx.clone();
    ehttp::fetch(ehttp::Request::delete(&url), move |result| {
        let reply = match result {
            Ok(response) if response.ok => Ok(()),
            Ok(response) => Err(error_text(&response)),
            Err(error) => Err(error),
        };
        let _ = tx.send(Reply::Withdrawn(reply));
    });
}

fn connect(api: Res<Api>, inbox: Res<Inbox>, mut session: ResMut<Session>) {
    if api.token.is_some() {
        fetch_status(&api, &inbox, &mut session);
    }
}

/// Whether an event is worth a line in the toast of the turn that ran.
fn notable(event: &Event, me: PlayerId) -> bool {
    match event {
        Event::HostClaimed { player, .. } => *player == me,
        Event::Installed {
            item: ItemType::CitadelModule,
            installed,
            ..
        } => *installed == Citadel::MODULES,
        Event::ResearchCompleted { .. }
        | Event::Unlocked { .. }
        | Event::VesselBurned { .. }
        | Event::VesselStopped { .. }
        | Event::WarDeclared { .. }
        | Event::FleetSighted { .. }
        | Event::UnderAttack { .. }
        | Event::AttackRepelled { .. }
        | Event::HostCaptured { .. }
        | Event::HostFreed { .. }
        | Event::BattleFought { .. }
        | Event::VesselLost { .. }
        | Event::FragmentFound { .. }
        | Event::Raid { .. }
        | Event::HostTaken { .. }
        | Event::GameOver { .. } => true,
        _ => false,
    }
}

/// The cue for the turn that ran: an alarm when the crew is under threat,
/// a fanfare when the game is over, a chime otherwise.
fn cue_for(events: &[Event], me: PlayerId) -> Cue {
    if events.iter().any(|e| matches!(e, Event::GameOver { .. })) {
        return Cue::GameOver;
    }
    let threatened = events.iter().any(|e| match e {
        Event::FleetSighted { player, .. }
        | Event::UnderAttack { player, .. }
        | Event::HostCaptured { player, .. } => *player == me,
        Event::Raid { defender, .. } | Event::HostTaken { from: defender, .. } => *defender == me,
        _ => false,
    });
    if threatened { Cue::Alarm } else { Cue::TurnRan }
}

/// Tells the player a turn has run: a toast over the map, a cue, and a
/// browser notification when the tab is in the background.
fn announce(
    status: &CrewStatus,
    previous: u32,
    rules: &Rules,
    toast: &mut Toast,
    play: &mut MessageWriter<Play>,
) {
    let mut lines = vec![format!(
        "Turn {previous} has run. It is now {}.",
        nullnet_core::date(status.day)
    )];
    let events: &[Event] = status
        .last_turn
        .as_ref()
        .map_or(&[], |t| t.report.events.as_slice());
    let me = status.player;
    lines.extend(events.iter().filter(|e| notable(e, me)).take(8).map(|e| {
        format!(
            "{}  {}",
            nullnet_core::date(e.day()),
            text::event(e, &rules.0, &status.view.vessels)
        )
    }));
    let cue = cue_for(events, me);
    play.write(Play(cue));
    let body = lines[1..].join("\n");
    notify::notify(
        &format!("NullNet: turn {previous} of {} has run", status.game.name),
        if body.is_empty() {
            "Your orders are open."
        } else {
            &body
        },
    );
    toast.show(lines);
}

fn receive(
    api: Res<Api>,
    inbox: Res<Inbox>,
    rules: Res<Rules>,
    mut session: ResMut<Session>,
    mut clock: ResMut<Clock>,
    mut toast: ResMut<Toast>,
    mut play: MessageWriter<Play>,
) {
    let replies: Vec<Reply> = {
        let rx = inbox.rx.lock().unwrap_or_else(|p| p.into_inner());
        rx.try_iter().collect()
    };
    for reply in replies {
        match reply {
            Reply::Status(Ok(status)) => {
                let status = *status;
                let previous = session.status.as_ref().map(|s| s.turn);
                if let Some(previous) = previous
                    && status.turn != previous
                {
                    session.notice = Some(format!(
                        "Turn {previous} has run. It is now {}.",
                        nullnet_core::date(status.day)
                    ));
                    session.draft.clear();
                    session.receipt = None;
                    announce(&status, previous, &rules, &mut toast, &mut play);
                }
                if !status.submitted && session.draft.is_empty() && !status.orders.is_empty() {
                    session.draft = status.orders.clone();
                }
                session.status = Some(status);
                session.error = None;
                session.busy = false;
                clock.since_fetch = 0.0;
                clock.since_poll = 0.0;
            }
            Reply::Receipt(Ok(receipt)) => {
                session.notice = Some(if receipt.resolved {
                    "Everyone had handed in: the turn has run.".into()
                } else if receipt.rejected.is_empty() {
                    "Orders handed in. The turn runs when everyone has, or at the deadline.".into()
                } else {
                    "Orders handed in, but some would be refused as things stand.".into()
                });
                session.receipt = Some(receipt);
                fetch_status(&api, &inbox, &mut session);
            }
            Reply::Withdrawn(Ok(())) => {
                session.notice = Some("Orders withdrawn.".into());
                session.receipt = None;
                fetch_status(&api, &inbox, &mut session);
            }
            Reply::Status(Err(e)) | Reply::Receipt(Err(e)) | Reply::Withdrawn(Err(e)) => {
                session.error = Some(e);
                session.busy = false;
            }
        }
    }
}

fn poll(
    time: Res<Time>,
    api: Res<Api>,
    inbox: Res<Inbox>,
    mut session: ResMut<Session>,
    mut clock: ResMut<Clock>,
) {
    clock.since_fetch += time.delta_secs();
    clock.since_poll += time.delta_secs();
    if api.token.is_some() && !session.busy && clock.since_poll >= POLL_SECONDS {
        clock.since_poll = 0.0;
        fetch_status(&api, &inbox, &mut session);
    }
}
