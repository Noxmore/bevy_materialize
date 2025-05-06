use std::{
	any::TypeId,
	sync::{Arc, RwLock},
};

#[cfg(feature = "bevy_image")]
use bevy::image::ImageLoaderSettings;
use bevy::{
	asset::{AssetPath, Deferred, NestedLoader, ParseAssetPathError, StaticTyped, io::AssetSourceId, meta::Settings},
	platform::collections::HashMap,
	prelude::*,
	reflect::{TypeRegistration, TypeRegistry},
};
use serde::{Deserialize, Serialize};

use super::processor::{MaterialProcessor, MaterialProcessorContext};

/// Material processor that loads assets from paths.
#[derive(Clone)]
pub struct AssetLoadingProcessor<P: MaterialProcessor>(pub P);
impl<P: MaterialProcessor> MaterialProcessor for AssetLoadingProcessor<P> {
	type Child = P;
	fn child(&self) -> Option<&Self::Child> {
		Some(&self.0)
	}

	fn try_deserialize<'de, D: serde::Deserializer<'de>>(
		&self,
		ctx: &mut MaterialProcessorContext,
		registration: &TypeRegistration,
		_registry: &TypeRegistry,
		deserializer: D,
	) -> Result<Result<Box<dyn PartialReflect>, D>, D::Error> {
		if let Some(loader) = registration.data::<ReflectGenericMaterialSubAsset>() {
			let path = String::deserialize(deserializer)?;

			let path = relative_asset_path(ctx.load_context.asset_path(), &path).map_err(serde::de::Error::custom)?;

			return Ok(Ok(loader.load(ctx, path)));
		}

		Ok(Err(deserializer))
	}
}

/// Reflected function that loads an asset. Used for asset loading from paths in generic materials.
#[derive(Debug, Clone)]
pub struct ReflectGenericMaterialSubAsset {
	load: fn(&mut MaterialProcessorContext, AssetPath<'static>) -> Box<dyn PartialReflect>,
}
impl ReflectGenericMaterialSubAsset {
	pub fn load(&self, ctx: &mut MaterialProcessorContext, path: AssetPath<'static>) -> Box<dyn PartialReflect> {
		(self.load)(ctx, path)
	}
}

pub trait GenericMaterialSubAssetAppExt {
	/// Registers an asset to be able to be loaded within a [`GenericMaterial`](crate::GenericMaterial).
	///
	/// Specifically, it allows loading of [`Handle<A>`] by simply providing a path relative to the material's directory.
	fn register_generic_material_sub_asset<A: Asset>(&mut self) -> &mut Self;

	/// Insert a modifier into [`GlobalAssetSettingsModifiers`].
	/// # Examples
	/// ```no_run
	/// # use bevy::{prelude::*, image::{ImageLoaderSettings, ImageSampler}};
	/// # use bevy_materialize::prelude::*;
	/// App::new()
	///     .insert_generic_material_asset_settings_modifier(
	///         AssetSettingsTarget::field::<StandardMaterial>("base_color_texture"),
	///         |settings: &mut ImageLoaderSettings| settings.sampler = ImageSampler::nearest(),
	///     )
	///     // All base_color_textures loaded from StandardMaterial now use nearest neighbor filtering!
	/// # ;
	/// ```
	/// To see more about this system, visit [`AssetSettingsModifiers`] and/or see readme.
	fn insert_generic_material_asset_settings_modifier<S: Settings>(
		&mut self,
		target: AssetSettingsTarget<'static>,
		modifier: impl Fn(&mut S) + Clone + Send + Sync + 'static,
	) -> &mut Self;
}
impl GenericMaterialSubAssetAppExt for App {
	#[track_caller]
	fn register_generic_material_sub_asset<A: Asset>(&mut self) -> &mut Self {
		let mut type_registry = self.world().resource::<AppTypeRegistry>().write();
		let registration = match type_registry.get_mut(TypeId::of::<Handle<A>>()) {
			Some(x) => x,
			None => panic!(
				"Asset handle not registered: {}, did you forget to call `add_asset()` first?",
				std::any::type_name::<A>()
			),
		};

		registration.insert(ReflectGenericMaterialSubAsset {
			load: |processor, path| Box::new(processor.load::<A>(path)),
		});

		drop(type_registry);

		self
	}

