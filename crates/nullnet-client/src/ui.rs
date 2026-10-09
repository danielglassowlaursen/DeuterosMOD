//! The game's screen over the map, laid out the way Stellaris lays out its
//! own: a bar of resources along the top, a rail of menu icons down the
//! left that opens one window beside it, an outliner of the crew's hosts,
//! vessels and threats down the right, and the turn's orders with the Hand
//! in button under it. The guide sits at the foot of the map; the story,
//! the help and battle replays open over it. The look is Deus Ex's, from
//! [`crate::theme`]. Every panel is rebuilt from the crew's latest view
//! whenever that view, the selection, the orders or the open menu change.

use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::ui::{FocusPolicy, RelativeCursorPosition};
use bevy::window::PrimaryWindow;
use nullnet_api::CrewStatus;
use nullnet_core::{
    BattleReport, Berth, Citadel, Command, Controller, Destination, EndReason, Event, GameData,
    HostId, ItemCategory, ItemType, Module, ModuleKind, PROTECTION_TURNS, RaidGoal, Route, Seat,
    Site, SiteRef, Staff, StaffKind, Vessel, VesselId, VesselKind, WorkshopRef, date,
};

use crate::Rules;
use crate::guide;
use crate::icons;
use crate::map::{MapInsets, Routing, Selected, crew_color};
use crate::music;
use crate::net::{self, Api, Clock, Inbox, Session};
use crate::notify;
use crate::sound::{Cue, Play, Sounds};
use crate::text;
use crate::theme::{self, PanelMaterial};
use crate::voice;

/// Seconds a battle replay takes from first to last snapshot.
const REPLAY_SECONDS: f32 = 5.0;
/// Seconds the turn toast stays up.
const TOAST_SECONDS: f32 = 14.0;
/// The layout, in logical pixels: the gap between panels, the top bar's
/// height, the rail's width, the window's and the outliner's widths, and
/// the turn box's height.
const GAP: f32 = 8.0;
const TOP_BAR: f32 = 48.0;
const RAIL: f32 = 52.0;
const WINDOW: f32 = 340.0;
const OUTLINER: f32 = 268.0;
const TURN_BOX: f32 = 214.0;
/// Where the window starts, right of the rail, and where the map's free
/// area starts while the window is open.
const WINDOW_LEFT: f32 = GAP + RAIL + GAP;
const PAST_WINDOW: f32 = WINDOW_LEFT + WINDOW + GAP;
/// The room the map leaves at its foot for the guide.
const GUIDE_ROOM: f32 = 150.0;
/// Least height of the story panel.
const STORY_HEIGHT: f32 = 360.0;
/// The resources the top bar shows from the hideout's store.
const BAR_RESOURCES: [ItemType; 9] = [
    ItemType::Compute,
    ItemType::Storage,
    ItemType::Memory,
    ItemType::Code,
    ItemType::Credentials,
    ItemType::Bandwidth,
    ItemType::ExitNodes,
    ItemType::Proxies,
    ItemType::ProxyChains,
];

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OverUi>()
            .init_resource::<Toast>()
            .init_resource::<Replay>()
            .init_resource::<GuideState>()
            .init_resource::<OpenMenu>()
            .add_systems(Startup, spawn_layout)
            .add_systems(
                Update,
                (
                    track_cursor,
                    press_buttons,
                    open_selection,
                    style_buttons,
                    scroll_panels,
                    countdown,
                    fade_toast,
                    animate_replay,
                    open_story.run_if(resource_changed::<Session>),
                    refresh.run_if(
                        resource_changed::<Session>
                            .or_else(resource_changed::<Selected>)
                            .or_else(resource_changed::<Routing>)
                            .or_else(resource_changed::<Toast>)
                            .or_else(resource_changed::<Replay>)
                            .or_else(resource_changed::<Sounds>)
                            .or_else(resource_changed::<GuideState>)
                            .or_else(resource_changed::<OpenMenu>),
                    ),
                    show_tip,
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

/// The guide over the map: whether the player hid it, whether the help
/// overlay is open, and the page of the story being read, if any.
#[derive(Resource)]
pub struct GuideState {
    pub hidden: bool,
    pub help: bool,
    pub story: Option<usize>,
}

impl Default for GuideState {
    fn default() -> Self {
        GuideState {
            hidden: notify::guide_hidden(),
            help: false,
            story: None,
        }
    }
}

/// Whether the cursor is over a panel, so the map leaves the click alone.
#[derive(Resource, Default)]
pub struct OverUi(pub bool);

/// A panel the cursor can be over and scroll.
#[derive(Component)]
struct Panel;

/// A menu on the rail, or the selection: what the window beside the rail
/// shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    Hideout,
    Research,
    Recruit,
    Workshop,
    Citadel,
    Orders,
    Log,
    Crews,
    Selection,
}

impl Menu {
    /// The menus on the rail, top to bottom.
    const RAIL: [Menu; 8] = [
        Menu::Hideout,
        Menu::Research,
        Menu::Recruit,
        Menu::Workshop,
        Menu::Citadel,
        Menu::Orders,
        Menu::Log,
        Menu::Crews,
    ];

    fn icon(self) -> char {
        match self {
            Menu::Hideout => icons::HOUSE,
            Menu::Research => icons::FLASK,
            Menu::Recruit => icons::USER_PLUS,
            Menu::Workshop => icons::WRENCH,
            Menu::Citadel => icons::CASTLE,
            Menu::Orders => icons::LIST,
            Menu::Log => icons::SCROLL,
            Menu::Crews => icons::CROWN,
            Menu::Selection => icons::TARGET,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Menu::Hideout => "Hideout",
            Menu::Research => "Research",
            Menu::Recruit => "Recruit",
            Menu::Workshop => "Workshop",
            Menu::Citadel => "Citadel",
            Menu::Orders => "Orders",
            Menu::Log => "Log",
            Menu::Crews => "Crews",
            Menu::Selection => "Selection",
        }
    }

    fn tip(self) -> &'static str {
        match self {
            Menu::Hideout => "Hideout: your teams, what they are doing, and the store",
            Menu::Research => "Research: what the analysts work on",
            Menu::Recruit => "Recruit: train analysts, coders and operators",
            Menu::Workshop => "Workshop: build in the hideout and install taps",
            Menu::Citadel => "Citadel: the workshop above the hideout",
            Menu::Orders => "Orders: everything queued for this turn",
            Menu::Log => "Log: what happened in the last turn",
            Menu::Crews => "Crews: points, heat and who has handed in",
            Menu::Selection => "",
        }
    }
}

/// The menu open in the window beside the rail, if any.
#[derive(Resource, Default, PartialEq, Eq)]
pub struct OpenMenu(pub Option<Menu>);

/// A container whose children are rebuilt on every refresh.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Slot {
    TopBar,
    Rail,
    Window,
    Outliner,
    TurnBox,
    Guide,
    Toast,
    Replay,
    Help,
    Story,
}

/// A see-through box over the map's free area that holds a panel in place:
/// the guide at its foot, the toast at its head, the overlays in its
/// middle. Its left edge moves aside when the window opens.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Frame {
    Foot,
    Head,
    Middle,
}

#[derive(Component)]
struct Countdown;

/// A line shown by the cursor while it rests on the node.
#[derive(Component)]
struct Tip(String);

/// The box the tip is shown in, and its text.
#[derive(Component)]
struct TipBox;

#[derive(Component)]
struct TipText;

/// How a button looks at rest and under the cursor.
#[derive(Component, Clone)]
struct Look {
    rest: Handle<PanelMaterial>,
    hover: Handle<PanelMaterial>,
}

/// What a button does when pressed.
#[derive(Component, Clone)]
enum Action {
    Order(Command),
    Undo,
    Clear,
    HandIn,
    Withdraw,
    SelectVessel(VesselId),
    SelectHost(HostId),
    Route(VesselId),
    CancelRoute,
    ToggleMute,
    ToggleMusic,
    RequestNotify,
    /// Replays the battle in the last turn's event at this index.
    Replay(usize),
    CloseReplay,
    ToggleGuide,
    ToggleHelp,
    /// Opens the story at this page.
    StoryPage(usize),
    CloseStory,
    /// Reads the story's open page aloud.
    Listen,
    /// Opens a menu in the window, or closes it if it is open.
    OpenMenu(Menu),
    CloseWindow,
}

