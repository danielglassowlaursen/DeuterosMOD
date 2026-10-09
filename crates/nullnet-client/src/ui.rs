//! The terminal panels over the map: the top bar, the hideout, whatever is
//! selected, the orders being put together, and the log of the last turn.
//! Every panel is rebuilt from the crew's latest view whenever that view,
//! the selection or the draft orders change.

use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::ui::{FocusPolicy, RelativeCursorPosition};
use nullnet_api::CrewStatus;
use nullnet_core::{
    BattleReport, Berth, Command, Controller, Destination, EndReason, Event, GameData, HostId,
    ItemCategory, ItemType, Module, ModuleKind, PROTECTION_TURNS, RaidGoal, Route, Seat, Site,
    SiteRef, Staff, StaffKind, Vessel, VesselId, VesselKind, WorkshopRef, date,
};

use crate::Rules;
use crate::map::{Routing, Selected, crew_color};
use crate::net::{self, Api, Clock, Inbox, Session};
use crate::notify;
use crate::sound::{Cue, Play, Sounds};
use crate::text;

/// Seconds a battle replay takes from first to last snapshot.
const REPLAY_SECONDS: f32 = 5.0;
/// Seconds the turn toast stays up.
const TOAST_SECONDS: f32 = 14.0;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OverUi>()
            .init_resource::<Toast>()
            .init_resource::<Replay>()
            .add_systems(Startup, spawn_layout)
            .add_systems(
                Update,
                (
                    track_cursor,
                    press_buttons,
                    style_buttons,
                    scroll_panels,
                    countdown,
                    fade_toast,
                    animate_replay,
                    refresh.run_if(
                        resource_changed::<Session>
                            .or_else(resource_changed::<Selected>)
                            .or_else(resource_changed::<Routing>)
                            .or_else(resource_changed::<Toast>)
                            .or_else(resource_changed::<Replay>)
                            .or_else(resource_changed::<Sounds>),
                    ),
                )
                    .chain(),
            );
    }
}

/// What happened in the turn that just ran, shown for a while over the map.
#[derive(Resource, Default)]
pub struct Toast {
    pub lines: Vec<String>,
    pub seconds_left: f32,
}

impl Toast {
    pub fn show(&mut self, lines: Vec<String>) {
        self.lines = lines;
        self.seconds_left = TOAST_SECONDS;
    }
}

/// A battle being replayed from its course of snapshots.
#[derive(Resource, Default)]
pub struct Replay {
    pub current: Option<ReplayState>,
}

pub struct ReplayState {
    pub title: String,
    pub attacker: String,
    pub defender: String,
    pub report: BattleReport,
    pub elapsed: f32,
}

/// The bar of one side in the replay: `true` for the attacker.
#[derive(Component)]
struct ReplayBar(bool);

#[derive(Component)]
struct ReplayCount(bool);

#[derive(Component)]
struct ReplayOutcome;

/// Whether the cursor is over a panel, so the map leaves the click alone.
#[derive(Resource, Default)]
pub struct OverUi(pub bool);

/// A panel the cursor can be over and scroll.
#[derive(Component)]
struct Panel;

/// A container whose children are rebuilt on every refresh.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Slot {
    TopBar,
    Hideout,
    Selection,
    Orders,
    Log,
    Toast,
    Replay,
}

#[derive(Component)]
struct Countdown;

/// What a button does when pressed.
#[derive(Component, Clone)]
enum Action {
    Order(Command),
    Undo,
    Clear,
    HandIn,
    Withdraw,
    SelectVessel(VesselId),
    Route(VesselId),
    CancelRoute,
    ToggleMute,
    RequestNotify,
    /// Replays the battle in the last turn's event at this index.
    Replay(usize),
    CloseReplay,
}

const PANEL_BG: Color = Color::srgba(0.04, 0.07, 0.10, 0.94);
const BORDER: Color = Color::srgba(0.17, 0.25, 0.33, 1.0);
const BUTTON_BG: Color = Color::srgba(0.08, 0.13, 0.18, 1.0);
const BUTTON_HOVER: Color = Color::srgba(0.12, 0.2, 0.27, 1.0);
const FG: Color = Color::srgb(0.85, 0.9, 0.95);
const MUTED: Color = Color::srgba(0.6, 0.7, 0.8, 0.85);
const ACCENT: Color = Color::srgb(0.37, 0.79, 0.85);
const WARN: Color = Color::srgb(0.94, 0.55, 0.35);
const GOOD: Color = Color::srgb(0.48, 0.83, 0.54);

fn text(s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(s),
        TextFont::from_font_size(size),
        TextColor(color),
    )
}

fn line(p: &mut ChildSpawnerCommands, s: impl Into<String>) {
    p.spawn(text(s, 12.0, FG));
}

fn muted(p: &mut ChildSpawnerCommands, s: impl Into<String>) {
    p.spawn(text(s, 12.0, MUTED));
}

fn heading(p: &mut ChildSpawnerCommands, s: &str) {
    p.spawn((
        text(s.to_uppercase(), 11.0, MUTED),
        Node {
            margin: UiRect::top(Val::Px(6.0)),
            ..default()
        },
    ));
}

