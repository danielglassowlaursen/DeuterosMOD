//! The client's screen: the top bar of resources, the left column of panels
//! (your crew, the market, the host you have selected, your orders and the
//! turn log), the guide at the foot of the map, and the story and help
//! overlays. The map itself is drawn behind by [`crate::map`].
//!
//! The panels are rebuilt from the [`Session`] whenever it changes, so the
//! screen always shows the latest turn.

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use nullnet_core::{
    Command, Controller, CrewView, GameData, HackerId, HostId, HostView, Operation, Upgrade,
    Weakness, chance, rules,
};

use crate::Rules;
use crate::guide;
use crate::icons;
use crate::map::{MapInsets, Selected};
use crate::net::{Api, Inbox, Session, hand_in, withdraw};
use crate::sound::{Cue, Play, Sounds};
use crate::theme::{self, FG, MUTED, PanelMaterial};
use crate::{music, notify, voice};

const ACCENT: Color = theme::GOLD;
const TOP_BAR: f32 = 46.0;
const GAP: f32 = 8.0;
const COL_W: f32 = 320.0;
const FOOT: f32 = 132.0;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Toast>()
            .init_resource::<Hud>()
            .add_systems(Startup, (build, open_story))
            .add_systems(
                Update,
                (
                    press_buttons,
                    style_buttons,
                    scroll_column,
                    fade_toast,
                    refresh.run_if(changed),
                ),
            );
    }
}

fn changed(session: Res<Session>, selected: Res<Selected>, hud: Res<Hud>) -> bool {
    session.is_changed() || selected.is_changed() || hud.is_changed()
}

/// A short message shown over the map when a turn runs.
#[derive(Resource, Default)]
pub struct Toast {
    lines: Vec<String>,
    timer: f32,
}

impl Toast {
    pub fn show(&mut self, lines: Vec<String>) {
        self.lines = lines;
        self.timer = 9.0;
    }
}

/// What the player has opened or turned off.
#[derive(Resource)]
struct Hud {
    show_story: bool,
    story_page: usize,
    show_help: bool,
    show_guide: bool,
    listened: bool,
}

impl Default for Hud {
    fn default() -> Self {
        Hud {
            show_story: false,
            story_page: 0,
            show_help: false,
            show_guide: !notify::guide_hidden(),
            listened: false,
        }
    }
}

/// Which panel a content container fills.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Bar,
    Crew,
    Market,
    Selection,
    Orders,
    Log,
    Guide,
    Story,
    Help,
    Toast,
}

/// A rebuilt content container.
#[derive(Component)]
struct Content(Panel);

/// The look a button rests and hovers in.
#[derive(Component, Clone)]
struct Look {
    rest: Handle<PanelMaterial>,
    hover: Handle<PanelMaterial>,
}

#[derive(Component, Clone)]
enum Action {
    Order(Command),
    DropOrder(usize),
    HandIn,
    Withdraw,
    Clear,
    DoGuide,
    ToggleMute,
    ToggleMusic,
    RequestNotify,
    ToggleHelp,
    ToggleGuide,
    StoryPage(i32),
    CloseStory,
    Listen,
}

// ------------------------------------------------------------ build

