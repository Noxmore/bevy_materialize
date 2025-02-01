#[cfg(feature = "json")]
pub use crate::load::JsonMaterialDeserializer;
#[cfg(feature = "toml")]
pub use crate::load::TomlMaterialDeserializer;
pub use crate::{
	load::MaterialDeserializer, GenericMaterial, GenericMaterial3d, GenericMaterials, MaterialProperty, MaterializeAppExt, MaterializePlugin,
};
