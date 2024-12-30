#![doc = include_str!("../readme.md")]

use core::str;
use std::{
    any::{type_name, Any, TypeId},
    borrow::Cow,
    error::Error,
    fmt::{self},
    io,
    sync::Arc,
};

use bevy::{
    asset::{LoadContext, UntypedAssetId},
    prelude::*,
    reflect::{serde::TypedReflectDeserializer, ApplyError, FromType, TypeRegistration, TypeRegistry},
    utils::HashMap,
};
use load::{GenericMaterialLoader, MaterialDeserializer};
use serde::{de::DeserializeSeed, Deserializer};
use thiserror::Error;

pub mod load;
pub mod prelude;

#[derive(Default)]
pub struct MaterializePlugin<D: MaterialDeserializer> {
    pub deserializer: Arc<D>,
}
impl<D: MaterialDeserializer> Plugin for MaterializePlugin<D> {
    fn build(&self, app: &mut App) {
        let type_registry = app.world().resource::<AppTypeRegistry>().clone();

        app.register_type::<GenericMaterial3d>()
            .init_asset::<GenericMaterial>()
            .register_asset_loader(GenericMaterialLoader {
                type_registry,
                deserializer: self.deserializer.clone(),
            })
            .register_generic_material::<StandardMaterial>()
            .add_systems(
                PreUpdate,
                (
                    Self::insert_generic_materials,
                    Self::visibility_material_property.before(Self::insert_generic_materials),
                ),
            )
            .add_systems(PostUpdate, Self::reload_generic_materials);
    }
}
impl<D: MaterialDeserializer> MaterializePlugin<D> {
    pub fn new(deserializer: D) -> Self {
        Self {
            deserializer: Arc::new(deserializer),
        }
    }

    pub fn insert_generic_materials(
        mut commands: Commands,
        query: Query<(Entity, &GenericMaterial3d), Without<GenericMaterialApplied>>,
        generic_materials: Res<Assets<GenericMaterial>>,
    ) {
        for (entity, holder) in &query {
            let Some(generic_material) = generic_materials.get(&holder.0) else { continue };

            let material = generic_material.material.clone();
            commands
                .entity(entity)
                .queue(move |entity: EntityWorldMut<'_>| material.insert(entity))
                .insert(GenericMaterialApplied);
        }
    }

    pub fn reload_generic_materials(
        mut commands: Commands,
        mut asset_events: EventReader<AssetEvent<GenericMaterial>>,
        query: Query<(Entity, &GenericMaterial3d), With<GenericMaterialApplied>>,
    ) {
        for event in asset_events.read() {
            let AssetEvent::Modified { id } = event else { continue };

            for (entity, holder) in &query {
                if *id == holder.0.id() {
                    commands.entity(entity).remove::<GenericMaterialApplied>();
                }
            }
        }
    }

    pub fn visibility_material_property(
        mut query: Query<(&GenericMaterial3d, &mut Visibility), Without<GenericMaterialApplied>>,
        generic_materials: Res<Assets<GenericMaterial>>,
    ) {
        for (generic_material_holder, mut visibility) in &mut query {
            let Some(generic_material) = generic_materials.get(&generic_material_holder.0) else { continue };
            let Ok(new_visibility) = generic_material.get_property(GenericMaterial::VISIBILITY) else { continue };

            *visibility = new_visibility;
        }
    }
}

pub trait MaterializeAppExt {
    /// Register a foreign material to be able to be created via [GenericMaterial].
    ///
    /// If you own the type, you can use `#[reflect(GenericMaterial)]` to automatically register it.
    fn register_generic_material<M: Material + Reflect + Struct + Default>(&mut self) -> &mut Self;
}
impl MaterializeAppExt for App {
    fn register_generic_material<M: Material + Reflect + Struct + Default>(&mut self) -> &mut Self {
        self.register_type_data::<M, ReflectGenericMaterial>()
    }
}

/// Generic version of [MeshMaterial3d]. Stores a handle to a [GenericMaterial].
///
/// When on an entity, this automatically inserts the appropriate [MeshMaterial3d].
///
/// When removing or replacing this component, the inserted [MeshMaterial3d] will be removed.
#[derive(Reflect, Debug, Clone, PartialEq, Eq, Default, Deref, DerefMut)]
#[reflect(Component, Default)]
pub struct GenericMaterial3d(pub Handle<GenericMaterial>);
impl Component for GenericMaterial3d {
    const STORAGE_TYPE: bevy::ecs::component::StorageType = bevy::ecs::component::StorageType::Table;

