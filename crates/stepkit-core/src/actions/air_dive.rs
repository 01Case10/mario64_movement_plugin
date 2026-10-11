//! Phase 6 air-action ports: verbatim ports of the decomp's dive-family air
//! actions and the ground pound, from `src/game/mario_actions_airborne.c`
//! (`act_dive`, `act_slide_kick`, `act_jump_kick`, `act_forward_rollout`,
//! `act_backward_rollout`, `act_ground_pound`) plus `act_ground_pound_land`
//! from `src/game/mario_actions_stationary.c`.
//!
//! Conventions (see `actions.rs` / `walk.rs`):
//! - `cx.goto(id, arg)` = same-tick (the decomp action function returns
//!   TRUE, propagating `set_mario_action`'s return); `cx.goto_next(id, arg)`
//!   = next-tick (the decomp action function returns FALSE after calling
//!   `set_mario_action`/`drop_and_set_mario_action` and discarding the
//!   return value).
//! - Airborne transitions are routed through
//!   `entry::set_mario_action_airborne` (entry velocities, `peakHeight`
//!   reset, `MARIO_UNKNOWN_08`), mirroring the decomp's `set_mario_action`
//!   group dispatch. Moving-group transitions need no explicit dispatch:
//!   `ActionCx::transition` applies `set_mario_action_moving` itself.
//! - Sound, particle, camera and gfx calls are skipped with a comment;
//!   `play_*` variants that set `ACTION_SOUND_PLAYED` / `MARIO_SOUND_PLAYED`
//!   use the `crate::sound` stubs (those flags are gameplay state, cleared
//!   by `set_mario_action`). Plain `play_sound` has no flag and is skipped.
//! - `should_get_stuck_in_ground` needs snow/sand terrain (`mario.c`); the
//!   sim models no terrain, so it is always false (dead branches kept
//!   structurally with a note).
//! - Object interaction (`mario_check_object_grab`, `heldObj`) is out of
//!   scope (plan §1.2): `heldObj` is always NULL, so dive-landing always
//!   takes the `ACT_DIVE_SLIDE` arm.
//!
//! This module is intentionally NOT registered in `actions.rs` (registration
//! is a separate step); the structs implement `ActionHandler` so a later
//! wiring can swap them in for the pre-Phase-6 wiki-based versions in
//! `air.rs` / `landings.rs`.

use super::air_model::{
    check_fall_damage, check_fall_damage_or_get_stuck, update_air_without_turn,
};
use super::entry::{drop_and_set_mario_action, set_mario_action_airborne, set_mario_action_moving};
use super::{ActionCx, ActionHandler, ActionResult};
use crate::angles::Angle;
use crate::anim_table;
use crate::input::input_flags;
use crate::sound;
use crate::state::{mario_flags, ActionId};
use crate::step::{
    mario_bonk_reflection, mario_set_forward_vel, perform_air_step, stationary_ground_step, AirStep,
};
use crate::world::{CELL_HEIGHT_LIMIT, FLOOR_LOWER_LIMIT};

// --- Action ids the decomp references here but `state.rs` does not (yet)
// declare. Values from `oracle/sm64/include/sm64.h`. ---
/// decomp: `ACT_HEAD_STUCK_IN_GROUND` (`include/sm64.h:384`).
const ACT_HEAD_STUCK_IN_GROUND: ActionId = ActionId(0x0002033A);
/// decomp: `ACT_BUTT_STUCK_IN_GROUND` (`include/sm64.h:385`).
const ACT_BUTT_STUCK_IN_GROUND: ActionId = ActionId(0x0002033B);
/// decomp: `ACT_DIVE_PICKING_UP` (`include/sm64.h:411`). Dead arm here:
/// `heldObj` is always NULL in the sim (no objects in scope).
const ACT_DIVE_PICKING_UP: ActionId = ActionId(0x00000385);

