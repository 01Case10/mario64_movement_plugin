//! Metal-cap submerged actions: the heavy underwater walk/jump family.
//!
//! Verbatim ports from the decomp's `src/game/mario_actions_submerged.c`:
//! `play_metal_water_jumping_sound`, `play_metal_water_walking_sound`,
//! `update_metal_water_walking_speed`, `update_metal_water_jump_speed`,
//! `act_metal_water_standing`, `act_metal_water_walking`,
//! `act_metal_water_jump`, `act_metal_water_falling`, and
//! `act_metal_water_jump_land`. The `act_hold_metal_water_*` variants are
//! intentionally skipped (no held objects in scope).
//!
//! Conventions: every ported item cites its decomp line; sound and particle
//! calls are skipped with a comment, except the `MARIO_ACTION_SOUND_PLAYED`
//! flag side effect of `play_sound_if_no_flag`, which is real gameplay state
//! and is kept via `crate::sound`; `m->input & INPUT_*` reads become
//! `cx.state.input & input_flags::*`; a returned `set_mario_action` becomes
//! `cx.goto` (same-tick), a discarded `set_mario_action` followed by
//! `return FALSE` becomes `cx.goto_next` (next-tick), and a bare
//! `return FALSE` is `ActionResult::Stay`.

use super::water::id as water_id;
use super::{ActionCx, ActionHandler, ActionResult};
use crate::angles::Angle;
use crate::anim_table;
use crate::input::input_flags as f;
use crate::math_util::{approach_f32, approach_s32};
use crate::state::{mario_flags, ActionId};
use super::water_physics::{perform_water_step, stationary_slow_down, WaterStep};
use crate::step::{
    perform_air_step, perform_ground_step, stop_and_set_height_to_floor, AirStep, GroundStep,
};
use crate::trig;

pub mod id {
    use crate::state::ActionId;
    /// decomp: `ACT_METAL_WATER_STANDING` (`include/sm64.h:323`).
    pub const METAL_WATER_STANDING: ActionId = ActionId(0x080042F0);
    /// decomp: `ACT_METAL_WATER_WALKING` (`include/sm64.h:325`).
    pub const METAL_WATER_WALKING: ActionId = ActionId(0x000044F2);
    /// decomp: `ACT_METAL_WATER_FALLING` (`include/sm64.h:327`).
    pub const METAL_WATER_FALLING: ActionId = ActionId(0x000042F4);
    /// decomp: `ACT_METAL_WATER_FALL_LAND` (`include/sm64.h:329`).
    pub const METAL_WATER_FALL_LAND: ActionId = ActionId(0x000042F6);
    /// decomp: `ACT_METAL_WATER_JUMP` (`include/sm64.h:331`).
    pub const METAL_WATER_JUMP: ActionId = ActionId(0x000044F8);
    /// decomp: `ACT_METAL_WATER_JUMP_LAND` (`include/sm64.h:333`).
    pub const METAL_WATER_JUMP_LAND: ActionId = ActionId(0x000044FA);
}

/// decomp: `MARIO_METAL_CAP` (`include/sm64.h:117`). Local until a
/// `METAL_CAP` bit lands in `crate::state::mario_flags`.
const MARIO_METAL_CAP: u32 = 0x00000004;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// decomp: src/game/mario_actions_submerged.c:1100 (`play_metal_water_jumping_sound`).
fn play_metal_water_jumping_sound(cx: &mut ActionCx, landing: bool) {
    // decomp: if (!(m->flags & MARIO_ACTION_SOUND_PLAYED)) m->particleFlags |= PARTICLE_MIST_CIRCLE;
    // particles have no core equivalent; skipped.
    // decomp: play_sound_if_no_flag(m, landing ? SOUND_ACTION_METAL_LAND_WATER
    //                                       : SOUND_ACTION_METAL_JUMP_WATER,
    //                              MARIO_ACTION_SOUND_PLAYED). Audio is skipped;
    // the flag is real state and is kept via the sound stub. (`landing` only
    // selects which sound plays, so it is dropped along with the audio.)
    let _ = landing;
    crate::sound::play_sound_if_no_flag(&mut cx.state, mario_flags::ACTION_SOUND_PLAYED);
}

/// decomp: src/game/mario_actions_submerged.c:1109 (`play_metal_water_walking_sound`).
fn play_metal_water_walking_sound(cx: &ActionCx) {
    // decomp: if (is_anim_past_frame(m, 10) || is_anim_past_frame(m, 49)) {
    //     play_sound(SOUND_ACTION_METAL_STEP_WATER, m->marioObj->header.gfx.cameraToObject);
    //     m->particleFlags |= PARTICLE_DUST;
    // }
    // Checked: `play_sound` (not `play_sound_if_no_flag`) sets no flag — the
    // call is audio-only — and particles have no core equivalent; skipped.
    if cx.is_anim_past_frame(10) || cx.is_anim_past_frame(49) {
        // decomp: audio (play_sound) + PARTICLE_DUST; both skipped.
    }
}

