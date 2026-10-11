//! Verbatim ports of the decomp's basic swim actions from
//! `src/game/mario_actions_submerged.c` (lines 305-660 and 953-1037):
//! `act_water_idle`, `act_water_action_end`, `act_breaststroke`,
//! `act_swimming_end`, `act_flutter_kick`, and `act_water_plunge`, plus the
//! shared helpers they call (`common_idle_step`, `common_swimming_step`,
//! `reset_bob_variables`, `surface_swim_bob`, `play_swimming_noise`,
//! `check_water_jump`).
//!
//! The HOLD variants (`act_hold_water_idle`, `act_hold_water_action_end`,
//! `act_hold_breaststroke`, `act_hold_swimming_end`, `act_hold_flutter_kick`)
//! are intentionally skipped: held objects are out of scope.
//!
//! Conventions: every ported item cites its decomp line; `set_mario_action`
//! calls whose return the decomp returns become `cx.goto` (same-tick); calls
//! whose return the decomp discards fall through to `return FALSE`, modeled
//! as `cx.goto_next` — the transition side effects happen now, but the new
//! action first runs next tick — with the handler returning
//! `ActionResult::Stay`.
//!
//! NOTE: this module is not yet declared in `actions.rs` (existing files may
//! not be modified); it assumes `pub mod water_swim;` alongside
//! `pub mod water_physics;` there, and reaches both through `super::`.

use super::water::id as swim_id;
use super::water_physics::{
    perform_water_step, set_swimming_at_surface_particles, stationary_slow_down,
    swimming_near_surface, update_swimming_pitch, update_swimming_speed, update_swimming_yaw,
    update_water_pitch, WaterStep, MARIO_METAL_CAP, MIN_SWIM_SPEED, MIN_SWIM_STRENGTH,
    S_BOB_HEIGHT_BITS, S_BOB_INCREMENT, S_BOB_TIMER,
};
use super::{ActionCx, ActionHandler, ActionResult};
use crate::angles::Angle;
use crate::anim_table;
use crate::input::input_flags as f;
use crate::math_util::approach_f32;
use crate::state::{act_flags, ActionId};
use crate::trig;
use crate::world::{CollisionWorld, FLOOR_LOWER_LIMIT};
use std::sync::atomic::Ordering;

/// Action IDs the swim ports transition to that exist in the decomp but not
/// (yet) in `crate::state::ActionId`. Water-plunge/fluid swim targets already
/// covered by `ActionId`/`water::id` are reused from there.
/// decomp: `oracle/sm64/include/sm64.h:306-328`.
pub mod id {
    use crate::state::ActionId;
    /// `ACT_WATER_PUNCH` (0x300024E1).
    pub const WATER_PUNCH: ActionId = ActionId(0x300024E1);
    /// `ACT_METAL_WATER_FALLING` (0x000042F4).
    pub const METAL_WATER_FALLING: ActionId = ActionId(0x000042F4);
    /// `ACT_HOLD_WATER_ACTION_END` (0x300022C3) — unreachable without objects.
    pub const HOLD_WATER_ACTION_END: ActionId = ActionId(0x300022C3);
    /// `ACT_HOLD_FLUTTER_KICK` (0x300024D5) — unreachable without objects.
    pub const HOLD_FLUTTER_KICK: ActionId = ActionId(0x300024D5);
    /// `ACT_HOLD_METAL_WATER_FALLING` (0x000042F5) — unreachable without objects.
    pub const HOLD_METAL_WATER_FALLING: ActionId = ActionId(0x000042F5);
}

/// decomp: `find_floor_slope` (`src/game/mario.c:689`), ported here because
/// `common_swimming_step` (submerged.c:449) is its only water caller and the
/// core has no equivalent yet.
fn find_floor_slope(cx: &ActionCx, yaw_offset: Angle) -> i16 {
    let yaw = cx.state.face_yaw.wrapping_add(yaw_offset);
    let x = trig::sins(yaw) * 5.0;
    let z = trig::coss(yaw) * 5.0;

    let forward_floor_y = cx
        .world
        .find_floor(
            cx.state.pos.x + x,
            cx.state.pos.y + 100.0,
            cx.state.pos.z + z,
        )
        .map(|f| f.y)
        .unwrap_or(FLOOR_LOWER_LIMIT);
    let backward_floor_y = cx
        .world
        .find_floor(
            cx.state.pos.x - x,
            cx.state.pos.y + 100.0,
            cx.state.pos.z - z,
        )
        .map(|f| f.y)
        .unwrap_or(FLOOR_LOWER_LIMIT);

    // If Mario is near OOB, these floorY's can sometimes be -11000.
    // This will cause these to be off and give improper slopes.
    // (decomp comment, verbatim, from find_floor_slope.)
    let forward_y_delta = forward_floor_y - cx.state.pos.y;
    let backward_y_delta = cx.state.pos.y - backward_floor_y;

    if forward_y_delta * forward_y_delta < backward_y_delta * backward_y_delta {
        trig::atan2s(5.0, forward_y_delta).0
    } else {
        trig::atan2s(5.0, backward_y_delta).0
    }
}