/// decomp: `act_dive` (`src/game/mario_actions_airborne.c:725`).
pub struct Dive;
impl ActionHandler for Dive {
    fn name(&self) -> &'static str {
        "Dive"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: play_mario_sound(m, actionArg == 0 ? SOUND_ACTION_THROW
        //   : SOUND_ACTION_TERRAIN_JUMP, actionArg == 0 ? SOUND_MARIO_HOOHOO : 0)
        // — the audio differs per branch; the flag side effects
        // (ACTION_SOUND_PLAYED + MARIO_SOUND_PLAYED) are identical. Audio skipped.
        sound::play_mario_action_sound(&mut cx.state);
        sound::play_mario_jump_sound(&mut cx.state);

        cx.set_mario_animation(anim_table::MARIO_ANIM_DIVE);
        // decomp: mario_check_object_grab / mario_grab_used_object — no objects in scope.

        update_air_without_turn(cx);

        match perform_air_step(&mut cx.state, cx.world, 0) {
            AirStep::None => {
                if cx.state.vel.y < 0.0 && cx.state.face_pitch.0 > -0x2AAA_i16 {
                    // s16 arithmetic on faceAngle[0].
                    cx.state.face_pitch = Angle(cx.state.face_pitch.0.wrapping_sub(0x200_i16));
                    if cx.state.face_pitch.0 < -0x2AAA_i16 {
                        cx.state.face_pitch = Angle(-0x2AAA_i16);
                    }
                }
                // decomp: m->marioObj->header.gfx.angle[0] = -m->faceAngle[0] — visual only.
            }
            AirStep::Landed => {
            cx.state.land_from = cx.state.action;
                // decomp: should_get_stuck_in_ground(m) — snow/sand terrain;
                // always false in the sim. Dead branch kept structurally.
                let stuck_in_ground = false && cx.state.face_pitch.0 == -0x2AAA_i16;
                if stuck_in_ground {
                    // decomp: `drop_and_set_mario_action(m, ACT_HEAD_STUCK_IN_GROUND, 0)`
                    // — return discarded, the function returns FALSE → next-tick.
                    let _ = drop_and_set_mario_action(cx, ACT_HEAD_STUCK_IN_GROUND, 0);
                    // decomp: m->faceAngle[0] = 0 runs after the transition.
                } else if !check_fall_damage(cx, ActionId::HARD_FORWARD_GROUND_KB) {
                    // decomp: `if (m->heldObj == NULL) set_mario_action(m, ACT_DIVE_SLIDE, 0)`
                    //   else `set_mario_action(m, ACT_DIVE_PICKING_UP, 0)` — return
                    // discarded → next-tick. No objects in scope, so heldObj is
                    // always NULL; the DIVE_PICKING_UP arm is unreachable in the sim.
                    let held_obj = false;
                    if held_obj {
                        let _ = cx.goto_next(ACT_DIVE_PICKING_UP, 0);
                    } else {
                        // The moving-group dispatch (set_mario_action_moving) runs
                        // inside transition().
                        let _ = cx.goto_next(ActionId::DIVE_SLIDE, 0);
                    }
                }
                cx.state.face_pitch = Angle(0);
            }
            AirStep::HitWall => {
                mario_bonk_reflection(&mut cx.state, true);
                cx.state.face_pitch = Angle(0);

                if cx.state.vel.y > 0.0 {
                    cx.state.vel.y = 0.0;
                }

                // decomp: particleFlags |= PARTICLE_VERTICAL_STAR — visual, skipped.
                // decomp: `drop_and_set_mario_action(m, ACT_BACKWARD_AIR_KB, 0)` —
                // return discarded → next-tick.
                let _ = drop_and_set_mario_action(cx, ActionId::BACKWARD_AIR_KB, 0);
            }
            AirStep::HitLavaWall => {
                // decomp: lava_boost_on_wall — out of scope (no lava surfaces).
            }
            _ => {}
        }

        ActionResult::Stay
    }
}

