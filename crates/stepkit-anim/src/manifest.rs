//! The animation timing manifest: the contract between gameplay and art.
//!
//! The manifest lives in the core's domain and states, per animation slot,
//! the frame count, loop mode, speed model, named events, root-motion policy
//! and contact windows. Gameplay reads this; the renderer only follows.

use serde::{Deserialize, Serialize};

/// Frames per second of the gameplay clock. Fixed.
pub const MANIFEST_FPS: u32 = 30;

/// How a slot plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotMode {
    Loop,
    Once,
}

/// How the timeline advances for a slot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SpeedModel {
    /// Constant speed multiplier.
    Fixed(f32),
    /// Speed scales with forward speed: `clamp(base + per_unit * speed, min, max)`.
    ScaledByForwardSpeed {
        base: f32,
        per_unit: f32,
        min: f32,
        max: f32,
    },
}

/// Root-motion policy for a slot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RootPolicy {
    /// The clip must stay in place (rule T04).
    InPlace,
    /// A position curve stored in the manifest drives the root.
    Curve,
}

/// A named event at a specific frame (footstep, takeoff, release, ...).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnimEvent {
    pub name: String,
    pub frame: u32,
}

/// Frames during which a foot bone should be at its lowest point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContactWindow {
    pub bone: String,
    pub start_frame: u32,
    pub end_frame: u32,
}

/// One animation slot: everything gameplay needs to know about a clip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    /// Stable slot id, e.g. `walk_cycle`.
    pub id: String,
    /// Frame count at 30 fps.
    pub frames: u32,
    pub mode: SlotMode,
    pub speed: SpeedModel,
    pub events: Vec<AnimEvent>,
    pub root: RootPolicy,
    pub contacts: Vec<ContactWindow>,
}

/// Tolerance profile for the validator, per rule.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToleranceProfile {
    /// Frames of slack for event placement (rule T03).
    pub event_frame_tolerance: u32,
    /// Max root travel (units) for in-place clips (rule T04).
    pub in_place_travel: f32,
    /// Max joint-angle difference (degrees) for transition pops (rule T07).
    pub transition_pop_degrees: f32,
}

impl Default for ToleranceProfile {
    fn default() -> Self {
        Self {
            event_frame_tolerance: 1,
            in_place_travel: 2.0,
            transition_pop_degrees: 8.0,
        }
    }
}

/// The whole manifest: the contract every animation set must satisfy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimingManifest {
    pub fps: u32,
    pub slots: Vec<Slot>,
    pub tolerances: ToleranceProfile,
}

impl TimingManifest {
    pub fn slot(&self, id: &str) -> Option<&Slot> {
        self.slots.iter().find(|s| s.id == id)
    }
}

impl TimingManifest {
    /// Load a manifest from a TOML file (see `anim/timing_manifest.toml`).
    pub fn from_toml_str(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_lookup() {
        let m = TimingManifest {
            fps: 30,
            slots: vec![Slot {
                id: "walk_cycle".into(),
                frames: 24,
                mode: SlotMode::Loop,
                speed: SpeedModel::Fixed(1.0),
                events: vec![],
                root: RootPolicy::InPlace,
                contacts: vec![],
            }],
            tolerances: ToleranceProfile::default(),
        };
        assert!(m.slot("walk_cycle").is_some());
        assert!(m.slot("nope").is_none());
    }
}
