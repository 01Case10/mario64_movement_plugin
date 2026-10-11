//! Phase 5 ground-action ports: the decomp's walking family from
//! `src/game/mario_actions_moving.c` (`update_walking_speed` and friends,
//! `act_walking`, `act_turning_around`, `act_finish_turning_around`,
//! `act_braking`, `act_decelerating`) plus the stationary actions they
//! transition into (`act_braking_stop`, `act_standing_against_wall`).
//!
//! Conventions: every ported item cites its decomp location; sound, particle,
//! and camera/gfx calls are skipped with a comment; `m->input & INPUT_*`
//! reads become `cx.state.input & input_flags::*`; `set_mario_action`'s
//! moving-group dispatch is modeled by routing `WALKING` / `BEGIN_SLIDING`
//! through `super::entry::set_mario_action_moving` (other moving actions
//! have no dispatch arm, so the id passes through unchanged).

use super::entry;
use super::{ActionCx, ActionHandler, ActionResult};
use crate::angles::Angle;
use crate::anim_table;
use crate::input::input_flags as f;
use crate::math_util::{approach_f32, approach_s32};
use crate::state::{act_flags, ActionId};
use crate::step::{
    mario_bonk_reflection, mario_update_moving_sand, mario_update_quicksand,
    mario_update_windy_ground, perform_ground_step, stationary_ground_step, GroundStep,
};
use crate::trig;
use crate::world::{
    mario_facing_downhill, mario_floor_is_slope, mario_get_floor_class, surface_type,
    terrain_type, SurfaceClass,
};
use glam::Vec3;

// ---------------------------------------------------------------------------
// Shared helpers (pub: other action modules call these).
// ---------------------------------------------------------------------------

/// decomp: src/game/mario_actions_moving.c:434 (`update_walking_speed`).
pub fn update_walking_speed(cx: &mut ActionCx) {
    let max_target_speed =
        if cx.state.floor.map(|f| f.surface_type) == Some(surface_type::SLOW) {
            24.0
        } else {
            32.0
        };

    let mut target_speed = cx.state.intended_mag.min(max_target_speed);

    if cx.state.quicksand_depth > 10.0 {
        // C: `targetSpeed *= 6.25 / m->quicksandDepth` — 6.25 is a double
        // literal, so the division runs in f64 before the f32 store (§5).
        target_speed =
            (target_speed as f64 * (6.25f64 / cx.state.quicksand_depth as f64)) as f32;
    }

    if cx.state.forward_speed <= 0.0 {
        cx.state.forward_speed += 1.1;
    } else if cx.state.forward_speed <= target_speed {
        cx.state.forward_speed += 1.1 - cx.state.forward_speed / 43.0;
    } else if cx.state.floor.map(|f| f.normal.y).unwrap_or(1.0) >= 0.95 {
        // The decomp dereferences m->floor unconditionally here; a missing
        // floor is treated as flat ground.
        cx.state.forward_speed -= 1.0;
    }

    if cx.state.forward_speed > 48.0 {
        cx.state.forward_speed = 48.0;
    }

    // decomp: `m->faceAngle[1] =
    //   m->intendedYaw - approach_s32((s16)(m->intendedYaw - m->faceAngle[1]), 0, 0x800, 0x800)`
    // The yaw difference wraps in s16; the final store truncates to s16 (gcc behavior).
    let dyaw = cx.state.intended_yaw.0.wrapping_sub(cx.state.face_yaw.0) as i32;
    let turn = approach_s32(dyaw, 0, 0x800, 0x800);
    cx.state.face_yaw = Angle((cx.state.intended_yaw.0 as i32 - turn) as i16);

    // NOTE: slope accel runs AFTER the turn, on the new facing (moving.c:462).
    apply_slope_accel(cx);
}

/// decomp: src/game/mario_actions_moving.c:283 (`apply_slope_accel`).
pub fn apply_slope_accel(cx: &mut ActionCx) {
    // The decomp dereferences m->floor unconditionally; a missing floor is
    // treated as a flat normal (zero steepness).
    let normal = cx.state.floor.map(|f| f.normal).unwrap_or(Vec3::Y);
    let steepness = (normal.x * normal.x + normal.z * normal.z).sqrt();
    let floor_dyaw = cx.state.floor_angle.0.wrapping_sub(cx.state.face_yaw.0);

    // Gated on mario_floor_is_slope (NOT any-tilt).
    if mario_floor_is_slope(&cx.state) {
        // decomp: slopeClass stays 0 (→ the default 1.7 arm) for the soft
        // knockback actions; SurfaceClass::Default maps to that same arm.
        let slope_class = if cx.state.action == ActionId::SOFT_BACKWARD_GROUND_KB
            || cx.state.action == ActionId::SOFT_FORWARD_GROUND_KB
        {
            SurfaceClass::Default
        } else {
            mario_get_floor_class(&cx.state)
        };
        let slope_accel = match slope_class {
            SurfaceClass::VerySlippery => 5.3,
            SurfaceClass::Slippery => 2.7,
            SurfaceClass::NotSlippery => 0.0,
            SurfaceClass::Default => 1.7,
        };

        if floor_dyaw > -0x4000 && floor_dyaw < 0x4000 {
            cx.state.forward_speed += slope_accel * steepness;
        } else {
            cx.state.forward_speed -= slope_accel * steepness;
        }
    }

    cx.state.slide_yaw = cx.state.face_yaw;
    // NOTE: the decomp writes slideVel/vel directly from forwardVel here
    // (it does NOT go through mario_set_forward_vel).
    let (s, c) = (trig::sins(cx.state.face_yaw), trig::coss(cx.state.face_yaw));
    cx.state.slide_vel_x = cx.state.forward_speed * s;
    cx.state.slide_vel_z = cx.state.forward_speed * c;
    cx.state.vel.x = cx.state.slide_vel_x;
    cx.state.vel.y = 0.0;
    cx.state.vel.z = cx.state.slide_vel_z;

    // Return values ignored, like the decomp.
    let _ = mario_update_moving_sand(&mut cx.state);
    let _ = mario_update_windy_ground(&mut cx.state);
}