/// decomp: `act_slide_kick` (`src/game/mario_actions_airborne.c:1580`).
///
/// NOTE (plan AR-03 discrepancy): the plan lists a
/// `floor.normal.y >= 0.9848077` condition on the landing bounce, but the
/// decomp has no such check in `act_slide_kick` (that condition belongs to
/// `act_butt_slide_air`, `airborne.c:1435`). Ported verbatim from the decomp.
/// Entry velocities (`vel[1] = 12`, `forwardVel = max(forwardVel, 32)`) live
/// in `set_mario_action_airborne` (`entry.rs`, already ported); the −2
/// gravity comes from the `apply_gravity` chain (ST-07), not this function.
pub struct SlideKick;
impl ActionHandler for SlideKick {
    fn name(&self) -> &'static str {
        "SlideKick"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        if cx.state.action_state == 0 && cx.state.action_timer == 0 {
            // decomp: play_mario_sound(m, SOUND_ACTION_TERRAIN_JUMP, SOUND_MARIO_HOOHOO)
            // — ACTION_SOUND_PLAYED + MARIO_SOUND_PLAYED; audio skipped.
            sound::play_mario_action_sound(&mut cx.state);
            sound::play_mario_jump_sound(&mut cx.state);
            cx.set_mario_animation(anim_table::MARIO_ANIM_SLIDE_KICK);
        }

        // decomp: `if (++(m->actionTimer) > 30 && ...)` — pre-increment.
        cx.state.action_timer += 1;
        if cx.state.action_timer > 30
            && cx.state.pos.y - cx.state.floor_y.unwrap_or(FLOOR_LOWER_LIMIT) > 500.0
        {
            // decomp: `return set_mario_action(m, ACT_FREEFALL, 2)` → TRUE → same-tick.
            let id = set_mario_action_airborne(cx, ActionId::FREEFALL, 2);
            return cx.goto(id, 2);
        }

        update_air_without_turn(cx);

        match perform_air_step(&mut cx.state, cx.world, 0) {
            AirStep::None => {
                // decomp: m->marioObj->header.gfx.angle[0] =
                //   atan2s(m->forwardVel, -m->vel[1]), clamped to 0x1800 — visual only.
            }
            AirStep::Landed => {
            cx.state.land_from = cx.state.action;
                if cx.state.action_state == 0 && cx.state.vel.y < 0.0 {
                    cx.state.vel.y = -cx.state.vel.y / 2.0;
                    cx.state.action_state = 1;
                    cx.state.action_timer = 0;
                } else {
                    // decomp: `set_mario_action(m, ACT_SLIDE_KICK_SLIDE, 0)` —
                    // return discarded → next-tick. Moving dispatch runs in transition().
                    let _ = cx.goto_next(ActionId::SLIDE_KICK_SLIDE, 0);
                }
                // decomp: play_mario_landing_sound → ACTION_SOUND_PLAYED.
                sound::play_mario_landing_sound_once(&mut cx.state);
            }
            AirStep::HitWall => {
                if cx.state.vel.y > 0.0 {
                    cx.state.vel.y = 0.0;
                }

                // decomp: particleFlags |= PARTICLE_VERTICAL_STAR — visual, skipped.
                // decomp: `set_mario_action(m, ACT_BACKWARD_AIR_KB, 0)` —
                // return discarded → next-tick.
                let id = set_mario_action_airborne(cx, ActionId::BACKWARD_AIR_KB, 0);
                let _ = cx.goto_next(id, 0);
            }
            AirStep::HitLavaWall => {
                // decomp: lava_boost_on_wall — out of scope (no lava surfaces).
            }
            _ => {}
        }

        ActionResult::Stay
    }
}

/// decomp: `act_jump_kick` (`src/game/mario_actions_airborne.c:1631`).
///
/// Entry velocity (`vel[1] = 20`) lives in `set_mario_action_airborne`
/// (`entry.rs`, already ported).
pub struct JumpKick;
impl ActionHandler for JumpKick {
    fn name(&self) -> &'static str {
        "JumpKick"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        if cx.state.action_state == 0 {
            // decomp: play_sound_if_no_flag(m, SOUND_MARIO_PUNCH_HOO, MARIO_ACTION_SOUND_PLAYED)
            sound::play_sound_if_no_flag(&mut cx.state, mario_flags::ACTION_SOUND_PLAYED);
            // decomp: m->marioObj->header.gfx.animInfo.animID = -1 — forces anim reinit.
            cx.state.anim.id = -1;
            cx.set_mario_animation(anim_table::MARIO_ANIM_AIR_KICK);
            cx.state.action_state = 1;
        }

