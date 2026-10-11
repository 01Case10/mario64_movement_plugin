//! Jump-family air actions: verbatim ports of the decomp's jump actions.
//!
//! - `act_jump` (decomp: src/game/mario_actions_airborne.c:446)
//! - `act_double_jump` (decomp: src/game/mario_actions_airborne.c:461)
//! - `act_triple_jump` (decomp: src/game/mario_actions_airborne.c:480)
//! - `act_backflip` (decomp: src/game/mario_actions_airborne.c:509)
//! - `act_side_flip` (decomp: src/game/mario_actions_airborne.c:595)
//! - `act_wall_kick_air` (decomp: src/game/mario_actions_airborne.c:618)
//! - `act_long_jump` (decomp: src/game/mario_actions_airborne.c:632)
//! - `act_steep_jump` (decomp: src/game/mario_actions_airborne.c:888)
//!
//! Conventions (same as walk.rs/landings.rs):
//! - `cx.goto(id, arg)` = the decomp's `set_mario_action` TRUE (the new
//!   action runs the same tick); `cx.goto_next(id, arg)` = decomp FALSE
//!   (the new action first runs next tick). `set_mario_action` always
//!   returns TRUE.
//! - Airborne targets are routed through `super::entry::set_mario_action_airborne`
//!   before `cx.goto`, matching the decomp's `set_mario_action` group
//!   dispatch (decomp: src/game/mario.c:979). The port's `cx.goto` only
//!   applies the *moving*-group routing internally, so the airborne routing
//!   must be explicit at the call site. That helper also applies the
//!   decomp's unconditional `peakHeight = pos[1]` / `flags |= MARIO_UNKNOWN_08`
//!   (decomp: src/game/mario.c:776).
//! - Sounds are flag effects only (see `crate::sound`): the plain
//!   `play_sound` calls (`play_flip_sounds`, the side-flip frame-6 spin
//!   sound, the long-jump vertical-wind "here we go") have no gameplay
//!   state and are skipped with a comment.
//! - The airborne group preamble (`check_common_airborne_cancels`,
//!   decomp: src/game/mario_actions_airborne.c:2046 — water plunge,
//!   squished, vertical wind, `quicksandDepth = 0`) is NOT inlined here;
//!   it belongs to the group executor that runs before the action.
//! - Not registered in actions.rs (the parent's job).
//!
//! Known deviations forced by missing `CharacterState` fields (state.rs is
//! outside this task's scope); each is marked at its use site and needs a
//! follow-up:
//! - `oMarioLongJumpIsSlow` (decomp: src/game/mario.c:866) has no state
//!   field; the fast/slow long-jump animation is approximated from the
//!   current forward speed.
//! - `oMarioSteepJumpYaw` (decomp: src/game/mario.c:741) has no state field;
//!   the decomp's render-only `gfx.angle[1]` snap is skipped.
//! - `gSpecialTripleJump` (decomp: save_file.h:78) is never set (Yoshi is
//!   out of scope); the branch is kept and evaluates to FALSE.
//! - `ACT_SPECIAL_TRIPLE_JUMP` has no port-side `ActionId` yet; it is
//!   carried as a module-local const with the decomp's value.

use super::air_model::{
    check_fall_damage_or_get_stuck, check_kick_or_dive_in_air, common_air_action_step,
};
use super::entry;
use super::{ActionCx, ActionHandler, ActionResult};
use crate::angles::Angle;
use crate::anim_table as anim_id;
use crate::input::input_flags;
use crate::sound;
use crate::state::ActionId;
use crate::step::{
    mario_set_forward_vel, perform_air_step, AirStep, AIR_STEP_CHECK_HANG,
    AIR_STEP_CHECK_LEDGE_GRAB,
};
use crate::world::surface_type;

/// decomp: `ACT_SPECIAL_TRIPLE_JUMP` (include/sm64.h:289).
/// No port-side `ActionId` exists yet; the decomp value is carried here.
const ACT_SPECIAL_TRIPLE_JUMP: ActionId = ActionId(0x030008AF);

/// decomp: `gSpecialTripleJump` (save_file.h:78).
/// Set TRUE when Mario gets Yoshi (decomp: behaviors/yoshi.inc.c:150),
/// cleared on level load (decomp: level_update.c:1262). Yoshi is out of
/// scope, so it is never set; kept for line-fidelity with `act_triple_jump`.
fn g_special_triple_jump() -> bool {
    false
}

