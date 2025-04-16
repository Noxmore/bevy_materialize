use bevy::prelude::*;
use bevy_materialize::prelude::*;

// These are stored as constants for ease of refactoring.
pub const COLLISION_PROPERTY_KEY: &str = "collision";
pub const SOUNDS_PROPERTY_KEY: &str = "sounds";

fn main() {
	App::new()
		.add_plugins(DefaultPlugins.set(ImagePlugin::default_nearest()))
		.add_plugins(MaterializePlugin::new(JsonMaterialDeserializer))
		.register_material_property::<bool>(COLLISION_PROPERTY_KEY)
		.register_material_property::<String>(SOUNDS_PROPERTY_KEY)
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
		GenericMaterial3d(asset_server.load("materials/example.material.json")),
	));

	commands.spawn((
		Camera3d::default(),
		Transform::from_translation(Vec3::splat(3.)).looking_at(Vec3::ZERO, Vec3::Y),
	));
}
