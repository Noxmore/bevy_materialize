use std::{any::Any, fmt, io};

use bevy::{
    asset::{AssetLoader, AsyncReadExt, LoadContext, UntypedAssetId},
    prelude::*,
    reflect::FromType,
};
use serde::Deserialize;
use thiserror::Error;

pub struct MaterializePlugin;
impl Plugin for MaterializePlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<GenericMaterial>()
            .init_asset_loader::<GenericMaterialLoader>()
            .register_generic_material::<StandardMaterial>()
            .add_systems(Update, Self::insert_generic_materials);
    }
}
impl MaterializePlugin {
    pub fn insert_generic_materials(
        mut commands: Commands,
        query: Query<(Entity, &GenericMaterialHolder), Without<GenericMaterialApplied>>,
        generic_materials: Res<Assets<GenericMaterial>>,
    ) {
        for (entity, generic_material_holder) in &query {
            let Some(generic_material) = generic_materials.get(&generic_material_holder.0) else { continue };

            let material = generic_material.material.clone();
            commands
                .entity(entity)
                .queue(move |entity: EntityWorldMut<'_>| material.insert(entity))
                .insert(GenericMaterialApplied);
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

// TODO Better name
#[derive(Component, Reflect)]
pub struct GenericMaterialHolder(pub Handle<GenericMaterial>);

#[derive(Component, Reflect)]
pub struct GenericMaterialApplied;

#[derive(Asset, TypePath)]
pub struct GenericMaterial {
    pub material: Box<dyn ErasedMaterialHandle>,
    pub properties: toml::Table,
}

#[derive(Error, Debug)]
pub enum GenericMaterialError {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("No registered material found for type {0}")]
    TypeNotFound(String),
    #[error("Too many type candidates found for `{0}`: {1:?}")]
    TooManyTypeCandidates(String, Vec<String>),
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

            let parsed: ParsedGenericMaterial = toml::from_str(&input_string)?;

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
                return Err(GenericMaterialError::TypeNotFound(type_name.to_string()));
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

            let mut mat = registry.get_type_data::<ReflectGenericMaterial>(reg.type_id()).expect("TODO").default();

            for field_idx in 0..mat.field_len() {
                let Some(field_name) = mat.name_at(field_idx) else { continue };
                let Some(value) = parsed.material.get(field_name) else { continue };
                // TODO should the latter error produce Err instead of panicking?
                let field = mat
                    .field_at_mut(field_idx)
                    .expect("Reflection lied!")
                    .try_as_reflect_mut()
                    .expect("Field not fully reflected");

                // TODO toml deserialize

                field.set(Box::new(1));
            }

            Ok(GenericMaterial {
                material: mat.add_labeled_asset(load_context, "Material".to_string()),
                properties: parsed.properties,
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
