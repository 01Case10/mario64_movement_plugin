//! Verbatim ports of the SM64 decomp slide family.
//!
//! Covers `update_sliding`, `update_sliding_angle`, `slide_bonk`,
//! `common_slide_action`, `common_slide_action_with_jump`,
//! `stomach_slide_action`, and the slide action handlers
//! (`act_butt_slide`, `act_crouch_slide`, `act_slide_kick_slide`,
//! `act_stomach_slide`, `act_dive_slide`) plus the anim-driven stationary
//! ends (`act_butt_slide_stop`, `act_slide_kick_slide_stop`).
//! decomp: src/game/mario_actions_moving.c (slide core),
//! src/game/mario_actions_stationary.c (stop actions).
//!
//! Numeric rules followed throughout: f32 math with C conversion
//! semantics (a double literal such as `0.9` computes in f64 before
//! narrowing), i16 angle wrapping, trig only via [`crate::trig`].

use super::{ActionCx, ActionHandler, ActionResult};
use crate::angles::Angle;
use crate::anim_table as anim_id;
use crate::input::{buttons, input_flags};
use crate::state::ActionId;
use crate::step::GroundStep;
use crate::trig;
use crate::world::SurfaceClass;

/// Inline copy of the decomp's per-tick moving-action preamble: the water
/// plunge cancel, then the quicksand update at the moving rate.
/// decomp: `check_common_moving_cancels` (src/game/mario_actions_moving.c:1953),
/// `mario_execute_moving_action` (src/game/mario_actions_moving.c:1982).
fn moving_prelude(cx: &mut ActionCx) -> Option<ActionResult> {
    // decomp: pos more than 100 below the water surface -> water plunge.
    // (The stomped/squished/death cancels live in the shared cancel layer,
    // not in the individual slide handlers.)
    if cx.state.pos.y < cx.state.water_level - 100.0 {
        return Some(super::entry::set_water_plunge_action(cx));
    }
    // The Rust `mario_update_quicksand` reports the deep/instant transition
    // without performing it (it takes `&mut CharacterState`, not the
    // `ActionCx`); the decomp performs `drop_and_set_mario_action` to
    // QUICKSAND_DEATH inside and returns TRUE, i.e. the new action runs the
    // same tick.
    if crate::step::mario_update_quicksand(&mut cx.state, 0.25) {
        return Some(cx.goto(ActionId::QUICKSAND_DEATH, 0));
    }
    None
}

/// Run a stationary step, play the stop animation, and transition when it
/// ends.
/// decomp: `stopping_step` (src/game/mario_actions_stationary.c:612).
fn stopping_step(cx: &mut ActionCx, anim: i32, action: ActionId) {
    let _ = crate::step::stationary_ground_step(&mut cx.state, cx.world);
    cx.set_mario_animation(anim);
    if cx.is_anim_at_end() {
        // decomp: `set_mario_action`, return value discarded -> the new
        // action first runs next tick.
        let _ = cx.goto_next(action, 0);
    }
}

