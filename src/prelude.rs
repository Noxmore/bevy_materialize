#[cfg(feature = "json")]
pub use crate::load::JsonMaterialDeserializer;
#[cfg(feature = "toml")]
pub use crate::load::TomlMaterialDeserializer;
#[cfg(feature = "ron")]
pub use crate::load::RonMaterialDeserializer;
pub use crate::{GenericMaterial, GenericMaterial3d, MaterialProperty, MaterializePlugin};
