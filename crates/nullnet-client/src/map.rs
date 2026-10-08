//! The network map: the home network's hosts as nodes along a trunk from
//! the backbone, each host's subsystems hanging under it, with data lines
//! between them and information about the node under the cursor.
//!
//! The map is built from the game's own data, so it shows exactly the hosts
//! the rules know.

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::window::PrimaryWindow;
use nullnet_core::{EPOCH, GameData, HostDef, HostId, NetworkDef, date};

use crate::materials::{
    BackgroundMaterial, BackgroundParams, LinkMaterial, LinkParams, NodeKind, NodeMaterial,
    NodeParams, linear,
};

/// The area the camera always keeps on screen, in world units.
const VIEW_SIZE: Vec2 = Vec2::new(1600.0, 900.0);
/// Where the backbone beam stands.
const BACKBONE_X: f32 = -730.0;
/// The trunk every host sits on.
const TRUNK_Y: f32 = 120.0;
/// The first host's x and the spacing between hosts along the trunk.
const FIRST_HOST_X: f32 = -600.0;
const HOST_SPACING: f32 = 132.0;
/// Where a host's first subsystem hangs, and the spacing down the column.
const SUBSYSTEM_DROP: f32 = 72.0;
const SUBSYSTEM_SPACING: f32 = 36.0;
const SUBSYSTEM_RADIUS: f32 = 9.0;

const TRUNK_COLOR: &str = "4f8fb3";
const FREE_RING: &str = "4e7f9e";
const LEGACY_RING: &str = "ff4d6a";
const HIDEOUT_RING: &str = "ffd27a";
const CACHE_COLOR: &str = "9c8a62";

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Hovered>()
            .add_systems(Startup, (spawn_scene, spawn_hud))
            .add_systems(
                Update,
                (
                    update_hover,
                    animate_highlight,
                    update_info.run_if(resource_changed::<Hovered>),
                    parallax,
                ),
            );
    }
}

/// A node that shows information when the cursor is over it.
#[derive(Component)]
struct MapNode {
    name: String,
    status: String,
    /// How far from the centre the cursor still counts as over the node.
    reach: f32,
}

#[derive(Resource, Default, PartialEq)]
struct Hovered(Option<Entity>);

#[derive(Component)]
struct InfoName;

#[derive(Component)]
struct InfoStatus;

/// Who holds a host when the map is drawn, before any game is joined.
enum Holder {
    Free,
    Legacy,
    Hideout,
}

/// The network the hideouts are in: the one the map shows.
fn home_network(data: &GameData) -> &NetworkDef {
    &data.networks[usize::from(data.host(data.hideout.host).network.0)]
}

fn holder(data: &GameData, id: HostId, def: &HostDef) -> Holder {
    if id == data.hideout.host {
        Holder::Hideout
    } else if def.legacy {
        Holder::Legacy
    } else {
        Holder::Free
    }
}

fn ring(holder: &Holder) -> Vec4 {
    let mut color = linear(match holder {
        Holder::Free => FREE_RING,
        Holder::Legacy => LEGACY_RING,
        Holder::Hideout => HIDEOUT_RING,
    });
    color.w = match holder {
        Holder::Free => 0.45,
        Holder::Legacy => 1.0,
        Holder::Hideout => 0.9,
    };
    color
}

fn fill(holder: &Holder) -> Vec4 {
    linear(match holder {
        Holder::Free => "16293c",
        Holder::Legacy => "33121f",
        Holder::Hideout => "2e2712",
    })
}

