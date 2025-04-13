use std::any::TypeId;
use std::convert::Infallible;
use std::ffi::OsStr;
use std::{io, str};
use std::sync::Arc;

use ::serde;
use bevy::asset::{AssetLoader, AssetPath};
#[cfg(feature = "bevy_image")]
use bevy::image::{ImageLoader, ImageLoaderSettings};
use bevy::platform_support::collections::HashMap;
use bevy::reflect::{serde::*, *};
use bevy::tasks::ConditionalSendFuture;
use bevy::{asset::LoadContext, prelude::*};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Deserializer;

use crate::{prelude::*, GenericMaterialError, GenericMaterialShorthands, GenericValue};

#[cfg(feature = "bevy_pbr")]
use crate::{ErasedMaterial, ReflectGenericMaterial};
#[cfg(feature = "bevy_pbr")]
use serde::de::DeserializeSeed;

/// Main trait for file format implementation of generic materials. See [`TomlMaterialDeserializer`] and [`JsonMaterialDeserializer`] for built-in/example implementations.
pub trait MaterialDeserializer: Send + Sync + 'static {
	type Value: GenericValue + DeserializeOwned + Deserializer<'static, Error: Send + Sync>;
	type Error: serde::de::Error + Send + Sync;
	/// The asset loader's file extensions.
	const EXTENSIONS: &[&str];

	/// Deserializes raw bytes into a value.
	fn deserialize<T: DeserializeOwned>(&self, input: &[u8]) -> Result<T, Self::Error>;

	/// Merges a value in-place, used for inheritance.
	/// 
	/// Implementors should recursively merge maps, and overwrite everything else.
	fn merge_value(&self, value: &mut Self::Value, other: Self::Value);
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

	fn merge_value(&self, value: &mut Self::Value, other: Self::Value) {
		match (value, other) {
			(toml::Value::Table(value), toml::Value::Table(other)) => for (key, other_value) in other {
				match value.get_mut(&key) {
					Some(value) => self.merge_value(value, other_value),
					None => { value.insert(key, other_value); },
				}
			},
			(value, other) => *value = other,
		}
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

	fn merge_value(&self, value: &mut Self::Value, other: Self::Value) {
		match (value, other) {
			(serde_json::Value::Object(value), serde_json::Value::Object(other)) => for (key, other_value) in other {
				match value.get_mut(&key) {
					Some(value) => self.merge_value(value, other_value),
					None => { value.insert(key, other_value); },
				}
			},
			(value, other) => *value = other,
		}
	}
}

pub struct GenericMaterialLoader<D: MaterialDeserializer> {
	pub type_registry: AppTypeRegistry,
	pub shorthands: GenericMaterialShorthands,
	pub default_inherits: Option<String>,
	pub deserializer: Arc<D>,
}
impl<D: MaterialDeserializer> GenericMaterialLoader<D> {
	/// Attempts to apply string replacements to a text-based material file. Currently these are hardcoded, but i'd prefer if eventually they won't be.
	pub fn try_apply_replacements(&self, load_context: &LoadContext, bytes: Vec<u8>) -> Vec<u8> {
		let mut s = match String::from_utf8(bytes) {
			Ok(x) => x,
			Err(err) => return err.into_bytes(),
		};

		if let Some(file_name) = load_context.path().with_extension("").file_name().and_then(OsStr::to_str) {
			s = s.replace("${name}", file_name);
		}

		s.into_bytes()
	}
}
impl<D: MaterialDeserializer> AssetLoader for GenericMaterialLoader<D> {
	type Asset = GenericMaterial;
	#[cfg(feature = "bevy_image")]
	type Settings = ImageLoaderSettings;
	#[cfg(not(feature = "bevy_image"))]
	type Settings = ();
	type Error = GenericMaterialError;