/// Slope acceleration, facing-yaw tracking, and the one-frame-late 100
/// speed cap for a sliding Mario.
/// decomp: `update_sliding_angle` (src/game/mario_actions_moving.c:159).
pub fn update_sliding_angle(cx: &mut ActionCx, accel: f32, loss_factor: f32) {
    // decomp: `m->floor` is dereferenced directly; the `None` fallback is a
    // flat floor (steepness 0 adds no acceleration) and is unreachable in
    // normal play.
    let (floor_nx, floor_nz) = match cx.state.floor {
        Some(f) => (f.normal.x, f.normal.z),
        None => (0.0, 0.0),
    };
    let slope_angle = trig::atan2s(floor_nz, floor_nx);
    let steepness = (floor_nx * floor_nx + floor_nz * floor_nz).sqrt();

    cx.state.slide_vel_x += accel * steepness * trig::sins(slope_angle);
    cx.state.slide_vel_z += accel * steepness * trig::coss(slope_angle);

    cx.state.slide_vel_x *= loss_factor;
    cx.state.slide_vel_z *= loss_factor;

    cx.state.slide_yaw = trig::atan2s(cx.state.slide_vel_z, cx.state.slide_vel_x);

    let facing_dyaw: i16 = cx.state.face_yaw.0.wrapping_sub(cx.state.slide_yaw.0);
    let mut new_facing_dyaw: i32 = facing_dyaw as i32;

    // ! -0x4000 not handled - can slide down a slope while facing perpendicular to it
    if new_facing_dyaw > 0 && new_facing_dyaw <= 0x4000 {
        new_facing_dyaw -= 0x200;
        if new_facing_dyaw < 0 {
            new_facing_dyaw = 0;
        }
    } else if new_facing_dyaw > -0x4000 && new_facing_dyaw < 0 {
        new_facing_dyaw += 0x200;
        if new_facing_dyaw > 0 {
            new_facing_dyaw = 0;
        }
    } else if new_facing_dyaw > 0x4000 && new_facing_dyaw < 0x8000 {
        new_facing_dyaw += 0x200;
        if new_facing_dyaw > 0x8000 {
            new_facing_dyaw = 0x8000;
        }
    } else if new_facing_dyaw > -0x8000 && new_facing_dyaw < -0x4000 {
        new_facing_dyaw -= 0x200;
        if new_facing_dyaw < -0x8000 {
            new_facing_dyaw = -0x8000;
        }
    }

    cx.state.face_yaw = Angle(
        cx.state
            .slide_yaw
            .0
            .wrapping_add(new_facing_dyaw as i16),
    );

    cx.state.vel.x = cx.state.slide_vel_x;
    cx.state.vel.y = 0.0;
    cx.state.vel.z = cx.state.slide_vel_z;

    // decomp: return values discarded.
    let _ = crate::step::mario_update_moving_sand(&mut cx.state);
    let _ = crate::step::mario_update_windy_ground(&mut cx.state);

    // ! Speed is capped a frame late (butt slide HSG)
    // NOTE: `forwardVel` is computed from the pre-cap slide velocity, the
    // slide velocity is then scaled, and only afterwards is the sign
    // applied. `vel` above keeps the pre-cap values.
    let mut forward_speed =
        (cx.state.slide_vel_x * cx.state.slide_vel_x + cx.state.slide_vel_z * cx.state.slide_vel_z)
            .sqrt();
    if forward_speed > 100.0 {
        cx.state.slide_vel_x = cx.state.slide_vel_x * 100.0 / forward_speed;
        cx.state.slide_vel_z = cx.state.slide_vel_z * 100.0 / forward_speed;
    }

    if new_facing_dyaw < -0x4000 || new_facing_dyaw > 0x4000 {
        forward_speed *= -1.0;
    }
    cx.state.forward_speed = forward_speed;
}

