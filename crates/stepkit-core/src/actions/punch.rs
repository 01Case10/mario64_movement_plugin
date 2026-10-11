//! Punch actions: PUNCHING (stationary) and MOVE_PUNCHING (moving).
//!
//! Verbatim port of the decomp punch state machine:
//! `mario_update_punch_sequence` / `act_punching`
//! (oracle/sm64/src/game/mario_actions_object.c:27, :147) and
//! `act_move_punching` (oracle/sm64/src/game/mario_actions_moving.c:840).
//! Sound calls, the MARIO_PUNCHING / MARIO_KICKING / MARIO_TRIPPING flags,
//! `marioBodyState->punchState`, and the object-grab hit detection are
//! object-interaction / render / audio concerns: out of scope, skipped with
//! comments. Only movement, animation, and the state machine are ported.
//!
//! # AUDIT: decomp vs the old `ground.rs` `punch_tick` port (replaced by this file)
//!
//! The old port modeled the combo as 3 stages (actionState 0/1/2) with fixed
//! frame budgets `PUNCH_STAGE_FRAMES = [8, 8, 12]` driven by actionTimer.
//! Reading the decomp for the first time shows the combo is an actionArg
//! 0-6 machine with *no* frame budgets — transitions are anim-end driven:
//!
//! - MATCH: the combo is punch -> punch -> kick, B chains stages, A on entry
//!   can become a jump kick, the moving punch preserves momentum with a
//!   slope decel, and the combo ends in IDLE / WALKING.
//! - MISMATCH (stage frame counts): the decomp has no 8/8/12 frame budgets.
//!   The sequence is 6 sub-states: FIRST_PUNCH (arg 0/1), FIRST_PUNCH_FAST
//!   recovery (arg 2), SECOND_PUNCH (arg 3/4), SECOND_PUNCH_FAST recovery
//!   (arg 5), GROUND_KICK (arg 6). Transitions happen when the anim is
//!   within 2 frames of its end (`is_anim_past_end`) or at its last frame
//!   (`is_anim_at_end`).
//! - MISMATCH (actionState machine): the old port used actionState as the
//!   combo stage. The decomp uses actionArg 0-6 for the combo stage and
//!   actionState only as a first-tick marker (0 on entry, 1 afterwards) for
//!   the jump-kick check.
//! - MISMATCH (B chaining): the decomp checks INPUT_B_PRESSED only during
//!   the FAST recovery anims (actionArg 2 -> 3, actionArg 5 -> 6). B during
//!   the main punch anims (args 0/1/3/4/6) does nothing. The old port let B
//!   chain during the whole stage (except the entry tick).
//! - MISMATCH (what ends the punch): anim end of a FAST recovery anim with
//!   no B pressed, or anim end of GROUND_KICK, goes to IDLE (stationary) or
//!   WALKING (move punch). The move punch goes to WALKING *unconditionally*,
//!   not only when the stick is held as the old port did.
//! - MISMATCH (jump-kick trigger): the decomp is `actionState == 0 &&
//!   INPUT_A_DOWN` (A *held* on the entry tick) -> ACT_JUMP_KICK, checked
//!   after the common exits, so a newly-pressed A on the entry tick is
//!   consumed by the exits first. The old port used the A-pressed edge
//!   within the first 2 timer ticks.
//! - MISMATCH (move-punch decel): the decomp applies
//!   `apply_slope_decel(m, 0.5)` only when forwardVel >= 0 — a
//!   slipperiness-class-scaled approach to 0 (0.1 / 0.35 / 1.0 / 1.5 per
//!   frame) plus the slope accel — and a separate negative-speed branch
//!   (forwardVel += 8.0, clamp at 0, then slope accel). The old port used a
//!   flat 0.5 toward-zero plus `slope_speed_delta` for all speeds.
//! - MISMATCH (stationary punch speed): the decomp sets forward speed from
//!   `sPunchingForwardVelocities[actionTimer]` each tick — 10,7,5,3,2,1,1,0
//!   as the timer runs 7->0 — a lunge during the first punch's startup that
//!   is zero afterwards. The old port decayed speed continuously.
//! - MISSING in old port: the FIRST/SECOND_PUNCH_FAST recovery anims; the
//!   decomp's case 9 (BREAKDANCE trip) — kept here for verbatim fidelity
//!   even though nothing in the decomp ever sets actionArg 9 (unreachable);
//!   the per-action timer discipline (timer forced to 7 on arg 0,
//!   decremented manually — never auto-incremented).
//! - NOTE (return semantics): the task brief described
//!   `mario_update_punch_sequence` as "returns TRUE while punching
//!   continues"; the decomp actually returns TRUE only when
//!   `mario_check_object_grab` succeeds (object interaction, no-op here),
//!   and both decomp callers ignore the return value. Ported as written:
//!   the grab no-op never fires, so the function always returns FALSE.

