//! The Sol overview: the sun, the nine planets on their orbits and Earth's
//! moon, with information about the planet under the cursor.

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::sprite_render::AlphaMode2d;
use bevy::window::PrimaryWindow;
use nullnet_core::calendar_date;

use crate::materials::{
    BackgroundMaterial, BackgroundParams, PlanetMaterial, PlanetParams, Surface, linear,
};

/// The area the camera always keeps on screen, in world units.
const VIEW_SIZE: Vec2 = Vec2::new(1600.0, 900.0);
const SUN_CENTRE: Vec2 = Vec2::new(-1080.0, 0.0);
const SUN_RADIUS: f32 = 430.0;
const EARTH_X: f32 = -360.0;

pub struct SolPlugin;

impl Plugin for SolPlugin {
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
                    orbit,
                ),
            );
    }
}

/// Visual description of one body. Sizes and spacing are stylised, not to
/// scale; colours are sRGB hex.
struct Look {
    name: &'static str,
    status: &'static str,
    x: f32,
    radius: f32,
    surface: Surface,
    colors: [&'static str; 3],
    atmosphere: (&'static str, f32),
    noise_scale: f32,
    spin: f32,
    /// Inner radius, outer radius and tilt, in planet radii.
    ring: Option<(f32, f32, f32)>,
}

const UNCLAIMED: &str = "Unclaimed";
// Sol's Methanoid colonies are fixed in the original game data.
const METHANOID: &str = "Methanoid colony";

const PLANETS: [Look; 9] = [
    Look {
        name: "Mercury",
        status: UNCLAIMED,
        x: -540.0,
        radius: 22.0,
        surface: Surface::Rocky,
        colors: ["6e6259", "a39a8f", "000000"],
        atmosphere: ("000000", 0.0),
        noise_scale: 3.0,
        spin: 0.03,
        ring: None,
    },
    Look {
        name: "Venus",
        status: UNCLAIMED,
        x: -455.0,
        radius: 34.0,
        surface: Surface::Gas,
        colors: ["d9b77a", "f2dcae", "c79a5a"],
        atmosphere: ("f5d9a0", 0.6),
        noise_scale: 2.0,
        spin: 0.02,
        ring: None,
    },
    Look {
        name: "Earth",
        status: "Home world: ground base, research and training",
        x: EARTH_X,
        radius: 36.0,
        surface: Surface::Earth,
        colors: ["1d4e89", "3f7d3a", "ffffff"],
        atmosphere: ("6fb6ff", 0.9),
        noise_scale: 2.2,
        spin: 0.05,
        ring: None,
    },
    Look {
        name: "Mars",
        status: UNCLAIMED,
        x: -270.0,
        radius: 28.0,
        surface: Surface::Rocky,
        colors: ["8a3b22", "c8693c", "000000"],
        atmosphere: ("e0a080", 0.25),
        noise_scale: 2.5,
        spin: 0.045,
        ring: None,
    },
    Look {
        name: "Jupiter",
        status: METHANOID,
        x: -120.0,
        radius: 78.0,
        surface: Surface::Gas,
        colors: ["c9a27e", "efe3cf", "a0603e"],
        atmosphere: ("e8d0b0", 0.22),
        noise_scale: 1.8,
        spin: 0.08,
        ring: None,
    },
    Look {
        name: "Saturn",
        status: UNCLAIMED,
        x: 110.0,
        radius: 64.0,
        surface: Surface::Gas,
        colors: ["d8c08e", "f1e4c0", "b89a62"],
        atmosphere: ("f0e0b0", 0.2),
        noise_scale: 1.6,
        spin: 0.07,
        ring: Some((1.35, 2.25, 0.32)),
    },
    Look {
        name: "Uranus",
        status: METHANOID,
        x: 320.0,
        radius: 46.0,
        surface: Surface::Gas,
        colors: ["8fd3dc", "b5eef0", "7cc0cc"],
        atmosphere: ("b0f0ff", 0.5),
        noise_scale: 1.2,
        spin: 0.05,
        ring: None,
    },
    Look {
        name: "Neptune",
        status: METHANOID,
        x: 460.0,
        radius: 44.0,
        surface: Surface::Gas,
        colors: ["2b4fb8", "4a7de0", "1d2f80"],
        atmosphere: ("6a9cff", 0.55),
        noise_scale: 1.5,
        spin: 0.06,
        ring: None,
    },
    Look {
        name: "Pluto",
        status: METHANOID,
        x: 580.0,
        radius: 16.0,
        surface: Surface::Ice,
        colors: ["b8a48e", "e6dccd", "000000"],
        atmosphere: ("000000", 0.0),
        noise_scale: 3.0,
        spin: 0.03,
        ring: None,
    },
];

/// A body that shows information when the cursor is over it.
#[derive(Component)]
struct Body {
    name: &'static str,
    status: &'static str,
    radius: f32,
}

/// Moves an entity along an ellipse around a body. The upper half of the
/// ellipse is the far side, where the orbiter passes behind the body.
///
/// Bevy 0.19 fixes the draw order of a transparent 2D mesh when it is
/// queued, so changing an entity's z later does not move it in front of or
/// behind its neighbours. Each orbiter is therefore spawned twice at fixed
/// depths, and only the copy for the current side is visible.
#[derive(Component)]
struct Orbit {
    centre: Vec2,
    radii: Vec2,
    speed: f32,
    far_side: bool,
}

#[derive(Resource, Default, PartialEq)]
struct Hovered(Option<Entity>);

#[derive(Component)]
struct InfoName;

#[derive(Component)]
struct InfoStatus;

fn planet_params(look: &Look, seed: f32) -> PlanetParams {
    let surface = match look.surface {
        Surface::Rocky => 0.0,
        Surface::Gas => 1.0,
        Surface::Earth => 2.0,
        Surface::Ice => 3.0,
        Surface::Star => 4.0,
    };
    let (ring_inner, ring_outer, ring_tilt) = look.ring.unwrap_or((0.0, 0.0, 1.0));
    let mut atmosphere = linear(look.atmosphere.0);
    atmosphere.w = look.atmosphere.1;
    PlanetParams {
        color_a: linear(look.colors[0]),
        color_b: linear(look.colors[1]),
        color_c: linear(look.colors[2]),
        atmosphere,
        shape: Vec4::new(surface, look.noise_scale, look.spin, seed),
        extra: Vec4::new(ring_inner, ring_outer, ring_tilt, 0.0),
        quad: Vec4::new(quad_extent(look), 0.0, 0.0, 0.0),
    }
}

/// Half-size of a body's quad in planet radii: room for the atmosphere,
/// hover halo and rings.
fn quad_extent(look: &Look) -> f32 {
    match look.ring {
        Some((_, outer, _)) => outer + 0.1,
        None => 1.35,
    }
}

fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut planets: ResMut<Assets<PlanetMaterial>>,
    mut backgrounds: ResMut<Assets<BackgroundMaterial>>,
    mut colors: ResMut<Assets<ColorMaterial>>,
) {
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
                nebula_a: linear("1a1046"),
                nebula_b: linear("0d3a5c"),
            },
        })),
        Transform::from_xyz(0.0, 0.0, -10.0),
    ));

    let sun = Look {
        name: "Sol",
        status: "Star",
        x: SUN_CENTRE.x,
        radius: SUN_RADIUS,
        surface: Surface::Star,
        colors: ["fff6d8", "ffb347", "ff9a3c"],
        atmosphere: ("000000", 0.0),
        noise_scale: 1.0,
        spin: 0.0,
        ring: None,
    };
    let sun_extent = 1.9;
    let mut sun_params = planet_params(&sun, 0.0);
    sun_params.quad.x = sun_extent;
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::from_length(2.0 * sun_extent * SUN_RADIUS))),
        MeshMaterial2d(planets.add(PlanetMaterial { params: sun_params })),
        Transform::from_translation(SUN_CENTRE.extend(-4.0)),
        Body {
            name: sun.name,
            status: sun.status,
            radius: SUN_RADIUS,
        },
    ));

    let orbit_material = colors.add(ColorMaterial {
        color: Color::srgba(0.55, 0.68, 1.0, 0.13),
        alpha_mode: AlphaMode2d::Blend,
        ..default()
    });
    let label_color = TextColor(Color::srgba(0.75, 0.82, 1.0, 0.6));

    for (index, look) in PLANETS.iter().enumerate() {
        let orbit_radius = look.x - SUN_CENTRE.x;
        commands.spawn((
            Mesh2d(
                meshes.add(
                    Annulus::new(orbit_radius - 0.75, orbit_radius + 0.75)
                        .mesh()
                        .resolution(512),
                ),
            ),
            MeshMaterial2d(orbit_material.clone()),
            Transform::from_translation(SUN_CENTRE.extend(-5.0)),
        ));

        let size = 2.0 * quad_extent(look) * look.radius;
        commands.spawn((
            Mesh2d(meshes.add(Rectangle::from_length(size))),
            MeshMaterial2d(planets.add(PlanetMaterial {
                params: planet_params(look, index as f32 * 1.37 + 0.5),
            })),
            Transform::from_xyz(look.x, 0.0, 0.0),
            Body {
                name: look.name,
                status: look.status,
                radius: look.radius,
            },
        ));

        commands.spawn((
            Text2d::new(look.name.to_uppercase()),
            TextFont::from_font_size(13.0),
            label_color,
            Transform::from_xyz(look.x, -(look.radius + 28.0), 1.0),
        ));
    }

    let moon = Look {
        name: "Moon",
        status: "",
        x: 0.0,
        radius: 8.0,
        surface: Surface::Rocky,
        colors: ["77736e", "b9b4ad", "000000"],
        atmosphere: ("000000", 0.0),
        noise_scale: 3.5,
        spin: 0.02,
        ring: None,
    };
    let moon_mesh = meshes.add(Rectangle::from_length(
        2.0 * quad_extent(&moon) * moon.radius,
    ));
    let moon_material = planets.add(PlanetMaterial {
        params: planet_params(&moon, 9.1),
    });
    for (far_side, z) in [(true, -0.5), (false, 0.5)] {
        commands.spawn((
            Mesh2d(moon_mesh.clone()),
            MeshMaterial2d(moon_material.clone()),
            Transform::from_xyz(0.0, 0.0, z),
            Orbit {
                centre: Vec2::new(EARTH_X, 0.0),
                radii: Vec2::new(54.0, 16.0),
                speed: 0.35,
                far_side,
            },
        ));
    }
}