const FG: Color = theme::FG;
const MUTED: Color = theme::MUTED;
const ACCENT: Color = theme::GOLD;
const WARN: Color = theme::WARN;
const GOOD: Color = theme::GOOD;
const BORDER: Color = theme::GOLD_DIM;

fn text(s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    theme::text(s, size, color)
}

fn line(p: &mut ChildSpawnerCommands, s: impl Into<String>) {
    p.spawn(text(s, 13.0, FG));
}

fn muted(p: &mut ChildSpawnerCommands, s: impl Into<String>) {
    p.spawn(text(s, 12.5, MUTED));
}

/// A section heading: a gold tick and the name in capitals.
fn heading(p: &mut ChildSpawnerCommands, s: &str) {
    p.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(6.0),
        margin: UiRect::top(Val::Px(8.0)),
        flex_shrink: 0.0,
        ..default()
    })
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

/// A pressable node with a look at rest and under the cursor.
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

fn button(label: impl Into<String>, action: Action) -> impl Bundle {
    (
        looked(
            action,
            theme::BUTTON,
            theme::BUTTON_HOVER,
            Node {
                padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                flex_shrink: 0.0,
                ..default()
            },
        ),
        children![theme::bold(label, 13.0, ACCENT)],
    )
}

/// The button that moves the turn on.
fn primary(label: impl Into<String>, action: Action) -> impl Bundle {
    (
        looked(
            action,
            theme::PRIMARY,
            theme::PRIMARY_HOVER,
            Node {
                padding: UiRect::axes(Val::Px(14.0), Val::Px(7.0)),
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
            theme::bold(label, 15.0, theme::INK)
        ],
    )
}

/// A square icon button, as on the rail; `on` while its menu is open.
fn icon_button(
    glyph: char,
    tip: impl Into<String>,
    action: Action,
    on: bool,
    color: Color,
) -> impl Bundle {
    let rest = if on { theme::BUTTON_ON } else { theme::BUTTON };
    (
        looked(
            action,
            rest,
            theme::BUTTON_HOVER,
            Node {
                width: Val::Px(40.0),
                height: Val::Px(40.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
        ),
        Tip(tip.into()),
        children![theme::icon(glyph, 19.0, color)],
    )
}

/// A small icon button inside a panel: close, undo, clear.
fn small_icon_button(glyph: char, tip: impl Into<String>, action: Action) -> impl Bundle {
    (
        looked(
            action,
            theme::BUTTON,
            theme::BUTTON_HOVER,
            Node {
                width: Val::Px(26.0),
                height: Val::Px(26.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
        ),
        Tip(tip.into()),
        children![theme::icon(glyph, 14.0, ACCENT)],
    )
}

/// A row in the outliner that selects what it shows.
fn list_row(action: Action, on: bool) -> impl Bundle {
    let rest = if on { theme::ROW_ON } else { theme::ROW };
    looked(
        action,
        rest,
        theme::ROW_HOVER,
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(7.0),
            padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
            width: Val::Percent(100.0),
            flex_shrink: 0.0,
            ..default()
        },
    )
}

fn row() -> Node {
    Node {
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        column_gap: Val::Px(6.0),
        row_gap: Val::Px(6.0),
        align_items: AlignItems::Center,
        flex_shrink: 0.0,
        ..default()
    }
}

/// Space that pushes what follows in a row to its far end.
fn spacer() -> Node {
    Node {
        flex_grow: 1.0,
        ..default()
    }
}

/// An icon and a value with a tip, as the top bar shows its numbers.
fn stat(
    p: &mut ChildSpawnerCommands,
    glyph: char,
    value: impl Into<String>,
    tip: impl Into<String>,
    color: Color,
) {
    p.spawn((
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(5.0),
            flex_shrink: 0.0,
            ..default()
        },
        Interaction::default(),
        Tip(tip.into()),
    ))
    .with_children(|s| {
        s.spawn(theme::icon(glyph, 14.0, theme::GOLD));
        s.spawn(theme::bold(value, 13.0, color));
    });
}

/// A line with an icon in front of it.
fn icon_line(p: &mut ChildSpawnerCommands, glyph: char, s: impl Into<String>, color: Color) {
    p.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(7.0),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|r| {
        r.spawn(theme::icon(glyph, 13.0, theme::GOLD));
        r.spawn(text(s, 13.0, color));
    });
}

/// Deus Ex's segmented bar: `fraction` of its segments lit.
fn progress(p: &mut ChildSpawnerCommands, fraction: f32) {
    const SEGMENTS: usize = 20;
    let lit = (fraction.clamp(0.0, 1.0) * SEGMENTS as f32).round() as usize;
    p.spawn(Node {
        flex_direction: FlexDirection::Row,
        column_gap: Val::Px(2.0),
        height: Val::Px(7.0),
        width: Val::Percent(100.0),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|b| {
        for segment in 0..SEGMENTS {
            b.spawn((
                Node {
                    flex_grow: 1.0,
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(if segment < lit {
                    theme::GOLD
                } else {
                    theme::GOLD.with_alpha(0.14)
                }),
            ));
        }
    });
}

fn cap(s: impl Into<String>) -> String {
    let s = s.into();
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => s,
    }
}

fn panel_with(node: Node, slot: Slot, material: Handle<PanelMaterial>) -> impl Bundle {
    (
        Panel,
        slot,
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            padding: UiRect::all(Val::Px(12.0)),
            overflow: Overflow::scroll_y(),
            ..node
        },
        ScrollPosition::default(),
        MaterialNode(material),
        RelativeCursorPosition::default(),
        FocusPolicy::Block,
    )
}

fn panel(node: Node, slot: Slot) -> impl Bundle {
    panel_with(node, slot, theme::PANEL)
}

/// The see-through box a [`Frame`] holds its panel in.
fn frame(kind: Frame) -> impl Bundle {
    let justify = match kind {
        Frame::Foot => JustifyContent::FlexEnd,
        Frame::Head => JustifyContent::FlexStart,
        Frame::Middle => JustifyContent::Center,
    };
    (
        kind,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(WINDOW_LEFT),
            right: Val::Px(OUTLINER + 2.0 * GAP),
            top: Val::Px(TOP_BAR + GAP),
            bottom: Val::Px(GAP),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            justify_content: justify,
            ..default()
        },
        FocusPolicy::Pass,
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
            height: Val::Px(TOP_BAR),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(18.0),
            padding: UiRect::axes(Val::Px(14.0), Val::Px(0.0)),
            overflow: Overflow::clip(),
            ..default()
        },
        MaterialNode(theme::BAR),
        RelativeCursorPosition::default(),
        FocusPolicy::Block,
    ));
    commands.spawn((
        Panel,
        Slot::Rail,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(GAP),
            top: Val::Px(TOP_BAR + GAP),
            bottom: Val::Px(GAP),
            width: Val::Px(RAIL),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(6.0),
            padding: UiRect::vertical(Val::Px(12.0)),
            overflow: Overflow::clip(),
            ..default()
        },
        MaterialNode(theme::PANEL),
        RelativeCursorPosition::default(),
        FocusPolicy::Block,
    ));
    commands.spawn((
        panel(
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(WINDOW_LEFT),
                top: Val::Px(TOP_BAR + GAP),
                bottom: Val::Px(GAP),
                width: Val::Px(WINDOW),
                ..default()
            },
            Slot::Window,
        ),
        Visibility::Hidden,
    ));
    commands.spawn(panel(
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(GAP),
            top: Val::Px(TOP_BAR + GAP),
            bottom: Val::Px(TURN_BOX + 2.0 * GAP),
            width: Val::Px(OUTLINER),
            row_gap: Val::Px(3.0),
            ..default()
        },
        Slot::Outliner,
    ));
    commands.spawn(panel(
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(GAP),
            bottom: Val::Px(GAP),
            width: Val::Px(OUTLINER),
            height: Val::Px(TURN_BOX),
            ..default()
        },
        Slot::TurnBox,
    ));

    commands
        .spawn((frame(Frame::Foot), GlobalZIndex(1)))
        .with_children(|f| {
            f.spawn((
                panel(
                    Node {
                        width: Val::Percent(100.0),
                        max_width: Val::Px(860.0),
                        max_height: Val::Px(250.0),
                        ..default()
                    },
                    Slot::Guide,
                ),
                Visibility::Hidden,
            ));
        });
    commands
        .spawn((frame(Frame::Head), GlobalZIndex(2)))
        .with_children(|f| {
            f.spawn((
                panel(
                    Node {
                        width: Val::Percent(100.0),
                        max_width: Val::Px(760.0),
                        ..default()
                    },
                    Slot::Toast,
                ),
                Visibility::Hidden,
            ));
        });
    commands
        .spawn((frame(Frame::Middle), GlobalZIndex(3)))
        .with_children(|f| {
            f.spawn((
                panel_with(
                    Node {
                        width: Val::Percent(100.0),
                        max_width: Val::Px(560.0),
                        row_gap: Val::Px(8.0),
                        ..default()
                    },
                    Slot::Replay,
                    theme::OVERLAY,
                ),
                Visibility::Hidden,
            ));
            f.spawn((
                panel_with(
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        max_width: Val::Px(820.0),
                        ..default()
                    },
                    Slot::Help,
                    theme::OVERLAY,
                ),
                Visibility::Hidden,
            ));
            f.spawn((
                panel_with(
                    Node {
                        width: Val::Percent(100.0),
                        max_width: Val::Px(780.0),
                        // Tall enough for the longest page, so the buttons
                        // pinned to the bottom stay put from page to page.
                        min_height: Val::Px(STORY_HEIGHT),
                        max_height: Val::Percent(100.0),
                        row_gap: Val::Px(10.0),
                        padding: UiRect::all(Val::Px(20.0)),
                        ..default()
                    },
                    Slot::Story,
                    theme::OVERLAY,
                ),
                Visibility::Hidden,
            ));
        });

    commands
        .spawn((
            TipBox,
            Node {
                position_type: PositionType::Absolute,
                padding: UiRect::axes(Val::Px(9.0), Val::Px(5.0)),
                max_width: Val::Px(300.0),
                ..default()
            },
            MaterialNode(theme::TIP),
            GlobalZIndex(30),
            Visibility::Hidden,
            FocusPolicy::Pass,
        ))
        .with_children(|t| {
            t.spawn((text("", 12.5, FG), TipText));
        });
}

