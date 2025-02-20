# bevy_materialize
Crate for loading and applying type-erased materials in Bevy.

Built-in supported formats are `json`, and `toml`, but you can easily add more.

# Usage Example (TOML)

First, add the `MaterializePlugin` to your `App`.
```rust
use bevy::prelude::*;
use bevy_materialize::prelude::*;

fn main_example() {
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
        GenericMaterial3d(asset_server.load("materials/example.material")),
    ));
}
```

`assets/materials/example.material`
```toml
# The type name of the material. Can either be the full path (e.g. bevy_pbr::pbr_material::StandardMaterial),
# or, if only one registered material has the name, just the name itself.
# If this field is not specified, defaults to StandardMaterial
type = "StandardMaterial"

[material]
# Asset paths are relative to the material's path.
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
use bevy_materialize::{prelude::*, load::SimpleGenericMaterialLoaderSettings};

MaterializePlugin::new(TomlMaterialDeserializer).with_simple_loader_settings(Some(SimpleGenericMaterialLoaderSettings {
    material: |image| StandardMaterial {
        base_color_texture: Some(image),
        // Now it's super shiny!
        perceptual_roughness: 0.1,
        ..default()
    }.into(),
    ..default()
}));

// This would disable the image loading functionality entirely.
MaterializePlugin::new(TomlMaterialDeserializer).with_simple_loader_settings(None);
```

## File Extensions
Currently, the supported file extensions are: (Replace `toml` with the file format you're using)
- `toml`
- `mat`
- `mat.toml`
- `material`
- `material.toml`

Feel free to just use the one you like the most.

## Properties

For retrieving properties from a material, the easiest way is with a `GenericMaterialView`, which you can get via the `GenericMaterials` system param.

It's not as easy as getting it from the `GenericMaterial` because properties need additional references to parse, such as the asset server and type registry.
```rust
use bevy::prelude::*;
use bevy_materialize::prelude::*;
use bevy_materialize::GenericMaterialError;

fn retrieve_properties_example(
    materials: GenericMaterials,
) {
    // You can also do materials.get(<asset id>) to get a view.
    for view in materials.iter() {
        // The type returned is based on the generic of the property. For example, VISIBILITY is a MaterialProperty<Visibility>.
        let _: Result<Visibility, GenericMaterialError> = view.get_property(GenericMaterial::VISIBILITY);
    
        // Like get_property(), but if anything goes wrong, returns the default value instead an error.
        let _: Visibility = view.property(GenericMaterial::VISIBILITY);
    }
}
```

For creating your own properties, you should make an extension trait for GenericMaterial.
```rust
use bevy_materialize::prelude::*;

pub trait MyMaterialPropertiesExt {
    const MY_PROPERTY: MaterialProperty<f32> = MaterialProperty::new("my_property", || 5.);
}
impl MyMaterialPropertiesExt for GenericMaterial {}
```

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


# Supported Bevy Versions
| Bevy | bevy_materialize |
-|-
| 0.15 | 0.1-0.4 |