/// decomp: `play_mario_sound(m, SOUND_ACTION_TERRAIN_JUMP, 0)`
/// (src/game/mario.c:350).
/// `play_mario_action_sound` (mario.c:304) sets MARIO_ACTION_SOUND_PLAYED;
/// `marioSound == 0` selects `play_mario_jump_sound` (mario.c:250), which
/// sets MARIO_MARIO_SOUND_PLAYED. The nonzero-`marioSound` variants
/// (e.g. `SOUND_MARIO_HOOHOO`) instead go through `play_sound_if_no_flag`,
/// which sets MARIO_MARIO_SOUND_PLAYED as well — same net flags. So every
/// `play_mario_sound(..., ...)` call below is this helper.
fn play_jump_sound(cx: &mut ActionCx) {
    sound::play_mario_action_sound(&mut cx.state);
    sound::play_mario_jump_sound(&mut cx.state);
}

/// Single jump.
/// decomp: `act_jump` (src/game/mario_actions_airborne.c:446).
pub struct Jump;
impl ActionHandler for Jump {
    fn name(&self) -> &'static str {
        "Jump"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: if (check_kick_or_dive_in_air(m)) return TRUE;
        if let Some(r) = check_kick_or_dive_in_air(cx) {
            return r;
        }

        // decomp: if (m->input & INPUT_Z_PRESSED) return set_mario_action(m, ACT_GROUND_POUND, 0);
        if cx.state.input & input_flags::Z_PRESSED != 0 {
            let id = entry::set_mario_action_airborne(cx, ActionId::GROUND_POUND, 0);
            return cx.goto(id, 0);
        }

        play_jump_sound(cx);
        let outcome = common_air_action_step(
            cx,
            ActionId::JUMP_LAND,
            anim_id::MARIO_ANIM_SINGLE_JUMP,
            AIR_STEP_CHECK_LEDGE_GRAB | AIR_STEP_CHECK_HANG,
        );
        if let Some(r) = outcome.transition_taken {
            return r;
        }
        ActionResult::Stay
    }
}

/// Double jump.
/// decomp: `act_double_jump` (src/game/mario_actions_airborne.c:461).
pub struct DoubleJump;
impl ActionHandler for DoubleJump {
    fn name(&self) -> &'static str {
        "DoubleJump"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: s32 animation = (m->vel[1] >= 0.0f) ? MARIO_ANIM_DOUBLE_JUMP_RISE
        //                             : MARIO_ANIM_DOUBLE_JUMP_FALL;
        let animation = if cx.state.vel.y >= 0.0 {
            anim_id::MARIO_ANIM_DOUBLE_JUMP_RISE
        } else {
            anim_id::MARIO_ANIM_DOUBLE_JUMP_FALL
        };

        // decomp: if (check_kick_or_dive_in_air(m)) return TRUE;
        if let Some(r) = check_kick_or_dive_in_air(cx) {
            return r;
        }

        // decomp: if (m->input & INPUT_Z_PRESSED) return set_mario_action(m, ACT_GROUND_POUND, 0);
        if cx.state.input & input_flags::Z_PRESSED != 0 {
            let id = entry::set_mario_action_airborne(cx, ActionId::GROUND_POUND, 0);
            return cx.goto(id, 0);
        }

        // decomp: play_mario_sound(m, SOUND_ACTION_TERRAIN_JUMP, SOUND_MARIO_HOOHOO);
        play_jump_sound(cx);
        let outcome = common_air_action_step(
            cx,
            ActionId::DOUBLE_JUMP_LAND,
            animation,
            AIR_STEP_CHECK_LEDGE_GRAB | AIR_STEP_CHECK_HANG,
        );
        if let Some(r) = outcome.transition_taken {
            return r;
        }
        ActionResult::Stay
    }
}

/// Triple jump.
/// decomp: `act_triple_jump` (src/game/mario_actions_airborne.c:480).
pub struct TripleJump;
impl ActionHandler for TripleJump {
    fn name(&self) -> &'static str {
        "TripleJump"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: if (gSpecialTripleJump) return set_mario_action(m, ACT_SPECIAL_TRIPLE_JUMP, 0);
        if g_special_triple_jump() {
            let id = entry::set_mario_action_airborne(cx, ACT_SPECIAL_TRIPLE_JUMP, 0);
            return cx.goto(id, 0);
        }

        // decomp: if (m->input & INPUT_B_PRESSED) return set_mario_action(m, ACT_DIVE, 0);
        // NOTE: unlike act_jump/act_double_jump this goes straight to DIVE,
        // not through check_kick_or_dive_in_air.
        if cx.state.input & input_flags::B_PRESSED != 0 {
            let id = entry::set_mario_action_airborne(cx, ActionId::DIVE, 0);
            return cx.goto(id, 0);
        }

        // decomp: if (m->input & INPUT_Z_PRESSED) return set_mario_action(m, ACT_GROUND_POUND, 0);
        if cx.state.input & input_flags::Z_PRESSED != 0 {
            let id = entry::set_mario_action_airborne(cx, ActionId::GROUND_POUND, 0);
            return cx.goto(id, 0);
        }