/// Opens the window on what the player selected on the map or in the
/// outliner, and closes it when the selection is cleared.
fn open_selection(selected: Res<Selected>, mut open: ResMut<OpenMenu>) {
    if !selected.is_changed() || selected.is_added() {
        return;
    }
    if selected.host.is_some() || selected.vessel.is_some() {
        open.set_if_neq(OpenMenu(Some(Menu::Selection)));
    } else if open.0 == Some(Menu::Selection) {
        open.0 = None;
    }
}

/// Shows the tip of whatever the cursor rests on, beside the cursor.
fn show_tip(
    window: Single<&Window, With<PrimaryWindow>>,
    tips: Query<(&Interaction, &Tip, &InheritedVisibility)>,
    tip_box: Single<(&mut Node, &mut Visibility), With<TipBox>>,
    tip_text: Single<&mut Text, With<TipText>>,
) {
    let (mut node, mut visibility) = tip_box.into_inner();
    let hovered = tips
        .iter()
        .find(|(interaction, _, shown)| **interaction != Interaction::None && shown.get())
        .map(|(_, tip, _)| tip.0.as_str())
        .filter(|tip| !tip.is_empty());
    let (Some(tip), Some(cursor)) = (hovered, window.cursor_position()) else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    let mut tip_text = tip_text.into_inner();
    if tip_text.0 != tip {
        tip_text.0 = tip.to_string();
    }
    // Beside the cursor, and to its left near the right edge.
    let left = if cursor.x > window.width() - 320.0 {
        cursor.x - 300.0
    } else {
        cursor.x + 16.0
    };
    let top = if cursor.y > window.height() - 60.0 {
        cursor.y - 36.0
    } else {
        cursor.y + 20.0
    };
    node.left = Val::Px(left.max(4.0));
    node.top = Val::Px(top);
    visibility.set_if_neq(Visibility::Inherited);
}

type ChangedButtons = (Changed<Interaction>, With<Button>);

fn style_buttons(
    mut buttons: Query<(&Interaction, &Look, &mut MaterialNode<PanelMaterial>), ChangedButtons>,
) {
    for (interaction, look, mut node) in &mut buttons {
        let wanted = match interaction {
            Interaction::None => &look.rest,
            Interaction::Hovered | Interaction::Pressed => &look.hover,
        };
        if node.0 != *wanted {
            node.0 = wanted.clone();
        }
    }
}

