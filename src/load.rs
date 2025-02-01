use std::any::TypeId;
use std::convert::Infallible;
use std::str;
use std::sync::Arc;

use ::serde;
use bevy::asset::{AssetLoader, AssetPath};
use bevy::image::ImageLoader;
use bevy::reflect::{serde::*, *};
use bevy::utils::HashMap;
use bevy::{asset::LoadContext, prelude::*};
use serde::de::DeserializeOwned;
use serde::Deserializer;
use serde::{de::DeserializeSeed, Deserialize};

use crate::{prelude::*, GenericMaterialError, GenericMaterialShorthands, GenericValue, ReflectGenericMaterial};

/// Main trait for file format implementation of generic materials. See [TomlMaterialDeserializer] and [JsonMaterialDeserializer] for built-in/example implementations.
pub trait MaterialDeserializer: Send + Sync + 'static {
	type Value: GenericValue + DeserializeOwned + Deserializer<'static, Error: Send + Sync>;
	type Error: serde::de::Error + Send + Sync;
	/// The asset loader's file extensions.
	const EXTENSIONS: &[&str];

	/// Deserializes raw bytes into a value.
	fn deserialize<T: DeserializeOwned>(&self, input: &[u8]) -> Result<T, Self::Error>;
}

#[cfg(feature = "toml")]
#[derive(Debug, Clone, Default)]
pub struct TomlMaterialDeserializer;
#[cfg(feature = "toml")]
impl MaterialDeserializer for TomlMaterialDeserializer {
	type Value = toml::Value;
	type Error = toml::de::Error;
	const EXTENSIONS: &[&str] = &["toml", "mat", "mat.toml", "material", "material.toml"];

	fn deserialize<T: DeserializeOwned>(&self, input: &[u8]) -> Result<T, Self::Error> {
		let s = str::from_utf8(input).map_err(serde::de::Error::custom)?;
		toml::from_str(s)
	}
}

#[cfg(feature = "json")]
#[derive(Debug, Clone, Default)]
pub struct JsonMaterialDeserializer;
#[cfg(feature = "json")]
impl MaterialDeserializer for JsonMaterialDeserializer {
	type Value = serde_json::Value;
	type Error = serde_json::Error;
	const EXTENSIONS: &[&str] = &["json", "mat", "mat.json", "material", "material.json"];

	fn deserialize<T: DeserializeOwned>(&self, input: &[u8]) -> Result<T, Self::Error> {
		let s = str::from_utf8(input).map_err(serde::de::Error::custom)?;
		serde_json::from_str(s)
	}
}

pub struct GenericMaterialLoader<D: MaterialDeserializer> {
	pub type_registry: AppTypeRegistry,
	pub shorthands: GenericMaterialShorthands,
	pub deserializer: Arc<D>,
}
impl<D: MaterialDeserializer> AssetLoader for GenericMaterialLoader<D> {
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
			struct ParsedGenericMaterial<Value: GenericValue> {
				#[serde(rename = "type")]
				ty: Option<String>,
				material: Option<Value>,
				properties: Option<HashMap<String, Value>>,
			}

			let mut input = Vec::new();
			reader.read_to_end(&mut input).await?;

			// let mut parsed: ParsedGenericMaterial<D::Value> = toml::from_str(&input_string).map_err(|err| GenericMaterialError::Deserialize(Box::new(err)))?;
			let parsed: ParsedGenericMaterial<D::Value> = self
				.deserializer
				.deserialize(&input)
				.map_err(|err| GenericMaterialError::Deserialize(Box::new(err)))?;

			let type_name = parsed.ty.as_deref().unwrap_or(StandardMaterial::type_path());

			let registry = self.type_registry.read();

			let mut registration_candidates = Vec::new();

			let shorthands = self.shorthands.values.read().unwrap();
			for (shorthand, reg) in shorthands.iter() {
				if type_name == shorthand {
					registration_candidates.push(reg);
				}
			}