/// decomp: src/game/mario_actions_submerged.c:1116 (`update_metal_water_walking_speed`).
fn update_metal_water_walking_speed(cx: &mut ActionCx) {
    // decomp: f32 val = m->intendedMag / 1.5f;
    let val = cx.state.intended_mag / 1.5;

    // decomp: if (m->forwardVel <= 0.0f) {
    //             m->forwardVel += 1.1f;
    //         } else if (m->forwardVel <= val) {
    //             m->forwardVel += 1.1f - m->forwardVel / 43.0f;
    //         } else if (m->floor->normal.y >= 0.95f) {
    //             m->forwardVel -= 1.0f;
    //         }
    if cx.state.forward_speed <= 0.0 {
        cx.state.forward_speed += 1.1;
    } else if cx.state.forward_speed <= val {
        cx.state.forward_speed += 1.1 - cx.state.forward_speed / 43.0;
    } else if cx.state.floor.map(|fl| fl.normal.y).unwrap_or(1.0) >= 0.95 {
        // The decomp dereferences m->floor unconditionally; a missing floor is
        // treated as flat ground (walk.rs convention).
        cx.state.forward_speed -= 1.0;
    }

    // decomp: if (m->forwardVel > 32.0f) m->forwardVel = 32.0f;
    if cx.state.forward_speed > 32.0 {
        cx.state.forward_speed = 32.0;
    }

    // decomp: m->faceAngle[1] =
    //     m->intendedYaw - approach_s32((s16)(m->intendedYaw - m->faceAngle[1]), 0, 0x800, 0x800);
    // The yaw difference wraps in s16; the final store truncates to s16
    // (walk.rs convention).
    let dyaw = cx.state.intended_yaw.0.wrapping_sub(cx.state.face_yaw.0) as i32;
    let turn = approach_s32(dyaw, 0, 0x800, 0x800);
    cx.state.face_yaw = Angle((cx.state.intended_yaw.0 as i32 - turn) as i16);

    // decomp: m->slideVelX = m->forwardVel * sins(m->faceAngle[1]);
    //         m->slideVelZ = m->forwardVel * coss(m->faceAngle[1]);
    //         m->vel[0] = m->slideVelX;
    //         m->vel[1] = 0.0f;
    //         m->vel[2] = m->slideVelZ;
    cx.state.slide_vel_x = cx.state.forward_speed * trig::sins(cx.state.face_yaw);
    cx.state.slide_vel_z = cx.state.forward_speed * trig::coss(cx.state.face_yaw);
    cx.state.vel.x = cx.state.slide_vel_x;
    cx.state.vel.y = 0.0;
    cx.state.vel.z = cx.state.slide_vel_z;
}

/// decomp: src/game/mario_actions_submerged.c:1142 (`update_metal_water_jump_speed`).
/// Returns true when Mario has risen above `waterLevel - 100` (exit to
/// `ACT_WATER_JUMP`).
fn update_metal_water_jump_speed(cx: &mut ActionCx) -> bool {
    // decomp: UNUSED f32 nextY = m->pos[1] + m->vel[1]; — dead; kept only as this comment.
    // decomp: f32 waterSurface = m->waterLevel - 100;
    let water_surface = cx.state.water_level - 100.0;

    // decomp: if (m->vel[1] > 0.0f && m->pos[1] > waterSurface) return TRUE;
    if cx.state.vel.y > 0.0 && cx.state.pos.y > water_surface {
        return true;
    }

    if cx.state.input & f::NONZERO_ANALOG != 0 {
        // decomp: s16 intendedDYaw = m->intendedYaw - m->faceAngle[1];
        //         m->forwardVel += 0.8f * coss(intendedDYaw);
        //         m->faceAngle[1] += 0x200 * sins(intendedDYaw);
        let intended_dyaw = cx.state.intended_yaw.0.wrapping_sub(cx.state.face_yaw.0);
        cx.state.forward_speed += 0.8 * trig::coss(Angle(intended_dyaw));
        cx.state.face_yaw = cx
            .state
            .face_yaw
            .wrapping_add(Angle((0x200 as f32 * trig::sins(Angle(intended_dyaw))) as i16));
    } else {
        // decomp: m->forwardVel = approach_f32(m->forwardVel, 0.0f, 0.25f, 0.25f);
        cx.state.forward_speed = approach_f32(cx.state.forward_speed, 0.0, 0.25, 0.25);
    }

    // decomp: if (m->forwardVel > 16.0f) m->forwardVel -= 1.0f;
    if cx.state.forward_speed > 16.0 {
        cx.state.forward_speed -= 1.0;
    }

    // decomp: if (m->forwardVel < 0.0f) m->forwardVel += 2.0f;
    if cx.state.forward_speed < 0.0 {
        cx.state.forward_speed += 2.0;
    }

    // decomp: m->vel[0] = m->slideVelX = m->forwardVel * sins(m->faceAngle[1]);
    //         m->vel[2] = m->slideVelZ = m->forwardVel * coss(m->faceAngle[1]);
    cx.state.slide_vel_x = cx.state.forward_speed * trig::sins(cx.state.face_yaw);
    cx.state.slide_vel_z = cx.state.forward_speed * trig::coss(cx.state.face_yaw);
    cx.state.vel.x = cx.state.slide_vel_x;
    cx.state.vel.z = cx.state.slide_vel_z;
    false
}

