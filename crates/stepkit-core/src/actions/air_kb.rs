//! Phase 6 air-action ports: the decomp's air knockback family from
//! `src/game/mario_actions_airborne.c` — `common_air_knockback_step` (with
//! its `check_wall_kick` helper), the five KB actions built on it
//! (`act_backward_air_kb`, `act_forward_air_kb`, `act_hard_backward_air_kb`,
//! `act_hard_forward_air_kb`, `act_soft_bonk`), `act_air_hit_wall` (with its
//! missing-`return` double-execute quirk), and `act_freefall`.
//!
//! Conventions: every ported item cites its decomp location; sound,
//! particle, and camera/gfx calls are skipped with a comment;
//! `m->input & INPUT_*` reads become `cx.state.input & input_flags::*`;
//! `set_mario_action`'s airborne-group dispatch is modeled by routing
//! WALL_KICK_AIR / BACKWARD_AIR_KB / SOFT_BONK / DIVE / GROUND_POUND through
//! `super::entry::set_mario_action_airborne` (the KB actions have no dispatch
//! arm, so the id passes through unchanged).
//!
//! NOTE: `act_hard_fall` does not exist in the decomp — the only "hard
//! fall" concept is the `hardFallAction` *parameter* of `check_fall_damage`
//! (already ported as `air_model::check_fall_damage` /
//! `check_fall_damage_or_get_stuck` and reused here).

use super::air_model::{check_fall_damage_or_get_stuck, common_air_action_step};
use super::entry;
use super::{ActionCx, ActionHandler, ActionResult};
use crate::angles::Angle;
use crate::anim_table;
use crate::input::input_flags as f;
use crate::sound;
use crate::state::{mario_flags, ActionId};
use crate::step::{
    mario_bonk_reflection, mario_set_forward_vel, perform_air_step, AirStep,
    AIR_STEP_CHECK_LEDGE_GRAB,
};

// ---------------------------------------------------------------------------
// Shared helpers (decomp functions, not actions).
// ---------------------------------------------------------------------------

/// decomp: `play_knockback_sound` (`src/game/mario_actions_airborne.c:39`,
/// non-JP build).
/// The `SOUND_MARIO_DOH` vs `SOUND_MARIO_UH` choice is audio-only; both arms
/// set `MARIO_MARIO_SOUND_PLAYED`, which is the only gameplay state, so the
/// port models just the flag.
fn play_knockback_sound(cx: &mut ActionCx) {
    // decomp: actionArg == 0 && (forwardVel <= -28 || forwardVel >= 28) →
    // DOH, else UH. (Sound selection skipped; flag side effect identical.)
    sound::play_sound_if_no_flag(&mut cx.state, mario_flags::MARIO_SOUND_PLAYED);
}

/// decomp: `check_wall_kick` (`src/game/mario_actions_airborne.c:1147`).
/// Called only from the two plain air KBs and soft bonk.
/// Returns `Some` (same-tick) when the kick fires.
fn check_wall_kick(cx: &mut ActionCx) -> Option<ActionResult> {
    if cx.state.input & f::A_PRESSED != 0
        && cx.state.wall_kick_timer != 0
        && cx.state.prev_action == ActionId::AIR_HIT_WALL
    {
        // decomp: m->faceAngle[1] += 0x8000 (s16 wrap).
        cx.state.face_yaw = Angle(cx.state.face_yaw.0.wrapping_add(0x8000u16 as i16));
        // decomp: `return set_mario_action(m, ACT_WALL_KICK_AIR, 0)` —
        // set_mario_action always returns TRUE → same-tick.
        let id = entry::set_mario_action_airborne(cx, ActionId::WALL_KICK_AIR, 0);
        return Some(cx.goto(id, 0));
    }
    None
}