/// decomp: src/game/mario_actions_submerged.c:305 (`common_idle_step`).
pub fn common_idle_step(cx: &mut ActionCx, animation: i32, arg: i32) {
    // `s16 *val = &m->marioBodyState->headAngle[0];` — the core has no
    // body-state head angle; the approach on it below is skipped (visual only).

    update_swimming_yaw(cx);
    update_swimming_pitch(cx);
    update_swimming_speed(cx, MIN_SWIM_SPEED);
    perform_water_step(cx);
    update_water_pitch(cx);

    // Head-angle approach (`faceAngle[0] / 2` when pitched down, else 0) —
    // head angle not modeled in the core; skipped.

    if arg == 0 {
        cx.set_mario_animation(animation);
    } else {
        cx.set_mario_anim_with_accel(animation, arg);
    }

    set_swimming_at_surface_particles(cx);
}

/// decomp: src/game/mario_actions_submerged.c:329 (`act_water_idle`).
pub struct ActWaterIdle;
impl ActionHandler for ActWaterIdle {
    fn name(&self) -> &'static str {
        "ActWaterIdle"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        let mut val = 0x10000;

        if cx.state.flags & MARIO_METAL_CAP != 0 {
            return cx.goto(id::METAL_WATER_FALLING, 1);
        }

        if cx.state.input & f::B_PRESSED != 0 {
            return cx.goto(id::WATER_PUNCH, 0);
        }

        if cx.state.input & f::A_PRESSED != 0 {
            return cx.goto(swim_id::BREASTSTROKE, 0);
        }

        if cx.state.face_pitch.0 < -0x1000 {
            val = 0x30000;
        }

        common_idle_step(cx, anim_table::MARIO_ANIM_WATER_IDLE, val);
        ActionResult::Stay
    }
}

/// decomp: src/game/mario_actions_submerged.c:373 (`act_water_action_end`).
pub struct ActWaterActionEnd;
impl ActionHandler for ActWaterActionEnd {
    fn name(&self) -> &'static str {
        "ActWaterActionEnd"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        if cx.state.flags & MARIO_METAL_CAP != 0 {
            return cx.goto(id::METAL_WATER_FALLING, 1);
        }

        if cx.state.input & f::B_PRESSED != 0 {
            return cx.goto(id::WATER_PUNCH, 0);
        }

        if cx.state.input & f::A_PRESSED != 0 {
            return cx.goto(swim_id::BREASTSTROKE, 0);
        }

        common_idle_step(cx, anim_table::MARIO_ANIM_WATER_ACTION_END, 0);
        if cx.is_anim_at_end() {
            // The decomp discards the `set_mario_action` return and falls
            // through to `return FALSE` → the transition lands now but the
            // new action first runs next tick.
            let _ = cx.goto_next(swim_id::WATER_IDLE, 0);
        }
        ActionResult::Stay
    }
}

/// decomp: src/game/mario_actions_submerged.c:419 (`reset_bob_variables`).
pub fn reset_bob_variables(cx: &ActionCx) {
    S_BOB_TIMER.store(0, Ordering::Relaxed);
    S_BOB_INCREMENT.store(0x800, Ordering::Relaxed);
    // decomp: `sBobHeight = m->faceAngle[0] / 256.0f + 20.0f;`
    // (s16 → float division).
    S_BOB_HEIGHT_BITS.store(
        (cx.state.face_pitch.0 as f32 / 256.0 + 20.0).to_bits(),
        Ordering::Relaxed,
    );
}

