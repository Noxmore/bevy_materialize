# bevy_materialize

[![crates.io](https://img.shields.io/crates/v/bevy_materialize)](https://crates.io/crates/bevy_materialize)
[![docs.rs](https://docs.rs/bevy_materialize/badge.svg)](https://docs.rs/bevy_materialize)

Crate for loading and applying type-erased materials in Bevy.

Built-in supported formats are `json`, and `toml`, but you can easily add more.

# Usage Example (TOML)

First, add the `MaterializePlugin` to your `App`.
```rust
use bevy::prelude::*;
use bevy_materialize::prelude::*;

fn example_main() {
    App::new()
        // ...
        .add_plugins(MaterializePlugin::new(TomlMaterialDeserializer))
        // ...
        .run();
}
```

## Loading

The API for adding to an entity is quite similar to `MeshMaterial3d<...>`, just with `GenericMaterial3d` storing a `Handle<GenericMaterial>` instead, which you can load from a file.
```rust
use bevy::prelude::*;
use bevy_materialize::prelude::*;

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
    commands.spawn((
        Mesh3d(asset_server.add(Cuboid::from_length(1.).into())),
        GenericMaterial3d(asset_server.load("materials/example.toml")),
    ));
}
```

`assets/materials/example.toml`
```toml
# The type name of the material. Can either be the full path (e.g. bevy_pbr::pbr_material::StandardMaterial),
# or, if only one registered material has the name, just the name itself.
# If this field is not specified, defaults to StandardMaterial
type = "StandardMaterial"

[material]
# Asset paths are relative to the material's path,
# unless they start with a '/', then they will be relative to the assets folder.
base_color_texture = "example.png"
emissive = [0.1, 0.2, 0.5, 1.0]
alpha_mode = { Mask = 0.5 }

# Optional custom properties, these can be whatever you want.
[properties]
# This one is built-in, and sets the entity's Visibility when the material is applied.
visibility = "Hidden"
collision = true
sounds = "wood"
```

For simplicity, you can also load a `GenericMaterial` directly from an image file, which by default puts a `StandardMaterial` internally. You can change the material that it uses via
```rust
use bevy::prelude::*;
use bevy_materialize::{prelude::*, load::simple::SimpleGenericMaterialLoader};

MaterializePlugin::new(TomlMaterialDeserializer).with_simple_loader(Some(SimpleGenericMaterialLoader {
    material: |image| StandardMaterial {
        base_color_texture: Some(image),
        // Now it's super shiny!
        perceptual_roughness: 0.1,
        ..default()
    }.into(),
    ..default()
}));

// This would disable the image loading functionality entirely.
MaterializePlugin::new(TomlMaterialDeserializer).with_simple_loader(None);
```

NOTE: This loader seems to take priority over Bevy's image loader when it doesn't know which asset you want, so if you're loading images as untyped assets you'll have to turn this off.

## File Extensions
Currently, the supported file extensions are: (Replace `toml` with the file format you're using)
- `toml`
- `mat`
- `mat.toml`
- `material`
- `material.toml`

Feel free to just use the one you like the most.

## Properties

For retrieving custom properties from a material, the API is pretty simple.

```rust
use bevy::prelude::*;
use bevy_materialize::prelude::*;
use bevy_materialize::material_property::GetPropertyError;

fn retrieve_properties_example(material: &GenericMaterial) {
    // The type returned is based on the generic of the property. For example, VISIBILITY is a MaterialProperty<Visibility>.
    let _: Result<&Visibility, GetPropertyError> = material.get_property(GenericMaterial::VISIBILITY);
}
```

For creating your own properties, you should make an extension trait for GenericMaterial, then register it with your app.
```rust
use bevy::prelude::*;
use bevy_materialize::prelude::*;

pub trait MyMaterialProperties {
    const MY_PROPERTY: MaterialProperty<f32> = MaterialProperty::new("my_property");
}
impl MyMaterialProperties for GenericMaterial {}

fn example_main() {
    App::new()
        .register_material_property(GenericMaterial::MY_PROPERTY)
        // ...
    ;
}
```
`MaterialProperty` is just a helper struct that bundles the type and key together, and technically isn't necessary for any of this.

## Registering

When creating your own custom materials, all you have to do is register them in your app like so.
```rust ignore
App::new()
    // ...
    .register_generic_material::<YourMaterial>()
```
This will also register the type if it hasn't been registered already.

You can also register a shorthand if your material's name is very long (like if it's an `ExtendedMaterial<...>`).
```rust ignore
App::new()
    // ...
    .register_generic_material_shorthand::<YourMaterialWithALongName>("YourMaterial")
```
This will allow you to put the shorthand in your file's `type` field instead of the type name.

## Headless

For headless contexts like dedicated servers where you only want properties, but no materials, you can turn off the `bevy_pbr` feature on this crate by disabling default features, and manually adding the loaders you want.

```toml
bevy_materialize = { version = "...", default-features = false, features = ["toml"] }
```

## Inheritance

When creating a bunch of PBR materials, your files might look something like this
```toml
# example.toml
[material]
base_color_texture = "example.png"
occlusion_texture = "example_ao.png"
metallic_roughness_texture = "example_mr.png"
normal_map_texture = "example_normal.png"
depth_map = "example_depth.png"
```

This is a lot of boilerplate, especially considering you have to manually rename 5 instances of your material name for *every* material.

This is where inheritance comes in, you can make a file like
```toml
# pbr.toml
[material]
base_color_texture = "${name}.png"
occlusion_texture = "${name}_ao.png"
metallic_roughness_texture = "${name}_mr.png"
normal_map_texture = "${name}_normal.png"
depth_map = "${name}_depth.png"
```
`${name}` is a special pattern that gets replaced to the name of the material loaded. (This functionality can be turned off from the plugin)

Now you can rewrite your `example.toml` into
```toml
inherits = "pbr.toml"
```
This is much less boilerplate, and you can just copy and paste it without needing to manually rename everything.
You can still override and add more fields to the sub-material, this just gives you a handy baseline.

TIP: Like other assets, if you start the path with a '/', it is relative to the assets folder rather than the material's. This is useful for setups with a bunch of subfolders.

## Processors

`bevy_materialize` has a processor API wrapping Bevy's [`ReflectDeserializerProcessor`](https://docs.rs/bevy/latest/bevy/reflect/serde/trait.ReflectDeserializerProcessor.html).
This allows you to modify data as it's being deserialized. For example, this system is used for loading assets, treating strings as paths.

It's used much like Rust's iterator API, each processor having a child processor that is stored via generic. If you want to make your own, check out `AssetLoadingProcessor` for a simple example of an implementation, then use it with your `MaterializePlugin`.
```rust ignore
pub struct MyProcessor<P: MaterialProcessor>(pub P);
impl<P: MaterialProcessor> MaterialProcessor for MyProcessor<P> {
	// ...
}

MaterializePlugin::new(TomlMaterialDeserializer) // type: MaterializePlugin<..., AssetLoadingProcessor<()>>
    .with_processor(MyProcessor) // type: MaterializePlugin<..., MyProcessor<AssetLoadingProcessor<()>>>
```

## Other Utilities
- By default, images in fields in `StandardMaterial` that want linear images will convert any sRGB images in them. This can be turned off with `MaterializePlugin::with_standard_material_color_space_fix`.

# Supported Bevy Versions
| Bevy | bevy_materialize |
|------|------------------|
| 0.18 | 0.9              |
| 0.17 | 0.8              |
| 0.16 | 0.5-0.7          |
| 0.15 | 0.1-0.4          |