        // decomp (US): play_mario_sound(m, SOUND_ACTION_TERRAIN_JUMP, 0);
        play_jump_sound(cx);
        let outcome = common_air_action_step(
            cx,
            ActionId::TRIPLE_JUMP_LAND,
            anim_id::MARIO_ANIM_TRIPLE_JUMP,
            0,
        );
        // decomp: play_flip_sounds(m, 2, 8, 20) — plain play_sound
        // (decomp: src/game/mario_actions_airborne.c:17), no flag effects;
        // audio skipped.
        if let Some(r) = outcome.transition_taken {
            return r;
        }
        ActionResult::Stay
    }
}

/// Backflip.
/// decomp: `act_backflip` (src/game/mario_actions_airborne.c:509).
pub struct Backflip;
impl ActionHandler for Backflip {
    fn name(&self) -> &'static str {
        "Backflip"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: if (m->input & INPUT_Z_PRESSED) return set_mario_action(m, ACT_GROUND_POUND, 0);
        if cx.state.input & input_flags::Z_PRESSED != 0 {
            let id = entry::set_mario_action_airborne(cx, ActionId::GROUND_POUND, 0);
            return cx.goto(id, 0);
        }

        // decomp: play_mario_sound(m, SOUND_ACTION_TERRAIN_JUMP, SOUND_MARIO_YAH_WAH_HOO);
        play_jump_sound(cx);
        let outcome = common_air_action_step(
            cx,
            ActionId::BACKFLIP_LAND,
            anim_id::MARIO_ANIM_BACKFLIP,
            0,
        );
        // decomp: play_flip_sounds(m, 2, 3, 17) — plain play_sound, no flag
        // effects; audio skipped.
        if let Some(r) = outcome.transition_taken {
            return r;
        }
        ActionResult::Stay
    }
}

/// Side flip.
/// decomp: `act_side_flip` (src/game/mario_actions_airborne.c:595).
pub struct SideFlip;
impl ActionHandler for SideFlip {
    fn name(&self) -> &'static str {
        "SideFlip"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: if (m->input & INPUT_B_PRESSED) return set_mario_action(m, ACT_DIVE, 0);
        if cx.state.input & input_flags::B_PRESSED != 0 {
            let id = entry::set_mario_action_airborne(cx, ActionId::DIVE, 0);
            return cx.goto(id, 0);
        }

        // decomp: if (m->input & INPUT_Z_PRESSED) return set_mario_action(m, ACT_GROUND_POUND, 0);
        if cx.state.input & input_flags::Z_PRESSED != 0 {
            let id = entry::set_mario_action_airborne(cx, ActionId::GROUND_POUND, 0);
            return cx.goto(id, 0);
        }

        // decomp: play_mario_sound(m, SOUND_ACTION_TERRAIN_JUMP, 0);
        play_jump_sound(cx);

        let outcome = common_air_action_step(
            cx,
            ActionId::SIDE_FLIP_LAND,
            anim_id::MARIO_ANIM_SLIDEFLIP,
            AIR_STEP_CHECK_LEDGE_GRAB,
        );
        if outcome.step != AirStep::GrabbedLedge {
            // decomp: m->marioObj->header.gfx.angle[1] += 0x8000 — render-only
            // (visual mesh yaw); skipped.
        }

        // decomp: if (animFrame == 6) play_sound(SOUND_ACTION_SIDE_FLIP_UNK, ...) —
        // plain play_sound, no flag effects; audio skipped.

        if let Some(r) = outcome.transition_taken {
            return r;
        }
        ActionResult::Stay
    }
}

/// Wall-kick air.
/// decomp: `act_wall_kick_air` (src/game/mario_actions_airborne.c:618).
pub struct WallKickAir;
impl ActionHandler for WallKickAir {
    fn name(&self) -> &'static str {
        "WallKickAir"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: if (m->input & INPUT_B_PRESSED) return set_mario_action(m, ACT_DIVE, 0);
        if cx.state.input & input_flags::B_PRESSED != 0 {
            let id = entry::set_mario_action_airborne(cx, ActionId::DIVE, 0);
            return cx.goto(id, 0);
        }

        // decomp: if (m->input & INPUT_Z_PRESSED) return set_mario_action(m, ACT_GROUND_POUND, 0);
        if cx.state.input & input_flags::Z_PRESSED != 0 {
            let id = entry::set_mario_action_airborne(cx, ActionId::GROUND_POUND, 0);
            return cx.goto(id, 0);
        }

        // decomp: play_mario_jump_sound(m) — sets MARIO_MARIO_SOUND_PLAYED only.
        sound::play_mario_jump_sound(&mut cx.state);

        let outcome = common_air_action_step(
            cx,
            ActionId::JUMP_LAND,
            anim_id::MARIO_ANIM_SLIDEJUMP,
            AIR_STEP_CHECK_LEDGE_GRAB,
        );
        if let Some(r) = outcome.transition_taken {
            return r;
        }
        ActionResult::Stay
    }
}

