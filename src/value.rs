use std::{
	any::{type_name, TypeId},
	fmt, io,
};

use crate::load::GenericMaterialDeserializationProcessor;
use bevy::{
	prelude::*,
	reflect::{serde::TypedReflectDeserializer, TypeRegistration, TypeRegistry},
};
use serde::{de::DeserializeSeed, Deserializer};
use std::error::Error;

/// Trait meant for `Value` types of different serialization libraries. For example, for the [`toml`] crate, this is implemented for [`toml::Value`].
///
/// This is for storing general non type specific data for deserializing on demand, such as in [`GenericMaterial`](crate::GenericMaterial) properties.
///
/// NOTE: Because of the limitation of not being able to implement foreign traits for foreign types, this is automatically implemented for applicable types implementing the [`Deserializer`] trait.
pub trait GenericValue: fmt::Debug + Send + Sync {
	fn generic_deserialize(
		&self,
		registration: &TypeRegistration,
		registry: &TypeRegistry,
		processor: &mut GenericMaterialDeserializationProcessor,
	) -> Result<Box<dyn PartialReflect>, Box<dyn Error + Send + Sync>>;
}
impl<T: Deserializer<'static, Error: Send + Sync> + fmt::Debug + Clone + Send + Sync + 'static> GenericValue for T {
	fn generic_deserialize(
		&self,
		registration: &TypeRegistration,
		registry: &TypeRegistry,
		processor: &mut GenericMaterialDeserializationProcessor,
	) -> Result<Box<dyn PartialReflect>, Box<dyn Error + Send + Sync>> {
		Ok(TypedReflectDeserializer::with_processor(registration, registry, processor).deserialize(self.clone())?)
	}
}

/// Thin wrapper type implementing [`GenericValue`]. Used for directly passing values to properties.
/// Usually you should use [`GenericMaterial::set_property`](crate::GenericMaterial::set_property), which uses this under the hood.
#[derive(Debug, Clone, Deref, DerefMut)]
pub struct DirectGenericValue<T>(pub T);
impl<T: PartialReflect + fmt::Debug + Clone + Send + Sync> GenericValue for DirectGenericValue<T> {
	fn generic_deserialize(
		&self,
		registration: &TypeRegistration,
		_registry: &TypeRegistry,
		_processor: &mut GenericMaterialDeserializationProcessor,
	) -> Result<Box<dyn PartialReflect>, Box<dyn Error + Send + Sync>> {
		if registration.type_id() == TypeId::of::<T>() {
			Ok(Box::new(self.0.clone()))
		} else {
			Err(Box::new(io::Error::other(format!(
				"Wrong type. Expected {}, found {}",
				registration.type_info().type_path(),
				type_name::<T>()
			))))
		}
	}
}

#[test]
fn direct_values() {
	use crate::*;
	use bevy::time::TimePlugin;

	App::new()
		.register_type::<StandardMaterial>()
		.register_type::<Visibility>()
		.add_plugins((
			AssetPlugin::default(),
			TimePlugin,
			MaterializePlugin::new(load::deserializer::TomlMaterialDeserializer),
		))
		.add_systems(Startup, setup)
		.add_systems(PostStartup, test)
		.run();

	fn setup(mut assets: ResMut<Assets<GenericMaterial>>) {
		let mut material = GenericMaterial {
			#[cfg(feature = "bevy_pbr")]
			handle: Handle::<StandardMaterial>::default().into(),
			properties: default(),
		};

		material.set_property(GenericMaterial::VISIBILITY, Visibility::Hidden);

		assets.add(material);
	}

	fn test(generic_materials: GenericMaterials) {
		assert!(matches!(
			generic_materials.iter().next().unwrap().get_property(GenericMaterial::VISIBILITY),
			Ok(Visibility::Hidden)
		));
	}
}
