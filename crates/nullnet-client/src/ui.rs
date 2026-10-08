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
    Berth, Command, Controller, Destination, GameData, HostId, ItemCategory, ItemType, Module,
    ModuleKind, Route, Seat, Site, SiteRef, Staff, StaffKind, Vessel, VesselId, VesselKind,
    WorkshopRef, date,
};

use crate::Rules;
use crate::map::{Routing, Selected, crew_color};
use crate::net::{self, Api, Clock, Inbox, Session};
use crate::text;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OverUi>()
            .add_systems(Startup, spawn_layout)
            .add_systems(
                Update,
                (
                    track_cursor,
                    press_buttons,
                    style_buttons,
                    scroll_panels,
                    countdown,
                    refresh.run_if(
                        resource_changed::<Session>
                            .or_else(resource_changed::<Selected>)
                            .or_else(resource_changed::<Routing>),
                    ),
                )
                    .chain(),
            );
    }
}

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

fn press_buttons(
    mouse: Res<ButtonInput<MouseButton>>,
    buttons: Query<(&Interaction, &Action), Changed<Interaction>>,
    api: Res<Api>,
    inbox: Res<Inbox>,
    mut session: ResMut<Session>,
    mut selected: ResMut<Selected>,
    mut routing: ResMut<Routing>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    for (interaction, action) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
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
        }
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

fn refresh(
    mut commands: Commands,
    session: Res<Session>,
    selected: Res<Selected>,
    routing: Res<Routing>,
    api: Res<Api>,
    rules: Res<Rules>,
    slots: Query<(Entity, &Slot)>,
) {
    let data = &rules.0;
    for (entity, slot) in &slots {
        let mut e = commands.entity(entity);
        e.despawn_children();
        e.with_children(|p| match slot {
            Slot::TopBar => top_bar(p, &session, &api),
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
        });
    }
}

// ------------------------------------------------------------ top bar

fn top_bar(p: &mut ChildSpawnerCommands, session: &Session, api: &Api) {
    p.spawn(text("NULLNET", 18.0, FG));
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
    p.spawn((text("", 12.0, MUTED), Countdown));
    for crew in &status.crews {
        let color = Color::Srgba(Srgba::hex(crew_color(status.player, crew.player)).unwrap());
        let held = status
            .view
            .crews
            .iter()
            .find(|c| c.player == crew.player)
            .map_or(0, |c| c.hosts);
        p.spawn(text(
            format!(
                "{}{} [{}] {} hosts",
                crew.name,
                if crew.bot { " (bot)" } else { "" },
                if crew.submitted { "x" } else { " " },
                held
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
    for event in turn.report.events.iter().take(40) {
        let day = match event {
            nullnet_core::Event::ResearchCompleted { day, .. }
            | nullnet_core::Event::StaffPromoted { day, .. }
            | nullnet_core::Event::RecruitsGraduated { day, .. }
            | nullnet_core::Event::ItemBuilt { day, .. }
            | nullnet_core::Event::HostClaimed { day, .. }
            | nullnet_core::Event::Installed { day, .. }
            | nullnet_core::Event::VesselArrived { day, .. }
            | nullnet_core::Event::VesselStopped { day, .. }
            | nullnet_core::Event::VesselBurned { day, .. }
            | nullnet_core::Event::Unlocked { day, .. } => *day,
        };
        line(
            p,
            format!(
                "{}  {}",
                date(day),
                text::event(event, data, &status.view.vessels)
            ),
        );
    }
}
