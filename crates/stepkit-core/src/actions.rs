//! Data-driven action system.
//!
//! An [`ActionId`] maps to an [`ActionHandler`] in the [`ActionRegistry`].
//! Each action runs its documented cancel checks first, then its body, via
//! [`ActionCx`]. Custom actions register alongside the built-ins and get the
//! same step routines, events, and timeline access.

pub mod air;
pub mod air_dive;
pub mod air_kb;
pub mod air_ledge;
pub mod air_jump;
pub mod air_model;
pub mod entry;
pub mod extra_air;
pub mod ground;
pub mod idle_crouch;
pub mod knockback;
pub mod landings;
pub mod punch;
pub mod quicksand;
pub mod slides;
pub mod walk;
pub mod water_combat;
pub mod water_metal;
pub mod water_physics;
pub mod water_swim;
pub mod water;

use crate::angles::Angle;
use crate::events::Event;
use crate::input::{input_flags, ProcessedInput, RawInput};
use crate::params::MovementParams;
use crate::state::{ActionId, CharacterState};
use crate::step::Timeline;
use crate::world::CollisionWorld;
use std::collections::BTreeMap;

/// What an action tick returns.
pub enum ActionResult {
    /// Stay in the current action.
    Stay,
    /// Transition now AND run the new action in the same tick. This is what
    /// the decomp's action loop does when the action function returns TRUE
    /// (a cancel/transition path that propagates the return value of
    /// `set_mario_action`).
    Goto(ActionId, u32),
    /// Transition now but do NOT run the new action until the next tick.
    /// This is what the decomp does when an action function calls
    /// `set_mario_action` and then returns FALSE (all airborne actions'
    /// step results, and the ground actions' `LEFT_GROUND`/`HIT_WALL`
    /// branches, which fall through to `return FALSE`).
    GotoNext(ActionId, u32),
}

/// Per-tick context handed to action handlers.
pub struct ActionCx<'a> {
    /// Working copy of the state; the handler mutates it directly.
    pub state: CharacterState,
    pub input: RawInput,
    /// The decomp's per-frame input conditioning ([`ProcessedInput`]),
    /// computed once per tick before the action runs.
    pub proc: ProcessedInput,
    pub world: &'a dyn CollisionWorld,
    pub params: &'a MovementParams,
    pub timeline: Timeline,
    pub events: Vec<Event>,
}

impl<'a> ActionCx<'a> {
    /// Set forward speed and sync the slide/linear velocities from the
    /// facing yaw (decomp: mario_set_forward_vel, mario.c:372).
    pub fn set_forward_speed(&mut self, v: f32) {
        self.state.forward_speed = v;
        let sx = crate::trig::sins(self.state.face_yaw);
        let sz = crate::trig::coss(self.state.face_yaw);
        self.state.slide_vel_x = sx * v;
        self.state.slide_vel_z = sz * v;
        self.state.vel.x = sx * v;
        self.state.vel.z = sz * v;
    }

    /// World-space yaw the stick intends, if the stick is held
    /// (`INPUT_NONZERO_ANALOG`).
    pub fn intended_yaw(&self) -> Option<Angle> {
        if self.proc.input & input_flags::NONZERO_ANALOG != 0 {
            Some(self.proc.intended_yaw)
        } else {
            None
        }
    }

    /// Intended speed magnitude, with the squish penalty already applied by
    /// `update_mario_joystick_inputs` (eighthed while squished).
    pub fn intended_magnitude(&self) -> f32 {
        self.proc.intended_mag
    }

    /// Post-cap stick magnitude (`controller->stickMag`, 0..64); the dive
    /// check and other thresholds read this, not the raw magnitude.
    pub fn stick_mag(&self) -> f32 {
        self.proc.stick_mag
    }

    /// True when the stick counts as held (`INPUT_NONZERO_ANALOG`).
    pub fn stick_held(&self) -> bool {
        self.proc.input & input_flags::NONZERO_ANALOG != 0
    }

    /// True on the frame a button transitions from released to pressed.
    /// Edge detection comes from the processed input; the `prev_buttons`
    /// parameter is kept so call sites stay mechanical (it is ignored).
    pub fn pressed(&self, button: u16, _prev_buttons: u16) -> bool {
        self.proc.button_pressed & button != 0
    }

    /// Transition now; carries the new action and its argument. The new
    /// action runs in the same tick (see [`ActionResult::Goto`]).
    pub fn goto(&mut self, new: ActionId, arg: u32) -> ActionResult {
        self.transition(new, arg, true)
    }

