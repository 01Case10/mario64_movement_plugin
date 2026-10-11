//! Water combat and death actions: verbatim ports of the decomp's submerged
//! combat/death family from `src/game/mario_actions_submerged.c`:
//! `act_water_punch` (:820), `common_water_knockback_step` (:866),
//! `act_backward_water_kb` (:882), `act_forward_water_kb` (:887),
//! `act_water_shocked` (:892), `act_drowning` (:913), `act_water_death` (:939).
//!
//! Conventions, matching the other action modules:
//! - Every ported item cites its decomp location (`// decomp:
//!   src/game/mario_actions_submerged.c:<line>`).
//! - `return set_mario_action(...)` becomes `cx.goto(...)` (same tick).
//!   Every `set_mario_action` in these actions has its return value
//!   *discarded* and is followed by `return FALSE`, so the transition
//!   fires but the rest of the tick still runs and the new action starts
//!   next tick: modeled as `cx.goto_next(...)` returned after the tail
//!   code, per the precedent in `actions/punch.rs`.
//! - Sound: `play_sound_if_no_flag` calls that set MARIO_ACTION_SOUND_PLAYED
//!   / MARIO_MARIO_SOUND_PLAYED go through `crate::sound` stubs (the flags
//!   are real gameplay state). Plain `play_sound` calls (no flag side
//!   effect) are audio-only and skipped with a comment, as are camera shake
//!   (`set_camera_shake_from_hit`) and render state (`marioBodyState`
//!   head/eye angles — the core has no head/eye state).
//! - Out of scope (not ported): `act_water_shell_swimming` (:748, shell),
//!   `act_water_throw` (:794, throwing), `act_caught_in_whirlpool` (:1040,
//!   whirlpool), all HOLD_* variants, and `check_water_grab` (:773,
//!   grabbing). Where a ported action references one, the call site is
//!   annotated.
//!
//! NOT-YET-PORTED DEPENDENCIES (called below verbatim; defined in later phases):
//! - `update_swimming_yaw` (:270), `update_swimming_pitch` (:289),
//!   `update_swimming_speed` (:305), `update_water_pitch` (:200): the
//!   swim-movement family, owned by the later water-movement phase.
//! - `perform_water_step` (:208): needs wall-collision resolution
//!   (`resolve_and_return_wall_collisions` via `perform_water_full_step`),
//!   which the `CollisionWorld` trait does not expose yet.
//! This file is not wired into the action registry yet.

use super::{ActionCx, ActionHandler, ActionResult};
use super::water_physics::{
    perform_water_step, update_swimming_pitch, update_swimming_speed, update_swimming_yaw,
    update_water_pitch,
};
use crate::angles::Angle;
use crate::anim_table;
use crate::math_util::{approach_f32, approach_s32};
use crate::sound;
use crate::state::{act_flags, mario_flags, ActionId, CharacterState};

pub mod id {
    use crate::state::ActionId;
    /// decomp: ACT_WATER_PUNCH (oracle/sm64/include/sm64.h:320).
    pub const WATER_PUNCH: ActionId = ActionId(0x300024E1);
    /// decomp: ACT_BACKWARD_WATER_KB (oracle/sm64/include/sm64.h:308).
    pub const BACKWARD_WATER_KB: ActionId = ActionId(0x300222C5);
    /// decomp: ACT_FORWARD_WATER_KB (oracle/sm64/include/sm64.h:309).
    pub const FORWARD_WATER_KB: ActionId = ActionId(0x300222C6);
    /// decomp: ACT_WATER_SHOCKED (oracle/sm64/include/sm64.h:311).
    pub const WATER_SHOCKED: ActionId = ActionId(0x300222C8);
    /// decomp: ACT_DROWNING (oracle/sm64/include/sm64.h:307).
    pub const DROWNING: ActionId = ActionId(0x300032C4);
    /// decomp: ACT_WATER_DEATH (oracle/sm64/include/sm64.h:310).
    pub const WATER_DEATH: ActionId = ActionId(0x300032C7);
    /// decomp: ACT_HOLD_WATER_ACTION_END (oracle/sm64/include/sm64.h:306).
    /// Transition target of `act_water_punch`'s object branch only; the
    /// HOLD_* variant itself is out of scope (no objects in the core).
    pub const HOLD_WATER_ACTION_END: ActionId = ActionId(0x300022C3);
}

// ---------------------------------------------------------------------------
// Verbatim helper ports (self-contained; shared by the actions below).
// ---------------------------------------------------------------------------

/// decomp: `MIN_SWIM_SPEED` (src/game/mario_actions_submerged.c:14).
const MIN_SWIM_SPEED: f32 = 16.0;

