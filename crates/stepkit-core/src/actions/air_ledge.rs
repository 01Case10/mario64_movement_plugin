//! Verbatim ports of the decomp's ledge actions from
//! `src/game/mario_actions_automatic.c` (lines ~490-640).
//!
//! These supersede the bespoke `LedgeGrab`/`LedgeClimb*` handlers in
//! [`super::air`]; they follow the decomp exactly (per plan AR-11):
//! - `let_go_of_ledge` (automatic.c:490)
//! - `climb_up_ledge` (automatic.c:510)
//! - `update_ledge_climb_camera` (automatic.c:516)
//! - `update_ledge_climb` (automatic.c:523)
//! - `act_ledge_grab` (automatic.c:532)
//! - `act_ledge_climb_slow` (automatic.c:605)
//! - `act_ledge_climb_down` (automatic.c:625)
//!
//! Conventions: `m->input & INPUT_*` reads become
//! `cx.state.input & input_flags::*`; the decomp's `A_PRESSED`/`Z_PRESSED`
//! edges are already edge-triggered in `cx.state.input` (input.rs,
//! `update_mario_button_inputs`). `set_mario_action`'s return value maps to
//! tick semantics: paths that `return set_mario_action(...)` (TRUE) are
//! same-tick (`cx.goto`); `update_ledge_climb` discards it, so its
//! transitions are next-tick (`cx.goto_next`). Camera and gfx writes are
//! skipped with comments; the state side effects (timer, flags) are kept.

use super::{ActionCx, ActionHandler, ActionResult};
use crate::angles::Angle;
use crate::anim_table;
use crate::input::input_flags as f;
use crate::state::{mario_flags, ActionId};
use crate::trig;
use crate::world;

// ---------------------------------------------------------------------------
// Shared helpers (pub: other action modules may call these).
// ---------------------------------------------------------------------------

/// decomp: src/game/mario_actions_automatic.c:490 (`let_go_of_ledge`).
/// The decomp returns `set_mario_action`'s TRUE, so this is a same-tick
/// transition (`cx.goto`).
pub fn let_go_of_ledge(cx: &mut ActionCx) -> ActionResult {
    cx.state.vel.y = 0.0;
    // decomp: direct `m->forwardVel = -8.0f` store — NOT mario_set_forward_vel,
    // so vel xz are deliberately left as-is.
    cx.state.forward_speed = -8.0;
    let (s, c) = (trig::sins(cx.state.face_yaw), trig::coss(cx.state.face_yaw));
    cx.state.pos.x -= 60.0 * s;
    cx.state.pos.z -= 60.0 * c;

    // decomp: find_floor with no hit returns FLOOR_LOWER_LIMIT, and the
    // `< pos.y - 100` branch then drops pos.y by 100.
    let floor_height = cx
        .world
        .find_floor(cx.state.pos.x, cx.state.pos.y, cx.state.pos.z)
        .map(|floor| floor.y)
        .unwrap_or(world::FLOOR_LOWER_LIMIT);
    if floor_height < cx.state.pos.y - 100.0 {
        cx.state.pos.y -= 100.0;
    } else {
        cx.state.pos.y = floor_height;
    }

    cx.goto(ActionId::SOFT_BONK, 0)
}

/// decomp: src/game/mario_actions_automatic.c:510 (`climb_up_ledge`).
pub fn climb_up_ledge(cx: &mut ActionCx) {
    cx.set_mario_animation(anim_table::MARIO_ANIM_IDLE_HEAD_LEFT);
    let (s, c) = (trig::sins(cx.state.face_yaw), trig::coss(cx.state.face_yaw));
    cx.state.pos.x += 14.0 * s;
    cx.state.pos.z += 14.0 * c;
    // decomp: vec3f_copy(m->marioObj->header.gfx.pos, m->pos) — renderer; skipped.
}

/// decomp: src/game/mario_actions_automatic.c:516 (`update_ledge_climb_camera`).
/// The camera-position writes are visual-only and skipped; the
/// `actionTimer++` and `flags |= MARIO_UNKNOWN_25` are real state.
pub fn update_ledge_climb_camera(cx: &mut ActionCx) {
    // decomp: sp4 = min(actionTimer, 14);
    //         statusForCamera->pos = pos + sp4 * facing — camera struct; skipped.
    cx.state.action_timer += 1;
    cx.state.flags |= mario_flags::UNKNOWN_25;
}

/// decomp: src/game/mario_actions_automatic.c:523 (`update_ledge_climb`).
/// The decomp discards `set_mario_action`'s return value (void function);
/// the caller's `return FALSE` governs tick timing, so the transition is
/// next-tick. Returns `Some` when the transition fired, `None` otherwise.
pub fn update_ledge_climb(
    cx: &mut ActionCx,
    animation: i32,
    end_action: ActionId,
) -> Option<ActionResult> {
    crate::step::stop_and_set_height_to_floor(&mut cx.state);

    cx.set_mario_animation(animation);
    if cx.is_anim_at_end() {
        let result = cx.goto_next(end_action, 0);
        if end_action == ActionId::IDLE {
            climb_up_ledge(cx);
        }
        Some(result)
    } else {
        None
    }
}

