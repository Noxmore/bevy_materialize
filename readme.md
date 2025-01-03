# bevy_materialize
Crate for loading materials from files.

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

For retrieving properties from a material, you do the following.
```rust
use bevy::prelude::*;
use bevy_materialize::prelude::*;
use bevy_materialize::GenericMaterialError;

fn retrieve_properties_example(material: &GenericMaterial) {
    // The type returned is based on the generic of the property. For example, VISIBILITY is a MaterialProperty<Visibility>.
    let _: Result<Visibility, GenericMaterialError> = material.get_property(GenericMaterial::VISIBILITY);

    // Like get_property(), but if anything goes wrong, returns the default value instead an error.
    let _: Visibility = material.property(GenericMaterial::VISIBILITY);
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

# Supported Bevy Versions
| Bevy | bevy_materialize |
-|-
| 0.15 | 0.1 |