/// decomp: src/game/mario_actions_submerged.c:428 (`surface_swim_bob`).
///
/// Controls the bobbing that happens when you swim near the surface.
/// NOTE: the bob offset applies only to `m->marioObj->header.gfx.pos[1]`
/// (the render mirror) — no core equivalent, so the gated add is skipped
/// while the timer/increment bookkeeping is kept verbatim.
pub fn surface_swim_bob(cx: &ActionCx) {
    if S_BOB_INCREMENT.load(Ordering::Relaxed) != 0
        && cx.state.pos.y > cx.state.water_level - 85.0
        && cx.state.face_pitch.0 >= 0
    {
        let timer = S_BOB_TIMER.load(Ordering::Relaxed) + S_BOB_INCREMENT.load(Ordering::Relaxed);
        S_BOB_TIMER.store(timer, Ordering::Relaxed);
        if timer >= 0 {
            // `m->marioObj->header.gfx.pos[1] += sBobHeight * sins(sBobTimer);`
            // gfx mirror only — no core equivalent; skipped.
            let _bob_height = f32::from_bits(S_BOB_HEIGHT_BITS.load(Ordering::Relaxed));
            let _bob_angle = Angle(timer as i16);
            return;
        }
    }

    S_BOB_INCREMENT.store(0, Ordering::Relaxed);
}

/// decomp: src/game/mario_actions_submerged.c:439 (`common_swimming_step`).
pub fn common_swimming_step(cx: &mut ActionCx, swim_strength: u16) {
    update_swimming_yaw(cx);
    update_swimming_pitch(cx);
    update_swimming_speed(cx, swim_strength as f32 / 10.0);

    match perform_water_step(cx) {
        WaterStep::HitFloor => {
            // decomp: `floorPitch = -find_floor_slope(m, -0x8000);`
            let floor_pitch = -find_floor_slope(cx, Angle(0x8000u16 as i16));
            if (cx.state.face_pitch.0 as i32) < floor_pitch as i32 {
                cx.state.face_pitch = Angle(floor_pitch);
            }
        }
        WaterStep::HitCeiling => {
            if cx.state.face_pitch.0 > -0x3000 {
                cx.state.face_pitch = Angle(cx.state.face_pitch.0 - 0x100);
            }
        }
        WaterStep::HitWall => {
            // decomp: `if (m->controller->stickY == 0.0f)`.
            if cx.proc.stick_y == 0.0 {
                if cx.state.face_pitch.0 > 0 {
                    let mut p = cx.state.face_pitch.0 as i32 + 0x200;
                    if p > 0x3F00 {
                        p = 0x3F00;
                    }
                    cx.state.face_pitch = Angle(p as i16);
                } else {
                    let mut p = cx.state.face_pitch.0 as i32 - 0x200;
                    if p < -0x3F00 {
                        p = -0x3F00;
                    }
                    cx.state.face_pitch = Angle(p as i16);
                }
            }
        }
        _ => {}
    }

    update_water_pitch(cx);
    // `m->marioBodyState->headAngle[0] = approach_s32(..., 0, 0x200, 0x200);`
    // head angle not modeled in the core; skipped.

    surface_swim_bob(cx);
    set_swimming_at_surface_particles(cx);
}

/// decomp: src/game/mario_actions_submerged.c:485 (`play_swimming_noise`).
///
/// The guarded `play_sound(SOUND_ACTION_UNKNOWN434, ...)` is plain
/// `play_sound` (sets no `MARIO_*_SOUND_PLAYED` flag) — pure audio, skipped.
/// The anim-frame check is kept verbatim for structure.
pub fn play_swimming_noise(cx: &ActionCx) {
    // decomp: `m->marioObj->header.gfx.animInfo.animFrame` (s16).
    let anim_frame = cx.state.anim.frame as i16;

    // This must be one line to match on -O2
    if anim_frame == 0 || anim_frame == 12 {
        // play_sound(SOUND_ACTION_UNKNOWN434, ...): audio only, no core equivalent.
    }
}

/// decomp: src/game/mario_actions_submerged.c:492 (`check_water_jump`).
///
/// Returns `Some(ActionResult)` when the jump fires — the decomp's
/// `return set_mario_action(...)` (TRUE → same-tick) — and `None` for the
/// decomp's `return FALSE`.
pub fn check_water_jump(cx: &mut ActionCx) -> Option<ActionResult> {
    // decomp: `s32 probe = (s32)(m->pos[1] + 1.5f);` (f32→s32 truncates).
    let probe = (cx.state.pos.y + 1.5) as i32;

    if cx.state.input & f::A_PRESSED != 0 {
        if probe as f32 >= cx.state.water_level - 80.0
            && cx.state.face_pitch.0 >= 0
            && cx.proc.stick_y < -60.0
        {
            cx.state.angle_vel = [Angle(0), Angle(0), Angle(0)];

            cx.state.vel.y = 62.0;

            // `m->heldObj` is out of scope (no objects): the decomp's
            // `ACT_HOLD_WATER_JUMP` arm is unreachable; always ACT_WATER_JUMP.
            return Some(cx.goto(ActionId::WATER_JUMP, 0));
        }
    }

    None
}