/// decomp: `find_floor_height_relative_polar` (mario.c:674): floor height at
/// a polar offset from Mario, probing from `pos.y + 100`.
fn find_floor_height_relative_polar(
    cx: &ActionCx,
    angle_from_mario: i16,
    dist_from_mario: f32,
) -> f32 {
    let angle = cx.state.face_yaw.wrapping_add(Angle(angle_from_mario));
    // decomp variable naming: `y = sins(...)*dist` is the x offset,
    // `x = coss(...)*dist` is the z offset.
    let x = cx.state.pos.x + trig::sins(angle) * dist_from_mario;
    let z = cx.state.pos.z + trig::coss(angle) * dist_from_mario;
    cx.world
        .find_floor(x, cx.state.pos.y + 100.0, z)
        .map(|floor| floor.y)
        .unwrap_or(world::FLOOR_LOWER_LIMIT)
}

// ---------------------------------------------------------------------------
// act_ledge_grab
// ---------------------------------------------------------------------------

/// decomp: src/game/mario_actions_automatic.c:532 (`act_ledge_grab`).
pub struct LedgeGrab;

impl ActionHandler for LedgeGrab {
    fn name(&self) -> &'static str {
        "LedgeGrab"
    }

    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: s16 intendedDYaw = m->intendedYaw - m->faceAngle[1];
        let intended_dyaw = cx.state.intended_yaw.0.wrapping_sub(cx.state.face_yaw.0);
        // decomp: s32 hasSpaceForMario = (m->ceilHeight - m->floorHeight >= 160.0f);
        let has_space_for_mario = cx.state.ceil_y.unwrap_or(world::CELL_HEIGHT_LIMIT)
            - cx.state.floor_y.unwrap_or(world::FLOOR_LOWER_LIMIT)
            >= 160.0;

        if cx.state.action_timer < 10 {
            cx.state.action_timer += 1;
        }

        // decomp: m->floor->normal.y < 0.9063078f (a missing floor reads as
        // flat ground, per the port's convention).
        if cx.state.floor.map(|floor| floor.normal.y).unwrap_or(1.0) < 0.9063078 {
            return let_go_of_ledge(cx);
        }

        if cx.state.input & (f::Z_PRESSED | f::OFF_FLOOR) != 0 {
            return let_go_of_ledge(cx);
        }

        if cx.state.input & f::A_PRESSED != 0 && has_space_for_mario {
            return cx.goto(ActionId::LEDGE_CLIMB_FAST, 0);
        }

        if cx.state.input & f::STOMPED != 0 {
            // decomp: damage only when `m->marioObj->oInteractStatus &
            // INT_STATUS_MARIO_KNOCKBACK_DMG`; interact status is not modeled
            // in the port, so the damage is applied whenever STOMPED fires.
            // (Flagged deviation.)
            let dmg = if cx.state.flags & mario_flags::CAP_ON_HEAD != 0 {
                12
            } else {
                18
            };
            cx.state.hurt_counter = cx.state.hurt_counter.saturating_add(dmg);
            return let_go_of_ledge(cx);
        }

        if cx.state.action_timer == 10 && cx.state.input & f::NONZERO_ANALOG != 0 {
            // NOTE: the oracle builds with VERSION_US (oracle/build.sh), so
            // the VERSION_EU `&& !(m->input & INPUT_A_DOWN)` guard is not active.
            if intended_dyaw >= -0x4000 && intended_dyaw <= 0x4000 {
                if has_space_for_mario {
                    return cx.goto(ActionId::LEDGE_CLIMB_SLOW_1, 0);
                }
            } else {
                return let_go_of_ledge(cx);
            }
        }

        // decomp: find_floor_height_relative_polar(m, -0x8000, 30.0f).
        let height_above_floor =
            cx.state.pos.y - find_floor_height_relative_polar(cx, -0x8000, 30.0);
        if has_space_for_mario && height_above_floor < 100.0 {
            return cx.goto(ActionId::LEDGE_CLIMB_FAST, 0);
        }

        if cx.state.action_arg == 0 {
            // decomp: play_sound_if_no_flag(m, SOUND_MARIO_WHOA, MARIO_MARIO_SOUND_PLAYED);
            // audio skipped; the flag is real state.
            crate::sound::play_sound_if_no_flag(&mut cx.state, mario_flags::MARIO_SOUND_PLAYED);
        }

        crate::step::stop_and_set_height_to_floor(&mut cx.state);
        cx.set_mario_animation(anim_table::MARIO_ANIM_IDLE_ON_LEDGE);

        ActionResult::Stay
    }
}

// ---------------------------------------------------------------------------
// act_ledge_climb_slow
// ---------------------------------------------------------------------------