use super::air::enter_jump_kick;
use super::ground;
use super::{ActionCx, ActionHandler, ActionResult};
use crate::anim_table as anim_id;
use crate::input::{buttons, input_flags};
use crate::state::{act_flags, ActionId};
use crate::step::{perform_ground_step, GroundStep};
use crate::world::{
    mario_facing_downhill, mario_floor_is_slope, mario_get_floor_class, terrain_type, SurfaceClass,
};

/// decomp: `sPunchingForwardVelocities[8]`
/// (src/game/mario_actions_object.c:17). s8 in the decomp; the port stores
/// f32 since `set_forward_speed` takes f32. Indexed by actionTimer, which
/// runs 7 -> 0.
const PUNCHING_FORWARD_VELOCITIES: [f32; 8] = [0.0, 1.0, 1.0, 2.0, 3.0, 5.0, 7.0, 10.0];

/// Advance the punch combo state machine (actionArg 0-6, plus the decomp's
/// unreachable case 9). Transitions are anim-end driven; endAction is
/// WALKING for the moving punch and IDLE otherwise. The decomp calls
/// `set_mario_action` for the end transitions and the callers then return
/// FALSE, so the new action starts next tick (`goto_next`).
/// Returns TRUE only when an object grab fired (no-op in this port: no
/// objects in scope — always FALSE in practice).
/// decomp: `mario_update_punch_sequence` (src/game/mario_actions_object.c:27)
fn mario_update_punch_sequence(cx: &mut ActionCx, prev_buttons: u16) -> bool {
    let moving = cx.state.action.0 & act_flags::MOVING != 0;
    let end_action = if moving {
        ActionId::WALKING
    } else {
        ActionId::IDLE
    };
    let crouch_end_action = if moving {
        ActionId::CROUCH_SLIDE
    } else {
        // decomp ACT_CROUCHING; this port names it CROUCH.
        ActionId::CROUCH
    };
    let end_slot = if moving {
        ground::slot::WALK
    } else {
        ground::slot::IDLE
    };
    let crouch_end_slot = if moving {
        ground::slot::CROUCH_SLIDE
    } else {
        ground::slot::CROUCH
    };

    match cx.state.action_arg {
        0 | 1 => {
            // decomp case 0: play_sound(SOUND_MARIO_PUNCH_YAH) — audio, skipped.
            // (case 0 falls through into case 1)
            let anim_frame = cx.set_mario_animation(anim_id::MARIO_ANIM_FIRST_PUNCH) as i32;
            if cx.is_anim_past_end() {
                cx.state.action_arg = 2;
            } else {
                cx.state.action_arg = 1;
            }
            if anim_frame >= 2 {
                // mario_check_object_grab: no-op (no objects in scope).
                // m->flags |= MARIO_PUNCHING: object interaction, skipped.
            }
            if cx.state.action_arg == 2 {
                // m->marioBodyState->punchState = (0 << 6) | 4: render, skipped.
            }
        }
        2 => {
            let anim_frame = cx.set_mario_animation(anim_id::MARIO_ANIM_FIRST_PUNCH_FAST) as i32;
            if anim_frame <= 0 {
                // m->flags |= MARIO_PUNCHING: object interaction, skipped.
            }
            if cx.pressed(buttons::B, prev_buttons) {
                cx.state.action_arg = 3;
            }
            if cx.is_anim_at_end() {
                cx.timeline.slot = end_slot;
                // decomp: set_mario_action's return is discarded here; the
                // caller returns FALSE, so the new action starts next tick.
                let _ = cx.goto_next(end_action, 0);
                return false;
            }
        }
        3 | 4 => {
            // decomp case 3: play_sound(SOUND_MARIO_PUNCH_WAH) — audio, skipped.
            // (case 3 falls through into case 4)
            let anim_frame = cx.set_mario_animation(anim_id::MARIO_ANIM_SECOND_PUNCH) as i32;
            if cx.is_anim_past_end() {
                cx.state.action_arg = 5;
            } else {
                cx.state.action_arg = 4;
            }
            if anim_frame > 0 {
                // m->flags |= MARIO_PUNCHING: object interaction, skipped.
            }
            if cx.state.action_arg == 5 {
                // m->marioBodyState->punchState = (1 << 6) | 4: render, skipped.
            }
        }
        5 => {
            let anim_frame = cx.set_mario_animation(anim_id::MARIO_ANIM_SECOND_PUNCH_FAST) as i32;
            if anim_frame <= 0 {
                // m->flags |= MARIO_PUNCHING: object interaction, skipped.
            }
            if cx.pressed(buttons::B, prev_buttons) {
                cx.state.action_arg = 6;
            }
            if cx.is_anim_at_end() {
                cx.timeline.slot = end_slot;
                // decomp: set_mario_action's return is discarded here; the
                // caller returns FALSE, so the new action starts next tick.
                let _ = cx.goto_next(end_action, 0);
                return false;
            }
        }
        6 => {
            // decomp: play_mario_action_sound(SOUND_MARIO_PUNCH_HOO) — audio
            // skipped, MARIO_ACTION_SOUND_PLAYED flag is real state.
            crate::sound::play_mario_action_sound(&mut cx.state);
            let anim_frame = cx.set_mario_animation(anim_id::MARIO_ANIM_GROUND_KICK) as i32;
            if anim_frame == 0 {
                // m->marioBodyState->punchState = (2 << 6) | 6: render, skipped.
            }
            if (0..8).contains(&anim_frame) {
                // m->flags |= MARIO_KICKING: object interaction, skipped.
            }
            if cx.is_anim_at_end() {
                cx.timeline.slot = end_slot;
                // decomp: set_mario_action's return is discarded here; the
                // caller returns FALSE, so the new action starts next tick.
                let _ = cx.goto_next(end_action, 0);
                return false;
            }
        }
        9 => {
            // Unreachable in the decomp: nothing in the game code ever sets
            // actionArg 9. Kept for verbatim fidelity.
            // decomp: play_mario_action_sound(SOUND_MARIO_PUNCH_HOO) — audio
            // skipped, MARIO_ACTION_SOUND_PLAYED flag is real state.
            crate::sound::play_mario_action_sound(&mut cx.state);
            let anim_frame = cx.set_mario_animation(anim_id::MARIO_ANIM_BREAKDANCE) as i32;
            if (2..8).contains(&anim_frame) {
                // m->flags |= MARIO_TRIPPING: object interaction, skipped.
            }
            if cx.is_anim_at_end() {
                cx.timeline.slot = crouch_end_slot;
                let _ = cx.goto_next(crouch_end_action, 0);
                return false;
            }
        }
        _ => {}
    }

    false
}

