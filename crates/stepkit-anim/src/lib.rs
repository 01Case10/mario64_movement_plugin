//! stepkit-anim: animation timing manifest, clip-info model and validator.
//!
//! Gameplay timing is owned by the manifest inside the core, never by
//! animation files, so swapping art cannot change how the character plays.
//! See `manifest/timing.ron` and the validator rules in [`validator`].

pub mod manifest;
pub mod rig;
pub mod validator;
