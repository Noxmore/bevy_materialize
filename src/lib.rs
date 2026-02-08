#![doc = include_str!("../readme.md")]

pub mod animation;
pub mod color_space_fix;
#[cfg(feature = "bevy_pbr")]
pub mod erased_material;
pub mod generic_material;
pub mod load;
pub mod material_property;
pub mod prelude;
pub mod value;

#[cfg(feature = "bevy_pbr")]
use std::any::TypeId;
use std::sync::Arc;

#[cfg(feature = "bevy_pbr")]
use bevy::{
	pbr::{ExtendedMaterial, MaterialExtension},
	reflect::{GetTypeRegistration, Typed},
};
use color_space_fix::ColorSpaceFixPlugin;
use generic_material::GenericMaterialShorthands;
use material_property::MaterialPropertyRegistry;

use bevy::prelude::*;
#[cfg(feature = "bevy_pbr")]
use generic_material::GenericMaterialApplied;
use load::{
	GenericMaterialLoader, asset::AssetLoadingProcessor, deserializer::MaterialDeserializer, processor::MaterialProcessor,
	simple::SimpleGenericMaterialLoader,
};
use prelude::*;

pub struct MaterializePlugin<D: MaterialDeserializer, P: MaterialProcessor> {
	pub deserializer: Arc<D>,
	/// Registers a [`GenericMaterial`] loader that loads directly from images. WARNING: This will cause conflicts if you try to load an image file without specifying what type you want to load.
	pub simple_loader: Option<SimpleGenericMaterialLoader>,
	/// Whether to add [`AnimationPlugin`](animation::AnimationPlugin), animating materials with the [`ANIMATION`](GenericMaterial::ANIMATION) property. (Default: `true`)
	pub animated_materials: bool,
	// Whether to replace special patterns in text, such as replacing `${name}` with the name of the material loading. (Default: `true`)
	pub do_text_replacements: bool,
	/// Whether to automatically set maps in [`StandardMaterial`] that aren't supposed to be to sRGB to linear if necessary.
	pub standard_material_color_space_fix: bool,
	pub processor: P,
}
impl<D: MaterialDeserializer, P: MaterialProcessor + Clone> Plugin for MaterializePlugin<D, P> {
	fn build(&self, app: &mut App) {
		let type_registry = app.world().resource::<AppTypeRegistry>().clone();

		if let Some(simple_loader) = self.simple_loader.clone() {
			app.register_asset_loader(simple_loader);
		}

		let shorthands = GenericMaterialShorthands::default();
		let property_registry = MaterialPropertyRegistry::default();

		#[rustfmt::skip]
		app
			.add_plugins(MaterializeMarkerPlugin)
			.insert_resource(shorthands.clone())
			.insert_resource(property_registry.clone())
			.register_type::<GenericMaterial3d>()
			.init_asset::<GenericMaterial>()
			.register_generic_material_sub_asset::<GenericMaterial>()
			.register_asset_loader(GenericMaterialLoader {
				type_registry,
				shorthands,
				property_registry,
				deserializer: self.deserializer.clone(),
				do_text_replacements: self.do_text_replacements,
				processor: self.processor.clone(),
			})
		;

		if self.animated_materials {
			app.add_plugins(animation::AnimationPlugin);
		}

		if self.standard_material_color_space_fix {
			app.add_plugins(ColorSpaceFixPlugin);
		}

		#[cfg(feature = "bevy_image")]
		app.register_generic_material_sub_asset::<Image>();

		#[cfg(feature = "bevy_pbr")]
		#[rustfmt::skip]
		app
			.register_material_property(GenericMaterial::VISIBILITY)
			.register_generic_material::<StandardMaterial>()
			.add_systems(PreUpdate, (
				reload_generic_materials,
				visibility_material_property, // Must be before `insert_generic_materials`
				insert_generic_materials,
			).chain())
		;
	}
}
impl<D: MaterialDeserializer> MaterializePlugin<D, AssetLoadingProcessor<()>> {
	/// Creates a new [`MaterializePlugin`] with an [`AssetLoadingProcessor`].
	pub fn new(deserializer: D) -> Self {
		Self::new_with_processor(deserializer, AssetLoadingProcessor(()))
	}
}

impl<D: MaterialDeserializer, P: MaterialProcessor> MaterializePlugin<D, P> {
	/// Use over [`MaterializePlugin::new`] if you don't want to use an [`AssetLoadingProcessor`].
	pub fn new_with_processor(deserializer: D, processor: P) -> Self {
		Self {
			deserializer: Arc::new(deserializer),
			simple_loader: None,
			animated_materials: true,
			do_text_replacements: true,
			standard_material_color_space_fix: true,
			processor,
		}
	}

	/// Registers a [`GenericMaterial`] loader that loads directly from images. WARNING: This will cause conflicts if you try to load an image file without specifying what type you want to load.
	pub fn with_simple_loader(self, loader: SimpleGenericMaterialLoader) -> Self {
		Self {
			simple_loader: Some(loader),
			..self
		}
	}

	/// Whether to replace special patterns in text, such as replacing `${name}` with the name of the material loading. (Default: `true`)
	pub fn with_text_replacements(self, value: bool) -> Self {
		Self {
			do_text_replacements: value,
			..self
		}
	}

	/// Whether to add [`AnimationPlugin`](animation::AnimationPlugin), animating materials with the [`ANIMATION`](GenericMaterial::ANIMATION) property.
	pub fn with_animated_materials(self, value: bool) -> Self {
		Self {
			animated_materials: value,
			..self
		}
	}