/// decomp: src/game/mario_actions_moving.c:393 (`apply_slope_decel`).
/// Returns true when the speed reaches zero.
pub fn apply_slope_decel(cx: &mut ActionCx, decel_coef: f32) -> bool {
    let decel = match mario_get_floor_class(&cx.state) {
        SurfaceClass::VerySlippery => decel_coef * 0.2,
        SurfaceClass::Slippery => decel_coef * 0.7,
        SurfaceClass::NotSlippery => decel_coef * 3.0,
        SurfaceClass::Default => decel_coef * 2.0,
    };

    cx.state.forward_speed = approach_f32(cx.state.forward_speed, 0.0, decel, decel);
    let stopped = cx.state.forward_speed == 0.0;

    apply_slope_accel(cx);
    stopped
}

/// decomp: src/game/mario_actions_moving.c:334 (`apply_landing_accel`).
/// Multiplicative friction; returns true when the speed reaches ~zero.
pub fn apply_landing_accel(cx: &mut ActionCx, factor: f32) -> bool {
    let mut stopped = false;

    apply_slope_accel(cx);

    if !mario_floor_is_slope(&cx.state) {
        // Multiplicative: the decomp scales forwardVel directly here (the
        // slide/linear vel sync already happened inside apply_slope_accel).
        cx.state.forward_speed *= factor;
        if cx.state.forward_speed * cx.state.forward_speed < 1.0 {
            cx.set_forward_speed(0.0);
            stopped = true;
        }
    }

    stopped
}

/// decomp: src/game/mario_actions_moving.c:420 (`update_decelerating_speed`).
/// Returns true when the speed reaches zero.
pub fn update_decelerating_speed(cx: &mut ActionCx) -> bool {
    cx.state.forward_speed = approach_f32(cx.state.forward_speed, 0.0, 1.0, 1.0);
    let stopped = cx.state.forward_speed == 0.0;

    let v = cx.state.forward_speed;
    cx.set_forward_speed(v);
    let _ = mario_update_moving_sand(&mut cx.state);
    let _ = mario_update_windy_ground(&mut cx.state);

    stopped
}

/// decomp: src/game/mario_actions_moving.c:480 (`analog_stick_held_back`).
pub fn analog_stick_held_back(cx: &mut ActionCx) -> bool {
    // s16 intendedDYaw = m->intendedYaw - m->faceAngle[1] (wrapping).
    let intended_dyaw = cx.state.intended_yaw.0.wrapping_sub(cx.state.face_yaw.0);
    intended_dyaw < -0x471C || intended_dyaw > 0x471C
}

/// decomp: src/game/mario_actions_moving.c:485 (`check_ground_dive_or_punch`).
pub fn check_ground_dive_or_punch(cx: &mut ActionCx, _prev_buttons: u16) -> Option<ActionResult> {
    if cx.state.input & f::B_PRESSED != 0 {
        // Speed-kick dive: the decomp sets ONLY vel.y = 20.0 here —
        // forwardVel is unchanged. Deliberately NOT super::air::enter_dive,
        // which adds +15 forward speed that the decomp does not apply.
        if cx.state.forward_speed >= 29.0 && cx.proc.stick_mag > 48.0 {
            cx.state.vel.y = 20.0;
            return Some(cx.goto(ActionId::DIVE, 1));
        }

        return Some(cx.goto(ActionId::MOVE_PUNCHING, 0));
    }

    None
}

/// decomp: src/game/mario_actions_moving.c:501 (`begin_braking_action`).
pub fn begin_braking_action(cx: &mut ActionCx) -> ActionResult {
    // mario_drop_held_object: no held objects in scope; no-op.

    if cx.state.action_state == 1 {
        // decomp: m->faceAngle[1] = m->actionArg (u32 → s16 truncation).
        cx.state.face_yaw = Angle(cx.state.action_arg as i16);
        return cx.goto(ActionId::STANDING_AGAINST_WALL, 0);
    }

    // The decomp dereferences m->floor unconditionally; a missing floor is
    // treated as flat ground.
    let normal_y = cx.state.floor.map(|f| f.normal.y).unwrap_or(1.0);
    if cx.state.forward_speed >= 16.0 && normal_y >= 0.17364818 {
        return cx.goto(ActionId::BRAKING, 0);
    }

    cx.goto(ActionId::DECELERATING, 0)
}

