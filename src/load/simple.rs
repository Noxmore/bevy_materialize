#[cfg(feature = "bevy_pbr")]
use crate::erased_material::ErasedMaterial;
use bevy::asset::AssetLoader;
#[cfg(feature = "bevy_image")]
use bevy::image::ImageLoader;
use bevy::platform::collections::HashMap;
use bevy::tasks::ConditionalSendFuture;
use bevy::{asset::LoadContext, prelude::*};
use std::convert::Infallible;

use crate::generic_material::GenericMaterial;

/// Loads a [`GenericMaterial`] directly from an image file. By default it loads a [`StandardMaterial`], putting the image into its `base_color_texture` field, and setting `perceptual_roughness` set to 1.
#[derive(Debug, Clone)]
pub struct SimpleGenericMaterialLoader {
	/// A function that provides the underlying material given the loaded image. Default is a [`StandardMaterial`] with `perceptual_roughness` set to 1.
	#[cfg(feature = "bevy_pbr")]
	pub material: fn(Handle<Image>) -> Box<dyn ErasedMaterial>,
	pub properties: fn() -> HashMap<String, Box<dyn Reflect>>,
}
impl AssetLoader for SimpleGenericMaterialLoader {
	type Asset = GenericMaterial;
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
			let material = (self.material)(load_context.load(path));

			Ok(GenericMaterial {
				#[cfg(feature = "bevy_pbr")]
				handle: material.add_labeled_asset(load_context, "Material".to_string()),
				properties: (self.properties)(),
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

impl Default for SimpleGenericMaterialLoader {
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