	fn load(
		&self,
		reader: &mut dyn bevy::asset::io::Reader,
		#[allow(unused)] settings: &Self::Settings,
		#[allow(unused)] load_context: &mut LoadContext,
	) -> impl ConditionalSendFuture<Output = Result<Self::Asset, Self::Error>> {
		Box::pin(async {
			#[derive(Deserialize)]
			struct ParsedGenericMaterial<Value: GenericValue> {
				#[cfg(feature = "bevy_pbr")]
				#[serde(rename = "type")]
				ty: Option<String>,
				inherits: Option<String>,
				#[cfg(feature = "bevy_pbr")]
				material: Option<Value>,
				properties: Option<HashMap<String, Value>>,
			}

			let mut input = Vec::new();
			reader.read_to_end(&mut input).await?;

			input = self.try_apply_replacements(load_context, input);

			let parsed: ParsedGenericMaterial<D::Value> = self
				.deserializer
				.deserialize(&input)
				.map_err(|err| GenericMaterialError::Deserialize(Box::new(err)))?;


			async fn apply_inheritance<D: MaterialDeserializer>(
				loader: &GenericMaterialLoader<D>,
				load_context: &mut LoadContext<'_>,
				sub_material: ParsedGenericMaterial<D::Value>,
			) -> Result<ParsedGenericMaterial<D::Value>, GenericMaterialError> {
				// We do a queue-based solution because async functions can't recurse

				async fn read_path<D: MaterialDeserializer>(
					loader: &GenericMaterialLoader<D>,
					load_context: &mut LoadContext<'_>,
					path: impl Into<AssetPath<'_>>,
				) -> Result<ParsedGenericMaterial<D::Value>, GenericMaterialError> {
					let bytes = load_context.read_asset_bytes(path).await.map_err(io::Error::other)?;
					let bytes = loader.try_apply_replacements(load_context, bytes);

					loader
						.deserializer
						.deserialize(&bytes)
						.map_err(|err| GenericMaterialError::Deserialize(Box::new(err)))
				}

				let mut application_queue: Vec<ParsedGenericMaterial<D::Value>> = Vec::new();

				// Build the queue
				application_queue.push(sub_material);
				
				while let Some(inherits) = &application_queue.last().unwrap().inherits {
					let parent_path = load_context.asset_path().parent().unwrap_or_default();
					let path = parent_path.resolve(inherits).map_err(io::Error::other)?;

					application_queue.push(read_path(loader, load_context, path).await
						.map_err(|err| GenericMaterialError::InSuperMaterial(inherits.clone(), Box::new(err)))?);

					// current_sub_material = application_queue.last().unwrap();
				}

				if let Some(inherits) = &loader.default_inherits {
					application_queue.push(read_path(loader, load_context, inherits).await
						.map_err(|err| GenericMaterialError::InSuperMaterial(inherits.clone(), Box::new(err)))?);
				}

				// Apply the queue

				// We are guaranteed to have at least 1 element. This is the highest super-material.
				let mut final_material = application_queue.pop().unwrap();
				
				// This goes through the queue from highest super-material to the one we started at, and merges them in that order.
				while let Some(sub_material) = application_queue.pop() {
					match (&mut final_material.properties, sub_material.properties) {
						(Some(final_material_properties), Some(sub_properties)) => for (key, sub_value) in sub_properties {
							match final_material_properties.get_mut(&key) {
								Some(value) => loader.deserializer.merge_value(value, sub_value),
								None => { final_material_properties.insert(key, sub_value); },
							}
						}
						(None, Some(applicator_properties)) => final_material.properties = Some(applicator_properties),
						_ => {}
					}

					if sub_material.ty.is_some() {
						final_material.ty = sub_material.ty;
						final_material.material = sub_material.material;
					} else {
						match (&mut final_material.material, sub_material.material) {
							(Some(final_material_mat), Some(sub_material_mat)) => {
								loader.deserializer.merge_value(final_material_mat, sub_material_mat);
							}
							(None, Some(sub_material_mat)) => final_material.material = Some(sub_material_mat),
						_ => {}
						}
					}
				}

				Ok(final_material)
			}

			let parsed = apply_inheritance(self, load_context, parsed).await?;
			
			#[cfg(feature = "bevy_pbr")]
			let mat = {
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
					let mut processor = GenericMaterialDeserializationProcessor::Loading {
						load_context,
						image_settings: settings.clone(),
					};
					let data = TypedReflectDeserializer::with_processor(reg, &registry, &mut processor)
						.deserialize(material)
						.map_err(|err| GenericMaterialError::Deserialize(Box::new(err)))?;

					mat.try_apply(data.as_ref())?;
				}

				mat
			};

			let mut properties: HashMap<String, Box<dyn GenericValue>> = HashMap::default();

			if let Some(parsed_properties) = parsed.properties {
				for (key, value) in parsed_properties {
					properties.insert(key, Box::new(value));
				}
			}

			Ok(GenericMaterial {
				#[cfg(feature = "bevy_pbr")]
				handle: mat.add_labeled_asset(load_context, "Material".to_string()),
				properties,
			})
		})
	}

	fn extensions(&self) -> &[&str] {
		D::EXTENSIONS
	}
}

