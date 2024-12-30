# bevy_materialize
Crate for loading materials from files.

Built-in supported formats are `json`, and `toml`, but you can easily add more.

# Usage Example (TOML)

First, add the `MaterializePlugin` to your `App`.
```rust
use bevy::prelude::*;
use bevy_materialize::prelude::*;

fn main() {
    App::new()
        // ...
        .add_plugins(DefaultPlugins)
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

# Supported Bevy Versions
| Bevy | bevy_materialize |
-|-
| 0.15 | 0.1 |