fn build(mut commands: Commands, mut insets: ResMut<MapInsets>) {
    *insets = MapInsets(Vec4::new(COL_W + GAP * 2.0, TOP_BAR + GAP, GAP, FOOT + GAP));

    // Top bar.
    commands.spawn((bar_node(), MaterialNode(theme::BAR), Content(Panel::Bar)));

    // Left column: a scroll of panels.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(GAP),
                top: Val::Px(TOP_BAR + GAP),
                width: Val::Px(COL_W),
                bottom: Val::Px(FOOT + GAP),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(GAP),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
        ))
        .with_children(|col| {
            for (panel, title) in [
                (Panel::Crew, "Crew"),
                (Panel::Market, "Market"),
                (Panel::Selection, "Host"),
                (Panel::Orders, "Orders"),
                (Panel::Log, "Last turn"),
            ] {
                col.spawn((panel_node(), MaterialNode(theme::PANEL)))
                    .with_children(|p| {
                        heading(p, title);
                        p.spawn((content_node(), Content(panel)));
                    });
            }
        });

    // The guide / turn box along the foot.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(GAP),
            right: Val::Px(GAP),
            bottom: Val::Px(GAP),
            height: Val::Px(FOOT),
            padding: UiRect::all(Val::Px(10.0)),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            overflow: Overflow::clip(),
            ..default()
        },
        MaterialNode(theme::PANEL),
        Content(Panel::Guide),
    ));

    // Toast over the map.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(TOP_BAR + GAP + 6.0),
            left: Val::Px(COL_W + GAP * 3.0),
            max_width: Val::Px(420.0),
            padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
            display: Display::None,
            ..default()
        },
        MaterialNode(theme::TIP),
        Content(Panel::Toast),
    ));

    // Story and help overlays, centred, hidden until opened.
    commands.spawn((
        overlay_node(520.0),
        MaterialNode(theme::OVERLAY),
        Content(Panel::Story),
    ));
    commands.spawn((
        overlay_node(560.0),
        MaterialNode(theme::OVERLAY),
        Content(Panel::Help),
    ));
}

fn open_story(mut hud: ResMut<Hud>) {
    if !notify::story_seen() {
        hud.show_story = true;
        hud.story_page = 0;
    }
}

fn bar_node() -> Node {
    Node {
        position_type: PositionType::Absolute,
        top: Val::ZERO,
        left: Val::ZERO,
        right: Val::ZERO,
        height: Val::Px(TOP_BAR),
        align_items: AlignItems::Center,
        column_gap: Val::Px(14.0),
        padding: UiRect::horizontal(Val::Px(14.0)),
        overflow: Overflow::clip(),
        ..default()
    }
}

fn panel_node() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        padding: UiRect::all(Val::Px(10.0)),
        row_gap: Val::Px(4.0),
        flex_shrink: 0.0,
        ..default()
    }
}

fn content_node() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(4.0),
        ..default()
    }
}

fn overlay_node(width: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Percent(50.0),
        top: Val::Percent(50.0),
        width: Val::Px(width),
        max_height: Val::Percent(86.0),
        margin: UiRect::new(Val::Px(-width / 2.0), Val::ZERO, Val::Px(-220.0), Val::ZERO),
        padding: UiRect::all(Val::Px(20.0)),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(10.0),
        overflow: Overflow::scroll_y(),
        display: Display::None,
        ..default()
    }
}

// ------------------------------------------------------------ content

#[allow(clippy::too_many_arguments)]
fn refresh(
    mut commands: Commands,
    session: Res<Session>,
    selected: Res<Selected>,
    hud: Res<Hud>,
    sounds: Res<Sounds>,
    rules: Res<Rules>,
    containers: Query<(Entity, &Content)>,
    children: Query<&Children>,
    mut nodes: Query<&mut Node>,
) {
    let data = &rules.0;
    for (entity, content) in &containers {
        let panel = content.0;
        // The toast's content is owned by fade_toast, not rebuilt here.
        if panel == Panel::Toast {
            continue;
        }
        // Clear the old content.
        if let Ok(kids) = children.get(entity) {
            for &child in kids {
                commands.entity(child).despawn();
            }
        }
        // Overlays toggle their own visibility.
        if let Ok(mut node) = nodes.get_mut(entity) {
            let shown = match panel {
                Panel::Story => hud.show_story,
                Panel::Help => hud.show_help,
                _ => true,
            };
            if matches!(panel, Panel::Story | Panel::Help) {
                node.display = if shown { Display::Flex } else { Display::None };
            }
        }
        commands.entity(entity).with_children(|p| match panel {
            Panel::Bar => build_bar(p, &session, &sounds, &hud),
            Panel::Crew => build_crew(p, &session, data),
            Panel::Market => build_market(p, &session),
            Panel::Selection => build_selection(p, &session, &selected, data),
            Panel::Orders => build_orders(p, &session, data),
            Panel::Log => build_log(p, &session, data),
            Panel::Guide => build_guide(p, &session, &hud),
            Panel::Story => build_story(p, &hud),
            Panel::Help => build_help(p),
            Panel::Toast => {}
        });
    }
}