fn spawn_hud(mut commands: Commands) {
    let (year, day) = calendar_date(0);
    let bright = TextColor(Color::srgb(0.88, 0.93, 1.0));
    let dim = TextColor(Color::srgba(0.75, 0.82, 1.0, 0.65));

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
                Text::new(format!("YEAR {year}   DAY {day:03}")),
                TextFont::from_font_size(15.0),
                dim,
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
                Text::new("Hover over a planet"),
                TextFont::from_font_size(15.0),
                dim,
            ),
        ],
    ));
}

fn update_hover(
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform)>,
    bodies: Query<(Entity, &Body, &GlobalTransform)>,
    mut hovered: ResMut<Hovered>,
) {
    let (camera, camera_transform) = *camera;
    let found = window
        .cursor_position()
        .and_then(|cursor| camera.viewport_to_world_2d(camera_transform, cursor).ok())
        .and_then(|point| {
            bodies
                .iter()
                .find(|(_, body, transform)| {
                    transform.translation().truncate().distance(point) < body.radius * 1.15
                })
                .map(|(entity, ..)| entity)
        });
    hovered.set_if_neq(Hovered(found));
}

fn animate_highlight(
    time: Res<Time>,
    hovered: Res<Hovered>,
    bodies: Query<(Entity, &MeshMaterial2d<PlanetMaterial>), With<Body>>,
    mut materials: ResMut<Assets<PlanetMaterial>>,
) {
    let step = time.delta_secs() * 6.0;
    for (entity, material) in &bodies {
        let target = if hovered.0 == Some(entity) { 1.0 } else { 0.0 };
        let Some(current) = materials.get(material.id()).map(PlanetMaterial::highlight) else {
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
    bodies: Query<&Body>,
    mut name: Single<&mut Text, With<InfoName>>,
    mut status: Single<&mut Text, (With<InfoStatus>, Without<InfoName>)>,
) {
    match hovered.0.and_then(|entity| bodies.get(entity).ok()) {
        Some(body) => {
            name.0 = body.name.to_uppercase();
            status.0 = body.status.to_string();
        }
        None => {
            name.0.clear();
            status.0 = "Hover over a planet".to_string();
        }
    }
}

/// Shifts the star layers slightly against the cursor for a sense of depth.
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

fn orbit(time: Res<Time>, mut orbiters: Query<(&Orbit, &mut Transform, &mut Visibility)>) {
    for (orbit, mut transform, mut visibility) in &mut orbiters {
        let angle = time.elapsed_secs() * orbit.speed;
        let offset = Vec2::new(angle.cos(), angle.sin()) * orbit.radii;
        let position = orbit.centre + offset;
        transform.translation.x = position.x;
        transform.translation.y = position.y;
        let on_far_side = angle.sin() > 0.0;
        visibility.set_if_neq(if on_far_side == orbit.far_side {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}