/// decomp: `swimming_near_surface` (src/game/mario_actions_submerged.c:44).
fn swimming_near_surface(state: &CharacterState) -> bool {
    // NOTE: MARIO_METAL_CAP (sm64.h:117, 0x00000004) is not in the core's
    // `mario_flags` yet; the raw decomp bit is used.
    if state.flags & 0x00000004 != 0 {
        return false;
    }

    (state.water_level - 80.0) - state.pos.y < 400.0
}

/// decomp: `get_buoyancy` (src/game/mario_actions_submerged.c:52).
fn get_buoyancy(state: &CharacterState) -> f32 {
    let mut buoyancy = 0.0;

    // NOTE: MARIO_METAL_CAP bit, see `swimming_near_surface` above.
    if state.flags & 0x00000004 != 0 {
        if state.action.0 & act_flags::INVULNERABLE != 0 {
            buoyancy = -2.0;
        } else {
            buoyancy = -18.0;
        }
    } else if swimming_near_surface(state) {
        buoyancy = 1.25;
    } else if state.action.0 & act_flags::MOVING == 0 {
        buoyancy = -2.0;
    }

    buoyancy
}

/// decomp: `stationary_slow_down` (src/game/mario_actions_submerged.c:214).
/// Pure-math helper: kills yaw velocity and eases forward/vertical speed
/// toward the buoyancy target while leveling pitch and roll.
fn stationary_slow_down(state: &mut CharacterState) {
    let buoyancy = get_buoyancy(state);

    state.angle_vel[0] = Angle::ZERO;
    state.angle_vel[1] = Angle::ZERO;

    state.forward_speed = approach_f32(state.forward_speed, 0.0, 1.0, 1.0);
    state.vel.y = approach_f32(state.vel.y, buoyancy, 2.0, 1.0);

    // decomp: faceAngle[0] / faceAngle[2] are s16; the approach runs in s32
    // and the store truncates (gcc behavior).
    state.face_pitch = Angle(approach_s32(state.face_pitch.0 as i32, 0, 0x200, 0x200) as i16);
    state.face_roll = Angle(approach_s32(state.face_roll.0 as i32, 0, 0x100, 0x100) as i16);

    let c = crate::trig::coss(state.face_pitch);
    let (sy, cy) = (crate::trig::sins(state.face_yaw), crate::trig::coss(state.face_yaw));
    state.vel.x = state.forward_speed * c * sy;
    state.vel.z = state.forward_speed * c * cy;
}

// ---------------------------------------------------------------------------
// Actions.
// ---------------------------------------------------------------------------

/// decomp: `act_water_punch` (src/game/mario_actions_submerged.c:820).
///
/// The water punch: a lunge (forward speed builds to 7.0) with a 3-stage
/// anim machine in `actionState`. Stage 0 -> 1 always advances on anim end
/// here (`check_water_grab` is grabbing, out of scope — with no grabbable
/// objects in the core it always returns FALSE, so `actionState =
/// FALSE + 1 = 1`). Stage 2 is unreachable in this core (see below).
pub struct WaterPunch;
impl ActionHandler for WaterPunch {
    fn name(&self) -> &'static str {
        "WaterPunch"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: submerged.c:821-823.
        if cx.state.forward_speed < 7.0 {
            cx.state.forward_speed += 1.0;
        }

        // NOTE: update_swimming_yaw (:270), update_swimming_pitch (:289),
        // update_swimming_speed (:305), perform_water_step (:208),
        // update_water_pitch (:200) — not yet ported (see module docs).
        update_swimming_yaw(cx);
        update_swimming_pitch(cx);
        update_swimming_speed(cx, MIN_SWIM_SPEED);
        perform_water_step(cx);
        update_water_pitch(cx);

        // NOTE: m->marioBodyState->headAngle[0] = approach_s32(..., 0, 0x200,
        // 0x200) — head angle is render state; the core has no head-angle
        // field. Skipped with comment per the module convention.

        // decomp: submerged.c:831 — sets MARIO_ACTION_SOUND_PLAYED.
        sound::play_sound_if_no_flag(&mut cx.state, mario_flags::ACTION_SOUND_PLAYED);