/// Opens the story the first time a crew's status arrives, unless this
/// browser has read it before.
fn open_story(session: Res<Session>, mut guide: ResMut<GuideState>, mut opened: Local<bool>) {
    if *opened || session.status.is_none() {
        return;
    }
    *opened = true;
    if !notify::story_seen() {
        guide.story = Some(0);
        music::duck(true);
    }
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

/// A hidden overlay still knows where the cursor is, so only panels that
/// are shown count as being under it.
fn track_cursor(
    panels: Query<(&RelativeCursorPosition, &Visibility), With<Panel>>,
    mut over: ResMut<OverUi>,
) {
    let now = panels
        .iter()
        .any(|(c, visibility)| c.cursor_over && *visibility != Visibility::Hidden);
    if over.0 != now {
        over.0 = now;
    }
}

fn scroll_panels(
    mut wheel: MessageReader<MouseWheel>,
    mut panels: Query<(&RelativeCursorPosition, &Visibility, &mut ScrollPosition), With<Panel>>,
) {
    for event in wheel.read() {
        let dy = match event.unit {
            MouseScrollUnit::Line => event.y * 24.0,
            MouseScrollUnit::Pixel => event.y,
        };
        for (cursor, visibility, mut scroll) in &mut panels {
            if cursor.cursor_over && *visibility != Visibility::Hidden {
                scroll.0.y = (scroll.0.y - dy).max(0.0);
            }
        }
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
    mut guide: ResMut<GuideState>,
    mut open: ResMut<OpenMenu>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    // The first click is what a browser wants before a page makes sound.
    if !sounds.unlocked {
        sounds.bypass_change_detection().unlocked = true;
        if !sounds.muted && sounds.music {
            music::start();
        }
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
            Action::SelectHost(host) => {
                *selected = Selected {
                    host: Some(host),
                    vessel: None,
                };
            }
            Action::OpenMenu(menu) => {
                open.0 = if open.0 == Some(menu) {
                    None
                } else {
                    Some(menu)
                };
            }
            Action::CloseWindow => open.0 = None,
            Action::Route(id) => routing.0 = Some(id),
            Action::CancelRoute => routing.0 = None,
            Action::ToggleMute => {
                sounds.muted = !sounds.muted;
                notify::remember_muted(sounds.muted);
                if sounds.muted {
                    voice::stop();
                    music::stop();
                } else if sounds.music {
                    music::start();
                }
            }
            Action::ToggleMusic => {
                sounds.music = !sounds.music;
                notify::remember_music_off(!sounds.music);
                if !sounds.music {
                    music::stop();
                } else if !sounds.muted {
                    music::start();
                }
            }
            Action::RequestNotify => notify::request_permission(),
            Action::Replay(index) => {
                if let Some(state) = replay_of(&session, index) {
                    replay.current = Some(state);
                    play.write(Play(Cue::Battle));
                }
            }
            Action::CloseReplay => replay.current = None,
            Action::ToggleGuide => {
                guide.hidden = !guide.hidden;
                notify::remember_guide_hidden(guide.hidden);
            }
            Action::ToggleHelp => guide.help = !guide.help,
            Action::StoryPage(page) => {
                guide.story = Some(page);
                guide.help = false;
                music::duck(true);
                if sounds.muted {
                    voice::stop();
                } else {
                    voice::play_story(page);
                }
            }
            Action::CloseStory => {
                guide.story = None;
                notify::remember_story_seen();
                voice::stop();
                music::duck(false);
            }
            Action::Listen => {
                if let Some(page) = guide.story {
                    voice::play_story(page);
                }
            }
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

/// The time left on the running turn, as the top bar shows it.
fn countdown_text(session: &Session, clock: &Clock) -> String {
    match session.seconds_left(clock) {
        Some(left) if left <= 0 => "deadline passed".to_string(),
        Some(left) => format!("deadline in {}h {:02}m", left / 3600, (left % 3600) / 60),
        None => String::new(),
    }
}

fn countdown(
    session: Res<Session>,
    clock: Res<Clock>,
    mut texts: Query<&mut Text, With<Countdown>>,
) {
    let wanted = countdown_text(&session, &clock);
    for mut text in &mut texts {
        if text.0 != wanted {
            text.0 = wanted.clone();
        }
    }
}

fn toast_lines(p: &mut ChildSpawnerCommands, toast: &Toast) {
    for (index, line) in toast.lines.iter().enumerate() {
        if index == 0 {
            p.spawn(text(line.clone(), 13.0, GOOD));
        } else {
            p.spawn(text(line.clone(), 12.0, FG));
        }
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

// ------------------------------------------------------------ guide

/// The step the crew is on, with what to press next.
fn guide_panel(
    p: &mut ChildSpawnerCommands,
    session: &Session,
    toast: Option<&Toast>,
    data: &GameData,
) {
    let Some(status) = &session.status else {
        return;
    };
    if let Some(toast) = toast {
        toast_lines(p, toast);
        p.spawn((
            Node {
                height: Val::Px(1.0),
                margin: UiRect::vertical(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(BORDER),
        ));
    }
    let steps = guide::steps(status, data, &session.draft);
    let current = guide::current(&steps);
    let step = &steps[current];
    p.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(8.0),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|r| {
        r.spawn(theme::icon(icons::TARGET, 16.0, theme::GOLD));
        r.spawn(theme::bold(
            format!("OBJECTIVE {} OF {}", current + 1, steps.len()),
            12.0,
            MUTED,
        ));
        r.spawn(theme::bold(
            step.title.to_uppercase(),
            15.0,
            theme::GOLD_BRIGHT,
        ));
    });
    if let Some(story) = guide::STEP_STORIES.get(current) {
        muted(p, *story);
    }
    for hint in step.hints.iter().take(5) {
        line(p, hint.clone());
    }
    if status.submitted {
        p.spawn(text(
            "Orders handed in. The turn runs when every crew has handed in or the deadline passes; the guide moves on then.",
            12.0,
            GOOD,
        ));
    }
    if let Some(next) = steps.get(current + 1) {
        muted(p, format!("Next: {}", next.title));
    }
    p.spawn(row()).with_children(|r| {
        r.spawn(button("Help", Action::ToggleHelp));
        r.spawn(button("Hide guide", Action::ToggleGuide));
    });
}

/// One page of the story, with the way to the next.
fn story_panel(p: &mut ChildSpawnerCommands, page: usize, session: &Session) {
    let pages = guide::STORY;
    let page = page.min(pages.len().saturating_sub(1));
    let (title, paragraphs) = pages[page];
    p.spawn(row()).with_children(|r| {
        r.spawn(theme::icon(icons::BOOK, 16.0, theme::GOLD));
        r.spawn(theme::bold(
            format!(
                "THE STORY OF NULLNET   PAGE {} OF {}",
                page + 1,
                pages.len()
            ),
            12.0,
            MUTED,
        ));
        r.spawn(button("Listen", Action::Listen));
    });
    p.spawn(theme::bold(title.to_uppercase(), 20.0, theme::GOLD_BRIGHT));
    for paragraph in paragraphs {
        p.spawn(text(*paragraph, 13.0, FG));
    }
    if let Some(status) = &session.status {
        if page == 0 {
            p.spawn(text(format!("Today is {}.", date(status.day)), 13.0, GOOD));
        }
        if page + 1 == pages.len() {
            p.spawn(text(
                format!(
                    "In this game a turn is {} days, and you are {}.",
                    status.game.turn_days, status.name
                ),
                13.0,
                GOOD,
            ));
        }
    }
    p.spawn(Node {
        margin: UiRect::top(Val::Auto),
        ..row()
    })
    .with_children(|r| {
        // The way on comes first, so it stays under the cursor page after page.
        let last = page + 1 == pages.len();
        if last {
            r.spawn(button("Begin", Action::CloseStory));
        } else {
            r.spawn(button("Next", Action::StoryPage(page + 1)));
        }
        if page > 0 {
            r.spawn(button("Back", Action::StoryPage(page - 1)));
        }
        if !last {
            r.spawn(button("Skip", Action::CloseStory));
        }
    });
}

/// Every step with its state, then the rules in brief.
fn help_panel(p: &mut ChildSpawnerCommands, session: &Session, data: &GameData) {
    p.spawn(row()).with_children(|r| {
        r.spawn(theme::icon(icons::HELP, 17.0, theme::GOLD));
        r.spawn(theme::bold("HOW TO PLAY", 16.0, theme::GOLD_BRIGHT));
        r.spawn(spacer());
        r.spawn(button("Read the story", Action::StoryPage(0)));
        r.spawn(small_icon_button(icons::CLOSE, "Close", Action::ToggleHelp));
    });
    muted(
        p,
        "Scroll for the rules. The guide over the map follows your crew step by step; the steps so far:",
    );
    if let Some(status) = &session.status {
        heading(p, "Your steps");
        let steps = guide::steps(status, data, &session.draft);
        let current = guide::current(&steps);
        for (index, step) in steps.iter().enumerate() {
            // Marks, not boxes: the list is read, not ticked.
            let (mark, color) = if step.done {
                (icons::CHECK, GOOD)
            } else if index == current {
                (icons::CHEVRON, ACCENT)
            } else {
                (icons::HEXAGON, MUTED)
            };
            p.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(7.0),
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|r| {
                r.spawn(theme::icon(mark, 13.0, color));
                r.spawn(text(format!("{}. {}", index + 1, step.title), 13.0, color));
            });
            if index == current {
                for hint in &step.hints {
                    p.spawn((
                        text(hint.clone(), 12.5, MUTED),
                        Node {
                            margin: UiRect::left(Val::Px(20.0)),
                            ..default()
                        },
                    ));
                }
            }
        }
    }
    for (title, lines) in guide::HELP {
        heading(p, title);
        for l in *lines {
            line(p, *l);
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
    clock: Res<Clock>,
    toast: Res<Toast>,
    replay: Res<Replay>,
    sounds: Res<Sounds>,
    guide: Res<GuideState>,
    open: Res<OpenMenu>,
    mut insets: ResMut<MapInsets>,
    mut frames: Query<&mut Node, (With<Frame>, Without<Slot>)>,
    mut slots: Query<(Entity, &Slot, &mut Visibility, &mut Node), Without<Frame>>,
) {
    let data = &rules.0;
    let status = session.status.as_ref();
    let window_open = status.is_some() && open.0.is_some();
    let toast_shown = !toast.lines.is_empty() && toast.seconds_left > 0.0;
    let reading = guide.story.is_some();
    // The guide carries the toast's lines while it is up, so the foot of
    // the map does not swap between the two; it gives way to a replay.
    let guide_shown =
        status.is_some() && !guide.hidden && !guide.help && !reading && replay.current.is_none();

    // The frames over the map move aside for the window, and the map fits
    // itself into what the panels leave free.
    let left = if window_open {
        PAST_WINDOW
    } else {
        WINDOW_LEFT
    };
    for mut node in &mut frames {
        if node.left != Val::Px(left) {
            node.left = Val::Px(left);
        }
    }
    insets.set_if_neq(MapInsets(Vec4::new(
        left,
        TOP_BAR + GAP,
        OUTLINER + 2.0 * GAP,
        if guide_shown { GUIDE_ROOM } else { GAP },
    )));

    for (entity, slot, mut visibility, mut node) in &mut slots {
        let shown = match slot {
            Slot::Window => window_open,
            Slot::Toast => toast_shown && !guide_shown && !reading,
            Slot::Replay => replay.current.is_some(),
            Slot::Guide => guide_shown,
            Slot::Help => guide.help && !reading,
            Slot::Story => reading,
            Slot::TopBar | Slot::Rail | Slot::Outliner | Slot::TurnBox => true,
        };
        let (wanted, display) = if shown {
            (Visibility::Inherited, Display::Flex)
        } else {
            (Visibility::Hidden, Display::None)
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
        if node.display != display {
            node.display = display;
        }
        let mut e = commands.entity(entity);
        e.despawn_children();
        if !shown {
            continue;
        }
        e.with_children(|p| match slot {
            Slot::TopBar => top_bar(p, &session, &clock, &api),
            Slot::Rail => rail(p, &open, &guide, &sounds),
            Slot::Window => {
                if let (Some(status), Some(menu)) = (status, open.0) {
                    window_panel(p, menu, status, &session, data, &selected, &routing);
                }
            }
            Slot::Outliner => match status {
                Some(status) => outliner(p, status, data, &selected),
                None => muted(p, "Waiting for the server..."),
            },
            Slot::TurnBox => turn_box(p, &session, data),
            Slot::Toast => toast_lines(p, &toast),
            Slot::Replay => {
                if let Some(state) = &replay.current {
                    replay_panel(p, state);
                }
            }
            Slot::Guide => guide_panel(
                p,
                &session,
                if toast_shown { Some(&toast) } else { None },
                data,
            ),
            Slot::Help => help_panel(p, &session, data),
            Slot::Story => {
                if let Some(page) = guide.story {
                    story_panel(p, page, &session);
                }
            }
        });
    }
}

// ------------------------------------------------------------ top bar

fn top_bar(p: &mut ChildSpawnerCommands, session: &Session, clock: &Clock, api: &Api) {
    let Some(status) = &session.status else {
        p.spawn(theme::bold("NULLNET", 18.0, theme::GOLD_BRIGHT));
        if api.token.is_none() {
            p.spawn(text(
                "No crew token. Open your invite link, or create a game at /console.",
                13.0,
                MUTED,
            ));
        } else if let Some(error) = &session.error {
            p.spawn(text(error.clone(), 13.0, WARN));
        } else {
            p.spawn(text("Connecting...", 13.0, MUTED));
        }
        return;
    };
    let me = &status.view.me;
    let colour = Color::Srgba(Srgba::hex(crew_color(status.player, status.player)).unwrap());

    // The crew.
    p.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(8.0),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|r| {
        r.spawn(theme::icon(icons::HEXAGON, 24.0, colour));
        r.spawn(Node {
            flex_direction: FlexDirection::Column,
            ..default()
        })
        .with_children(|c| {
            c.spawn(theme::bold(status.name.to_uppercase(), 15.0, colour));
            c.spawn(text(status.game.name.clone(), 11.0, MUTED));
        });
    });

    // What the hideout's store holds, and the recruits left.
    p.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(16.0),
        flex_shrink: 1.0,
        min_width: Val::Px(0.0),
        overflow: Overflow::clip(),
        ..default()
    })
    .with_children(|r| {
        for item in BAR_RESOURCES {
            stat(
                r,
                icons::item(item),
                me.hideout.store.get(item).to_string(),
                format!("{} in the hideout store", cap(text::item(item))),
                FG,
            );
        }
        stat(
            r,
            icons::USERS,
            me.recruitment.available.to_string(),
            "Recruits left for the rest of the game",
            FG,
        );
    });

    p.spawn(spacer());

    // The crew's standing, and the turn.
    if me.war.is_some() {
        stat(
            p,
            icons::SWORDS,
            "AT WAR",
            "The Legacy Net is at war with you: its swarms come for your hosts",
            WARN,
        );
    }
    let points = status
        .view
        .scores
        .iter()
        .find(|s| s.player == status.player)
        .map_or(0, |s| s.total);
    stat(
        p,
        icons::TROPHY,
        points.to_string(),
        "Your points; Crews on the rail has the table",
        FG,
    );
    stat(
        p,
        icons::FLAME,
        me.heat.to_string(),
        "Heat: every raid adds to it, and it cools a point a day. The Legacy Net's swarms go for the hottest crew.",
        if me.heat > 0 { WARN } else { FG },
    );
    if let Some(end) = &status.view.ended {
        let winner = status
            .crews
            .iter()
            .find(|c| c.player == end.winner)
            .map_or("a crew".to_string(), |c| c.name.clone());
        stat(
            p,
            icons::CROWN,
            format!("GAME OVER: {winner} wins"),
            match end.reason {
                EndReason::Domination => "By holding most of the home network",
                EndReason::DayLimit => "On points, on the last day",
            },
            GOOD,
        );
    } else {
        p.spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(5.0),
                flex_shrink: 0.0,
                ..default()
            },
            Interaction::default(),
            Tip("When the turn runs, whether or not every crew has handed in".into()),
        ))
        .with_children(|r| {
            r.spawn(theme::icon(icons::HOURGLASS, 14.0, theme::GOLD));
            r.spawn((
                theme::bold(countdown_text(session, clock), 13.0, FG),
                Countdown,
            ));
        });
    }
    let calendar = format!(
        "{} days pass when the turn runs{}",
        status.game.turn_days,
        status
            .game
            .end_day
            .map(|end| format!("; the game ends on {}", date(end)))
            .unwrap_or_default()
    );
    p.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexEnd,
            flex_shrink: 0.0,
            ..default()
        },
        Interaction::default(),
        Tip(calendar),
    ))
    .with_children(|c| {
        c.spawn(theme::bold(
            format!("TURN {}", status.turn),
            15.0,
            theme::GOLD_BRIGHT,
        ));
        c.spawn(text(date(status.day), 11.0, MUTED));
    });
}