#[derive(Debug, Clone)]
pub struct ReflectGenericMaterialLoad {
	pub load: fn(&mut GenericMaterialDeserializationProcessor, AssetPath<'static>) -> Box<dyn PartialReflect>,
}

pub trait ReflectGenericMaterialLoadAppExt {
	/// Registers an asset to be able to be loaded within a [`GenericMaterial`].
	///
	/// Specifically, it allows loading of [`Handle<A>`] by simply providing a path relative to the material's directory.
	fn register_generic_material_sub_asset<A: Asset>(&mut self) -> &mut Self;

	/// Same as [`register_generic_material_sub_asset`](Self::register_generic_material_sub_asset), but passes image settings through.
	/// This will cause an error if the asset loader doesn't use image settings.
	fn register_generic_material_sub_asset_image_settings_passthrough<A: Asset>(&mut self) -> &mut Self;
}
impl ReflectGenericMaterialLoadAppExt for App {
	// Lot of duplicated code here
	#[track_caller]
	fn register_generic_material_sub_asset<A: Asset>(&mut self) -> &mut Self {
		let mut type_registry = self.main().world().resource::<AppTypeRegistry>().write();
		let registration = match type_registry.get_mut(TypeId::of::<Handle<A>>()) {
			Some(x) => x,
			None => panic!("Asset handle not registered: {}", std::any::type_name::<A>()),
		};

		registration.insert(ReflectGenericMaterialLoad {
			load: |processor, path| Box::new(processor.load::<A>(path)),
		});

		drop(type_registry);

		self
	}

	#[track_caller]
	fn register_generic_material_sub_asset_image_settings_passthrough<A: Asset>(&mut self) -> &mut Self {
		let mut type_registry = self.main().world().resource::<AppTypeRegistry>().write();
		let registration = match type_registry.get_mut(TypeId::of::<Handle<A>>()) {
			Some(x) => x,
			None => panic!("Asset handle not registered: {}", std::any::type_name::<A>()),
		};

		registration.insert(ReflectGenericMaterialLoad {
			load: |processor, path| Box::new(processor.load_with_image_settings::<A>(path)),
		});

		drop(type_registry);

		self
	}
}

pub enum GenericMaterialDeserializationProcessor<'w, 'l> {
	Loading {
		#[cfg(feature = "bevy_image")]
		image_settings: ImageLoaderSettings,
		load_context: &'l mut LoadContext<'w>,
	},
	Loaded {
		asset_server: &'w AssetServer,
		path: Option<&'l AssetPath<'static>>,
	},
}
impl GenericMaterialDeserializationProcessor<'_, '_> {
	pub fn asset_path(&self) -> Option<&AssetPath<'static>> {
		match self {
			Self::Loading { load_context, .. } => Some(load_context.asset_path()),
			Self::Loaded { path, .. } => *path,
		}
	}

	/// Same as [`load`](Self::load) but passes image load settings through.
	pub fn load_with_image_settings<'b, A: Asset>(&mut self, path: impl Into<AssetPath<'b>>) -> Handle<A> {
		match self {
			#[cfg(feature = "bevy_image")]
			Self::Loading {
				load_context,
				image_settings,
			} => load_context.loader().with_settings(set_image_loader_settings(image_settings)).load(path),
			#[cfg(not(feature = "bevy_image"))]
			Self::Loading { load_context } => load_context.load(path),

			Self::Loaded { asset_server, .. } => asset_server.load(path),
		}
	}

	pub fn load<'b, A: Asset>(&mut self, path: impl Into<AssetPath<'b>>) -> Handle<A> {
		match self {
			Self::Loading { load_context, .. } => load_context.load(path),
			Self::Loaded { asset_server, .. } => asset_server.load(path),
		}
	}
}
impl ReflectDeserializerProcessor for GenericMaterialDeserializationProcessor<'_, '_> {
	fn try_deserialize<'de, D: serde::Deserializer<'de>>(
		&mut self,
		#[allow(unused)] registration: &TypeRegistration,
		_registry: &TypeRegistry,
		deserializer: D,
	) -> Result<Result<Box<dyn PartialReflect>, D>, D::Error> {
		#[cfg(feature = "bevy_image")]
		if let Some(asset_path) = self.asset_path() {
			// TODO good way to register loadable assets

			if let Some(loader) = registration.data::<ReflectGenericMaterialLoad>() {
				let path = String::deserialize(deserializer)?;

				let parent_path = asset_path.parent().unwrap_or_default();
				let path = parent_path.resolve(&path).map_err(serde::de::Error::custom)?;

				return Ok(Ok((loader.load)(self, path)));
			}
		}

		Ok(Err(deserializer))
	}
}

