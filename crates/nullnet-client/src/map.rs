//! The network map: the home network's hosts as nodes along a trunk from
//! the backbone, each host's subsystems hanging under it, data lines
//! between them, and the crew's vessels at their berths. Clicking selects
//! a host or vessel; while a vessel is being routed, clicking a host sends
//! it there.
//!
//! The layout comes from the game's data, the colours from the crew's view
//! of the world.

use std::collections::HashMap;

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::window::PrimaryWindow;
use nullnet_core::{
    Berth, Citadel, Command, Controller, CrewView, Destination, Event, GameData, HostDef, HostId,
    ItemType, NetworkDef, PlayerId, VesselId, VesselState,
};

use crate::Rules;
use crate::materials::{
    BackgroundMaterial, BackgroundParams, LinkMaterial, LinkParams, NodeKind, NodeMaterial,
    NodeParams, linear,
};
use crate::net::Session;
use crate::ui::OverUi;

/// The area the camera always keeps on screen, in world units.
const VIEW_SIZE: Vec2 = Vec2::new(1600.0, 900.0);
/// Where the backbone beam stands.
const BACKBONE_X: f32 = -445.0;
/// The trunk every host sits on.
const TRUNK_Y: f32 = 150.0;
/// The first host's x and the spacing between hosts along the trunk.
const FIRST_HOST_X: f32 = -365.0;
const HOST_SPACING: f32 = 77.0;
/// Where a host's first subsystem hangs, and the spacing down the column.
const SUBSYSTEM_DROP: f32 = 64.0;
const SUBSYSTEM_SPACING: f32 = 34.0;
const SUBSYSTEM_RADIUS: f32 = 8.0;
const MARKER_RADIUS: f32 = 5.0;

const TRUNK_COLOR: &str = "4f8fb3";
const FREE_RING: &str = "4e7f9e";
const LEGACY_RING: &str = "ff4d6a";
const CACHE_COLOR: &str = "9c8a62";
/// The crew's own colour, then its rivals' in player order.
const CREW_COLORS: [&str; 4] = ["3fe0c8", "f09357", "b58cff", "9bd36a"];

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Hovered>()
            .init_resource::<Selected>()
            .init_resource::<Routing>()
            .init_resource::<Layout>()
            .add_systems(Startup, spawn_scene)
            .add_systems(
                Update,
                (
                    update_hover,
                    click,
                    animate_highlight,
                    decay_flash,
                    parallax,
                    (recolor, place_vessels).run_if(resource_changed::<Session>),
                    place_vessels.run_if(resource_changed::<Selected>),
                ),
            );
    }
}

/// What a node on the map stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Backbone,
    Host(HostId),
    Vessel(VesselId),
}

/// A node the cursor can pick.
#[derive(Component)]
pub struct MapNode {
    pub target: Target,
    /// How far from the centre the cursor still counts as over the node.
    reach: f32,
}

/// A vessel's marker; rebuilt whenever the view changes.
#[derive(Component)]
struct VesselMarker;

#[derive(Resource, Default, PartialEq)]
pub struct Hovered(pub Option<Target>);

/// What the player has clicked on.
#[derive(Resource, Default, PartialEq, Eq)]
pub struct Selected {
    pub host: Option<HostId>,
    pub vessel: Option<VesselId>,
}

/// A vessel waiting for the player to click the host to route it to.
#[derive(Resource, Default, PartialEq, Eq)]
pub struct Routing(pub Option<VesselId>);

/// Where each host sits, for markers and the HUD.
#[derive(Resource, Default)]
pub struct Layout {
    pub hosts: HashMap<HostId, (Vec2, f32)>,
}

/// The colour a host or vessel takes from whoever holds it.
pub fn crew_color(me: PlayerId, crew: PlayerId) -> &'static str {
    if crew == me {
        CREW_COLORS[0]
    } else {
        let rivals: Vec<u8> = (0..4u8).filter(|&c| c != me.0).collect();
        let index = rivals.iter().position(|&c| c == crew.0).unwrap_or(0);
        CREW_COLORS[1 + index.min(2)]
    }
}

