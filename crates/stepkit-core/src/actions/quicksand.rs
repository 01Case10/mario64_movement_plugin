//! Quicksand actions, verbatim from the decomp.
//!
//! - `act_in_quicksand` (decomp: src/game/mario_actions_stationary.c:509),
//!   with its `check_common_idle_cancels` preamble inlined as a private copy
//!   (decomp: src/game/mario_actions_stationary.c:20).
//! - `quicksand_jump_land_action` (decomp: src/game/mario_actions_moving.c:1914)
//!   and `act_quicksand_jump_land` (decomp: src/game/mario_actions_moving.c:1940).
//!
//! Layering note: in the decomp these actions never run the group preamble
//! themselves. `mario_execute_stationary_action` (stationary.c:1106) runs
//! `check_common_stationary_cancels` and then `mario_update_quicksand(m,
//! 0.5f)` before dispatching to `act_in_quicksand`; the moving group runs
//! `mario_update_quicksand(m, 0.25f)` before its actions. That group-level
//! wiring belongs to the dispatch layer, not to these handlers, so it is not
//! duplicated here. `sink_mario_in_quicksand` (mario.c:1545) only subtracts
//! `quicksandDepth` from the graphical position (`o->header.gfx.pos[1]` and
//! the throw matrix) — visual-only, renderer-side.

use super::{ActionCx, ActionHandler, ActionResult};
use crate::anim_table as anim_id;
use crate::input::{buttons, input_flags};
use crate::state::ActionId;
use crate::step::{
    mario_push_off_steep_floor, perform_ground_step, stationary_ground_step, GroundStep,
};

/// Private copy of `check_common_idle_cancels`
/// (decomp: src/game/mario_actions_stationary.c:20).
///
/// Transitions use `cx.goto` (the decomp's `set_mario_action` returns TRUE →
/// same tick), except the jump branch which propagates `set_jumping_action`'s
/// own result.
fn check_common_idle_cancels(cx: &mut ActionCx, prev_buttons: u16) -> Option<ActionResult> {
    // mario_drop_held_object: no held objects in scope (no-op).
    // m->floor is guaranteed non-NULL by the callers in the decomp.
    if let Some(floor) = cx.state.floor {
        if floor.normal.y < 0.29237169 {
            mario_push_off_steep_floor(&mut cx.state);
            return Some(cx.goto(ActionId::FREEFALL, 0));
        }
    }

    if cx.state.input & input_flags::STOMPED != 0 {
        return Some(cx.goto(ActionId::SHOCKWAVE_BOUNCE, 0));
    }

    if cx.pressed(buttons::A, prev_buttons) {
        return Some(super::entry::set_jumping_action(cx, ActionId::JUMP, 0));
    }

    if cx.state.input & input_flags::OFF_FLOOR != 0 {
        return Some(cx.goto(ActionId::FREEFALL, 0));
    }

    if cx.state.input & input_flags::ABOVE_SLIDE != 0 {
        return Some(cx.goto(ActionId::BEGIN_SLIDING, 0));
    }

    if cx.state.input & input_flags::FIRST_PERSON != 0 {
        // decomp ACT_FIRST_PERSON (0x0C000227, oracle/sm64/include/sm64.h:195).
        // The crate has no ActionId constant or handler for it yet, and the
        // flag is never set by the current input processing.
        return Some(cx.goto(ActionId::FIRST_PERSON, 0));
    }

    if cx.stick_held() {
        cx.state.face_yaw = cx.proc.intended_yaw;
        return Some(cx.goto(ActionId::WALKING, 0));
    }

    if cx.pressed(buttons::B, prev_buttons) {
        return Some(cx.goto(ActionId::PUNCHING, 0));
    }

    if cx.input.buttons & buttons::Z != 0 {
        return Some(cx.goto(ActionId::START_CROUCHING, 0));
    }

    None
}

