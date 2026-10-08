//! stepkit-core: deterministic 30 Hz character-movement simulation.
//!
//! This crate knows nothing about Godot, files, or the clock. Every test,
//! trace comparison and fuzz run works under plain `cargo test`.
//!
//! # Determinism rules (enforced by review + CI)
//!
//! * `f32` only inside the simulation. `f64` is allowed solely when
//!   generating lookup tables at build time.
//! * No `mul_add` (it may fuse on some targets and not others); write
//!   `a * b + c` explicitly.
//! * No hash-map iteration, no clock, no `unsafe` code.
//! * Angles are 16-bit wrapping integers: 65536 units per full turn.
//! * Trigonometry comes from [`trig`], the core's own deterministic
//!   16-bit-angle tables, never the platform `libm`.
//!
//! Every numeric constant that shapes behavior must be documented in
//! `spec/constants.toml` with a provenance entry, and referenced from the
//! doc comment on the constant via `spec: <key>`.

#![forbid(unsafe_code)]

pub mod actions;
pub mod angles;
pub mod events;
pub mod input;
pub mod params;
pub mod state;
pub mod step;
pub mod surface;
pub mod trig;
pub mod world;

/// Current crate version, kept in sync with `Cargo.toml`.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");