/// decomp: src/game/mario_actions_submerged.c:512 (`act_breaststroke`).
pub struct ActBreaststroke;
impl ActionHandler for ActBreaststroke {
    fn name(&self) -> &'static str {
        "ActBreaststroke"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        if cx.state.action_arg == 0 {
            cx.state.swim_strength = MIN_SWIM_STRENGTH;
        }

        if cx.state.flags & MARIO_METAL_CAP != 0 {
            return cx.goto(id::METAL_WATER_FALLING, 1);
        }

        if cx.state.input & f::B_PRESSED != 0 {
            return cx.goto(id::WATER_PUNCH, 0);
        }

        cx.state.action_timer += 1;
        if cx.state.action_timer == 14 {
            return cx.goto(swim_id::FLUTTER_KICK, 0);
        }

        if let Some(r) = check_water_jump(cx) {
            return r;
        }

        if cx.state.action_timer < 6 {
            cx.state.forward_speed += 0.5;
        }

        if cx.state.action_timer >= 9 {
            cx.state.forward_speed += 1.5;
        }

        if cx.state.action_timer >= 2 {
            if cx.state.action_timer < 6 && cx.state.input & f::A_PRESSED != 0 {
                cx.state.action_state = 1;
            }

            if cx.state.action_timer == 9 && cx.state.action_state == 1 {
                crate::anim::set_anim_to_frame(&mut cx.state, 0);
                cx.state.action_state = 0;
                cx.state.action_timer = 1;
                cx.state.swim_strength = MIN_SWIM_STRENGTH;
            }
        }

        if cx.state.action_timer == 1 {
            // play_sound(SOUND_ACTION_SWIM / SOUND_ACTION_SWIM_FAST, ...):
            // plain `play_sound` — pure audio, skipped.
            reset_bob_variables(cx);
        }

        // `#if ENABLE_RUMBLE` block (func_sh_8024CA04): no rumble in the core; skipped.

        cx.set_mario_animation(anim_table::MARIO_ANIM_SWIM_PART1);
        common_swimming_step(cx, cx.state.swim_strength);

        ActionResult::Stay
    }
}

/// decomp: src/game/mario_actions_submerged.c:572 (`act_swimming_end`).
pub struct ActSwimmingEnd;
impl ActionHandler for ActSwimmingEnd {
    fn name(&self) -> &'static str {
        "ActSwimmingEnd"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        if cx.state.flags & MARIO_METAL_CAP != 0 {
            return cx.goto(id::METAL_WATER_FALLING, 1);
        }

        if cx.state.input & f::B_PRESSED != 0 {
            return cx.goto(id::WATER_PUNCH, 0);
        }

        if cx.state.action_timer >= 15 {
            return cx.goto(swim_id::WATER_ACTION_END, 0);
        }

        if let Some(r) = check_water_jump(cx) {
            return r;
        }

        if cx.state.input & f::A_DOWN != 0 && cx.state.action_timer >= 7 {
            if cx.state.action_timer == 7 && cx.state.swim_strength < 280 {
                cx.state.swim_strength += 10;
            }
            return cx.goto(swim_id::BREASTSTROKE, 1);
        }

        if cx.state.action_timer >= 7 {
            cx.state.swim_strength = MIN_SWIM_STRENGTH;
        }

        cx.state.action_timer += 1;

        cx.state.forward_speed -= 0.25;
        cx.set_mario_animation(anim_table::MARIO_ANIM_SWIM_PART2);
        common_swimming_step(cx, cx.state.swim_strength);

        ActionResult::Stay
    }
}

/// decomp: src/game/mario_actions_submerged.c:609 (`act_flutter_kick`).
pub struct ActFlutterKick;
impl ActionHandler for ActFlutterKick {
    fn name(&self) -> &'static str {
        "ActFlutterKick"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        if cx.state.flags & MARIO_METAL_CAP != 0 {
            return cx.goto(id::METAL_WATER_FALLING, 1);
        }