/// Steering, surface-class accel/loss, and the flat-ground stop check for a
/// sliding Mario. Returns true when the slide stopped this tick.
/// decomp: `update_sliding` (src/game/mario_actions_moving.c:219).
pub fn update_sliding(cx: &mut ActionCx, stop_speed: f32) -> bool {
    let intended_dyaw = cx
        .proc
        .intended_yaw
        .0
        .wrapping_sub(cx.state.slide_yaw.0);
    let mut forward = trig::coss(Angle(intended_dyaw));
    let sideward = trig::sins(Angle(intended_dyaw));

    // ! 10k glitch
    if forward < 0.0 && cx.state.forward_speed >= 0.0 {
        forward *= 0.5 + 0.5 * cx.state.forward_speed / 100.0;
    }

    let intended_mag = cx.proc.intended_mag;
    let (accel, loss_factor) = match crate::world::mario_get_floor_class(&cx.state) {
        SurfaceClass::VerySlippery => (10.0, intended_mag / 32.0 * forward * 0.02 + 0.98),
        SurfaceClass::Slippery => (8.0, intended_mag / 32.0 * forward * 0.02 + 0.96),
        // decomp `default:` arm (covers SURFACE_CLASS_DEFAULT).
        SurfaceClass::Default => (7.0, intended_mag / 32.0 * forward * 0.02 + 0.92),
        SurfaceClass::NotSlippery => (5.0, intended_mag / 32.0 * forward * 0.02 + 0.92),
    };

    let old_speed =
        (cx.state.slide_vel_x * cx.state.slide_vel_x + cx.state.slide_vel_z * cx.state.slide_vel_z)
            .sqrt();

    // ! This is attempting to use trig derivatives to rotate Mario's speed.
    // It is slightly off/asymmetric since it uses the new X speed, but the old
    // Z speed.
    cx.state.slide_vel_x += cx.state.slide_vel_z * (intended_mag / 32.0) * sideward * 0.05;
    cx.state.slide_vel_z -= cx.state.slide_vel_x * (intended_mag / 32.0) * sideward * 0.05;

    let new_speed =
        (cx.state.slide_vel_x * cx.state.slide_vel_x + cx.state.slide_vel_z * cx.state.slide_vel_z)
            .sqrt();

    if old_speed > 0.0 && new_speed > 0.0 {
        cx.state.slide_vel_x = cx.state.slide_vel_x * old_speed / new_speed;
        cx.state.slide_vel_z = cx.state.slide_vel_z * old_speed / new_speed;
    }

    update_sliding_angle(cx, accel, loss_factor);

    let mut stopped = false;
    if !crate::world::mario_floor_is_slope(&cx.state)
        && cx.state.forward_speed * cx.state.forward_speed < stop_speed * stop_speed
    {
        cx.set_forward_speed(0.0);
        stopped = true;
    }
    stopped
}

/// Wall hit while sliding: fast slides reflect into `fast_action`, slow
/// slides stop into `slow_action`.
///
/// decomp: `slide_bonk` (src/game/mario_actions_moving.c:137) returns void;
/// both of its transitions (`drop_and_set_mario_action` for the fast case,
/// `set_mario_action` for the slow case) have their return values discarded
/// by every in-decomp caller, so the new action runs next tick in both
/// cases. The returned [`ActionResult`] carries the booked transition;
/// callers that want the exact decomp behavior should discard it and return
/// [`ActionResult::Stay`].
pub fn slide_bonk(
    cx: &mut ActionCx,
    fast_action: ActionId,
    slow_action: ActionId,
) -> ActionResult {
    if cx.state.forward_speed > 16.0 {
        crate::step::mario_bonk_reflection(&mut cx.state, true);
        super::entry::drop_and_set_mario_action(cx, fast_action, 0)
    } else {
        cx.set_forward_speed(0.0);
        // decomp: `set_mario_action(m, slowAction, 0)` (same-tick TRUE);
        // its return value is discarded by the callers.
        cx.goto(slow_action, 0)
    }
}

