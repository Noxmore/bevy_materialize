pub mod deserializer;
pub mod inheritance;
pub mod processor;
pub mod simple;

use std::any::TypeId;
use std::ffi::OsStr;
use std::str;
use std::sync::Arc;

use ::serde;
use bevy::asset::io::AssetSourceId;
use bevy::asset::{AssetLoader, AssetPath, ParseAssetPathError};
#[cfg(feature = "bevy_image")]
use bevy::image::ImageLoaderSettings;
use bevy::platform::collections::HashMap;
use bevy::reflect::{serde::*, *};
use bevy::tasks::ConditionalSendFuture;
use bevy::{asset::LoadContext, prelude::*};
use inheritance::apply_inheritance;
use processor::{MaterialDeserializerProcessor, MaterialProcessorContext, MaterialSubProcessor};
use serde::Deserialize;

use crate::generic_material::MaterialPropertyRegistry;
use crate::{prelude::*, value::GenericValue, GenericMaterialShorthands};

#[cfg(feature = "bevy_pbr")]
use crate::{generic_material::ErasedMaterial, generic_material::ReflectGenericMaterial};
use serde::de::DeserializeSeed;

pub struct GenericMaterialLoader<D: MaterialDeserializer, P: MaterialSubProcessor> {
	pub type_registry: AppTypeRegistry,
	pub shorthands: GenericMaterialShorthands,
	pub property_registry: MaterialPropertyRegistry,
	pub deserializer: Arc<D>,
	pub do_text_replacements: bool,
	pub processor: P,
}
impl<D: MaterialDeserializer, P: MaterialSubProcessor> GenericMaterialLoader<D, P> {
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
impl<D: MaterialDeserializer, P: MaterialSubProcessor> AssetLoader for GenericMaterialLoader<D, P> {
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
			let mut input = Vec::new();
			reader.read_to_end(&mut input).await?;

			if self.do_text_replacements {
				input = self.try_apply_replacements(load_context, input);
			}

			let parsed: ParsedGenericMaterial<D::Value> = self
				.deserializer
				.deserialize(&input)
				.map_err(|err| GenericMaterialError::Deserialize(Box::new(err)))?;

			let parsed = apply_inheritance(self, load_context, parsed).await?;

			assert!(parsed.inherits.is_none());