#[derive(Debug, Clone)]
pub struct SimpleGenericMaterialLoaderSettings {
	/// A function that provides the underlying material given the loaded image. Default is a [`StandardMaterial`] with `perceptual_roughness` set to 1.
	#[cfg(feature = "bevy_pbr")]
	pub material: fn(Handle<Image>) -> Box<dyn ErasedMaterial>,
	pub properties: fn() -> HashMap<String, Box<dyn GenericValue>>,
}
impl Default for SimpleGenericMaterialLoaderSettings {
	fn default() -> Self {
		Self {
			#[cfg(feature = "bevy_pbr")]
			material: |image| {
				StandardMaterial {
					base_color_texture: Some(image),
					perceptual_roughness: 1.,
					..default()
				}
				.into()
			},
			properties: HashMap::default,
		}
	}
}

#[cfg(feature = "bevy_image")]
fn set_image_loader_settings(settings: &ImageLoaderSettings) -> impl Fn(&mut ImageLoaderSettings) {
	let settings = settings.clone();
	move |s| *s = settings.clone()
}

/// Loads a [`GenericMaterial`] directly from an image file. By default it loads a [`StandardMaterial`], putting the image into its `base_color_texture` field, and setting `perceptual_roughness` set to 1.
pub struct SimpleGenericMaterialLoader {
	pub settings: SimpleGenericMaterialLoaderSettings,
}
impl AssetLoader for SimpleGenericMaterialLoader {
	type Asset = GenericMaterial;
	#[cfg(feature = "bevy_image")]
	type Settings = ImageLoaderSettings;
	#[cfg(not(feature = "bevy_image"))]
	type Settings = ();
	type Error = Infallible;

	fn load(
		&self,
		_reader: &mut dyn bevy::asset::io::Reader,
		#[allow(unused)] settings: &Self::Settings,
		#[allow(unused)] load_context: &mut LoadContext,
	) -> impl ConditionalSendFuture<Output = Result<Self::Asset, Self::Error>> {
		Box::pin(async move {
			#[cfg(feature = "bevy_pbr")]
			let path = load_context.asset_path().clone();

			#[cfg(feature = "bevy_pbr")]
			let material = (self.settings.material)(load_context.loader().with_settings(set_image_loader_settings(settings)).load(path));

			Ok(GenericMaterial {
				#[cfg(feature = "bevy_pbr")]
				handle: material.add_labeled_asset(load_context, "Material".to_string()),
				properties: (self.settings.properties)(),
			})
		})
	}

	#[cfg(feature = "bevy_image")]
	fn extensions(&self) -> &[&str] {
		ImageLoader::SUPPORTED_FILE_EXTENSIONS
	}
	#[cfg(not(feature = "bevy_image"))]
	fn extensions(&self) -> &[&str] {
		// Since we aren't actually loading any images, let's just say we support them all.
		&[
			"basis", "bmp", "dds", "ff", "farbfeld", "gif", "exr", "hdr", "ico", "jpg", "jpeg", "ktx2", "pam", "pbm", "pgm", "ppm", "png", "qoi",
			"tga", "tif", "tiff", "webp",
		]
	}
}

#[cfg(test)]
fn create_loading_test_app() -> App {
	let mut app = App::new();

	app.add_plugins((
		MinimalPlugins,
		AssetPlugin::default(),
		ImagePlugin::default(),
		MaterializePlugin::new(TomlMaterialDeserializer),
	))
	.init_asset::<StandardMaterial>();

	app
}

#[test]
fn load_materials() {
	let app = create_loading_test_app();
	let asset_server = app.world().resource::<AssetServer>();

	smol::block_on(async {
		asset_server.load_untyped_async("materials/animated.toml").await.unwrap();
		// These require special scaffolding in the associated example.
		// asset_server.load_untyped_async("materials/custom_material.toml").await.unwrap();
		// asset_server.load_untyped_async("materials/extended_material.toml").await.unwrap();
		asset_server.load_untyped_async("materials/example.material.toml").await.unwrap();
		#[cfg(feature = "json")]
		asset_server.load_untyped_async("materials/example.material.json").await.unwrap();
		asset_server.load_untyped_async("materials/sub-material.toml").await.unwrap();
	});
}
