use std::io;

use bevy::asset::{AssetPath, LoadContext};
use bevy::prelude::*;

use crate::{load::ParsedGenericMaterial, GenericMaterialError};

use super::deserializer::MaterialDeserializer;
use super::*;

pub(super) async fn apply_inheritance<D: MaterialDeserializer>(
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

		application_queue.push(
			read_path(loader, load_context, path)
				.await
				.map_err(|err| GenericMaterialError::InSuperMaterial(inherits.clone(), Box::new(err)))?,
		);
	}

	// Apply the queue

	// We are guaranteed to have at least 1 element. This is the highest super-material.
	let mut final_material = application_queue.pop().unwrap();

	// This goes through the queue from highest super-material to the one we started at, and merges them in that order.
	while let Some(sub_material) = application_queue.pop() {
		match (&mut final_material.properties, sub_material.properties) {
			(Some(final_material_properties), Some(sub_properties)) => {
				for (key, sub_value) in sub_properties {
					match final_material_properties.get_mut(&key) {
						Some(value) => loader.deserializer.merge_value(value, sub_value),
						None => {
							final_material_properties.insert(key, sub_value);
						}
					}
				}
			}
			(None, Some(applicator_properties)) => final_material.properties = Some(applicator_properties),
			_ => {}
		}

		#[cfg(feature = "bevy_pbr")]
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
