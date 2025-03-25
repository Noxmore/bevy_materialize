use std::time::Duration;

use bevy::{
	platform_support::collections::{HashMap, HashSet},
	prelude::*,
};

use crate::{prelude::*, GenericMaterialError};

pub struct AnimationPlugin;
impl Plugin for AnimationPlugin {
	fn build(&self, app: &mut App) {
		#[rustfmt::skip]
		app
			.register_type::<MaterialAnimations>()
			.init_resource::<AnimatedGenericMaterials>()
			.add_systems(Update, Self::animate_materials)
		;

		#[cfg(feature = "bevy_pbr")]
		app.add_systems(PostUpdate, Self::setup_animated_materials.before(crate::insert_generic_materials));
		#[cfg(not(feature = "bevy_pbr"))]
		app.add_systems(PostUpdate, Self::setup_animated_materials);
	}
}
impl AnimationPlugin {
	pub fn setup_animated_materials(
		mut animated_materials: ResMut<AnimatedGenericMaterials>,
		generic_materials: GenericMaterials,
		time: Res<Time>,

		mut asset_events: EventReader<AssetEvent<GenericMaterial>>,

		mut failed_reading: Local<HashSet<AssetId<GenericMaterial>>>,
	) {
		for event in asset_events.read() {
			let AssetEvent::Modified { id } = event else { continue };

			failed_reading.remove(id);
			animated_materials.states.remove(id);
		}

		for view in generic_materials.iter() {
			if failed_reading.contains(&view.id) || animated_materials.states.contains_key(&view.id) {
				continue;
			}
			let mut animations = match view.get_property(GenericMaterial::ANIMATION) {
				Ok(v) => v,
				Err(GenericMaterialError::NoProperty(_)) => continue,
				Err(err) => {
					error!("Failed to read animation property from GenericMaterial {:?}: {err}", view.id);
					failed_reading.insert(view.id);
					continue;
				}
			};

			// Make next not switch instantly, slightly hacky.
			if let Some(animation) = &mut animations.next {
				animation.state.next_frame_time = animation.new_next_frame_time(time.elapsed());
			}

			animated_materials.states.insert(view.id, animations);
		}
	}

	pub fn animate_materials(
		mut commands: Commands,
		mut animated_materials: ResMut<AnimatedGenericMaterials>,
		#[cfg(feature = "bevy_pbr")] generic_materials: Res<Assets<GenericMaterial>>,
		time: Res<Time>,

		query: Query<(Entity, &GenericMaterial3d)>,
	) {
		let now = time.elapsed();

		for (id, animations) in &mut animated_materials.states {
			// Material switching
			if let Some(animation) = &mut animations.next {
				if animation.state.next_frame_time <= now {
					animation.advance_frame(now);

					for (entity, generic_material_3d) in &query {
						if generic_material_3d.id() != *id {
							continue;
						}

						commands.entity(entity).insert(GenericMaterial3d(animation.value.clone()));
					}
				}
			}

			// Image switching
			#[cfg(feature = "bevy_pbr")]
			if let Some(animation) = &mut animations.images {
				if animation.state.next_frame_time <= now {
					animation.advance_frame(now);
					let Some(generic_material) = generic_materials.get(*id) else { continue };

					for (field_name, frames) in &animation.value {
						let new_idx = animation.state.current_frame % frames.len();
						generic_material
							.handle
							.modify_field_with_commands(&mut commands, field_name.clone(), frames[new_idx].clone());
					}
				}
			}
		}
	}
}

impl GenericMaterial {
	pub const ANIMATION: MaterialProperty<MaterialAnimations> = MaterialProperty::new("animation", default);
}

/// Stores the states and animations of [`GenericMaterial`]s.
#[derive(Resource, Reflect, Default)]
pub struct AnimatedGenericMaterials {
	pub states: HashMap<AssetId<GenericMaterial>, MaterialAnimations>,
}

/// Animations stored in a [`GenericMaterial`].
#[derive(Reflect, Debug, Clone, Default)]
pub struct MaterialAnimations {
	pub next: Option<NextAnimation>,
	pub images: Option<ImagesAnimation>,
}

#[derive(Reflect, Debug, Clone, Default)]
pub struct MaterialAnimation<T> {
	pub fps: f32,
	pub value: T,

	#[reflect(ignore)]
	pub state: GenericMaterialAnimationState,
}
impl<T> MaterialAnimation<T> {
	/// Increases current frame and updates when the next frame is scheduled.
	pub fn advance_frame(&mut self, current_time: Duration) {
		self.state.current_frame = self.state.current_frame.wrapping_add(1);
		self.state.next_frame_time = self.new_next_frame_time(current_time);
	}

	/// This returns when in the future (from `current_time`) the frame should advance again.
	pub fn new_next_frame_time(&self, current_time: Duration) -> Duration {
		current_time + Duration::from_secs_f32(1. / self.fps)
	}
}

pub type NextAnimation = MaterialAnimation<Handle<GenericMaterial>>;
#[cfg(feature = "bevy_image")]
pub type ImagesAnimation = MaterialAnimation<HashMap<String, Vec<Handle<Image>>>>;
#[cfg(not(feature = "bevy_image"))]
pub type ImagesAnimation = MaterialAnimation<HashMap<String, Vec<String>>>;

/// Stores the current frame, and schedules when the next frame should occur.
#[derive(Debug, Clone)]
pub struct GenericMaterialAnimationState {
	/// Is [`usize::MAX`] by default so it'll wrap around immediately to frame 0.
	pub current_frame: usize,
	/// The elapsed time from program start that the next frame will appear.
	pub next_frame_time: Duration,
}
impl Default for GenericMaterialAnimationState {
	fn default() -> Self {
		Self {
			current_frame: usize::MAX,
			next_frame_time: Duration::default(),
		}
	}
}