/// Inline stationary-action preamble for the punch: water plunge, deep
/// quicksand, death, then the slide check. Duplicated rather than
/// cross-called (the decomp's `act_punching` head routes through
/// `check_common_action_exits`; INPUT_STOMPED is always 0 in this port and
/// the A-press / analog exits are handled per-action, so the preamble keeps
/// only the stationary checks).
/// decomp: `act_punching` head (src/game/mario_actions_object.c:147)
fn punch_stationary_prelude(cx: &mut ActionCx) -> Option<ActionResult> {
    // Water plunge: more than 100 below the surface.
    if let Some(wl) = crate::actions::water::water_plunge_surface(cx) {
        return Some(crate::actions::water::enter_water_plunge(cx, wl));
    }
    // NOTE: no quicksand-depth check here. The decomp's act_punching does
    // NOT route to IN_QUICKSAND; that check lives in act_idle
    // (stationary.c:106). Adding it here caused an IN_QUICKSAND <-> PUNCHING
    // same-tick loop.
    // Dead (health < 0x100): no input response; lie still.
    if cx.state.health < 0x100 {
        cx.state.forward_speed = 0.0;
        cx.state.vel.x = 0.0;
        cx.state.vel.z = 0.0;
        return Some(ActionResult::Stay);
    }
    // Above a slide surface: begin sliding (the decomp reaches this via
    // check_common_action_exits' INPUT_ABOVE_SLIDE branch).
    if cx.state.input & input_flags::ABOVE_SLIDE != 0 {
        return Some(ground::begin_sliding(cx));
    }
    None
}

/// Inline moving-group slide check for the moving punch.
/// decomp: `should_begin_sliding` (src/game/mario_actions_moving.c:467)
fn should_begin_sliding(cx: &ActionCx) -> bool {
    if cx.state.input & input_flags::ABOVE_SLIDE == 0 {
        return false;
    }
    // decomp: (m->area->terrainType & TERRAIN_MASK) == TERRAIN_SLIDE.
    let slide_level = (cx.state.terrain_type & terrain_type::MASK) == terrain_type::SLIDE;
    let moving_backward = cx.state.forward_speed <= -1.0;
    slide_level || moving_backward || mario_facing_downhill(&cx.state, 0)
}

/// Class-scaled decel toward 0 plus the slope accel; returns whether the
/// speed reached 0.
/// NOTE: the task contract expected this as `super::walk::apply_slope_decel`
/// (parallel agent), but `walk.rs` is still a stub — this private copy keeps
/// `act_move_punching` decomp-faithful until that lands; it can be swapped
/// for the shared one without behavior change.
/// decomp: `apply_slope_decel` (src/game/mario_actions_moving.c:393)
fn apply_slope_decel(cx: &mut ActionCx, decel_coef: f32) -> bool {
    super::walk::apply_slope_decel(cx, decel_coef)
}