// ------------------------------------------------------------ rail

fn rail(p: &mut ChildSpawnerCommands, open: &OpenMenu, guide: &GuideState, sounds: &Sounds) {
    for menu in Menu::RAIL {
        p.spawn(icon_button(
            menu.icon(),
            menu.tip(),
            Action::OpenMenu(menu),
            open.0 == Some(menu),
            theme::GOLD,
        ));
    }
    p.spawn(spacer());
    p.spawn(icon_button(
        icons::HELP,
        "Help: every step of the opening, and the rules",
        Action::ToggleHelp,
        guide.help,
        theme::GOLD,
    ));
    p.spawn(icon_button(
        icons::BOOK,
        "The story of NullNet",
        Action::StoryPage(0),
        guide.story.is_some(),
        theme::GOLD,
    ));
    p.spawn(icon_button(
        icons::TARGET,
        if guide.hidden {
            "Guide: hidden. Press to show it."
        } else {
            "Guide: shown. Press to hide it."
        },
        Action::ToggleGuide,
        false,
        if guide.hidden { MUTED } else { theme::GOLD },
    ));
    p.spawn(icon_button(
        if sounds.muted {
            icons::VOLUME_OFF
        } else {
            icons::VOLUME
        },
        if sounds.muted {
            "Sound: off"
        } else {
            "Sound: on"
        },
        Action::ToggleMute,
        false,
        if sounds.muted { MUTED } else { theme::GOLD },
    ));
    p.spawn(icon_button(
        icons::MUSIC,
        if sounds.music {
            "Music: on"
        } else {
            "Music: off"
        },
        Action::ToggleMusic,
        false,
        if sounds.music && !sounds.muted {
            theme::GOLD
        } else {
            MUTED
        },
    ));
    if notify::permission() == notify::Permission::Ask {
        p.spawn(icon_button(
            icons::BELL,
            "Let the browser tell you when a turn runs",
            Action::RequestNotify,
            false,
            theme::GOLD,
        ));
    }
}