// ---------------------------------------------------------------------------
// act_metal_water_standing
// ---------------------------------------------------------------------------

/// decomp: src/game/mario_actions_submerged.c:1171 (`act_metal_water_standing`).
pub struct MetalWaterStanding;
impl ActionHandler for MetalWaterStanding {
    fn name(&self) -> &'static str {
        "MetalWaterStanding"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: if (!(m->flags & MARIO_METAL_CAP)) return set_mario_action(m, ACT_WATER_IDLE, 0);
        if cx.state.flags & MARIO_METAL_CAP == 0 {
            return cx.goto(water_id::WATER_IDLE, 0);
        }

        // decomp: if (m->input & INPUT_A_PRESSED) return set_mario_action(m, ACT_METAL_WATER_JUMP, 0);
        if cx.state.input & f::A_PRESSED != 0 {
            return cx.goto(id::METAL_WATER_JUMP, 0);
        }

        // decomp: if (m->input & INPUT_NONZERO_ANALOG) return set_mario_action(m, ACT_METAL_WATER_WALKING, 0);
        if cx.state.input & f::NONZERO_ANALOG != 0 {
            return cx.goto(id::METAL_WATER_WALKING, 0);
        }

        // decomp: switch (m->actionState) {
        //             case 0: set_mario_animation(m, MARIO_ANIM_IDLE_HEAD_LEFT); break;
        //             case 1: set_mario_animation(m, MARIO_ANIM_IDLE_HEAD_RIGHT); break;
        //             case 2: set_mario_animation(m, MARIO_ANIM_IDLE_HEAD_CENTER); break;
        //         }
        match cx.state.action_state {
            0 => {
                cx.set_mario_animation(anim_table::MARIO_ANIM_IDLE_HEAD_LEFT);
            }
            1 => {
                cx.set_mario_animation(anim_table::MARIO_ANIM_IDLE_HEAD_RIGHT);
            }
            2 => {
                cx.set_mario_animation(anim_table::MARIO_ANIM_IDLE_HEAD_CENTER);
            }
            _ => {}
        }

        // decomp: if (is_anim_at_end(m) && ++m->actionState == 3) m->actionState = 0;
        if cx.is_anim_at_end() {
            cx.state.action_state += 1;
            if cx.state.action_state == 3 {
                cx.state.action_state = 0;
            }
        }

        // decomp: stop_and_set_height_to_floor(m);
        stop_and_set_height_to_floor(&mut cx.state);

        // decomp: if (m->pos[1] >= m->waterLevel - 150) m->particleFlags |= PARTICLE_IDLE_WATER_WAVE;
        // particles have no core equivalent; the condition and the flag write are skipped.

        ActionResult::Stay
    }
}

// ---------------------------------------------------------------------------
// act_metal_water_walking
// ---------------------------------------------------------------------------

/// decomp: src/game/mario_actions_submerged.c:1230 (`act_metal_water_walking`).
pub struct MetalWaterWalking;
impl ActionHandler for MetalWaterWalking {
    fn name(&self) -> &'static str {
        "MetalWaterWalking"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: if (!(m->flags & MARIO_METAL_CAP)) return set_mario_action(m, ACT_WATER_IDLE, 0);
        if cx.state.flags & MARIO_METAL_CAP == 0 {
            return cx.goto(water_id::WATER_IDLE, 0);
        }