/// decomp: src/game/mario_actions_moving.c:670 (`push_or_sidle_wall`).
///
/// SIGNATURE NOTE: the decomp takes `startPos` — the tick-start position,
/// captured in `act_walking` before `update_walking_speed` — and uses it to
/// derive the sidestep animation accel (`movedDistance`). The brief's
/// signature omitted it; it is restored here because there is no other
/// channel for it. The sole caller (`Walking::tick`) captures the position
/// at the top of the tick and passes it in.
pub fn push_or_sidle_wall(cx: &mut ActionCx, start_pos: Vec3) -> ActionResult {
    let dx = cx.state.pos.x - start_pos.x;
    let dz = cx.state.pos.z - start_pos.z;
    let moved_distance = (dx * dx + dz * dz).sqrt();
    // (s32)(movedDistance * 2.0f * 0x10000): f32 arithmetic, truncated.
    let val04 = (moved_distance * 2.0 * 65536.0) as i32;

    if cx.state.forward_speed > 6.0 {
        cx.set_forward_speed(6.0);
    }

    // decomp: dWallAngle is left uninitialized when wall == NULL, but the
    // NULL arm of the condition below short-circuits before reading it.
    let (wall_angle, d_wall_angle) = match cx.state.wall {
        Some(w) => {
            let wa = trig::atan2s(w.normal.z, w.normal.x);
            (Some(wa), wa.0.wrapping_sub(cx.state.face_yaw.0))
        }
        None => (None, 0),
    };

    if cx.state.wall.is_none() || d_wall_angle <= -0x71C8 || d_wall_angle >= 0x71C8 {
        // MARIO_UNKNOWN_31 is not yet in crate::state::mario_flags (more bits
        // arrive with Phase 5); value from the decomp (0x80000000).
        const MARIO_UNKNOWN_31: u32 = 0x80000000;
        cx.state.flags |= MARIO_UNKNOWN_31;
        cx.set_mario_animation(anim_table::MARIO_ANIM_PUSHING);
        // play_step_sound(m, 6, 18): audio; skipped.
    } else {
        let wall_angle = wall_angle.unwrap();
        if d_wall_angle < 0 {
            cx.set_mario_anim_with_accel(anim_table::MARIO_ANIM_SIDESTEP_RIGHT, val04);
        } else {
            cx.set_mario_anim_with_accel(anim_table::MARIO_ANIM_SIDESTEP_LEFT, val04);
        }

        // animFrame < 20 → slide sound + dust particles: audio; skipped.
        // marioObj header gfx angle writes: renderer-only; skipped.

        cx.state.action_state = 1;
        // decomp: m->actionArg = wallAngle + 0x8000 (int arithmetic → u32).
        cx.state.action_arg = (wall_angle.0 as i32 + 0x8000) as u32;
    }

    // decomp returns void; act_walking continues (actionTimer = 0, ledge
    // check) and returns FALSE.
    ActionResult::Stay
}

/// decomp: src/game/mario_actions_moving.c:100 (`check_ledge_climb_down`).
/// Transitions to LEDGE_CLIMB_DOWN (next tick) when it fires.
pub fn check_ledge_climb_down(cx: &mut ActionCx) -> Option<ActionResult> {
    if cx.state.forward_speed < 10.0 {
        // decomp: find_wall_collisions with radius 10.0, offsetY -10.0.
        let wc =
            cx.world
                .find_wall_collisions(cx.state.pos.x, cx.state.pos.y, cx.state.pos.z, -10.0, 10.0);
        if wc.num_walls != 0 {
            // decomp queries the floor at the pushed (post-collision) position.
            if let Some(floor) = cx.world.find_floor(wc.pos.x, wc.pos.y, wc.pos.z) {
                if wc.pos.y - floor.y > 160.0 {
                    // decomp: wall = walls[numWalls - 1] (the referenced wall).
                    if let Some(wall) = wc.wall {
                        let wall_angle = trig::atan2s(wall.normal.z, wall.normal.x);
                        let wall_dyaw = wall_angle.0.wrapping_sub(cx.state.face_yaw.0);
                        if wall_dyaw > -0x4000 && wall_dyaw < 0x4000 {
                            cx.state.pos.x = wc.pos.x - 20.0 * wall.normal.x;
                            cx.state.pos.z = wc.pos.z - 20.0 * wall.normal.z;
                            cx.state.face_pitch = Angle(0);
                            cx.state.face_yaw = wall_angle.wrapping_add(Angle::HALF_TURN);
                            // decomp discards set_mario_action's return (void
                            // fn): transition now, new action runs next tick.
                            let _ = cx.goto_next(ActionId::LEDGE_CLIMB_DOWN, 0);
                            cx.set_mario_animation(anim_table::MARIO_ANIM_CLIMB_DOWN_LEDGE);
                            return Some(ActionResult::Stay);
                        }
                    }
                }
            }
        }
    }
    None
}

