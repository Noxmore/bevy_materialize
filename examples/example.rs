use std::{env, sync::LazyLock};

use bevy::prelude::*;
use bevy_materialize::prelude::*;

static DESERIALIZER: LazyLock<String> = LazyLock::new(|| env::args().nth(1).expect("Set argument for toml/json"));

fn main() {
    let mut app = App::new();

    app.add_plugins(DefaultPlugins);

    match DESERIALIZER.as_str() {
        "toml" => app.add_plugins(MaterializePlugin::new(TomlMaterialDeserializer)),
        "json" => app.add_plugins(MaterializePlugin::new(JsonMaterialDeserializer)),
        format => panic!("Invalid format: {format}"),
    };
    
    app
        .insert_resource(AmbientLight {
            brightness: 1000.,
            ..default()
        })
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        Mesh3d(asset_server.add(Cuboid::from_length(1.).into())),
        GenericMaterial3d(asset_server.load(format!("materials/example.material.{}", DESERIALIZER.as_str()))),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(Vec3::splat(3.)).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
