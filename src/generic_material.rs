use std::{
	any::TypeId,
	error::Error,
	fmt, io,
	sync::{Arc, RwLock},
};

use bevy::{
	platform::collections::HashMap,
	prelude::*,
	reflect::{ApplyError, GetTypeRegistration, TypeInfo, TypeRegistration},
};

#[cfg(feature = "bevy_pbr")]
use bevy::{
	asset::{LoadContext, UntypedAssetId},
	ecs::{component::HookContext, world::DeferredWorld},
	reflect::{ReflectMut, Typed},
};
#[cfg(feature = "bevy_pbr")]
use std::any::Any;
use thiserror::Error;

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

/// Material asset containing a type-erased material handle, and generic user-defined properties.
#[derive(Asset, TypePath, Debug)]
#[cfg_attr(not(feature = "bevy_pbr"), derive(Default))]
pub struct GenericMaterial {
	#[cfg(feature = "bevy_pbr")]
	pub handle: Box<dyn ErasedMaterialHandle>,
	pub properties: HashMap<String, Box<dyn Reflect>>,
}
impl GenericMaterial {
	#[cfg(feature = "bevy_pbr")]
	pub fn new(handle: impl Into<Box<dyn ErasedMaterialHandle>>) -> Self {
		Self {
			handle: handle.into(),
			properties: HashMap::default(),
		}
	}

	/// Sets a property to `value`.
	pub fn set_property<T: Reflect + fmt::Debug + Send + Sync>(&mut self, key: impl Into<String>, value: T) {
		self.properties.insert(key.into(), Box::new(value));
	}

	/// Attempts to get the specified property as `T`.
	pub fn get_property<T: Reflect>(&self, key: &str) -> Result<&T, GetPropertyError> {
		let value = self.properties.get(key).ok_or(GetPropertyError::NotFound)?;
		value.downcast_ref().ok_or(GetPropertyError::WrongType {
			found: value.get_represented_type_info(),
		})
	}
}

#[derive(Error, Debug, Clone)]
pub enum GetPropertyError {
	#[error("Property not found")]
	NotFound,
	#[error("Property found doesn't have the required type. Type found: {:?}", found.map(TypeInfo::type_path))]
	WrongType { found: Option<&'static TypeInfo> },
}

/// Maps property names to the types they represent.
#[derive(Resource, Debug, Clone, Default)]
pub struct MaterialPropertyRegistry {
	pub inner: Arc<RwLock<HashMap<String, TypeId>>>,
}

pub trait MaterialPropertyAppExt {
	/// Registers material properties with the specified key to try to deserialize into `T`.
	///
	/// Also registers the type if it hasn't been already.
	fn register_material_property<T: Reflect + GetTypeRegistration>(&mut self, key: impl Into<String>) -> &mut Self;
}
impl MaterialPropertyAppExt for App {
	fn register_material_property<T: Reflect + GetTypeRegistration>(&mut self, key: impl Into<String>) -> &mut Self {
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
}

/// Version of [`ReflectDefault`] that returns `Box<dyn ErasedMaterial>` instead of `Box<dyn Reflect>`.
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

#[cfg(feature = "bevy_pbr")]
pub trait ErasedMaterial: Send + Sync + Reflect + Struct {
	// TODO Can't use just `self` because i can't move out of trait objects.
	fn add_labeled_asset(&self, load_context: &mut LoadContext, label: String) -> Box<dyn ErasedMaterialHandle>;
	fn add_asset(&self, asset_server: &AssetServer) -> Box<dyn ErasedMaterialHandle>;
	fn clone_erased(&self) -> Box<dyn ErasedMaterial>;
}
#[cfg(feature = "bevy_pbr")]
impl<M: Material + Reflect + Struct + Clone> ErasedMaterial for M {
	fn add_labeled_asset(&self, load_context: &mut LoadContext, label: String) -> Box<dyn ErasedMaterialHandle> {
		load_context.add_labeled_asset(label, self.clone()).into()
	}

	fn add_asset(&self, asset_server: &AssetServer) -> Box<dyn ErasedMaterialHandle> {
		asset_server.add(self.clone()).into()
	}

	fn clone_erased(&self) -> Box<dyn ErasedMaterial> {
		Box::new(self.clone())
	}
}
#[cfg(feature = "bevy_pbr")]
impl<M: Material + Reflect + Struct + Clone> From<M> for Box<dyn ErasedMaterial> {
	fn from(value: M) -> Self {
		Box::new(value)
	}
}
#[cfg(feature = "bevy_pbr")]
impl Clone for Box<dyn ErasedMaterial> {
	fn clone(&self) -> Self {
		self.clone_erased()
	}
}

#[cfg(feature = "bevy_pbr")]
pub trait ErasedMaterialHandle: Send + Sync + fmt::Debug + Any {
	fn clone_erased(&self) -> Box<dyn ErasedMaterialHandle>;
	fn insert(&self, entity: EntityWorldMut);
	fn remove(&self, entity: EntityWorldMut);
	fn to_untyped_handle(&self) -> UntypedHandle;
	fn id(&self) -> UntypedAssetId;

