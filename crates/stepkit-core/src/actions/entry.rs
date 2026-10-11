//! Action-entry helpers: the `set_mario_action*` family from the decomp.
//!
//! These run at transition time (not per-frame): they pick the concrete
//! action, set entry velocities, and return whether the new action runs in
//! the same tick (`Goto`) or the next one (`GotoNext`).
//! decomp: src/game/mario.c (set_mario_action_moving:896,
//! set_jump_from_landing:1018, set_jumping_action:1074,
//! drop_and_set_mario_action:1098, check_common_action_exits:1117,
//! set_water_plunge_action:1174), src/game/mario_actions_moving.c
//! (set_triple_jump_action:147).
//!
//! The per-action entry velocities live in `super::air::enter_*` (ported
//! under AR-04); these helpers add the routing (steep / quicksand /
//! double-jump-window) around them.

use super::{ActionCx, ActionResult};
use crate::angles::Angle;
use crate::input::input_flags;
use crate::state::{mario_flags, ActionId};
use crate::trig;
use crate::world::{mario_facing_downhill, mario_floor_is_steep, mario_get_floor_class, SurfaceClass};

/// Drop anything held (no-op: no objects in scope) and transition; the new
/// action first runs NEXT tick.
/// decomp: `drop_and_set_mario_action` (mario.c:1098) returns
/// `set_mario_action`'s value, and callers that use it return FALSE, so the
/// action loop does not continue.
pub fn drop_and_set_mario_action(
    cx: &mut ActionCx,
    id: ActionId,
    arg: u32,
) -> ActionResult {
    // mario_stop_riding_and_holding: no held/ridden objects in scope.
    cx.goto_next(id, arg)
}

/// Water plunge entry: quarter the forward speed, halve vertical speed,
/// snap below the surface.
/// decomp: `set_water_plunge_action` (mario.c:1174). Camera mode is
/// renderer-only.
pub fn set_water_plunge_action(cx: &mut ActionCx) -> ActionResult {
    cx.state.forward_speed /= 4.0;
    cx.state.vel.y /= 2.0;
    cx.state.pos.y = cx.state.water_level - 100.0;
    cx.state.face_roll = Angle(0);
    cx.state.angle_vel = [Angle(0), Angle(0), Angle(0)];
    if cx.state.action.0 & crate::state::act_flags::DIVING == 0 {
        cx.state.face_pitch = Angle(0);
    }
    cx.goto(ActionId::WATER_PLUNGE, 0)
}

/// Jump routing from a landing action: quicksand / steep / double-jump
/// window via `doubleJumpTimer` + `prevAction`.
/// decomp: `set_jump_from_landing` (mario.c:1018). Returns TRUE (same tick).
pub fn set_jump_from_landing(cx: &mut ActionCx) -> ActionResult {
    if cx.state.quicksand_depth >= 11.0 {
        // heldObj is always NULL (no objects in scope).
        return cx.goto(ActionId::QUICKSAND_JUMP_LAND, 0);
    }

    if mario_floor_is_steep(&cx.state) {
        let r = set_steep_jump_action(cx);
        cx.state.double_jump_timer = 0;
        return r;
    }

    if cx.state.double_jump_timer == 0 || cx.state.squish_timer != 0 {
        let r = super::air::enter_jump(cx);
        cx.state.double_jump_timer = 0;
        return r;
    }

    let r = match cx.state.prev_action {
        ActionId::JUMP_LAND | ActionId::FREEFALL_LAND | ActionId::SIDE_FLIP_LAND_STOP => {
            enter_double_jump_or_downgrade(cx)
        }
        ActionId::DOUBLE_JUMP_LAND => {
            // Wing cap never present in scope; the decomp's
            // ACT_FLYING_TRIPLE_JUMP branch is dead here.
            if cx.state.forward_speed > 20.0 {
                super::air::enter_triple_jump(cx)
            } else {
                super::air::enter_jump(cx)
            }
        }
        _ => super::air::enter_jump(cx),
    };
    cx.state.double_jump_timer = 0;
    r
}