        // decomp: if (m->input & INPUT_FIRST_PERSON) return set_mario_action(m, ACT_METAL_WATER_STANDING, 0);
        if cx.state.input & f::FIRST_PERSON != 0 {
            return cx.goto(id::METAL_WATER_STANDING, 0);
        }

        // decomp: if (m->input & INPUT_A_PRESSED) return set_mario_action(m, ACT_METAL_WATER_JUMP, 0);
        if cx.state.input & f::A_PRESSED != 0 {
            return cx.goto(id::METAL_WATER_JUMP, 0);
        }

        // decomp: if (m->input & INPUT_UNKNOWN_5) return set_mario_action(m, ACT_METAL_WATER_STANDING, 0);
        if cx.state.input & f::UNKNOWN_5 != 0 {
            return cx.goto(id::METAL_WATER_STANDING, 0);
        }

        // decomp: if ((val04 = (s32)(m->forwardVel / 4.0f * 0x10000)) < 0x1000) val04 = 0x1000;
        let mut val04 = (cx.state.forward_speed / 4.0 * 65536.0) as i32;
        if val04 < 0x1000 {
            val04 = 0x1000;
        }

        // decomp: set_mario_anim_with_accel(m, MARIO_ANIM_WALKING, val04);
        cx.set_mario_anim_with_accel(anim_table::MARIO_ANIM_WALKING, val04);
        // decomp: play_metal_water_walking_sound(m);
        play_metal_water_walking_sound(cx);
        // decomp: update_metal_water_walking_speed(m);
        update_metal_water_walking_speed(cx);

        // decomp: switch (perform_ground_step(m)) {
        //             case GROUND_STEP_LEFT_GROUND:
        //                 set_mario_action(m, ACT_METAL_WATER_FALLING, 1);
        //                 break;
        //             case GROUND_STEP_HIT_WALL:
        //                 m->forwardVel = 0.0f;
        //                 break;
        //         }
        //         return FALSE;
        // The decomp discards set_mario_action's return and falls through to
        // `return FALSE`: the transition runs next tick (goto_next).
        match perform_ground_step(&mut cx.state, cx.world) {
            GroundStep::LeftGround => {
                return cx.goto_next(id::METAL_WATER_FALLING, 1);
            }
            GroundStep::HitWall => {
                // decomp: m->forwardVel = 0.0f;
                cx.state.forward_speed = 0.0;
            }
            GroundStep::None => {}
        }

        ActionResult::Stay
    }
}

// ---------------------------------------------------------------------------
// act_metal_water_jump
// ---------------------------------------------------------------------------

/// decomp: src/game/mario_actions_submerged.c:1312 (`act_metal_water_jump`).
pub struct MetalWaterJump;
impl ActionHandler for MetalWaterJump {
    fn name(&self) -> &'static str {
        "MetalWaterJump"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: if (!(m->flags & MARIO_METAL_CAP)) return set_mario_action(m, ACT_WATER_IDLE, 0);
        if cx.state.flags & MARIO_METAL_CAP == 0 {
            return cx.goto(water_id::WATER_IDLE, 0);
        }

        // decomp: if (update_metal_water_jump_speed(m)) return set_mario_action(m, ACT_WATER_JUMP, 1);
        // The breach exit leaves the metal family for the regular (airborne)
        // ACT_WATER_JUMP with arg 1.
        if update_metal_water_jump_speed(cx) {
            return cx.goto(ActionId::WATER_JUMP, 1);
        }

        // decomp: play_metal_water_jumping_sound(m, FALSE);
        play_metal_water_jumping_sound(cx, false);

        // decomp: set_mario_animation(m, MARIO_ANIM_SINGLE_JUMP);
        cx.set_mario_animation(anim_table::MARIO_ANIM_SINGLE_JUMP);

        // decomp: switch (perform_air_step(m, 0)) {
        //             case AIR_STEP_LANDED:
        //                 set_mario_action(m, ACT_METAL_WATER_JUMP_LAND, 0);
        //                 break;
        //             case AIR_STEP_HIT_WALL:
        //                 m->forwardVel = 0.0f;
        //                 break;
        //         }
        //         return FALSE;
        // The decomp discards set_mario_action's return and falls through to
        // `return FALSE`: the transition runs next tick (goto_next).
        match perform_air_step(&mut cx.state, cx.world, 0) {
            AirStep::Landed => {
                return cx.goto_next(id::METAL_WATER_JUMP_LAND, 0);
            }
            AirStep::HitWall => {
                // decomp: m->forwardVel = 0.0f;
                cx.state.forward_speed = 0.0;
            }
            _ => {}
        }

        ActionResult::Stay
    }
}