// ------------------------------------------------------------ outliner

fn outliner(
    p: &mut ChildSpawnerCommands,
    status: &CrewStatus,
    data: &GameData,
    selected: &Selected,
) {
    let me = status.player;
    let home = data.hideout.host;
    let hideout = &status.view.me.hideout;
    let chosen_host = |host: HostId| selected.vessel.is_none() && selected.host == Some(host);

    heading(p, "Hideout");
    p.spawn(list_row(Action::SelectHost(home), chosen_host(home)))
        .with_children(|r| {
            r.spawn(theme::icon(icons::HOUSE, 14.0, theme::GOLD));
            r.spawn(theme::bold(data.host(home).name.clone(), 13.0, FG));
            r.spawn(spacer());
            r.spawn(text(
                format!(
                    "taps {}/{}  citadel {}/{}",
                    hideout.taps,
                    Site::MAX_TAPS,
                    hideout.citadel.modules,
                    Citadel::MODULES
                ),
                11.5,
                MUTED,
            ));
        });

    let held: Vec<HostId> = status
        .view
        .hosts
        .iter()
        .enumerate()
        .map(|(h, view)| (HostId(h as u16), view))
        .filter(|(h, view)| *h != home && view.controller == Some(Controller::Crew(me)))
        .map(|(h, _)| h)
        .collect();
    heading(p, &format!("Hosts ({})", held.len()));
    if held.is_empty() {
        muted(
            p,
            "None yet. A worm with a citadel module claims a free host.",
        );
    }
    for host in held {
        let view = &status.view.hosts[usize::from(host.0)];
        let threatened = status.view.threats.iter().any(|t| t.host == host);
        p.spawn(list_row(Action::SelectHost(host), chosen_host(host)))
            .with_children(|r| {
                r.spawn(theme::icon(
                    if view.citadel_modules >= Citadel::MODULES {
                        icons::CASTLE
                    } else {
                        icons::SERVER
                    },
                    14.0,
                    theme::GOLD,
                ));
                r.spawn(theme::bold(data.host(host).name.clone(), 13.0, FG));
                r.spawn(spacer());
                if threatened {
                    r.spawn((
                        theme::icon(icons::ALERT, 13.0, WARN),
                        Interaction::default(),
                        Tip("A Legacy swarm is coming for this host".into()),
                    ));
                }
                r.spawn(text(
                    format!("citadel {}/{}", view.citadel_modules, Citadel::MODULES),
                    11.5,
                    MUTED,
                ));
            });
    }

    let vessels: Vec<(VesselId, &Vessel)> = status
        .view
        .vessels
        .iter()
        .filter(|(_, v)| v.owner == me)
        .map(|(&id, v)| (id, v))
        .collect();
    heading(p, &format!("Vessels ({})", vessels.len()));
    if vessels.is_empty() {
        muted(p, "None yet. The guide shows how to build a dropper.");
    }
    for (id, vessel) in vessels {
        p.spawn(list_row(
            Action::SelectVessel(id),
            selected.vessel == Some(id),
        ))
        .with_children(|r| {
            r.spawn(theme::icon(icons::vessel(vessel.kind), 14.0, theme::GOLD));
            r.spawn(Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                ..default()
            })
            .with_children(|c| {
                c.spawn(theme::bold(
                    cap(text::vessel(id, &status.view.vessels)),
                    13.0,
                    FG,
                ));
                c.spawn(text(
                    format!(
                        "{}, {}",
                        text::state(vessel.state, data),
                        data.host(vessel.host).name
                    ),
                    11.0,
                    MUTED,
                ));
            });
            if vessel.pilot.is_none() {
                r.spawn((
                    theme::icon(icons::HEADSET, 13.0, WARN),
                    Interaction::default(),
                    Tip("No pilot: it cannot move".into()),
                ));
            }
            if vessel.fuel == 0 {
                r.spawn((
                    theme::icon(icons::FUEL, 13.0, WARN),
                    Interaction::default(),
                    Tip("No anonymisation left".into()),
                ));
            }
        });
    }

    if !status.view.threats.is_empty() {
        heading(p, "Threats");
        for threat in &status.view.threats {
            let when = match (threat.siege_until, threat.arrives) {
                (Some(until), _) => format!("siege, falls {}", date(until)),
                (None, Some(arrives)) => format!("arrives {}", date(arrives)),
                _ => "on its way".to_string(),
            };
            p.spawn(list_row(
                Action::SelectHost(threat.host),
                chosen_host(threat.host),
            ))
            .with_children(|r| {
                r.spawn(theme::icon(icons::ALERT, 14.0, WARN));
                r.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    flex_grow: 1.0,
                    ..default()
                })
                .with_children(|c| {
                    c.spawn(theme::bold(data.host(threat.host).name.clone(), 13.0, FG));
                    c.spawn(text(
                        format!("{} daemons, {when}", threat.daemons),
                        11.0,
                        WARN,
                    ));
                });
            });
        }
    }

    heading(p, "Crews");
    for crew in &status.crews {
        let colour = Color::Srgba(Srgba::hex(crew_color(me, crew.player)).unwrap());
        let summary = status.view.crews.iter().find(|c| c.player == crew.player);
        let points = status
            .view
            .scores
            .iter()
            .find(|s| s.player == crew.player)
            .map_or(0, |s| s.total);
        p.spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(7.0),
            padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(|r| {
            r.spawn(theme::icon(icons::HEXAGON, 13.0, colour));
            r.spawn(Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                ..default()
            })
            .with_children(|c| {
                c.spawn(theme::bold(
                    format!("{}{}", crew.name, if crew.bot { " (bot)" } else { "" }),
                    13.0,
                    colour,
                ));
                c.spawn(text(
                    format!(
                        "{} hosts, {} pts, heat {}",
                        summary.map_or(0, |c| c.hosts),
                        points,
                        summary.map_or(0, |c| c.heat)
                    ),
                    11.0,
                    MUTED,
                ));
            });
            if crew.submitted {
                r.spawn((
                    theme::icon(icons::CHECK, 14.0, GOOD),
                    Interaction::default(),
                    Tip("Handed in".into()),
                ));
            }
        });
    }
    if status.turn < PROTECTION_TURNS {
        icon_line(
            p,
            icons::SHIELD,
            format!("No raids between crews before turn {PROTECTION_TURNS}"),
            MUTED,
        );
    }
}

// ------------------------------------------------------------ turn box

fn turn_box(p: &mut ChildSpawnerCommands, session: &Session, data: &GameData) {
    let Some(status) = &session.status else {
        return;
    };
    p.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(8.0),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|r| {
        r.spawn(theme::bold(
            format!("ORDERS ({})", session.draft.len()),
            12.0,
            theme::GOLD,
        ));
        r.spawn(spacer());
        if !session.draft.is_empty() {
            r.spawn(small_icon_button(
                icons::UNDO,
                "Undo the last order",
                Action::Undo,
            ));
            r.spawn(small_icon_button(
                icons::TRASH,
                "Clear every order",
                Action::Clear,
            ));
        }
        r.spawn(small_icon_button(
            icons::LIST,
            "Every order, in the Orders menu",
            Action::OpenMenu(Menu::Orders),
        ));
    });
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
    const SHOWN: usize = 3;
    if session.draft.is_empty() {
        muted(
            p,
            "No orders yet. Handing in nothing is also a move: the days pass.",
        );
    }
    let skip = session.draft.len().saturating_sub(SHOWN);
    if skip > 0 {
        muted(p, format!("{skip} earlier, in Orders"));
    }
    for (index, command) in session.draft.iter().enumerate().skip(skip) {
        let entry = format!(
            "{}. {}",
            index + 1,
            text::command(command, data, &status.view.vessels)
        );
        match rejected.get(&index) {
            Some(error) => p.spawn(text(format!("{entry}: {error}"), 12.0, WARN)),
            None => p.spawn(text(entry, 12.0, FG)),
        };
    }
    if let Some(error) = &session.error {
        p.spawn(text(error.clone(), 12.0, WARN));
    } else if let Some(notice) = &session.notice {
        p.spawn(text(notice.clone(), 12.0, GOOD));
    }
    p.spawn(Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(5.0),
        margin: UiRect::top(Val::Auto),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|c| {
        if session.busy {
            c.spawn(text("Talking to the server...", 12.0, MUTED));
            return;
        }
        if status.submitted {
            c.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|r| {
                r.spawn(theme::icon(icons::CHECK, 14.0, GOOD));
                r.spawn(text("Handed in", 12.5, GOOD));
                r.spawn(spacer());
                r.spawn(button("Withdraw", Action::Withdraw));
            });
        }
        c.spawn(primary(
            if status.submitted {
                "HAND IN AGAIN"
            } else {
                "HAND IN"
            },
            Action::HandIn,
        ));
    });
}