    fn register_component_hooks(hooks: &mut bevy::ecs::component::ComponentHooks) {
        hooks.on_replace(|mut world, entity, _| {
            let generic_material_handle = &world.entity(entity).get::<Self>().unwrap().0;
            let Some(generic_material) = world.resource::<Assets<GenericMaterial>>().get(generic_material_handle) else { return };
            let material_handle = generic_material.material.clone();

            world.commands().queue(move |world: &mut World| {
                let Ok(mut entity) = world.get_entity_mut(entity) else { return };

                entity.remove::<GenericMaterialApplied>();
                material_handle.remove(entity);
            });
        });
    }
}

/// Automatically put on entities when their [GenericMaterial3d] inserts [MeshMaterial3d].
/// This is required because [MeshMaterial3d] is generic, and as such can't be used in query parameters for generic materials.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct GenericMaterialApplied;

/// Material asset containing a type-erased material handle, and generic user-defined properties.
#[derive(Asset, TypePath, Debug)]
pub struct GenericMaterial {
    pub material: Box<dyn ErasedMaterialHandle>,
    // This could be better stored as a dyn PartialReflect with types like DynamicStruct,
    // but as far as i can tell Bevy's deserialization infrastructure does not support that
    pub properties: HashMap<String, Box<dyn GenericValue>>,
    pub type_registry: AppTypeRegistry,
}
impl GenericMaterial {
    /// Property that changes the visibility component of applied entities to this value.
    pub const VISIBILITY: MaterialProperty<Visibility> = MaterialProperty::new("visibility", || Visibility::Inherited);

    pub fn get_property<T: PartialReflect>(&self, property: MaterialProperty<T>) -> Result<T, GenericMaterialError> {
        let mut value = (property.default)();
        let registry = self.type_registry.read();
        let registration = registry
            .get(TypeId::of::<T>())
            .ok_or(GenericMaterialError::TypeNotRegistered(type_name::<T>()))?;

        value.try_apply(
            self.properties
                .get(property.key.as_ref())
                .ok_or_else(|| GenericMaterialError::NoProperty(property.key.to_string()))?
                .generic_deserialize(registration, &registry)
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

/// User-defined property about a material. These are stored in the [GenericMaterial] namespace, so custom properties should be created via an extension trait.
///
/// To be used with [GenericMaterial::property] or [GenericMaterial::get_property].
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

/// Trait meant for `Value` types of different serialization libraries. For example, for the [toml](::toml) crate, this is implemented for [toml::Value](::toml::Value).
///
/// This is for storing general non type specific data for deserializing on demand, such as in [GenericMaterial] properties.
///
/// NOTE: Because of the limitation of not being able to implement foreign traits for foreign types, this is automatically implemented for applicable types implementing the [Deserializer](serde::de::Deserializer) trait.
pub trait GenericValue: fmt::Debug + Send + Sync {
    fn generic_deserialize(
        &self,
        registration: &TypeRegistration,
        registry: &TypeRegistry,
    ) -> Result<Box<dyn PartialReflect>, Box<dyn Error + Send + Sync>>;
}
impl<T: Deserializer<'static, Error: Send + Sync> + fmt::Debug + Clone + Send + Sync + 'static> GenericValue for T {
    fn generic_deserialize(
        &self,
        registration: &TypeRegistration,
        registry: &TypeRegistry,
    ) -> Result<Box<dyn PartialReflect>, Box<dyn Error + Send + Sync>> {
        Ok(TypedReflectDeserializer::new(registration, registry).deserialize(self.clone())?)
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
}

/// Version of [ReflectDefault] that returns `Box<dyn ErasedMaterial>` instead of Box<dyn Reflect>.
#[derive(Clone)]
pub struct ReflectGenericMaterial {
    default: fn() -> Box<dyn ErasedMaterial>,
}
impl ReflectGenericMaterial {
    pub fn default(&self) -> Box<dyn ErasedMaterial> {
        (self.default)()
    }
}

impl<T: ErasedMaterial + Default> FromType<T> for ReflectGenericMaterial {
    fn from_type() -> Self {
        Self {
            default: || Box::<T>::default(),
        }
    }
}

pub trait ErasedMaterial: Send + Sync + Reflect + Struct {
    // TODO Can't use just `self` because i can't move trait objects.
    fn add_labeled_asset(&self, load_context: &mut LoadContext, label: String) -> Box<dyn ErasedMaterialHandle>;
    fn add_asset(&self, asset_server: &AssetServer) -> Box<dyn ErasedMaterialHandle>;
}
impl<M: Material + Reflect + Struct + Clone> ErasedMaterial for M {
    fn add_labeled_asset(&self, load_context: &mut LoadContext, label: String) -> Box<dyn ErasedMaterialHandle> {
        load_context.add_labeled_asset(label, self.clone()).into()
    }

    fn add_asset(&self, asset_server: &AssetServer) -> Box<dyn ErasedMaterialHandle> {
        asset_server.add(self.clone()).into()
    }
}
impl<M: Material + Reflect + Struct + Clone> From<M> for Box<dyn ErasedMaterial> {
    fn from(value: M) -> Self {
        Box::new(value)
    }
}

pub trait ErasedMaterialHandle: Send + Sync + fmt::Debug + Any {
    fn clone_into_erased(&self) -> Box<dyn ErasedMaterialHandle>;
    fn insert(&self, entity: EntityWorldMut);
    fn remove(&self, entity: EntityWorldMut);
    fn to_untyped_handle(&self) -> UntypedHandle;
    fn id(&self) -> UntypedAssetId;
}
impl<M: Material> ErasedMaterialHandle for Handle<M> {
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
}
impl<M: Material> From<Handle<M>> for Box<dyn ErasedMaterialHandle> {
    fn from(value: Handle<M>) -> Self {
        Box::new(value)
    }
}
impl Clone for Box<dyn ErasedMaterialHandle> {
    fn clone(&self) -> Self {
        self.clone_into_erased()
    }
}
