//! The map: the network as a graph of nodes and links. Every host is a
//! hexagon whose ring shows who holds it; the links between them are the
//! paths a crew works along. The camera fits the graph into the room the
//! panels leave. Clicking a node selects it.
//!
//! The nodes, links and backdrop are drawn by the shaders in
//! [`crate::materials`], so the client still ships no images.

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use nullnet_core::{Controller, GameData, HostId, PlayerId};

use crate::Rules;
use crate::materials::{
    BackgroundMaterial, BackgroundParams, LinkMaterial, LinkParams, NodeMaterial, NodeParams,
    linear,
};
use crate::net::Session;

/// World units per map unit (host positions run −100 to 100).
const SCALE: f32 = 5.0;
/// The graph plus a margin, for the camera to frame.
const VIEW: Vec2 = Vec2::new(1180.0, 1180.0);

const CREW_COLORS: [&str; 4] = ["4fd08a", "e0a441", "6fb8e0", "c874d8"];
const LEGACY_RING: &str = "e06a5a";
const FREE_RING: &str = "6a7a8a";

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Selected>()
            .init_resource::<MapInsets>()
            .add_systems(Startup, spawn_scene)
            .add_systems(
                Update,
                (
                    fit_camera,
                    click,
                    recolor.run_if(resource_changed::<Session>),
                    animate_highlight,
                ),
            );
    }
}

/// The host the player has clicked, if any.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub struct Selected(pub Option<HostId>);

/// How much of the screen the panels cover: left, top, right, bottom, in
/// logical pixels. The camera frames the graph in what is left.
#[derive(Resource, Clone, Copy)]
pub struct MapInsets(pub Vec4);

impl Default for MapInsets {
    fn default() -> Self {
        // A guess until the UI measures itself on the first frame.
        MapInsets(Vec4::new(300.0, 56.0, 16.0, 150.0))
    }
}

#[derive(Component)]
struct MapNode {
    host: HostId,
    /// How far from the node centre a click still counts, in world units.
    reach: f32,
    /// The highlight the node is easing towards (0 rest, up to 1).
    target: f32,
}

/// The colour a crew is drawn in: you first, then the rivals in turn.
pub fn crew_color(me: PlayerId, crew: PlayerId) -> &'static str {
    if crew == me {
        return CREW_COLORS[0];
    }
    let rivals: Vec<u8> = (0..4u8).filter(|&c| c != me.0).collect();
    let index = rivals.iter().position(|&c| c == crew.0).unwrap_or(0);
    CREW_COLORS[1 + index.min(2)]
}

fn node_radius(data: &GameData, host: HostId) -> f32 {
    use nullnet_core::Role::*;
    match data.host(host).role {
        Hideout => 22.0,
        Cortex => 26.0,
        Grid | Stronghold => 20.0,
        _ => 16.0,
    }
}

fn node_params(fill: Vec4, ring: Vec4, seed: f32) -> NodeParams {
    NodeParams {
        fill,
        ring,
        shape: Vec4::new(0.0, seed, 0.0, 1.6),
        quad: Vec4::new(1.25, 1.0, 0.0, 0.0),
    }
}

fn world(data: &GameData, host: HostId) -> Vec2 {
    let (x, y) = data.host(host).pos;
    Vec2::new(f32::from(x), f32::from(y)) * SCALE
}

fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut nodes: ResMut<Assets<NodeMaterial>>,
    mut links: ResMut<Assets<LinkMaterial>>,
    mut backgrounds: ResMut<Assets<BackgroundMaterial>>,
    rules: Res<Rules>,
) {
    let data = &rules.0;

    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::WindowSize,
            ..OrthographicProjection::default_2d()
        }),
    ));

    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(VIEW.x * 4.0, VIEW.y * 4.0))),
        MeshMaterial2d(backgrounds.add(BackgroundMaterial {
            params: BackgroundParams {
                offset: Vec4::ZERO,
                grid: linear("1d2a3a"),
                fog_a: linear("07121c"),
                fog_b: linear("0a1f28"),
            },
        })),
        Transform::from_xyz(0.0, 0.0, -10.0),
    ));

    for &(a, b) in &data.links {
        let from = world(data, a);
        let to = world(data, b);
        let delta = to - from;
        let length = delta.length();
        commands.spawn((
            Mesh2d(meshes.add(Rectangle::new(length, 2.5))),
            MeshMaterial2d(links.add(LinkMaterial {
                params: LinkParams {
                    color: linear("3a5a74"),
                    shape: Vec4::new(length, 2.5, 80.0, (a.0 + b.0) as f32),
                },
            })),
            Transform::from_translation(((from + to) * 0.5).extend(-5.0))
                .with_rotation(Quat::from_rotation_z(delta.y.atan2(delta.x))),
        ));
    }

    for host in data.host_ids() {
        let radius = node_radius(data, host);
        let def = data.host(host);
        let (fill, ring) = rest_colors(def.legacy);
        let seed = f32::from(host.0) * 0.137;
        commands.spawn((
            Mesh2d(meshes.add(Rectangle::from_length(2.0 * 1.25 * radius))),
            MeshMaterial2d(nodes.add(NodeMaterial {
                params: node_params(fill, ring, seed),
            })),
            MapNode {
                host,
                reach: radius * 1.2,
                target: 0.0,
            },
            Transform::from_translation(world(data, host).extend(1.0)),
        ));
        commands.spawn((
            Text2d::new(def.name.to_uppercase()),
            TextFont::from_font_size(11.0),
            TextColor(Color::srgba(0.82, 0.88, 0.94, 0.85)),
            Transform::from_translation(
                (world(data, host) + Vec2::new(0.0, -radius - 9.0)).extend(2.0),
            ),
        ));
    }
}

