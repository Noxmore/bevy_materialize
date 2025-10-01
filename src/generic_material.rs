use std::sync::{Arc, RwLock};

use bevy::{platform::collections::HashMap, prelude::*, reflect::TypeRegistration};

#[cfg(feature = "bevy_pbr")]
use bevy::ecs::{lifecycle::HookContext, world::DeferredWorld};

#[cfg(feature = "bevy_pbr")]
use crate::erased_material::{ErasedMaterial, ErasedMaterialHandle};

use crate::{material_property::GetPropertyError, prelude::MaterialProperty};

/// Generic version of [`MeshMaterial3d`]. Stores a handle to a [`GenericMaterial`].
///
/// When on an entity, this automatically inserts the appropriate [`MeshMaterial3d`].
///
/// When removing or replacing this component, the inserted [`MeshMaterial3d`] will be removed.
#[derive(Component, Reflect, Debug, Clone, PartialEq, Eq, Default, Deref, DerefMut)]
#[cfg_attr(feature = "bevy_pbr", component(on_replace = Self::on_replace))]
#[reflect(Component, Default)]
pub struct GenericMaterial3d(pub Handle<GenericMaterial>);
impl GenericMaterial3d {
	#[cfg(feature = "bevy_pbr")]
	fn on_replace(mut world: DeferredWorld, ctx: HookContext) {
		let generic_material_handle = &world.entity(ctx.entity).get::<Self>().unwrap().0;
		let Some(generic_material) = world.resource::<Assets<GenericMaterial>>().get(generic_material_handle) else { return };
		let material_handle = generic_material.handle.clone();

		world.commands().queue(move |world: &mut World| {
			let Ok(mut entity) = world.get_entity_mut(ctx.entity) else { return };

			entity.remove::<GenericMaterialApplied>();
			material_handle.remove(entity);
		});
	}
}

/// Automatically put on entities when their [`GenericMaterial3d`] inserts [`MeshMaterial3d`].
/// This is required because [`MeshMaterial3d`] is generic, and as such can't be used in query parameters for generic materials.
#[cfg(feature = "bevy_pbr")]
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct GenericMaterialApplied;

/// Material asset containing a type-erased material handle, and arbitrary user-defined properties.
#[derive(Asset, TypePath, Debug)]
#[cfg_attr(not(feature = "bevy_pbr"), derive(Default))]
pub struct GenericMaterial {
	#[cfg(feature = "bevy_pbr")]
	pub handle: ErasedMaterialHandle,
	pub properties: HashMap<String, Box<dyn Reflect>>,
}
impl GenericMaterial {
	#[cfg(feature = "bevy_pbr")]
	pub fn new(handle: impl Into<ErasedMaterialHandle>) -> Self {
		Self {
			handle: handle.into(),
			properties: HashMap::default(),
		}
	}

	/// Sets a property to `value`.
	pub fn set_property_manual<T: Reflect>(&mut self, key: impl Into<String>, value: T) {
		self.properties.insert(key.into(), Box::new(value));
	}

	/// Sets a property to `value`.
	pub fn set_property<T: Reflect>(&mut self, property: MaterialProperty<T>, value: T) {
		self.set_property_manual(property.key, value);
	}

	/// Attempts to get the specified property as `T`.
	pub fn get_property_manual<T: Reflect>(&self, key: &str) -> Result<&T, GetPropertyError> {
		let value = self.properties.get(key).ok_or(GetPropertyError::NotFound)?;
		value.downcast_ref().ok_or(GetPropertyError::WrongType {
			found: value.get_represented_type_info(),
		})
	}

	/// Attempts to get the specified property.
	pub fn get_property<T: Reflect>(&self, property: MaterialProperty<T>) -> Result<&T, GetPropertyError> {
		self.get_property_manual(property.key)
	}
}

/// Stores a default value of a certain material that is cloned whenever a new copy of said material is needed to load a [`GenericMaterial`].
#[cfg(feature = "bevy_pbr")]
#[derive(Clone)]
pub struct ReflectGenericMaterial {
	pub(crate) default_value: Box<dyn ErasedMaterial>,
}
#[cfg(feature = "bevy_pbr")]
impl ReflectGenericMaterial {
	pub fn default(&self) -> Box<dyn ErasedMaterial> {
		self.default_value.clone_erased()
	}
}

/// Collection of material type name shorthands for use loading by [`GenericMaterial`]s.
#[derive(Resource, Debug, Clone, Default)]
pub struct GenericMaterialShorthands {
	pub values: Arc<RwLock<HashMap<String, TypeRegistration>>>,
}
