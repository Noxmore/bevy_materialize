use bevy::prelude::*;
use serde::de::DeserializeOwned;
use serde::Deserializer;

use super::*;

/// Main trait for file format implementation of generic materials. See [`TomlMaterialDeserializer`] and [`JsonMaterialDeserializer`] for built-in/example implementations.
pub trait MaterialDeserializer: Send + Sync + 'static {
	type Value: GenericValue + DeserializeOwned + Deserializer<'static, Error: Send + Sync>;
	type Error: serde::de::Error + Send + Sync;
	/// The asset loader's file extensions.
	const EXTENSIONS: &[&str];

	/// Deserializes raw bytes into a value.
	fn deserialize<T: DeserializeOwned>(&self, input: &[u8]) -> Result<T, Self::Error>;

	/// Merges a value in-place, used for inheritance.
	///
	/// Implementors should recursively merge maps, and overwrite everything else.
	fn merge_value(&self, value: &mut Self::Value, other: Self::Value);
}

#[cfg(feature = "toml")]
#[derive(Debug, Clone, Default)]
pub struct TomlMaterialDeserializer;
#[cfg(feature = "toml")]
impl MaterialDeserializer for TomlMaterialDeserializer {
	type Value = toml::Value;
	type Error = toml::de::Error;
	const EXTENSIONS: &[&str] = &["toml", "mat", "mat.toml", "material", "material.toml"];

	fn deserialize<T: DeserializeOwned>(&self, input: &[u8]) -> Result<T, Self::Error> {
		let s = str::from_utf8(input).map_err(serde::de::Error::custom)?;
		toml::from_str(s)
	}

	fn merge_value(&self, value: &mut Self::Value, other: Self::Value) {
		match (value, other) {
			(toml::Value::Table(value), toml::Value::Table(other)) => {
				for (key, other_value) in other {
					match value.get_mut(&key) {
						Some(value) => self.merge_value(value, other_value),
						None => {
							value.insert(key, other_value);
						}
					}
				}
			}
			(value, other) => *value = other,
		}
	}
}

#[cfg(feature = "json")]
#[derive(Debug, Clone, Default)]
pub struct JsonMaterialDeserializer;
#[cfg(feature = "json")]
impl MaterialDeserializer for JsonMaterialDeserializer {
	type Value = serde_json::Value;
	type Error = serde_json::Error;
	const EXTENSIONS: &[&str] = &["json", "mat", "mat.json", "material", "material.json"];

	fn deserialize<T: DeserializeOwned>(&self, input: &[u8]) -> Result<T, Self::Error> {
		let s = str::from_utf8(input).map_err(serde::de::Error::custom)?;
		serde_json::from_str(s)
	}

	fn merge_value(&self, value: &mut Self::Value, other: Self::Value) {
		match (value, other) {
			(serde_json::Value::Object(value), serde_json::Value::Object(other)) => {
				for (key, other_value) in other {
					match value.get_mut(&key) {
						Some(value) => self.merge_value(value, other_value),
						None => {
							value.insert(key, other_value);
						}
					}
				}
			}
			(value, other) => *value = other,
		}
	}
}