        // decomp: submerged.c:833-861. All `set_mario_action` returns here
        // are discarded and the function falls through to `return FALSE`,
        // so any transition starts next tick (goto_next).
        match cx.state.action_state {
            0 => {
                cx.set_mario_animation(anim_table::MARIO_ANIM_WATER_GRAB_OBJ_PART1);
                if cx.is_anim_at_end() {
                    // decomp: `m->actionState = check_water_grab(m) + 1`.
                    // check_water_grab (submerged.c:773) is grabbing — out of
                    // scope; with no grabbable objects in the core it always
                    // returns FALSE, so actionState becomes 1.
                    cx.state.action_state = 1;
                }
            }
            1 => {
                cx.set_mario_animation(anim_table::MARIO_ANIM_WATER_GRAB_OBJ_PART2);
                if cx.is_anim_at_end() {
                    // decomp: submerged.c:845 — discarded return.
                    return cx.goto_next(super::water::id::WATER_ACTION_END, 0);
                }
            }
            2 => {
                cx.set_mario_animation(anim_table::MARIO_ANIM_WATER_PICK_UP_OBJ);
                if cx.is_anim_at_end() {
                    // NOTE: unreachable in this core — actionState can only
                    // ever be 0 or 1 (check_water_grab always FALSE; no
                    // held objects exist). The decomp's shell branch
                    // (heldObj->behavior == bhvKoopaShellUnderwater ->
                    // play_shell_music + ACT_WATER_SHELL_SWIMMING) is shell,
                    // out of scope; heldObj is always None here, so the else
                    // arm (ACT_HOLD_WATER_ACTION_END, 1 — discarded return)
                    // is the live path. play_shell_music is music, no flag
                    // side effect; skipped with the out-of-scope branch.
                    return cx.goto_next(id::HOLD_WATER_ACTION_END, 1);
                }
            }
            _ => {}
        }

        ActionResult::Stay
    }
}

/// decomp: `common_water_knockback_step`
/// (src/game/mario_actions_submerged.c:866).
///
/// Shared knockback tail: slow down, water step, anim; on anim end, grant
/// 30 frames of invincibility when the knockback was "hard" (`arg3 > 0`)
/// and go to `end_action` — or to WATER_DEATH when health is below 0x100.
/// The decomp returns void and both callers discard the `set_mario_action`
/// result and `return FALSE`; the transition is therefore next-tick.
/// Returns the transition target, if any.
fn common_water_knockback_step(
    cx: &mut ActionCx,
    animation: i32,
    end_action: ActionId,
    arg3: u32,
) -> Option<ActionId> {
    // decomp: submerged.c:867-868.
    stationary_slow_down(&mut cx.state);
    // NOTE: perform_water_step (:208) — not yet ported (see module docs).
    perform_water_step(cx);
    // decomp: submerged.c:869.
    cx.set_mario_animation(animation);

    // NOTE: m->marioBodyState->headAngle[0] = 0 — head angle is render
    // state; skipped with comment per the module convention.

    // decomp: submerged.c:873-879.
    if cx.is_anim_at_end() {
        // decomp param is s32; the core's action_arg is u32 — the `> 0`
        // comparison is identical either way.
        if arg3 > 0 {
            cx.state.invinc_timer = 30;
        }

        // decomp: submerged.c:878 — discarded return.
        return Some(if cx.state.health >= 0x100 {
            end_action
        } else {
            id::WATER_DEATH
        });
    }

    None
}

/// decomp: `act_backward_water_kb` (src/game/mario_actions_submerged.c:882).
pub struct BackwardWaterKb;
impl ActionHandler for BackwardWaterKb {
    fn name(&self) -> &'static str {
        "BackwardWaterKb"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: submerged.c:883 — helper return discarded, `return FALSE`.
        match common_water_knockback_step(
            cx,
            anim_table::MARIO_ANIM_BACKWARDS_WATER_KB,
            super::water::id::WATER_IDLE,
            cx.state.action_arg,
        ) {
            Some(a) => cx.goto_next(a, 0),
            None => ActionResult::Stay,
        }
    }
}

/// decomp: `act_forward_water_kb` (src/game/mario_actions_submerged.c:887).
pub struct ForwardWaterKb;
impl ActionHandler for ForwardWaterKb {
    fn name(&self) -> &'static str {
        "ForwardWaterKb"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: submerged.c:888 — helper return discarded, `return FALSE`.
        match common_water_knockback_step(
            cx,
            anim_table::MARIO_ANIM_WATER_FORWARD_KB,
            super::water::id::WATER_IDLE,
            cx.state.action_arg,
        ) {
            Some(a) => cx.goto_next(a, 0),
            None => ActionResult::Stay,
        }
    }
}

/// decomp: `act_water_shocked` (src/game/mario_actions_submerged.c:892).
///
/// Electric shock in water: the SHOCKED anim plays, `actionTimer` counts
/// frames while the anim is on frame 0, and at 6 frames Mario gets 30
/// frames of invincibility and exits to WATER_DEATH (health < 0x100) or
/// WATER_IDLE. The `set_mario_action` return is discarded and the slow-down
/// tail still runs this tick, so the transition is next-tick.
pub struct WaterShocked;
impl ActionHandler for WaterShocked {
    fn name(&self) -> &'static str {
        "WaterShocked"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: submerged.c:893 — sets MARIO_ACTION_SOUND_PLAYED.
        sound::play_sound_if_no_flag(&mut cx.state, mario_flags::ACTION_SOUND_PLAYED);
        // NOTE: play_sound(SOUND_MOVING_SHOCKED, ...) (submerged.c:894) is a
        // plain play_sound — no flag side effect — so audio-only; skipped.
        // NOTE: set_camera_shake_from_hit(SHAKE_SHOCK) (submerged.c:895) is
        // camera; skipped per the module convention.

