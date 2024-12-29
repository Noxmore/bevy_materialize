use std::any::TypeId;

use ::serde;
use bevy::reflect::{serde::*, *};
use bevy::{asset::LoadContext, prelude::*};
use serde::Deserialize;

pub struct GenericMaterialDeserializationProcessor<'w, 'l> {
    pub load_context: &'l mut LoadContext<'w>,
}
impl ReflectDeserializerProcessor for GenericMaterialDeserializationProcessor<'_, '_> {
    fn try_deserialize<'de, D: serde::Deserializer<'de>>(
        &mut self,
        registration: &TypeRegistration,
        _registry: &TypeRegistry,
        deserializer: D,
    ) -> Result<Result<Box<dyn PartialReflect>, D>, D::Error> {
        // TODO maybe make this customizable at some point?

        if registration.type_id() == TypeId::of::<Handle<Image>>() {
            let path = String::deserialize(deserializer)?;
            
            let parent_path = self.load_context.asset_path().parent().unwrap_or_default();
            let path = parent_path.resolve(&path).map_err(serde::de::Error::custom)?;
            let handle = self.load_context.load::<Image>(path);
            return Ok(Ok(Box::new(handle)));
        }

        Ok(Err(deserializer))
    }
}