/// Shared step/anim/wall handling for the slide actions. The returned
/// [`ActionResult`] is always [`ActionResult::Stay`]: the decomp returns
/// void and its wrappers return FALSE; any transition booked inside (e.g.
/// the `LEFT_GROUND` air action) first runs next tick.
/// decomp: `common_slide_action` (src/game/mario_actions_moving.c:1359).
pub fn common_slide_action(
    cx: &mut ActionCx,
    _prev_buttons: u16,
    end_action: ActionId,
    air_action: ActionId,
    anim_id: i32,
) -> ActionResult {
    // decomp: slide sound + rumble reset + adjust_sound_for_speed skipped.
    match crate::step::perform_ground_step(&mut cx.state, cx.world) {
        GroundStep::LeftGround => {
            // decomp: `set_mario_action(m, airAction, 0)`, return value
            // discarded -> next-tick semantics.
            let _ = cx.goto_next(air_action, 0);
            // decomp: SOUND_MARIO_HOOHOO when |forwardVel| > 50 skipped.
            ActionResult::Stay
        }
        GroundStep::None => {
            cx.set_mario_animation(anim_id);
            // decomp: align_with_floor (body tilt, renderer-side) skipped.
            // decomp: PARTICLE_DUST skipped.
            ActionResult::Stay
        }
        GroundStep::HitWall => {
            if !crate::world::mario_floor_is_slippery(&cx.state) {
                // decomp: PARTICLE_VERTICAL_STAR when forwardVel > 16 skipped.
                // slide_bonk's result is discarded, matching the decomp's
                // void call; the booked transition runs next tick.
                let _ = slide_bonk(cx, ActionId::GROUND_BONK, end_action);
            } else if let Some(wall) = cx.state.wall {
                let wall_angle = trig::atan2s(wall.normal.z, wall.normal.x);
                let mut slide_speed =
                    (cx.state.slide_vel_x * cx.state.slide_vel_x
                        + cx.state.slide_vel_z * cx.state.slide_vel_z)
                        .sqrt();

                // NOTE: `0.9` is a double literal in the decomp, so the
                // multiply rounds in f64 before narrowing back to f32.
                slide_speed = (slide_speed as f64 * 0.9) as f32;
                if slide_speed < 4.0 {
                    slide_speed = 4.0;
                }

                let dyaw = cx.state.slide_yaw.0.wrapping_sub(wall_angle.0);
                cx.state.slide_yaw = Angle(
                    wall_angle
                        .0
                        .wrapping_sub(dyaw)
                        .wrapping_add(0x8000u16 as i16),
                );

                let sx = slide_speed * trig::sins(cx.state.slide_yaw);
                let sz = slide_speed * trig::coss(cx.state.slide_yaw);
                cx.state.vel.x = sx;
                cx.state.slide_vel_x = sx;
                cx.state.vel.z = sz;
                cx.state.slide_vel_z = sz;
                // (vel[1] is intentionally untouched, as in the decomp.)
            }
            // decomp: align_with_floor (body tilt, renderer-side) skipped.
            ActionResult::Stay
        }
    }
}

/// Slide variant with the actionTimer-gated A jump: while `action_timer ==
/// 5` an A press jumps via `jump_action`; every other frame increments the
/// timer. Then the 4.0 stop-speed slide, then [`common_slide_action`].
/// decomp: `common_slide_action_with_jump`
/// (src/game/mario_actions_moving.c:1414). Parameter order follows the
/// decomp (`stopAction, jumpAction, airAction`).
pub fn common_slide_action_with_jump(
    cx: &mut ActionCx,
    prev_buttons: u16,
    stop_action: ActionId,
    jump_action: ActionId,
    air_action: ActionId,
    anim_id: i32,
) -> ActionResult {
    if cx.state.action_timer == 5 {
        if cx.pressed(buttons::A, prev_buttons) {
            return super::entry::set_jumping_action(cx, jump_action, 0);
        }
    } else {
        cx.state.action_timer = cx.state.action_timer.wrapping_add(1);
    }

    if update_sliding(cx, 4.0) {
        return cx.goto(stop_action, 0);
    }

    // decomp returns void here; the wrapper always returns FALSE.
    let _ = common_slide_action(cx, prev_buttons, stop_action, air_action, anim_id);
    ActionResult::Stay
}

/// Stomach-slide shared logic: the actionTimer-gated A/B rollout (only
/// when not above a slide), then the 4.0 stop-speed slide.
/// decomp: `stomach_slide_action` (src/game/mario_actions_moving.c:1520).
fn stomach_slide_action(
    cx: &mut ActionCx,
    prev_buttons: u16,
    stop_action: ActionId,
    air_action: ActionId,
    anim_id: i32,
) -> ActionResult {
    if cx.state.action_timer == 5 {
        if cx.state.input & input_flags::ABOVE_SLIDE == 0
            && (cx.pressed(buttons::A, prev_buttons) || cx.pressed(buttons::B, prev_buttons))
        {
            // (rumble skipped)
            let rollout = if cx.state.forward_speed >= 0.0 {
                ActionId::FORWARD_ROLLOUT
            } else {
                ActionId::BACKWARD_ROLLOUT
            };
            return super::entry::drop_and_set_mario_action(cx, rollout, 0);
        }
    } else {
        cx.state.action_timer = cx.state.action_timer.wrapping_add(1);
    }

    if update_sliding(cx, 4.0) {
        return cx.goto(stop_action, 0);
    }

    let _ = common_slide_action(cx, prev_buttons, stop_action, air_action, anim_id);
    ActionResult::Stay
}

