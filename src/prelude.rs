#[cfg(feature = "json")]
pub use crate::load::deserializer::JsonMaterialDeserializer;
#[cfg(feature = "toml")]
pub use crate::load::deserializer::TomlMaterialDeserializer;
pub use crate::{
	load::{deserializer::MaterialDeserializer, ReflectGenericMaterialLoadAppExt},
	GenericMaterial, GenericMaterial3d, GenericMaterials, MaterialProperty, MaterializePlugin,
};
#[cfg(feature = "bevy_pbr")]
pub use crate::{MaterializeAppExt, ReflectGenericMaterial};