	#[track_caller]
	fn insert_generic_material_asset_settings_modifier<S: Settings>(
		&mut self,
		target: AssetSettingsTarget<'static>,
		modifier: impl Fn(&mut S) + Clone + Send + Sync + 'static,
	) -> &mut Self {
		self.world().resource::<GlobalAssetSettingsModifiers>().insert(target, modifier);
		self
	}
}

/// Produces an asset path relative to another for use in generic material loading.
///
/// # Examples
/// ```
/// # use bevy_materialize::load::asset::relative_asset_path;
/// assert_eq!(relative_asset_path(&"materials/foo.toml".into(), "foo.png").unwrap(), "materials/foo.png".into());
/// assert_eq!(relative_asset_path(&"materials/foo.toml".into(), "textures/foo.png").unwrap(), "materials/textures/foo.png".into());
/// assert_eq!(relative_asset_path(&"materials/foo.toml".into(), "/textures/foo.png").unwrap(), "textures/foo.png".into());
/// assert_eq!(relative_asset_path(&"materials/foo.toml".into(), "\\textures\\foo.png").unwrap(), "textures\\foo.png".into());
/// ```
pub fn relative_asset_path(relative_to: &AssetPath<'static>, path: &str) -> Result<AssetPath<'static>, ParseAssetPathError> {
	let parent = relative_to.parent().unwrap_or_default();

	// Handle root
	let root_pattern = ['/', '\\'];

	if path.starts_with(root_pattern) {
		let mut asset_path = AssetPath::from(path.trim_start_matches(root_pattern)).into_owned();
		if let AssetSourceId::Default = asset_path.source() {
			asset_path = asset_path.with_source(relative_to.source().clone_owned());
		}

		Ok(asset_path)
	} else {
		parent.resolve(path)
	}
}

/// A function that modifies a nested loader, expected to call `with_settings()`. This is like this instead of the modifier that `with_settings()` takes because of the generic in that function.
///
/// If the api that `with_settings()` calls was public, we would be able to use that instead, oh well!
pub type AssetSettingsModifier = Arc<
	dyn for<'ctx, 'builder> Fn(NestedLoader<'ctx, 'builder, StaticTyped, Deferred>) -> NestedLoader<'ctx, 'builder, StaticTyped, Deferred>
		+ Send
		+ Sync
		+ 'static,
>;

/// Asset settings modifiers can either target specific material's fields, or specific [material properties](crate::MaterialProperty).
///
/// NOTE: Currently, specific fields *within* material properties aren't supported for simplicity, if your use case requires this, make an issue!
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetSettingsTarget<'a> {
	/// A field within a [`Material`] struct.
	Field {
		/// The type id of the material.
		type_id: TypeId,
		/// The field within the material.
		field: &'a str,
	},
	/// A [`MaterialProperty`](crate::MaterialProperty). Contains the name of the property you want to target.
	Property(&'a str),
}
impl<'a> AssetSettingsTarget<'a> {
	/// Shorthand for
	/// ```
	/// # use bevy_materialize::load::asset::AssetSettingsTarget;
	/// # use std::any::TypeId;
	/// # type T = ();
	/// AssetSettingsTarget::Field { type_id: TypeId::of::<T>(), field: "<field name>" }
	/// # ;
	/// ```
	#[inline]
	pub fn field<T: Asset>(field: &'a str) -> Self {
		Self::Field {
			type_id: TypeId::of::<T>(),
			field,
		}
	}
}

/// Settings passed to a [`GenericMaterial`](crate::GenericMaterial) loader to modify sub-assets' settings.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct AssetSettingsModifiers {
	/// Maps a material's field or a material property to a function that modifies the asset settings when loading sub-assets from that field/property.
	///
	/// Use [`insert(...)`](Self::insert), [`replace(...)`](Self::replace) or [`with(...)`](Self::with) to modify this.
	#[serde(skip)]
	pub settings_map: HashMap<AssetSettingsTarget<'static>, AssetSettingsModifier>,
}
impl AssetSettingsModifiers {
	/// Inserts a new settings modifier into the map. This function stacks, meaning the code
	/// ```
	/// # use bevy_materialize::{prelude::*, load::asset::AssetSettingsModifiers};
	/// # use bevy::{prelude::*, image::{ImageLoaderSettings, ImageSampler}};
	/// let mut modifiers = AssetSettingsModifiers::default();
	/// modifiers.insert(AssetSettingsTarget::field::<StandardMaterial>("base_color_texture"), |settings: &mut ImageLoaderSettings| settings.sampler = ImageSampler::linear());
	/// modifiers.insert(AssetSettingsTarget::field::<StandardMaterial>("base_color_texture"), |settings: &mut ImageLoaderSettings| settings.is_srgb = false);
	/// ```
	/// Will cause [`base_color_texture`](StandardMaterial::base_color_texture) to use both linear filtering *and* a linear color space.
	///
	/// If you want to fully *replace* the modifier, you should use [`replace(...)`](Self::replace).
	pub fn insert<S: Settings>(&mut self, target: AssetSettingsTarget<'static>, modifier: impl Fn(&mut S) + Clone + Send + Sync + 'static) {
		if let Some(previous_modifier) = self.settings_map.remove(&target) {
			self.settings_map.insert(
				target,
				Arc::new(move |loader| {
					let loader = previous_modifier(loader);
					loader.with_settings(modifier.clone())
				}),
			);
		} else {
			self.replace(target, modifier);
		}
	}