	/// Whether to automatically set maps in [`StandardMaterial`] that aren't supposed to be to sRGB to linear if necessary.
	pub fn with_standard_material_color_space_fix(self, value: bool) -> Self {
		Self {
			standard_material_color_space_fix: value,
			..self
		}
	}

	/// Adds a new processor to the processor stack. The function specified takes in the old processor and produces a new one.
	///
	/// Zero-sized processors are usually tuples, meaning you can just put their type name (e.g. `.with_processor(MyProcessor)`).
	pub fn with_processor<NewP: MaterialProcessor>(self, f: impl FnOnce(P) -> NewP) -> MaterializePlugin<D, NewP> {
		MaterializePlugin {
			deserializer: self.deserializer,
			simple_loader: self.simple_loader,
			animated_materials: self.animated_materials,
			do_text_replacements: self.do_text_replacements,
			standard_material_color_space_fix: self.standard_material_color_space_fix,
			processor: f(self.processor),
		}
	}
}
impl<D: MaterialDeserializer + Default, P: MaterialProcessor + Default> Default for MaterializePlugin<D, P> {
	fn default() -> Self {
		Self::new_with_processor(default(), default())
	}
}

/// Added when a [`MaterializePlugin`] is added. Can be used to check if any [`MaterializePlugin`] has been added.
pub struct MaterializeMarkerPlugin;
impl Plugin for MaterializeMarkerPlugin {
	fn build(&self, _app: &mut App) {}
}

// Can't have these in a MaterializePlugin impl because of the generics.
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
	mut asset_events: MessageReader<AssetEvent<GenericMaterial>>,
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

impl GenericMaterial {
	/// Material property that sets the visibility of the mesh it's applied to.
	#[cfg(feature = "bevy_pbr")]
	pub const VISIBILITY: MaterialProperty<Visibility> = MaterialProperty::new("visibility");
}

#[cfg(feature = "bevy_pbr")]
pub fn visibility_material_property(
	mut query: Query<(&GenericMaterial3d, &mut Visibility), Without<GenericMaterialApplied>>,
	generic_materials: Res<Assets<GenericMaterial>>,
) {
	for (generic_material_holder, mut visibility) in &mut query {
		let Some(generic_material) = generic_materials.get(&generic_material_holder.0) else { continue };
		let Ok(new_visibility) = generic_material.get_property(GenericMaterial::VISIBILITY) else { continue };

		*visibility = *new_visibility;
	}
}

#[cfg(feature = "bevy_pbr")]
pub trait MaterializeAppExt {
	/// Register a material to be able to be created via [`GenericMaterial`].
	///
	/// This also registers the type if it isn't already registered.
	///
	/// NOTES:
	/// - [`from_world`](FromWorld::from_world) is only called once when the material is registered, then that value is cloned each time a new instance is required.
	/// - If you're registering an [`ExtendedMaterial`] that requires [`FromWorld`], you should use [`register_extended_generic_material(...)`](MaterializeAppExt::register_extended_generic_material).
	fn register_generic_material<M: Material + Reflect + Struct + FromWorld + GetTypeRegistration>(&mut self) -> &mut Self;

	/// Registers an [`ExtendedMaterial`] using [`FromWorld`], and sets a shorthand to it.
	///
	/// This function is necessary if either the base or extension requires [`FromWorld`], as [`ExtendedMaterial`] has a [`Default`] impl that would conflict with a [`FromWorld`] impl.
	/// This is also why the base and extension are separate generic parameters.
	fn register_extended_generic_material<
		Base: Material + FromReflect + Typed + Struct + FromWorld + GetTypeRegistration,
		Ext: MaterialExtension + FromReflect + Typed + Struct + FromWorld + GetTypeRegistration,
	>(
		&mut self,
		shorthand: impl Into<String>,
	) -> &mut Self;

	/// Same as [`register_generic_material`](MaterializeAppExt::register_generic_material), but with a provided default value.
	///
	/// This main use of this is for extended materials, allowing you to specify defaults for the base material that you wouldn't be able to otherwise.
	fn register_generic_material_with_default<M: Material + Reflect + Struct + GetTypeRegistration>(&mut self, default_value: M) -> &mut Self;

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
		let default_value = M::from_world(self.world_mut());
		self.register_generic_material_with_default(default_value)
	}

	fn register_extended_generic_material<
		Base: Material + FromReflect + Typed + Struct + FromWorld + GetTypeRegistration,
		Ext: MaterialExtension + FromReflect + Typed + Struct + FromWorld + GetTypeRegistration,
	>(
		&mut self,
		shorthand: impl Into<String>,
	) -> &mut Self {
		let base = Base::from_world(self.world_mut());
		let extension = Ext::from_world(self.world_mut());
		self.register_generic_material_with_default(ExtendedMaterial { base, extension })
			.register_generic_material_shorthand::<ExtendedMaterial<Base, Ext>>(shorthand)
	}

	fn register_generic_material_with_default<M: Material + Reflect + Struct + GetTypeRegistration>(&mut self, default_value: M) -> &mut Self {
		let mut type_registry = self.world().resource::<AppTypeRegistry>().write();
		if type_registry.get(TypeId::of::<M>()).is_none() {
			type_registry.register::<M>();
		}

		type_registry.get_mut(TypeId::of::<M>()).unwrap().insert(ReflectGenericMaterial {
			default_value: Box::new(default_value),
		});

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