/// decomp: src/game/mario_actions_moving.c:467 (`should_begin_sliding`).
pub fn should_begin_sliding(cx: &mut ActionCx) -> bool {
    if cx.state.input & f::ABOVE_SLIDE != 0 {
        let slide_level = (cx.state.terrain_type & terrain_type::MASK) == terrain_type::SLIDE;
        let moving_backward = cx.state.forward_speed <= -1.0;
        if slide_level || moving_backward || mario_facing_downhill(&cx.state, 0) {
            return true;
        }
    }
    false
}

/// The moving-group preamble: water plunge, stomp/squish (never set in
/// scope), low-health death, then the per-tick quicksand update.
/// decomp: `check_common_moving_cancels` (mario_actions_moving.c:1953) plus
/// the `mario_update_quicksand(m, 0.25f)` step of `mario_execute_moving_action`
/// (mario_actions_moving.c:1978).
pub fn check_common_moving_cancels(cx: &mut ActionCx) -> Option<ActionResult> {
    if cx.state.pos.y < cx.state.water_level - 100.0 {
        return Some(entry::set_water_plunge_action(cx));
    }

    // INPUT_STOMPED: never set in scope (no stomp sources); the decomp would
    // `drop_and_set_mario_action(m, ACT_SHOCKWAVE_BOUNCE, 0)` here when the
    // action is not invulnerable.
    // INPUT_SQUISHED: never set in scope; the decomp would
    // `drop_and_set_mario_action(m, ACT_SQUISHED, 0)`.

    if cx.state.action.0 & act_flags::INVULNERABLE == 0 && cx.state.health < 0x100 {
        return Some(entry::drop_and_set_mario_action(
            cx,
            ActionId::STANDING_DEATH,
            0,
        ));
    }

    // The Rust mario_update_quicksand returns only the drowned bool; the
    // decomp transitions to ACT_QUICKSAND_DEATH internally and the executor
    // returns TRUE (same tick), so mirror that transition here.
    if mario_update_quicksand(&mut cx.state, 0.25) {
        return Some(cx.goto(ActionId::QUICKSAND_DEATH, 0));
    }

    None
}

// ---------------------------------------------------------------------------
// Private helpers.
// ---------------------------------------------------------------------------

/// Walk animation state machine. `actionTimer` doubles as the walk phase:
/// 0 = start-tiptoe, 1 = tiptoe, 2 = walk, 3 = run. (Audio calls and the
/// `tilt_body_running` lean are skipped.)
///
/// Called only from `act_walking`'s GROUND_STEP_NONE path (moving.c:823).
/// decomp: src/game/mario_actions_moving.c:516 (`anim_and_audio_for_walk`).
fn anim_and_audio_for_walk(cx: &mut ActionCx) {
    let mut val04 = if cx.state.intended_mag > cx.state.forward_speed {
        cx.state.intended_mag
    } else {
        cx.state.forward_speed
    };
    if val04 < 4.0 {
        val04 = 4.0;
    }

    if cx.state.quicksand_depth > 50.0 {
        // (s32)(val04 / 4.0f * 0x10000): f32 arithmetic, truncated.
        let val14 = (val04 / 4.0 * 65536.0) as i32;
        cx.set_mario_anim_with_accel(anim_table::MARIO_ANIM_MOVE_IN_QUICKSAND, val14);
        // play_step_sound(m, 19, 93): audio; skipped.
        cx.state.action_timer = 0;
    } else {
        // The decomp loops (`while (val0C)`) so a phase change applies
        // immediately within the same tick (e.g. 0 → 2 → 3 when val04 > 22 —
        // the oracle shows actionTimer = 3 after one walking frame).
        let mut again = true;
        while again {
            match cx.state.action_timer {
                0 => {
                    if val04 > 8.0 {
                        cx.state.action_timer = 2;
                    } else {
                        let mut val14 = (val04 / 4.0 * 65536.0) as i32;
                        if val14 < 0x1000 {
                            val14 = 0x1000;
                        }
                        cx.set_mario_anim_with_accel(anim_table::MARIO_ANIM_START_TIPTOE, val14);
                        // play_step_sound(m, 7, 22): audio; skipped.
                        if cx.is_anim_past_frame(23) {
                            cx.state.action_timer = 2;
                        }
                        again = false;
                    }
                }
                1 => {
                    if val04 > 8.0 {
                        cx.state.action_timer = 2;
                    } else {
                        // NOTE: no /4 here (unlike phases 0/2/3) — verbatim.
                        let mut val14 = (val04 * 65536.0) as i32;
                        if val14 < 0x1000 {
                            val14 = 0x1000;
                        }
                        cx.set_mario_anim_with_accel(anim_table::MARIO_ANIM_TIPTOE, val14);
                        // play_step_sound(m, 14, 72): audio; skipped.
                        again = false;
                    }
                }
                2 => {
                    if val04 < 5.0 {
                        cx.state.action_timer = 1;
                    } else if val04 > 22.0 {
                        cx.state.action_timer = 3;
                    } else {
                        let val14 = (val04 / 4.0 * 65536.0) as i32;
                        cx.set_mario_anim_with_accel(anim_table::MARIO_ANIM_WALKING, val14);
                        // play_step_sound(m, 10, 49): audio; skipped.
                        again = false;
                    }
                }
                3 => {
                    if val04 < 18.0 {
                        cx.state.action_timer = 2;
                    } else {
                        let val14 = (val04 / 4.0 * 65536.0) as i32;
                        cx.set_mario_anim_with_accel(anim_table::MARIO_ANIM_RUNNING, val14);
                        // play_step_sound(m, 9, 45): audio; skipped.
                        // targetPitch = tilt_body_running(m): body lean; skipped.
                        again = false;
                    }
                }
                _ => {
                    // Unreachable in the decomp (actionTimer only ever holds
                    // 0..=3 here); break instead of looping forever.
                    again = false;
                }
            }
        }
    }
}

