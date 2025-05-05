use std::{
	any::TypeId,
	marker::PhantomData,
	sync::{Arc, RwLock},
};

use bevy::{
	platform::collections::HashMap,
	prelude::*,
	reflect::{GetTypeRegistration, TypeInfo},
};
use thiserror::Error;

/// Maps property names to the types they represent.
#[derive(Resource, Debug, Clone, Default)]
pub struct MaterialPropertyRegistry {
	pub inner: Arc<RwLock<HashMap<String, TypeId>>>,
}

/// Helper type containing both a type and key for material properties.
///
/// # Examples
/// ```
/// # use bevy::prelude::*;
/// # use bevy_materialize::prelude::*;
///
/// pub trait MyMaterialProperties {
///     const MY_PROPERTY: MaterialProperty<f32> = MaterialProperty::new("my_property");
/// }
/// impl MyMaterialProperties for GenericMaterial {}
///
/// fn example_main() {
///     App::new()
///         .register_material_property(GenericMaterial::MY_PROPERTY)
///         // ...
/// # ;
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct MaterialProperty<T> {
	pub key: &'static str,
	_marker: PhantomData<T>,
}
impl<T> MaterialProperty<T> {
	pub const fn new(key: &'static str) -> Self {
		Self { key, _marker: PhantomData }
	}
}

/// Errors that may occur when retrieving a property from a [`GenericMaterial`](crate::GenericMaterial).
#[derive(Error, Debug, Clone)]
pub enum GetPropertyError {
	#[error("Property not found")]
	NotFound,
	#[error("Property found doesn't have the required type. Type found: {:?}", found.map(TypeInfo::type_path))]
	WrongType { found: Option<&'static TypeInfo> },
}

pub trait MaterialPropertyAppExt {
	/// Registers material properties with the specified key to try to deserialize into `T`. Overwrites registration if one already exists for `key`.
	///
	/// Also registers the type if it hasn't been already.
	fn register_material_property_manual<T: Reflect + GetTypeRegistration>(&mut self, key: impl Into<String>) -> &mut Self;

	/// Uses the [`MaterialProperty`] helper type to register a material property. Overwrites registration if one already exists for `key`.
	///
	/// Also registers the type if it hasn't been already.
	fn register_material_property<T: Reflect + GetTypeRegistration>(&mut self, property: MaterialProperty<T>) -> &mut Self;
}
impl MaterialPropertyAppExt for App {
	fn register_material_property_manual<T: Reflect + GetTypeRegistration>(&mut self, key: impl Into<String>) -> &mut Self {
		let mut type_registry = self.world().resource::<AppTypeRegistry>().write();
		if type_registry.get(TypeId::of::<T>()).is_none() {
			type_registry.register::<T>();
		}
		drop(type_registry);

		let mut property_map = self.world().resource::<MaterialPropertyRegistry>().inner.write().unwrap();
		property_map.insert(key.into(), TypeId::of::<T>());
		drop(property_map);

		self
	}

	fn register_material_property<T: Reflect + GetTypeRegistration>(&mut self, property: MaterialProperty<T>) -> &mut Self {
		self.register_material_property_manual::<T>(property.key)
	}
}