/// Slide dispatcher: remaps to butt or stomach slide by downhill facing.
/// In practice the remap already happened inside `set_mario_action_moving`
/// at transition time, so this handler only runs for direct transitions;
/// it re-dispatches the same way.
/// decomp: `set_mario_action_moving` (src/game/mario.c:896).
pub struct BeginSliding;
impl ActionHandler for BeginSliding {
    fn name(&self) -> &'static str {
        "BeginSliding"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        if let Some(r) = moving_prelude(cx) {
            return r;
        }
        let id = super::entry::set_mario_action_moving(cx, ActionId::BEGIN_SLIDING);
        cx.goto(id, 0)
    }
}

/// Butt slide: downhill slide with the timer-gated A jump.
/// decomp: `act_butt_slide` (src/game/mario_actions_moving.c:1432).
pub struct ButtSlide;
impl ActionHandler for ButtSlide {
    fn name(&self) -> &'static str {
        "ButtSlide"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = moving_prelude(cx) {
            return r;
        }
        let cancel = common_slide_action_with_jump(
            cx,
            prev_buttons,
            ActionId::BUTT_SLIDE_STOP,
            ActionId::JUMP,
            ActionId::BUTT_SLIDE_AIR,
            anim_id::MARIO_ANIM_SLIDE,
        );
        // decomp: tilt_body_butt_slide (body tilt, renderer-side) skipped.
        cancel
    }
}

/// Stomach slide: uphill/flat slide with the timer-gated A/B rollout.
/// decomp: `act_stomach_slide` (src/game/mario_actions_moving.c:1541).
pub struct StomachSlide;
impl ActionHandler for StomachSlide {
    fn name(&self) -> &'static str {
        "StomachSlide"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = moving_prelude(cx) {
            return r;
        }
        stomach_slide_action(
            cx,
            prev_buttons,
            ActionId::STOMACH_SLIDE_STOP,
            ActionId::FREEFALL,
            anim_id::MARIO_ANIM_SLIDE_DIVE,
        )
    }
}

/// Dive slide: the dive landing. A/B at any time rolls out; the slide ends
/// when the 8.0 stop speed is reached AND the dive animation finished.
/// decomp: `act_dive_slide` (src/game/mario_actions_moving.c:1557).
pub struct DiveSlide;
impl ActionHandler for DiveSlide {
    fn name(&self) -> &'static str {
        "DiveSlide"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = moving_prelude(cx) {
            return r;
        }
        if cx.state.input & input_flags::ABOVE_SLIDE == 0
            && (cx.pressed(buttons::A, prev_buttons) || cx.pressed(buttons::B, prev_buttons))
        {
            // (rumble skipped)
            // NOTE: strictly `> 0.0` here; the stomach slide uses `>= 0.0`.
            let rollout = if cx.state.forward_speed > 0.0 {
                ActionId::FORWARD_ROLLOUT
            } else {
                ActionId::BACKWARD_ROLLOUT
            };
            return cx.goto(rollout, 0);
        }

        // (play_mario_landing_sound_once skipped)

        if update_sliding(cx, 8.0) && cx.is_anim_at_end() {
            cx.set_forward_speed(0.0);
            // decomp: `set_mario_action`, return value discarded.
            let _ = cx.goto_next(ActionId::STOMACH_SLIDE_STOP, 0);
        }

        // Object grab: no objects in scope; the decomp's
        // mario_check_object_grab / mario_grab_used_object block is skipped.

        let _ = common_slide_action(
            cx,
            prev_buttons,
            ActionId::STOMACH_SLIDE_STOP,
            ActionId::FREEFALL,
            anim_id::MARIO_ANIM_DIVE,
        );
        ActionResult::Stay
    }
}