/// Slope-driven speed change along the facing yaw, then re-sync the slide
/// and linear velocities from the new speed (the decomp's tail sets
/// slideVel/vel from faceAngle unconditionally). Quicksand/wind nudges
/// (mario_update_moving_sand / mario_update_windy_ground) are not modeled in
/// this port.
/// decomp: `apply_slope_accel` (src/game/mario_actions_moving.c:283)
fn apply_slope_accel(cx: &mut ActionCx) {
    super::walk::apply_slope_accel(cx)
}

/// Stationary punch: the 3-hit combo (punch, punch, kick). A held on the
/// entry tick becomes a jump kick instead.
/// decomp: `act_punching` (src/game/mario_actions_object.c:147)
fn act_punching(cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
    if let Some(r) = punch_stationary_prelude(cx) {
        return r;
    }
    // Jump kick: A held while actionState is still 0 (the entry tick).
    // decomp: `if (m->actionState == 0 && (m->input & INPUT_A_DOWN))`.
    if cx.state.action_state == 0 && cx.state.input & input_flags::A_DOWN != 0 {
        return enter_jump_kick(cx);
    }

    cx.state.action_state = 1;
    if cx.state.action_arg == 0 {
        cx.state.action_timer = 7;
    }
    // Per-action timer discipline (no auto-increment in this port): the
    // table index counts down 7 -> 0, lunging only during the first punch.
    cx.set_forward_speed(PUNCHING_FORWARD_VELOCITIES[cx.state.action_timer as usize]);
    if cx.state.action_timer > 0 {
        cx.state.action_timer -= 1;
    }

    mario_update_punch_sequence(cx, prev_buttons);

    let step = perform_ground_step(&mut cx.state, cx.world);
    if let Some(f) = cx.state.floor {
        cx.state.floor_kind = ground::surface_kind_index(f.kind);
    }
    if step == GroundStep::LeftGround {
        // The decomp's head covers OFF_FLOOR via the common exits; the step
        // itself just falls through, so the port routes the walked-off edge
        // to freefall like the other ground actions.
        return cx.goto_next(ActionId::FREEFALL, 0);
    }
    cx.timeline.slot = ground::slot::PUNCH;
    ActionResult::Stay
}

/// Moving punch: the same combo while moving. Momentum is kept and decayed
/// with the decomp's 0.5 slope decel; the combo hands back to WALKING
/// (unconditionally) when the sequence ends.
/// decomp: `act_move_punching` (src/game/mario_actions_moving.c:840)
fn act_move_punching(cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
    if should_begin_sliding(cx) {
        return ground::begin_sliding(cx);
    }
    // Jump kick: A held while actionState is still 0 (the entry tick).
    if cx.state.action_state == 0 && cx.state.input & input_flags::A_DOWN != 0 {
        return enter_jump_kick(cx);
    }

    cx.state.action_state = 1;

    mario_update_punch_sequence(cx, prev_buttons);

    if cx.state.forward_speed >= 0.0 {
        // decomp coefficient 0.5, verified at mario_actions_moving.c:851.
        apply_slope_decel(cx, 0.5);
    } else {
        let mut speed = cx.state.forward_speed + 8.0;
        if speed >= 0.0 {
            speed = 0.0;
        }
        cx.state.forward_speed = speed;
        apply_slope_accel(cx);
    }

    match perform_ground_step(&mut cx.state, cx.world) {
        GroundStep::LeftGround => {
            if let Some(f) = cx.state.floor {
                cx.state.floor_kind = ground::surface_kind_index(f.kind);
            }
            cx.goto_next(ActionId::FREEFALL, 0)
        }
        _ => {
            // decomp GROUND_STEP_NONE: PARTICLE_DUST — particles, skipped.
            // (no HIT_WALL case in the decomp)
            if let Some(f) = cx.state.floor {
                cx.state.floor_kind = ground::surface_kind_index(f.kind);
            }
            cx.timeline.slot = ground::slot::MOVE_PUNCH;
            ActionResult::Stay
        }
    }
}

/// Punching: the stationary 3-hit combo (punch, punch, kick).
pub struct Punching;
impl ActionHandler for Punching {
    fn name(&self) -> &'static str {
        "Punching"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        act_punching(cx, prev_buttons)
    }
}

/// MovePunching: the same combo while moving; momentum is kept and the
/// combo hands back to walking when the sequence ends.
pub struct MovePunching;
impl ActionHandler for MovePunching {
    fn name(&self) -> &'static str {
        "MovePunching"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        act_move_punching(cx, prev_buttons)
    }
}