	/// [`insert(...)`](Self::insert) without generics. You should almost always use said function over this.
	///
	/// Like [`insert(...)`](Self::insert), this function does stack the modifier rather than overwrite.
	pub fn insert_raw(&mut self, target: AssetSettingsTarget<'static>, modifier: AssetSettingsModifier) {
		if let Some(previous_modifier) = self.settings_map.remove(&target) {
			self.settings_map.insert(
				target,
				Arc::new(move |loader| {
					// We don't call `insert_raw` from `insert` because this causes two Arc references per one of these functions, rather than just the one above thanks to the generics
					let loader = previous_modifier(loader);
					(modifier)(loader)
				}),
			);
		} else {
			self.settings_map.insert(target, modifier);
		}
	}

	/// Shorthand for `settings_map.insert` and calling `with_settings`, but not called `insert` because the default behavior is to stack.
	///
	/// You usually don't need this, and should use [`insert(...)`](Self::insert) instead.
	pub fn replace<S: Settings>(&mut self, target: AssetSettingsTarget<'static>, modifier: impl Fn(&mut S) + Clone + Send + Sync + 'static) {
		self.settings_map
			.insert(target, Arc::new(move |loader| loader.with_settings(modifier.clone())));
	}

	// We don't have a `replace_raw` function, since it would just be passing directly through to `settings_map.insert(...)` which is public api.

	/// Calls [`insert(...)`](Self::insert) and returns `self` to provide a little builder syntax.
	pub fn with<S: Settings>(mut self, target: AssetSettingsTarget<'static>, modifier: impl Fn(&mut S) + Clone + Send + Sync + 'static) -> Self {
		self.insert(target, modifier);
		self
	}
}

/// Globally stored [`AssetSettingsModifiers`], this is used as a base when loading generic materials with the [`AssetSettingsModifiers`] provided per-asset-load inserted overtop.
#[derive(Resource, Clone)]
#[cfg_attr(not(feature = "bevy_pbr"), derive(Default))]
pub struct GlobalAssetSettingsModifiers {
	pub inner: Arc<RwLock<AssetSettingsModifiers>>,
}
impl GlobalAssetSettingsModifiers {
	/// Calls the inner [`AssetSettingsModifiers::insert`] function. See docs for that.
	pub fn insert<S: Settings>(&self, target: AssetSettingsTarget<'static>, modifier: impl Fn(&mut S) + Clone + Send + Sync + 'static) {
		self.inner.write().unwrap().insert(target, modifier);
	}
}
#[cfg(feature = "bevy_pbr")]
impl Default for GlobalAssetSettingsModifiers {
	fn default() -> Self {
		let linear_modifier = |settings: &mut ImageLoaderSettings| settings.is_srgb = false;

		#[rustfmt::skip]
		let modifiers = AssetSettingsModifiers::default()
			.with(AssetSettingsTarget::field::<StandardMaterial>("normal_map_texture"), linear_modifier)
			.with(AssetSettingsTarget::field::<StandardMaterial>("occlusion_texture"), linear_modifier)
			.with(AssetSettingsTarget::field::<StandardMaterial>("metallic_roughness_texture"), linear_modifier)
			.with(AssetSettingsTarget::field::<StandardMaterial>("anisotropy_texture"), linear_modifier)
			.with(AssetSettingsTarget::field::<StandardMaterial>("clearcoat_texture"), linear_modifier)
			.with(AssetSettingsTarget::field::<StandardMaterial>("clearcoat_roughness_texture"), linear_modifier)
			.with(AssetSettingsTarget::field::<StandardMaterial>("clearcoat_normal_texture"), linear_modifier)
		;

		Self {
			inner: Arc::new(RwLock::new(modifiers)),
		}
	}
}