        let anim_frame = cx.state.anim.frame;
        if anim_frame == 0 {
            // decomp: m->marioBodyState->punchState = (2 << 6) | 6 — body state; no objects in scope.
        }
        if anim_frame >= 0 && anim_frame < 8 {
            // decomp: m->flags |= MARIO_KICKING — read only by object
            // interactions (out of scope); skipped.
        }

        update_air_without_turn(cx);

        match perform_air_step(&mut cx.state, cx.world, 0) {
            AirStep::Landed => {
            cx.state.land_from = cx.state.action;
                if !check_fall_damage_or_get_stuck(cx, ActionId::HARD_BACKWARD_GROUND_KB) {
                    // decomp: `set_mario_action(m, ACT_FREEFALL_LAND, 0)` —
                    // return discarded → next-tick. Stationary group: no dispatch arm.
                    let _ = cx.goto_next(ActionId::FREEFALL_LAND, 0);
                }
            }
            AirStep::HitWall => {
                mario_set_forward_vel(&mut cx.state, 0.0);
            }
            _ => {}
        }

        ActionResult::Stay
    }
}

/// decomp: `act_forward_rollout` (`src/game/mario_actions_airborne.c:1353`).
pub struct ForwardRollout;
impl ActionHandler for ForwardRollout {
    fn name(&self) -> &'static str {
        "ForwardRollout"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        if cx.state.action_state == 0 {
            cx.state.vel.y = 30.0;
            cx.state.action_state = 1;
        }

        // decomp: play_mario_sound(m, SOUND_ACTION_TERRAIN_JUMP, 0) —
        // ACTION_SOUND_PLAYED + MARIO_SOUND_PLAYED; audio skipped.
        sound::play_mario_action_sound(&mut cx.state);
        sound::play_mario_jump_sound(&mut cx.state);

        update_air_without_turn(cx);

        match perform_air_step(&mut cx.state, cx.world, 0) {
            AirStep::None => {
                if cx.state.action_state == 1 {
                    // decomp: set_mario_animation returns the anim frame.
                    if cx.set_mario_animation(anim_table::MARIO_ANIM_FORWARD_SPINNING) == 4 {
                        // decomp: play_sound(SOUND_ACTION_SPIN, ...) — plain, no flag; skipped.
                    }
                } else {
                    cx.set_mario_animation(anim_table::MARIO_ANIM_GENERAL_FALL);
                }
            }
            AirStep::Landed => {
            cx.state.land_from = cx.state.action;
                // decomp: `set_mario_action(m, ACT_FREEFALL_LAND_STOP, 0)` —
                // return discarded → next-tick; the landing sound then sets
                // ACTION_SOUND_PLAYED on the NEW action's flags (order kept).
                let _ = cx.goto_next(ActionId::FREEFALL_LAND_STOP, 0);
                sound::play_mario_landing_sound_once(&mut cx.state);
            }
            AirStep::HitWall => {
                mario_set_forward_vel(&mut cx.state, 0.0);
            }
            AirStep::HitLavaWall => {
                // decomp: lava_boost_on_wall — out of scope (no lava surfaces).
            }
            _ => {}
        }

        if cx.state.action_state == 1 && cx.is_anim_past_end() {
            cx.state.action_state = 2;
        }
        ActionResult::Stay
    }
}

/// decomp: `act_backward_rollout` (`src/game/mario_actions_airborne.c:1394`).
pub struct BackwardRollout;
impl ActionHandler for BackwardRollout {
    fn name(&self) -> &'static str {
        "BackwardRollout"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        if cx.state.action_state == 0 {
            cx.state.vel.y = 30.0;
            cx.state.action_state = 1;
        }

        // decomp: play_mario_sound(m, SOUND_ACTION_TERRAIN_JUMP, 0) —
        // ACTION_SOUND_PLAYED + MARIO_SOUND_PLAYED; audio skipped.
        sound::play_mario_action_sound(&mut cx.state);
        sound::play_mario_jump_sound(&mut cx.state);

        update_air_without_turn(cx);