fn status(data: &GameData, id: HostId, def: &HostDef, subsystems: usize) -> String {
    let resources = format!("{} resources", def.resources.len());
    if def.cache_field {
        return "Cache field   /   nothing can be built here".to_string();
    }
    let mut parts = Vec::new();
    match (holder(data, id, def), def.parent) {
        (Holder::Hideout, _) => parts.push("Every crew's hideout sits here".to_string()),
        (Holder::Legacy, _) => parts.push("Held by the Legacy Net".to_string()),
        (Holder::Free, None) => parts.push("Free host".to_string()),
        (Holder::Free, Some(_)) => parts.push("Free subsystem".to_string()),
    }
    if let Some(parent) = def.parent {
        parts.push(format!("Service on {}", data.host(parent).name));
    }
    parts.push(resources);
    if subsystems > 0 {
        parts.push(format!("{subsystems} subsystems"));
    }
    parts.join("   /   ")
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

/// Spawns a data line between two points.
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
) {
    let data = GameData::classic();
    let home = home_network(&data);
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
    let beam = Vec2::new(140.0, VIEW_SIZE.y * 1.2);
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
            name: home.name.to_uppercase(),
            status: "Backbone   /   the trunk every host in the network hangs off".into(),
            reach: beam.x * 0.35,
        },
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
        Vec2::new(BACKBONE_X + 40.0, TRUNK_Y),
        Vec2::new(last_x + 60.0, TRUNK_Y),
        9.0,
        0.5,
    );

    let label = TextColor(Color::srgba(0.72, 0.84, 0.95, 0.75));
    let small_label = TextColor(Color::srgba(0.6, 0.72, 0.85, 0.6));

    for (host, def) in &top {
        let subsystems = subsystems_of(*host);
        let x = FIRST_HOST_X + HOST_SPACING * def.order as f32;
        let centre = Vec2::new(x, TRUNK_Y);
        let seed = def.order as f32 * 1.37 + 0.5;
        let held_by = holder(&data, *host, def);

        if def.cache_field {
            let radius = 38.0;
            let mut color = linear(CACHE_COLOR);
            color.w = 0.6;
            commands.spawn((
                Mesh2d(meshes.add(Rectangle::from_length(2.0 * 1.2 * radius))),
                MeshMaterial2d(nodes.add(NodeMaterial {
                    params: node_params(NodeKind::CacheField, linear("5b5240"), color, seed, 1.2),
                })),
                Transform::from_translation(centre.extend(0.0)),
                MapNode {
                    name: def.name.to_uppercase(),
                    status: status(&data, *host, def, 0),
                    reach: radius,
                },
            ));
        } else {
            let radius = if *host == data.hideout.host {
                34.0
            } else {
                24.0 + 1.5 * subsystems.len() as f32
            };
            let extent = 1.7;
            commands.spawn((
                Mesh2d(meshes.add(Rectangle::from_length(2.0 * extent * radius))),
                MeshMaterial2d(nodes.add(NodeMaterial {
                    params: node_params(
                        NodeKind::Host,
                        fill(&held_by),
                        ring(&held_by),
                        seed,
                        extent,
                    ),
                })),
                Transform::from_translation(centre.extend(0.0)),
                MapNode {
                    name: def.name.to_uppercase(),
                    status: status(&data, *host, def, subsystems.len()),
                    reach: radius * 1.15,
                },
            ));
        }
        commands.spawn((
            Text2d::new(def.name.to_uppercase()),
            TextFont::from_font_size(13.0),
            label,
            Transform::from_xyz(x, TRUNK_Y + 60.0, 1.0),
        ));

        if subsystems.is_empty() {
            continue;
        }
        let bottom = TRUNK_Y - SUBSYSTEM_DROP - SUBSYSTEM_SPACING * (subsystems.len() - 1) as f32;
        spawn_link(
            &mut commands,
            &mut meshes,
            &mut links,
            Vec2::new(x, TRUNK_Y - 10.0),
            Vec2::new(x, bottom),
            5.0,
            seed + 3.0,
        );
        for (index, (sub, sub_def)) in subsystems.iter().enumerate() {
            let y = TRUNK_Y - SUBSYSTEM_DROP - SUBSYSTEM_SPACING * index as f32;
            let held_by = holder(&data, *sub, sub_def);
            let extent = 2.2;
            commands.spawn((
                Mesh2d(meshes.add(Rectangle::from_length(2.0 * extent * SUBSYSTEM_RADIUS))),
                MeshMaterial2d(nodes.add(NodeMaterial {
                    params: node_params(
                        NodeKind::Subsystem,
                        fill(&held_by),
                        ring(&held_by),
                        seed + index as f32 * 0.71,
                        extent,
                    ),
                })),
                Transform::from_xyz(x, y, 0.0),
                MapNode {
                    name: sub_def.name.to_uppercase(),
                    status: status(&data, *sub, sub_def, 0),
                    reach: SUBSYSTEM_RADIUS * 1.8,
                },
            ));
            commands.spawn((
                Text2d::new(sub_def.name.clone()),
                TextFont::from_font_size(10.0),
                small_label,
                TextLayout::justify(Justify::Left),
                Anchor::CENTER_LEFT,
                Transform::from_xyz(x + SUBSYSTEM_RADIUS + 8.0, y, 1.0),
            ));
        }
    }
}