/// decomp: `act_in_quicksand` (src/game/mario_actions_stationary.c:509).
///
/// Note: the decomp function has no eye-state logic and no dedicated Z/A
/// handling of its own — the Z_DOWN (crouch) and A_PRESSED (jump) branches
/// live in `check_common_idle_cancels`, inlined above.
fn act_in_quicksand(cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
    if cx.state.quicksand_depth < 30.0 {
        return cx.goto(ActionId::IDLE, 0);
    }

    if let Some(r) = check_common_idle_cancels(cx, prev_buttons) {
        return r;
    }

    if cx.state.quicksand_depth > 70.0 {
        cx.set_mario_animation(anim_id::MARIO_ANIM_DYING_IN_QUICKSAND);
    } else {
        cx.set_mario_animation(anim_id::MARIO_ANIM_IDLE_IN_QUICKSAND);
    }

    stationary_ground_step(&mut cx.state, cx.world);
    ActionResult::Stay
}

/// decomp: `quicksand_jump_land_action`
/// (src/game/mario_actions_moving.c:1914).
///
/// This is NOT the `common_landing_action` shape: for the first 6 ticks it
/// drains `quicksandDepth` by `(7 - actionTimer) * 0.8` (floored back up to
/// 1.1), plays the jump sound and `animation1`; from tick 6 it plays
/// `animation2` until tick 13, when it transitions to `end_action`. Then it
/// applies the 0.95x landing accel and steps, dropping to `air_action` on
/// the next tick if the step leaves the ground.
pub fn quicksand_jump_land_action(
    cx: &mut ActionCx,
    animation1: i32,
    animation2: i32,
    end_action: ActionId,
    air_action: ActionId,
) -> ActionResult {
    // decomp: `if (m->actionTimer++ < 6)` — post-increment compare; the depth
    // term below uses the post-increment value.
    let timer = cx.state.action_timer;
    cx.state.action_timer += 1;
    if timer < 6 {
        cx.state.quicksand_depth -= (7 - cx.state.action_timer) as f32 * 0.8;
        if cx.state.quicksand_depth < 1.0 {
            cx.state.quicksand_depth = 1.1;
        }

        // decomp: play_mario_jump_sound — audio skipped, MARIO_SOUND_PLAYED
        // flag is real compared state.
        crate::sound::play_mario_jump_sound(&mut cx.state, crate::sound::SoundEvent::Jump);
        cx.set_mario_animation(animation1);
    } else {
        if cx.state.action_timer >= 13 {
            return cx.goto(end_action, 0);
        }

        cx.set_mario_animation(animation2);
    }

    // Owned by the parallel walk.rs agent; exact signature
    // `pub fn apply_landing_accel(cx: &mut ActionCx, factor: f32)`.
    super::walk::apply_landing_accel(cx, 0.95);
    if perform_ground_step(&mut cx.state, cx.world) == GroundStep::LeftGround {
        return cx.goto_next(air_action, 0);
    }

    ActionResult::Stay
}

/// decomp: `act_quicksand_jump_land`
/// (src/game/mario_actions_moving.c:1940).
fn act_quicksand_jump_land(cx: &mut ActionCx) -> ActionResult {
    quicksand_jump_land_action(
        cx,
        anim_id::MARIO_ANIM_SINGLE_JUMP,
        anim_id::MARIO_ANIM_LAND_FROM_SINGLE_JUMP,
        ActionId::JUMP_LAND_STOP,
        ActionId::FREEFALL,
    )
}

/// Waist-deep quicksand idle (stationary action).
pub struct InQuicksand;

impl ActionHandler for InQuicksand {
    fn name(&self) -> &'static str {
        "InQuicksand"
    }

    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        act_in_quicksand(cx, prev_buttons)
    }
}

/// Landing from a jump out of quicksand (moving action).
pub struct QuicksandJumpLand;

impl ActionHandler for QuicksandJumpLand {
    fn name(&self) -> &'static str {
        "QuicksandJumpLand"
    }

    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        act_quicksand_jump_land(cx)
    }
}
