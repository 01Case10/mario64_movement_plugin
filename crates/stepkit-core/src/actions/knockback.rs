//! Ground knockback / bonk actions: verbatim ports of
//! `common_ground_knockback_action` and the `act_*_ground_kb` /
//! `act_ground_bonk` wrappers from `src/game/mario_actions_moving.c`.
//!
//! NOTE: the legacy behavioral handlers for these actions still live in
//! `super::ground` (and are the ones registered in the action registry);
//! the handlers here are the verbatim decomp ports and are not registered
//! yet — the registry swap-over is a later phase's job.

use super::ground::slot;
use super::walk::apply_landing_accel;
use super::{ActionCx, ActionHandler, ActionResult};
use crate::anim_table as anim_id;
use crate::state::ActionId;
use crate::step::{perform_ground_step, GroundStep};

/// Moving-action preamble: the canonical version lives in
/// `super::walk::check_common_moving_cancels` (water plunge, stomp/squish
/// notes, death, quicksand 0.25). Delegating keeps the one faithful copy.
fn moving_preamble(cx: &mut ActionCx) -> Option<ActionResult> {
    super::walk::check_common_moving_cancels(cx)
}

/// Verbatim port of `common_ground_knockback_action`.
// decomp: src/game/mario_actions_moving.c:1588
///
/// The decomp signature is
/// `s32 common_ground_knockback_action(struct MarioState *m, s32 animation, s32 arg2, s32 arg3, s32 arg4)`:
/// there is no `prev_buttons` parameter, and `arg3` is an s32 sound flag
/// (not f32). It returns the animation frame so the wrappers can key
/// frame-specific behavior off it; the Rust port returns
/// `(anim_frame, result)`, where `result` carries the decomp's internal
/// `set_mario_action(...)` calls. Every internal transition is followed by
/// the wrapper's `return FALSE` in the decomp, so they map to `goto_next`
/// (transition now, new action first runs next tick).
///
/// `arg3` is the heavy-landing-sound flag (`TRUE` for all but the soft
/// knockbacks); `arg4` is `m->actionArg` (the air-KB arg, and the
/// post-knockback invulnerability flag).
fn common_ground_knockback_action(
    cx: &mut ActionCx,
    anim_id: i32,
    arg2: i32,
    arg3: i32,
    arg4: i32,
) -> (i32, ActionResult) {
    // decomp: `if (arg3) play_mario_heavy_landing_sound_once(m, SOUND_ACTION_TERRAIN_BODY_HIT_GROUND);`
    // decomp: `if (arg4 > 0) play_sound_if_no_flag(m, SOUND_MARIO_ATTACKED, MARIO_MARIO_SOUND_PLAYED);`
    //         `else play_sound_if_no_flag(m, SOUND_MARIO_OOOF2, MARIO_MARIO_SOUND_PLAYED);`
    // Audio skipped; the flags are real compared state.
    if arg3 != 0 {
        crate::sound::play_mario_heavy_landing_sound_once(&mut cx.state, crate::sound::SoundEvent::Landing);
    }
    crate::sound::play_sound_if_no_flag(
        &mut cx.state,
        crate::sound::SoundEvent::Bonk,
        crate::state::mario_flags::MARIO_SOUND_PLAYED,
    );

    // decomp: direct assignment (no vel re-derivation here); the calls below
    // re-derive vel from the clamped forwardVel (apply_slope_accel inside
    // apply_landing_accel, or mario_set_forward_vel in the else branch).
    if cx.state.forward_speed > 32.0 {
        cx.state.forward_speed = 32.0;
    }
    if cx.state.forward_speed < -32.0 {
        cx.state.forward_speed = -32.0;
    }

    let anim_frame = cx.set_mario_animation(anim_id);
    if i32::from(anim_frame) < arg2 {
        apply_landing_accel(cx, 0.9);
    } else if cx.state.forward_speed >= 0.0 {
        // decomp: `mario_set_forward_vel(m, 0.1f);`
        // (crate: `set_forward_speed` mirrors `mario_set_forward_vel` —
        // forward speed plus slide/linear vel derived from the facing yaw.)
        cx.set_forward_speed(0.1);
    } else {
        // decomp: `mario_set_forward_vel(m, -0.1f);`
        cx.set_forward_speed(-0.1);
    }

    // decomp: a wall hit takes no branch here (nothing happens); only
    // LeftGround and anim-end transition.
    match perform_ground_step(&mut cx.state, cx.world) {
        GroundStep::LeftGround => {
            // decomp: `if (m->forwardVel >= 0.0f) set_mario_action(m, ACT_FORWARD_AIR_KB, arg4);`
            //         `else set_mario_action(m, ACT_BACKWARD_AIR_KB, arg4);`
            let air_kb = if cx.state.forward_speed >= 0.0 {
                ActionId::FORWARD_AIR_KB
            } else {
                ActionId::BACKWARD_AIR_KB
            };
            return (i32::from(anim_frame), cx.goto_next(air_kb, arg4 as u32));
        }
        GroundStep::None | GroundStep::HitWall => {}
    }

    if cx.is_anim_at_end() {
        if cx.state.health < 0x100 {
            // decomp: `set_mario_action(m, ACT_STANDING_DEATH, 0);`
            return (
                i32::from(anim_frame),
                cx.goto_next(ActionId::STANDING_DEATH, 0),
            );
        }
        // decomp: `if (arg4 > 0) m->invincTimer = 30;` — the 30-frame
        // invulnerability is granted at the END of the knockback, not at
        // entry.
        if arg4 > 0 {
            cx.state.invinc_timer = 30;
        }
        // decomp: `set_mario_action(m, ACT_IDLE, 0);`
        return (i32::from(anim_frame), cx.goto_next(ActionId::IDLE, 0));
    }

    (i32::from(anim_frame), ActionResult::Stay)
}