    /// Transition now; the new action first runs on the next tick (see
    /// [`ActionResult::GotoNext`]).
    pub fn goto_next(&mut self, new: ActionId, arg: u32) -> ActionResult {
        self.transition(new, arg, false)
    }

    fn transition(&mut self, new: ActionId, arg: u32, same_tick: bool) -> ActionResult {
        // decomp: `set_mario_action` (mario.c:979) routes every moving-group
        // transition through `set_mario_action_moving` (mario.c:896): the
        // WALKING entry-speed clamp and the BEGIN_SLIDING butt/stomach
        // dispatcher. Applying it here — rather than at each call site —
        // matches the decomp, where no moving transition can bypass it.
        // (Callers that already remapped explicitly are unaffected: the
        // remap is idempotent.)
        let new = if new.0 & crate::state::act_flags::GROUP_MASK
            == crate::state::act_flags::GROUP_MOVING
        {
            entry::set_mario_action_moving(self, new)
        } else {
            new
        };
        // decomp: set_mario_action clears the sound-played flags (mario.c:999).
        self.state.flags &= !(crate::state::mario_flags::ACTION_SOUND_PLAYED
            | crate::state::mario_flags::MARIO_SOUND_PLAYED);
        let old = self.state.action;
        self.state.prev_action = old;
        self.state.action = new;
        self.state.action_timer = 0;
        self.state.action_state = 0;
        self.state.action_arg = arg;
        self.events.push(Event::ActionChanged { old, new });
        if same_tick {
            ActionResult::Goto(new, arg)
        } else {
            ActionResult::GotoNext(new, arg)
        }
    }

    /// The argument passed to the current action by the last [`ActionCx::goto`].
    /// Actions that need their entry parameter (landing fall speed, knockback
    /// variant, ...) read it here instead of stashing copies.
    pub fn arg(&self) -> u32 {
        self.state.action_arg
    }

    /// Request an animation (no acceleration); returns the current frame.
    /// decomp: `set_mario_animation` (`src/game/mario.c:64`).
    pub fn set_mario_animation(&mut self, target_anim_id: i32) -> i16 {
        crate::anim::set_mario_animation(&mut self.state, target_anim_id)
    }

    /// Request an animation with frame acceleration; returns the current frame.
    /// decomp: `set_mario_anim_with_accel` (`src/game/mario.c:97`).
    pub fn set_mario_anim_with_accel(&mut self, target_anim_id: i32, accel: i32) -> i16 {
        crate::anim::set_mario_anim_with_accel(&mut self.state, target_anim_id, accel)
    }

    /// True when the animation is on its last frame.
    /// decomp: `is_anim_at_end` (`src/game/mario.c:46`).
    pub fn is_anim_at_end(&self) -> bool {
        crate::anim::is_anim_at_end(&self.state)
    }

    /// True when the animation reached (or passed) 2 frames before its end.
    /// decomp: `is_anim_past_end` (`src/game/mario.c:55`).
    pub fn is_anim_past_end(&self) -> bool {
        crate::anim::is_anim_past_end(&self.state)
    }

    /// True when the animation just passed `anim_frame`.
    /// decomp: `is_anim_past_frame` (`src/game/mario.c:151`).
    pub fn is_anim_past_frame(&self, anim_frame: i16) -> bool {
        crate::anim::is_anim_past_frame(&self.state, anim_frame)
    }

    /// Horizontal forward unit vector from the facing yaw.
    pub fn forward_xz(&self) -> (f32, f32) {
        let (s, c) = (
            crate::trig::sins(self.state.face_yaw),
            crate::trig::coss(self.state.face_yaw),
        );
        (s, c)
    }
}

/// One action's per-frame behavior.
pub trait ActionHandler: Send + Sync {
    /// Run the documented cancel checks first, then the action body.
    /// `prev_buttons` is the previous frame's button bitfield for edge detection.
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult;
    fn name(&self) -> &'static str;
}

/// Maps action IDs to handlers. `BTreeMap` keeps iteration deterministic.
pub struct ActionRegistry {
    handlers: BTreeMap<u32, Box<dyn ActionHandler>>,
}

impl ActionRegistry {
    pub fn new() -> Self {
        Self {
            handlers: BTreeMap::new(),
        }
    }

