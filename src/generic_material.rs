use std::{
	any::{type_name, TypeId},
	borrow::Cow,
	error::Error,
	fmt, io,
	sync::{Arc, RwLock},
};

use crate::{load::GenericMaterialDeserializationProcessor, value::DirectGenericValue, value::GenericValue};
use bevy::{
	asset::AssetPath,
	ecs::system::SystemParam,
	platform::collections::HashMap,
	prelude::*,
	reflect::{ApplyError, TypeRegistration},
};

#[cfg(feature = "bevy_pbr")]
use bevy::{
	asset::{LoadContext, UntypedAssetId},
	ecs::{component::HookContext, world::DeferredWorld},
	reflect::{GetTypeRegistration, ReflectMut, Typed},
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
	// This could be better stored as a dyn PartialReflect with types like DynamicStruct,
	// but as far as i can tell Bevy's deserialization infrastructure does not support that
	pub properties: HashMap<String, Box<dyn GenericValue>>,
}
impl GenericMaterial {
	/// Property that changes the visibility component of applied entities to this value.
	#[cfg(feature = "bevy_pbr")]
	pub const VISIBILITY: MaterialProperty<Visibility> = MaterialProperty::new("visibility", || Visibility::Inherited);

	#[cfg(feature = "bevy_pbr")]
	pub fn new(handle: impl Into<Box<dyn ErasedMaterialHandle>>) -> Self {
		Self {
			handle: handle.into(),
			properties: HashMap::default(),
		}
	}

	/// Sets a property to a [`DirectGenericValue`] containing `value`.
	pub fn set_property<T: PartialReflect + fmt::Debug + Clone + Send + Sync>(&mut self, property: MaterialProperty<T>, value: T) {
		self.properties.insert(property.key.to_string(), Box::new(DirectGenericValue(value)));
	}
}

/// Contains all necessary information to parse properties of a [`GenericMaterial`].
#[derive(Clone)]
pub struct GenericMaterialView<'w> {
	pub material: &'w GenericMaterial,
	pub id: AssetId<GenericMaterial>,
	/// You can get an asset path with the supplied AssetServer, unless the asset is currently being loaded TODO ?
	pub path: Option<Cow<'w, AssetPath<'static>>>,
	pub asset_server: &'w AssetServer,
	pub type_registry: &'w AppTypeRegistry,
}
impl GenericMaterialView<'_> {
	pub fn get_property<T: PartialReflect>(&self, property: MaterialProperty<T>) -> Result<T, GenericMaterialError> {
		let mut value = (property.default)();
		let registry = self.type_registry.read();
		let registration = registry
			.get(TypeId::of::<T>())
			.ok_or(GenericMaterialError::TypeNotRegistered(type_name::<T>()))?;

		let mut processor = GenericMaterialDeserializationProcessor::Loaded {
			asset_server: self.asset_server,
			path: self.path.as_ref().map(Cow::as_ref),
		};

		value.try_apply(
			self.material
				.properties
				.get(property.key.as_ref())
				.ok_or_else(|| GenericMaterialError::NoProperty(property.key.to_string()))?
				.generic_deserialize(registration, &registry, &mut processor)
				.map_err(GenericMaterialError::Deserialize)?
				.as_ref(),
		)?;

		Ok(value)
	}

	/// Gets the property or default.
	pub fn property<T: PartialReflect>(&self, property: MaterialProperty<T>) -> T {
		let default = property.default;
		self.get_property(property).ok().unwrap_or_else(default)
	}
}

#[derive(SystemParam)]
pub struct GenericMaterials<'w> {
	type_registry: Res<'w, AppTypeRegistry>,
	asset_server: Res<'w, AssetServer>,
	pub assets: Res<'w, Assets<GenericMaterial>>,
}
impl GenericMaterials<'_> {
	pub fn get(&self, id: impl Into<AssetId<GenericMaterial>>) -> Option<GenericMaterialView> {
		let id = id.into();
		let material = self.assets.get(id)?;
		let path = self.asset_server.get_path(id).map(AssetPath::into_owned).map(Cow::Owned);

		Some(GenericMaterialView {
			material,
			id,
			path,
			asset_server: &self.asset_server,
			type_registry: &self.type_registry,
		})
	}

	pub fn iter(&self) -> impl Iterator<Item = GenericMaterialView> {
		// self.asset_server.get_path(id)
		self.assets.iter().map(|(id, material)| GenericMaterialView {
			material,
			id,
			path: self.asset_server.get_path(id).map(AssetPath::into_owned).map(Cow::Owned),
			asset_server: &self.asset_server,
			type_registry: &self.type_registry,
		})
	}
}

/// User-defined property about a material. These are stored in the [`GenericMaterial`] namespace, so custom properties should be created via an extension trait.
///
/// To be used with [`GenericMaterialView::property`] or [`GenericMaterialView::get_property`].
///
/// # Examples
/// ```
/// use bevy_materialize::prelude::*;
///
/// pub trait MyMaterialPropertiesExt {
///     const MY_PROPERTY: MaterialProperty<f32> = MaterialProperty::new("my_property", || 5.);
/// }
/// impl MyMaterialPropertiesExt for GenericMaterial {}
///
/// // Then we can get property like so
/// let _ = GenericMaterial::MY_PROPERTY;
/// ```
pub struct MaterialProperty<T> {
	pub key: Cow<'static, str>,
	pub default: fn() -> T,
}

impl<T: PartialReflect> MaterialProperty<T> {
	pub const fn new(key: &'static str, default: fn() -> T) -> Self {
		Self {
			key: Cow::Borrowed(key),
			default,
		}
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
	fn clone_into_erased(&self) -> Box<dyn ErasedMaterialHandle>;
	fn insert(&self, entity: EntityWorldMut);
	fn remove(&self, entity: EntityWorldMut);
	fn to_untyped_handle(&self) -> UntypedHandle;
	fn id(&self) -> UntypedAssetId;

	#[allow(clippy::type_complexity)]
	fn modify_with_commands(&self, commands: &mut Commands, modifier: Box<dyn FnOnce(Option<&mut dyn Reflect>) + Send + Sync>);
}
#[cfg(feature = "bevy_pbr")]
impl<M: Material + Reflect> ErasedMaterialHandle for Handle<M> {
	// A lot of cloning here! Fun!
	fn clone_into_erased(&self) -> Box<dyn ErasedMaterialHandle> {
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
		self.clone_into_erased()
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

	#[error("in field {0} - {1}")]
	InField(String, Box<Self>),

	#[error("in super-material {0} - {1}")]
	InSuperMaterial(String, Box<Self>),
}