/// Double jump with the squish/quicksand downgrade to a single jump.
/// decomp: `set_mario_action_airborne` (mario.c:776): when
/// `squishTimer != 0 || quicksandDepth >= 1.0`, DOUBLE_JUMP (and TWIRLING)
/// become JUMP.
fn enter_double_jump_or_downgrade(cx: &mut ActionCx) -> ActionResult {
    if cx.state.squish_timer != 0 || cx.state.quicksand_depth >= 1.0 {
        super::air::enter_jump(cx)
    } else {
        super::air::enter_double_jump(cx)
    }
}

/// Jump routing that is overruled by quicksand or a steep floor.
/// decomp: `set_jumping_action` (mario.c:1074). Returns TRUE (same tick).
pub fn set_jumping_action(cx: &mut ActionCx, action: ActionId, arg: u32) -> ActionResult {
    if cx.state.quicksand_depth >= 11.0 {
        // heldObj is always NULL (no objects in scope).
        // decomp: set_mario_action(ACT_QUICKSAND_JUMP_LAND, 0) → TRUE.
        let id = set_mario_action_moving(cx, ActionId::QUICKSAND_JUMP_LAND);
        return cx.goto(id, 0);
    }

    if mario_floor_is_steep(&cx.state) {
        return set_steep_jump_action(cx);
    }

    // decomp: set_mario_action(m, action, actionArg) → TRUE.
    // For air actions, this routes through set_mario_action_airborne
    // (entry velocities, peakHeight, UNKNOWN_08).
    let id = set_mario_action_airborne(cx, action, arg);
    // The decomp's set_mario_action applies group routing; the port's
    // set_mario_action_airborne already did the airborne part. For moving-
    // group actions, apply moving routing too.
    let id = set_mario_action_moving(cx, id);
    cx.goto(id, arg)
}

/// Triple-jump routing (from the double-jump landing's A press).
/// decomp: `set_triple_jump_action` (mario_actions_moving.c:147).
/// The wing-cap branch is dead in scope (no wing cap); returns TRUE.
pub fn set_triple_jump_action(cx: &mut ActionCx) -> ActionResult {
    debug_assert!(cx.state.action == ActionId::DOUBLE_JUMP_LAND);
    if cx.state.flags & mario_flags::WING_CAP != 0 {
        // ACT_FLYING_TRIPLE_JUMP: not ported (Phase 7+); unreachable here.
        return cx.goto(ActionId::JUMP, 0);
    } else if cx.state.forward_speed > 20.0 {
        super::air::enter_triple_jump(cx)
    } else {
        super::air::enter_jump(cx)
    }
}

/// Steep jump: redirect the horizontal velocity up the slope, then go to
/// STEEP_JUMP.
/// decomp: `set_steep_jump_action` (mario.c:741). The `//!` UB comment is
/// real: `(s16)(floorAngle + 0x8000)` wraps; replicate with wrapping
/// arithmetic. The `oMarioSteepJumpYaw` object field has no equivalent.
pub fn set_steep_jump_action(cx: &mut ActionCx) -> ActionResult {
    if cx.state.forward_speed > 0.0 {
        let angle_temp = cx.state.floor_angle.0.wrapping_add(0x8000u16 as i16);
        let face_angle_temp = cx.state.face_yaw.0.wrapping_sub(angle_temp);
        let y = trig::sins(Angle(face_angle_temp)) * cx.state.forward_speed;
        let x = trig::coss(Angle(face_angle_temp)) * cx.state.forward_speed * 0.75;
        cx.state.forward_speed = (y * y + x * x).sqrt();
        cx.state.face_yaw = Angle(trig::atan2s(x, y).0.wrapping_add(angle_temp));
    }
    // decomp: set_mario_action_airborne's ACT_STEEP_JUMP case (mario.c:845):
    // vel.y = 42 + forwardVel*0.25 (halved if squished/quicksand), pitch up.
    cx.state.vel.y =
        super::air::adjusted_jump_vy(cx, 42.0 + cx.state.forward_speed * 0.25);
    cx.state.face_pitch = Angle(-0x2000);
    // decomp: drop_and_set_mario_action, whose TRUE return propagates, so
    // the new action runs the same tick.
    cx.goto(ActionId::STEEP_JUMP, 0)
}