/// Crouch slide: Z-slide with the 30-frame long-jump / slide-kick window.
/// decomp: `act_crouch_slide` (src/game/mario_actions_moving.c:1452).
pub struct CrouchSlide;
impl ActionHandler for CrouchSlide {
    fn name(&self) -> &'static str {
        "CrouchSlide"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = moving_prelude(cx) {
            return r;
        }

        if cx.state.input & input_flags::ABOVE_SLIDE != 0 {
            return cx.goto(ActionId::BUTT_SLIDE, 0);
        }

        if cx.state.action_timer < 30 {
            cx.state.action_timer = cx.state.action_timer.wrapping_add(1);
            // NOTE: the decomp reads forwardVel (not slide speed) here.
            if cx.pressed(buttons::A, prev_buttons) && cx.state.forward_speed > 10.0 {
                // decomp: set_jumping_action (quicksand/steep routing), not
                // the raw entry.
                return super::entry::set_jumping_action(cx, ActionId::LONG_JUMP, 0);
            }
        }

        if cx.pressed(buttons::B, prev_buttons) {
            if cx.state.forward_speed >= 10.0 {
                // decomp: set_mario_action ACT_SLIDE_KICK case (mario.c:875)
                // sets vel.y=12 and forwardVel=max(32).
                let id = super::entry::set_mario_action_airborne(cx, ActionId::SLIDE_KICK, 0);
                return cx.goto(id, 0);
            } else {
                return cx.goto(ActionId::MOVE_PUNCHING, 0x0009);
            }
        }

        // Second A check: reachable while action_timer < 30 when the long
        // jump gate above did not fire (speed <= 10), and on every frame
        // once the 30-frame window has elapsed.
        if cx.pressed(buttons::A, prev_buttons) {
            return super::entry::set_jumping_action(cx, ActionId::JUMP, 0);
        }

        if cx.state.input & input_flags::FIRST_PERSON != 0 {
            return cx.goto(ActionId::BRAKING, 0);
        }

        common_slide_action_with_jump(
            cx,
            prev_buttons,
            ActionId::CROUCH,
            ActionId::JUMP,
            ActionId::FREEFALL,
            anim_id::MARIO_ANIM_START_CROUCHING,
        )
    }
}

/// Slide-kick slide: ground continuation of an airborne slide kick. A rolls
/// forward; a wall reflects into the backward ground knockback; the slide
/// ends (into the slide-kick-slide stop) when the kick animation finished
/// and speed dropped below 1.0.
/// decomp: `act_slide_kick_slide` (src/game/mario_actions_moving.c:1489).
pub struct SlideKickSlide;
impl ActionHandler for SlideKickSlide {
    fn name(&self) -> &'static str {
        "SlideKickSlide"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        if let Some(r) = moving_prelude(cx) {
            return r;
        }

        if cx.pressed(buttons::A, _prev_buttons) {
            // (rumble skipped)
            return super::entry::set_jumping_action(cx, ActionId::FORWARD_ROLLOUT, 0);
        }

        cx.set_mario_animation(anim_id::MARIO_ANIM_SLIDE_KICK);
        if cx.is_anim_at_end() && cx.state.forward_speed < 1.0 {
            return cx.goto(ActionId::SLIDE_KICK_SLIDE_STOP, 0);
        }

        // decomp: update_sliding's return value is discarded here; the stop
        // is handled by the anim-end check above.
        let _ = update_sliding(cx, 1.0);
        match crate::step::perform_ground_step(&mut cx.state, cx.world) {
            GroundStep::LeftGround => {
                // decomp: `set_mario_action(m, ACT_FREEFALL, 2)`, return
                // value discarded.
                let _ = cx.goto_next(ActionId::FREEFALL, 2);
            }
            GroundStep::HitWall => {
                crate::step::mario_bonk_reflection(&mut cx.state, true);
                // decomp: PARTICLE_VERTICAL_STAR skipped.
                // decomp: `set_mario_action(m, ACT_BACKWARD_GROUND_KB, 0)`,
                // return value discarded.
                let _ = cx.goto_next(ActionId::BACKWARD_GROUND_KB, 0);
            }
            GroundStep::None => {}
        }