fn build_bar(p: &mut ChildSpawnerCommands, session: &Session, sounds: &Sounds, hud: &Hud) {
    p.spawn(theme::bold("NULLNET", 17.0, ACCENT));
    let Some(status) = session.status.as_ref() else {
        p.spawn(text("connecting…", 13.0, MUTED));
        return;
    };
    let view = &status.view;
    let me = &view.me;
    p.spawn(text(
        format!(
            "Turn {}/{}  ·  {}",
            status.turn, status.game.last_turn, status.game.difficulty
        ),
        13.0,
        FG,
    ));
    chip(p, icons::CREDITS, me.credits.to_string(), ACCENT);
    chip(p, icons::COMPUTE, me.compute.to_string(), theme::GOOD);
    chip(p, icons::DATA, me.data.to_string(), theme::GOOD);
    chip(p, icons::BANDWIDTH, view.bandwidth.to_string(), FG);
    let trace_color = if me.trace * 2 >= view.sweep_at {
        theme::WARN
    } else {
        MUTED
    };
    chip(
        p,
        icons::TRACE,
        format!("{}/{}", me.trace, view.sweep_at),
        trace_color,
    );

    p.spawn(spacer());
    icon_button(
        p,
        if sounds.muted {
            icons::VOLUME_OFF
        } else {
            icons::VOLUME
        },
        Action::ToggleMute,
    );
    icon_button(p, icons::MUSIC, Action::ToggleMusic);
    if matches!(notify::permission(), notify::Permission::Ask) {
        icon_button(p, icons::BELL, Action::RequestNotify);
    }
    icon_button(p, icons::BOOK, Action::ToggleHelp);
    icon_button(
        p,
        if hud.show_guide {
            icons::CHECK
        } else {
            icons::TARGET
        },
        Action::ToggleGuide,
    );
}