/// Moving-group entry adjustments: walk entry speed clamp and the
/// BEGIN_SLIDING dispatcher.
/// decomp: `set_mario_action_moving` (mario.c:896). Returns the (possibly
/// remapped) action id; the caller transitions to it.
pub fn set_mario_action_moving(cx: &mut ActionCx, action: ActionId) -> ActionId {
    let floor_class = mario_get_floor_class(&cx.state);
    let forward_vel = cx.state.forward_speed;
    let mag = cx.state.intended_mag.min(8.0);

    match action {
        ActionId::WALKING => {
            // NOTE the `0.0 <= forwardVel` condition: negative speeds are
            // NOT clamped up (the plan's GR-03 called this out).
            if floor_class != SurfaceClass::VerySlippery
                && 0.0 <= forward_vel
                && forward_vel < mag
            {
                cx.state.forward_speed = mag;
            }
            // oMarioWalkingPitch is an object field; no equivalent.
        }
        ActionId::BEGIN_SLIDING => {
            return if mario_facing_downhill(&cx.state, 0) {
                ActionId::BUTT_SLIDE
            } else {
                ActionId::STOMACH_SLIDE
            };
        }
        _ => {}
    }
    action
}

/// Common exits checked by braking-style actions: A, off-floor, stick,
/// slide trigger, in that order.
/// decomp: `check_common_action_exits` (mario.c:1117). Each branch returns
/// `set_mario_action` (TRUE): same-tick.
pub fn check_common_action_exits(
    cx: &mut ActionCx,
    prev_buttons: u16,
) -> Option<ActionResult> {
    let pressed_a = cx.pressed(crate::input::buttons::A, prev_buttons);
    if pressed_a {
        // decomp: set_mario_action(m, ACT_JUMP, 0) — direct, NOT
        // set_jumping_action (no steep/quicksand routing here).
        return Some(super::air::enter_jump(cx));
    }
    if cx.state.input & input_flags::OFF_FLOOR != 0 {
        return Some(cx.goto(ActionId::FREEFALL, 0));
    }
    if cx.state.input & input_flags::NONZERO_ANALOG != 0 {
        let id = set_mario_action_moving(cx, ActionId::WALKING);
        return Some(cx.goto(id, 0));
    }
    if cx.state.input & input_flags::ABOVE_SLIDE != 0 {
        let id = set_mario_action_moving(cx, ActionId::BEGIN_SLIDING);
        return Some(cx.goto(id, 0));
    }
    None
}

/// decomp: `set_mario_y_vel_based_on_fspeed` (`src/game/mario.c:763`).
/// `get_additive_y_vel_for_jumps` is always 0 (stubbed in the decomp).
fn set_mario_y_vel_based_on_fspeed(cx: &mut ActionCx, initial_vel_y: f32, multiplier: f32) {
    cx.state.vel.y = initial_vel_y + cx.state.forward_speed * multiplier;
    // NOTE: strict `>` here, vs `>=` in the action-downgrade below (plan AR-04).
    if cx.state.squish_timer != 0 || cx.state.quicksand_depth > 1.0 {
        cx.state.vel.y *= 0.5;
    }
}

