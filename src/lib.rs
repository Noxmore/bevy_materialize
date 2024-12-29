use core::str;
use std::{
    any::{type_name, Any, TypeId},
    borrow::Cow,
    error::Error,
    fmt::{self},
    io,
    sync::{Arc, RwLock},
};

use bevy::{
    asset::{AssetLoader, AsyncReadExt, LoadContext, UntypedAssetId},
    prelude::*,
    reflect::{
        serde::{ReflectDeserializer, TypedReflectDeserializer},
        ApplyError, DynamicStruct, DynamicTypePath, FromType, TypeRegistration, TypeRegistry,
    },
    utils::HashMap,
};
use de::GenericMaterialDeserializationProcessor;
use serde::{
    de::{DeserializeOwned, DeserializeSeed},
    Deserialize,
};
use thiserror::Error;

pub mod de;
pub mod prelude;

#[derive(Default)]
pub struct MaterializePlugin<D: MaterialDeserializer> {
    pub deserializer: D,
}
impl<D: MaterialDeserializer> Plugin for MaterializePlugin<D> {
    fn build(&self, app: &mut App) {
        app.init_asset::<GenericMaterial>()
            .init_asset_loader::<GenericMaterialLoader>()
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
        Self { deserializer }
    }

    pub fn insert_generic_materials(
        mut commands: Commands,
        query: Query<(Entity, &GenericMaterialHolder), Without<GenericMaterialApplied>>,
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
        query: Query<(Entity, &GenericMaterialHolder), With<GenericMaterialApplied>>,
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
        mut query: Query<(&GenericMaterialHolder, &mut Visibility), Without<GenericMaterialApplied>>,
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

// pub enum DynamicValue {

// }

// TODO Better name
#[derive(Component, Reflect)]
pub struct GenericMaterialHolder(pub Handle<GenericMaterial>);

#[derive(Component, Reflect)]
pub struct GenericMaterialApplied;

#[derive(Asset, TypePath)]
pub struct GenericMaterial {
    pub material: Box<dyn ErasedMaterialHandle>,
    pub properties: HashMap<String, Box<dyn GenericValue>>,
    pub type_registry: AppTypeRegistry,
}
impl GenericMaterial {
    /// Whether the surface should render in the world.
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

/// Information about an expected field from [MaterialProperties]. Built-in properties are stored in the [MaterialProperties] namespace, such as [MaterialProperties::RENDER].
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

/* #[derive(Resource, Debug, Clone, Default)]
pub struct MaterialPropertyRegistry {
    inner: Arc<RwLock<HashMap<String, MaterialPropertyRegistration>>>,
} */

#[derive(Debug, Clone)]
pub struct MaterialPropertyRegistration {
    pub default: fn() -> Box<dyn Reflect>,
    pub type_registration: &'static TypeRegistration, // TODO should be TypeInfo?
}

pub trait MaterialDeserializer: Send + Sync + 'static {
    type Value: GenericValue;
    type Error: serde::de::Error;

    fn read(&mut self, input: &[u8]) -> Result<Self::Value, Self::Error>;
}

pub struct TomlMaterialDeserializer;
impl MaterialDeserializer for TomlMaterialDeserializer {
    type Value = toml::Value;
    type Error = toml::de::Error;

    fn read(&mut self, input: &[u8]) -> Result<Self::Value, Self::Error> {
        use serde::de::Error;
        let s = str::from_utf8(input).map_err(toml::de::Error::custom)?;
        toml::from_str(s)
    }
}

pub trait GenericValue: Send + Sync {
    fn generic_deserialize(
        &self,
        registration: &TypeRegistration,
        registry: &TypeRegistry,
    ) -> Result<Box<dyn PartialReflect>, Box<dyn Error + Send + Sync>>;
}
impl<T: serde::de::Deserializer<'static, Error: Send + Sync> + Clone + Send + Sync + 'static> GenericValue for T {
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

pub struct GenericMaterialLoader {
    pub type_registry: AppTypeRegistry,
}
impl FromWorld for GenericMaterialLoader {
    fn from_world(world: &mut World) -> Self {
        Self {
            type_registry: world.resource::<AppTypeRegistry>().clone(),
        }
    }
}
impl AssetLoader for GenericMaterialLoader {
    type Asset = GenericMaterial;
    type Settings = ();
    type Error = GenericMaterialError;

    fn load(
        &self,
        reader: &mut dyn bevy::asset::io::Reader,
        _settings: &Self::Settings,
        load_context: &mut LoadContext,
    ) -> impl bevy::utils::ConditionalSendFuture<Output = Result<Self::Asset, Self::Error>> {
        Box::pin(async {
            #[derive(Deserialize)]
            struct ParsedGenericMaterial {
                material: toml::Table,
                properties: toml::Table,
            }

            let mut input_string = String::new();
            reader.read_to_string(&mut input_string).await?;

            let mut parsed: ParsedGenericMaterial = toml::from_str(&input_string).map_err(|err| GenericMaterialError::Deserialize(Box::new(err)))?;

            let type_name = parsed
                .material
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or(StandardMaterial::type_path());

            let registry = self.type_registry.read();

            let mut registration_candidates = Vec::new();

            for reg in registry.iter() {
                if reg.type_info().type_path() == type_name || reg.type_info().ty().ident() == Some(type_name) {
                    registration_candidates.push(reg);
                }
            }

            if registration_candidates.is_empty() {
                return Err(GenericMaterialError::MaterialTypeNotFound(type_name.to_string()));
            } else if registration_candidates.len() > 1 {
                return Err(GenericMaterialError::TooManyTypeCandidates(
                    type_name.to_string(),
                    registration_candidates
                        .into_iter()
                        .map(|reg| reg.type_info().type_path().to_string())
                        .collect(),
                ));
            }
            let reg = registration_candidates[0];

            parsed.material.remove("type");

            let mut mat = registry.get_type_data::<ReflectGenericMaterial>(reg.type_id()).expect("TODO").default();

            let mut processor = GenericMaterialDeserializationProcessor { load_context };
            let data = TypedReflectDeserializer::with_processor(reg, &registry, &mut processor)
                .deserialize(parsed.material)
                .map_err(|err| GenericMaterialError::Deserialize(Box::new(err)))?;

            mat.try_apply(data.as_ref())?;

            let mut properties: HashMap<String, Box<dyn GenericValue>> = HashMap::new();

            for (key, value) in parsed.properties {
                properties.insert(key, Box::new(value));
            }

            Ok(GenericMaterial {
                material: mat.add_labeled_asset(load_context, "Material".to_string()),
                properties,
                type_registry: self.type_registry.clone(),
            })
        })
    }

    fn extensions(&self) -> &[&str] {
        &["mat", "mat.toml", "material", "material.toml"]
    }
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