/// decomp: `common_air_knockback_step`
/// (`src/game/mario_actions_airborne.c:1097`).
/// Constant horizontal speed — `mario_set_forward_vel(m, speed)` every frame,
/// no drag, no steering — then `perform_air_step`.
/// Returns the LANDED transition (next-tick; the decomp caller returns FALSE)
/// or `None` to stay.
fn common_air_knockback_step(
    cx: &mut ActionCx,
    land_action: ActionId,
    hard_fall_action: ActionId,
    animation: i32,
    speed: f32,
) -> Option<ActionResult> {
    mario_set_forward_vel(&mut cx.state, speed);

    match perform_air_step(&mut cx.state, cx.world, 0) {
        AirStep::None => {
            cx.set_mario_animation(animation);
            None
        }
        AirStep::Landed => {
            cx.state.land_from = cx.state.action;
            // decomp: ENABLE_RUMBLE-guarded queue_rumble_data(5, 40) unless
            // ACT_SOFT_BONK — rumble has no gameplay state; skipped.
            if check_fall_damage_or_get_stuck(cx, hard_fall_action) {
                // The helper already applied the hard-fall transition
                // same-tick (decomp: drop_and_set_mario_action returns
                // TRUE). Re-emit the Goto so the action loop re-runs the new
                // action this tick — same convention as
                // air_model::common_air_action_step.
                return Some(cx.goto(cx.state.action, cx.state.action_arg));
            }
            // decomp (US, non-JP): ACT_THROWN_FORWARD/BACKWARD land with
            // m->hurtCounter as the arg; the ported KB set never enters via
            // a thrown action, so the arg is always m->actionArg.
            let arg = cx.state.action_arg;
            // decomp: set_mario_action(landAction, arg); the caller returns
            // FALSE → next-tick. The moving-group dispatch (entry) runs
            // inside the transition, as in set_mario_action.
            Some(cx.goto_next(land_action, arg))
        }
        AirStep::HitWall => {
            cx.set_mario_animation(anim_table::MARIO_ANIM_BACKWARD_AIR_KB);
            mario_bonk_reflection(&mut cx.state, false);
            if cx.state.vel.y > 0.0 {
                cx.state.vel.y = 0.0;
            }
            mario_set_forward_vel(&mut cx.state, -speed);
            None
        }
        AirStep::HitLavaWall => {
            // decomp: lava_boost_on_wall — lava surfaces are out of scope
            // (no lava in the scenario set); skipped like the other ports.
            None
        }
        // decomp: no ledge/hang arms (the step arg is 0).
        AirStep::GrabbedLedge | AirStep::GrabbedCeiling => None,
    }
}

// ---------------------------------------------------------------------------
// The KB actions.
// ---------------------------------------------------------------------------

/// decomp: `act_backward_air_kb` (`src/game/mario_actions_airborne.c:1156`).
pub struct BackwardAirKb;
impl ActionHandler for BackwardAirKb {
    fn name(&self) -> &'static str {
        "BackwardAirKb"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: `if (check_wall_kick(m)) return TRUE;` — same-tick.
        if let Some(r) = check_wall_kick(cx) {
            return r;
        }
        play_knockback_sound(cx);
        // decomp: common_air_knockback_step(m, ACT_BACKWARD_GROUND_KB,
        //   ACT_HARD_BACKWARD_GROUND_KB, 0x0002, -16.0f); return FALSE.
        if let Some(r) = common_air_knockback_step(
            cx,
            ActionId::BACKWARD_GROUND_KB,
            ActionId::HARD_BACKWARD_GROUND_KB,
            anim_table::MARIO_ANIM_BACKWARD_AIR_KB,
            -16.0,
        ) {
            return r;
        }
        ActionResult::Stay
    }
}

/// decomp: `act_forward_air_kb` (`src/game/mario_actions_airborne.c:1170`).
pub struct ForwardAirKb;
impl ActionHandler for ForwardAirKb {
    fn name(&self) -> &'static str {
        "ForwardAirKb"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: `if (check_wall_kick(m)) return TRUE;` — same-tick.
        if let Some(r) = check_wall_kick(cx) {
            return r;
        }
        play_knockback_sound(cx);
        // decomp: common_air_knockback_step(m, ACT_FORWARD_GROUND_KB,
        //   ACT_HARD_FORWARD_GROUND_KB, 0x002D, 16.0f); return FALSE.
        if let Some(r) = common_air_knockback_step(
            cx,
            ActionId::FORWARD_GROUND_KB,
            ActionId::HARD_FORWARD_GROUND_KB,
            anim_table::MARIO_ANIM_AIR_FORWARD_KB,
            16.0,
        ) {
            return r;
        }
        ActionResult::Stay
    }
}

