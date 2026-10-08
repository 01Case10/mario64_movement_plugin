//! The complete simulation state: one small `Copy` struct.
//!
//! Everything the core needs to tick fits here, so states can be snapshotted,
//! hashed for determinism checks, and serialized for netplay later.

use crate::angles::Angle;
use glam::Vec3;

/// Action identifiers. Built-in IDs mirror the community-documented action
/// index where it is public knowledge (see `spec/actions/`); custom actions
/// start at `0x0100_0000` via [`ActionId::custom`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct ActionId(pub u32);

impl ActionId {
    // --- Verified against community documentation (see spec/provenance/ukikipedia-notes.md).
    // --- Every other ID below is STEPKIT-PROVISIONAL (bit 30 set, a bit the
    // --- community documents as unused): it is NOT a community value and will
    // --- be replaced if/when the community documents the real one.

    /// Idle: standing still on the ground.
    /// spec: actions.idle (verified: wiki:Idle@20401)
    pub const IDLE: ActionId = ActionId(0x0C400201);
    /// Walking: stick-held ground locomotion (covers all speeds; there is no
    /// separate community-documented "running" action).
    /// spec: actions.walking (PROVISIONAL -- wiki:Walking lists "todo")
    pub const WALKING: ActionId = ActionId(0x40000440);
    /// Turning around while moving.
    /// spec: actions.turning_around (verified: wiki:Turning Around@18182)
    pub const TURNING_AROUND: ActionId = ActionId(0x00000443);
    /// Braking: hard stop from speed >= 16 with neutral stick.
    /// spec: actions.braking (PROVISIONAL)
    pub const BRAKING: ActionId = ActionId(0x40000444);
    /// Decelerating: gentle stop from speed < 16 with neutral stick.
    /// spec: actions.decelerating (PROVISIONAL)
    pub const DECELERATING: ActionId = ActionId(0x40000445);
    /// Landing from a jump.
    /// spec: actions.landing (PROVISIONAL)
    pub const LANDING: ActionId = ActionId(0x40000446);
    /// Single jump, airborne.
    /// spec: actions.jump (verified: wiki:Single Jump@19309)
    pub const JUMP: ActionId = ActionId(0x03000880);
    /// Double jump, airborne.
    /// spec: actions.double_jump (verified: wiki:Double Jump@18964)
    pub const DOUBLE_JUMP: ActionId = ActionId(0x03000881);
    /// Triple jump, airborne.
    /// spec: actions.triple_jump (verified: wiki:Triple Jump@19300)
    pub const TRIPLE_JUMP: ActionId = ActionId(0x01000882);
    /// Backflip, airborne.
    /// spec: actions.backflip (verified: wiki:Backflip@19307)
    pub const BACKFLIP: ActionId = ActionId(0x01000883);
    /// Side flip, airborne.
    /// spec: actions.side_flip (verified: wiki:Side Flip@20374)
    pub const SIDE_FLIP: ActionId = ActionId(0x01000887);
    /// Long jump, airborne.
    /// spec: actions.long_jump (verified: wiki:Long Jump@14374)
    pub const LONG_JUMP: ActionId = ActionId(0x03000888);
    /// Dive, airborne.
    /// spec: actions.dive (verified: wiki:Dive@19303)
    pub const DIVE: ActionId = ActionId(0x0188088A);
    /// Freefall: walked off a ledge or jump expired.
    /// spec: actions.freefall (PROVISIONAL)
    pub const FREEFALL: ActionId = ActionId(0x4000088C);
    /// Ledge grab.
    /// spec: actions.ledge_grab (verified: wiki:Ledge Grab@19518)
    pub const LEDGE_GRAB: ActionId = ActionId(0x0800034B);

    /// Custom action IDs start here; the registry enforces the range.
    pub const CUSTOM_BASE: u32 = 0x0100_0000;

    pub fn custom(slot: u32) -> ActionId {
        ActionId(Self::CUSTOM_BASE + slot)
    }

    pub fn is_custom(self) -> bool {
        self.0 >= Self::CUSTOM_BASE && !self.is_provisional()
    }

    /// True for stepkit-provisional built-in IDs (bit 30 set): not community
    /// values, replaced when the community documents the real ones.
    pub fn is_provisional(self) -> bool {
        self.0 & 0x4000_0000 != 0
    }
}

