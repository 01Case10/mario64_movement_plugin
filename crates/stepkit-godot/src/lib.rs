//! stepkit-godot: thin Godot 4.7+ GDExtension adapter for stepkit-core.
//!
//! This crate converts Godot types to core types, runs the core on a fixed
//! 30 Hz tick, and exposes nodes/resources/signals. It holds no gameplay
//! logic of its own.

use godot::prelude::*;

mod character;
mod params;
mod world;

pub use character::StepChar3D;
pub use params::StepParams;
pub use world::StepWorld3D;

struct StepkitExtension;

#[gdextension]
unsafe impl ExtensionLibrary for StepkitExtension {}
