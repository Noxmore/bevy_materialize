use bevy::prelude::*;
use bevy_materialize::prelude::*;

pub trait MyMaterialProperties {
	const COLLISION: MaterialProperty<bool> = MaterialProperty::new("collision");
	const SOUNDS: MaterialProperty<String> = MaterialProperty::new("sounds");
}
impl MyMaterialProperties for GenericMaterial {}

fn main() {
	App::new()
		.add_plugins(DefaultPlugins.set(ImagePlugin::default_nearest()))
		.add_plugins(MaterializePlugin::new(TomlMaterialDeserializer))
		.register_material_property(GenericMaterial::COLLISION)
		.register_material_property(GenericMaterial::SOUNDS)
		.insert_resource(AmbientLight {
			brightness: 1000.,
			..default()
		})
		.add_systems(Startup, setup)
		.run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
	let cube = asset_server.add(Cuboid::from_length(1.).into());

	commands.spawn((
		Mesh3d(cube.clone()),
		GenericMaterial3d(asset_server.load("materials/example.material.toml")),
	));
	commands.spawn((
		Mesh3d(cube.clone()),
		GenericMaterial3d(asset_server.load("materials/animated.toml")),
		Transform::from_xyz(-1.5, 0., 1.5),
	));
	commands.spawn((
		Mesh3d(cube),
		GenericMaterial3d(asset_server.load("materials/sub-material.toml")),
		Transform::from_xyz(1.5, 0., -1.5),
	));

	commands.spawn((
		Camera3d::default(),
		Transform::from_translation(Vec3::splat(3.)).looking_at(Vec3::ZERO, Vec3::Y),
	));
}