fn build_crew(p: &mut ChildSpawnerCommands, session: &Session, _data: &GameData) {
    let Some(status) = session.status.as_ref() else {
        muted(p, "connecting…");
        return;
    };
    let view = &status.view;
    let me = &view.me;
    for hacker in &me.hackers {
        let ready = hacker.out_until <= view.turn;
        p.spawn(row()).with_children(|r| {
            r.spawn(theme::icon(
                icons::weakness(hacker.specialty),
                15.0,
                if ready { ACCENT } else { MUTED },
            ));
            r.spawn(text(
                format!(
                    "{}  ·  {} L{}",
                    hacker.handle,
                    hacker.specialty.name(),
                    hacker.level
                ),
                13.0,
                if ready { FG } else { MUTED },
            ));
            if !ready {
                r.spawn(text(
                    format!("(out {})", hacker.out_until),
                    12.0,
                    theme::WARN,
                ));
            }
        });
    }
    if !me.kits.is_empty() {
        muted(
            p,
            format!(
                "Kits: {}",
                me.kits
                    .iter()
                    .map(|w| w.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }
    if me.zero_days > 0 {
        muted(p, format!("Zero-days: {}", me.zero_days));
    }

    heading(p, "Buy");
    p.spawn(wrap()).with_children(|r| {
        for weakness in Weakness::ALL {
            if !me.kits.contains(&weakness) {
                small(
                    r,
                    format!("{} {}", weakness.name(), rules::KIT_PRICE),
                    Action::Order(Command::BuyKit { weakness }),
                );
            }
        }
        small(
            r,
            format!("0-day {}", rules::ZERO_DAY_PRICE),
            Action::Order(Command::BuyZeroDay),
        );
    });

    heading(p, "Upgrades");
    p.spawn(wrap()).with_children(|r| {
        for up in Upgrade::ALL {
            let level = me.upgrades.level(up);
            if level < up.max_level() {
                let (c, _) = up.cost(level);
                small(
                    r,
                    format!("{} {}", up.name(), c),
                    Action::Order(Command::Upgrade { upgrade: up }),
                );
            }
        }
    });
}

fn build_market(p: &mut ChildSpawnerCommands, session: &Session) {
    let Some(status) = session.status.as_ref() else {
        return;
    };
    let me = &status.view.me;
    if me.market.is_empty() {
        muted(p, "the market is empty");
        return;
    }
    let room = me.hackers.len() < me.slots();
    for offer in &me.market {
        let can = room && offer.price <= me.credits;
        p.spawn(row()).with_children(|r| {
            r.spawn(theme::icon(
                icons::weakness(offer.hacker.specialty),
                14.0,
                MUTED,
            ));
            r.spawn(text(
                format!(
                    "{} · {} L{} · {}c",
                    offer.hacker.handle,
                    offer.hacker.specialty.name(),
                    offer.hacker.level,
                    offer.price
                ),
                12.5,
                if can { FG } else { MUTED },
            ));
            if can {
                r.spawn(spacer());
                small(
                    r,
                    "Hire",
                    Action::Order(Command::Hire {
                        hacker: offer.hacker.id,
                    }),
                );
            }
        });
    }
}

fn build_selection(
    p: &mut ChildSpawnerCommands,
    session: &Session,
    selected: &Selected,
    data: &GameData,
) {
    let Some(status) = session.status.as_ref() else {
        return;
    };
    let view = &status.view;
    let Some(host) = selected.0 else {
        muted(p, "Click a host on the map.");
        return;
    };
    let def = data.host(host);
    let hv = &view.hosts[host.index()];
    p.spawn(theme::bold(def.name.to_uppercase(), 14.0, ACCENT));
    muted(
        p,
        format!(
            "{}  ·  {}",
            def.district.name(),
            controller_name(session, hv)
        ),
    );

    if let Some(intel) = hv.intel {
        p.spawn(row()).with_children(|r| {
            r.spawn(theme::icon(icons::SHIELD, 13.0, MUTED));
            r.spawn(text(format!("Security {}", intel.security), 12.5, FG));
            r.spawn(theme::icon(
                icons::weakness(intel.weakness),
                13.0,
                theme::GOOD,
            ));
            r.spawn(text(intel.weakness.name(), 12.5, FG));
            r.spawn(text(format!("ICE {}", intel.ice), 12.5, FG));
        });
    } else {
        muted(p, "Unknown — scan it to learn its weakness.");
    }

    // Operation buttons that fit the host now.
    let mine = hv.controller == Some(Controller::Crew(view.player));
    p.spawn(wrap()).with_children(|r| {
        if hv.can_scan {
            op_button(r, view, &session.draft, Operation::Scan, host);
        }
        if hv.can_break_in {
            op_button(r, view, &session.draft, Operation::BreakIn, host);
        }
        if hv.access && !mine {
            op_button(r, view, &session.draft, Operation::Backdoor, host);
            op_button(r, view, &session.draft, Operation::StealData, host);
        }
        if mine && host != view.me.hideout {
            op_button(r, view, &session.draft, Operation::Defend, host);
        }
    });
    if !hv.can_scan && !hv.can_break_in && !hv.access && !mine {
        muted(p, "Out of reach — take a linked host first.");
    }
}

fn build_orders(p: &mut ChildSpawnerCommands, session: &Session, data: &GameData) {
    if session.draft.is_empty() {
        muted(p, "No orders yet. Pick a host and choose an operation.");
    } else {
        for (index, command) in session.draft.iter().enumerate() {
            p.spawn((looked(
                Action::DropOrder(index),
                theme::ROW,
                theme::ROW_HOVER,
                row(),
            ),))
                .with_children(|r| {
                    r.spawn(theme::icon(icons::CLOSE, 12.0, MUTED));
                    r.spawn(text(crate::text::command(command, data), 12.5, FG));
                });
        }
    }
    if let Some(receipt) = &session.receipt
        && !receipt.rejected.is_empty()
    {
        for rejected in &receipt.rejected {
            p.spawn(text(
                format!("#{}: {}", rejected.index + 1, rejected.error),
                12.0,
                theme::WARN,
            ));
        }
    }
    if let Some(notice) = &session.notice {
        muted(p, notice.clone());
    }
    if let Some(error) = &session.error {
        p.spawn(text(error.clone(), 12.0, theme::WARN));
    }

    let submitted = session.status.as_ref().is_some_and(|s| s.submitted);
    p.spawn(Node {
        margin: UiRect::top(Val::Px(6.0)),
        ..default()
    });
    p.spawn(primary(
        if submitted { "Handed in" } else { "Hand in" },
        Action::HandIn,
    ));
    p.spawn(wrap()).with_children(|r| {
        small(r, "Withdraw", Action::Withdraw);
        small(r, "Clear", Action::Clear);
    });
}

fn build_log(p: &mut ChildSpawnerCommands, session: &Session, data: &GameData) {
    let Some(status) = session.status.as_ref() else {
        return;
    };
    if status.over {
        let scores = &status.view.scores;
        let winner = match scores.split_first() {
            Some((first, rest)) if rest.first().map(|s| s.total) != Some(first.total) => {
                crate::text::crew(session, first.player)
            }
            _ => "a tie".into(),
        };
        p.spawn(theme::bold(format!("GAME OVER · {winner}"), 13.0, ACCENT));
    }
    let Some(last) = status.last_turn.as_ref() else {
        muted(p, "No turn has run yet.");
        return;
    };
    let mut any = false;
    for event in &last.report.events {
        let line = crate::text::event(event, data, session);
        if !line.is_empty() {
            p.spawn(text(line, 12.5, FG));
            any = true;
        }
    }
    if !any {
        muted(p, "Nothing to report.");
    }
}

fn build_guide(p: &mut ChildSpawnerCommands, session: &Session, hud: &Hud) {
    let Some(status) = session.status.as_ref() else {
        return;
    };
    if !hud.show_guide {
        // Just the hand-in line when the guide is off.
        p.spawn(row()).with_children(|r| {
            r.spawn(text(deadline_line(session), 12.5, MUTED));
        });
        return;
    }
    let steps = guide::steps(status, &session.draft);
    let current = guide::current(&steps);
    let step = &steps[current];
    p.spawn(row()).with_children(|r| {
        r.spawn(theme::bold(
            format!("OBJECTIVE {} OF {}", current + 1, steps.len()),
            12.0,
            ACCENT,
        ));
        r.spawn(spacer());
        r.spawn(text(deadline_line(session), 12.0, MUTED));
    });
    p.spawn(theme::bold(step.title.clone(), 14.0, FG));
    p.spawn(text(step.hint.clone(), 12.5, MUTED));
    p.spawn(wrap()).with_children(|r| {
        if step.action.is_some() {
            small(r, "Do it", Action::DoGuide);
        }
    });
}

fn build_story(p: &mut ChildSpawnerCommands, hud: &Hud) {
    let page = &guide::STORY[hud.story_page.min(guide::STORY.len() - 1)];
    p.spawn(theme::bold(page.title.to_uppercase(), 18.0, ACCENT));
    p.spawn(text(page.body, 14.0, FG));
    p.spawn(wrap()).with_children(|r| {
        if hud.story_page > 0 {
            small(r, "Back", Action::StoryPage(-1));
        }
        if hud.story_page + 1 < guide::STORY.len() {
            small(r, "Next", Action::StoryPage(1));
        } else {
            small(r, "Begin", Action::CloseStory);
        }
        small(r, "Skip", Action::CloseStory);
        small(r, "Listen", Action::Listen);
    });
}

fn build_help(p: &mut ChildSpawnerCommands) {
    p.spawn(row()).with_children(|r| {
        r.spawn(theme::bold("HOW TO PLAY", 18.0, ACCENT));
        r.spawn(spacer());
        small(r, "Close", Action::ToggleHelp);
    });
    for (title, lines) in guide::help() {
        heading_owned(p, title);
        for line in lines {
            p.spawn(text(line, 13.0, FG));
        }
    }
}

// ------------------------------------------------------------ helpers

fn controller_name(session: &Session, hv: &HostView) -> String {
    match hv.controller {
        None => "free".into(),
        Some(Controller::Legacy) => "Legacy Net".into(),
        Some(Controller::Crew(p)) => crate::text::crew(session, p),
    }
}

/// A ready hacker for an operation, preferring a specialty match, skipping
/// any already busy in the draft.
fn pick_hacker(view: &CrewView, draft: &[Command], want: Option<Weakness>) -> Option<HackerId> {
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
    ready.iter().max_by_key(|h| h.level).map(|h| h.id)
}

/// A button for an operation on a host, with the odds for a break-in.
fn op_button(
    r: &mut ChildSpawnerCommands,
    view: &CrewView,
    draft: &[Command],
    op: Operation,
    host: HostId,
) {
    let want = (op == Operation::BreakIn)
        .then(|| view.hosts[host.index()].intel.map(|i| i.weakness))
        .flatten();
    let hacker = pick_hacker(view, draft, want);
    let Some(hacker) = hacker else {
        small_disabled(r, op.name());
        return;
    };
    let label = match op {
        Operation::BreakIn => {
            if let Some(intel) = view.hosts[host.index()].intel {
                let a = breakin_attack(view, hacker, intel.weakness);
                format!("Break in {}%", chance(a, intel.defence))
            } else {
                "Break in".to_string()
            }
        }
        other => other.name().to_string(),
    };
    let command = match op {
        Operation::Scan => Command::Scan { hacker, host },
        Operation::BreakIn => Command::BreakIn {
            hacker,
            host,
            zero_day: false,
            boost: 0,
        },
        Operation::Backdoor => Command::Backdoor { hacker, host },
        Operation::StealData => Command::StealData { hacker, host },
        Operation::Defend => Command::Defend { hacker, host },
    };
    small(r, label, Action::Order(command));
}

/// The attack a break-in would have, worked out from the view alone (the
/// client has the crew, kits and the host's weakness).
fn breakin_attack(view: &CrewView, hacker: HackerId, weakness: Weakness) -> u32 {
    let Some(h) = view.me.hacker(hacker) else {
        return 0;
    };
    let mut a = 2 * u32::from(h.level);
    if h.specialty == weakness {
        a += rules::SPECIALTY_BONUS;
    }
    if view.me.kits.contains(&weakness) {
        a += rules::KIT_BONUS;
    }
    a
}

fn deadline_line(session: &Session) -> String {
    let Some(status) = session.status.as_ref() else {
        return String::new();
    };
    if status.game.deadline_hours == 0 {
        "Practice: the turn runs as soon as you hand in.".into()
    } else if status.submitted {
        "Handed in. Waiting for the other crews.".into()
    } else {
        "Hand in your orders to run the turn.".into()
    }
}

fn chip(p: &mut ChildSpawnerCommands, glyph: char, value: String, color: Color) {
    p.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(4.0),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|c| {
        c.spawn(theme::icon(glyph, 14.0, color));
        c.spawn(text(value, 13.0, FG));
    });
}

fn text(s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    theme::text(s, size, color)
}

fn muted(p: &mut ChildSpawnerCommands, s: impl Into<String>) {
    p.spawn(text(s, 12.5, MUTED));
}

fn heading(p: &mut ChildSpawnerCommands, s: &str) {
    heading_owned(p, s);
}

fn heading_owned(p: &mut ChildSpawnerCommands, s: &str) {
    p.spawn((
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(6.0),
            margin: UiRect::top(Val::Px(6.0)),
            flex_shrink: 0.0,
            ..default()
        },
        children![],
    ))
    .with_children(|r| {
        r.spawn((
            Node {
                width: Val::Px(3.0),
                height: Val::Px(11.0),
                ..default()
            },
            BackgroundColor(theme::GOLD),
        ));
        r.spawn(theme::bold(s.to_uppercase(), 12.0, theme::GOLD));
    });
}

fn row() -> Node {
    Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(6.0),
        width: Val::Percent(100.0),
        ..default()
    }
}

