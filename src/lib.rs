#![doc = include_str!("../readme.md")]

pub mod animation;
pub mod generic_material;
pub mod load;
pub mod prelude;
pub mod value;

#[cfg(feature = "bevy_pbr")]
use std::any::TypeId;
use std::sync::Arc;

#[cfg(feature = "bevy_pbr")]
use bevy::reflect::GetTypeRegistration;
use generic_material::GenericMaterialShorthands;

use bevy::prelude::*;
#[cfg(feature = "bevy_pbr")]
use generic_material::GenericMaterialApplied;
use load::{
	deserializer::MaterialDeserializer,
	simple::{SimpleGenericMaterialLoader, SimpleGenericMaterialLoaderSettings},
	GenericMaterialLoader, ReflectGenericMaterialLoadAppExt,
};
use prelude::*;

pub struct MaterializePlugin<D: MaterialDeserializer> {
	pub deserializer: Arc<D>,
	/// If [`None`], doesn't register [`SimpleGenericMaterialLoader`].
	pub simple_loader_settings: Option<SimpleGenericMaterialLoaderSettings>,
}
impl<D: MaterialDeserializer> Plugin for MaterializePlugin<D> {
	fn build(&self, app: &mut App) {
		let type_registry = app.world().resource::<AppTypeRegistry>().clone();

		if let Some(settings) = &self.simple_loader_settings {
			app.register_asset_loader(SimpleGenericMaterialLoader { settings: settings.clone() });
		}

		let shorthands = GenericMaterialShorthands::default();

		#[rustfmt::skip]
		app
			.add_plugins((MaterializeMarkerPlugin, animation::AnimationPlugin))
			.insert_resource(shorthands.clone())
			.register_type::<GenericMaterial3d>()
			.init_asset::<GenericMaterial>()
			.register_generic_material_sub_asset_image_settings_passthrough::<GenericMaterial>()
			.register_asset_loader(GenericMaterialLoader {
				type_registry,
				shorthands,
				deserializer: self.deserializer.clone(),
			})
		;

		#[cfg(feature = "bevy_image")]
		app.register_generic_material_sub_asset_image_settings_passthrough::<Image>();

		#[cfg(feature = "bevy_pbr")]
		#[rustfmt::skip]
		app
			.register_generic_material::<StandardMaterial>()
			.add_systems(PreUpdate, reload_generic_materials)
			.add_systems(PostUpdate, (
				insert_generic_materials,
				visibility_material_property.before(insert_generic_materials),
			))
		;
	}
}
impl<D: MaterialDeserializer> MaterializePlugin<D> {
	pub fn new(deserializer: D) -> Self {
		Self {
			deserializer: Arc::new(deserializer),
			simple_loader_settings: Some(default()),
		}
	}

	/// If [`None`], doesn't register [`SimpleGenericMaterialLoader`].
	pub fn with_simple_loader_settings(mut self, settings: Option<SimpleGenericMaterialLoaderSettings>) -> Self {
		self.simple_loader_settings = settings;
		self
	}
}
impl<D: MaterialDeserializer + Default> Default for MaterializePlugin<D> {
	fn default() -> Self {
		Self {
			deserializer: Arc::new(D::default()),
			simple_loader_settings: Some(default()),
		}
	}
}

/// Added when a [`MaterializePlugin`] is added. Can be used to check if any [`MaterializePlugin`] has been added.
pub struct MaterializeMarkerPlugin;
impl Plugin for MaterializeMarkerPlugin {
	fn build(&self, _app: &mut App) {}
}

// Can't have these in a MaterializePlugin impl because of the generic.
// ////////////////////////////////////////////////////////////////////////////////
// // SYSTEMS
// ////////////////////////////////////////////////////////////////////////////////

