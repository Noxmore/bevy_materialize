use bevy::prelude::*;
use serde::de::DeserializeOwned;

use super::*;

/// Main trait for file format implementation of generic materials. See [`TomlMaterialDeserializer`] and [`JsonMaterialDeserializer`] for built-in/example implementations.
pub trait MaterialDeserializer: Send + Sync + 'static {
	type Value: GenericValue + DeserializeOwned;
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

#[cfg(feature = "ron")]
#[derive(Debug, Clone, Default)]
pub struct RonMaterialDeserializer;
#[cfg(feature = "ron")]
impl MaterialDeserializer for RonMaterialDeserializer {
	type Value = ron::Value;
	type Error = RonDeserializeError;
	const EXTENSIONS: &[&str] = &["ron", "mat", "mat.ron", "material", "material.ron"];

	fn deserialize<T: DeserializeOwned>(&self, input: &[u8]) -> Result<T, Self::Error> {
		let s = str::from_utf8(input).map_err(serde::de::Error::custom)?;
		ron::from_str(s).map_err(RonDeserializeError)
	}

	fn merge_value(&self, value: &mut Self::Value, other: Self::Value) {
		match (value, other) {
			(ron::Value::Map(value), ron::Value::Map(other)) => {
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
/// Hack wrapper because SpannedError doesn't implement serde::de::Error
#[cfg(feature = "ron")]
pub struct RonDeserializeError(ron::error::SpannedError);
#[cfg(feature = "ron")]
impl std::fmt::Debug for RonDeserializeError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		self.0.fmt(f)
	}
}
#[cfg(feature = "ron")]
impl std::fmt::Display for RonDeserializeError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		self.0.fmt(f)
	}
}
#[cfg(feature = "ron")]
impl std::error::Error for RonDeserializeError {}
#[cfg(feature = "ron")]
impl serde::de::Error for RonDeserializeError {
	fn custom<T>(msg: T) -> Self
	where
		T: std::fmt::Display,
	{
		RonDeserializeError(ron::error::SpannedError {
			code: ron::Error::custom(msg),
			position: ron::de::Position { col: 0, line: 0 },
		})
	}
}