        // decomp: slide sound + PARTICLE_DUST skipped.
        ActionResult::Stay
    }
}

/// Butt-slide stop: anim-driven stationary end.
/// decomp: `act_butt_slide_stop` (src/game/mario_actions_stationary.c:642).
pub struct ButtSlideStop;
impl ActionHandler for ButtSlideStop {
    fn name(&self) -> &'static str {
        "ButtSlideStop"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if cx.state.input & input_flags::STOMPED != 0 {
            return cx.goto(ActionId::SHOCKWAVE_BOUNCE, 0);
        }

        if cx.state.input
            & (input_flags::NONZERO_ANALOG
                | input_flags::A_PRESSED
                | input_flags::OFF_FLOOR
                | input_flags::ABOVE_SLIDE)
            != 0
        {
            if let Some(r) = super::entry::check_common_action_exits(cx, prev_buttons) {
                return r;
            }
        }

        stopping_step(cx, anim_id::MARIO_ANIM_STOP_SLIDE, ActionId::IDLE);
        // decomp: landing sound at anim frame 6 skipped.
        ActionResult::Stay
    }
}

/// Slide-kick-slide stop: anim-driven stationary end.
/// decomp: `act_slide_kick_slide_stop`
/// (src/game/mario_actions_stationary.c:680).
pub struct SlideKickSlideStop;
impl ActionHandler for SlideKickSlideStop {
    fn name(&self) -> &'static str {
        "SlideKickSlideStop"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        if cx.state.input & input_flags::STOMPED != 0 {
            return super::entry::drop_and_set_mario_action(cx, ActionId::SHOCKWAVE_BOUNCE, 0);
        }

        if cx.state.input & input_flags::OFF_FLOOR != 0 {
            return super::entry::drop_and_set_mario_action(cx, ActionId::FREEFALL, 0);
        }

        stopping_step(
            cx,
            anim_id::MARIO_ANIM_CROUCH_FROM_SLIDE_KICK,
            ActionId::CROUCH,
        );
        ActionResult::Stay
    }
}

/// Stomach slide stop: stationary end of the stomach slide. NOTE: in the
/// decomp this is an OBJECT-group action (`mario_actions_object.c:298`,
/// dispatched by `mario_execute_object_action`), not a stationary one —
/// the stationary switch has no case for it.
/// decomp: `act_stomach_slide_stop` (src/game/mario_actions_object.c:298).
pub struct StomachSlideStop;
impl ActionHandler for StomachSlideStop {
    fn name(&self) -> &'static str {
        "StomachSlideStop"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // Object-group preamble is held-object cancels (out of scope);
        // the quicksand update is shared with the other groups.
        if crate::step::mario_update_quicksand(&mut cx.state, 0.5) {
            return cx.goto(ActionId::QUICKSAND_DEATH, 0);
        }

        if cx.state.input & input_flags::STOMPED != 0 {
            return cx.goto(ActionId::SHOCKWAVE_BOUNCE, 0);
        }

        if cx.state.input & input_flags::OFF_FLOOR != 0 {
            return cx.goto(ActionId::FREEFALL, 0);
        }

        if cx.state.input & input_flags::ABOVE_SLIDE != 0 {
            let id = super::entry::set_mario_action_moving(cx, ActionId::BEGIN_SLIDING);
            return cx.goto(id, 0);
        }

        // decomp: `animated_stationary_ground_step(m,
        // MARIO_ANIM_SLOW_LAND_FROM_DIVE, ACT_IDLE)` — the set_mario_action
        // return is discarded, so IDLE first runs next tick.
        let _ = crate::step::stationary_ground_step(&mut cx.state, cx.world);
        cx.set_mario_animation(anim_id::MARIO_ANIM_SLOW_LAND_FROM_DIVE);
        if cx.is_anim_at_end() {
            let _ = cx.goto_next(ActionId::IDLE, 0);
        }
        ActionResult::Stay
    }
}
