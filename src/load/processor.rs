use ::serde;
use bevy::reflect::{serde::*, *};
use bevy::{asset::LoadContext, prelude::*};

/// API wrapping Bevy's [`ReflectDeserializerProcessor`](https://docs.rs/bevy/latest/bevy/reflect/serde/trait.ReflectDeserializerProcessor.html).
/// This allows you to modify data as it's being deserialized. For example, this system is used for loading assets, treating strings as paths.
///
/// If you want to make your own, check out [`AssetLoadingProcessor`](crate::AssetLoadingProcessor) for a simple example of an implementation.
///
/// Material processors are assembled as a type stack that terminates at `()` (unit), each processor having a child processor that is stored via generic.
///
/// For example:
/// ```ignore
/// Processor2<Processor1<()>>
/// ```
///
/// It starts at the lowest in the stack, `()`, which is a no-op processor that has no child, and immediately gives the deserializer up to `Processor1`.
/// Then as `Processor1` gets hold of the deserializer, it can decide to override the default deserialization
/// and immediately return from the stack, or give the deserializer to `Processor2`, and so on.
///
/// If no processor overrides the deserialization, giving the deserializer back every time, then regular deserialization proceeds as normal.
///
/// Processors are preferably tuple structs like so
/// ```
/// # use bevy_materialize::load::processor::MaterialProcessor;
/// struct MyMaterialProcessor<P: MaterialProcessor>(pub P);
/// ```
/// This makes the API for adding them to your `MaterializePlugin` super simple.
pub trait MaterialProcessor: Clone + Send + Sync + 'static {
	/// The type of processor this processor holds as a child that will hand the deserializer to this. Should be set from a generic in the struct.
	type Child: MaterialProcessor;

	/// Should **never** return [`None`] unless you want your processor to be a dead-end, which is what `()` is for.
	fn child(&self) -> Option<&Self::Child>;

	/// Passes through to [`ReflectDeserializerProcessor::try_deserialize`], see the documentation for that for details on how to use.
	fn try_deserialize<'de, D: serde::Deserializer<'de>>(
		&self,
		ctx: &mut MaterialProcessorContext,
		registration: &TypeRegistration,
		registry: &TypeRegistry,
		deserializer: D,
	) -> Result<Result<Box<dyn PartialReflect>, D>, D::Error>;

	/// Recursively attempts to deserialize through child processors, see [`MaterialProcessor`] documentation for exactly *how* it does this.
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

/// The root processor. Has no child, and immediately gives back its deserializer.
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

/// Data used for [`MaterialProcessor`]
pub struct MaterialProcessorContext<'w, 'l> {
	pub load_context: &'l mut LoadContext<'w>,
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