// ------------------------------------------------------------ window

fn window_panel(
    p: &mut ChildSpawnerCommands,
    menu: Menu,
    status: &CrewStatus,
    session: &Session,
    data: &GameData,
    selected: &Selected,
    routing: &Routing,
) {
    p.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(8.0),
        margin: UiRect::bottom(Val::Px(2.0)),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|r| {
        r.spawn(theme::icon(menu.icon(), 17.0, theme::GOLD));
        r.spawn(theme::bold(
            menu.title().to_uppercase(),
            16.0,
            theme::GOLD_BRIGHT,
        ));
        r.spawn(spacer());
        r.spawn(small_icon_button(
            icons::CLOSE,
            "Close",
            Action::CloseWindow,
        ));
    });
    match menu {
        Menu::Hideout => hideout_menu(p, status),
        Menu::Research => research_menu(p, status, data),
        Menu::Recruit => recruit_menu(p, status, data),
        Menu::Workshop => workshop_menu(p, status, data),
        Menu::Citadel => citadel_menu(p, status, data),
        Menu::Orders => orders_panel(p, session, data),
        Menu::Log => log_panel(p, status, data),
        Menu::Crews => crews_menu(p, status),
        Menu::Selection => selection_panel(p, status, data, selected, routing),
    }
}

fn team_row(p: &mut ChildSpawnerCommands, kind: StaffKind, label: &str, staff: Option<&Staff>) {
    icon_line(
        p,
        icons::staff(kind),
        team_line(label, staff),
        if staff.is_some() { FG } else { MUTED },
    );
}

/// The line for a course under way, with the day its recruits graduate.
fn course_line(status: &CrewStatus, data: &GameData, kind: StaffKind) -> Option<String> {
    let course = status.view.me.recruitment.courses.get(&kind)?;
    if course.enrolled == 0 {
        return None;
    }
    let day = status.day;
    let graduation = course.started.unwrap_or(day) + data.recruitment.courses[&kind].days;
    Some(format!(
        "{} {}s in training, graduating {} ({} days)",
        course.enrolled,
        format!("{kind:?}").to_lowercase(),
        date(graduation),
        graduation.saturating_sub(day)
    ))
}

fn hideout_menu(p: &mut ChildSpawnerCommands, status: &CrewStatus) {
    let me = &status.view.me;
    let hideout = &me.hideout;
    icon_line(
        p,
        icons::PLUG,
        format!("Taps {}/{}", hideout.taps, Site::MAX_TAPS),
        FG,
    );
    icon_line(
        p,
        icons::CASTLE,
        format!(
            "Citadel {}/{} modules",
            hideout.citadel.modules,
            Citadel::MODULES
        ),
        FG,
    );
    icon_line(
        p,
        icons::USERS,
        format!("Recruits left {}", me.recruitment.available),
        FG,
    );

    heading(p, "Teams");
    team_row(p, StaffKind::Analyst, "Analysts", me.research_team.as_ref());
    team_row(p, StaffKind::Coder, "Coders", me.workshop.coders.as_ref());
    for staff in &hideout.staff {
        icon_line(
            p,
            icons::staff(staff.kind),
            format!("Waiting: {}", text::team(staff)),
            FG,
        );
    }

    heading(p, "Now");
    match me
        .current_research
        .and_then(|i| me.research.get(&i).map(|r| (i, r)))
    {
        Some((item, progress)) if !progress.researched => {
            icon_line(
                p,
                icons::FLASK,
                format!("Researching {} {}%", text::item(item), progress.percent),
                FG,
            );
            self::progress(p, f32::from(progress.percent) / 100.0);
        }
        _ => icon_line(p, icons::FLASK, "Researching nothing", MUTED),
    }
    match me.workshop.jobs.iter().find(|j| j.active) {
        Some(job) => {
            icon_line(
                p,
                icons::HAMMER,
                format!("Building {} (stage {}/4)", text::item(job.item), job.stage),
                FG,
            );
            self::progress(p, job.stage as f32 / 4.0);
        }
        None => icon_line(p, icons::HAMMER, "Building nothing", MUTED),
    }

    heading(p, "Store");
    store_lines(p, &hideout.store);
    if hideout.citadel.store.iter().next().is_some() {
        heading(p, "Citadel store");
        store_lines(p, &hideout.citadel.store);
    }
}

fn research_menu(p: &mut ChildSpawnerCommands, status: &CrewStatus, data: &GameData) {
    let me = &status.view.me;
    match &me.research_team {
        Some(team) => icon_line(
            p,
            icons::MICROSCOPE,
            format!("{}: level {}", team.leader, team.level()),
            FG,
        ),
        None => {
            muted(p, "No analysts yet: Recruit trains them.");
            return;
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
            self::progress(p, f32::from(progress.percent) / 100.0);
        }
        _ => muted(p, "Researching nothing: pick an item below."),
    }

    heading(p, "Open to research");
    p.spawn(row()).with_children(|r| {
        let mut open: Vec<ItemType> = me
            .research
            .iter()
            .filter(|(_, r)| !r.researched)
            .map(|(&i, _)| i)
            .collect();
        open.sort_by_key(|i| data.research[i].tech_level);
        if open.is_empty() {
            r.spawn(text("Nothing left to research for now.", 12.5, MUTED));
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

    let mut done: Vec<String> = me
        .research
        .iter()
        .filter(|(_, r)| r.researched)
        .map(|(&i, _)| text::item(i))
        .collect();
    if !done.is_empty() {
        done.sort();
        heading(p, "Researched");
        muted(p, done.join(", "));
    }
}

fn recruit_menu(p: &mut ChildSpawnerCommands, status: &CrewStatus, data: &GameData) {
    let me = &status.view.me;
    icon_line(
        p,
        icons::USERS,
        format!("Recruits left {}", me.recruitment.available),
        FG,
    );
    muted(
        p,
        "Analysts research, coders build, operators pilot and defend. A course takes a few turns; operators wait in the hideout until a vessel takes them.",
    );
    heading(p, "Courses");
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
                offered = true;
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
            r.spawn(text("Every course is running or full.", 12.5, MUTED));
        }
    });
    for kind in [StaffKind::Analyst, StaffKind::Coder, StaffKind::Operator] {
        if let Some(line) = course_line(status, data, kind) {
            icon_line(p, icons::staff(kind), line, GOOD);
        }
    }
    heading(p, "Teams");
    team_row(p, StaffKind::Analyst, "Analysts", me.research_team.as_ref());
    team_row(p, StaffKind::Coder, "Coders", me.workshop.coders.as_ref());
    let operators: Vec<&Staff> = me
        .hideout
        .staff
        .iter()
        .chain(&me.hideout.citadel.staff)
        .filter(|s| s.kind == StaffKind::Operator)
        .collect();
    if operators.is_empty() {
        icon_line(p, icons::HEADSET, "Operators: none waiting", MUTED);
    }
    for team in operators {
        icon_line(
            p,
            icons::HEADSET,
            format!("Operators waiting: {}", text::team(team)),
            FG,
        );
    }
}