        match perform_air_step(&mut cx.state, cx.world, 0) {
            AirStep::None => {
                if cx.state.action_state == 1 {
                    // decomp: set_mario_animation returns the anim frame.
                    if cx.set_mario_animation(anim_table::MARIO_ANIM_BACKWARD_SPINNING) == 4 {
                        // decomp: play_sound(SOUND_ACTION_SPIN, ...) — plain, no flag; skipped.
                    }
                } else {
                    cx.set_mario_animation(anim_table::MARIO_ANIM_GENERAL_FALL);
                }
            }
            AirStep::Landed => {
            cx.state.land_from = cx.state.action;
                // decomp: `set_mario_action(m, ACT_FREEFALL_LAND_STOP, 0)` —
                // return discarded → next-tick; the landing sound then sets
                // ACTION_SOUND_PLAYED on the NEW action's flags (order kept).
                let _ = cx.goto_next(ActionId::FREEFALL_LAND_STOP, 0);
                sound::play_mario_landing_sound_once(&mut cx.state);
            }
            AirStep::HitWall => {
                mario_set_forward_vel(&mut cx.state, 0.0);
            }
            AirStep::HitLavaWall => {
                // decomp: lava_boost_on_wall — out of scope (no lava surfaces).
            }
            _ => {}
        }

        // NOTE: forward rollout gates on is_anim_past_end(); backward rollout
        // gates on animFrame == 2 (airborne.c:1429).
        if cx.state.action_state == 1 && cx.state.anim.frame == 2 {
            cx.state.action_state = 2;
        }
        ActionResult::Stay
    }
}

/// decomp: `act_ground_pound` (`src/game/mario_actions_airborne.c:918`).
///
/// Animation/frame-count driven: `actionState == 0` is the wind-up rise
/// (ends at `actionTimer >= curAnim->loopEnd + 4`), `actionState == 1` is
/// the fall. NOT REVIEWED by the plan — ported straight from the decomp.
pub struct GroundPound;
impl ActionHandler for GroundPound {
    fn name(&self) -> &'static str {
        "GroundPound"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: play_sound_if_no_flag(m, SOUND_ACTION_THROW, MARIO_ACTION_SOUND_PLAYED)
        sound::play_sound_if_no_flag(&mut cx.state, mario_flags::ACTION_SOUND_PLAYED);

        if cx.state.action_state == 0 {
            if cx.state.action_timer < 10 {
                // decomp: yOffset = 20 - 2 * m->actionTimer (s32 math).
                let y_offset = (20 - 2 * cx.state.action_timer) as f32;
                if cx.state.pos.y + y_offset + 160.0
                    < cx.state.ceil_y.unwrap_or(CELL_HEIGHT_LIMIT)
                {
                    cx.state.pos.y += y_offset;
                    cx.state.peak_height = cx.state.pos.y;
                    // decomp: vec3f_copy(m->marioObj->header.gfx.pos, m->pos) — visual only.
                }
            }

            cx.state.vel.y = -50.0;
            cx.set_forward_speed(0.0);

            let anim = if cx.state.action_arg == 0 {
                anim_table::MARIO_ANIM_START_GROUND_POUND
            } else {
                anim_table::MARIO_ANIM_TRIPLE_JUMP_GROUND_POUND
            };
            cx.set_mario_animation(anim);
            if cx.state.action_timer == 0 {
                // decomp: play_sound(SOUND_ACTION_SPIN, ...) — plain, no flag; skipped.
            }

            cx.state.action_timer += 1;
            // decomp: m->actionTimer >= curAnim->loopEnd + 4.
            let loop_end = anim_table::anim_header(cx.state.anim.id).loop_end as i32;
            if cx.state.action_timer as i32 >= loop_end + 4 {
                // decomp: play_sound(SOUND_MARIO_GROUND_POUND_WAH, ...) — plain, no flag; skipped.
                cx.state.action_state = 1;
            }
        } else {
            cx.set_mario_animation(anim_table::MARIO_ANIM_GROUND_POUND);

            match perform_air_step(&mut cx.state, cx.world, 0) {
                AirStep::Landed => {
            cx.state.land_from = cx.state.action;
                    // decomp: should_get_stuck_in_ground(m) — snow/sand terrain;
                    // always false in the sim. Dead branch kept structurally.
                    let stuck_in_ground = false;
                    if stuck_in_ground {
                        // decomp: `set_mario_action(m, ACT_BUTT_STUCK_IN_GROUND, 0)` —
                        // return discarded → next-tick.
                        let _ = cx.goto_next(ACT_BUTT_STUCK_IN_GROUND, 0);
                    } else {
                        // decomp: play_mario_heavy_landing_sound → ACTION_SOUND_PLAYED.
                        sound::play_mario_heavy_landing_sound_once(&mut cx.state);
                        if !check_fall_damage(cx, ActionId::HARD_BACKWARD_GROUND_KB) {
                            // decomp: particleFlags |= PARTICLE_MIST_CIRCLE |
                            //   PARTICLE_HORIZONTAL_STAR — visual, skipped.
                            // decomp: `set_mario_action(m, ACT_GROUND_POUND_LAND, 0)` —
                            // return discarded → next-tick.
                            let _ = cx.goto_next(ActionId::GROUND_POUND_LAND, 0);
                        }
                    }
                    // decomp: set_camera_shake_from_hit(SHAKE_GROUND_POUND) — camera; skipped.
                }
                AirStep::HitWall => {
                    mario_set_forward_vel(&mut cx.state, -16.0);
                    if cx.state.vel.y > 0.0 {
                        cx.state.vel.y = 0.0;
                    }

                    // decomp: particleFlags |= PARTICLE_VERTICAL_STAR — visual, skipped.
                    // decomp: `set_mario_action(m, ACT_BACKWARD_AIR_KB, 0)` —
                    // return discarded → next-tick.
                    let id = set_mario_action_airborne(cx, ActionId::BACKWARD_AIR_KB, 0);
                    let _ = cx.goto_next(id, 0);
                }
                // NOTE: unlike the other air actions, act_ground_pound has NO
                // AIR_STEP_HIT_LAVA_WALL arm in the decomp.
                _ => {}
            }
        }

