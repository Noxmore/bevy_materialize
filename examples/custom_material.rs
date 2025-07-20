use bevy::{
	image::{ImageAddressMode, ImageSamplerDescriptor},
	math::vec3,
	pbr::{ExtendedMaterial, MaterialExtension},
	prelude::*,
	render::render_resource::AsBindGroup,
};
use bevy_materialize::prelude::*;

fn main() {
	App::new()
		.add_plugins((
			DefaultPlugins.set(ImagePlugin {
				default_sampler: ImageSamplerDescriptor {
					// For the sky material
					address_mode_u: ImageAddressMode::Repeat,
					address_mode_v: ImageAddressMode::Repeat,
					address_mode_w: ImageAddressMode::Repeat,
					..ImageSamplerDescriptor::nearest()
				},
			}),
			MaterializePlugin::new(TomlMaterialDeserializer),
			MaterialPlugin::<QuakeSkyMaterial>::default(),
			MaterialPlugin::<QuakeLiquidMaterial>::default(),
		))
		.register_extended_generic_material::<StandardMaterial, QuakeLiquidMaterialExt>("QuakeLiquidMaterial")
		.register_generic_material::<QuakeSkyMaterial>()
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
		GenericMaterial3d(asset_server.load("materials/custom_material.toml")),
	));
	commands.spawn((
		Mesh3d(cube),
		GenericMaterial3d(asset_server.load("materials/extended_material.toml")),
		Transform::from_xyz(-1.5, 0., 1.5),
	));

	commands.spawn((
		Camera3d::default(),
		Transform::from_translation(Vec3::splat(3.)).looking_at(Vec3::ZERO, Vec3::Y),
	));
}

/// Material extension to [`StandardMaterial`] that emulates the wave effect of Quake liquid.
pub type QuakeLiquidMaterial = ExtendedMaterial<StandardMaterial, QuakeLiquidMaterialExt>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct QuakeLiquidMaterialExt {
	#[uniform(100)]
	pub magnitude: f32,
	#[uniform(100)]
	pub cycles: f32,
}
// We don't need to use `FromWorld` here, this is to make sure `register_extended_generic_material` works correctly.
impl FromWorld for QuakeLiquidMaterialExt {
	fn from_world(_world: &mut World) -> Self {
		Self {
			magnitude: 0.1,
			cycles: std::f32::consts::PI,
		}
	}
}
impl MaterialExtension for QuakeLiquidMaterialExt {
	fn fragment_shader() -> bevy::render::render_resource::ShaderRef {
		"shaders/quake_liquid.wgsl".into()
	}
}

/// Material that emulates the Quake sky.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct QuakeSkyMaterial {
	/// The speed the foreground layer moves.
	#[uniform(0)]
	pub fg_scroll: Vec2,
	/// The speed the background layer moves.
	#[uniform(0)]
	pub bg_scroll: Vec2,
	/// The scale of the textures.
	#[uniform(0)]
	pub texture_scale: f32,
	/// Scales the sphere before it is re-normalized, used to shape it.
	#[uniform(0)]
	pub sphere_scale: Vec3,

	#[texture(1)]
	#[sampler(2)]
	pub fg: Handle<Image>,

	#[texture(3)]
	#[sampler(4)]
	pub bg: Handle<Image>,
}
impl Default for QuakeSkyMaterial {
	fn default() -> Self {
		Self {
			fg_scroll: Vec2::splat(0.1),
			bg_scroll: Vec2::splat(0.05),
			texture_scale: 2.,
			sphere_scale: vec3(1., 3., 1.),
			fg: default(),
			bg: default(),
		}
	}
}
impl Material for QuakeSkyMaterial {
	fn fragment_shader() -> bevy::render::render_resource::ShaderRef {
		"shaders/quake_sky.wgsl".into()
	}

	fn alpha_mode(&self) -> AlphaMode {
		AlphaMode::Opaque
	}
}

#[test]
fn load_custom_materials() {
	let mut app = bevy_materialize::load::create_loading_test_app(TomlMaterialDeserializer);

	#[rustfmt::skip]
	app
		.init_asset::<QuakeSkyMaterial>()
		.init_asset::<QuakeLiquidMaterial>()
		.register_extended_generic_material::<StandardMaterial, QuakeLiquidMaterialExt>("QuakeLiquidMaterial")
		.register_generic_material::<QuakeSkyMaterial>()
	;

	let asset_server = app.world().resource::<AssetServer>();

	smol::block_on(async {
		asset_server.load_untyped_async("materials/custom_material.toml").await.unwrap();
		asset_server.load_untyped_async("materials/extended_material.toml").await.unwrap();
	});
}