        if cx.state.input & f::B_PRESSED != 0 {
            return cx.goto(id::WATER_PUNCH, 0);
        }

        if cx.state.input & f::A_DOWN == 0 {
            if cx.state.action_timer == 0 && cx.state.swim_strength < 280 {
                cx.state.swim_strength += 10;
            }
            return cx.goto(swim_id::SWIMMING_END, 0);
        }

        cx.state.forward_speed = approach_f32(cx.state.forward_speed, 12.0, 0.1, 0.15);
        cx.state.action_timer = 1;
        cx.state.swim_strength = MIN_SWIM_STRENGTH;

        if cx.state.forward_speed < 14.0 {
            play_swimming_noise(cx);
            cx.set_mario_animation(anim_table::MARIO_ANIM_FLUTTERKICK);
        }

        common_swimming_step(cx, cx.state.swim_strength);

        ActionResult::Stay
    }
}

/// decomp: src/game/mario_actions_submerged.c:953 (`act_water_plunge`).
pub struct ActWaterPlunge;
impl ActionHandler for ActWaterPlunge {
    fn name(&self) -> &'static str {
        "ActWaterPlunge"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // `s32 stateFlags = m->heldObj != NULL;` — no objects in the core,
        // so the base is always 0 (the hold arms stay verbatim but unreachable).
        let mut state_flags: i32 = 0;

        let end_vspeed = if swimming_near_surface(cx) { 0.0 } else { -5.0 };

        if cx.state.flags & MARIO_METAL_CAP != 0 {
            state_flags |= 4;
        } else if cx.state.prev_action.0 & act_flags::DIVING != 0 || cx.state.input & f::A_DOWN != 0
        {
            state_flags |= 2;
        }

        cx.state.action_timer += 1;

        stationary_slow_down(cx);

        let step_result = perform_water_step(cx);

        if cx.state.action_state == 0 {
            // play_sound(SOUND_ACTION_UNKNOWN430, ...): plain `play_sound` —
            // pure audio, skipped.
            if cx.state.peak_height - cx.state.pos.y > 1150.0 {
                // play_sound(SOUND_MARIO_HAHA_2, ...): plain `play_sound` —
                // pure audio, skipped.
            }

            // `m->particleFlags |= PARTICLE_WATER_SPLASH;` — no particle
            // flags in the core; skipped.
            cx.state.action_state = 1;
            // `#if ENABLE_RUMBLE` block (queue_rumble_data): skipped.
        }

        if step_result == WaterStep::HitFloor
            || cx.state.vel.y >= end_vspeed
            || cx.state.action_timer > 20
        {
            // The decomp discards every `set_mario_action` return here and
            // falls through to `return FALSE`, so the transition lands now
            // but the new action first runs next tick (`goto_next`).
            let _ = match state_flags {
                0 => cx.goto_next(swim_id::WATER_ACTION_END, 0),
                1 => cx.goto_next(id::HOLD_WATER_ACTION_END, 0),
                2 => cx.goto_next(swim_id::FLUTTER_KICK, 0),
                3 => cx.goto_next(id::HOLD_FLUTTER_KICK, 0),
                4 => cx.goto_next(id::METAL_WATER_FALLING, 0),
                _ => cx.goto_next(id::HOLD_METAL_WATER_FALLING, 0),
            };
            S_BOB_INCREMENT.store(0, Ordering::Relaxed);
        }

        match state_flags {
            0 => {
                cx.set_mario_animation(anim_table::MARIO_ANIM_WATER_ACTION_END);
            }
            1 => {
                cx.set_mario_animation(anim_table::MARIO_ANIM_WATER_ACTION_END_WITH_OBJ);
            }
            2 => {
                cx.set_mario_animation(anim_table::MARIO_ANIM_FLUTTERKICK);
            }
            3 => {
                cx.set_mario_animation(anim_table::MARIO_ANIM_FLUTTERKICK_WITH_OBJ);
            }
            4 => {
                cx.set_mario_animation(anim_table::MARIO_ANIM_GENERAL_FALL);
            }
            _ => {
                cx.set_mario_animation(anim_table::MARIO_ANIM_FALL_WITH_LIGHT_OBJ);
            }
        }

        // `m->particleFlags |= PARTICLE_PLUNGE_BUBBLE;` — no particle flags
        // in the core; skipped.
        ActionResult::Stay
    }
}