        ActionResult::Stay
    }
}

/// decomp: `act_ground_pound_land` (`src/game/mario_actions_stationary.c:1037`).
pub struct GroundPoundLand;
impl ActionHandler for GroundPoundLand {
    fn name(&self) -> &'static str {
        "GroundPoundLand"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        cx.state.action_state = 1;
        let input = cx.state.input;
        if input & input_flags::STOMPED != 0 {
            // decomp: `return drop_and_set_mario_action(m, ACT_SHOCKWAVE_BOUNCE, 0)`
            // → TRUE → same-tick.
            return drop_and_set_mario_action(cx, ActionId::SHOCKWAVE_BOUNCE, 0);
        }
        if input & input_flags::OFF_FLOOR != 0 {
            // decomp: `return set_mario_action(m, ACT_FREEFALL, 0)` → TRUE → same-tick.
            let id = set_mario_action_airborne(cx, ActionId::FREEFALL, 0);
            return cx.goto(id, 0);
        }
        if input & input_flags::ABOVE_SLIDE != 0 {
            // decomp: `return set_mario_action(m, ACT_BUTT_SLIDE, 0)` → TRUE → same-tick.
            // Moving-group dispatch (set_mario_action_moving) is explicit here,
            // matching the decomp's set_mario_action routing; transition()
            // would apply it again idempotently.
            let id = set_mario_action_moving(cx, ActionId::BUTT_SLIDE);
            return cx.goto(id, 0);
        }
        landing_step(cx, anim_table::MARIO_ANIM_GROUND_POUND_LANDING, ActionId::BUTT_SLIDE_STOP)
    }
}

/// decomp: `landing_step` (`src/game/mario_actions_stationary.c:830`).
/// The decomp returns FALSE after `set_mario_action`, so the end action
/// runs next-tick (`goto_next`), not same-tick.
fn landing_step(cx: &mut ActionCx, anim: i32, end_action: ActionId) -> ActionResult {
    stationary_ground_step(&mut cx.state, cx.world);
    cx.set_mario_animation(anim);
    if cx.is_anim_at_end() {
        // The decomp's `return set_mario_action(m, action, 0)` return value is
        // discarded by landing_step's FALSE return; the moving dispatch (for
        // moving-group end actions) runs inside transition().
        return cx.goto_next(end_action, 0);
    }
    ActionResult::Stay
}
