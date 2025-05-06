use super::asset::{AssetSettingsModifiers, AssetSettingsTarget};
use ::serde;
use bevy::asset::AssetPath;
use bevy::reflect::{serde::*, *};
use bevy::{asset::LoadContext, prelude::*};

/// API wrapping Bevy's [`ReflectDeserializerProcessor`](https://docs.rs/bevy/latest/bevy/reflect/serde/trait.ReflectDeserializerProcessor.html).
/// This allows you to modify data as it's being deserialized. For example, this system is used for loading assets, treating strings as paths.
///
/// It's used much like Rust's iterator API, each processor having a child processor that is stored via generic. If you want to make your own, check out [`AssetLoadingProcessor`](crate::AssetLoadingProcessor) for a simple example of an implementation.
pub trait MaterialProcessor: Clone + Send + Sync + 'static {
	type Child: MaterialProcessor;

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

impl MaterialProcessor for () {
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

/// Data used for [`MaterialProcessor`] when
pub struct MaterialProcessorContext<'w, 'l> {
	pub asset_target: AssetSettingsTarget<'l>,
	pub settings_modifiers: &'l AssetSettingsModifiers,
	pub load_context: &'l mut LoadContext<'w>,
}
impl MaterialProcessorContext<'_, '_> {
	/// Loads an asset, you should do this instead of going through `load_context` to respect asset settings overrides.
	pub fn load<'b, A: Asset>(&mut self, path: impl Into<AssetPath<'b>>) -> Handle<A> {
		let mut loader = self.load_context.loader();

		if let Some(modifier) = self.settings_modifiers.settings_map.get(&self.asset_target) {
			loader = modifier(loader);
		}

		loader.load(path)
	}
}

/// Contains a [`MaterialProcessor`] and context, and kicks off the processing.
pub struct MaterialDeserializerProcessor<'w, 'l, P: MaterialProcessor> {
	pub ctx: MaterialProcessorContext<'w, 'l>,
	pub material_processor: &'l P,
}

impl<P: MaterialProcessor> ReflectDeserializerProcessor for MaterialDeserializerProcessor<'_, '_, P> {
	fn try_deserialize<'de, D: serde::Deserializer<'de>>(
		&mut self,
		registration: &TypeRegistration,
		registry: &TypeRegistry,
		deserializer: D,
	) -> Result<Result<Box<dyn PartialReflect>, D>, D::Error> {
		self.material_processor
			.try_deserialize_recursive(&mut self.ctx, registration, registry, deserializer)
	}
}