/// Airborne action entry: routing + entry velocities.
/// decomp: `set_mario_action_airborne` (`src/game/mario.c:776`).
/// Returns the (possibly downgraded) action id; the caller applies it via
/// `cx.goto`/`cx.goto_next` per the decomp's return contract.
pub fn set_mario_action_airborne(
    cx: &mut ActionCx,
    action: ActionId,
    action_arg: u32,
) -> ActionId {
    // decomp: squish/quicksand downgrade — DOUBLE_JUMP/TWIRLING → JUMP.
    // NOTE: `>=` here (vs `>` in set_mario_y_vel_based_on_fspeed).
    let mut action = action;
    if (cx.state.squish_timer != 0 || cx.state.quicksand_depth >= 1.0)
        && (action == ActionId::DOUBLE_JUMP || action == ActionId::TWIRLING)
    {
        action = ActionId::JUMP;
    }

    match action {
        ActionId::DOUBLE_JUMP => {
            set_mario_y_vel_based_on_fspeed(cx, 52.0, 0.25);
            cx.state.forward_speed *= 0.8;
        }
        ActionId::BACKFLIP => {
            // decomp: m->marioObj->header.gfx.animInfo.animID = -1
            // (forces anim reinit); the port's anim.id serves the same role.
            cx.state.anim.id = -1;
            cx.state.forward_speed = -16.0;
            set_mario_y_vel_based_on_fspeed(cx, 62.0, 0.0);
        }
        ActionId::TRIPLE_JUMP => {
            set_mario_y_vel_based_on_fspeed(cx, 69.0, 0.0);
            cx.state.forward_speed *= 0.8;
        }
        ActionId::FLYING_TRIPLE_JUMP => {
            set_mario_y_vel_based_on_fspeed(cx, 82.0, 0.0);
        }
        ActionId::WATER_JUMP | ActionId::HOLD_WATER_JUMP => {
            if action_arg == 0 {
                set_mario_y_vel_based_on_fspeed(cx, 42.0, 0.0);
            }
        }
        ActionId::BURNING_JUMP => {
            cx.state.vel.y = 31.5;
            cx.state.forward_speed = 8.0;
        }
        ActionId::RIDING_SHELL_JUMP => {
            set_mario_y_vel_based_on_fspeed(cx, 42.0, 0.25);
        }
        ActionId::JUMP | ActionId::HOLD_JUMP => {
            cx.state.anim.id = -1;
            set_mario_y_vel_based_on_fspeed(cx, 42.0, 0.25);
            cx.state.forward_speed *= 0.8;
        }
        ActionId::WALL_KICK_AIR | ActionId::TOP_OF_POLE_JUMP => {
            set_mario_y_vel_based_on_fspeed(cx, 62.0, 0.0);
            if cx.state.forward_speed < 24.0 {
                cx.state.forward_speed = 24.0;
            }
            cx.state.wall_kick_timer = 0;
        }
        ActionId::SIDE_FLIP => {
            set_mario_y_vel_based_on_fspeed(cx, 62.0, 0.0);
            cx.state.forward_speed = 8.0;
            cx.state.face_yaw = cx.state.intended_yaw;
        }
        ActionId::STEEP_JUMP => {
            cx.state.anim.id = -1;
            set_mario_y_vel_based_on_fspeed(cx, 42.0, 0.25);
            cx.state.face_pitch = crate::angles::Angle(-0x2000);
        }
        ActionId::LAVA_BOOST => {
            cx.state.vel.y = 84.0;
            if action_arg == 0 {
                cx.state.forward_speed = 0.0;
            }
        }
        ActionId::DIVE => {
            let mut forward_vel = cx.state.forward_speed + 15.0;
            if forward_vel > 48.0 {
                forward_vel = 48.0;
            }
            crate::step::mario_set_forward_vel(&mut cx.state, forward_vel);
        }
        ActionId::LONG_JUMP => {
            cx.state.anim.id = -1;
            set_mario_y_vel_based_on_fspeed(cx, 30.0, 0.0);
            // decomp: oMarioLongJumpIsSlow = forwardVel > 16 ? FALSE : TRUE
            // (mario.c:866), evaluated BEFORE the 1.5x multiplier below.
            cx.state.long_jump_is_slow = cx.state.forward_speed <= 16.0;
            // BLJ: (forwardVel *= 1.5) > 48 → 48. Negative speeds unclamped.
            cx.state.forward_speed *= 1.5;
            if cx.state.forward_speed > 48.0 {
                cx.state.forward_speed = 48.0;
            }
        }
        ActionId::SLIDE_KICK => {
            cx.state.vel.y = 12.0;
            if cx.state.forward_speed < 32.0 {
                cx.state.forward_speed = 32.0;
            }
        }
        ActionId::JUMP_KICK => {
            cx.state.vel.y = 20.0;
        }
        _ => {}
    }

    cx.state.peak_height = cx.state.pos.y;
    cx.state.flags |= crate::state::mario_flags::UNKNOWN_08;
    action
}