#[cfg(feature = "bevy_pbr")]
pub fn insert_generic_materials(
	mut commands: Commands,
	query: Query<(Entity, &GenericMaterial3d), Without<GenericMaterialApplied>>,
	generic_materials: Res<Assets<GenericMaterial>>,
) {
	for (entity, holder) in &query {
		let Some(generic_material) = generic_materials.get(&holder.0) else { continue };

		let material = generic_material.handle.clone();
		commands
			.entity(entity)
			.queue(move |entity: EntityWorldMut<'_>| material.insert(entity))
			.insert(GenericMaterialApplied);
	}
}

#[cfg(feature = "bevy_pbr")]
pub fn reload_generic_materials(
	mut commands: Commands,
	mut asset_events: EventReader<AssetEvent<GenericMaterial>>,
	query: Query<(Entity, &GenericMaterial3d), With<GenericMaterialApplied>>,
) {
	for event in asset_events.read() {
		let AssetEvent::Modified { id } = event else { continue };

		for (entity, holder) in &query {
			if *id == holder.0.id() {
				commands.entity(entity).remove::<GenericMaterialApplied>();
			}
		}
	}
}

#[cfg(feature = "bevy_pbr")]
pub fn visibility_material_property(
	mut query: Query<(&GenericMaterial3d, &mut Visibility), Without<GenericMaterialApplied>>,
	generic_materials: GenericMaterials,
) {
	for (generic_material_holder, mut visibility) in &mut query {
		let Some(generic_material) = generic_materials.get(&generic_material_holder.0) else { continue };
		let Ok(new_visibility) = generic_material.get_property(GenericMaterial::VISIBILITY) else { continue };

		*visibility = new_visibility;
	}
}

#[cfg(feature = "bevy_pbr")]
pub trait MaterializeAppExt {
	/// Register a material to be able to be created via [`GenericMaterial`].
	///
	/// This also registers the type if it isn't already registered.
	///
	/// It's also worth noting that [`from_world`](FromWorld::from_world) is only called once when the material is registered, then that value is cloned each time a new instance is required.
	///
	/// If you own the type, you can also use `#[reflect(GenericMaterial)]` to automatically register it when you use `App::register_type::<...>()`.
	/// I personally recommend just using this function though - saves a line of code.
	fn register_generic_material<M: Material + Reflect + Struct + FromWorld + GetTypeRegistration>(&mut self) -> &mut Self;

	/// If your material name is really long, you can use this to register a shorthand that can be used in place of it.
	///
	/// This is namely useful for extended materials, as those type names tend to have a lot of boilerplate.
	///
	/// # Examples
	/// ```ignore
	/// # App::new()
	/// .register_generic_material_shorthand::<YourOldReallyLongNameOhMyGoshItsSoLong>("ShortName")
	/// ```
	/// Now you can turn
	/// ```toml
	/// type = "YourOldReallyLongNameOhMyGoshItsSoLong"
	/// ```
	/// into
	/// ```toml
	/// type = "ShortName"
	/// ```
	fn register_generic_material_shorthand<M: GetTypeRegistration>(&mut self, shorthand: impl Into<String>) -> &mut Self;
}
#[cfg(feature = "bevy_pbr")]
impl MaterializeAppExt for App {
	fn register_generic_material<M: Material + Reflect + Struct + FromWorld + GetTypeRegistration>(&mut self) -> &mut Self {
		let default_value = Box::new(M::from_world(self.world_mut()));

		let mut type_registry = self.world().resource::<AppTypeRegistry>().write();
		if type_registry.get(TypeId::of::<M>()).is_none() {
			type_registry.register::<M>();
		}

		type_registry
			.get_mut(TypeId::of::<M>())
			.unwrap()
			.insert(ReflectGenericMaterial { default_value });

		drop(type_registry);

		self
	}

	fn register_generic_material_shorthand<M: GetTypeRegistration>(&mut self, shorthand: impl Into<String>) -> &mut Self {
		self.world()
			.resource::<GenericMaterialShorthands>()
			.values
			.write()
			.unwrap()
			.insert(shorthand.into(), M::get_type_registration());
		self
	}
}
