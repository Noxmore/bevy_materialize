use std::time::{Duration, Instant};

use bevy::{prelude::*, reflect::ReflectMut, utils::HashMap};

use crate::prelude::*;

pub struct AnimationPlugin;
impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_systems(PreUpdate, Self::setup_animated_materials)
            .add_systems(Update, Self::animate_materials)
        ;
    }
}
impl AnimationPlugin {
    pub fn setup_animated_materials(
        mut commands: Commands,
        query: Query<(Entity, &GenericMaterial3d), Without<GenericMaterialAnimationState>>,
        generic_materials: Res<Assets<GenericMaterial>>,
    ) {
        for (entity, generic_material_3d) in &query {
            let Some(generic_material) = generic_materials.get(generic_material_3d.id()) else { continue };
            let Ok(animation) = generic_material.get_property(GenericMaterial::ANIMATION) else { continue };

            commands.entity(entity).insert(GenericMaterialAnimationState {
                current_frame: 0,
                next_frame_time: animation.next_frame_time(),
            });
        }
    }
    
    pub fn animate_materials(
        mut commands: Commands,
        mut query: Query<(Entity, &GenericMaterial3d, &mut GenericMaterialAnimationState)>,
        generic_materials: Res<Assets<GenericMaterial>>,
    ) {
        let now = Instant::now();
        
        for (entity, generic_material_3d, mut state) in &mut query {
            // If the next frame is in the future, we don't need to update this.
            if state.next_frame_time > now { continue }
            // In the very common case that someone 
            state.current_frame = state.current_frame.wrapping_add(1);
            
            let Some(generic_material) = generic_materials.get(generic_material_3d.id()) else {
                commands.entity(entity).remove::<GenericMaterialAnimationState>();
                continue;
            };
            let Ok(animation) = generic_material.get_property(GenericMaterial::ANIMATION) else {
                commands.entity(entity).remove::<GenericMaterialAnimationState>();
                continue;
            };

            state.next_frame_time = animation.next_frame_time();

            if let Some(next) = animation.next {
                commands.entity(entity).insert(GenericMaterial3d(next));
            } else {
                let current_frame = state.current_frame;
                generic_material.material.modify_with_commands(&mut commands, Box::new(move |material| {
                    let Some(material) = material else { return };
                    let ReflectMut::Struct(s) = material.reflect_mut() else { return };

                    for (field_name, frames) in animation.images {
                        let Some(field) = s.field_mut(&field_name) else {
                            error!("Tried to animate field {field_name} of {}, but said field doesn't exist!", s.reflect_short_type_path());
                            continue;
                        };
                        
                        let new_idx = current_frame % frames.len();
                        
                        if let Err(err) = field.try_apply(&frames[new_idx]) {
                            error!("Tried to animate field {field_name} of {}, but failed to apply: {err}", s.reflect_short_type_path());
                        }
                    }
                }));
            }
        }
    }
}
 
impl GenericMaterial {
    pub const ANIMATION: MaterialProperty<MaterialAnimation> = MaterialProperty::new("animation", default);
}

// TODO different framerates for both next and images, also maybe support alt textures? (unlikely)
#[derive(Reflect, Debug, Clone, Default)]
pub struct MaterialAnimation {
    pub fps: f32,
    pub next: Option<Handle<GenericMaterial>>,
    pub images: HashMap<String, Vec<Handle<Image>>>,
}
impl MaterialAnimation {
    /// Creates an instant that is the point in time in the future that the next frame will show. (Assuming that a frame has just switched)
    pub fn next_frame_time(&self) -> Instant {
        Instant::now() + Duration::from_secs_f32(1. / self.fps)
    }
}

/// Component that schedules the next frame of a [GenericMaterial3d] with it's material containing a [MaterialAnimation].
#[derive(Component, Reflect, Debug, Clone, Copy)]
pub struct GenericMaterialAnimationState {
    pub current_frame: usize,
    pub next_frame_time: Instant,
}