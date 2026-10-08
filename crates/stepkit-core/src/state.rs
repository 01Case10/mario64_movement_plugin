//! The complete simulation state: one small `Copy` struct.
//!
//! Everything the core needs to tick fits here, so states can be snapshotted,
//! hashed for determinism checks, and serialized for netplay later.

use crate::angles::Angle;
use glam::Vec3;

/// Action identifiers. Built-in IDs mirror the decomp-documented action
/// index (behavioral reference only; no decomp source is used in this
/// repo). A handful of IDs predate that reference and stay STEPKIT-
/// PROVISIONAL (bit 30 set, a bit the community documents as unused):
/// they are NOT community values and will be replaced if/when the
/// community documents the real ones.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct ActionId(pub u32);

impl ActionId {
    // --- Verified against the decomp action index (behavioral reference
    // --- only; see spec/provenance/ukikipedia-notes.md for the public-data
    // --- trail). A few IDs below remain STEPKIT-PROVISIONAL (bit 30 set):
    // --- NOT community values, replaced when the community documents them.

    /// Idle: standing still on the ground.
    /// spec: actions.idle (verified: wiki:Idle@20401)
    pub const IDLE: ActionId = ActionId(0x0C400201);
    /// Walking: stick-held ground locomotion (covers all speeds; there is no
    /// separate community-documented "running" action).
    /// spec: actions.walking (verified: decomp ACT_WALKING)
    pub const WALKING: ActionId = ActionId(0x04000440);
    /// Turning around while moving.
    /// spec: actions.turning_around (verified: wiki:Turning Around@18182)
    pub const TURNING_AROUND: ActionId = ActionId(0x00000443);
    /// Finish turning around: re-accelerate after the turn.
    /// spec: actions.finish_turning_around (verified: decomp ACT_FINISH_TURNING_AROUND)
    pub const FINISH_TURNING_AROUND: ActionId = ActionId(0x00000444);
    /// Braking: hard stop from speed >= 16 with neutral stick.
    /// spec: actions.braking (verified: decomp ACT_BRAKING)
    pub const BRAKING: ActionId = ActionId(0x04000445);
    /// Decelerating: gentle stop from speed < 16 with neutral stick.
    /// spec: actions.decelerating (verified: decomp ACT_DECELERATING)
    pub const DECELERATING: ActionId = ActionId(0x0400044A);
    /// Landing from a single jump.
    /// spec: actions.jump_land (verified: decomp ACT_JUMP_LAND)
    pub const JUMP_LAND: ActionId = ActionId(0x04000470);
    /// Landing from a freefall.
    /// spec: actions.freefall_land (verified: decomp ACT_FREEFALL_LAND)
    pub const FREEFALL_LAND: ActionId = ActionId(0x04000471);
    /// Landing from a double jump.
    /// spec: actions.double_jump_land (verified: decomp ACT_DOUBLE_JUMP_LAND)
    pub const DOUBLE_JUMP_LAND: ActionId = ActionId(0x04000472);
    /// Landing from a side flip.
    /// spec: actions.side_flip_land (verified: decomp ACT_SIDE_FLIP_LAND)
    pub const SIDE_FLIP_LAND: ActionId = ActionId(0x04000473);
    /// Landing from a triple jump (A press suppressed).
    /// spec: actions.triple_jump_land (verified: decomp ACT_TRIPLE_JUMP_LAND)
    pub const TRIPLE_JUMP_LAND: ActionId = ActionId(0x04000478);
    /// Landing from a long jump (ends crouching, keeps Z-stance).
    /// spec: actions.long_jump_land (verified: decomp ACT_LONG_JUMP_LAND)
    pub const LONG_JUMP_LAND: ActionId = ActionId(0x00000479);
    /// Landing from a backflip (A stripped unless Z held).
    /// spec: actions.backflip_land (verified: decomp ACT_BACKFLIP_LAND)
    pub const BACKFLIP_LAND: ActionId = ActionId(0x0400047A);
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
    /// Slide kick, airborne (B during a fast crouch slide).
    /// spec: actions.slide_kick (verified: decomp ACT_SLIDE_KICK)
    pub const SLIDE_KICK: ActionId = ActionId(0x018008AA);
    /// Forward rollout: exit from dive/slide-kick slides.
    /// spec: actions.forward_rollout (verified: decomp ACT_FORWARD_ROLLOUT)
    pub const FORWARD_ROLLOUT: ActionId = ActionId(0x010008A6);
    /// Backward rollout: exit from dive/slide-kick slides.
    /// spec: actions.backward_rollout (verified: decomp ACT_BACKWARD_ROLLOUT)
    pub const BACKWARD_ROLLOUT: ActionId = ActionId(0x010008AD);
    /// Freefall: walked off a ledge or jump expired.
    /// spec: actions.freefall (PROVISIONAL)
    pub const FREEFALL: ActionId = ActionId(0x4000088C);
    /// Ledge grab.
    /// spec: actions.ledge_grab (verified: wiki:Ledge Grab@19518)
    pub const LEDGE_GRAB: ActionId = ActionId(0x0800034B);
    /// Crouching (Z held on the ground).
    /// spec: actions.crouch (PROVISIONAL)
    pub const CROUCH: ActionId = ActionId(0x40000010);
    /// Crawling (stick held while crouching).
    /// spec: actions.crawl (PROVISIONAL)
    pub const CRAWL: ActionId = ActionId(0x40000011);
    /// Ground pound.
    /// spec: actions.ground_pound (PROVISIONAL -- no community page)
    pub const GROUND_POUND: ActionId = ActionId(0x400008A8);
    /// Wall kick.
    /// spec: actions.wall_kick (verified: wiki:Wall Kick rev 19311)
    pub const WALL_KICK: ActionId = ActionId(0x018808B0);
    /// Wall-kick flight: the airborne action after a wall kick.
    /// spec: actions.wall_kick_air (verified: decomp ACT_WALL_KICK_AIR)
    pub const WALL_KICK_AIR: ActionId = ActionId(0x03000886);
    /// Air hit wall: transient 2-frame bonk state entered on air wall hits
    /// at speed > 16. A in the window -> wall kick; then knockback/soft-bonk.
    /// spec: actions.air_hit_wall (verified: decomp ACT_AIR_HIT_WALL)
    pub const AIR_HIT_WALL: ActionId = ActionId(0x000008A7);
    /// Steep jump: jump from a very steep floor (normal.y < 0.2924).
    /// spec: actions.steep_jump (verified: decomp ACT_STEEP_JUMP)
    pub const STEEP_JUMP: ActionId = ActionId(0x03000885);
    /// Soft bonk: low-speed air wall-hit outcome; keeps forward speed.
    /// spec: actions.soft_bonk (verified: decomp ACT_SOFT_BONK)
    pub const SOFT_BONK: ActionId = ActionId(0x010208B6);
    /// Backwards air knockback (bonk).
    /// spec: actions.backward_air_kb (verified: decomp ACT_BACKWARD_AIR_KB)
    pub const BACKWARD_AIR_KB: ActionId = ActionId(0x010208B0);
    /// Forwards air knockback.
    /// spec: actions.forward_air_kb (verified: decomp ACT_FORWARD_AIR_KB)
    pub const FORWARD_AIR_KB: ActionId = ActionId(0x010208B1);
    /// Hard backwards air knockback (arg 1 = hard; Phase E consumes it).
    /// spec: actions.hard_backward_air_kb (verified: decomp)
    pub const HARD_BACKWARD_AIR_KB: ActionId = ActionId(0x010208B2);
    /// Hard forwards air knockback (arg 1 = hard; Phase E consumes it).
    /// spec: actions.hard_forward_air_kb (verified: decomp)
    pub const HARD_FORWARD_AIR_KB: ActionId = ActionId(0x010208B3);
    /// Butt slide.
    /// spec: actions.butt_slide (verified: decomp ACT_BUTT_SLIDE)
    pub const BUTT_SLIDE: ActionId = ActionId(0x00840452);
    /// Stomach slide (slide trigger while facing uphill/flat).
    /// spec: actions.stomach_slide (verified: decomp ACT_STOMACH_SLIDE)
    pub const STOMACH_SLIDE: ActionId = ActionId(0x008C0453);
    /// Dive slide: landing from a dive.
    /// spec: actions.dive_slide (verified: decomp ACT_DIVE_SLIDE)
    pub const DIVE_SLIDE: ActionId = ActionId(0x00880456);
    /// Crouch slide: Z while moving.
    /// spec: actions.crouch_slide (verified: decomp ACT_CROUCH_SLIDE)
    pub const CROUCH_SLIDE: ActionId = ActionId(0x04808459);
    /// Slide-kick slide: ground continuation of an airborne slide kick.
    /// spec: actions.slide_kick_slide (verified: decomp ACT_SLIDE_KICK_SLIDE)
    pub const SLIDE_KICK_SLIDE: ActionId = ActionId(0x0080045A);
    /// Waist-deep in quicksand: can still jump and crouch.
    /// spec: actions.in_quicksand (verified: decomp ACT_IN_QUICKSAND)
    pub const IN_QUICKSAND: ActionId = ActionId(0x0002020D);
    /// Landing from a jump while deep in quicksand: 13-frame escape.
    /// spec: actions.quicksand_jump_land (verified: decomp ACT_QUICKSAND_JUMP_LAND)
    pub const QUICKSAND_JUMP_LAND: ActionId = ActionId(0x04000476);
    /// Fast ledge pull-up (A with headroom).
    /// spec: actions.ledge_climb_fast (verified: decomp ACT_LEDGE_CLIMB_FAST)
    pub const LEDGE_CLIMB_FAST: ActionId = ActionId(0x0000054F);
    /// Slow ledge climb, part 1 (stick toward the wall).
    /// spec: actions.ledge_climb_slow_1 (verified: decomp ACT_LEDGE_CLIMB_SLOW_1)
    pub const LEDGE_CLIMB_SLOW_1: ActionId = ActionId(0x0000054C);
    /// Slow ledge climb, part 2.
    /// spec: actions.ledge_climb_slow_2 (verified: decomp ACT_LEDGE_CLIMB_SLOW_2)
    pub const LEDGE_CLIMB_SLOW_2: ActionId = ActionId(0x0000054D);

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
    /// Sliding velocity vector (independent of facing for authentic slides).
    pub slide_vel_x: f32,
    pub slide_vel_z: f32,
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
    /// Normal of the last wall hit (points away from the wall).
    pub wall_normal: Vec3,
    /// The air action the last landing came from (for jump chaining).
    pub land_from: ActionId,
    /// Downward speed at the last landing (from the landing goto arg).
    /// Consumed by fall-damage/squish logic.
    pub last_fall_speed: f32,
    /// Health. Full is 0x880 (2176); death below 0x100.
    /// spec: damage.health_full
    pub health: i32,
    /// Frames of squish remaining (0 = not squished). While > 0 the
    /// intended stick magnitude is quartered, jump velocity halved, and
    /// double/triple jump chains are suppressed.
    pub squish_timer: u32,
    /// Highest Y reached during the current airtime; fall damage compares
    /// it against the landing height. Reset on landing and by ground
    /// actions.
    pub peak_height: f32,
    /// Quicksand sink depth. Grows while standing on quicksand, resets to
    /// 0 on other floors.
    pub quicksand_depth: f32,
    /// Slide speed-cap latch: the 100-unit cap applies one frame late, so
    /// a slide may exceed it for exactly one tick.
    pub slide_over_cap: bool,
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
            slide_vel_x: 0.0,
            slide_vel_z: 0.0,
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
            wall_normal: Vec3::X,
            land_from: ActionId::IDLE,
            last_fall_speed: 0.0,
            health: 0x880, // full health (verified: decomp health model)
            squish_timer: 0,
            peak_height: 0.0,
            quicksand_depth: 0.0,
            slide_over_cap: false,
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
        self.slide_vel_x.to_bits().hash(&mut h);
        self.slide_vel_z.to_bits().hash(&mut h);
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
        self.wall_normal.x.to_bits().hash(&mut h);
        self.wall_normal.y.to_bits().hash(&mut h);
        self.wall_normal.z.to_bits().hash(&mut h);
        self.land_from.0.hash(&mut h);
        self.last_fall_speed.to_bits().hash(&mut h);
        self.health.hash(&mut h);
        self.squish_timer.hash(&mut h);
        self.peak_height.to_bits().hash(&mut h);
        self.quicksand_depth.to_bits().hash(&mut h);
        self.slide_over_cap.hash(&mut h);
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