/// Verbatim port of decomp `slide_bonk` (mario_actions_moving.c:136).
///
/// CONTRACT NOTE: the brief routes `Braking`'s wall hit through
/// `super::slides::slide_bonk(cx, fast_action, slow_action)`, but slides.rs
/// is still an empty stub in this checkout, so the logic is inlined here to
/// keep the crate compiling. When slides.rs lands, replace the call site in
/// `Braking::tick` with `super::slides::slide_bonk(...)` — the bodies must
/// stay identical: the transition happens immediately (next-tick semantics)
/// and this returns `Stay` so the caller continues the decomp's fall-through
/// (skid anim, then `return FALSE`).
fn slide_bonk(cx: &mut ActionCx, fast_action: ActionId, slow_action: ActionId) -> ActionResult {
    if cx.state.forward_speed > 16.0 {
        mario_bonk_reflection(&mut cx.state, true);
        // decomp: drop_and_set_mario_action (TRUE) discarded by act_braking.
        let _ = entry::drop_and_set_mario_action(cx, fast_action, 0);
    } else {
        cx.set_forward_speed(0.0);
        // decomp: set_mario_action (TRUE) discarded by act_braking.
        let _ = cx.goto_next(slow_action, 0);
    }
    ActionResult::Stay
}

/// Stationary-group preamble, inlined here so `BrakingStop` (and
/// `StandingAgainstWall`) don't depend on another agent's module.
/// decomp: `check_common_stationary_cancels`
/// (mario_actions_stationary.c:1083) plus the
/// `mario_update_quicksand(m, 0.5f)` step of `mario_execute_stationary_action`
/// (mario_actions_stationary.c:1109).
fn check_common_stationary_cancels(cx: &mut ActionCx) -> Option<ActionResult> {
    if cx.state.pos.y < cx.state.water_level - 100.0 {
        // decomp: ACT_SPAWN_SPIN_LANDING level-text + sound/camera updates skipped.
        return Some(entry::set_water_plunge_action(cx));
    }

    // INPUT_SQUISHED: never set in scope; the decomp would
    // `drop_and_set_mario_action(m, ACT_SQUISHED, 0)`.
    // The decomp guards the health check with `m->action !=
    // ACT_UNKNOWN_0002020E`; that action is not handled by this engine, so
    // the guard is vacuous.

    if cx.state.health < 0x100 {
        return Some(entry::drop_and_set_mario_action(
            cx,
            ActionId::STANDING_DEATH,
            0,
        ));
    }

    // NOTE: the decomp's stationary preamble has no INPUT_ABOVE_SLIDE step;
    // ABOVE_SLIDE is handled per-action (here via check_common_action_exits
    // in the body below).
    if mario_update_quicksand(&mut cx.state, 0.5) {
        return Some(cx.goto(ActionId::QUICKSAND_DEATH, 0));
    }

    None
}

// ---------------------------------------------------------------------------
// Handlers.
// ---------------------------------------------------------------------------

