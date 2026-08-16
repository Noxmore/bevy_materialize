use std::f32;

use bevy::prelude::*;
use bevy_materialize::prelude::*;

fn main() {
	App::new()
		.add_plugins(DefaultPlugins)
		.add_plugins(MaterializePlugin::new(TomlMaterialDeserializer))
		.insert_resource(GlobalAmbientLight {
			brightness: light_consts::lux::FULL_MOON_NIGHT,
			..default()
		})
		.add_systems(Startup, setup)
		.add_systems(Update, slowly_spin)
		.run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
	let mesh: Mesh = Mesh::from(Cuboid::from_length(1.)).with_generated_tangents().unwrap();

	commands.spawn((
		Mesh3d(asset_server.add(mesh)),
		GenericMaterial3d(asset_server.load("materials/polyhaven_dark_rock/dark_rock.toml")),
		// gives the cube a slight tilt for better viewing of material properties
		Transform::from_rotation(Quat::from_rotation_x(15.0_f32.to_radians())),
	));

	commands.spawn((
		DirectionalLight {
			illuminance: light_consts::lux::DIRECT_SUNLIGHT,
			shadow_maps_enabled: true,
			..default()
		},
		Transform::from_translation(Vec3::new(-1.0, 5.0, 1.0)).looking_at(Vec3::ZERO, Vec3::Y),
	));

	commands.spawn((
		Camera3d::default(),
		bevy::camera::Exposure::OVERCAST,
		bevy::post_process::bloom::Bloom::NATURAL,
		Transform::from_translation(Vec3::splat(1.5)).looking_at(Vec3::ZERO, Vec3::Y),
	));
}

/// spin any mesh3d in the world
fn slowly_spin(mut q: Query<&mut Transform, With<Mesh3d>>, time: Res<Time>) {
	const FULL_CYCLE: f32 = 2.0 * f32::consts::PI;
	const CYCLES_PER_SECOND: f32 = FULL_CYCLE * 0.1;

	for mut t in q.iter_mut() {
		t.rotate_y(time.delta_secs() * CYCLES_PER_SECOND);
	}
}