/// The full per-character simulation state.
///
/// Positions and velocities are in SM64 units (the public documentation
/// gives the hitbox as radius 50, height 160). Angles are 16-bit.
#[derive(Clone, Copy, Debug)]
pub struct CharacterState {
    /// Feet position.
    pub pos: Vec3,
    /// Current velocity.
    pub vel: Vec3,
    /// Signed speed along the facing direction.
    pub forward_speed: f32,
    /// Facing yaw / pitch / roll.
    pub face_yaw: Angle,
    pub face_pitch: Angle,
    pub face_roll: Angle,
    /// Current and previous action.
    pub action: ActionId,
    pub prev_action: ActionId,
    /// Action-local state machine value, timer, and argument.
    pub action_state: u32,
    pub action_timer: u32,
    pub action_arg: u32,
    /// Floor and ceiling heights found on the last step (`None` if none).
    pub floor_y: Option<f32>,
    pub ceil_y: Option<f32>,
    /// Surface kind underfoot (index into the surface table).
    pub floor_kind: u8,
    /// Wall contact on the last step.
    pub wall_hit: bool,
    /// Wall-kick window timer (frames remaining).
    pub wall_kick_timer: u32,
    /// Gravity direction. Fixed to +Y in v1 (see open decision 7); stored so
    /// planetary gravity can be added later without rewriting every action.
    pub up: Vec3,
    /// Set by warps so the renderer skips interpolation across the jump.
    pub warped: bool,
}

impl Default for CharacterState {
    fn default() -> Self {
        Self {
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
            forward_speed: 0.0,
            face_yaw: Angle::ZERO,
            face_pitch: Angle::ZERO,
            face_roll: Angle::ZERO,
            action: ActionId::IDLE,
            prev_action: ActionId::IDLE,
            action_state: 0,
            action_timer: 0,
            action_arg: 0,
            floor_y: None,
            ceil_y: None,
            floor_kind: 0,
            wall_hit: false,
            wall_kick_timer: 0,
            up: Vec3::Y,
            warped: false,
        }
    }
}

impl CharacterState {
    /// Hash the state for cross-platform determinism checks (L0).
    /// Uses the raw bit patterns so NaNs and -0.0 are distinguished.
    pub fn hash_state(&self) -> u64 {
        use core::hash::{Hash, Hasher};
        let mut h = crate::state::fnv::Fnv1a64::new();
        for v in [
            self.pos.x, self.pos.y, self.pos.z, self.vel.x, self.vel.y, self.vel.z,
        ] {
            v.to_bits().hash(&mut h);
        }
        self.forward_speed.to_bits().hash(&mut h);
        self.face_yaw.0.hash(&mut h);
        self.face_pitch.0.hash(&mut h);
        self.face_roll.0.hash(&mut h);
        self.action.0.hash(&mut h);
        self.prev_action.0.hash(&mut h);
        self.action_state.hash(&mut h);
        self.action_timer.hash(&mut h);
        self.action_arg.hash(&mut h);
        self.floor_y.map(f32::to_bits).hash(&mut h);
        self.ceil_y.map(f32::to_bits).hash(&mut h);
        self.floor_kind.hash(&mut h);
        self.wall_hit.hash(&mut h);
        self.wall_kick_timer.hash(&mut h);
        h.finish()
    }
}

/// Minimal FNV-1a 64 used by [`CharacterState::hash_state`]; the core does
/// not depend on the platform hasher (which is intentionally randomized).
pub(crate) mod fnv {
    use core::hash::Hasher;
    pub struct Fnv1a64(u64);
    impl Fnv1a64 {
        pub fn new() -> Self {
            Self(0xcbf29ce484222325)
        }
    }
    impl Hasher for Fnv1a64 {
        fn write(&mut self, bytes: &[u8]) {
            for b in bytes {
                self.0 ^= *b as u64;
                self.0 = self.0.wrapping_mul(0x100000001b3);
            }
        }
        fn finish(&self) -> u64 {
            self.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_sane() {
        let s = CharacterState::default();
        assert_eq!(s.action, ActionId::IDLE);
        assert_eq!(s.up, Vec3::Y);
        assert!(!s.warped);
    }

    #[test]
    fn hash_is_stable_and_sensitive() {
        let a = CharacterState::default();
        let b = CharacterState::default();
        assert_eq!(a.hash_state(), b.hash_state());
        let mut c = a;
        c.pos.x = 0.5;
        assert_ne!(a.hash_state(), c.hash_state());
    }
}
