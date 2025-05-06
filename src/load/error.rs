use std::{error::Error, io};

use bevy::reflect::{ApplyError, TypeInfo};
use thiserror::Error;

/// Various errors that may occur when loading a [`GenericMaterial`](crate::GenericMaterial).
#[derive(Error, Debug)]
pub enum GenericMaterialLoadError {
	#[error("{0}")]
	Io(#[from] io::Error),
	#[error("Deserialize error: {0}")]
	Deserialize(Box<dyn Error + Send + Sync>),
	#[error("No registered material found for type {0}")]
	MaterialTypeNotFound(String),
	#[error("Too many type candidates found for `{0}`: {1:?}")]
	TooManyTypeCandidates(String, Vec<String>),
	#[error("field {field} is of type {expected}, but {found} was provided")]
	WrongType { expected: String, found: String, field: String },
	#[error("{0}")]
	Apply(#[from] ApplyError),
	#[error("Enums defined with structures must have exactly one variant (e.g. `alpha_mode = {{ Mask = 0.5 }}`)")]
	WrongNumberEnumElements,
	#[error("No property by the name of {0}")]
	NoProperty(String),
	#[error("No field by the name of {field_name:?} in type {type_name}")]
	NoField { type_name: &'static str, field_name: String },
	#[error("Type not registered: {0}")]
	TypeNotRegistered(&'static str),
	#[error("Property {0} found, but was not registered to any type. Use `App::register_material_property` to register it")]
	PropertyNotRegistered(String),
	#[error("Property {0} found and was registered, but the type it points to isn't registered in the type registry")]
	PropertyTypeNotRegistered(String),
	#[error("Could not get `ReflectFromReflect` for type {0}")]
	NoFromReflect(&'static str),
	#[error("Could not fully reflect property of type {:?}", ty.map(TypeInfo::type_path))]
	FullReflect { ty: Option<&'static TypeInfo> },

	#[error("in field {0} - {1}")]
	InField(String, Box<Self>),

	#[error("in super-material {0} - {1}")]
	InSuperMaterial(String, Box<Self>),
}