fn controller_colors(
    data: &GameData,
    view: Option<&CrewView>,
    id: HostId,
    def: &HostDef,
) -> (Vec4, Vec4) {
    let me = view.map(|v| v.player);
    let controller = view
        .map(|v| v.hosts[usize::from(id.0)].controller)
        .unwrap_or(def.legacy.then_some(Controller::Legacy));
    let is_home = id == data.hideout.host;
    let (ring, strength, fill) = match (controller, is_home, me) {
        (_, true, Some(me)) => (crew_color(me, me), 0.9, "2a2612"),
        (_, true, None) => ("ffd27a", 0.9, "2a2612"),
        (Some(Controller::Legacy), _, _) => (LEGACY_RING, 1.0, "33121f"),
        (Some(Controller::Crew(crew)), _, Some(me)) => (crew_color(me, crew), 0.9, "12302a"),
        (Some(Controller::Crew(_)), _, None) => (CREW_COLORS[1], 0.9, "12302a"),
        (None, _, _) => (FREE_RING, 0.45, "16293c"),
    };
    let mut ring = linear(ring);
    ring.w = strength;
    (linear(fill), ring)
}

/// The network the hideouts are in: the one the map shows.
pub fn home_network(data: &GameData) -> &NetworkDef {
    &data.networks[usize::from(data.host(data.hideout.host).network.0)]
}

fn node_params(kind: NodeKind, fill: Vec4, ring: Vec4, seed: f32, extent: f32) -> NodeParams {
    let kind = match kind {
        NodeKind::Host => 0.0,
        NodeKind::Subsystem => 1.0,
        NodeKind::Backbone => 2.0,
        NodeKind::CacheField => 3.0,
    };
    NodeParams {
        fill,
        ring,
        shape: Vec4::new(kind, seed, 0.0, 1.6),
        quad: Vec4::new(extent, 1.0, 0.0, 0.0),
    }
}

fn spawn_link(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    links: &mut Assets<LinkMaterial>,
    from: Vec2,
    to: Vec2,
    thickness: f32,
    seed: f32,
) {
    let delta = to - from;
    let length = delta.length();
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(length, thickness))),
        MeshMaterial2d(links.add(LinkMaterial {
            params: LinkParams {
                color: linear(TRUNK_COLOR),
                shape: Vec4::new(length, thickness, 90.0, seed),
            },
        })),
        Transform::from_translation(((from + to) * 0.5).extend(-5.0))
            .with_rotation(Quat::from_rotation_z(delta.y.atan2(delta.x))),
    ));
}

fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut nodes: ResMut<Assets<NodeMaterial>>,
    mut links: ResMut<Assets<LinkMaterial>>,
    mut backgrounds: ResMut<Assets<BackgroundMaterial>>,
    mut layout: ResMut<Layout>,
    rules: Res<Rules>,
) {
    let data = &rules.0;
    let home = home_network(data);
    let home_id = data.host(data.hideout.host).network;

    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: VIEW_SIZE.x,
                min_height: VIEW_SIZE.y,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));

    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(VIEW_SIZE.x * 4.0, VIEW_SIZE.y * 4.0))),
        MeshMaterial2d(backgrounds.add(BackgroundMaterial {
            params: BackgroundParams {
                offset: Vec4::ZERO,
                grid: linear("23405a"),
                fog_a: linear("07121c"),
                fog_b: linear("0a2a33"),
            },
        })),
        Transform::from_xyz(0.0, 0.0, -10.0),
    ));

    // The backbone: a beam the trunk line leaves from.
    let beam = Vec2::new(120.0, VIEW_SIZE.y * 1.2);
    let mut params = node_params(
        NodeKind::Backbone,
        linear("1b3f5a"),
        linear("8fd8ff"),
        0.3,
        1.0,
    );
    params.quad.y = beam.y / beam.x;
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(beam.x, beam.y))),
        MeshMaterial2d(nodes.add(NodeMaterial { params })),
        Transform::from_xyz(BACKBONE_X, 0.0, -4.0),
        MapNode {
            target: Target::Backbone,
            reach: beam.x * 0.35,
        },
    ));
    commands.spawn((
        Text2d::new(home.name.to_uppercase()),
        TextFont::from_font_size(11.0),
        TextColor(Color::srgba(0.72, 0.84, 0.95, 0.75)),
        Transform::from_xyz(BACKBONE_X, TRUNK_Y + 60.0, 1.0),
    ));

    let hosts: Vec<(HostId, &HostDef)> = data
        .hosts
        .iter()
        .enumerate()
        .map(|(index, def)| (HostId(index as u16), def))
        .filter(|(_, def)| def.network == home_id)
        .collect();
    let subsystems_of = |parent: HostId| -> Vec<(HostId, &HostDef)> {
        hosts
            .iter()
            .filter(|(_, def)| def.parent == Some(parent))
            .copied()
            .collect()
    };
    let top: Vec<(HostId, &HostDef)> = hosts
        .iter()
        .filter(|(_, def)| def.parent.is_none())
        .copied()
        .collect();
    let last_x = FIRST_HOST_X + HOST_SPACING * (top.len() - 1) as f32;
    spawn_link(
        &mut commands,
        &mut meshes,
        &mut links,
        Vec2::new(BACKBONE_X + 30.0, TRUNK_Y),
        Vec2::new(last_x + 40.0, TRUNK_Y),
        8.0,
        0.5,
    );

    let label = TextColor(Color::srgba(0.72, 0.84, 0.95, 0.75));
    let small_label = TextColor(Color::srgba(0.6, 0.72, 0.85, 0.6));

    for (host, def) in &top {
        let subsystems = subsystems_of(*host);
        let x = FIRST_HOST_X + HOST_SPACING * def.order as f32;
        let centre = Vec2::new(x, TRUNK_Y);
        let seed = def.order as f32 * 1.37 + 0.5;
        let (fill, ring) = controller_colors(data, None, *host, def);

        let radius = if def.cache_field || *host == data.hideout.host {
            26.0
        } else {
            18.0 + 1.2 * subsystems.len() as f32
        };
        layout.hosts.insert(*host, (centre, radius));
        if def.cache_field {
            let mut color = linear(CACHE_COLOR);
            color.w = 0.6;
            commands.spawn((
                Mesh2d(meshes.add(Rectangle::from_length(2.0 * 1.2 * radius))),
                MeshMaterial2d(nodes.add(NodeMaterial {
                    params: node_params(NodeKind::CacheField, linear("5b5240"), color, seed, 1.2),
                })),
                Transform::from_translation(centre.extend(0.0)),
                MapNode {
                    target: Target::Host(*host),
                    reach: radius,
                },
            ));
        } else {
            let extent = 1.7;
            commands.spawn((
                Mesh2d(meshes.add(Rectangle::from_length(2.0 * extent * radius))),
                MeshMaterial2d(nodes.add(NodeMaterial {
                    params: node_params(NodeKind::Host, fill, ring, seed, extent),
                })),
                Transform::from_translation(centre.extend(0.0)),
                MapNode {
                    target: Target::Host(*host),
                    reach: radius * 1.15,
                },
            ));
        }
        commands.spawn((
            Text2d::new(def.name.to_uppercase()),
            TextFont::from_font_size(10.0),
            label,
            Transform::from_xyz(x, TRUNK_Y + 46.0, 1.0),
        ));

        if subsystems.is_empty() {
            continue;
        }
        let bottom = TRUNK_Y - SUBSYSTEM_DROP - SUBSYSTEM_SPACING * (subsystems.len() - 1) as f32;
        spawn_link(
            &mut commands,
            &mut meshes,
            &mut links,
            Vec2::new(x, TRUNK_Y - 8.0),
            Vec2::new(x, bottom),
            4.0,
            seed + 3.0,
        );
        for (index, (sub, sub_def)) in subsystems.iter().enumerate() {
            let y = TRUNK_Y - SUBSYSTEM_DROP - SUBSYSTEM_SPACING * index as f32;
            let centre = Vec2::new(x, y);
            layout.hosts.insert(*sub, (centre, SUBSYSTEM_RADIUS));
            let (fill, ring) = controller_colors(data, None, *sub, sub_def);
            let extent = 2.2;
            commands.spawn((
                Mesh2d(meshes.add(Rectangle::from_length(2.0 * extent * SUBSYSTEM_RADIUS))),
                MeshMaterial2d(nodes.add(NodeMaterial {
                    params: node_params(
                        NodeKind::Subsystem,
                        fill,
                        ring,
                        seed + index as f32 * 0.71,
                        extent,
                    ),
                })),
                Transform::from_translation(centre.extend(0.0)),
                MapNode {
                    target: Target::Host(*sub),
                    reach: SUBSYSTEM_RADIUS * 1.8,
                },
            ));
            commands.spawn((
                Text2d::new(sub_def.name.clone()),
                TextFont::from_font_size(9.0),
                small_label,
                TextLayout::justify(Justify::Left),
                Anchor::CENTER_LEFT,
                Transform::from_xyz(x + SUBSYSTEM_RADIUS + 7.0, y, 1.0),
            ));
        }
    }
}

/// The usual pulse of a node's glow, and the faster ones of a host with a
/// swarm on its way or at its gate.
const PULSE: f32 = 1.6;
const PULSE_SIGHTED: f32 = 4.5;
const PULSE_SIEGE: f32 = 9.0;