    /// All built-in actions.
    pub fn sm64_style() -> Self {
        let mut r = Self::new();
        r.register_builtin(ActionId::IDLE, idle_crouch::Idle);
        r.register_builtin(ActionId::WALKING, walk::Walking);
        r.register_builtin(ActionId::TURNING_AROUND, walk::TurningAround);
        r.register_builtin(ActionId::FINISH_TURNING_AROUND, walk::FinishTurningAround);
        r.register_builtin(ActionId::BRAKING, walk::Braking);
        r.register_builtin(ActionId::DECELERATING, walk::Decelerating);
        // §4.7 stationary actions new in Phase 5.
        r.register_builtin(ActionId::BEGIN_SLIDING, slides::BeginSliding);
        r.register_builtin(ActionId::BRAKING_STOP, walk::BrakingStop);
        r.register_builtin(ActionId::BUTT_SLIDE_STOP, slides::ButtSlideStop);
        r.register_builtin(ActionId::SLIDE_KICK_SLIDE_STOP, slides::SlideKickSlideStop);
        r.register_builtin(ActionId::STOMACH_SLIDE_STOP, slides::StomachSlideStop);
        r.register_builtin(ActionId::START_CROUCHING, idle_crouch::StartCrouching);
        r.register_builtin(ActionId::STOP_CROUCHING, idle_crouch::StopCrouching);
        r.register_builtin(ActionId::START_CRAWLING, idle_crouch::StartCrawling);
        r.register_builtin(ActionId::STOP_CRAWLING, idle_crouch::StopCrawling);
        r.register_builtin(ActionId::STANDING_AGAINST_WALL, walk::StandingAgainstWall);
        r.register_builtin(ActionId::LEDGE_CLIMB_DOWN, walk::LedgeClimbDown);
        r.register_builtin(ActionId::STANDING_DEATH, walk::StandingDeath);
        r.register_builtin(ActionId::SHOCKWAVE_BOUNCE, walk::ShockwaveBounce);
        r.register_builtin(ActionId::SQUISHED, walk::Squished);
        r.register_builtin(ActionId::GROUND_POUND_LAND, landings::GroundPoundLand);
        r.register_builtin(ActionId::JUMP_LAND, landings::JumpLand);
        r.register_builtin(ActionId::JUMP_LAND_STOP, landings::JumpLandStop);
        r.register_builtin(ActionId::DOUBLE_JUMP_LAND_STOP, landings::DoubleJumpLandStop);
        r.register_builtin(ActionId::SIDE_FLIP_LAND_STOP, landings::SideFlipLandStop);
        r.register_builtin(ActionId::TRIPLE_JUMP_LAND_STOP, landings::TripleJumpLandStop);
        r.register_builtin(ActionId::BACKFLIP_LAND_STOP, landings::BackflipLandStop);
        r.register_builtin(ActionId::LONG_JUMP_LAND_STOP, landings::LongJumpLandStop);
        r.register_builtin(ActionId::FREEFALL_LAND_STOP, landings::FreefallLandStop);
        r.register_builtin(ActionId::DOUBLE_JUMP_LAND, landings::DoubleJumpLand);
        r.register_builtin(ActionId::TRIPLE_JUMP_LAND, landings::TripleJumpLand);
        r.register_builtin(ActionId::BACKFLIP_LAND, landings::BackflipLand);
        r.register_builtin(ActionId::SIDE_FLIP_LAND, landings::SideFlipLand);
        r.register_builtin(ActionId::FREEFALL_LAND, landings::FreefallLand);
        r.register_builtin(ActionId::LONG_JUMP_LAND, landings::LongJumpLand);
        r.register_builtin(ActionId::BUTT_SLIDE, slides::ButtSlide);
        r.register_builtin(ActionId::STOMACH_SLIDE, slides::StomachSlide);
        r.register_builtin(ActionId::DIVE_SLIDE, slides::DiveSlide);
        r.register_builtin(ActionId::CROUCH_SLIDE, slides::CrouchSlide);
        r.register_builtin(ActionId::SLIDE_KICK_SLIDE, slides::SlideKickSlide);
        r.register_builtin(ActionId::IN_QUICKSAND, quicksand::InQuicksand);
        r.register_builtin(ActionId::QUICKSAND_JUMP_LAND, quicksand::QuicksandJumpLand);
        r.register_builtin(ActionId::PUNCHING, punch::Punching);
        r.register_builtin(ActionId::MOVE_PUNCHING, punch::MovePunching);
        r.register_builtin(ActionId::BACKWARD_GROUND_KB, knockback::BackwardGroundKb);
        r.register_builtin(ActionId::FORWARD_GROUND_KB, knockback::ForwardGroundKb);
        r.register_builtin(
            ActionId::HARD_BACKWARD_GROUND_KB,
            knockback::HardBackwardGroundKb,
        );
        r.register_builtin(
            ActionId::HARD_FORWARD_GROUND_KB,
            knockback::HardForwardGroundKb,
        );
        r.register_builtin(
            ActionId::SOFT_BACKWARD_GROUND_KB,
            knockback::SoftBackwardGroundKb,
        );
        r.register_builtin(
            ActionId::SOFT_FORWARD_GROUND_KB,
            knockback::SoftForwardGroundKb,
        );
        r.register_builtin(ActionId::GROUND_BONK, knockback::GroundBonk);
        r.register_builtin(ActionId::JUMP, air_jump::Jump);
        r.register_builtin(ActionId::DOUBLE_JUMP, air_jump::DoubleJump);
        r.register_builtin(ActionId::TRIPLE_JUMP, air_jump::TripleJump);
        r.register_builtin(ActionId::BACKFLIP, air_jump::Backflip);
        r.register_builtin(ActionId::SIDE_FLIP, air_jump::SideFlip);
        r.register_builtin(ActionId::LONG_JUMP, air_jump::LongJump);
        r.register_builtin(ActionId::DIVE, air_dive::Dive);
        r.register_builtin(ActionId::SLIDE_KICK, air_dive::SlideKick);
        r.register_builtin(ActionId::FORWARD_ROLLOUT, air_dive::ForwardRollout);
        r.register_builtin(ActionId::BACKWARD_ROLLOUT, air_dive::BackwardRollout);
        r.register_builtin(ActionId::GROUND_POUND, air_dive::GroundPound);
        r.register_builtin(ActionId::WALL_KICK_AIR, air_jump::WallKickAir);
        r.register_builtin(ActionId::AIR_HIT_WALL, air_kb::AirHitWall);
        r.register_builtin(ActionId::STEEP_JUMP, air_jump::SteepJump);
        r.register_builtin(ActionId::SOFT_BONK, air_kb::SoftBonk);
        r.register_builtin(ActionId::BACKWARD_AIR_KB, air_kb::BackwardAirKb);
        r.register_builtin(ActionId::HARD_BACKWARD_AIR_KB, air_kb::HardBackwardAirKb);
        r.register_builtin(ActionId::FORWARD_AIR_KB, air_kb::ForwardAirKb);
        r.register_builtin(ActionId::HARD_FORWARD_AIR_KB, air_kb::HardForwardAirKb);
        r.register_builtin(ActionId::JUMP_KICK, air_dive::JumpKick);
        r.register_builtin(ActionId::LEDGE_GRAB, air_ledge::LedgeGrab);
        r.register_builtin(ActionId::LEDGE_CLIMB_FAST, air_ledge::LedgeClimbFast);
        r.register_builtin(ActionId::LEDGE_CLIMB_SLOW_1, air_ledge::LedgeClimbSlow);
        r.register_builtin(ActionId::LEDGE_CLIMB_SLOW_2, air_ledge::LedgeClimbSlow);
        r.register_builtin(ActionId::LEDGE_CLIMB_DOWN, air_ledge::LedgeClimbDown);
        r.register_builtin(ActionId::TWIRLING, extra_air::Twirling);
        r.register_builtin(ActionId::TWIRL_LAND, extra_air::TwirlLand);
        r.register_builtin(ActionId::BUTT_SLIDE_AIR, extra_air::ButtSlideAir);
        r.register_builtin(ActionId::START_HANGING, extra_air::StartHanging);
        r.register_builtin(ActionId::HANGING, extra_air::Hanging);
        r.register_builtin(ActionId::HANG_MOVING, extra_air::Hanging); // TODO: separate HangMoving
        r.register_builtin(ActionId::FREEFALL, air_kb::Freefall);
        r.register_builtin(ActionId::CROUCH, idle_crouch::Crouch);
        r.register_builtin(ActionId::CRAWL, idle_crouch::Crawl);
        r.register_builtin(ActionId::WATER_PLUNGE, water_swim::ActWaterPlunge);
        r.register_builtin(ActionId::WATER_IDLE, water_swim::ActWaterIdle);
        r.register_builtin(ActionId::WATER_ACTION_END, water_swim::ActWaterActionEnd);
        r.register_builtin(ActionId::BREASTSTROKE, water_swim::ActBreaststroke);
        r.register_builtin(ActionId::SWIMMING_END, water_swim::ActSwimmingEnd);
        r.register_builtin(ActionId::FLUTTER_KICK, water_swim::ActFlutterKick);
        r.register_builtin(ActionId::WATER_PUNCH, water_combat::WaterPunch);
        r.register_builtin(ActionId::BACKWARD_WATER_KB, water_combat::BackwardWaterKb);
        r.register_builtin(ActionId::FORWARD_WATER_KB, water_combat::ForwardWaterKb);
        r.register_builtin(ActionId::WATER_SHOCKED, water_combat::WaterShocked);
        r.register_builtin(ActionId::DROWNING, water_combat::Drowning);
        r.register_builtin(ActionId::WATER_DEATH, water_combat::WaterDeath);
        r.register_builtin(ActionId::METAL_WATER_STANDING, water_metal::MetalWaterStanding);
        r.register_builtin(ActionId::METAL_WATER_WALKING, water_metal::MetalWaterWalking);
        r.register_builtin(ActionId::METAL_WATER_JUMP, water_metal::MetalWaterJump);
        r.register_builtin(ActionId::METAL_WATER_FALLING, water_metal::MetalWaterFalling);
        r.register_builtin(ActionId::METAL_WATER_JUMP_LAND, water_metal::MetalWaterJumpLand);
        r.register_builtin(ActionId::WATER_JUMP, air::WaterJump);
        r
    }

