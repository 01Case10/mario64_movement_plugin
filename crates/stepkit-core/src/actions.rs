//! Data-driven action system.
//!
//! An [`ActionId`] maps to an [`ActionHandler`] in the [`ActionRegistry`].
//! Each action runs its documented cancel checks first, then its body, via
//! [`ActionCx`]. Custom actions register alongside the built-ins and get the
//! same step routines, events, and timeline access.

pub mod air;
pub mod ground;
pub mod water;

use crate::angles::Angle;
use crate::events::Event;
use crate::input::RawInput;
use crate::params::MovementParams;
use crate::state::{ActionId, CharacterState};
use crate::step::Timeline;
use crate::world::CollisionWorld;
use std::collections::BTreeMap;

/// What an action tick returns.
pub enum ActionResult {
    /// Stay in the current action.
    Stay,
    /// Transition now; carries the new action and its argument.
    Goto(ActionId, u32),
}

/// Per-tick context handed to action handlers.
pub struct ActionCx<'a> {
    /// Working copy of the state; the handler mutates it directly.
    pub state: CharacterState,
    pub input: RawInput,
    pub world: &'a dyn CollisionWorld,
    pub params: &'a MovementParams,
    pub timeline: Timeline,
    pub events: Vec<Event>,
}

impl<'a> ActionCx<'a> {
    /// World-space yaw the stick intends, if the stick is held.
    pub fn intended_yaw(&self) -> Option<Angle> {
        self.input.intended_yaw()
    }

    /// Stick magnitude in stick units.
    pub fn intended_magnitude(&self) -> f32 {
        self.input.intended_mag()
    }

    /// Raw (unreshaped) stick magnitude; used for thresholds the reference
    /// game applies pre-reshape (e.g. the dive stick check).
    pub fn raw_magnitude(&self) -> f32 {
        self.input.raw_magnitude()
    }

    /// True on the frame a button transitions from released to pressed.
    /// (Edge detection needs the previous input; the tick supplies it.)
    pub fn pressed(&self, button: u16, prev_buttons: u16) -> bool {
        self.input.buttons & !prev_buttons & button != 0
    }

    /// Transition to another action: books prev/action, resets the timer and
    /// action state, and emits [`Event::ActionChanged`].
    pub fn goto(&mut self, new: ActionId, arg: u32) -> ActionResult {
        let old = self.state.action;
        self.state.prev_action = old;
        self.state.action = new;
        self.state.action_timer = 0;
        self.state.action_state = 0;
        self.state.action_arg = arg;
        self.events.push(Event::ActionChanged { old, new });
        ActionResult::Goto(new, arg)
    }

    /// The argument passed to the current action by the last [`ActionCx::goto`].
    /// Actions that need their entry parameter (landing fall speed, knockback
    /// variant, ...) read it here instead of stashing copies.
    pub fn arg(&self) -> u32 {
        self.state.action_arg
    }

    /// Horizontal forward unit vector from the facing yaw.
    pub fn forward_xz(&self) -> (f32, f32) {
        let (s, c) = (
            crate::trig::sin(self.state.face_yaw),
            crate::trig::cos(self.state.face_yaw),
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
        r.register_builtin(ActionId::IDLE, ground::Idle);
        r.register_builtin(ActionId::WALKING, ground::Walking);
        r.register_builtin(ActionId::TURNING_AROUND, ground::TurningAround);
        r.register_builtin(ActionId::FINISH_TURNING_AROUND, ground::FinishTurningAround);
        r.register_builtin(ActionId::BRAKING, ground::Braking);
        r.register_builtin(ActionId::DECELERATING, ground::Decelerating);
        r.register_builtin(ActionId::JUMP_LAND, ground::JumpLand);
        r.register_builtin(ActionId::DOUBLE_JUMP_LAND, ground::DoubleJumpLand);
        r.register_builtin(ActionId::TRIPLE_JUMP_LAND, ground::TripleJumpLand);
        r.register_builtin(ActionId::BACKFLIP_LAND, ground::BackflipLand);
        r.register_builtin(ActionId::SIDE_FLIP_LAND, ground::SideFlipLand);
        r.register_builtin(ActionId::FREEFALL_LAND, ground::FreefallLand);
        r.register_builtin(ActionId::LONG_JUMP_LAND, ground::LongJumpLand);
        r.register_builtin(ActionId::BUTT_SLIDE, ground::ButtSlide);
        r.register_builtin(ActionId::STOMACH_SLIDE, ground::StomachSlide);
        r.register_builtin(ActionId::DIVE_SLIDE, ground::DiveSlide);
        r.register_builtin(ActionId::CROUCH_SLIDE, ground::CrouchSlide);
        r.register_builtin(ActionId::SLIDE_KICK_SLIDE, ground::SlideKickSlide);
        r.register_builtin(ActionId::JUMP, air::SingleJump);
        r.register_builtin(ActionId::DOUBLE_JUMP, air::DoubleJump);
        r.register_builtin(ActionId::TRIPLE_JUMP, air::TripleJump);
        r.register_builtin(ActionId::BACKFLIP, air::Backflip);
        r.register_builtin(ActionId::SIDE_FLIP, air::SideFlip);
        r.register_builtin(ActionId::LONG_JUMP, air::LongJump);
        r.register_builtin(ActionId::DIVE, air::Dive);
        r.register_builtin(ActionId::SLIDE_KICK, air::SlideKick);
        r.register_builtin(ActionId::FORWARD_ROLLOUT, air::ForwardRollout);
        r.register_builtin(ActionId::BACKWARD_ROLLOUT, air::BackwardRollout);
        r.register_builtin(ActionId::GROUND_POUND, air::GroundPound);
        r.register_builtin(ActionId::WALL_KICK, air::WallKick);
        r.register_builtin(ActionId::LEDGE_GRAB, air::LedgeGrab);
        r.register_builtin(ActionId::AIR_KNOCKBACK, air::AirKnockback);
        r.register_builtin(ActionId::FREEFALL, air::Freefall);
        r.register_builtin(ActionId::CROUCH, ground::Crouch);
        r.register_builtin(ActionId::CRAWL, ground::Crawl);
        r.register_builtin(water::id::WATER_PLUNGE, water::WaterPlunge);
        r.register_builtin(water::id::SWIMMING, water::Swimming);
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
            ActionId::JUMP,
            ActionId::SLIDE_KICK,
            ActionId::FREEFALL,
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
