#[cfg(feature = "json")]
pub use crate::load::deserializer::JsonMaterialDeserializer;
#[cfg(feature = "toml")]
pub use crate::load::deserializer::TomlMaterialDeserializer;
#[cfg(feature = "bevy_pbr")]
pub use crate::{generic_material::ReflectGenericMaterial, MaterializeAppExt};
pub use crate::{
	generic_material::{GenericMaterial, GenericMaterial3d, GenericMaterialError, MaterialProperty, MaterialPropertyAppExt},
	load::{deserializer::MaterialDeserializer, ReflectGenericMaterialLoadAppExt},
	MaterializePlugin,
};
