mod map;
mod materials;

use bevy::prelude::*;

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::BLACK))
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
        .add_plugins((materials::MaterialsPlugin, map::MapPlugin))
        .run();
}