/// The four plain knockback wrappers: preamble, then
/// `common_ground_knockback_action` with the action's exact decomp args.
/// (The hard variants and the bonk have extra frame checks and are
/// hand-written below.)
macro_rules! ground_kb_handler {
    ($name:ident, $label:literal, $anim:expr, $arg2:expr, $arg3:expr) => {
        pub struct $name;
        impl ActionHandler for $name {
            fn name(&self) -> &'static str {
                $label
            }
            fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
                let action_arg = cx.state.action_arg as i32;
                let (_anim_frame, result) =
                    common_ground_knockback_action(cx, $anim, $arg2, $arg3, action_arg);
                cx.timeline.slot = slot::GROUND_KB;
                result
            }
        }
    };
}

// decomp: src/game/mario_actions_moving.c:1660
// `common_ground_knockback_action(m, MARIO_ANIM_BACKWARD_KB, 22, TRUE, m->actionArg); return FALSE;`
ground_kb_handler!(
    BackwardGroundKb,
    "BackwardGroundKb",
    anim_id::MARIO_ANIM_BACKWARD_KB,
    22,
    1
);

// decomp: src/game/mario_actions_moving.c:1665
// `common_ground_knockback_action(m, MARIO_ANIM_FORWARD_KB, 20, TRUE, m->actionArg); return FALSE;`
ground_kb_handler!(
    ForwardGroundKb,
    "ForwardGroundKb",
    anim_id::MARIO_ANIM_FORWARD_KB,
    20,
    1
);

// decomp: src/game/mario_actions_moving.c:1670
// `common_ground_knockback_action(m, MARIO_ANIM_SOFT_BACK_KB, 100, FALSE, m->actionArg); return FALSE;`
ground_kb_handler!(
    SoftBackwardGroundKb,
    "SoftBackwardGroundKb",
    anim_id::MARIO_ANIM_SOFT_BACK_KB,
    100,
    0
);

// decomp: src/game/mario_actions_moving.c:1675
// `common_ground_knockback_action(m, MARIO_ANIM_SOFT_FRONT_KB, 100, FALSE, m->actionArg); return FALSE;`
ground_kb_handler!(
    SoftForwardGroundKb,
    "SoftForwardGroundKb",
    anim_id::MARIO_ANIM_SOFT_FRONT_KB,
    100,
    0
);

/// Hard backward ground knockback: the animFrame == 43 death check.
// decomp: src/game/mario_actions_moving.c:1649
pub struct HardBackwardGroundKb;
impl ActionHandler for HardBackwardGroundKb {
    fn name(&self) -> &'static str {
        "HardBackwardGroundKb"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        let action_arg = cx.state.action_arg as i32;
        let (anim_frame, result) = common_ground_knockback_action(
            cx,
            anim_id::MARIO_ANIM_FALL_OVER_BACKWARDS,
            43,
            1,
            action_arg,
        );
        // decomp: `if (animFrame == 43 && m->health < 0x100) set_mario_action(m, ACT_DEATH_ON_BACK, 0);`
        // animFrame == 54 (SOUND_MARIO_MAMA_MIA, non-JP) and == 69
        // (landing sound) are sound-only and skipped.
        // DEFERRED (state.rs phase): ACT_DEATH_ON_BACK (0x00021316) has no
        // ActionId in this crate yet, so the transition cannot be expressed.
        // The health < 0x100 path still reaches STANDING_DEATH at anim end
        // via common_ground_knockback_action.
        let _death_on_back = anim_frame == 43 && cx.state.health < 0x100;
        cx.timeline.slot = slot::GROUND_KB;
        result
    }
}

/// Hard forward ground knockback: the animFrame == 23 death check.
// decomp: src/game/mario_actions_moving.c:1656
pub struct HardForwardGroundKb;
impl ActionHandler for HardForwardGroundKb {
    fn name(&self) -> &'static str {
        "HardForwardGroundKb"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        let action_arg = cx.state.action_arg as i32;
        let (anim_frame, result) = common_ground_knockback_action(
            cx,
            anim_id::MARIO_ANIM_LAND_ON_STOMACH,
            21,
            1,
            action_arg,
        );
        // decomp: `if (animFrame == 23 && m->health < 0x100) set_mario_action(m, ACT_DEATH_ON_STOMACH, 0);`
        // DEFERRED (state.rs phase): ACT_DEATH_ON_STOMACH (0x00021315) has no
        // ActionId in this crate yet; same STANDING_DEATH fallback as above.
        let _death_on_stomach = anim_frame == 23 && cx.state.health < 0x100;
        cx.timeline.slot = slot::GROUND_KB;
        result
    }
}

/// Ground bonk: its own tiny wrapper around the common action (not a
/// macro case), with the frame-32 landing sound.
// decomp: src/game/mario_actions_moving.c:1691
pub struct GroundBonk;
impl ActionHandler for GroundBonk {
    fn name(&self) -> &'static str {
        "GroundBonk"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        let action_arg = cx.state.action_arg as i32;
        // decomp: `common_ground_knockback_action(m, MARIO_ANIM_GROUND_BONK, 32, TRUE, m->actionArg);`
        let (anim_frame, result) =
            common_ground_knockback_action(cx, anim_id::MARIO_ANIM_GROUND_BONK, 32, 1, action_arg);
        // decomp: `if (animFrame == 32) play_mario_landing_sound(m, SOUND_ACTION_TERRAIN_LANDING);`
        // Sound skipped.
        let _ = anim_frame;
        cx.timeline.slot = slot::GROUND_BONK;
        result
    }
}