fn wrap() -> Node {
    Node {
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        column_gap: Val::Px(6.0),
        row_gap: Val::Px(6.0),
        margin: UiRect::top(Val::Px(4.0)),
        ..default()
    }
}

fn spacer() -> impl Bundle {
    Node {
        flex_grow: 1.0,
        ..default()
    }
}

fn looked(
    action: Action,
    rest: Handle<PanelMaterial>,
    hover: Handle<PanelMaterial>,
    node: Node,
) -> impl Bundle {
    (
        Button,
        action,
        Look {
            rest: rest.clone(),
            hover,
        },
        node,
        MaterialNode(rest),
    )
}

fn small(r: &mut ChildSpawnerCommands, label: impl Into<String>, action: Action) {
    r.spawn((
        looked(
            action,
            theme::BUTTON,
            theme::BUTTON_HOVER,
            Node {
                padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                flex_shrink: 0.0,
                ..default()
            },
        ),
        children![theme::bold(label.into(), 12.5, ACCENT)],
    ));
}

fn small_disabled(r: &mut ChildSpawnerCommands, label: &str) {
    r.spawn((
        Node {
            padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
            ..default()
        },
        MaterialNode(theme::BUTTON),
        children![theme::bold(label, 12.5, MUTED)],
    ));
}