        // decomp: submerged.c:897-900. set_mario_animation returns the
        // current anim frame; the timer only advances on frame 0.
        if cx.set_mario_animation(anim_table::MARIO_ANIM_SHOCKED) == 0 {
            cx.state.action_timer += 1;
            // NOTE: m->flags |= MARIO_METAL_SHOCK (sm64.h:121) — visual-only
            // (mario.c:1621 sets MODEL_STATE_METAL from it); not in the
            // core's mario_flags. Skipped with comment.
        }

        // decomp: submerged.c:902-905 — discarded set_mario_action return;
        // the tail below still runs this tick.
        let transition = if cx.state.action_timer >= 6 {
            cx.state.invinc_timer = 30;
            Some(if cx.state.health < 0x100 {
                id::WATER_DEATH
            } else {
                super::water::id::WATER_IDLE
            })
        } else {
            None
        };

        // decomp: submerged.c:907-909.
        stationary_slow_down(&mut cx.state);
        // NOTE: perform_water_step (:208) — not yet ported (see module docs).
        perform_water_step(cx);
        // NOTE: m->marioBodyState->headAngle[0] = 0 — head angle is render
        // state; skipped with comment per the module convention.

        match transition {
            Some(a) => cx.goto_next(a, 0),
            None => ActionResult::Stay,
        }
    }
}

/// decomp: `act_drowning` (src/game/mario_actions_submerged.c:913).
///
/// Two-stage drowning anim machine in `actionState`; on anim frame 30 of
/// part 2 the death warp fires (`level_trigger_warp(m, WARP_OP_DEATH)` —
/// the core's `state.warped` flag, cf. `step.rs`).
pub struct Drowning;
impl ActionHandler for Drowning {
    fn name(&self) -> &'static str {
        "Drowning"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: submerged.c:914-931.
        match cx.state.action_state {
            0 => {
                cx.set_mario_animation(anim_table::MARIO_ANIM_DROWNING_PART1);
                // NOTE: m->marioBodyState->eyeState = MARIO_EYES_HALF_CLOSED
                // — eye state is render; the core has no eye-state field.
                // Skipped with comment per the module convention.
                if cx.is_anim_at_end() {
                    cx.state.action_state = 1;
                }
            }
            1 => {
                cx.set_mario_animation(anim_table::MARIO_ANIM_DROWNING_PART2);
                // NOTE: eyeState = MARIO_EYES_DEAD — render; skipped (above).
                // decomp: submerged.c:926 — exact frame equality against
                // animInfo.animFrame; the core's AnimState.frame is that
                // field's port (anim.rs).
                if cx.state.anim.frame == 30 {
                    // decomp: level_trigger_warp(m, WARP_OP_DEATH).
                    cx.state.warped = true;
                }
            }
            _ => {}
        }

        // decomp: submerged.c:933 — sets MARIO_ACTION_SOUND_PLAYED.
        sound::play_sound_if_no_flag(&mut cx.state, mario_flags::ACTION_SOUND_PLAYED);

        // decomp: submerged.c:934-935.
        stationary_slow_down(&mut cx.state);
        // NOTE: perform_water_step (:208) — not yet ported (see module docs).
        perform_water_step(cx);

        // decomp: submerged.c:937 — `return FALSE`.
        ActionResult::Stay
    }
}

/// decomp: `act_water_death` (src/game/mario_actions_submerged.c:939).
///
/// The death float: slows to a stop, plays WATER_DYING, and fires the
/// death warp on that anim's frame 35. Note the decomp calls
/// `set_mario_animation` twice per tick (submerged.c:945-946) — the first
/// call's return is unused; the port issues it once.
pub struct WaterDeath;
impl ActionHandler for WaterDeath {
    fn name(&self) -> &'static str {
        "WaterDeath"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: submerged.c:940-941.
        stationary_slow_down(&mut cx.state);
        // NOTE: perform_water_step (:208) — not yet ported (see module docs).
        perform_water_step(cx);

        // NOTE: m->marioBodyState->eyeState = MARIO_EYES_DEAD — eye state is
        // render; skipped with comment per the module convention.

        // decomp: submerged.c:945-946 — the double call is verbatim in the
        // decomp (first call's return unused); one call suffices here.
        if cx.set_mario_animation(anim_table::MARIO_ANIM_WATER_DYING) == 35 {
            // decomp: level_trigger_warp(m, WARP_OP_DEATH).
            cx.state.warped = true;
        }

        // decomp: submerged.c:950 — `return FALSE`.
        ActionResult::Stay
    }
}
