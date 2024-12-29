#[cfg(feature = "json")]
pub use crate::load::JsonMaterialDeserializer;
#[cfg(feature = "toml")]
pub use crate::load::TomlMaterialDeserializer;
pub use crate::{GenericMaterial, GenericMaterial3d, MaterialProperty, MaterializePlugin};
