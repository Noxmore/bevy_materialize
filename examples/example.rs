use bevy::prelude::*;
use bevy_materialize::prelude::*;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, MaterializePlugin::new(TomlMaterialDeserializer)))
        .insert_resource(AmbientLight { brightness: 1000., ..default() })
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        Mesh3d(asset_server.add(Cuboid::from_length(1.).into())),
        GenericMaterial3d(asset_server.load("materials/example.material")),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(Vec3::splat(3.)).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
