use std::{any::Any, fmt};

use bevy::{
	asset::{LoadContext, UntypedAssetId},
	prelude::*,
	reflect::{GetTypeRegistration, ReflectMut, Typed},
};

/// Type-erased [`Material`].
pub trait ErasedMaterial: Send + Sync + Reflect + Struct {
	fn add_labeled_asset(&self, load_context: &mut LoadContext, label: String) -> Box<dyn ErasedMaterialHandle>;
	fn add_asset(&self, asset_server: &AssetServer) -> Box<dyn ErasedMaterialHandle>;
	fn clone_erased(&self) -> Box<dyn ErasedMaterial>;
}
impl<M: Material + Reflect + Struct + Clone> ErasedMaterial for M {
	fn add_labeled_asset(&self, load_context: &mut LoadContext, label: String) -> Box<dyn ErasedMaterialHandle> {
		load_context.add_labeled_asset(label, self.clone()).into()
	}

	fn add_asset(&self, asset_server: &AssetServer) -> Box<dyn ErasedMaterialHandle> {
		asset_server.add(self.clone()).into()
	}

	fn clone_erased(&self) -> Box<dyn ErasedMaterial> {
		Box::new(self.clone())
	}
}
impl<M: Material + Reflect + Struct + Clone> From<M> for Box<dyn ErasedMaterial> {
	fn from(value: M) -> Self {
		Box::new(value)
	}
}
impl Clone for Box<dyn ErasedMaterial> {
	fn clone(&self) -> Self {
		self.clone_erased()
	}
}

/// Type-erased [`Handle`]`<`[`Material`]`>`.
pub trait ErasedMaterialHandle: Send + Sync + fmt::Debug + Any {
	fn clone_erased(&self) -> Box<dyn ErasedMaterialHandle>;
	fn insert(&self, entity: EntityWorldMut);
	fn remove(&self, entity: EntityWorldMut);
	fn to_untyped_handle(&self) -> UntypedHandle;
	fn id(&self) -> UntypedAssetId;

	#[allow(clippy::type_complexity)]
	fn asset_scope_mut(&self, world: &mut World, f: Box<dyn FnOnce(&mut World, Option<&mut dyn Reflect>) + Send + Sync>);
	#[allow(clippy::type_complexity)]
	fn asset_scope(&self, world: &mut World, f: Box<dyn FnOnce(&mut World, Option<&dyn Reflect>) + Send + Sync>);
}
impl<M: Material + Reflect> ErasedMaterialHandle for Handle<M> {
	fn clone_erased(&self) -> Box<dyn ErasedMaterialHandle> {
		self.clone().into()
	}

	fn insert(&self, mut entity: EntityWorldMut) {
		entity.insert(MeshMaterial3d(self.clone()));
	}

	fn remove(&self, mut entity: EntityWorldMut) {
		entity.remove::<MeshMaterial3d<M>>();
	}

	fn to_untyped_handle(&self) -> UntypedHandle {
		self.clone().untyped()
	}

	fn id(&self) -> UntypedAssetId {
		self.id().untyped()
	}

	fn asset_scope_mut(&self, world: &mut World, f: Box<dyn FnOnce(&mut World, Option<&mut dyn Reflect>) + Send + Sync>) {
		world.resource_scope(|world, mut assets: Mut<'_, Assets<M>>| {
			let asset = assets.get_mut(self.id());
			let asset: Option<&mut dyn Reflect> = match asset {
				Some(m) => Some(m),
				None => None,
			};

			f(world, asset);
		});
	}

	fn asset_scope(&self, world: &mut World, f: Box<dyn FnOnce(&mut World, Option<&dyn Reflect>) + Send + Sync>) {
		world.resource_scope(|world, assets: Mut<'_, Assets<M>>| {
			let asset = assets.get(self.id());
			let asset: Option<&dyn Reflect> = match asset {
				Some(m) => Some(m),
				None => None,
			};

			f(world, asset);
		});
	}
}
impl<M: Material + Reflect> From<Handle<M>> for Box<dyn ErasedMaterialHandle> {
	fn from(value: Handle<M>) -> Self {
		Box::new(value)
	}
}
impl Clone for Box<dyn ErasedMaterialHandle> {
	fn clone(&self) -> Self {
		self.clone_erased()
	}
}

impl dyn ErasedMaterialHandle {
	#[allow(clippy::type_complexity)]
	pub fn modify_with_commands(&self, commands: &mut Commands, f: Box<dyn FnOnce(&mut World, Option<&mut dyn Reflect>) + Send + Sync>) {
		let handle = self.clone_erased();

		commands.queue(move |world: &mut World| {
			handle.asset_scope_mut(world, f);
		});
	}

	/// Attempts to modify a single field in the material. Writes an error out if something fails.
	pub fn modify_field_with_commands<T: Reflect + Typed + FromReflect + GetTypeRegistration>(
		&self,
		commands: &mut Commands,
		field_name: String,
		value: T,
	) {
		self.modify_with_commands(
			commands,
			Box::new(move |_, material| {
				let Some(material) = material else { return };
				let ReflectMut::Struct(s) = material.reflect_mut() else { return };

				let Some(field) = s.field_mut(&field_name) else {
					error!(
						"Tried to modify field {field_name} of {}, but said field doesn't exist!",
						s.reflect_short_type_path()
					);
					return;
				};

				let apply_result = if field.represents::<Option<T>>() {
					field.try_apply(&Some(value))
				} else {
					field.try_apply(&value)
				};

				if let Err(err) = apply_result {
					error!(
						"Tried to modify field {field_name} of {}, but failed to apply: {err}",
						s.reflect_short_type_path()
					);
				}
			}),
		);
	}
}