/// Walking: stick-held ground locomotion.
/// decomp: src/game/mario_actions_moving.c:777 (`act_walking`).
pub struct Walking;
impl ActionHandler for Walking {
    fn name(&self) -> &'static str {
        "Walking"
    }

    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = check_common_moving_cancels(cx) {
            return r;
        }

        // mario_drop_held_object: no held objects in scope; no-op.

        if should_begin_sliding(cx) {
            // decomp: set_mario_action → moving-group dispatch remaps
            // BEGIN_SLIDING to BUTT_SLIDE / STOMACH_SLIDE.
            let id = entry::set_mario_action_moving(cx, ActionId::BEGIN_SLIDING);
            return cx.goto(id, 0);
        }

        // INPUT_FIRST_PERSON is never set in scope; the decomp's
        // begin_braking_action shortcut is dead here.

        if cx.state.input & f::A_PRESSED != 0 {
            return entry::set_jump_from_landing(cx);
        }

        if let Some(r) = check_ground_dive_or_punch(cx, prev_buttons) {
            return r;
        }

        if cx.state.input & f::UNKNOWN_5 != 0 {
            return begin_braking_action(cx);
        }

        if analog_stick_held_back(cx) && cx.state.forward_speed >= 16.0 {
            // decomp: set_mario_action → dispatch; set_mario_action_moving
            // has no TURNING_AROUND arm, so the id passes through unchanged.
            return cx.goto(ActionId::TURNING_AROUND, 0);
        }

        // Z_PRESSED always → CROUCH_SLIDE (no crouch-vs-slide split).
        if cx.state.input & f::Z_PRESSED != 0 {
            return cx.goto(ActionId::CROUCH_SLIDE, 0);
        }

        // The old A+Z-held long-jump shortcut is removed (the decomp has none).

        cx.state.action_state = 0;

        // decomp: vec3f_copy(startPos, m->pos) before update_walking_speed.
        let start_pos = cx.state.pos;
        update_walking_speed(cx);

        match perform_ground_step(&mut cx.state, cx.world) {
            GroundStep::LeftGround => {
                // decomp discards set_mario_action's return and falls through
                // to `return FALSE`: FREEFALL runs next tick. The fall anim
                // is set on the new action immediately, like the decomp.
                let _ = cx.goto_next(ActionId::FREEFALL, 0);
                cx.set_mario_animation(anim_table::MARIO_ANIM_GENERAL_FALL);
            }
            GroundStep::None => {
                anim_and_audio_for_walk(cx);
                // PARTICLE_DUST when intendedMag - forwardVel > 16:
                // particles; skipped.
            }
            GroundStep::HitWall => {
                // decomp: push_or_sidle_wall(m, startPos); m->actionTimer = 0.
                let _ = push_or_sidle_wall(cx, start_pos);
                cx.state.action_timer = 0;
            }
        }

        // decomp: check_ledge_climb_down(m); (void — return ignored).
        let _ = check_ledge_climb_down(cx);
        // tilt_body_walking(m, startYaw): body lean; renderer-only; skipped.
        ActionResult::Stay
    }
}

/// Turning around: stick held back at speed >= 16. When the speed hits zero,
/// hands off to FinishTurningAround; when the turn anim ends, back to Walking.
/// decomp: src/game/mario_actions_moving.c:962 (`act_turning_around`).
pub struct TurningAround;
impl ActionHandler for TurningAround {
    fn name(&self) -> &'static str {
        "TurningAround"
    }

    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        if let Some(r) = check_common_moving_cancels(cx) {
            return r;
        }

        if cx.state.input & f::ABOVE_SLIDE != 0 {
            let id = entry::set_mario_action_moving(cx, ActionId::BEGIN_SLIDING);
            return cx.goto(id, 0);
        }

        if cx.state.input & f::A_PRESSED != 0 {
            return entry::set_jumping_action(cx, ActionId::SIDE_FLIP, 0);
        }

        if cx.state.input & f::UNKNOWN_5 != 0 {
            return cx.goto(ActionId::BRAKING, 0);
        }

        if !analog_stick_held_back(cx) {
            let id = entry::set_mario_action_moving(cx, ActionId::WALKING);
            return cx.goto(id, 0);
        }

        if apply_slope_decel(cx, 2.0) {
            // decomp: return begin_walking_action(m, 8.0f,
            //   ACT_FINISH_TURNING_AROUND, 0): facing = intendedYaw, speed
            //   8.0, same-tick transition (set_mario_action_moving has no
            //   FINISH_TURNING_AROUND arm).
            cx.state.face_yaw = cx.state.intended_yaw;
            cx.set_forward_speed(8.0);
            return cx.goto(ActionId::FINISH_TURNING_AROUND, 0);
        }

        // Terrain slide sound + adjust_sound_for_speed: audio; skipped.

        match perform_ground_step(&mut cx.state, cx.world) {
            GroundStep::LeftGround => {
                // decomp discards the return; falls through to `return FALSE`.
                let _ = cx.goto_next(ActionId::FREEFALL, 0);
            }
            GroundStep::None => {
                // PARTICLE_DUST: particles; skipped.
            }
            // decomp: no HIT_WALL case.
            GroundStep::HitWall => {}
        }

        // The facing is NOT rotated during the turn; only the anim is picked.
        if cx.state.forward_speed >= 18.0 {
            cx.set_mario_animation(anim_table::MARIO_ANIM_TURNING_PART1);
        } else {
            cx.set_mario_animation(anim_table::MARIO_ANIM_TURNING_PART2);
            if cx.is_anim_at_end() {
                // decomp: begin_walking_action(...); return value discarded —
                // entry effects apply, but WALKING runs next tick.
                cx.state.face_yaw = cx.state.intended_yaw;
                if cx.state.forward_speed > 0.0 {
                    let v = -cx.state.forward_speed;
                    cx.set_forward_speed(v);
                } else {
                    cx.set_forward_speed(8.0);
                }
                let id = entry::set_mario_action_moving(cx, ActionId::WALKING);
                let _ = cx.goto_next(id, 0);
            }
        }

        ActionResult::Stay
    }
}

