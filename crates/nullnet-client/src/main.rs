mod guide;
mod icons;
mod map;
mod materials;
mod music;
mod net;
mod notify;
mod sound;
mod text;
mod theme;
mod ui;
mod voice;

use bevy::prelude::*;
use nullnet_core::{GameData, MapSpec};

/// The map the crew's game is played on: the standard one until the first
/// status says otherwise, then whatever the game's spec builds.
#[derive(Resource)]
pub struct Rules {
    pub data: GameData,
    pub map: MapSpec,
}

impl Rules {
    pub fn new(map: MapSpec) -> Self {
        Rules {
            data: GameData::for_map(map),
            map,
        }
    }
}

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(Rules::new(MapSpec::Standard))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "NullNet".into(),
                // In the browser, render into the page's canvas and let
                // browser shortcuts through.
                canvas: Some("#nullnet".into()),
                fit_canvas_to_parent: true,
                prevent_default_event_handling: false,
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            materials::MaterialsPlugin,
            theme::ThemePlugin,
            sound::SoundPlugin,
            net::NetPlugin,
            map::MapPlugin,
            ui::UiPlugin,
        ))
        .run();
}
