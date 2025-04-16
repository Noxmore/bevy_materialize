#[cfg(feature = "bevy_image")]
use super::set_image_loader_settings;
use super::{relative_asset_path, ReflectGenericMaterialLoad};
use ::serde;
use bevy::asset::AssetPath;
#[cfg(feature = "bevy_image")]
use bevy::image::ImageLoaderSettings;
use bevy::reflect::{serde::*, *};
use bevy::{asset::LoadContext, prelude::*};
use serde::Deserialize;

/// API wrapping Bevy's [`ReflectDeserializerProcessor`](https://docs.rs/bevy/latest/bevy/reflect/serde/trait.ReflectDeserializerProcessor.html).
/// This allows you to modify data as it's being deserialized. For example, this system is used for loading assets, treating strings as paths.
///
/// It's used much like Rust's iterator API, each processor having a child processor that is stored via generic. If you want to make your own, check out [`AssetLoadingProcessor`] for a simple example of an implementation.
pub trait MaterialSubProcessor: Clone + Send + Sync + 'static {
	type Child: MaterialSubProcessor;

	fn child(&self) -> Option<&Self::Child>;

	fn try_deserialize<'de, D: serde::Deserializer<'de>>(
		&self,
		ctx: &mut MaterialProcessorContext,
		registration: &TypeRegistration,
		registry: &TypeRegistry,
		deserializer: D,
	) -> Result<Result<Box<dyn PartialReflect>, D>, D::Error>;

	fn try_deserialize_recursive<'de, D: serde::Deserializer<'de>>(
		&self,
		ctx: &mut MaterialProcessorContext,
		registration: &TypeRegistration,
		registry: &TypeRegistry,
		deserializer: D,
	) -> Result<Result<Box<dyn PartialReflect>, D>, D::Error> {
		if let Some(child) = self.child() {
			match child.try_deserialize_recursive(ctx, registration, registry, deserializer) {
				Ok(Err(returned_deserializer)) => self.try_deserialize(ctx, registration, registry, returned_deserializer),
				out => out,
			}
		} else {
			Ok(Err(deserializer))
		}
	}
}

impl MaterialSubProcessor for () {
	type Child = Self;
	fn child(&self) -> Option<&Self::Child> {
		None
	}

	fn try_deserialize<'de, D: serde::Deserializer<'de>>(
		&self,
		_ctx: &mut MaterialProcessorContext,
		_registration: &TypeRegistration,
		_registry: &TypeRegistry,
		deserializer: D,
	) -> Result<Result<Box<dyn PartialReflect>, D>, D::Error> {
		Ok(Err(deserializer))
	}
}

/// Material processor that loads assets from paths.
#[derive(Clone)]
pub struct AssetLoadingProcessor<P: MaterialSubProcessor>(pub P);
impl<P: MaterialSubProcessor> MaterialSubProcessor for AssetLoadingProcessor<P> {
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
		if let Some(loader) = registration.data::<ReflectGenericMaterialLoad>() {
			let path = String::deserialize(deserializer)?;

			let path = relative_asset_path(ctx.load_context.asset_path(), &path).map_err(serde::de::Error::custom)?;

			return Ok(Ok((loader.load)(ctx, path)));
		}

		Ok(Err(deserializer))
	}
}

/// Data used for [`MaterialSubProcessor`]
pub struct MaterialProcessorContext<'w, 'l> {
	#[cfg(feature = "bevy_image")]
	pub image_settings: ImageLoaderSettings,
	pub load_context: &'l mut LoadContext<'w>,
}
impl MaterialProcessorContext<'_, '_> {
	/// Loads via `load_context` but passes image load settings through if the `bevy_image` feature is enabled.
	pub fn load_with_image_settings<'b, A: Asset>(&mut self, path: impl Into<AssetPath<'b>>) -> Handle<A> {
		#[cfg(feature = "bevy_image")]
		return self
			.load_context
			.loader()
			.with_settings(set_image_loader_settings(&self.image_settings))
			.load(path);
		#[cfg(not(feature = "bevy_image"))]
		return self.load_context.load(path);
	}
}

/// Contains a [`MaterialSubProcessor`] and context, and kicks off the processing.
pub struct MaterialDeserializerProcessor<'w, 'l, P: MaterialSubProcessor> {
	pub ctx: MaterialProcessorContext<'w, 'l>,
	pub sub_processor: &'l P,
}

impl<P: MaterialSubProcessor> ReflectDeserializerProcessor for MaterialDeserializerProcessor<'_, '_, P> {
	fn try_deserialize<'de, D: serde::Deserializer<'de>>(
		&mut self,
		registration: &TypeRegistration,
		registry: &TypeRegistry,
		deserializer: D,
	) -> Result<Result<Box<dyn PartialReflect>, D>, D::Error> {
		self.sub_processor
			.try_deserialize_recursive(&mut self.ctx, registration, registry, deserializer)
	}
}
