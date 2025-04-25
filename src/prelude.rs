#[cfg(feature = "json")]
pub use crate::load::deserializer::JsonMaterialDeserializer;
#[cfg(feature = "toml")]
pub use crate::load::deserializer::TomlMaterialDeserializer;
#[cfg(feature = "bevy_pbr")]
pub use crate::{MaterializeAppExt, generic_material::ReflectGenericMaterial};
pub use crate::{
	MaterializePlugin,
	generic_material::{GenericMaterial, GenericMaterial3d, MaterialProperty, MaterialPropertyAppExt},
	load::{ReflectGenericMaterialLoadAppExt, deserializer::MaterialDeserializer},
};