// ---------------------------------------------------------------------------
// act_metal_water_falling
// ---------------------------------------------------------------------------

/// decomp: src/game/mario_actions_submerged.c:1366 (`act_metal_water_falling`).
pub struct MetalWaterFalling;
impl ActionHandler for MetalWaterFalling {
    fn name(&self) -> &'static str {
        "MetalWaterFalling"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: if (!(m->flags & MARIO_METAL_CAP)) return set_mario_action(m, ACT_WATER_IDLE, 0);
        if cx.state.flags & MARIO_METAL_CAP == 0 {
            return cx.goto(water_id::WATER_IDLE, 0);
        }

        // decomp: if (m->input & INPUT_NONZERO_ANALOG) {
        //             m->faceAngle[1] += 0x400 * sins(m->intendedYaw - m->faceAngle[1]);
        //         }
        if cx.state.input & f::NONZERO_ANALOG != 0 {
            let dyaw = cx.state.intended_yaw.0.wrapping_sub(cx.state.face_yaw.0);
            cx.state.face_yaw = cx
                .state
                .face_yaw
                .wrapping_add(Angle((0x400 as f32 * trig::sins(Angle(dyaw))) as i16));
        }

        // decomp: set_mario_animation(m, m->actionArg == 0 ? MARIO_ANIM_GENERAL_FALL
        //                                                  : MARIO_ANIM_FALL_FROM_WATER);
        // arg 1 is the walking-off-a-ledge entry (from act_metal_water_walking).
        cx.set_mario_animation(if cx.arg() == 0 {
            anim_table::MARIO_ANIM_GENERAL_FALL
        } else {
            anim_table::MARIO_ANIM_FALL_FROM_WATER
        });

        // decomp: stationary_slow_down(m);
        // NOTE: unresolved dependency — `stationary_slow_down`
        // (src/game/mario_actions_submerged.c:214, the submerged variant using
        // buoyancy) is not ported yet.
        stationary_slow_down(cx);

        // decomp: if (perform_water_step(m) & WATER_STEP_HIT_FLOOR) { // hit floor or cancelled
        //             set_mario_action(m, ACT_METAL_WATER_FALL_LAND, 0);
        //         }
        //         return FALSE;
        // NOTE: unresolved dependency — `perform_water_step`
        // (src/game/mario_actions_submerged.c:167) and `WATER_STEP_HIT_FLOOR`
        // (= 1; `include/sm64.h:86`, so the mask also catches WATER_STEP_CANCELLED
        // = 3, "hit floor or cancelled") are not ported yet.
        // The decomp discards set_mario_action's return and falls through to
        // `return FALSE`: the transition runs next tick (goto_next).
        if matches!(perform_water_step(cx), WaterStep::HitFloor) {
            return cx.goto_next(id::METAL_WATER_FALL_LAND, 0);
        }

        ActionResult::Stay
    }
}

// ---------------------------------------------------------------------------
// act_metal_water_jump_land
// ---------------------------------------------------------------------------

/// decomp: src/game/mario_actions_submerged.c:1408 (`act_metal_water_jump_land`).
pub struct MetalWaterJumpLand;
impl ActionHandler for MetalWaterJumpLand {
    fn name(&self) -> &'static str {
        "MetalWaterJumpLand"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: play_metal_water_jumping_sound(m, TRUE);
        // Note the order: the landing sound plays even when the cap is gone.
        play_metal_water_jumping_sound(cx, true);

        // decomp: if (!(m->flags & MARIO_METAL_CAP)) return set_mario_action(m, ACT_WATER_IDLE, 0);
        if cx.state.flags & MARIO_METAL_CAP == 0 {
            return cx.goto(water_id::WATER_IDLE, 0);
        }

        // decomp: if (m->input & INPUT_NONZERO_ANALOG) return set_mario_action(m, ACT_METAL_WATER_WALKING, 0);
        if cx.state.input & f::NONZERO_ANALOG != 0 {
            return cx.goto(id::METAL_WATER_WALKING, 0);
        }

        // decomp: stop_and_set_height_to_floor(m);
        stop_and_set_height_to_floor(&mut cx.state);

        // decomp: set_mario_animation(m, MARIO_ANIM_LAND_FROM_SINGLE_JUMP);
        cx.set_mario_animation(anim_table::MARIO_ANIM_LAND_FROM_SINGLE_JUMP);

        // decomp: if (is_anim_at_end(m)) return set_mario_action(m, ACT_METAL_WATER_STANDING, 0);
        if cx.is_anim_at_end() {
            return cx.goto(id::METAL_WATER_STANDING, 0);
        }

        ActionResult::Stay
    }
}