fn button(label: impl Into<String>, action: Action) -> impl Bundle {
    (
        Button,
        action,
        Node {
            padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(BUTTON_BG),
        BorderColor::all(BORDER),
        children![text(label, 12.0, ACCENT)],
    )
}

fn row() -> Node {
    Node {
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        column_gap: Val::Px(6.0),
        row_gap: Val::Px(6.0),
        align_items: AlignItems::Center,
        ..default()
    }
}

fn panel(node: Node, slot: Slot) -> impl Bundle {
    (
        Panel,
        slot,
        Node {
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(5.0),
            padding: UiRect::all(Val::Px(10.0)),
            border: UiRect::all(Val::Px(1.0)),
            overflow: Overflow::scroll_y(),
            ..node
        },
        ScrollPosition::default(),
        BackgroundColor(PANEL_BG),
        BorderColor::all(BORDER),
        RelativeCursorPosition::default(),
        FocusPolicy::Block,
    )
}

fn spawn_layout(mut commands: Commands) {
    commands.spawn((
        Panel,
        Slot::TopBar,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(0.0),
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            height: Val::Px(40.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(18.0),
            padding: UiRect::axes(Val::Px(16.0), Val::Px(0.0)),
            border: UiRect::bottom(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(PANEL_BG),
        BorderColor::all(BORDER),
        RelativeCursorPosition::default(),
        FocusPolicy::Block,
    ));
    commands.spawn(panel(
        Node {
            left: Val::Px(12.0),
            top: Val::Px(50.0),
            width: Val::Px(300.0),
            bottom: Val::Px(196.0),
            ..default()
        },
        Slot::Hideout,
    ));
    commands.spawn(panel(
        Node {
            right: Val::Px(12.0),
            top: Val::Px(50.0),
            width: Val::Px(300.0),
            bottom: Val::Px(196.0),
            ..default()
        },
        Slot::Selection,
    ));
    commands.spawn(panel(
        Node {
            left: Val::Px(12.0),
            bottom: Val::Px(12.0),
            width: Val::Percent(49.0),
            height: Val::Px(176.0),
            ..default()
        },
        Slot::Orders,
    ));
    commands.spawn(panel(
        Node {
            right: Val::Px(12.0),
            bottom: Val::Px(12.0),
            width: Val::Percent(49.0),
            height: Val::Px(176.0),
            ..default()
        },
        Slot::Log,
    ));
    commands.spawn((
        panel(
            Node {
                top: Val::Px(50.0),
                left: Val::Percent(24.0),
                right: Val::Percent(24.0),
                ..default()
            },
            Slot::Toast,
        ),
        Visibility::Hidden,
    ));
    commands.spawn((
        panel(
            Node {
                top: Val::Percent(22.0),
                left: Val::Percent(30.0),
                right: Val::Percent(30.0),
                row_gap: Val::Px(8.0),
                ..default()
            },
            Slot::Replay,
        ),
        Visibility::Hidden,
    ));
}

fn fade_toast(time: Res<Time>, mut toast: ResMut<Toast>) {
    if toast.seconds_left <= 0.0 {
        return;
    }
    // Ticking is not a change worth rebuilding the panels for; clearing is.
    let inner = toast.bypass_change_detection();
    inner.seconds_left -= time.delta_secs();
    if inner.seconds_left <= 0.0 {
        toast.lines.clear();
    }
}

/// Moves the replay's bars along the battle's course.
fn animate_replay(
    time: Res<Time>,
    mut replay: ResMut<Replay>,
    mut bars: Query<(&ReplayBar, &mut Node)>,
    mut counts: Query<(&ReplayCount, &mut Text)>,
    mut outcome: Query<&mut Visibility, With<ReplayOutcome>>,
) {
    let Some(state) = replay.bypass_change_detection().current.as_mut() else {
        return;
    };
    if state.elapsed >= REPLAY_SECONDS {
        return;
    }
    state.elapsed += time.delta_secs();
    let progress = (state.elapsed / REPLAY_SECONDS).min(1.0);
    let course = &state.report.course;
    let (attacker, defender) = if course.len() < 2 {
        let last = course.last().copied().unwrap_or((0, 0));
        (last.0 as f32, last.1 as f32)
    } else {
        let position = progress * (course.len() - 1) as f32;
        let index = (position.floor() as usize).min(course.len() - 2);
        let fraction = position - index as f32;
        let (a0, d0) = course[index];
        let (a1, d1) = course[index + 1];
        (
            a0 as f32 + (a1 as f32 - a0 as f32) * fraction,
            d0 as f32 + (d1 as f32 - d0 as f32) * fraction,
        )
    };
    let most = state
        .report
        .attacker
        .daemons
        .max(state.report.defender.daemons)
        .max(1) as f32;
    for (bar, mut node) in &mut bars {
        let left = if bar.0 { attacker } else { defender };
        node.width = Val::Percent(left / most * 100.0);
    }
    for (count, mut text) in &mut counts {
        let left = if count.0 { attacker } else { defender };
        let wanted = format!("{}", left.round() as u32);
        if text.0 != wanted {
            text.0 = wanted;
        }
    }
    if progress >= 1.0 {
        for mut visibility in &mut outcome {
            *visibility = Visibility::Inherited;
        }
    }
}

fn track_cursor(panels: Query<&RelativeCursorPosition, With<Panel>>, mut over: ResMut<OverUi>) {
    let now = panels.iter().any(|c| c.cursor_over);
    if over.0 != now {
        over.0 = now;
    }
}

fn scroll_panels(
    mut wheel: MessageReader<MouseWheel>,
    mut panels: Query<(&RelativeCursorPosition, &mut ScrollPosition), With<Panel>>,
) {
    for event in wheel.read() {
        let dy = match event.unit {
            MouseScrollUnit::Line => event.y * 24.0,
            MouseScrollUnit::Pixel => event.y,
        };
        for (cursor, mut scroll) in &mut panels {
            if cursor.cursor_over {
                scroll.0.y = (scroll.0.y - dy).max(0.0);
            }
        }
    }
}

type ChangedButtons = (Changed<Interaction>, With<Button>);

fn style_buttons(mut buttons: Query<(&Interaction, &mut BackgroundColor), ChangedButtons>) {
    for (interaction, mut color) in &mut buttons {
        color.0 = match interaction {
            Interaction::None => BUTTON_BG,
            Interaction::Hovered | Interaction::Pressed => BUTTON_HOVER,
        };
    }
}

#[allow(clippy::too_many_arguments)]
fn press_buttons(
    mouse: Res<ButtonInput<MouseButton>>,
    buttons: Query<(&Interaction, &Action), Changed<Interaction>>,
    api: Res<Api>,
    inbox: Res<Inbox>,
    mut session: ResMut<Session>,
    mut selected: ResMut<Selected>,
    mut routing: ResMut<Routing>,
    mut sounds: ResMut<Sounds>,
    mut play: MessageWriter<Play>,
    mut replay: ResMut<Replay>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    // The first click is what a browser wants before a page makes sound.
    if !sounds.unlocked {
        sounds.bypass_change_detection().unlocked = true;
    }
    for (interaction, action) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        play.write(Play(Cue::Click));
        match action.clone() {
            Action::Order(command) => {
                session.draft.push(command);
                session.notice = None;
            }
            Action::Undo => {
                session.draft.pop();
            }
            Action::Clear => session.draft.clear(),
            Action::HandIn => {
                if !session.busy {
                    let orders = session.draft.clone();
                    net::hand_in(&api, &inbox, &mut session, &orders);
                }
            }
            Action::Withdraw => {
                if !session.busy {
                    net::withdraw(&api, &inbox, &mut session);
                }
            }
            Action::SelectVessel(id) => {
                let host = session
                    .status
                    .as_ref()
                    .and_then(|s| s.view.vessels.get(&id))
                    .map(|v| v.host);
                *selected = Selected {
                    host,
                    vessel: Some(id),
                };
            }
            Action::Route(id) => routing.0 = Some(id),
            Action::CancelRoute => routing.0 = None,
            Action::ToggleMute => {
                sounds.muted = !sounds.muted;
                notify::remember_muted(sounds.muted);
            }
            Action::RequestNotify => notify::request_permission(),
            Action::Replay(index) => {
                if let Some(state) = replay_of(&session, index) {
                    replay.current = Some(state);
                    play.write(Play(Cue::Battle));
                }
            }
            Action::CloseReplay => replay.current = None,
        }
    }
}

/// The replay of the battle in the last turn's event at `index`.
fn replay_of(session: &Session, index: usize) -> Option<ReplayState> {
    let status = session.status.as_ref()?;
    let turn = status.last_turn.as_ref()?;
    let crew = |player: nullnet_core::PlayerId| {
        status
            .crews
            .iter()
            .find(|c| c.player == player)
            .map_or(format!("crew {}", player.0 + 1), |c| c.name.clone())
    };
    let host = |h: HostId| {
        status
            .view
            .hosts
            .get(usize::from(h.0))
            .map(|_| h)
            .map_or(String::new(), |_| format!("host {}", h.0))
    };
    let _ = host;
    match turn.report.events.get(index)? {
        Event::BattleFought {
            player,
            vessel,
            report,
            day,
            ..
        } => Some(ReplayState {
            title: format!("Battle on {}", date(*day)),
            attacker: match vessel {
                Some(id) => format!(
                    "{} ({})",
                    text::vessel(*id, &status.view.vessels),
                    crew(*player)
                ),
                None => format!("Garrison ({})", crew(*player)),
            },
            defender: "The Legacy Net".into(),
            report: report.clone(),
            elapsed: 0.0,
        }),
        Event::Raid {
            player,
            defender,
            report,
            day,
            goal,
            ..
        } => Some(ReplayState {
            title: format!("Raid on {} to {}", date(*day), text::raid_goal(*goal)),
            attacker: crew(*player),
            defender: format!("Garrison ({})", crew(*defender)),
            report: report.clone(),
            elapsed: 0.0,
        }),
        _ => None,
    }
}

fn countdown(
    session: Res<Session>,
    clock: Res<Clock>,
    mut texts: Query<&mut Text, With<Countdown>>,
) {
    let wanted = match session.seconds_left(&clock) {
        Some(left) if left <= 0 => "deadline passed".to_string(),
        Some(left) => format!("deadline in {}h {:02}m", left / 3600, (left % 3600) / 60),
        None => String::new(),
    };
    for mut text in &mut texts {
        if text.0 != wanted {
            text.0 = wanted.clone();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn refresh(
    mut commands: Commands,
    session: Res<Session>,
    selected: Res<Selected>,
    routing: Res<Routing>,
    api: Res<Api>,
    rules: Res<Rules>,
    toast: Res<Toast>,
    replay: Res<Replay>,
    sounds: Res<Sounds>,
    mut slots: Query<(Entity, &Slot, &mut Visibility)>,
) {
    let data = &rules.0;
    for (entity, slot, mut visibility) in &mut slots {
        let shown = match slot {
            Slot::Toast => !toast.lines.is_empty() && toast.seconds_left > 0.0,
            Slot::Replay => replay.current.is_some(),
            _ => true,
        };
        let wanted = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
        let mut e = commands.entity(entity);
        e.despawn_children();
        if !shown {
            continue;
        }
        e.with_children(|p| match slot {
            Slot::TopBar => top_bar(p, &session, &api, &sounds),
            Slot::Hideout => match &session.status {
                Some(status) => hideout_panel(p, status, data),
                None => muted(p, "Waiting for the server..."),
            },
            Slot::Selection => {
                if let Some(status) = &session.status {
                    selection_panel(p, status, data, &selected, &routing);
                }
            }
            Slot::Orders => orders_panel(p, &session, data),
            Slot::Log => {
                if let Some(status) = &session.status {
                    log_panel(p, status, data);
                }
            }
            Slot::Toast => {
                for (index, line) in toast.lines.iter().enumerate() {
                    if index == 0 {
                        p.spawn(text(line.clone(), 13.0, GOOD));
                    } else {
                        p.spawn(text(line.clone(), 12.0, FG));
                    }
                }
            }
            Slot::Replay => {
                if let Some(state) = &replay.current {
                    replay_panel(p, state);
                }
            }
        });
    }
}

fn replay_panel(p: &mut ChildSpawnerCommands, state: &ReplayState) {
    heading(p, &state.title);
    for (attacker, name, side) in [
        (true, &state.attacker, state.report.attacker),
        (false, &state.defender, state.report.defender),
    ] {
        line(
            p,
            format!(
                "{name}: {} daemons, operator level {}",
                side.daemons, side.level
            ),
        );
        p.spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(16.0),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(BORDER),
        ))
        .with_children(|bar| {
            bar.spawn((
                ReplayBar(attacker),
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(if attacker { ACCENT } else { WARN }),
            ));
        });
        p.spawn((
            text(format!("{}", side.daemons), 12.0, FG),
            ReplayCount(attacker),
        ));
    }
    p.spawn((
        text(
            format!(
                "{}: {} vs {} daemons left",
                text::outcome(state.report.outcome),
                state.report.attacker_left(),
                state.report.defender_left()
            ),
            12.0,
            GOOD,
        ),
        ReplayOutcome,
        Visibility::Hidden,
    ));
    p.spawn(row()).with_children(|r| {
        r.spawn(button("Close", Action::CloseReplay));
    });
}

// ------------------------------------------------------------ top bar

fn top_bar(p: &mut ChildSpawnerCommands, session: &Session, api: &Api, sounds: &Sounds) {
    p.spawn(text("NULLNET", 18.0, FG));
    p.spawn(button(
        if sounds.muted {
            "Sound: off"
        } else {
            "Sound: on"
        },
        Action::ToggleMute,
    ));
    if notify::permission() == notify::Permission::Ask {
        p.spawn(button("Notify me", Action::RequestNotify));
    }
    let Some(status) = &session.status else {
        if api.token.is_none() {
            muted(
                p,
                "No crew token. Open your invite link, or create a game at /console.",
            );
        } else if let Some(error) = &session.error {
            p.spawn(text(error.clone(), 12.0, WARN));
        } else {
            muted(p, "Connecting...");
        }
        return;
    };
    muted(p, status.game.name.clone());
    p.spawn(text(
        status.name.clone(),
        13.0,
        Color::Srgba(Srgba::hex(crew_color(status.player, status.player)).unwrap()),
    ));
    line(
        p,
        format!(
            "Turn {}   {}   {} days per turn",
            status.turn,
            date(status.day),
            status.game.turn_days
        ),
    );
    if let Some(end) = &status.view.ended {
        let winner = status
            .crews
            .iter()
            .find(|c| c.player == end.winner)
            .map_or("a crew".to_string(), |c| c.name.clone());
        p.spawn(text(
            format!(
                "GAME OVER on {}: {} wins {}",
                date(end.day),
                winner,
                match end.reason {
                    EndReason::Domination => "by holding most of the home network",
                    EndReason::DayLimit => "on points",
                }
            ),
            13.0,
            GOOD,
        ));
    } else {
        p.spawn((text("", 12.0, MUTED), Countdown));
        if let Some(end) = status.game.end_day {
            muted(p, format!("Last day {}", date(end)));
        }
    }
    if status.view.me.war.is_some() {
        p.spawn(text("AT WAR", 12.0, WARN));
    }
    if status.turn < PROTECTION_TURNS {
        muted(
            p,
            format!("Crews cannot raid each other before turn {PROTECTION_TURNS}"),
        );
    }
    for crew in &status.crews {
        let color = Color::Srgba(Srgba::hex(crew_color(status.player, crew.player)).unwrap());
        let summary = status.view.crews.iter().find(|c| c.player == crew.player);
        let held = summary.map_or(0, |c| c.hosts);
        let heat = summary.map_or(0, |c| c.heat);
        let points = status
            .view
            .scores
            .iter()
            .find(|s| s.player == crew.player)
            .map_or(0, |s| s.total);
        p.spawn(text(
            format!(
                "{}{} [{}] {} hosts  {} pts  heat {}",
                crew.name,
                if crew.bot { " (bot)" } else { "" },
                if crew.submitted { "x" } else { " " },
                held,
                points,
                heat
            ),
            12.0,
            color,
        ));
    }
}

// ------------------------------------------------------------ hideout

fn team_line(label: &str, staff: Option<&Staff>) -> String {
    match staff {
        Some(s) => format!("{label}: {}", text::team(s)),
        None => format!("{label}: none"),
    }
}

/// Items the crew can build somewhere, with what it has researched.
fn buildable(status: &CrewStatus, data: &GameData, orbit: bool) -> Vec<ItemType> {
    data.items
        .iter()
        .filter(|(item, def)| {
            def.category == ItemCategory::Item
                && !def.recipe.is_empty()
                && (orbit || !def.orbit_only)
                && status
                    .view
                    .me
                    .research
                    .get(item)
                    .is_some_and(|r| r.researched)
        })
        .map(|(&item, _)| item)
        .collect()
}

fn store_lines(p: &mut ChildSpawnerCommands, store: &nullnet_core::Store) {
    let items: Vec<(ItemType, u32)> = store.iter().collect();
    if items.is_empty() {
        muted(p, "nothing yet");
        return;
    }
    for (item, count) in items {
        line(p, format!("{} {count}", text::item(item)));
    }
}

fn hideout_panel(p: &mut ChildSpawnerCommands, status: &CrewStatus, data: &GameData) {
    let me = &status.view.me;
    let hideout = &me.hideout;
    heading(p, "Hideout");
    line(
        p,
        format!(
            "Taps {}/8   Citadel {}/8   Recruits {}",
            hideout.taps, hideout.citadel.modules, me.recruitment.available
        ),
    );
    line(p, team_line("Analysts", me.research_team.as_ref()));
    line(p, team_line("Coders", me.workshop.coders.as_ref()));
    if !hideout.staff.is_empty() {
        line(
            p,
            format!(
                "Waiting: {}",
                hideout
                    .staff
                    .iter()
                    .map(text::team)
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        );
    }
    for (kind, course) in &me.recruitment.courses {
        if course.enrolled > 0 {
            muted(
                p,
                format!(
                    "Training {} {}s{}",
                    course.enrolled,
                    format!("{kind:?}").to_lowercase(),
                    if course.running() {
                        ""
                    } else {
                        " (starts tomorrow)"
                    }
                ),
            );
        }
    }
    match me
        .current_research
        .and_then(|i| me.research.get(&i).map(|r| (i, r)))
    {
        Some((item, progress)) if !progress.researched => {
            line(
                p,
                format!("Researching {} {}%", text::item(item), progress.percent),
            );
        }
        _ => muted(p, "Researching nothing"),
    }
    match me.workshop.jobs.iter().find(|j| j.active) {
        Some(job) => line(
            p,
            format!("Building {} (stage {}/4)", text::item(job.item), job.stage),
        ),
        None => muted(p, "Building nothing"),
    }

    heading(p, "Store");
    store_lines(p, &hideout.store);
    if hideout.citadel.store.iter().next().is_some() {
        heading(p, "Citadel store");
        store_lines(p, &hideout.citadel.store);
    }

    heading(p, "Recruit");
    p.spawn(row()).with_children(|r| {
        let available = me.recruitment.available;
        let mut offered = false;
        for (kind, label, wanted, team) in [
            (
                StaffKind::Analyst,
                "analysts",
                100,
                me.research_team.as_ref(),
            ),
            (StaffKind::Coder, "coders", 100, me.workshop.coders.as_ref()),
            (StaffKind::Operator, "operators", 20, None),
        ] {
            let def = &data.recruitment.courses[&kind];
            let course = me.recruitment.courses.get(&kind);
            if course.is_some_and(|c| c.enrolled > 0) {
                continue;
            }
            let room = def.team_max.map_or(u32::MAX, |max| {
                max.saturating_sub(team.map_or(0, |t| t.count))
            });
            let count = wanted.min(def.batch_max).min(room).min(available);
            if count > 0 {
                offered = true;
                r.spawn(button(
                    format!("+{count} {label}"),
                    Action::Order(Command::Recruit { kind, count }),
                ));
            }
        }
        if !offered {
            r.spawn(text("Every course is running or full.", 11.0, MUTED));
        }
    });

    heading(p, "Research");
    p.spawn(row()).with_children(|r| {
        let mut open: Vec<ItemType> = me
            .research
            .iter()
            .filter(|(_, r)| !r.researched)
            .map(|(&i, _)| i)
            .collect();
        open.sort_by_key(|i| data.research[i].tech_level);
        if open.is_empty() {
            r.spawn(text("nothing to research", 12.0, MUTED));
        }
        for item in open {
            let label = if me.current_research == Some(item) {
                format!("{} (now)", text::item(item))
            } else {
                text::item(item)
            };
            r.spawn(button(label, Action::Order(Command::SetResearch { item })));
        }
    });

    if me.workshop.coders.is_some() {
        heading(p, "Build in the hideout");
        p.spawn(row()).with_children(|r| {
            for item in buildable(status, data, false) {
                r.spawn(button(
                    text::item(item),
                    Action::Order(Command::Build {
                        at: WorkshopRef::Hideout,
                        item,
                    }),
                ));
            }
        });
    }
    if hideout.store.get(ItemType::Tap) > 0 && hideout.taps < Site::MAX_TAPS {
        p.spawn(row()).with_children(|r| {
            r.spawn(button(
                "Install a tap",
                Action::Order(Command::InstallTaps {
                    site: SiteRef::Hideout,
                    count: 1,
                }),
            ));
        });
    }

    if hideout.citadel.complete() {
        heading(p, "Citadel");
        let citadel = &hideout.citadel;
        line(p, team_line("Coders", citadel.workshop.coders.as_ref()));
        if !citadel.staff.is_empty() {
            line(
                p,
                format!(
                    "Waiting: {}",
                    citadel
                        .staff
                        .iter()
                        .map(text::team)
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            );
        }
        if let Some(job) = citadel.workshop.jobs.iter().find(|j| j.active) {
            line(
                p,
                format!("Building {} (stage {}/4)", text::item(job.item), job.stage),
            );
        }
        p.spawn(row()).with_children(|r| {
            let at = WorkshopRef::Citadel(SiteRef::Hideout);
            if citadel.workshop.coders.is_none()
                && let Some(team) = citadel
                    .staff
                    .iter()
                    .position(|s| s.kind == StaffKind::Coder)
            {
                r.spawn(button(
                    "Put the coders to work",
                    Action::Order(Command::AssignCoders { at, team }),
                ));
            }
            if me.workshop.coders.is_some()
                && !me.workshop.jobs.iter().any(|j| j.active)
                && hideout.staff.len() < nullnet_core::STAFF_SLOTS
            {
                r.spawn(button(
                    "Release the hideout coders",
                    Action::Order(Command::ReleaseCoders {
                        at: WorkshopRef::Hideout,
                    }),
                ));
            }
            if citadel.workshop.coders.is_some() {
                for item in buildable(status, data, true) {
                    r.spawn(button(
                        format!("Build {}", text::item(item)),
                        Action::Order(Command::Build { at, item }),
                    ));
                }
            }
        });
    }
}

// ------------------------------------------------------------ selection

/// The crew's own site on a host, if it holds one there.
fn site_of<'a>(status: &'a CrewStatus, data: &GameData, host: HostId) -> Option<&'a Site> {
    if host == data.hideout.host {
        Some(&status.view.me.hideout)
    } else {
        status.view.hosts[usize::from(host.0)].site.as_ref()
    }
}

fn host_status(status: &CrewStatus, data: &GameData, host: HostId) -> String {
    let def = data.host(host);
    let view = &status.view.hosts[usize::from(host.0)];
    let mut parts = Vec::new();
    if def.cache_field {
        return "Cache field: nothing can be built here".into();
    }
    parts.push(match view.controller {
        None => "Free".to_string(),
        Some(Controller::Legacy) => "Held by the Legacy Net".to_string(),
        Some(Controller::Crew(crew)) if crew == status.player => "Yours".to_string(),
        Some(Controller::Crew(crew)) => format!(
            "Held by {}",
            status
                .crews
                .iter()
                .find(|c| c.player == crew)
                .map_or("a rival".to_string(), |c| c.name.clone())
        ),
    });
    if host == data.hideout.host {
        parts.push("every crew's hideout".into());
    }
    if let Some(parent) = def.parent {
        parts.push(format!("service on {}", data.host(parent).name));
    }
    parts.push(format!("{} resources", def.resources.len()));
    parts.push(format!("citadel {}/8", view.citadel_modules));
    if let Some(threat) = status.view.threats.iter().find(|t| t.host == host) {
        parts.push(match (threat.siege_until, threat.arrives) {
            (Some(until), _) => format!(
                "UNDER SIEGE by {} daemons, falls {}",
                threat.daemons,
                date(until)
            ),
            (None, Some(arrives)) => format!(
                "a swarm of {} daemons arrives {}",
                threat.daemons,
                date(arrives)
            ),
            _ => "a swarm is coming".to_string(),
        });
    }
    if let Some((_, until)) = status.view.taps_planted.iter().find(|(h, _)| *h == host) {
        parts.push(format!("your tap siphons here until {}", date(*until)));
    }
    if let Some(site) = site_of(status, data, host)
        && !site.siphons.is_empty()
    {
        parts.push(format!(
            "{} rival tap(s) siphon your extraction",
            site.siphons.len()
        ));
    }
    parts.join(", ")
}

fn selection_panel(
    p: &mut ChildSpawnerCommands,
    status: &CrewStatus,
    data: &GameData,
    selected: &Selected,
    routing: &Routing,
) {
    if let Some(id) = selected.vessel
        && let Some(vessel) = status.view.vessels.get(&id)
    {
        vessel_panel(p, status, data, id, vessel, routing);
        return;
    }
    let Some(host) = selected.host else {
        heading(p, "Selection");
        muted(p, "Click a host or a vessel on the map.");
        return;
    };
    heading(p, &data.host(host).name);
    muted(p, host_status(status, data, host));

    if let Some(site) = site_of(status, data, host)
        && host != data.hideout.host
    {
        line(
            p,
            format!(
                "Taps {}/8   Backdoor {}/2   Citadel {}/8",
                site.taps, site.backdoor_parts, site.citadel.modules
            ),
        );
        heading(p, "Store");
        store_lines(p, &site.store);
        if site.citadel.complete() {
            heading(p, "Citadel store");
            store_lines(p, &site.citadel.store);
            line(
                p,
                team_line("Coders", site.citadel.workshop.coders.as_ref()),
            );
        }
        p.spawn(row()).with_children(|r| {
            if site.store.get(ItemType::Tap) > 0 && site.taps < Site::MAX_TAPS {
                r.spawn(button(
                    "Install a tap",
                    Action::Order(Command::InstallTaps {
                        site: SiteRef::Host(host),
                        count: 1,
                    }),
                ));
            }
            if site.citadel.complete() {
                let at = WorkshopRef::Citadel(SiteRef::Host(host));
                if site.citadel.workshop.coders.is_none()
                    && let Some(team) = site
                        .citadel
                        .staff
                        .iter()
                        .position(|s| s.kind == StaffKind::Coder)
                {
                    r.spawn(button(
                        "Put the coders to work",
                        Action::Order(Command::AssignCoders { at, team }),
                    ));
                }
                if site.citadel.workshop.coders.is_some() {
                    for item in buildable(status, data, true) {
                        r.spawn(button(
                            format!("Build {}", text::item(item)),
                            Action::Order(Command::Build { at, item }),
                        ));
                    }
                }
            }
        });
    }

    if let Some(site) = site_of(status, data, host) {
        heading(p, "Assemble");
        p.spawn(row()).with_children(|r| {
            let own_dropper_here = status.view.vessels.values().any(|v| {
                v.owner == status.player && v.host == host && v.kind == VesselKind::Dropper
            });
            if !own_dropper_here
                && site.store.get(ItemType::DropperCore) > 0
                && site.store.get(ItemType::DropperEngine) > 0
            {
                r.spawn(button(
                    "Assemble a dropper inside",
                    Action::Order(Command::Assemble {
                        host,
                        berth: Berth::Planted,
                        kind: VesselKind::Dropper,
                    }),
                ));
            }
            if site.citadel.complete() {
                for kind in [VesselKind::Worm, VesselKind::Tunneler] {
                    if site.citadel.store.get(kind.core()) > 0
                        && site.citadel.store.get(kind.engine()) > 0
                    {
                        r.spawn(button(
                            format!("Assemble a {} at the citadel", text::kind(kind)),
                            Action::Order(Command::Assemble {
                                host,
                                berth: Berth::Connected,
                                kind,
                            }),
                        ));
                    }
                }
            }
            r.spawn(text(
                "Parts come from the store of the bay the vessel is built in.",
                11.0,
                MUTED,
            ));
        });
    }

    let here: Vec<(&VesselId, &Vessel)> = status
        .view
        .vessels
        .iter()
        .filter(|(_, v)| v.host == host)
        .collect();
    if !here.is_empty() {
        heading(p, "Vessels here");
        for (&id, vessel) in here {
            p.spawn(row()).with_children(|r| {
                let owner = if vessel.owner == status.player {
                    String::new()
                } else {
                    format!(" (crew {})", vessel.owner.0 + 1)
                };
                r.spawn(text(
                    format!(
                        "{}{}: {}",
                        text::vessel(id, &status.view.vessels),
                        owner,
                        text::state(vessel.state, data)
                    ),
                    12.0,
                    FG,
                ));
                if vessel.owner == status.player {
                    r.spawn(button("Select", Action::SelectVessel(id)));
                }
            });
        }
    }
}

fn vessel_panel(
    p: &mut ChildSpawnerCommands,
    status: &CrewStatus,
    data: &GameData,
    id: VesselId,
    vessel: &Vessel,
    routing: &Routing,
) {
    let name = text::vessel(id, &status.view.vessels);
    heading(p, &name);
    line(
        p,
        format!(
            "{} {}, {}/{} anonymisation",
            text::state(vessel.state, data),
            data.host(vessel.host).name,
            vessel.fuel,
            Vessel::FUEL_CAPACITY
        ),
    );
    if let Some(to) = vessel.destination {
        muted(
            p,
            format!(
                "Heading {} {}",
                text::berth(to.berth),
                data.host(to.host).name
            ),
        );
    }
    line(p, team_line("Pilot", vessel.pilot.as_ref()));
    if vessel.kind != VesselKind::Dropper {
        line(
            p,
            format!(
                "Daemons {}/{}   C2 controller: {}",
                vessel.daemons,
                Vessel::DAEMON_CAPACITY,
                if vessel.c2 { "installed" } else { "none" }
            ),
        );
    }
    for (slot, module) in vessel.modules.iter().enumerate() {
        line(p, format!("Slot {}: {}", slot + 1, text::module(module)));
    }
    if let Some(script) = &vessel.script {
        muted(
            p,
            if script.running() {
                "Exfil script: running its route"
            } else {
                "Exfil script installed, no route set"
            },
        );
    }
    if vessel.owner != status.player {
        muted(p, "Not yours.");
        return;
    }
    if routing.0 == Some(id) {
        p.spawn(text("Click a host on the map to route there.", 12.0, GOOD));
        p.spawn(row()).with_children(|r| {
            r.spawn(button("Cancel", Action::CancelRoute));
        });
        return;
    }

    let Some(berth) = vessel.berth() else {
        muted(p, "On its way; orders wait until it arrives.");
        return;
    };
    let site = site_of(status, data, vessel.host);
    let bay = site.and_then(|site| match berth {
        Berth::Planted => Some((&site.store, &site.staff)),
        Berth::Connected => Some((&site.citadel.store, &site.citadel.staff)),
        Berth::Lurking => None,
    });

    if let Some((store, staff)) = bay {
        heading(p, "Crew");
        p.spawn(row()).with_children(|r| {
            if vessel.pilot.is_none() {
                for (team, member) in staff.iter().enumerate() {
                    if member.kind == StaffKind::Operator {
                        r.spawn(button(
                            format!("Pilot: {}", member.leader),
                            Action::Order(Command::Board {
                                vessel: id,
                                seat: Seat::Pilot,
                                team,
                            }),
                        ));
                    }
                }
                if !staff.iter().any(|s| s.kind == StaffKind::Operator) {
                    r.spawn(text(
                        "No operator team in this bay to pilot it.",
                        11.0,
                        MUTED,
                    ));
                }
            } else {
                r.spawn(button(
                    "Pilot leaves",
                    Action::Order(Command::Disembark {
                        vessel: id,
                        seat: Seat::Pilot,
                    }),
                ));
            }
            let fuel = vessel.kind.fuel();
            let amount = 50
                .min(store.get(fuel))
                .min(Vessel::FUEL_CAPACITY - vessel.fuel);
            if amount > 0 {
                r.spawn(button(
                    format!("Refuel +{amount}"),
                    Action::Order(Command::Refuel { vessel: id, amount }),
                ));
            }
            if vessel.kind != VesselKind::Dropper {
                if !vessel.c2 && store.get(ItemType::C2Controller) > 0 {
                    r.spawn(button(
                        "Install a C2 controller",
                        Action::Order(Command::InstallC2 { vessel: id }),
                    ));
                }
                let load = 50
                    .min(store.get(ItemType::Daemon))
                    .min(Vessel::DAEMON_CAPACITY - vessel.daemons);
                if load > 0 {
                    r.spawn(button(
                        format!("Load {load} daemons"),
                        Action::Order(Command::LoadDaemons {
                            vessel: id,
                            count: load,
                        }),
                    ));
                }
                if vessel.daemons > 0 {
                    let count = vessel.daemons.min(50);
                    r.spawn(button(
                        format!("Unload {count} daemons"),
                        Action::Order(Command::UnloadDaemons { vessel: id, count }),
                    ));
                }
            }
        });

        for (slot, module) in vessel.modules.iter().enumerate() {
            heading(p, &format!("Slot {}", slot + 1));
            p.spawn(row()).with_children(|r| match module {
                Module::Empty => {
                    for (pod, kind) in [
                        (ItemType::ToolModule, ModuleKind::ToolModule),
                        (ItemType::DataContainer, ModuleKind::DataContainer),
                        (ItemType::SessionPod, ModuleKind::SessionPod),
                    ] {
                        if store.get(pod) > 0 {
                            r.spawn(button(
                                format!("Fit {}", text::item(pod)),
                                Action::Order(Command::Fit {
                                    vessel: id,
                                    slot,
                                    module: Some(kind),
                                }),
                            ));
                        }
                    }
                    if [
                        ItemType::ToolModule,
                        ItemType::DataContainer,
                        ItemType::SessionPod,
                    ]
                    .iter()
                    .all(|&pod| store.get(pod) == 0)
                    {
                        r.spawn(text("No pods in this bay's store to fit.", 11.0, MUTED));
                    }
                }
                Module::ToolModule(None) => {
                    for (item, count) in store.iter() {
                        let def = &data.items[&item];
                        if def.tool_module {
                            let load = if def.tool_module_single {
                                1
                            } else {
                                count.min(20)
                            };
                            r.spawn(button(
                                format!("Load {load} {}", text::item(item)),
                                Action::Order(Command::Load {
                                    vessel: id,
                                    slot,
                                    item,
                                    count: load,
                                }),
                            ));
                        }
                    }
                    r.spawn(button(
                        "Remove pod",
                        Action::Order(Command::Fit {
                            vessel: id,
                            slot,
                            module: None,
                        }),
                    ));
                }
                Module::DataContainer(None) => {
                    for (item, count) in store.iter() {
                        if data.items[&item].category == ItemCategory::Resource {
                            let load = count.min(Module::CONTAINER_CAPACITY);
                            r.spawn(button(
                                format!("Load {load} {}", text::item(item)),
                                Action::Order(Command::Load {
                                    vessel: id,
                                    slot,
                                    item,
                                    count: load,
                                }),
                            ));
                        }
                    }
                    r.spawn(button(
                        "Remove pod",
                        Action::Order(Command::Fit {
                            vessel: id,
                            slot,
                            module: None,
                        }),
                    ));
                }
                Module::SessionPod(None) => {
                    for (team, member) in staff.iter().enumerate() {
                        r.spawn(button(
                            format!("Board: {}", member.leader),
                            Action::Order(Command::Board {
                                vessel: id,
                                seat: Seat::Pod(slot),
                                team,
                            }),
                        ));
                    }
                    r.spawn(button(
                        "Remove pod",
                        Action::Order(Command::Fit {
                            vessel: id,
                            slot,
                            module: None,
                        }),
                    ));
                }
                Module::ToolModule(Some(cargo)) => {
                    if cargo.item == ItemType::BackdoorKit && berth == Berth::Planted {
                        r.spawn(button(
                            "Install the backdoor kit",
                            Action::Order(Command::Deploy { vessel: id, slot }),
                        ));
                    }
                    r.spawn(button(
                        "Unload",
                        Action::Order(Command::Unload { vessel: id, slot }),
                    ));
                }
                Module::DataContainer(Some(_)) => {
                    r.spawn(button(
                        "Unload",
                        Action::Order(Command::Unload { vessel: id, slot }),
                    ));
                }
                Module::SessionPod(Some(_)) => {
                    r.spawn(button(
                        "Disembark",
                        Action::Order(Command::Disembark {
                            vessel: id,
                            seat: Seat::Pod(slot),
                        }),
                    ));
                }
            });
        }

        if vessel.kind == VesselKind::Dropper {
            heading(p, "Exfil script");
            p.spawn(row()).with_children(|r| match &vessel.script {
                None if store.get(ItemType::ExfilScript) > 0 => {
                    r.spawn(button(
                        "Install an exfil script",
                        Action::Order(Command::InstallScript { vessel: id }),
                    ));
                }
                None => {
                    r.spawn(text(
                        "Build an exfil script to run supplies automatically.",
                        11.0,
                        MUTED,
                    ));
                }
                Some(script) if script.running() => {
                    r.spawn(button(
                        "Stop the route",
                        Action::Order(Command::ConfigureScript {
                            vessel: id,
                            route: None,
                        }),
                    ));
                }
                Some(_) => {
                    let outbound: Vec<ItemType> = site
                        .map(|s| {
                            s.store
                                .iter()
                                .filter(|(i, _)| data.items[i].category == ItemCategory::Resource)
                                .map(|(i, _)| i)
                                .collect()
                        })
                        .unwrap_or_default();
                    r.spawn(button(
                        "Run supplies inside -> citadel",
                        Action::Order(Command::ConfigureScript {
                            vessel: id,
                            route: Some(Route {
                                from: Destination {
                                    host: vessel.host,
                                    berth: Berth::Planted,
                                },
                                to: Destination {
                                    host: vessel.host,
                                    berth: Berth::Connected,
                                },
                                outbound,
                                inbound: Vec::new(),
                            }),
                        }),
                    ));
                }
            });
        }
    } else if berth == Berth::Lurking {
        heading(p, "Outside");
        p.spawn(row()).with_children(|r| {
            let host = vessel.host;
            let legacy_here =
                status.view.hosts[usize::from(host.0)].controller == Some(Controller::Legacy);
            let besieged = status
                .view
                .threats
                .iter()
                .any(|t| t.host == host && t.siege_until.is_some());
            if (legacy_here || besieged) && vessel.c2 && vessel.daemons > 0 {
                r.spawn(button(
                    format!(
                        "Attack {} with {} daemons",
                        if besieged {
                            "the swarm"
                        } else {
                            "the garrison"
                        },
                        vessel.daemons
                    ),
                    Action::Order(Command::Attack { vessel: id }),
                ));
            } else if legacy_here {
                r.spawn(text(
                    "A C2 controller and daemons aboard could take this host.",
                    11.0,
                    MUTED,
                ));
            }
            let rival_here = matches!(
                status.view.hosts[usize::from(host.0)].controller,
                Some(Controller::Crew(crew)) if crew != status.player
            );
            if rival_here && vessel.c2 && vessel.daemons > 0 {
                if status.turn < PROTECTION_TURNS {
                    r.spawn(text(
                        format!("Raids open on turn {PROTECTION_TURNS}."),
                        11.0,
                        MUTED,
                    ));
                } else {
                    for (goal, label) in [
                        (RaidGoal::Exfiltrate, "Raid: exfiltrate the stores"),
                        (RaidGoal::PlantTap, "Raid: plant a tap"),
                        (RaidGoal::TakeOver, "Raid: take the host"),
                    ] {
                        r.spawn(button(
                            format!("{label} ({} daemons)", vessel.daemons),
                            Action::Order(Command::Raid { vessel: id, goal }),
                        ));
                    }
                }
            } else if rival_here {
                r.spawn(text(
                    "A C2 controller and daemons aboard could raid this host.",
                    11.0,
                    MUTED,
                ));
            }
            for (slot, module) in vessel.modules.iter().enumerate() {
                if let Module::ToolModule(Some(cargo)) = module
                    && cargo.item == ItemType::CitadelModule
                {
                    r.spawn(button(
                        format!("Install citadel module (slot {})", slot + 1),
                        Action::Order(Command::Deploy { vessel: id, slot }),
                    ));
                }
            }
        });
    }

    heading(p, "Send");
    p.spawn(row()).with_children(|r| {
        let host = vessel.host;
        let send = |label: &'static str, to: Berth| {
            button(
                label,
                Action::Order(Command::Dispatch {
                    vessel: id,
                    to: Destination { host, berth: to },
                }),
            )
        };
        if vessel.kind == VesselKind::Dropper && berth != Berth::Planted {
            r.spawn(send("Go inside", Berth::Planted));
        }
        if berth != Berth::Connected {
            r.spawn(send("Go to the citadel", Berth::Connected));
        }
        if berth != Berth::Lurking {
            r.spawn(send("Go outside", Berth::Lurking));
        }
        if vessel.kind != VesselKind::Dropper {
            r.spawn(button("Route to another host...", Action::Route(id)));
        }
    });
}

// ------------------------------------------------------------ orders and log

fn orders_panel(p: &mut ChildSpawnerCommands, session: &Session, data: &GameData) {
    let Some(status) = &session.status else {
        heading(p, "Orders");
        return;
    };
    p.spawn(row()).with_children(|r| {
        r.spawn(text(
            format!("ORDERS ({})", session.draft.len()),
            11.0,
            MUTED,
        ));
        if !session.busy {
            r.spawn(button("Hand in", Action::HandIn));
        } else {
            r.spawn(text("talking to the server...", 11.0, MUTED));
        }
        if !session.draft.is_empty() {
            r.spawn(button("Undo", Action::Undo));
            r.spawn(button("Clear", Action::Clear));
        }
        if status.submitted {
            r.spawn(text("handed in", 12.0, GOOD));
            if !session.busy {
                r.spawn(button("Withdraw", Action::Withdraw));
            }
        }
    });
    if let Some(error) = &session.error {
        p.spawn(text(error.clone(), 12.0, WARN));
    } else if let Some(notice) = &session.notice {
        p.spawn(text(notice.clone(), 12.0, GOOD));
    }
    if session.draft.is_empty() {
        muted(
            p,
            "No orders yet. Use the panels, then hand in. Handing in nothing is also a move.",
        );
    }
    let rejected: std::collections::HashMap<usize, &str> = session
        .receipt
        .as_ref()
        .map(|r| {
            r.rejected
                .iter()
                .map(|x| (x.index, x.error.as_str()))
                .collect()
        })
        .unwrap_or_default();
    for (index, command) in session.draft.iter().enumerate() {
        let desc = format!(
            "{}. {}",
            index + 1,
            text::command(command, data, &status.view.vessels)
        );
        match rejected.get(&index) {
            Some(error) => p.spawn(text(
                format!("{desc}  <- would be refused: {error}"),
                12.0,
                WARN,
            )),
            None => p.spawn(text(desc, 12.0, FG)),
        };
    }
}

fn log_panel(p: &mut ChildSpawnerCommands, status: &CrewStatus, data: &GameData) {
    let Some(turn) = &status.last_turn else {
        heading(p, "Log");
        muted(p, "No turn has run yet.");
        return;
    };
    heading(
        p,
        &format!(
            "Log: turn {} ran, days {} to {}",
            turn.turn, turn.report.first_day, turn.report.last_day
        ),
    );
    for rejected in &turn.report.rejected {
        p.spawn(text(
            format!(
                "Order {} was refused: {}",
                rejected.index + 1,
                rejected.error
            ),
            12.0,
            WARN,
        ));
    }
    if turn.report.events.is_empty() {
        muted(p, "Nothing to report.");
    }
    for (index, event) in turn.report.events.iter().enumerate().take(60) {
        let day = event.day();
        let entry = format!(
            "{}  {}",
            date(day),
            text::event(event, data, &status.view.vessels)
        );
        if matches!(event, Event::BattleFought { .. } | Event::Raid { .. }) {
            p.spawn(row()).with_children(|r| {
                r.spawn(text(entry, 12.0, FG));
                r.spawn(button("Replay", Action::Replay(index)));
            });
        } else {
            line(p, entry);
        }
    }
}