fn workshop_menu(p: &mut ChildSpawnerCommands, status: &CrewStatus, data: &GameData) {
    let me = &status.view.me;
    let hideout = &me.hideout;
    match &me.workshop.coders {
        Some(team) => icon_line(
            p,
            icons::TERMINAL,
            format!("{}: level {}", team.leader, team.level()),
            FG,
        ),
        None => {
            muted(p, "No coders yet: Recruit trains them.");
            return;
        }
    }
    match me.workshop.jobs.iter().find(|j| j.active) {
        Some(job) => {
            line(
                p,
                format!("Building {} (stage {}/4)", text::item(job.item), job.stage),
            );
            self::progress(p, job.stage as f32 / 4.0);
        }
        None => muted(p, "Building nothing: pick an item below."),
    }
    if hideout.store.get(ItemType::Tap) > 0 && hideout.taps < Site::MAX_TAPS {
        heading(p, "Taps");
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
    heading(p, "Build in the hideout");
    muted(
        p,
        "One item at a time; starting another pauses the current one. The resources come from the hideout store.",
    );
    p.spawn(row()).with_children(|r| {
        let items = buildable(status, data, false);
        if items.is_empty() {
            r.spawn(text("Research something to build first.", 12.5, MUTED));
        }
        for item in items {
            r.spawn(button(
                text::item(item),
                Action::Order(Command::Build {
                    at: WorkshopRef::Hideout,
                    item,
                }),
            ));
        }
    });
    heading(p, "Store");
    store_lines(p, &hideout.store);
}

fn citadel_menu(p: &mut ChildSpawnerCommands, status: &CrewStatus, data: &GameData) {
    let me = &status.view.me;
    let hideout = &me.hideout;
    let citadel = &hideout.citadel;
    if !citadel.complete() {
        icon_line(
            p,
            icons::CASTLE,
            format!("{}/{} modules", citadel.modules, Citadel::MODULES),
            FG,
        );
        self::progress(p, citadel.modules as f32 / Citadel::MODULES as f32);
        muted(
            p,
            "The citadel above the hideout is built from eight citadel modules, lifted by the dropper and installed from outside. It brings a workshop and a store of its own; worms are built there.",
        );
        return;
    }
    team_row(
        p,
        StaffKind::Coder,
        "Coders",
        citadel.workshop.coders.as_ref(),
    );
    for staff in &citadel.staff {
        icon_line(
            p,
            icons::staff(staff.kind),
            format!("Waiting: {}", text::team(staff)),
            FG,
        );
    }
    if let Some(job) = citadel.workshop.jobs.iter().find(|j| j.active) {
        line(
            p,
            format!("Building {} (stage {}/4)", text::item(job.item), job.stage),
        );
        self::progress(p, job.stage as f32 / 4.0);
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
    });
    if citadel.workshop.coders.is_some() {
        heading(p, "Build in the citadel");
        p.spawn(row()).with_children(|r| {
            let at = WorkshopRef::Citadel(SiteRef::Hideout);
            for item in buildable(status, data, true) {
                r.spawn(button(
                    format!("Build {}", text::item(item)),
                    Action::Order(Command::Build { at, item }),
                ));
            }
        });
    }
    heading(p, "Citadel store");
    store_lines(p, &citadel.store);
}

fn crews_menu(p: &mut ChildSpawnerCommands, status: &CrewStatus) {
    muted(
        p,
        "10 points per complete citadel, 3 per other host, 15 per Legacy host freed, 5 per host taken from a rival, 2 per item researched.",
    );
    for crew in &status.crews {
        let colour = Color::Srgba(Srgba::hex(crew_color(status.player, crew.player)).unwrap());
        let score = status.view.scores.iter().find(|s| s.player == crew.player);
        let summary = status.view.crews.iter().find(|c| c.player == crew.player);
        p.spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            margin: UiRect::top(Val::Px(8.0)),
            flex_shrink: 0.0,
            ..default()
        })
        .with_children(|r| {
            r.spawn(theme::icon(icons::HEXAGON, 16.0, colour));
            r.spawn(theme::bold(
                format!("{}{}", crew.name, if crew.bot { " (bot)" } else { "" }),
                14.0,
                colour,
            ));
            r.spawn(spacer());
            r.spawn(theme::bold(
                format!("{} pts", score.map_or(0, |s| s.total)),
                14.0,
                theme::GOLD_BRIGHT,
            ));
        });
        if let Some(score) = score {
            muted(
                p,
                format!(
                    "{} citadels, {} other hosts, {} freed, {} taken, {} researched",
                    score.citadels, score.hosts, score.freed, score.taken, score.research
                ),
            );
        }
        muted(
            p,
            format!(
                "Heat {}{}",
                summary.map_or(0, |c| c.heat),
                if crew.submitted {
                    ", handed in"
                } else {
                    ", not handed in yet"
                }
            ),
        );
    }
    if status.turn < PROTECTION_TURNS {
        heading(p, "Protection");
        muted(
            p,
            format!("Crews cannot raid each other before turn {PROTECTION_TURNS}."),
        );
    }
}

fn team_line(label: &str, staff: Option<&Staff>) -> String {
    match staff {
        Some(s) => format!("{label}: {}", text::team(s)),
        None => format!("{label}: none"),
    }
}

/// What a store holds, two to a row, each with its icon.
fn store_lines(p: &mut ChildSpawnerCommands, store: &nullnet_core::Store) {
    let items: Vec<(ItemType, u32)> = store.iter().collect();
    if items.is_empty() {
        muted(p, "nothing yet");
        return;
    }
    p.spawn(Node {
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        row_gap: Val::Px(3.0),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|grid| {
        for (item, count) in items {
            grid.spawn(Node {
                width: Val::Percent(50.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|cell| {
                cell.spawn(theme::icon(icons::item(item), 13.0, theme::GOLD));
                cell.spawn(text(text::item(item), 12.5, FG));
                cell.spawn(spacer());
                cell.spawn((
                    theme::bold(count.to_string(), 12.5, FG),
                    Node {
                        margin: UiRect::right(Val::Px(10.0)),
                        ..default()
                    },
                ));
            });
        }
    });
}

// ------------------------------------------------------------ selection

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

/// What a host offers and how to act on it. Clicking a host only shows it;
/// everything done to a host is done by a vessel, so say which.
fn host_hints(p: &mut ChildSpawnerCommands, status: &CrewStatus, data: &GameData, host: HostId) {
    let def = data.host(host);
    let view = &status.view.hosts[usize::from(host.0)];
    if !def.resources.is_empty() {
        muted(
            p,
            format!(
                "Resources: {}",
                def.resources
                    .iter()
                    .map(|r| text::item(*r))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }
    if host != data.hideout.host {
        let days = nullnet_core::transport::latency(data, data.hideout.host, host);
        muted(p, format!("{days} day(s) from your hideout."));
    }
    let own = host == data.hideout.host || view.controller == Some(Controller::Crew(status.player));
    if own {
        return;
    }
    heading(p, "What you can do here");
    let hint = match view.controller {
        None if def.cache_field => {
            "Dig here: a worm with a sniffer fitted turns up source fragments, and a crawler mines the caches.".to_string()
        }
        None => "Claim it: select a worm carrying a citadel module, press 'Route to another host...' and click this host. Once the worm is outside it, press 'Install citadel module'. Eight modules make a citadel; backdoor kits and taps make it extract.".to_string(),
        Some(Controller::Legacy) => format!(
            "Only force takes it: a worm with a C2 controller and daemons outside it can 'Attack the garrison'. Freeing a Legacy host scores {} points.",
            nullnet_core::score::FREED_POINTS
        ),
        Some(Controller::Crew(_)) => format!(
            "A rival's. From turn {PROTECTION_TURNS} a worm with a C2 controller and daemons outside it can raid it: exfiltrate its store, plant a tap, or take the host."
        ),
    };
    muted(p, hint);
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
    host_hints(p, status, data, host);

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