/// decomp: `act_hard_backward_air_kb`
/// (`src/game/mario_actions_airborne.c:1184`).
/// Note: the hard variants do NOT call check_wall_kick.
pub struct HardBackwardAirKb;
impl ActionHandler for HardBackwardAirKb {
    fn name(&self) -> &'static str {
        "HardBackwardAirKb"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        play_knockback_sound(cx);
        // decomp: common_air_knockback_step(m, ACT_HARD_BACKWARD_GROUND_KB,
        //   ACT_HARD_BACKWARD_GROUND_KB, 0x0002, -16.0f); return FALSE.
        if let Some(r) = common_air_knockback_step(
            cx,
            ActionId::HARD_BACKWARD_GROUND_KB,
            ActionId::HARD_BACKWARD_GROUND_KB,
            anim_table::MARIO_ANIM_BACKWARD_AIR_KB,
            -16.0,
        ) {
            return r;
        }
        ActionResult::Stay
    }
}

/// decomp: `act_hard_forward_air_kb`
/// (`src/game/mario_actions_airborne.c:1197`).
pub struct HardForwardAirKb;
impl ActionHandler for HardForwardAirKb {
    fn name(&self) -> &'static str {
        "HardForwardAirKb"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        play_knockback_sound(cx);
        // decomp: common_air_knockback_step(m, ACT_HARD_FORWARD_GROUND_KB,
        //   ACT_HARD_FORWARD_GROUND_KB, 0x002D, 16.0f); return FALSE.
        if let Some(r) = common_air_knockback_step(
            cx,
            ActionId::HARD_FORWARD_GROUND_KB,
            ActionId::HARD_FORWARD_GROUND_KB,
            anim_table::MARIO_ANIM_AIR_FORWARD_KB,
            16.0,
        ) {
            return r;
        }
        ActionResult::Stay
    }
}

/// decomp: `act_soft_bonk` (`src/game/mario_actions_airborne.c:1247`).
pub struct SoftBonk;
impl ActionHandler for SoftBonk {
    fn name(&self) -> &'static str {
        "SoftBonk"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: `if (check_wall_kick(m)) return TRUE;` — same-tick.
        if let Some(r) = check_wall_kick(cx) {
            return r;
        }
        play_knockback_sound(cx);
        // decomp: the speed argument is m->forwardVel, read at call time —
        // the helper then pins it constant (no drag).
        let speed = cx.state.forward_speed;
        // decomp: common_air_knockback_step(m, ACT_FREEFALL_LAND,
        //   ACT_HARD_BACKWARD_GROUND_KB, 0x0056, m->forwardVel); return FALSE.
        if let Some(r) = common_air_knockback_step(
            cx,
            ActionId::FREEFALL_LAND,
            ActionId::HARD_BACKWARD_GROUND_KB,
            anim_table::MARIO_ANIM_GENERAL_FALL,
            speed,
        ) {
            return r;
        }
        ActionResult::Stay
    }
}

// ---------------------------------------------------------------------------
// act_air_hit_wall — with the missing-return quirk.
// ---------------------------------------------------------------------------

