use std::any::TypeId;

#[cfg(feature = "bevy_image")]
use bevy::image::ImageLoaderSettings;
use bevy::{
	asset::{AssetPath, ParseAssetPathError, io::AssetSourceId},
	prelude::*,
	reflect::{TypeRegistration, TypeRegistry},
};
use serde::Deserialize;

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

	/// Same as [`register_generic_material_sub_asset`](Self::register_generic_material_sub_asset), but passes image settings through.
	/// This will cause an error if the asset loader doesn't use image settings.
	fn register_generic_material_sub_asset_image_settings_passthrough<A: Asset>(&mut self) -> &mut Self;
}

/// Reduces code duplication for the functions below.
fn register_generic_material_sub_asset_internal<A: Asset>(app: &mut App, loader: ReflectGenericMaterialSubAsset) -> &mut App {
	let mut type_registry = app.world().resource::<AppTypeRegistry>().write();
	let registration = match type_registry.get_mut(TypeId::of::<Handle<A>>()) {
		Some(x) => x,
		None => panic!("Asset handle not registered: {}", std::any::type_name::<A>()),
	};

	registration.insert(loader);

	drop(type_registry);

	app
}

impl GenericMaterialSubAssetAppExt for App {
	#[track_caller]
	fn register_generic_material_sub_asset<A: Asset>(&mut self) -> &mut Self {
		register_generic_material_sub_asset_internal::<A>(
			self,
			ReflectGenericMaterialSubAsset {
				load: |processor, path| Box::new(processor.load_context.load::<A>(path)),
			},
		)
	}

	#[track_caller]
	fn register_generic_material_sub_asset_image_settings_passthrough<A: Asset>(&mut self) -> &mut Self {
		register_generic_material_sub_asset_internal::<A>(
			self,
			ReflectGenericMaterialSubAsset {
				load: |processor, path| Box::new(processor.load_with_image_settings::<A>(path)),
			},
		)
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

// TODO: This ignores meta files. Is there some way to check if a meta file is being used?

/// Returns a function for setting an asset loader's settings to the supplied [`ImageLoaderSettings`].
#[cfg(feature = "bevy_image")]
pub fn set_image_loader_settings(settings: &ImageLoaderSettings) -> impl Fn(&mut ImageLoaderSettings) + 'static {
	let settings = settings.clone();
	move |s| *s = settings.clone()
}