/// Recolours every host by whoever holds it in the latest view, makes the
/// threatened ones pulse, and flashes those that changed hands in the turn
/// that just ran.
fn recolor(
    session: Res<Session>,
    rules: Res<Rules>,
    nodes: Query<(&MapNode, &MeshMaterial2d<NodeMaterial>)>,
    mut materials: ResMut<Assets<NodeMaterial>>,
    mut flashed_turn: Local<Option<u32>>,
) {
    let Some(status) = &session.status else {
        return;
    };
    let data = &rules.0;
    let flash_now = *flashed_turn != Some(status.turn);
    *flashed_turn = Some(status.turn);
    let changed: Vec<HostId> = status
        .last_turn
        .iter()
        .flat_map(|t| &t.report.events)
        .filter_map(|e| match e {
            Event::HostClaimed { host, .. }
            | Event::HostCaptured { host, .. }
            | Event::HostFreed { host, .. }
            | Event::HostTaken { host, .. } => Some(*host),
            Event::Installed {
                host,
                item: ItemType::CitadelModule,
                installed,
                ..
            } if *installed == Citadel::MODULES => Some(*host),
            _ => None,
        })
        .collect();
    for (node, material) in &nodes {
        let Target::Host(id) = node.target else {
            continue;
        };
        let def = data.host(id);
        if def.cache_field {
            continue;
        }
        let (fill, mut ring) = controller_colors(data, Some(&status.view), id, def);
        let threat = status.view.threats.iter().find(|t| t.host == id);
        let pulse = match threat {
            Some(t) if t.siege_until.is_some() => {
                ring = linear(LEGACY_RING);
                ring.w = 1.3;
                PULSE_SIEGE
            }
            Some(_) => {
                ring.w += 0.3;
                PULSE_SIGHTED
            }
            None => PULSE,
        };
        if let Some(mut material) = materials.get_mut(material.id()) {
            material.params.fill = fill;
            material.params.ring = ring;
            material.params.shape.w = pulse;
            if flash_now && changed.contains(&id) {
                material.params.quad.z = 1.0;
            }
        }
    }
}

/// Lets a flash die down over a couple of seconds.
fn decay_flash(time: Res<Time>, mut materials: ResMut<Assets<NodeMaterial>>) {
    let step = time.delta_secs() * 0.4;
    let flashing: Vec<AssetId<NodeMaterial>> = materials
        .iter()
        .filter(|(_, m)| m.params.quad.z > 0.0)
        .map(|(id, _)| id)
        .collect();
    for id in flashing {
        if let Some(mut material) = materials.get_mut(id) {
            material.params.quad.z = (material.params.quad.z - step).max(0.0);
        }
    }
}

/// Where a vessel's marker sits beside its host.
fn berth_offset(state: VesselState, radius: f32, slot: usize) -> Vec2 {
    let stack = slot as f32 * (2.0 * MARKER_RADIUS + 3.0);
    match state {
        VesselState::At(Berth::Planted) | VesselState::Injecting { .. } => {
            Vec2::new(-(radius + 12.0) - stack, -8.0)
        }
        VesselState::At(Berth::Connected) | VesselState::Connecting => {
            Vec2::new(stack - 6.0, radius + 12.0)
        }
        _ => Vec2::new(radius + 12.0 + stack, 8.0),
    }
}

/// Rebuilds the vessel markers from the latest view.
fn place_vessels(
    mut commands: Commands,
    session: Res<Session>,
    selected: Res<Selected>,
    layout: Res<Layout>,
    old: Query<Entity, With<VesselMarker>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut nodes: ResMut<Assets<NodeMaterial>>,
) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let Some(status) = &session.status else {
        return;
    };
    let mut slots: HashMap<(HostId, u8), usize> = HashMap::new();
    for (&id, vessel) in &status.view.vessels {
        let Some(&(centre, radius)) = layout.hosts.get(&vessel.host) else {
            continue;
        };
        let group = match vessel.state {
            VesselState::At(Berth::Planted) | VesselState::Injecting { .. } => 0,
            VesselState::At(Berth::Connected) | VesselState::Connecting => 1,
            _ => 2,
        };
        let slot = slots.entry((vessel.host, group)).or_default();
        let position = centre + berth_offset(vessel.state, radius, *slot);
        *slot += 1;

        let color = crew_color(status.player, vessel.owner);
        let mut ring = linear(color);
        ring.w = if vessel.owner == status.player {
            0.9
        } else {
            0.6
        };
        let mut params = node_params(
            NodeKind::Subsystem,
            linear(color) * 0.5,
            ring,
            id.0 as f32 * 0.37,
            2.4,
        );
        params.shape.z = if selected.vessel == Some(id) {
            1.0
        } else {
            0.0
        };
        commands.spawn((
            VesselMarker,
            Mesh2d(meshes.add(Rectangle::from_length(2.0 * 2.4 * MARKER_RADIUS))),
            MeshMaterial2d(nodes.add(NodeMaterial { params })),
            Transform::from_translation(position.extend(0.5)),
            MapNode {
                target: Target::Vessel(id),
                reach: MARKER_RADIUS * 2.2,
            },
        ));
    }
}