			#[cfg(feature = "bevy_pbr")]
			let mat = {
				let type_name = parsed.ty.as_deref().unwrap_or(StandardMaterial::type_path());

				let type_registry = self.type_registry.read();

				let mut registration_candidates = Vec::new();

				let shorthands = self.shorthands.values.read().unwrap();
				for (shorthand, reg) in shorthands.iter() {
					if type_name == shorthand {
						registration_candidates.push(reg);
					}
				}

				for reg in type_registry.iter() {
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
				let registration = registration_candidates[0];

				let Some(mut mat) = type_registry
					.get_type_data::<ReflectGenericMaterial>(registration.type_id())
					.map(ReflectGenericMaterial::default)
				else {
					panic!("{} isn't a registered generic material", registration.type_info().type_path());
				};

				if let Some(material) = parsed.material {
					let mut processor = MaterialDeserializerProcessor {
						ctx: MaterialProcessorContext {
							load_context,
							image_settings: settings.clone(),
						},
						sub_processor: &self.processor,
					};

					let data = TypedReflectDeserializer::with_processor(registration, &type_registry, &mut processor)
						.deserialize(material)
						.map_err(|err| GenericMaterialError::Deserialize(Box::new(err)))?;

					mat.try_apply(data.as_ref())?;
				}

				mat
			};

			let mut properties: HashMap<String, Box<dyn Reflect>> = HashMap::default();

			if let Some(parsed_properties) = parsed.properties {
				let type_registry = self.type_registry.read();
				let property_registry = self.property_registry.inner.read().unwrap();

				let mut processor = MaterialDeserializerProcessor {
					ctx: MaterialProcessorContext {
						load_context,
						#[cfg(feature = "bevy_image")]
						image_settings: settings.clone(),
					},
					sub_processor: &self.processor,
				};

				for (key, value) in parsed_properties {
					let Some(type_id) = property_registry.get(&key).copied() else {
						return Err(GenericMaterialError::PropertyNotRegistered(key));
					};
					let Some(registration) = type_registry.get(type_id) else {
						return Err(GenericMaterialError::PropertyTypeNotRegistered(key));
					};
					let Some(from_reflect) = registration.data::<ReflectFromReflect>() else {
						return Err(GenericMaterialError::NoFromReflect(registration.type_info().type_path()));
					};

					let partial_data = TypedReflectDeserializer::with_processor(registration, &type_registry, &mut processor)
						.deserialize(value)
						.map_err(|err| GenericMaterialError::Deserialize(Box::new(err)))?;

					let Some(data) = from_reflect.from_reflect(&*partial_data) else {
						return Err(GenericMaterialError::FullReflect {
							ty: partial_data.get_represented_type_info(),
						});
					};

					properties.insert(key, data);
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

#[derive(Deserialize)]
struct ParsedGenericMaterial<Value: GenericValue> {
	inherits: Option<String>,
	#[cfg(feature = "bevy_pbr")]
	#[serde(rename = "type")]
	ty: Option<String>,
	#[cfg(feature = "bevy_pbr")]
	material: Option<Value>,
	properties: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone)]
pub struct ReflectGenericMaterialLoad {
	pub load: fn(&mut MaterialProcessorContext, AssetPath<'static>) -> Box<dyn PartialReflect>,
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
			load: |processor, path| Box::new(processor.load_context.load::<A>(path)),
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

#[cfg(feature = "bevy_image")]
pub fn set_image_loader_settings(settings: &ImageLoaderSettings) -> impl Fn(&mut ImageLoaderSettings) {
	let settings = settings.clone();
	move |s| *s = settings.clone()
}

/// Produces an asset path relative to another for use in generic material loading.
///
/// # Examples
/// ```
/// # use bevy_materialize::load::relative_asset_path;
/// assert_eq!(relative_asset_path(&"materials/foo.toml".into(), "foo.png").unwrap(), "materials/foo.png".into());
/// assert_eq!(relative_asset_path(&"materials/foo.toml".into(), "textures/foo.png").unwrap(), "materials/textures/foo.png".into());
/// assert_eq!(relative_asset_path(&"materials/foo.toml".into(), "/textures/foo.png").unwrap(), "textures/foo.png".into());
/// assert_eq!(relative_asset_path(&"materials/foo.toml".into(), "\\textures\\foo.png").unwrap(), "textures\\foo.png".into());
/// ```
pub fn relative_asset_path(relative_to: &AssetPath<'static>, path: &str) -> Result<AssetPath<'static>, ParseAssetPathError> {
	let parent = relative_to.parent().unwrap_or_default();

	// Handle root
	let root_pattern = ['/', '\\'];

	if path.starts_with(root_pattern) {
		let mut asset_path = AssetPath::from(path.trim_start_matches(root_pattern)).into_owned();
		if let AssetSourceId::Default = asset_path.source() {
			asset_path = asset_path.with_source(relative_to.source().clone_owned());
		}

		Ok(asset_path)
	} else {
		parent.resolve(path)
	}
}

/// For unit tests.
#[doc(hidden)]
#[cfg(feature = "bevy_pbr")]
pub fn create_loading_test_app(deserializer: impl MaterialDeserializer) -> App {
	let mut app = App::new();

	app.add_plugins((
		MinimalPlugins,
		AssetPlugin::default(),
		ImagePlugin::default(),
		MaterializePlugin::new(deserializer),
	))
	.register_material_property_manual::<bool>("collision")
	.register_material_property_manual::<String>("sounds")
	.init_asset::<StandardMaterial>();

	app
}

#[test]
fn load_toml() {
	let app = create_loading_test_app(TomlMaterialDeserializer);
	let asset_server = app.world().resource::<AssetServer>();

	smol::block_on(async {
		asset_server.load_untyped_async("materials/animated.toml").await.unwrap();
		// Custom materials require special scaffolding in the associated example, and so the test is there.
		asset_server.load_untyped_async("materials/example.material.toml").await.unwrap();
		asset_server.load_untyped_async("materials/sub-material.toml").await.unwrap();
	});
}

#[cfg(feature = "json")]
#[test]
fn load_json() {
	let app = create_loading_test_app(JsonMaterialDeserializer);
	let asset_server = app.world().resource::<AssetServer>();

	smol::block_on(async {
		asset_server.load_untyped_async("materials/example.material.json").await.unwrap();
	});
}
