use std::any::TypeId;

use bevy::{
	asset::{AssetPath, ParseAssetPathError, io::AssetSourceId},
	prelude::*,
	reflect::{TypeRegistration, TypeRegistry},
};
use serde::Deserialize;

use super::processor::{MaterialProcessor, MaterialProcessorContext};

/// Material processor that loads assets from paths.
#[derive(TypePath, Clone)]
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

			let path = relative_asset_path(ctx.load_context.path(), &path).map_err(serde::de::Error::custom)?;

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
}
impl GenericMaterialSubAssetAppExt for App {
	#[track_caller]
	fn register_generic_material_sub_asset<A: Asset>(&mut self) -> &mut Self {
		let mut type_registry = self.world().resource::<AppTypeRegistry>().write();
		let registration = match type_registry.get_mut(TypeId::of::<Handle<A>>()) {
			Some(x) => x,
			None => panic!("Asset handle not registered: {}", std::any::type_name::<A>()),
		};

		registration.insert(ReflectGenericMaterialSubAsset {
			load: |processor, path| Box::new(processor.load_context.load::<A>(path)),
		});

		drop(type_registry);

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
		let mut asset_path = AssetPath::try_parse(path.trim_start_matches(root_pattern))?.into_owned();
		if let AssetSourceId::Default = asset_path.source() {
			asset_path = asset_path.with_source(relative_to.source().clone_owned());
		}

		Ok(asset_path)
	} else {
		parent.resolve(path)
	}
}
