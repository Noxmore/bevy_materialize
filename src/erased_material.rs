use std::fmt;

use bevy::{
	asset::{LoadContext, UntypedAssetId},
	prelude::*,
	reflect::{GetTypeRegistration, ReflectMut, Typed},
};

/// Type-erased [`Material`].
pub trait ErasedMaterial: Send + Sync + Reflect + Struct {
	fn add_labeled_asset(self: Box<Self>, load_context: &mut LoadContext, label: String) -> ErasedMaterialHandle;
	fn add_asset(self: Box<Self>, asset_server: &AssetServer) -> ErasedMaterialHandle;
	fn clone_erased(&self) -> Box<dyn ErasedMaterial>;
}
impl<M: Material + Reflect + Struct + Clone> ErasedMaterial for M {
	fn add_labeled_asset(self: Box<Self>, load_context: &mut LoadContext, label: String) -> ErasedMaterialHandle {
		load_context.add_labeled_asset(label, *self).into()
	}

	fn add_asset(self: Box<Self>, asset_server: &AssetServer) -> ErasedMaterialHandle {
		asset_server.add(*self).into()
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

// Wrapper struct instead of dyn-compatible trait because `Handle<T>` is always the same size (we have `UntypedHandle`)

/// Wrapper over [`UntypedHandle`] specifically for reflected [`Material`]s, containing functions related to managing said materials on entities.
#[derive(Clone)]
pub struct ErasedMaterialHandle {
	inner: UntypedHandle,
	vtable: &'static ErasedMaterialHandleVTable,
}
#[allow(clippy::type_complexity)]
impl ErasedMaterialHandle {
	pub fn new<M: Material + Reflect>(handle: Handle<M>) -> Self {
		Self {
			inner: handle.untyped(),
			vtable: ErasedMaterialHandleVTable::of::<M>(),
		}
	}

	#[inline]
	pub fn inner(&self) -> &UntypedHandle {
		&self.inner
	}
	#[inline]
	pub fn take_inner(self) -> UntypedHandle {
		self.inner
	}

	#[inline]
	pub fn id(&self) -> UntypedAssetId {
		self.inner.id()
	}

	/// Inserts the appropriate [`MeshMaterial3d`] on an entity.
	#[inline]
	pub fn insert(self, entity: EntityWorldMut) {
		(self.vtable.insert)(self.inner, entity);
	}

	/// Removes the appropriate [`MeshMaterial3d`] from an entity.
	#[inline]
	pub fn remove(&self, entity: EntityWorldMut) {
		(self.vtable.remove)(entity);
	}

	/// Gets the asset from the world's appropriate [`Assets<...>`] collection.
	#[inline]
	pub fn get_from_world<'w>(&self, world: &'w World) -> Option<&'w dyn Reflect> {
		(self.vtable.get_from_world)(self.id(), world)
	}

	/// Runs a function on the reference to this asset grabbed from the world's appropriate [`Assets<...>`] collection
	///
	/// Passes the world through to the function to allow for mutable world access while having access to the material.
	///
	/// If you don't need access to the world, use [`get_from_world(...)`](Self::get_from_world).
	#[inline]
	pub fn asset_scope(&self, world: &mut World, f: Box<dyn FnOnce(&mut World, Option<&dyn Reflect>) + Send + Sync>) {
		(self.vtable.asset_scope)(self.id(), world, f);
	}

	/// Runs a function on the reference to this asset grabbed from the world's appropriate [`Assets<...>`] collection
	///
	/// Passes the world through to the function to allow for mutable world access while having access to the material.
	///
	/// If you don't need mutable access to the material, use [`asset_scope(...)`](Self::asset_scope).
	#[inline]
	pub fn asset_scope_mut(&self, world: &mut World, f: Box<dyn FnOnce(&mut World, Option<&mut dyn Reflect>) + Send + Sync>) {
		(self.vtable.asset_scope_mut)(self.id(), world, f);
	}

	/// Attempts to modify a single field in the material. Writes an error out if something fails.
	pub fn modify_field<T: Reflect + Typed + FromReflect + GetTypeRegistration>(&self, world: &mut World, field_name: String, value: T) {
		self.asset_scope_mut(
			world,
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

#[allow(clippy::type_complexity)]
struct ErasedMaterialHandleVTable {
	insert: fn(UntypedHandle, EntityWorldMut),
	remove: fn(EntityWorldMut),
	get_from_world: for<'w> fn(UntypedAssetId, &'w World) -> Option<&'w dyn Reflect>,
	asset_scope: fn(UntypedAssetId, &mut World, Box<dyn FnOnce(&mut World, Option<&dyn Reflect>) + Send + Sync>),
	asset_scope_mut: fn(UntypedAssetId, &mut World, Box<dyn FnOnce(&mut World, Option<&mut dyn Reflect>) + Send + Sync>),
}
impl ErasedMaterialHandleVTable {
	fn of<M: Material + Reflect>() -> &'static Self {
		&Self {
			insert: |handle, mut entity| {
				entity.insert(MeshMaterial3d::<M>(handle.typed_debug_checked()));
			},
			remove: |mut entity| {
				entity.remove::<MeshMaterial3d<M>>();
			},
			get_from_world: |id, world| {
				let asset: &dyn Reflect = world.get_resource::<Assets<M>>()?.get(id.typed_debug_checked())?;
				Some(asset)
			},
			asset_scope: |id, world, f| {
				world.resource_scope(|world, assets: Mut<'_, Assets<M>>| {
					let asset = assets.get(id.typed_debug_checked());
					let asset: Option<&dyn Reflect> = match asset {
						Some(m) => Some(m),
						None => None,
					};

					f(world, asset);
				});
			},
			asset_scope_mut: |id, world, f| {
				world.resource_scope(|world, mut assets: Mut<'_, Assets<M>>| {
					let asset = assets.get_mut(id.typed_debug_checked());
					let asset: Option<&mut dyn Reflect> = match asset {
						Some(m) => Some(m),
						None => None,
					};

					f(world, asset);
				});
			},
		}
	}
}
impl<M: Material + Reflect> From<Handle<M>> for ErasedMaterialHandle {
	fn from(value: Handle<M>) -> Self {
		Self::new(value)
	}
}
impl fmt::Debug for ErasedMaterialHandle {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.inner.fmt(f)
	}
}