/// decomp: src/game/mario_actions_automatic.c:605 (`act_ledge_climb_slow`).
/// The decomp dispatches both `ACT_LEDGE_CLIMB_SLOW_1` and
/// `ACT_LEDGE_CLIMB_SLOW_2` to this function (automatic.c:875); the frame-17
/// switch below assigns the action id directly without `set_mario_action`.
pub struct LedgeClimbSlow;

impl ActionHandler for LedgeClimbSlow {
    fn name(&self) -> &'static str {
        "LedgeClimbSlow"
    }

    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        if cx.state.input & f::OFF_FLOOR != 0 {
            return let_go_of_ledge(cx);
        }

        if cx.state.action_timer >= 28
            && cx.state.input & (f::NONZERO_ANALOG | f::A_PRESSED | f::OFF_FLOOR | f::ABOVE_SLIDE)
                != 0
        {
            climb_up_ledge(cx);
            // decomp: `return check_common_action_exits(m);` (mario.c:1117),
            // inlined verbatim — plain set_mario_action transitions (the
            // port's entry::check_common_action_exits embellishes the A
            // branch with jump-entry setup the decomp does not do here).
            if cx.state.input & f::A_PRESSED != 0 {
                return cx.goto(ActionId::JUMP, 0);
            }
            if cx.state.input & f::OFF_FLOOR != 0 {
                return cx.goto(ActionId::FREEFALL, 0);
            }
            if cx.state.input & f::NONZERO_ANALOG != 0 {
                return cx.goto(ActionId::WALKING, 0);
            }
            if cx.state.input & f::ABOVE_SLIDE != 0 {
                return cx.goto(ActionId::BEGIN_SLIDING, 0);
            }
            return ActionResult::Stay;
        }

        if cx.state.action_timer == 10 {
            // decomp: play_sound_if_no_flag(m, SOUND_MARIO_EEUH, MARIO_MARIO_SOUND_PLAYED);
            // audio skipped; the flag is real state.
            crate::sound::play_sound_if_no_flag(&mut cx.state, mario_flags::MARIO_SOUND_PLAYED);
        }

        let transition =
            update_ledge_climb(cx, anim_table::MARIO_ANIM_SLOW_LEDGE_GRAB, ActionId::IDLE);

        update_ledge_climb_camera(cx);

        // decomp: `m->marioObj->header.gfx.animInfo.animFrame == 17` — direct
        // assignment, NOT set_mario_action (timer/arg preserved).
        if cx.state.anim.frame == 17 {
            cx.state.action = ActionId::LEDGE_CLIMB_SLOW_2;
        }

        transition.unwrap_or(ActionResult::Stay)
    }
}

// ---------------------------------------------------------------------------
// act_ledge_climb_down
// ---------------------------------------------------------------------------

/// decomp: src/game/mario_actions_automatic.c:625 (`act_ledge_climb_down`).
pub struct LedgeClimbDown;

impl ActionHandler for LedgeClimbDown {
    fn name(&self) -> &'static str {
        "LedgeClimbDown"
    }

    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        if cx.state.input & f::OFF_FLOOR != 0 {
            return let_go_of_ledge(cx);
        }

        // decomp: play_sound_if_no_flag(m, SOUND_MARIO_WHOA, MARIO_MARIO_SOUND_PLAYED);
        // audio skipped; the flag is real state.
        crate::sound::play_sound_if_no_flag(&mut cx.state, mario_flags::MARIO_SOUND_PLAYED);

        let transition =
            update_ledge_climb(cx, anim_table::MARIO_ANIM_CLIMB_DOWN_LEDGE, ActionId::LEDGE_GRAB);

        // decomp: `m->actionArg = 1;` AFTER the transition (overwrites the 0
        // set_mario_action just wrote), suppressing the WHOA sound when the
        // climb-down lands back in LEDGE_GRAB.
        cx.state.action_arg = 1;

        transition.unwrap_or(ActionResult::Stay)
    }
}

/// Fast ledge climb.
/// decomp: `act_ledge_climb_fast` (`src/game/mario_actions_automatic.c:639`).
pub struct LedgeClimbFast;
impl ActionHandler for LedgeClimbFast {
    fn name(&self) -> &'static str {
        "LedgeClimbFast"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        if cx.state.input & f::OFF_FLOOR != 0 {
            return let_go_of_ledge(cx);
        }

        // decomp: play_sound_if_no_flag(m, SOUND_MARIO_UH2, MARIO_MARIO_SOUND_PLAYED);
        // audio skipped; the flag is real state.
        crate::sound::play_sound_if_no_flag(&mut cx.state, mario_flags::MARIO_SOUND_PLAYED);

        let transition =
            update_ledge_climb(cx, anim_table::MARIO_ANIM_FAST_LEDGE_GRAB, ActionId::IDLE);

        // decomp: if (animFrame == 8) play_mario_landing_sound(...)
        // — plain play_sound, no flag; audio skipped.
        // (The animFrame check reads the post-update frame.)

        update_ledge_climb_camera(cx);

        // decomp: return FALSE → next-tick. The transition (if any) was
        // already applied via goto_next inside update_ledge_climb.
        transition.unwrap_or(ActionResult::Stay)
    }
}