fn icon_button(p: &mut ChildSpawnerCommands, glyph: char, action: Action) {
    p.spawn((
        looked(
            action,
            theme::BUTTON,
            theme::BUTTON_HOVER,
            Node {
                width: Val::Px(30.0),
                height: Val::Px(28.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
        ),
        children![theme::icon(glyph, 16.0, ACCENT)],
    ));
}

fn primary(label: impl Into<String>, action: Action) -> impl Bundle {
    (
        looked(
            action,
            theme::PRIMARY,
            theme::PRIMARY_HOVER,
            Node {
                padding: UiRect::axes(Val::Px(12.0), Val::Px(7.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                width: Val::Percent(100.0),
                flex_shrink: 0.0,
                ..default()
            },
        ),
        children![
            theme::icon(icons::SEND, 15.0, theme::INK),
            theme::bold(label.into(), 14.0, theme::INK)
        ],
    )
}

// ------------------------------------------------------------ systems

#[allow(clippy::too_many_arguments)]
fn press_buttons(
    interactions: Query<(&Interaction, &Action), Changed<Interaction>>,
    api: Res<Api>,
    inbox: Res<Inbox>,
    rules: Res<Rules>,
    mut session: ResMut<Session>,
    mut selected: ResMut<Selected>,
    mut hud: ResMut<Hud>,
    mut sounds: ResMut<Sounds>,
    mut play: MessageWriter<Play>,
) {
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if !sounds.muted {
            play.write(Play(Cue::Click));
        }
        match action {
            Action::Order(command) => session.draft.push(command.clone()),
            Action::DropOrder(index) => {
                if *index < session.draft.len() {
                    session.draft.remove(*index);
                }
            }
            Action::HandIn => {
                let orders = session.draft.clone();
                hand_in(&api, &inbox, &mut session, &orders);
            }
            Action::Withdraw => {
                session.draft.clear();
                withdraw(&api, &inbox, &mut session);
            }
            Action::Clear => session.draft.clear(),
            Action::DoGuide => {
                if let Some(status) = session.status.as_ref() {
                    let steps = guide::steps(status, &session.draft);
                    let current = guide::current(&steps);
                    if let Some(command) = steps[current].action.clone() {
                        // Point the player at the host it acts on.
                        if let Some((_, _, host)) = command.operation() {
                            *selected = Selected(Some(host));
                        }
                        session.draft.push(command);
                    }
                }
                let _ = &rules;
            }
            Action::ToggleMute => {
                sounds.muted = !sounds.muted;
                notify::remember_muted(sounds.muted);
                if sounds.muted {
                    music::stop();
                } else if sounds.music && sounds.unlocked {
                    music::start();
                }
            }
            Action::ToggleMusic => {
                sounds.music = !sounds.music;
                notify::remember_music_off(!sounds.music);
                if sounds.music && !sounds.muted && sounds.unlocked {
                    music::start();
                } else {
                    music::stop();
                }
            }
            Action::RequestNotify => notify::request_permission(),
            Action::ToggleHelp => {
                hud.show_help = !hud.show_help;
            }
            Action::ToggleGuide => {
                hud.show_guide = !hud.show_guide;
                notify::remember_guide_hidden(!hud.show_guide);
            }
            Action::StoryPage(delta) => {
                hud.story_page = (hud.story_page as i32 + delta).max(0) as usize;
                if hud.listened {
                    voice::play_story(hud.story_page);
                }
            }
            Action::CloseStory => {
                hud.show_story = false;
                voice::stop();
                notify::remember_story_seen();
            }
            Action::Listen => {
                hud.listened = true;
                voice::play_story(hud.story_page);
            }
        }
    }
}

fn style_buttons(
    mut buttons: Query<
        (&Interaction, &Look, &mut MaterialNode<PanelMaterial>),
        Changed<Interaction>,
    >,
) {
    for (interaction, look, mut material) in &mut buttons {
        material.0 = match interaction {
            Interaction::Hovered | Interaction::Pressed => look.hover.clone(),
            Interaction::None => look.rest.clone(),
        };
    }
}

fn scroll_column(mut wheel: MessageReader<MouseWheel>, mut scrolls: Query<&mut ScrollPosition>) {
    let mut delta = 0.0;
    for event in wheel.read() {
        delta += match event.unit {
            MouseScrollUnit::Line => event.y * 24.0,
            MouseScrollUnit::Pixel => event.y,
        };
    }
    if delta != 0.0 {
        for mut scroll in &mut scrolls {
            scroll.0.y = (scroll.0.y - delta).max(0.0);
        }
    }
}

fn fade_toast(
    time: Res<Time>,
    mut toast: ResMut<Toast>,
    containers: Query<(Entity, &Content)>,
    children: Query<&Children>,
    mut nodes: Query<&mut Node>,
    mut commands: Commands,
    mut last: Local<usize>,
) {
    let toast_entity = containers
        .iter()
        .find(|(_, c)| c.0 == Panel::Toast)
        .map(|(e, _)| e);
    let Some(entity) = toast_entity else {
        return;
    };
    if toast.timer > 0.0 {
        toast.timer -= time.delta_secs();
    }
    let shown = toast.timer > 0.0 && !toast.lines.is_empty();
    if let Ok(mut node) = nodes.get_mut(entity) {
        node.display = if shown { Display::Flex } else { Display::None };
    }
    // Rebuild the lines only when they change.
    if shown && *last != toast.lines.len() + toast.lines.first().map_or(0, |l| l.len()) {
        *last = toast.lines.len() + toast.lines.first().map_or(0, |l| l.len());
        if let Ok(kids) = children.get(entity) {
            for &c in kids {
                commands.entity(c).despawn();
            }
        }
        let lines = toast.lines.clone();
        commands.entity(entity).with_children(|p| {
            for (i, line) in lines.iter().enumerate() {
                let size = if i == 0 { 14.0 } else { 12.5 };
                let color = if i == 0 { ACCENT } else { FG };
                p.spawn(theme::text(line.clone(), size, color));
            }
        });
    }
}
