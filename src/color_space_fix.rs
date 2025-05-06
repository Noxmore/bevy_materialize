use bevy::prelude::*;

/// Automatically fixes material maps with the incorrect color space. Currently only affects [`StandardMaterial`].
pub struct ColorSpaceFixPlugin;
impl Plugin for ColorSpaceFixPlugin {
	fn build(&self, #[allow(unused)] app: &mut App) {
		#[cfg(feature = "bevy_pbr")]
		app.add_systems(Update, Self::standard_material);
	}
}
impl ColorSpaceFixPlugin {
	#[cfg(feature = "bevy_pbr")]
	pub fn standard_material(
		materials: Res<Assets<StandardMaterial>>,
		mut images: ResMut<Assets<Image>>,
		mut material_events: EventReader<AssetEvent<StandardMaterial>>,
		mut image_events: EventReader<AssetEvent<Image>>,
	) {
		if material_events.is_empty() && image_events.is_empty() {
			return;
		}
		material_events.clear();
		image_events.clear();

		for (_, material) in materials.iter() {
			macro_rules! make_linear {
				($($field:ident)*) => {
					$(if let Some(image_handle) = &material.$field {
						if let Some(image) = images.get(image_handle) {
							if image.texture_descriptor.format.is_srgb() {
								if let Some(image) = images.get_mut(image_handle) {
									image.texture_descriptor.format = image.texture_descriptor.format.remove_srgb_suffix();
								}
							}
						}
					})*
				};
			}

			// The ones commented out are feature locked
			make_linear!(normal_map_texture occlusion_texture metallic_roughness_texture /* anisotropy_texture clearcoat_texture clearcoat_roughness_texture clearcoat_normal_texture */);
		}
	}
}