/// Finish turning around: re-accelerate after the turn.
/// decomp: src/game/mario_actions_moving.c:1013 (`act_finish_turning_around`).
pub struct FinishTurningAround;
impl ActionHandler for FinishTurningAround {
    fn name(&self) -> &'static str {
        "FinishTurningAround"
    }

    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        if let Some(r) = check_common_moving_cancels(cx) {
            return r;
        }

        if cx.state.input & f::ABOVE_SLIDE != 0 {
            let id = entry::set_mario_action_moving(cx, ActionId::BEGIN_SLIDING);
            return cx.goto(id, 0);
        }

        if cx.state.input & f::A_PRESSED != 0 {
            return entry::set_jumping_action(cx, ActionId::SIDE_FLIP, 0);
        }

        update_walking_speed(cx);
        cx.set_mario_animation(anim_table::MARIO_ANIM_TURNING_PART2);

        if perform_ground_step(&mut cx.state, cx.world) == GroundStep::LeftGround {
            // decomp discards set_mario_action's return.
            let _ = cx.goto_next(ActionId::FREEFALL, 0);
        }

        if cx.is_anim_at_end() {
            // decomp discards set_mario_action's return; the moving-group
            // dispatch still applies the WALKING entry clamp.
            let id = entry::set_mario_action_moving(cx, ActionId::WALKING);
            let _ = cx.goto_next(id, 0);
        }

        // m->marioObj->header.gfx.angle[1] += 0x8000: renderer-only; skipped.
        ActionResult::Stay
    }
}

/// Braking: skid to a stop, then BrakingStop.
/// decomp: src/game/mario_actions_moving.c:1037 (`act_braking`).
pub struct Braking;
impl ActionHandler for Braking {
    fn name(&self) -> &'static str {
        "Braking"
    }

    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = check_common_moving_cancels(cx) {
            return r;
        }

        // decomp gates this on !FIRST_PERSON (never set) and an input mask;
        // check_common_action_exits returns None when no bit is set, so the
        // gate is subsumed.
        if let Some(r) = entry::check_common_action_exits(cx, prev_buttons) {
            return r;
        }

        if apply_slope_decel(cx, 2.0) {
            return cx.goto(ActionId::BRAKING_STOP, 0);
        }

        if cx.state.input & f::B_PRESSED != 0 {
            return cx.goto(ActionId::MOVE_PUNCHING, 0);
        }

        match perform_ground_step(&mut cx.state, cx.world) {
            GroundStep::LeftGround => {
                // decomp discards the return; falls through to `return FALSE`.
                let _ = cx.goto_next(ActionId::FREEFALL, 0);
            }
            GroundStep::None => {
                // PARTICLE_DUST: particles; skipped.
            }
            GroundStep::HitWall => {
                // decomp: slide_bonk(m, ACT_BACKWARD_GROUND_KB,
                //   ACT_BRAKING_STOP) (moving.c:1062) — NOT ACT_GROUND_BONK.
                // The brief routes this through super::slides::slide_bonk,
                // but slides.rs is still an empty stub; the verbatim logic
                // is the private slide_bonk() above — swap the call when
                // slides.rs lands. The decomp falls through to the skid anim
                // and `return FALSE`.
                let _ = slide_bonk(cx, ActionId::BACKWARD_GROUND_KB, ActionId::BRAKING_STOP);
            }
        }

        // play_sound + adjust_sound_for_speed: audio; skipped.
        cx.set_mario_animation(anim_table::MARIO_ANIM_SKID_ON_GROUND);
        ActionResult::Stay
    }
}

/// Braking stop: stationary skid-out, then Idle.
/// decomp: src/game/mario_actions_stationary.c:625 (`act_braking_stop`)
/// with `stopping_step` (stationary.c:612).
pub struct BrakingStop;
impl ActionHandler for BrakingStop {
    fn name(&self) -> &'static str {
        "BrakingStop"
    }

    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = check_common_stationary_cancels(cx) {
            return r;
        }

        // INPUT_STOMPED: never set in scope; the decomp would
        // set_mario_action(m, ACT_SHOCKWAVE_BOUNCE, 0).

        if cx.state.input & f::OFF_FLOOR != 0 {
            return cx.goto(ActionId::FREEFALL, 0);
        }

        if cx.state.input & f::B_PRESSED != 0 {
            return cx.goto(ActionId::PUNCHING, 0);
        }

        // decomp gates on !FIRST_PERSON (never set) + input mask; subsumed.
        if let Some(r) = entry::check_common_action_exits(cx, prev_buttons) {
            return r;
        }

        // stopping_step(m, MARIO_ANIM_STOP_SKID, ACT_IDLE):
        stationary_ground_step(&mut cx.state, cx.world);
        cx.set_mario_animation(anim_table::MARIO_ANIM_STOP_SKID);
        if cx.is_anim_at_end() {
            // decomp: set_mario_action in a void context → next tick.
            let _ = cx.goto_next(ActionId::IDLE, 0);
        }
        ActionResult::Stay
    }
}