			for reg in registry.iter() {
				if reg.type_info().type_path() == type_name || reg.type_info().type_path_table().short_path() == type_name {
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

			let Some(mut mat) = registry
				.get_type_data::<ReflectGenericMaterial>(reg.type_id())
				.map(ReflectGenericMaterial::default)
			else {
				panic!("{} isn't a registered generic material", reg.type_info().type_path());
			};

			if let Some(material) = parsed.material {
				let mut processor = GenericMaterialDeserializationProcessor::Loading(load_context);
				let data = TypedReflectDeserializer::with_processor(reg, &registry, &mut processor)
					.deserialize(material)
					.map_err(|err| GenericMaterialError::Deserialize(Box::new(err)))?;

				mat.try_apply(data.as_ref())?;
			}

			let mut properties: HashMap<String, Box<dyn GenericValue>> = HashMap::new();

			if let Some(parsed_properties) = parsed.properties {
				for (key, value) in parsed_properties {
					properties.insert(key, Box::new(value));
				}
			}

			Ok(GenericMaterial {
				handle: mat.add_labeled_asset(load_context, "Material".to_string()),
				properties,
			})
		})
	}

	fn extensions(&self) -> &[&str] {
		D::EXTENSIONS
	}
}

pub enum GenericMaterialDeserializationProcessor<'w, 'l> {
	Loading(&'l mut LoadContext<'w>),
	Loaded {
		asset_server: &'w AssetServer,
		path: Option<&'l AssetPath<'static>>,
	},
}
impl GenericMaterialDeserializationProcessor<'_, '_> {
	pub fn asset_path(&self) -> Option<&AssetPath<'static>> {
		match self {
			Self::Loading(load_context) => Some(load_context.asset_path()),
			Self::Loaded { asset_server: _, path } => *path,
		}
	}

	pub fn load<'b, A: Asset>(&mut self, path: impl Into<AssetPath<'b>>) -> Handle<A> {
		match self {
			Self::Loading(load_context) => load_context.load(path),
			Self::Loaded { asset_server, path: _ } => asset_server.load(path),
		}
	}
}
impl ReflectDeserializerProcessor for GenericMaterialDeserializationProcessor<'_, '_> {
	fn try_deserialize<'de, D: serde::Deserializer<'de>>(
		&mut self,
		registration: &TypeRegistration,
		_registry: &TypeRegistry,
		deserializer: D,
	) -> Result<Result<Box<dyn PartialReflect>, D>, D::Error> {
		if let Some(asset_path) = self.asset_path() {
			// TODO good way to register loadable assets

			if registration.type_id() == TypeId::of::<Handle<Image>>() {
				let path = String::deserialize(deserializer)?;

				let parent_path = asset_path.parent().unwrap_or_default();
				let path = parent_path.resolve(&path).map_err(serde::de::Error::custom)?;
				let handle = self.load::<Image>(path);
				return Ok(Ok(Box::new(handle)));
			} else if registration.type_id() == TypeId::of::<Handle<GenericMaterial>>() {
				let path = String::deserialize(deserializer)?;

				let parent_path = asset_path.parent().unwrap_or_default();
				let path = parent_path.resolve(&path).map_err(serde::de::Error::custom)?;
				let handle = self.load::<GenericMaterial>(path);
				return Ok(Ok(Box::new(handle)));
			}
		}

		Ok(Err(deserializer))
	}
}

#[derive(Debug, Clone)]
pub struct SimpleGenericMaterialLoaderSettings {
	/// The `StandardMaterial` to use as a base when loading materials.
	pub material: StandardMaterial,
	pub properties: fn() -> HashMap<String, Box<dyn GenericValue>>,
}
impl Default for SimpleGenericMaterialLoaderSettings {
	fn default() -> Self {
		Self {
			material: StandardMaterial {
				perceptual_roughness: 1.,
				..default()
			},
			properties: HashMap::new,
		}
	}
}

/// Loads a [GenericMaterial] containing a [StandardMaterial] directly from an image file, putting said image into the `base_color_texture` field of the material.
pub struct SimpleGenericMaterialLoader {
	pub settings: SimpleGenericMaterialLoaderSettings,
}
impl AssetLoader for SimpleGenericMaterialLoader {
	type Asset = GenericMaterial;
	type Settings = ();
	type Error = Infallible;

	fn load(
		&self,
		_reader: &mut dyn bevy::asset::io::Reader,
		_settings: &Self::Settings,
		load_context: &mut LoadContext,
	) -> impl bevy::utils::ConditionalSendFuture<Output = Result<Self::Asset, Self::Error>> {
		Box::pin(async {
			let path = load_context.asset_path().clone();

			let material = StandardMaterial {
				base_color_texture: Some(load_context.load(path)),
				..self.settings.material.clone()
			};

			Ok(GenericMaterial {
				handle: load_context.add_labeled_asset("Material".to_string(), material).into(),
				properties: (self.settings.properties)(),
			})
		})
	}

	fn extensions(&self) -> &[&str] {
		ImageLoader::SUPPORTED_FILE_EXTENSIONS
	}
}