fn spawn_hud(mut commands: Commands) {
    let data = GameData::classic();
    let network = home_network(&data).name.to_uppercase();
    let bright = TextColor(Color::srgb(0.88, 0.95, 1.0));
    let dim = TextColor(Color::srgba(0.72, 0.84, 0.95, 0.65));
    let faint = TextColor(Color::srgba(0.72, 0.84, 0.95, 0.4));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(32.0),
            top: Val::Px(24.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            ..default()
        },
        children![
            (Text::new("NULLNET"), TextFont::from_font_size(34.0), bright),
            (
                Text::new(format!("{network}   {}", date(0))),
                TextFont::from_font_size(15.0),
                dim,
            ),
            (
                Text::new(format!("days since {}", EPOCH.to_uppercase())),
                TextFont::from_font_size(11.0),
                faint,
            ),
        ],
    ));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(36.0),
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(6.0),
            ..default()
        },
        children![
            (
                InfoName,
                Text::new(""),
                TextFont::from_font_size(24.0),
                bright
            ),
            (
                InfoStatus,
                Text::new("Hover over a host"),
                TextFont::from_font_size(15.0),
                dim,
            ),
        ],
    ));
}

fn update_hover(
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform)>,
    nodes: Query<(Entity, &MapNode, &GlobalTransform)>,
    mut hovered: ResMut<Hovered>,
) {
    let (camera, camera_transform) = *camera;
    let found = window
        .cursor_position()
        .and_then(|cursor| camera.viewport_to_world_2d(camera_transform, cursor).ok())
        .and_then(|point| {
            // The nearest node whose reach covers the cursor, so a subsystem
            // under a big host still wins when the cursor is on it.
            nodes
                .iter()
                .filter_map(|(entity, node, transform)| {
                    let distance = transform.translation().truncate().distance(point);
                    (node.reach > 0.0 && distance < node.reach)
                        .then_some((entity, distance / node.reach))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(entity, _)| entity)
        });
    hovered.set_if_neq(Hovered(found));
}

fn animate_highlight(
    time: Res<Time>,
    hovered: Res<Hovered>,
    nodes: Query<(Entity, &MeshMaterial2d<NodeMaterial>), With<MapNode>>,
    mut materials: ResMut<Assets<NodeMaterial>>,
) {
    let step = time.delta_secs() * 6.0;
    for (entity, material) in &nodes {
        let target = if hovered.0 == Some(entity) { 1.0 } else { 0.0 };
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

fn update_info(
    hovered: Res<Hovered>,
    nodes: Query<&MapNode>,
    mut name: Single<&mut Text, With<InfoName>>,
    mut status: Single<&mut Text, (With<InfoStatus>, Without<InfoName>)>,
) {
    match hovered.0.and_then(|entity| nodes.get(entity).ok()) {
        Some(node) => {
            name.0 = node.name.clone();
            status.0 = node.status.clone();
        }
        None => {
            name.0.clear();
            status.0 = "Hover over a host".to_string();
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