/// Decelerating: coast to a stop, then Idle.
/// decomp: src/game/mario_actions_moving.c:1072 (`act_decelerating`).
pub struct Decelerating;
impl ActionHandler for Decelerating {
    fn name(&self) -> &'static str {
        "Decelerating"
    }

    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = check_common_moving_cancels(cx) {
            return r;
        }
        let slope_class = mario_get_floor_class(&cx.state);

        // INPUT_FIRST_PERSON is never set in scope; the decomp's gate around
        // these cancels is vacuous.
        if should_begin_sliding(cx) {
            let id = entry::set_mario_action_moving(cx, ActionId::BEGIN_SLIDING);
            return cx.goto(id, 0);
        }

        if cx.state.input & f::A_PRESSED != 0 {
            return entry::set_jump_from_landing(cx);
        }

        if let Some(r) = check_ground_dive_or_punch(cx, prev_buttons) {
            return r;
        }

        if cx.state.input & f::NONZERO_ANALOG != 0 {
            let id = entry::set_mario_action_moving(cx, ActionId::WALKING);
            return cx.goto(id, 0);
        }

        if cx.state.input & f::Z_PRESSED != 0 {
            return cx.goto(ActionId::CROUCH_SLIDE, 0);
        }

        if update_decelerating_speed(cx) {
            return cx.goto(ActionId::IDLE, 0);
        }

        match perform_ground_step(&mut cx.state, cx.world) {
            GroundStep::LeftGround => {
                // decomp discards the return; falls through to `return FALSE`.
                let _ = cx.goto_next(ActionId::FREEFALL, 0);
            }
            GroundStep::HitWall => {
                if slope_class == SurfaceClass::VerySlippery {
                    mario_bonk_reflection(&mut cx.state, true);
                } else {
                    cx.set_forward_speed(0.0);
                }
            }
            GroundStep::None => {}
        }

        if slope_class == SurfaceClass::VerySlippery {
            cx.set_mario_animation(anim_table::MARIO_ANIM_IDLE_HEAD_LEFT);
            // Slide sound + dust particles: audio; skipped.
        } else {
            // (s32)(forwardVel / 4.0f * 0x10000), clamped to >= 0x1000.
            let val0c = ((cx.state.forward_speed / 4.0 * 65536.0) as i32).max(0x1000);
            cx.set_mario_anim_with_accel(anim_table::MARIO_ANIM_WALKING, val0c);
            // play_step_sound(m, 10, 49): audio; skipped.
        }

        ActionResult::Stay
    }
}

/// Standing against a wall (entered from `begin_braking_action` when
/// actionState == 1 after a wall sidestep).
/// decomp: src/game/mario_actions_stationary.c:487
/// (`act_standing_against_wall`).
pub struct StandingAgainstWall;
impl ActionHandler for StandingAgainstWall {
    fn name(&self) -> &'static str {
        "StandingAgainstWall"
    }

    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = check_common_stationary_cancels(cx) {
            return r;
        }

        // INPUT_STOMPED: never set in scope; the decomp would
        // set_mario_action(m, ACT_SHOCKWAVE_BOUNCE, 0).

        if cx.state.input & (f::NONZERO_ANALOG | f::A_PRESSED | f::OFF_FLOOR | f::ABOVE_SLIDE) != 0
        {
            // The mask guarantees one of the four exit bits is set, so the
            // decomp's check_common_action_exits always transitions here.
            if let Some(r) = entry::check_common_action_exits(cx, prev_buttons) {
                return r;
            }
        }

        // INPUT_FIRST_PERSON: never set in scope; the decomp would
        // set_mario_action(m, ACT_FIRST_PERSON, 0).

        if cx.state.input & f::B_PRESSED != 0 {
            return cx.goto(ActionId::PUNCHING, 0);
        }

        cx.set_mario_animation(anim_table::MARIO_ANIM_STAND_AGAINST_WALL);
        stationary_ground_step(&mut cx.state, cx.world);
        ActionResult::Stay
    }
}

// ---------------------------------------------------------------------------
// Minimal stubs (replaced by later phases).
// ---------------------------------------------------------------------------

/// Minimal stub: the full ledge-climb-down sequence lands with AR-11/Phase 6.
pub struct LedgeClimbDown;
impl ActionHandler for LedgeClimbDown {
    fn name(&self) -> &'static str {
        "LedgeClimbDown"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        stationary_ground_step(&mut cx.state, cx.world);
        ActionResult::Stay
    }
}

/// Minimal stub: death sequence lands with a later phase.
pub struct StandingDeath;
impl ActionHandler for StandingDeath {
    fn name(&self) -> &'static str {
        "StandingDeath"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        stationary_ground_step(&mut cx.state, cx.world);
        ActionResult::Stay
    }
}

/// Minimal stub: shockwave-bounce knockback lands with a later phase.
pub struct ShockwaveBounce;
impl ActionHandler for ShockwaveBounce {
    fn name(&self) -> &'static str {
        "ShockwaveBounce"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        stationary_ground_step(&mut cx.state, cx.world);
        ActionResult::Stay
    }
}

/// Minimal stub: squished sequence lands with a later phase.
pub struct Squished;
impl ActionHandler for Squished {
    fn name(&self) -> &'static str {
        "Squished"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        stationary_ground_step(&mut cx.state, cx.world);
        ActionResult::Stay
    }
}