    /// Register a custom action. Panics if the ID is not in the custom range.
    pub fn register(&mut self, id: ActionId, handler: impl ActionHandler + 'static) {
        assert!(
            id.is_custom(),
            "custom actions must use ActionId::custom(n); got {:#010x}",
            id.0
        );
        self.handlers.insert(id.0, Box::new(handler));
    }

    fn register_builtin(&mut self, id: ActionId, handler: impl ActionHandler + 'static) {
        self.handlers.insert(id.0, Box::new(handler));
    }

    pub fn get(&self, id: ActionId) -> Option<&dyn ActionHandler> {
        self.handlers.get(&id.0).map(|h| h.as_ref())
    }

    pub fn action_name(&self, id: ActionId) -> &'static str {
        self.get(id).map_or("<unknown>", |h| h.name())
    }

    /// Look up an action ID by handler name (e.g. "LedgeGrab").
    /// Used for scenario `start.action` and semantic assertions.
    pub fn id_by_name(&self, name: &str) -> Option<ActionId> {
        self.handlers
            .iter()
            .find_map(|(&id, h)| (h.name() == name).then_some(ActionId(id)))
    }
}

impl Default for ActionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_actions_registered() {
        let r = ActionRegistry::sm64_style();
        for id in [
            ActionId::IDLE,
            ActionId::WALKING,
            ActionId::TURNING_AROUND,
            ActionId::FINISH_TURNING_AROUND,
            ActionId::BRAKING,
            ActionId::DECELERATING,
            ActionId::JUMP_LAND,
            ActionId::FREEFALL_LAND,
            ActionId::DIVE_SLIDE,
            ActionId::CROUCH_SLIDE,
            ActionId::STOMACH_SLIDE,
            ActionId::WALL_KICK_AIR,
            ActionId::AIR_HIT_WALL,
            ActionId::STEEP_JUMP,
            ActionId::SOFT_BONK,
            ActionId::BACKWARD_AIR_KB,
            ActionId::FORWARD_AIR_KB,
            ActionId::JUMP_KICK,
            ActionId::PUNCHING,
            ActionId::MOVE_PUNCHING,
            ActionId::BACKWARD_GROUND_KB,
            ActionId::FORWARD_GROUND_KB,
            ActionId::HARD_BACKWARD_GROUND_KB,
            ActionId::HARD_FORWARD_GROUND_KB,
            ActionId::SOFT_BACKWARD_GROUND_KB,
            ActionId::SOFT_FORWARD_GROUND_KB,
            ActionId::GROUND_BONK,
            ActionId::JUMP,
            ActionId::SLIDE_KICK,
            ActionId::FREEFALL,
            ActionId::WATER_JUMP,
            ActionId::WATER_PLUNGE,
            ActionId::WATER_IDLE,
            ActionId::WATER_ACTION_END,
            ActionId::BREASTSTROKE,
            ActionId::SWIMMING_END,
            ActionId::FLUTTER_KICK,
        ] {
            assert!(r.get(id).is_some(), "{id:?}");
        }
    }

    #[test]
    fn custom_registration() {
        struct Spin;
        impl ActionHandler for Spin {
            fn tick(&self, _cx: &mut ActionCx, _p: u16) -> ActionResult {
                ActionResult::Stay
            }
            fn name(&self) -> &'static str {
                "Spin"
            }
        }
        let mut r = ActionRegistry::new();
        r.register(ActionId::custom(0), Spin);
        assert_eq!(r.action_name(ActionId::custom(0)), "Spin");
    }
}
