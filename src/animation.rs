use std::time::Duration;

use bevy::{
	platform::collections::{HashMap, HashSet},
	prelude::*,
};

use crate::{
	material_property::{GetPropertyError, MaterialPropertyAppExt},
	prelude::*,
};

impl GenericMaterial {
	/// Material property supporting animation, only works if [`MaterializePlugin::animated_materials`] is enabled.
	///
	/// See docs for [`MaterialAnimations`] for more information.
	pub const ANIMATION: MaterialProperty<MaterialAnimations> = MaterialProperty::new("animation");
}

pub struct AnimationPlugin;
impl Plugin for AnimationPlugin {
	fn build(&self, app: &mut App) {
		#[rustfmt::skip]
		app
			.register_material_property(GenericMaterial::ANIMATION)
			.init_resource::<AnimatedGenericMaterials>()
			.add_systems(Update, Self::animate_materials)
		;

		#[cfg(feature = "bevy_pbr")]
		app.add_systems(PreUpdate, Self::setup_animated_materials.before(crate::insert_generic_materials));
		#[cfg(not(feature = "bevy_pbr"))]
		app.add_systems(PreUpdate, Self::setup_animated_materials);
	}
}
impl AnimationPlugin {
	/// Sets up materials using the [`ANIMATION`](GenericMaterial::ANIMATION) property, and reports errors if they're invalid.
	pub fn setup_animated_materials(
		mut animated_materials: ResMut<AnimatedGenericMaterials>,
		generic_materials: Res<Assets<GenericMaterial>>,
		time: Res<Time>,

		mut asset_events: MessageReader<AssetEvent<GenericMaterial>>,
		mut failed_reading: Local<HashSet<AssetId<GenericMaterial>>>,
	) {
		for event in asset_events.read() {
			let AssetEvent::Modified { id } = event else { continue };

			failed_reading.remove(id);
			animated_materials.states.remove(id);
		}

		for (id, generic_material) in generic_materials.iter() {
			// Already set up or failed
			if failed_reading.contains(&id) || animated_materials.states.contains_key(&id) {
				continue;
			}

			let mut animations = match generic_material.get_property(GenericMaterial::ANIMATION).cloned() {
				Ok(x) => x,
				Err(GetPropertyError::NotFound) => continue,
				Err(err) => {
					error!("Failed to read animation property from GenericMaterial: {err}");
					failed_reading.insert(id);
					continue;
				}
			};

			// Make next not switch instantly, slightly hacky.
			if let Some(animation) = &mut animations.next {
				animation.state.next_frame_time = animation.new_next_frame_time(time.elapsed());
			}

			animated_materials.states.insert(id, animations);
		}
	}

	/// Animates generic materials with the [`ANIMATION`](GenericMaterial::ANIMATION) property.
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
			if let Some(animation) = &mut animations.next
				&& animation.state.next_frame_time <= now
			{
				animation.advance_frame(now);

				for (entity, generic_material_3d) in &query {
					if generic_material_3d.id() != *id {
						continue;
					}

					commands.entity(entity).insert(GenericMaterial3d(animation.material.clone()));
				}
			}

			// Image switching
			#[cfg(feature = "bevy_pbr")]
			if let Some(animation) = &mut animations.images
				&& animation.state.next_frame_time <= now
			{
				animation.advance_frame(now);
				let Some(generic_material) = generic_materials.get(*id) else { continue };

				for (field_name, frames) in &animation.fields {
					let new_idx = animation.state.current_frame % frames.len();

					let handle = generic_material.handle.clone();
					let field_name = field_name.clone();
					let new_frame = frames[new_idx].clone();

					commands.queue(move |world: &mut World| {
						handle.modify_field(world, field_name, new_frame);
					});
				}
			}
		}
	}
}

/// Stores the states and animations of [`GenericMaterial`]s.
#[derive(Resource, Reflect, Default)]
pub struct AnimatedGenericMaterials {
	pub states: HashMap<AssetId<GenericMaterial>, MaterialAnimations>,
}

/// Animations stored in a [`GenericMaterial`].
///
/// Stores both [`NextAnimation`], which allows the material to switch to another after a period of time,
/// and [`ImagesAnimation`], which allows different image fields to cycle a list of images at a specified framerate.
///
/// For practical examples of how to use these, see the associated examples in the repo.
#[derive(Reflect, Debug, Clone)]
pub struct MaterialAnimations {
	pub next: Option<NextAnimation>,
	pub images: Option<ImagesAnimation>,
}

/// Functionality shared across different animations.
pub trait MaterialAnimation {
	fn state_mut(&mut self) -> &mut GenericMaterialAnimationState;

	/// Increases current frame and updates when the next frame is scheduled.
	fn advance_frame(&mut self, current_time: Duration) {
		let new_next_frame_time = self.new_next_frame_time(current_time);

		let state = self.state_mut();
		state.current_frame = state.current_frame.wrapping_add(1);
		state.next_frame_time = new_next_frame_time;
	}

	/// This returns when in the future (from `current_time`) the frame should advance again.
	fn new_next_frame_time(&self, current_time: Duration) -> Duration;
}

/// Switch to [`material`](Self::material) after [`seconds`](Self::seconds).
#[derive(Reflect, Debug, Clone)]
pub struct NextAnimation {
	pub seconds: f32,
	pub material: Handle<GenericMaterial>,

	#[reflect(ignore)]
	pub state: GenericMaterialAnimationState,
}
impl MaterialAnimation for NextAnimation {
	fn state_mut(&mut self) -> &mut GenericMaterialAnimationState {
		&mut self.state
	}

	fn new_next_frame_time(&self, current_time: Duration) -> Duration {
		current_time + Duration::from_secs_f32(self.seconds)
	}
}

/// Allows different image [`fields`](Self::fields) to cycle a list of images at a specified [`fps`](Self::fps).
#[derive(Reflect, Debug, Clone)]
pub struct ImagesAnimation {
	pub fps: f32,
	#[cfg(feature = "bevy_image")]
	pub fields: HashMap<String, Vec<Handle<Image>>>,
	#[cfg(not(feature = "bevy_image"))]
	pub fields: HashMap<String, Vec<String>>,

	#[reflect(ignore)]
	pub state: GenericMaterialAnimationState,
}
impl MaterialAnimation for ImagesAnimation {
	fn state_mut(&mut self) -> &mut GenericMaterialAnimationState {
		&mut self.state
	}

	fn new_next_frame_time(&self, current_time: Duration) -> Duration {
		current_time + Duration::from_secs_f32(1. / self.fps)
	}
}

/// Stores the current frame, and schedules when the next frame should occur.
#[derive(Debug, Clone, Copy)]
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