	#[allow(clippy::type_complexity)]
	fn modify_with_commands(&self, commands: &mut Commands, modifier: Box<dyn FnOnce(Option<&mut dyn Reflect>) + Send + Sync>);
}
#[cfg(feature = "bevy_pbr")]
impl<M: Material + Reflect> ErasedMaterialHandle for Handle<M> {
	fn clone_erased(&self) -> Box<dyn ErasedMaterialHandle> {
		self.clone().into()
	}

	fn insert(&self, mut entity: EntityWorldMut) {
		entity.insert(MeshMaterial3d(self.clone()));
	}

	fn remove(&self, mut entity: EntityWorldMut) {
		entity.remove::<MeshMaterial3d<M>>();
	}

	fn to_untyped_handle(&self) -> UntypedHandle {
		self.clone().untyped()
	}

	fn id(&self) -> UntypedAssetId {
		self.id().untyped()
	}

	fn modify_with_commands(&self, commands: &mut Commands, modifier: Box<dyn FnOnce(Option<&mut dyn Reflect>) + Send + Sync>) {
		let handle = self.clone();

		commands.queue(move |world: &mut World| {
			let mut assets = world.resource_mut::<Assets<M>>();
			let asset = assets.get_mut(handle.id());
			let asset: Option<&mut dyn Reflect> = match asset {
				Some(m) => Some(m),
				None => None,
			};

			modifier(asset);
		});
	}
}
#[cfg(feature = "bevy_pbr")]
impl<M: Material + Reflect> From<Handle<M>> for Box<dyn ErasedMaterialHandle> {
	fn from(value: Handle<M>) -> Self {
		Box::new(value)
	}
}
#[cfg(feature = "bevy_pbr")]
impl Clone for Box<dyn ErasedMaterialHandle> {
	fn clone(&self) -> Self {
		self.clone_erased()
	}
}

#[cfg(feature = "bevy_pbr")]
impl dyn ErasedMaterialHandle {
	/// Attempts to modify a single field in the material. Writes an error out if something fails.
	pub fn modify_field_with_commands<T: Reflect + Typed + FromReflect + GetTypeRegistration>(
		&self,
		commands: &mut Commands,
		field_name: String,
		value: T,
	) {
		self.modify_with_commands(
			commands,
			Box::new(move |material| {
				let Some(material) = material else { return };
				let ReflectMut::Struct(s) = material.reflect_mut() else { return };

				let Some(field) = s.field_mut(&field_name) else {
					error!(
						"Tried to animate field {field_name} of {}, but said field doesn't exist!",
						s.reflect_short_type_path()
					);
					return;
				};

				let apply_result = if field.represents::<Option<T>>() {
					field.try_apply(&Some(value))
				} else {
					field.try_apply(&value)
				};

				if let Err(err) = apply_result {
					error!(
						"Tried to animate field {field_name} of {}, but failed to apply: {err}",
						s.reflect_short_type_path()
					);
				}
			}),
		);
	}
}

#[derive(Error, Debug)]
pub enum GenericMaterialError {
	#[error("{0}")]
	Io(#[from] io::Error),
	#[error("Deserialize error: {0}")]
	Deserialize(Box<dyn Error + Send + Sync>),
	#[error("No registered material found for type {0}")]
	MaterialTypeNotFound(String),
	#[error("Too many type candidates found for `{0}`: {1:?}")]
	TooManyTypeCandidates(String, Vec<String>),
	#[error("field {field} is of type {expected}, but {found} was provided")]
	WrongType { expected: String, found: String, field: String },
	#[error("{0}")]
	Apply(#[from] ApplyError),
	#[error("Enums defined with structures must have exactly one variant (e.g. `alpha_mode = {{ Mask = 0.5 }}`)")]
	WrongNumberEnumElements,
	#[error("No property by the name of {0}")]
	NoProperty(String),
	#[error("Type not registered: {0}")]
	TypeNotRegistered(&'static str),
	#[error("Property {0} found, but was not registered to any type. Use `App::register_material_property` to register it")]
	PropertyNotRegistered(String),
	#[error("Property {0} found and was registered, but the type it points to isn't registered in the type registry")]
	PropertyTypeNotRegistered(String),
	#[error("Could not get `ReflectFromReflect` for type {0}")]
	NoFromReflect(&'static str),
	#[error("Could not fully reflect property of type {:?}", ty.map(TypeInfo::type_path))]
	FullReflect { ty: Option<&'static TypeInfo> },

	#[error("in field {0} - {1}")]
	InField(String, Box<Self>),

	#[error("in super-material {0} - {1}")]
	InSuperMaterial(String, Box<Self>),
}
