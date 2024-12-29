use std::{
    any::TypeId,
    fmt::{self, Formatter},
};

use ::serde;
use bevy::reflect::{serde::*, *};
use bevy::{asset::LoadContext, prelude::*};
use serde::de::{SeqAccess, Visitor};

// pub type DeserializationProcessor = dyn Fn(&mut GenericMaterialDeserializationProcessor, &bevy::reflect::TypeRegistration, &bevy::reflect::TypeRegistry, );
pub struct GenericMaterialDeserializationProcessor<'w, 'l> {
    pub load_context: &'l mut LoadContext<'w>,
}
impl ReflectDeserializerProcessor for GenericMaterialDeserializationProcessor<'_, '_> {
    fn try_deserialize<'de, D: serde::Deserializer<'de>>(
        &mut self,
        registration: &TypeRegistration,
        registry: &TypeRegistry,
        deserializer: D,
    ) -> Result<Result<Box<dyn PartialReflect>, D>, D::Error> {
        // TODO maybe make this customizable at some point?

        if registration.type_id() == TypeId::of::<Handle<Image>>() {
            // TODO Load assets
        }

        Ok(Err(deserializer))
    }
}