fn update_hover(
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform)>,
    nodes: Query<(&MapNode, &GlobalTransform)>,
    over_ui: Res<OverUi>,
    mut hovered: ResMut<Hovered>,
) {
    let (camera, camera_transform) = *camera;
    let found = window
        .cursor_position()
        .filter(|_| !over_ui.0)
        .and_then(|cursor| camera.viewport_to_world_2d(camera_transform, cursor).ok())
        .and_then(|point| {
            // The nearest node whose reach covers the cursor, so a marker
            // beside a big host still wins when the cursor is on it.
            nodes
                .iter()
                .filter_map(|(node, transform)| {
                    let distance = transform.translation().truncate().distance(point);
                    (node.reach > 0.0 && distance < node.reach)
                        .then_some((node.target, distance / node.reach))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(target, _)| target)
        });
    hovered.set_if_neq(Hovered(found));
}

/// Selects what is clicked, or routes the vessel being sent.
fn click(
    mouse: Res<ButtonInput<MouseButton>>,
    hovered: Res<Hovered>,
    over_ui: Res<OverUi>,
    mut selected: ResMut<Selected>,
    mut routing: ResMut<Routing>,
    mut session: ResMut<Session>,
) {
    if !mouse.just_pressed(MouseButton::Left) || over_ui.0 {
        return;
    }
    match (routing.0, hovered.0) {
        (Some(vessel), Some(Target::Host(host))) => {
            session.draft.push(Command::Dispatch {
                vessel,
                to: Destination {
                    host,
                    berth: Berth::Lurking,
                },
            });
            routing.0 = None;
        }
        (Some(_), _) => routing.0 = None,
        (None, Some(Target::Host(host))) => {
            selected.set_if_neq(Selected {
                host: Some(host),
                vessel: None,
            });
        }
        (None, Some(Target::Vessel(id))) => {
            let host = session
                .status
                .as_ref()
                .and_then(|s| s.view.vessels.get(&id))
                .map(|v| v.host);
            selected.set_if_neq(Selected {
                host,
                vessel: Some(id),
            });
        }
        (None, Some(Target::Backbone)) | (None, None) => {
            selected.set_if_neq(Selected::default());
        }
    }
}

fn animate_highlight(
    time: Res<Time>,
    hovered: Res<Hovered>,
    selected: Res<Selected>,
    nodes: Query<(&MapNode, &MeshMaterial2d<NodeMaterial>)>,
    mut materials: ResMut<Assets<NodeMaterial>>,
) {
    let step = time.delta_secs() * 6.0;
    for (node, material) in &nodes {
        let is_selected = match node.target {
            Target::Host(h) => selected.host == Some(h) && selected.vessel.is_none(),
            Target::Vessel(v) => selected.vessel == Some(v),
            Target::Backbone => false,
        };
        let target = if hovered.0 == Some(node.target) || is_selected {
            1.0
        } else {
            0.0
        };
        let Some(current) = materials.get(material.id()).map(NodeMaterial::highlight) else {
            continue;
        };
        if current != target
            && let Some(mut material) = materials.get_mut(material.id())
        {
            material.set_highlight(current + (target - current).clamp(-step, step));
        }
    }
}

/// Shifts the backdrop slightly against the cursor for a sense of depth.
fn parallax(
    time: Res<Time>,
    window: Single<&Window, With<PrimaryWindow>>,
    background: Single<&MeshMaterial2d<BackgroundMaterial>>,
    mut materials: ResMut<Assets<BackgroundMaterial>>,
    mut smoothed: Local<Vec2>,
) {
    let target = window
        .cursor_position()
        .map(|cursor| (cursor / window.size() - 0.5) * Vec2::new(-60.0, 60.0))
        .unwrap_or(Vec2::ZERO);
    let previous = *smoothed;
    *smoothed = previous.lerp(target, 1.0 - (-time.delta_secs() * 3.0).exp());
    if previous.distance_squared(*smoothed) > 1e-6
        && let Some(mut material) = materials.get_mut(background.id())
    {
        material.params.offset = smoothed.extend(0.0).extend(0.0);
    }
}
