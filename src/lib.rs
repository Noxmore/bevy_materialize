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
    asset::{AssetPath, LoadContext, UntypedAssetId},
    ecs::{component::ComponentId, system::SystemParam, world::DeferredWorld},
    prelude::*,
    reflect::{serde::TypedReflectDeserializer, ApplyError, FromType, GetTypeRegistration, ReflectMut, TypeRegistration, TypeRegistry, Typed},
    utils::HashMap,
};
use load::{
    GenericMaterialDeserializationProcessor, GenericMaterialLoader, MaterialDeserializer, SimpleGenericMaterialLoader,
    SimpleGenericMaterialLoaderSettings,
};
use serde::{de::DeserializeSeed, Deserializer};
use thiserror::Error;

pub mod animation;
pub mod load;
pub mod prelude;

pub struct MaterializePlugin<D: MaterialDeserializer> {
    pub deserializer: Arc<D>,
    /// If `None`, doesn't register [SimpleGenericMaterialLoader].
    pub simple_loader_settings: Option<SimpleGenericMaterialLoaderSettings>,
}
impl<D: MaterialDeserializer> Plugin for MaterializePlugin<D> {
    fn build(&self, app: &mut App) {
        let type_registry = app.world().resource::<AppTypeRegistry>().clone();

        if let Some(settings) = &self.simple_loader_settings {
            app.register_asset_loader(SimpleGenericMaterialLoader { settings: settings.clone() });
        }

        app.add_plugins((MaterializeMarkerPlugin, animation::AnimationPlugin))
            .register_type::<GenericMaterial3d>()
            .init_asset::<GenericMaterial>()
            .register_asset_loader(GenericMaterialLoader {
                type_registry,
                deserializer: self.deserializer.clone(),
            })
            .register_generic_material::<StandardMaterial>()
            .add_systems(PreUpdate, reload_generic_materials)
            .add_systems(
                PostUpdate,
                (visibility_material_property.before(insert_generic_materials), insert_generic_materials),
            );
    }
}
impl<D: MaterialDeserializer> MaterializePlugin<D> {
    pub fn new(deserializer: D) -> Self {
        Self {
            deserializer: Arc::new(deserializer),
            simple_loader_settings: Some(default()),
        }
    }

    /// If `None`, doesn't register [SimpleGenericMaterialLoader].
    pub fn with_simple_loader_settings(mut self, settings: Option<SimpleGenericMaterialLoaderSettings>) -> Self {
        self.simple_loader_settings = settings;
        self
    }
}
impl<D: MaterialDeserializer + Default> Default for MaterializePlugin<D> {
    fn default() -> Self {
        Self {
            deserializer: Arc::new(D::default()),
            simple_loader_settings: Some(default()),
        }
    }
}

/// Added when a [MaterializePlugin] is added. Can be used to check if any [MaterializePlugin] has been added.
pub struct MaterializeMarkerPlugin;
impl Plugin for MaterializeMarkerPlugin {
    fn build(&self, _app: &mut App) {}
}

// Can't have these in a [MaterializePlugin] impl because of the generic.
// ////////////////////////////////////////////////////////////////////////////////
// // SYSTEMS
// ////////////////////////////////////////////////////////////////////////////////

pub fn insert_generic_materials(
    mut commands: Commands,
    query: Query<(Entity, &GenericMaterial3d), Without<GenericMaterialApplied>>,
    generic_materials: Res<Assets<GenericMaterial>>,
) {
    for (entity, holder) in &query {
        let Some(generic_material) = generic_materials.get(&holder.0) else { continue };

        let material = generic_material.handle.clone();
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
    generic_materials: GenericMaterials,
) {
    for (generic_material_holder, mut visibility) in &mut query {
        let Some(generic_material) = generic_materials.get(&generic_material_holder.0) else { continue };
        let Ok(new_visibility) = generic_material.get_property(GenericMaterial::VISIBILITY) else { continue };

        *visibility = new_visibility;
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
#[derive(Component, Reflect, Debug, Clone, PartialEq, Eq, Default, Deref, DerefMut)]
#[component(on_replace = Self::on_replace)]
#[reflect(Component, Default)]
pub struct GenericMaterial3d(pub Handle<GenericMaterial>);
impl GenericMaterial3d {
    fn on_replace(mut world: DeferredWorld, entity: Entity, _id: ComponentId) {
        let generic_material_handle = &world.entity(entity).get::<Self>().unwrap().0;
        let Some(generic_material) = world.resource::<Assets<GenericMaterial>>().get(generic_material_handle) else { return };
        let material_handle = generic_material.handle.clone();

        world.commands().queue(move |world: &mut World| {
            let Ok(mut entity) = world.get_entity_mut(entity) else { return };

            entity.remove::<GenericMaterialApplied>();
            material_handle.remove(entity);
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
    pub handle: Box<dyn ErasedMaterialHandle>,
    // This could be better stored as a dyn PartialReflect with types like DynamicStruct,
    // but as far as i can tell Bevy's deserialization infrastructure does not support that
    pub properties: HashMap<String, Box<dyn GenericValue>>,
}
impl GenericMaterial {
    /// Property that changes the visibility component of applied entities to this value.
    pub const VISIBILITY: MaterialProperty<Visibility> = MaterialProperty::new("visibility", || Visibility::Inherited);

    pub fn new(handle: impl Into<Box<dyn ErasedMaterialHandle>>) -> Self {
        Self {
            handle: handle.into(),
            properties: HashMap::new(),
        }
    }

    /// Sets a property to a [DirectGenericValue] containing `value`.
    pub fn set_property<T: PartialReflect + fmt::Debug + Clone + Send + Sync>(&mut self, property: MaterialProperty<T>, value: T) {
        self.properties.insert(property.key.to_string(), Box::new(DirectGenericValue(value)));
    }
}

/// Contains all necessary information to parse properties of a [GenericMaterial].
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

/// User-defined property about a material. These are stored in the [GenericMaterial] namespace, so custom properties should be created via an extension trait.
///
/// To be used with [GenericMaterialView::property] or [GenericMaterialView::get_property].
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

/// Thin wrapper type implementing [GenericValue]. Used for directly passing values to properties.
/// Usually you should use [GenericMaterial::set_property], which uses this under the hood.
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

    #[allow(clippy::type_complexity)]
    fn modify_with_commands(&self, commands: &mut Commands, modifier: Box<dyn FnOnce(Option<&mut dyn Reflect>) + Send + Sync>);
}
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
impl<M: Material + Reflect> From<Handle<M>> for Box<dyn ErasedMaterialHandle> {
    fn from(value: Handle<M>) -> Self {
        Box::new(value)
    }
}
impl Clone for Box<dyn ErasedMaterialHandle> {
    fn clone(&self) -> Self {
        self.clone_into_erased()
    }
}

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

#[test]
fn direct_values() {
    App::new()
        .register_type::<StandardMaterial>()
        .register_type::<Visibility>()
        .add_plugins((AssetPlugin::default(), MaterializePlugin::new(crate::load::TomlMaterialDeserializer)))
        .add_systems(Startup, setup)
        .add_systems(PostStartup, test)
        .run();

    fn setup(mut assets: ResMut<Assets<GenericMaterial>>) {
        let mut material = GenericMaterial {
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