/// Long jump.
/// decomp: `act_long_jump` (src/game/mario_actions_airborne.c:632).
pub struct LongJump;
impl ActionHandler for LongJump {
    fn name(&self) -> &'static str {
        "LongJump"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: if (!m->marioObj->oMarioLongJumpIsSlow) animation = MARIO_ANIM_FAST_LONGJUMP;
        //         else animation = MARIO_ANIM_SLOW_LONGJUMP;
        // (mario.c:866; the flag is entry-constant in state.long_jump_is_slow).
        let animation = if cx.state.long_jump_is_slow {
            anim_id::MARIO_ANIM_SLOW_LONGJUMP
        } else {
            anim_id::MARIO_ANIM_FAST_LONGJUMP
        };

        // decomp: play_mario_sound(m, SOUND_ACTION_TERRAIN_JUMP, SOUND_MARIO_YAHOO);
        play_jump_sound(cx);

        // decomp: if (m->floor->type == SURFACE_VERTICAL_WIND && m->actionState == 0) {
        //             play_sound(SOUND_MARIO_HERE_WE_GO, ...); m->actionState = 1; }
        // The play_sound has no flag effects (audio skipped); the actionState
        // latch is real state.
        if cx.state.action_state == 0 {
            if let Some(f) = cx.state.floor {
                if f.surface_type as u16 == surface_type::VERTICAL_WIND {
                    cx.state.action_state = 1;
                }
            }
        }

        let outcome = common_air_action_step(
            cx,
            ActionId::LONG_JUMP_LAND,
            animation,
            AIR_STEP_CHECK_LEDGE_GRAB,
        );
        if let Some(r) = outcome.transition_taken {
            return r;
        }
        ActionResult::Stay
    }
}

/// Steep jump: runs its own air step instead of `common_air_action_step`.
/// decomp: `act_steep_jump` (src/game/mario_actions_airborne.c:888).
pub struct SteepJump;
impl ActionHandler for SteepJump {
    fn name(&self) -> &'static str {
        "SteepJump"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: if (m->input & INPUT_B_PRESSED) return set_mario_action(m, ACT_DIVE, 0);
        if cx.state.input & input_flags::B_PRESSED != 0 {
            let id = entry::set_mario_action_airborne(cx, ActionId::DIVE, 0);
            return cx.goto(id, 0);
        }

        // decomp: play_mario_sound(m, SOUND_ACTION_TERRAIN_JUMP, 0);
        play_jump_sound(cx);

        // decomp: mario_set_forward_vel(m, 0.98f * m->forwardVel);
        let new_forward = 0.98 * cx.state.forward_speed;
        mario_set_forward_vel(&mut cx.state, new_forward);

        match perform_air_step(&mut cx.state, cx.world, 0) {
            AirStep::Landed => {
            cx.state.land_from = cx.state.action;
                // decomp: if (!check_fall_damage_or_get_stuck(m, ACT_HARD_BACKWARD_GROUND_KB)) {
                if !check_fall_damage_or_get_stuck(cx, ActionId::HARD_BACKWARD_GROUND_KB) {
                    // decomp: m->faceAngle[0] = 0;
                    cx.state.face_pitch = Angle::ZERO;
                    // decomp: set_mario_action(m, m->forwardVel < 0.0f ? ACT_BEGIN_SLIDING
                    //                                  : ACT_JUMP_LAND, 0);
                    // act_steep_jump returns FALSE, so this is next-tick.
                    // BEGIN_SLIDING is remapped to butt/stomach slide by the
                    // moving-group routing (applied by cx.goto).
                    let land_action = if cx.state.forward_speed < 0.0 {
                        ActionId::BEGIN_SLIDING
                    } else {
                        ActionId::JUMP_LAND
                    };
                    let id = entry::set_mario_action_moving(cx, land_action);
                    return cx.goto_next(id, 0);
                }
            }
            AirStep::HitWall => {
                // decomp: mario_set_forward_vel(m, 0.0f);
                mario_set_forward_vel(&mut cx.state, 0.0);
            }
            AirStep::HitLavaWall => {
                // decomp: lava_boost_on_wall(m) — no lava surfaces in the
                // scenario set; skipped.
            }
            _ => {}
        }

        // decomp: set_mario_animation(m, MARIO_ANIM_SINGLE_JUMP);
        cx.set_mario_animation(anim_id::MARIO_ANIM_SINGLE_JUMP);
        // decomp: m->marioObj->header.gfx.angle[1] = m->marioObj->oMarioSteepJumpYaw —
        // render-only (visual mesh yaw), and oMarioSteepJumpYaw has no
        // CharacterState field; skipped.
        ActionResult::Stay
    }
}
