use std::fmt;

use serde::Deserializer;

/// Trait meant for `Value` types of different serialization libraries. For example, for the [`toml`] crate, this is implemented for [`toml::Value`].
///
/// This is for storing general non type specific data for deserializing on demand, such as in [`GenericMaterial`](crate::GenericMaterial) properties.
///
/// NOTE: Because of the limitation of not being able to implement foreign traits for foreign types, this is automatically implemented for applicable types implementing the [`Deserializer`] trait.
pub trait GenericValue: fmt::Debug + Send + Sync {}
impl<T: Deserializer<'static, Error: Send + Sync> + fmt::Debug + Clone + Send + Sync + 'static> GenericValue for T {}