/// decomp: `act_air_hit_wall` (`src/game/mario_actions_airborne.c:1309`).
///
/// **The missing-`return` quirk.** After the `++actionTimer <= 2` window the
/// function falls off the end without a `return`; with `AVOID_UB` (the
/// oracle build flags) this is `return set_mario_animation(m,
/// MARIO_ANIM_START_WALLKICK)`, and the returned anim frame is in practice
/// nonzero, so `execute_mario_action`'s action loop treats it as TRUE
/// ("cancel") and re-runs the *same* action in the *same* frame. The port
/// models this by looping the body in-tick until the timer leaves the
/// window: the 2-frame kick window is consumed in a single real frame
/// (hence wall-kick "firsties" are only possible for one frame), unless A
/// is pressed or the impact speed resolves it sooner.
pub struct AirHitWall;
impl ActionHandler for AirHitWall {
    fn name(&self) -> &'static str {
        "AirHitWall"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: mario_drop_held_object — no held objects in scope; no-op.
        loop {
            // decomp: `if (++(m->actionTimer) <= 2)` — pre-increment.
            cx.state.action_timer += 1;
            if cx.state.action_timer <= 2 {
                if cx.state.input & f::A_PRESSED != 0 {
                    // decomp: vel[1] = 52.0f (the airborne entry then
                    // overwrites it to 62 and enforces forwardVel >= 24).
                    cx.state.vel.y = 52.0;
                    cx.state.face_yaw =
                        Angle(cx.state.face_yaw.0.wrapping_add(0x8000u16 as i16));
                    // decomp: `return set_mario_action(m, ACT_WALL_KICK_AIR, 0)`
                    // — TRUE → same-tick.
                    let id = entry::set_mario_action_airborne(cx, ActionId::WALL_KICK_AIR, 0);
                    return cx.goto(id, 0);
                }
                // decomp: falls through to
                // set_mario_animation(MARIO_ANIM_START_WALLKICK); the missing
                // return re-runs this body in the same tick (see quirk note).
                cx.set_mario_animation(anim_table::MARIO_ANIM_START_WALLKICK);
                // The timer strictly advances, so this terminates after at
                // most two window iterations.
                continue;
            }
            if cx.state.forward_speed >= 38.0 {
                cx.state.wall_kick_timer = 5;
                if cx.state.vel.y > 0.0 {
                    cx.state.vel.y = 0.0;
                }
                // decomp: particleFlags |= PARTICLE_VERTICAL_STAR
                // (visual; skipped).
                // decomp: `return set_mario_action(m, ACT_BACKWARD_AIR_KB, 0)`
                // — TRUE → same-tick.
                let id = entry::set_mario_action_airborne(cx, ActionId::BACKWARD_AIR_KB, 0);
                return cx.goto(id, 0);
            }
            cx.state.wall_kick_timer = 5;
            if cx.state.vel.y > 0.0 {
                cx.state.vel.y = 0.0;
            }
            if cx.state.forward_speed > 8.0 {
                mario_set_forward_vel(&mut cx.state, -8.0);
            }
            // decomp: `return set_mario_action(m, ACT_SOFT_BONK, 0)` —
            // TRUE → same-tick.
            let id = entry::set_mario_action_airborne(cx, ActionId::SOFT_BONK, 0);
            return cx.goto(id, 0);
        }
    }
}

// ---------------------------------------------------------------------------
// act_freefall.
// ---------------------------------------------------------------------------

/// decomp: `act_freefall` (`src/game/mario_actions_airborne.c:525`).
pub struct Freefall;
impl ActionHandler for Freefall {
    fn name(&self) -> &'static str {
        "Freefall"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: `if (m->input & INPUT_B_PRESSED)
        //   return set_mario_action(m, ACT_DIVE, 0);` — TRUE → same-tick.
        if cx.state.input & f::B_PRESSED != 0 {
            let id = entry::set_mario_action_airborne(cx, ActionId::DIVE, 0);
            return cx.goto(id, 0);
        }
        // decomp: `if (m->input & INPUT_Z_PRESSED)
        //   return set_mario_action(m, ACT_GROUND_POUND, 0);` — TRUE → same-tick.
        if cx.state.input & f::Z_PRESSED != 0 {
            let id = entry::set_mario_action_airborne(cx, ActionId::GROUND_POUND, 0);
            return cx.goto(id, 0);
        }
        // decomp: the animation is picked by actionArg; there is no default
        // arm (the arg is 0/1/2 in practice).
        let animation = match cx.state.action_arg {
            0 => anim_table::MARIO_ANIM_GENERAL_FALL,
            1 => anim_table::MARIO_ANIM_FALL_FROM_SLIDE,
            2 => anim_table::MARIO_ANIM_FALL_FROM_SLIDE_KICK,
            _ => anim_table::MARIO_ANIM_GENERAL_FALL,
        };
        // decomp: common_air_action_step(m, ACT_FREEFALL_LAND, animation,
        //   AIR_STEP_CHECK_LEDGE_GRAB); return FALSE.
        let outcome =
            common_air_action_step(cx, ActionId::FREEFALL_LAND, animation, AIR_STEP_CHECK_LEDGE_GRAB);
        if let Some(r) = outcome.transition_taken {
            return r;
        }
        ActionResult::Stay
    }
}