/// The colours a host rests in before any game state is known.
fn rest_colors(legacy: bool) -> (Vec4, Vec4) {
    if legacy {
        (linear("33121f"), with_strength(linear(LEGACY_RING), 1.0))
    } else {
        (linear("16293c"), with_strength(linear(FREE_RING), 0.45))
    }
}

fn with_strength(mut color: Vec4, strength: f32) -> Vec4 {
    color.w = strength;
    color
}

/// Colours a host by who holds it, from the crew's own view.
fn controller_colors(session: &Session, host: HostId) -> (Vec4, Vec4) {
    let Some(status) = session.status.as_ref() else {
        return rest_colors(false);
    };
    let view = &status.view;
    let me = view.player;
    let hv = &view.hosts[host.index()];
    let is_my_hideout = host == view.me.hideout;
    match (hv.controller, is_my_hideout) {
        (_, true) => (
            linear("2a2612"),
            with_strength(linear(crew_color(me, me)), 0.95),
        ),
        (Some(Controller::Legacy), _) => {
            (linear("33121f"), with_strength(linear(LEGACY_RING), 1.0))
        }
        (Some(Controller::Crew(crew)), _) => (
            linear("12302a"),
            with_strength(linear(crew_color(me, crew)), 0.9),
        ),
        (None, _) => (linear("16293c"), with_strength(linear(FREE_RING), 0.45)),
    }
}

fn recolor(
    session: Res<Session>,
    selected: Res<Selected>,
    nodes: Query<(&mut MapNode, &MeshMaterial2d<NodeMaterial>)>,
    mut materials: ResMut<Assets<NodeMaterial>>,
) {
    let _ = &selected; // highlight is set by its own system
    for (node, handle) in nodes {
        if let Some(mut material) = materials.get_mut(handle.id()) {
            let (fill, ring) = controller_colors(&session, node.host);
            material.params.fill = fill;
            material.params.ring = ring;
        }
    }
}

fn fit_camera(
    time: Res<Time>,
    window: Single<&Window, With<PrimaryWindow>>,
    insets: Res<MapInsets>,
    camera: Single<(&mut Transform, &mut Projection), With<Camera2d>>,
    mut placed: Local<bool>,
) {
    let size = window.size();
    let inset = insets.0;
    let free =
        Vec2::new(size.x - inset.x - inset.z, size.y - inset.y - inset.w).max(Vec2::splat(140.0));
    // World units per logical pixel, with a little room around the edges.
    let scale = (VIEW.x / free.x).max(VIEW.y / free.y) * 1.04;
    // Shift the camera so the graph centres in the free area, not the window.
    let target = Vec2::new(
        (inset.x - inset.z) * 0.5 * scale,
        -(inset.y - inset.w) * 0.5 * scale,
    );

    let (mut transform, mut projection) = camera.into_inner();
    let Projection::Orthographic(current) = &*projection else {
        return;
    };
    let follow = if *placed {
        1.0 - (-time.delta_secs() * 7.0).exp()
    } else {
        1.0
    };
    *placed = true;
    let next_scale = current.scale + (scale - current.scale) * follow;
    if let Projection::Orthographic(ortho) = projection.as_mut() {
        ortho.scale = next_scale;
    }
    let now = transform.translation.truncate();
    let next = now.lerp(target, follow);
    transform.translation.x = next.x;
    transform.translation.y = next.y;
}

fn click(
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    insets: Res<MapInsets>,
    camera: Single<(&Camera, &GlobalTransform)>,
    nodes: Query<(&MapNode, &GlobalTransform)>,
    mut selected: ResMut<Selected>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    // Ignore clicks over the panels.
    let size = window.size();
    let inset = insets.0;
    if cursor.x < inset.x
        || cursor.x > size.x - inset.z
        || cursor.y < inset.y
        || cursor.y > size.y - inset.w
    {
        return;
    }
    let (camera, camera_transform) = *camera;
    let Ok(point) = camera.viewport_to_world_2d(camera_transform, cursor) else {
        return;
    };
    let found = nodes
        .iter()
        .filter_map(|(node, transform)| {
            let distance = transform.translation().truncate().distance(point);
            (distance < node.reach).then_some((node.host, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(host, _)| host);
    if let Some(host) = found {
        *selected = Selected(Some(host));
    } else {
        *selected = Selected(None);
    }
}

/// Eases each node's glow towards its target: bright when selected, a softer
/// glow when the crew can reach it this turn.
fn animate_highlight(
    time: Res<Time>,
    session: Res<Session>,
    selected: Res<Selected>,
    nodes: Query<(&mut MapNode, &MeshMaterial2d<NodeMaterial>)>,
    mut materials: ResMut<Assets<NodeMaterial>>,
) {
    let view = session.status.as_ref().map(|s| &s.view);
    let step = time.delta_secs() * 6.0;
    for (mut node, handle) in nodes {
        let reachable = view
            .map(|v| {
                let hv = &v.hosts[node.host.index()];
                hv.can_break_in || hv.can_scan || hv.access
            })
            .unwrap_or(false);
        node.target = if selected.0 == Some(node.host) {
            1.0
        } else if reachable {
            0.45
        } else {
            0.0
        };
        if let Some(mut material) = materials.get_mut(handle.id()) {
            let current = material.params.shape.z;
            material.params.shape.z = current + (node.target - current).clamp(-step, step);
        }
    }
}
