//! Air actions: SingleJump, DoubleJump, TripleJump, Backflip, SideFlip,
//! LongJump, SteepJump, Dive, SlideKick, GroundPound, WallKickAir,
//! AirHitWall, SoftBonk, BackwardAirKb/ForwardAirKb (+hard variants),
//! Freefall, LedgeGrab.
//!
//! Entry numbers from the community wiki (see spec/actions/*.md):
//! Double Jump vy=52+hspeed/4 (wiki:Double Jump@18964), Triple vy=69
//! (wiki:Triple Jump@19300), Backflip fwd=-16/vy=62 (wiki:Backflip@19307),
//! SideFlip vy=62/fwd=8/facing=intended (wiki:Side Flip@20374),
//! Dive +15 horizontal clamped to 48 and +20 vy from ground
//! (wiki:Dive@19303). Long jump, wall-kick flight, steep jump, and
//! knockback numbers are decomp-derived (behavioral reference).

use super::{ActionCx, ActionHandler, ActionResult};
use crate::angles::Angle;
use crate::anim_table as anim_id;
use crate::events::Event;
use crate::input::buttons;
use crate::state::{mario_flags, ActionId};
use crate::step::{perform_air_step, AirStep, AIR_STEP_CHECK_LEDGE_GRAB};
use crate::world::{CELL_HEIGHT_LIMIT, FLOOR_LOWER_LIMIT};
use glam::Vec3;

/// Animation slot ids for air actions.
pub mod slot {
    pub const JUMP: u32 = 6;
    pub const FREEFALL: u32 = 7;
    pub const DOUBLE_JUMP: u32 = 8;
    pub const TRIPLE_JUMP: u32 = 9;
    pub const BACKFLIP: u32 = 10;
    pub const SIDE_FLIP: u32 = 11;
    pub const LONG_JUMP: u32 = 12;
    pub const DIVE: u32 = 13;
    pub const GROUND_POUND: u32 = 14;
    pub const WALL_KICK: u32 = 17;
    pub const LEDGE_GRAB: u32 = 18;
    pub const AIR_KNOCKBACK: u32 = 20;
    pub const SLIDE_KICK: u32 = 28;
    pub const ROLLOUT: u32 = 29;
    pub const STEEP_JUMP: u32 = 30;
    pub const AIR_HIT_WALL: u32 = 31;
    pub const SOFT_BONK: u32 = 32;
    pub const JUMP_KICK: u32 = 37;
    pub const WATER_JUMP: u32 = 42;
}

/// Per-action air behavior switches.
struct AirConfig {
    steer_rate: u16,
    air_drag_threshold: f32,
    allow_dive: bool,
    allow_pound: bool,
    /// Whether this action may ledge-grab (decomp: `AIR_STEP_CHECK_LEDGE_GRAB`
    /// in the action's `stepArg`).
    can_ledge_grab: bool,
    /// When true, wall hits slide along the wall (forward speed reset to
    /// 15) instead of routing to the air-hit-wall state. Used by the water
    /// jump. (verified: decomp-derived)
    wall_slide: bool,
    /// Whether the shared cancel set (water plunge, wall kick, dive,
    /// pound) runs. Disabled for the dead fall (no input response).
    allow_cancels: bool,
    /// Animation id requested every frame via `common_air_action_step`
    /// (decomp: `mario_actions_airborne.c:371`). `-1` = do not request
    /// (placeholder until the action's anim id is ported).
    anim: i32,
    /// Animation id requested on the AIR_STEP_HIT_WALL arm instead of
    /// `anim`. `None` = reuse `anim`. Needed by the knockbacks: decomp
    /// `common_air_knockback_step` requests the action's animation on
    /// AIR_STEP_NONE but MARIO_ANIM_BACKWARD_AIR_KB on AIR_STEP_HIT_WALL
    /// (airborne.c:1097).
    anim_wall: Option<i32>,
}


/// Mark the start of a jump (decomp: `set_jumping_action` sets
/// `MARIO_UNKNOWN_08`, which arms the jump-ascent gravity strengthening).
fn mark_jump(cx: &mut ActionCx) {
    // decomp: set_jumping_action sets peakHeight and MARIO_UNKNOWN_08 (mario.c:887).
    cx.state.peak_height = cx.state.pos.y;
    cx.state.flags |= mario_flags::UNKNOWN_08;
}

/// Set the sound-played flags after a transition (decomp: the action's
/// play_mario_sound runs after set_mario_action clears them).
fn mark_jump_sound(cx: &mut ActionCx) {
    cx.state.flags |= mario_flags::ACTION_SOUND_PLAYED | mario_flags::MARIO_SOUND_PLAYED;
}

/// Shared air cancels: B -> dive (speed > 28), Z -> ground pound.
/// Returns `Some` when a transition was taken.
fn air_cancels(cx: &mut ActionCx, prev_buttons: u16, cfg: &AirConfig) -> Option<ActionResult> {
    // Water plunge: more than 100 below the surface.
    // (verified: wiki:Water Plunge, rev 2023-07-27)
    if let Some(wl) = crate::actions::water::water_plunge_surface(cx) {
        return Some(crate::actions::water::enter_water_plunge(cx, wl));
    }
    // Wall kick: A while the kick window from a recent wall hit is open.
    // (ID verified: wiki:Wall Kick rev 19311; numbers (verify))
    // NOTE: not when already wall-kicking (decomp: act_wall_kick_air has no
    // A check; airborne.c:618); otherwise the held A re-triggers the kick
    // and the action loop cycles.
    if cx.state.action != ActionId::WALL_KICK_AIR
        && cx.pressed(buttons::A, prev_buttons)
        && cx.state.wall_kick_timer > 0
    {
        let n = cx.state.wall.map(|w| w.normal).unwrap_or(glam::Vec3::X);
        return Some(enter_wall_kick(cx, n));
    }
    if cfg.allow_dive && cx.pressed(buttons::B, prev_buttons) {
        // spec: dive.air_speed_threshold (verified: wiki:Dive@19303)
        if cx.state.forward_speed > 28.0 {
            return Some(enter_dive(cx, false));
        }
        // Slow air: a jump kick instead of a dive.
        // spec: combat.jump_kick_speed_threshold (decomp-derived)
        return Some(enter_jump_kick(cx));
    }
    if cfg.allow_pound && cx.pressed(buttons::Z, prev_buttons) {
        return Some(enter_ground_pound(cx));
    }
    None
}

/// Shared air physics + step. Returns `Some` on landing (or a cancel).
fn air_common(cx: &mut ActionCx, prev_buttons: u16, cfg: &AirConfig) -> Option<ActionResult> {
    if cfg.allow_cancels {
        if let Some(r) = air_cancels(cx, prev_buttons, cfg) {
            return Some(r);
        }
    }
    let p = cfg;
    // Air control: preserve momentum, apply accelerations additively.
    // (Behavioral: new vel = existing momentum + fwd accel + side accel + drag.)
    let (fx, fz) = cx.forward_xz();
    // Decompose existing velocity into forward and sideways components.
    let mut fwd = cx.state.vel.x * fx + cx.state.vel.z * fz;
    let mut side = cx.state.vel.x * fz - cx.state.vel.z * fx;
    // Forward drag toward zero.
    if fwd > 0.0 {
        fwd = (fwd - 0.35).max(0.0);
    } else if fwd < 0.0 {
        fwd = (fwd + 0.35).min(0.0);
    }
    // Sideways drag (weaker).
    side *= 0.95;
    if let Some(iy) = cx.intended_yaw() {
        let mag = cx.intended_magnitude() / 32.0;
        if mag > 0.01 {
            let dyaw = Angle(iy.diff_to(cx.state.face_yaw) as i16);
            // Forward accel: 1.5 * cos(dYaw) * mag. Side accel: 10.0 * sin(dYaw) * mag.
            fwd += mag * crate::trig::coss(dyaw) * 1.5;
            side += mag * crate::trig::sins(dyaw) * 10.0;
            // Turn facing independently (does not rotate velocity).
            // Multiplier comes from the action config: 512 for the
            // with-turn model, 0 to disable (knockbacks, dives).
            let turn = Angle((p.steer_rate as f32 * crate::trig::sins(dyaw) * mag) as i16);
            cx.state.face_yaw = cx.state.face_yaw.wrapping_add(turn);
        }
    }
    // High-speed drag and backward recovery.
    let drag_threshold = p.air_drag_threshold;
    if fwd > drag_threshold {
        fwd -= 1.0;
    }
    if fwd < -16.0 {
        fwd += 2.0;
    }
    // Recompose velocity from components (facing may have changed).
    let (nfx, nfz) = cx.forward_xz();
    cx.state.vel.x = nfx * fwd + nfz * side;
    cx.state.vel.z = nfz * fwd - nfx * side;
    cx.state.forward_speed = fwd;
    // Step (ST-04): the ported perform_air_step moves, collides, applies
    // gravity (ST-07), and updates peakHeight internally. The ledge grab
    // now happens inside the step (ST-08 check_ledge_grab).
    let step_arg = if cfg.can_ledge_grab {
        AIR_STEP_CHECK_LEDGE_GRAB
    } else {
        0
    };
    // AIR_STEP_CHECK_HANG is not passed: START_HANGING is not implemented.
    let vy_entry = cx.state.vel.y;
    let step_result = perform_air_step(&mut cx.state, cx.world, step_arg);
    match step_result {
        AirStep::Landed => {
            let fall_speed = (-vy_entry).max(0.0);
            // Slide kick: the first touchdown bounces at half the impact speed
            // (tracked via action_state); the second becomes a ground slide.
            // decomp: the bounce also resets actionTimer to 0 (airborne.c:1580).
            if cx.state.action == ActionId::SLIDE_KICK && cx.state.action_state == 0 {
                cx.state.action_state = 1;
                cx.state.action_timer = 0;
                cx.state.vel.y = (fall_speed * 0.5).max(8.0);
                cx.events.push(Event::Landed { fall_speed });
            } else {
                cx.events.push(Event::Landed { fall_speed });
                if let Some(f) = cx.state.floor {
                    cx.state.floor_kind = super::ground::surface_kind_index(f.kind);
                }
                cx.state.land_from = cx.state.action;
                cx.timeline.slot = super::ground::slot::LAND;
                return Some(enter_landing_for(cx, fall_speed));
            }
        }
        AirStep::HitWall | AirStep::HitLavaWall => {
            // decomp: common_air_action_step requests the action's
            // animation before the wall handling (airborne.c:371); the
            // knockbacks request MARIO_ANIM_BACKWARD_AIR_KB here instead
            // (common_air_knockback_step, airborne.c:1097).
            let wall_anim = cfg.anim_wall.unwrap_or(cfg.anim);
            if wall_anim >= 0 {
                cx.set_mario_animation(wall_anim);
            }
            // (No lava boost yet; HitLavaWall is handled as a wall hit.)
            // Note: HIT_WALL from the OOB/ceiling branches has no referenced
            // wall (the decomp's m->wall is NULL there too).
            let n = cx.state.wall.map(|w| w.normal).unwrap_or_else(|| {
                let (fx, fz) = cx.forward_xz();
                Vec3::new(-fx, 0.0, -fz)
            });
            cx.events.push(Event::WallHit {
                normal_yaw: crate::trig::atan2s(n.z, n.x),
            });
            cx.state.wall_kick_timer = 5; // spec: wall.kick_window (verified: decomp act_air_hit_wall)
            // Speed-dependent wall response.
            let speed = (cx.state.vel.x * cx.state.vel.x + cx.state.vel.z * cx.state.vel.z).sqrt();
            // Dive into a wall -> bonk -> backwards air knockback at any speed.
            // (wiki:Dive@19303; numbers (verify))
            if cx.state.action == ActionId::DIVE {
                return Some(enter_backward_air_kb(cx, n, false));
            }
            // Slide kick and rollouts stop dead on walls.
            let stopped_dead = matches!(
                cx.state.action,
                ActionId::SLIDE_KICK | ActionId::FORWARD_ROLLOUT | ActionId::BACKWARD_ROLLOUT
            );
            if stopped_dead {
                cx.state.vel.x = 0.0;
                cx.state.vel.z = 0.0;
                cx.state.forward_speed = 0.0;
            }
            if cfg.wall_slide {
                // Water jump: slide along the wall instead of bonking; forward
                // speed resets to 15. (verified: decomp-derived)
                // spec: swim.wall_slide_speed
                let (fx, fz) = cx.forward_xz();
                cx.state.vel.x = fx * 15.0;
                cx.state.vel.z = fz * 15.0;
                let dot = cx.state.vel.x * n.x + cx.state.vel.z * n.z;
                if dot < 0.0 {
                    cx.state.vel.x -= dot * n.x;
                    cx.state.vel.z -= dot * n.z;
                }
                cx.state.forward_speed = 15.0;
            } else if !stopped_dead {
                if speed > 16.0 {
                    // Solid hit -> the transient air-hit-wall state: a 2-frame
                    // wall-kick window, then knockback or soft bonk by speed.
                    return Some(enter_air_hit_wall(cx, n));
                }
                // Graze (speed <= 16): no bonk, no action change, no dampening.
                // Remove just the into-wall component so the character slides
                // along the wall instead of penetrating it.
                let dot = cx.state.vel.x * n.x + cx.state.vel.z * n.z;
                if dot < 0.0 {
                    cx.state.vel.x -= dot * n.x;
                    cx.state.vel.z -= dot * n.z;
                }
                let (fx, fz) = cx.forward_xz();
                cx.state.forward_speed = cx.state.vel.x * fx + cx.state.vel.z * fz;
            }
        }
        AirStep::GrabbedLedge => {
            // decomp: common_air_action_step requests MARIO_ANIM_IDLE_ON_LEDGE
            // before the ledge-grab transition (airborne.c:371). The
            // decomp's drop_and_set_mario_action return is discarded, so the
            // grab runs next-tick.
            cx.set_mario_animation(anim_id::MARIO_ANIM_IDLE_ON_LEDGE);
            // The decomp's act_* returns FALSE after the grab, so LEDGE_GRAB
            // first runs on the next tick.
            return Some(cx.goto_next(ActionId::LEDGE_GRAB, 0));
        }
        AirStep::GrabbedCeiling | AirStep::None => {
            // decomp: common_air_action_step requests the action's animation
            // on AIR_STEP_NONE (airborne.c:371). GRABBED_CEILING is serviced
            // by the hang action on the next tick.
            if step_result == AirStep::None && cfg.anim >= 0 {
                cx.set_mario_animation(cfg.anim);
            }
        }
    }
    None
}

/// Apply damage and emit the event.
fn apply_damage(cx: &mut ActionCx, amount: i32) {
    cx.state.health -= amount;
    cx.events.push(Event::Damaged { amount });
}

/// Death simplification: freefall with arg 2 = "dead fall" (no input
/// response). The landing routes to idle, where input stays ignored while
/// health < 0x100. Full death warp is out of scope.
fn enter_dead_fall(cx: &mut ActionCx) -> ActionResult {
    cx.timeline.slot = slot::FREEFALL;
    cx.goto(ActionId::FREEFALL, 2)
}

/// Fall damage + squish, assessed on landing when the impact speed exceeds
/// 55 (vy < -55). Returns `Some` when the landing is replaced by a damage
/// outcome. (decomp-derived fall-damage model)
///
/// - fall height > 3000: 16 damage + hard-knockback landing (the hard
///   backward air knockback for now; Phase E routes its landing to the
///   hard ground knockback).
/// - fall height > 1150 on a non-slippery floor (Default/NotSlippery
///   class): 8 damage + 30 frames of squish.
/// - health < 0x100 after damage: dead fall.
fn check_fall_damage(cx: &mut ActionCx, fall_speed: f32) -> Option<ActionResult> {
    if fall_speed <= 55.0 {
        return None;
    }
    let fall_height = cx.state.peak_height - cx.state.pos.y;
    if fall_height > 3000.0 {
        apply_damage(cx, 16);
        if cx.state.health < 0x100 {
            return Some(enter_dead_fall(cx));
        }
        // Hard-knockback landing: hard backward air knockback, arg 1 =
        // hard (Phase E consumes the flag).
        cx.state.forward_speed = -15.0;
        set_air_velocity(cx, 40.0);
        cx.timeline.slot = slot::AIR_KNOCKBACK;
        return Some(cx.goto(ActionId::HARD_BACKWARD_AIR_KB, 1));
    }
    if fall_height > 1150.0 {
        // Non-slippery floor (Default/NotSlippery class): damage + squish.
        // Queried live: the ground-pound landing path doesn't refresh
        // floor_kind before this runs.
        let non_slippery = cx.world.find_floor(cx.state.pos.x, cx.state.pos.y, cx.state.pos.z).is_some_and(|f| {
            matches!(
                crate::world::SurfaceClass::of(f.kind),
                crate::world::SurfaceClass::Default | crate::world::SurfaceClass::NotSlippery
            )
        });
        if non_slippery {
            apply_damage(cx, 8);
            if cx.state.health < 0x100 {
                return Some(enter_dead_fall(cx));
            }
            cx.state.squish_timer = 30;
        }
    }
    None
}

/// Route a touchdown to the per-action landing (or slide) matching
/// `land_from`. Fall damage is assessed first; the impact speed travels as
/// the goto arg so the landing can record it.
fn enter_landing_for(cx: &mut ActionCx, fall_speed: f32) -> ActionResult {
    if let Some(r) = check_fall_damage(cx, fall_speed) {
            return r;
    }
    // Dead: collapse to idle (input stays ignored while health < 0x100).
    if cx.state.health < 0x100 {
        return cx.goto_next(ActionId::IDLE, 2);
    }
    // Deep quicksand landing: the escape sequence.
    if cx.state.quicksand_depth >= 11.0 {
        return cx.goto_next(ActionId::QUICKSAND_JUMP_LAND, 0);
    }
    // decomp: the landing arg is 0 (not fall_speed); fall damage was
    // already assessed via check_fall_damage above.
    match cx.state.land_from {
        ActionId::JUMP => cx.goto_next(ActionId::JUMP_LAND, 0),
        ActionId::DOUBLE_JUMP => cx.goto_next(ActionId::DOUBLE_JUMP_LAND, 0),
        ActionId::TRIPLE_JUMP => cx.goto_next(ActionId::TRIPLE_JUMP_LAND, 0),
        ActionId::BACKFLIP => cx.goto_next(ActionId::BACKFLIP_LAND, 0),
        ActionId::SIDE_FLIP => cx.goto_next(ActionId::SIDE_FLIP_LAND, 0),
        ActionId::LONG_JUMP => cx.goto_next(ActionId::LONG_JUMP_LAND, 0),
        ActionId::DIVE => super::ground::enter_dive_slide(cx),
        ActionId::SLIDE_KICK => super::ground::enter_slide_kick_slide(cx),
        ActionId::WALL_KICK_AIR => cx.goto_next(ActionId::JUMP_LAND, 0),
        ActionId::WATER_JUMP => cx.goto_next(ActionId::JUMP_LAND, 0),
        ActionId::STEEP_JUMP => {
            // Still moving backward along facing -> slide; else jump-land.
            if cx.state.forward_speed < 0.0 {
                super::ground::begin_sliding(cx)
            } else {
                cx.goto_next(ActionId::JUMP_LAND, 0)
            }
        }
        // Air knockbacks land into the matching ground knockbacks. The
        // hard variants are separate action ids (the hard flag travels as
        // the id, with arg 1 as well).
        ActionId::BACKWARD_AIR_KB => {
            super::ground::enter_ground_kb(cx, ActionId::BACKWARD_GROUND_KB)
        }
        ActionId::HARD_BACKWARD_AIR_KB => {
            super::ground::enter_ground_kb(cx, ActionId::HARD_BACKWARD_GROUND_KB)
        }
        ActionId::FORWARD_AIR_KB => super::ground::enter_ground_kb(cx, ActionId::FORWARD_GROUND_KB),
        ActionId::HARD_FORWARD_AIR_KB => {
            super::ground::enter_ground_kb(cx, ActionId::HARD_FORWARD_GROUND_KB)
        }
        // Soft bonk: soft ground knockback by the sign of forward speed.
        ActionId::SOFT_BONK => {
            let id = if cx.state.forward_speed < 0.0 {
                ActionId::SOFT_BACKWARD_GROUND_KB
            } else {
                ActionId::SOFT_FORWARD_GROUND_KB
            };
            super::ground::enter_ground_kb(cx, id)
        }
        // Jump kick: plain freefall landing.
        ActionId::JUMP_KICK => cx.goto_next(ActionId::FREEFALL_LAND, 0),
        _ => cx.goto_next(ActionId::FREEFALL_LAND, 0),
    }
}

fn base_air_config() -> AirConfig {
    AirConfig {
        steer_rate: 512, // with-turn yaw multiplier (decomp: 512*sin(dyaw)*mag)
        air_drag_threshold: 32.0,
        allow_dive: true,
        allow_pound: true,
        can_ledge_grab: true,
        allow_cancels: true,
        wall_slide: false,
        anim: -1,
        anim_wall: None,
    }
}

/// Jump entry velocity: mirrors the decomp's `set_mario_y_vel_based_on_fspeed`
/// (mario.c:763) — the computed vy is halved ONCE when *either*
/// `squishTimer != 0` or `quicksandDepth > 1.0`, not once per condition.
pub(crate) fn adjusted_jump_vy(cx: &ActionCx, vy: f32) -> f32 {
    let mut v = vy;
    if cx.state.squish_timer != 0 || cx.state.quicksand_depth > 1.0 {
        v *= 0.5;
    }
    v
}

/// Shared jump entry: vertical speed = 42 + forward_speed/4, forward *= 0.8.
/// (wiki:Single Jump@19309)
pub fn enter_jump(cx: &mut ActionCx) -> ActionResult {
    mark_jump(cx);
    let p = cx.params;
    let vy = adjusted_jump_vy(
        cx,
        p.jump_vertical_base + cx.state.forward_speed * p.jump_vertical_forward_factor,
    );
    cx.state.forward_speed *= p.jump_forward_retain;
    set_air_velocity(cx, vy);
    cx.timeline.slot = slot::JUMP;
    cx.events.push(Event::Jumped { velocity_y: vy });
    let r = cx.goto(ActionId::JUMP, 0);
    // decomp: act_jump runs same-frame and its play_mario_sound sets these.
    mark_jump_sound(cx);
    r
}

/// Double jump entry: vy = 52 + hspeed/4, forward *= 0.8.
/// (wiki:Double Jump@18964)
pub fn enter_double_jump(cx: &mut ActionCx) -> ActionResult {
    mark_jump(cx);
    // spec: jump.double_vertical_base (verified: wiki:Double Jump@18964)
    let vy = adjusted_jump_vy(cx, 52.0 + cx.state.forward_speed * 0.25);
    cx.state.forward_speed *= 0.8;
    set_air_velocity(cx, vy);
    cx.timeline.slot = slot::DOUBLE_JUMP;
    cx.events.push(Event::Jumped { velocity_y: vy });
    cx.goto(ActionId::DOUBLE_JUMP, 0)
}

/// Triple jump entry: vy = 69 fixed, forward *= 0.8, no height control.
/// (wiki:Triple Jump@19300)
pub fn enter_triple_jump(cx: &mut ActionCx) -> ActionResult {
    mark_jump(cx);
    // spec: jump.triple_vertical (verified: wiki:Triple Jump@19300)
    let vy = adjusted_jump_vy(cx, 69.0);
    cx.state.forward_speed *= 0.8;
    set_air_velocity(cx, vy);
    cx.timeline.slot = slot::TRIPLE_JUMP;
    cx.events.push(Event::Jumped { velocity_y: vy });
    cx.goto(ActionId::TRIPLE_JUMP, 0)
}

/// Backflip entry: forward = -16, vy = 62. (wiki:Backflip@19307)
pub fn enter_backflip(cx: &mut ActionCx) -> ActionResult {
    mark_jump(cx);
    // spec: jump.backflip_forward / jump.backflip_vertical (verified)
    cx.state.forward_speed = -16.0;
    let vy = adjusted_jump_vy(cx, 62.0);
    set_air_velocity(cx, vy);
    cx.timeline.slot = slot::BACKFLIP;
    cx.events.push(Event::Jumped { velocity_y: vy });
    cx.goto(ActionId::BACKFLIP, 0)
}

/// Side flip entry: vy = 62, forward = 8, facing = intended yaw.
/// (wiki:Side Flip@20374)
pub fn enter_side_flip(cx: &mut ActionCx) -> ActionResult {
    mark_jump(cx);
    // spec: jump.sideflip_vertical / jump.sideflip_forward (verified)
    if let Some(iy) = cx.intended_yaw() {
        cx.state.face_yaw = iy;
    }
    cx.state.forward_speed = 8.0;
    let vy = adjusted_jump_vy(cx, 62.0);
    set_air_velocity(cx, vy);
    cx.timeline.slot = slot::SIDE_FLIP;
    cx.events.push(Event::Jumped { velocity_y: vy });
    cx.goto(ActionId::SIDE_FLIP, 0)
}

/// Long jump entry: vy = 30, forward speed x1.5 capped at 48.
/// (decomp-derived; gravity -2 documented: wiki:Gravity@20294)
pub fn enter_long_jump(cx: &mut ActionCx) -> ActionResult {
    mark_jump(cx);
    // spec: jump.longjump_vertical / jump.longjump_forward_scale
    let vy = adjusted_jump_vy(cx, 30.0);
    cx.state.forward_speed = (cx.state.forward_speed * 1.5).min(48.0);
    set_air_velocity(cx, vy);
    cx.timeline.slot = slot::LONG_JUMP;
    cx.events.push(Event::Jumped { velocity_y: vy });
    cx.goto(ActionId::LONG_JUMP, 0)
}

/// Dive entry: +15 horizontal (clamped to 48); +20 vy when from the ground.
/// (wiki:Dive@19303)
pub fn enter_dive(cx: &mut ActionCx, from_ground: bool) -> ActionResult {
    mark_jump(cx);
    // spec: dive.horizontal_gain / dive.vertical_gain_ground (verified)
    cx.state.forward_speed = (cx.state.forward_speed + cx.params.dive_horizontal_gain).min(48.0);
    let vy = if from_ground { 20.0 } else { cx.state.vel.y };
    set_air_velocity(cx, vy);
    cx.timeline.slot = slot::DIVE;
    cx.events.push(Event::Jumped { velocity_y: vy });
    cx.goto(ActionId::DIVE, 0)
}

/// Ground pound entry: kill momentum. The rise phase needs 320 units of
/// headroom; without it the pound skips straight to the slam phase.
pub fn enter_ground_pound(cx: &mut ActionCx) -> ActionResult {
    cx.state.forward_speed = 0.0;
    cx.state.vel = Vec3::ZERO;
    cx.timeline.slot = slot::GROUND_POUND;
    cx.goto(ActionId::GROUND_POUND, 0)
}

fn set_air_velocity(cx: &mut ActionCx, vy: f32) {
    let (fx, fz) = cx.forward_xz();
    cx.state.vel = Vec3::new(fx * cx.state.forward_speed, vy, fz * cx.state.forward_speed);
}

macro_rules! air_action {
    ($name:ident, $label:literal, $cx:ident, $cfg:expr, $slot:expr) => {
        pub struct $name;
        impl ActionHandler for $name {
            fn name(&self) -> &'static str {
                $label
            }
            fn tick(&self, $cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
                if let Some(r) = air_common($cx, prev_buttons, &$cfg) {
                    return r;
                }
                $cx.timeline.slot = $slot;
                ActionResult::Stay
            }
        }
    };
}

air_action!(
    SingleJump,
    "SingleJump",
    cx,
    AirConfig {
        anim: anim_id::MARIO_ANIM_SINGLE_JUMP,
        ..base_air_config()
    },
    slot::JUMP
);
air_action!(
    DoubleJump,
    "DoubleJump",
    cx,
    AirConfig {
        // decomp: act_double_jump picks the anim by vel[1] >= 0 BEFORE the
        // step (airborne.c:461).
        anim: if cx.state.vel.y >= 0.0 {
            anim_id::MARIO_ANIM_DOUBLE_JUMP_RISE
        } else {
            anim_id::MARIO_ANIM_DOUBLE_JUMP_FALL
        },
        ..base_air_config()
    },
    slot::DOUBLE_JUMP
);
air_action!(
    TripleJump,
    "TripleJump",
    cx,
    AirConfig {
        // decomp: act_triple_jump requests MARIO_ANIM_TRIPLE_JUMP via
        // common_air_action_step (airborne.c:480).
        anim: anim_id::MARIO_ANIM_TRIPLE_JUMP,
        can_ledge_grab: false, // documented: no ledge grab or hang
        ..base_air_config()
    },
    slot::TRIPLE_JUMP
);
air_action!(
    Backflip,
    "Backflip",
    cx,
    AirConfig {
        // decomp: act_backflip requests MARIO_ANIM_BACKFLIP via
        // common_air_action_step (airborne.c:509).
        anim: anim_id::MARIO_ANIM_BACKFLIP,
        allow_dive: false,
        can_ledge_grab: false, // documented: no ledge grab or hang
        ..base_air_config()
    },
    slot::BACKFLIP
);
air_action!(
    SideFlip,
    "SideFlip",
    cx,
    AirConfig {
        // decomp: act_side_flip requests MARIO_ANIM_SLIDEFLIP via
        // common_air_action_step (airborne.c:595).
        anim: anim_id::MARIO_ANIM_SLIDEFLIP,
        ..base_air_config()
    },
    slot::SIDE_FLIP
);
/// LongJump: gravity comes from params (spec: physics.longjump_gravity).
pub struct LongJump;
impl ActionHandler for LongJump {
    fn name(&self) -> &'static str {
        "LongJump"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        // decomp: act_long_jump latches actionState on vertical-wind floors
        // (airborne.c:632); the latch only gates a sound, nothing reads it.
        // The oMarioLongJumpIsSlow fast/slow anim pick is not modeled yet
        // (Phase 5-7), so the fast variant is used.
        if cx.state.action_state == 0
            && cx
                .state
                .floor
                .is_some_and(|f| f.surface_type == crate::world::surface_type::VERTICAL_WIND)
        {
            cx.state.action_state = 1;
        }
        // Gravity -2/terminal -75 comes from apply_gravity's action chain (ST-07).
        let cfg = AirConfig {
            anim: anim_id::MARIO_ANIM_FAST_LONGJUMP,
            ..base_air_config()
        };
        if let Some(r) = air_common(cx, prev_buttons, &cfg) {
            return r;
        }
        cx.timeline.slot = slot::LONG_JUMP;
        ActionResult::Stay
    }
}
air_action!(
    Dive,
    "Dive",
    cx,
    AirConfig {
        // decomp: act_dive requests MARIO_ANIM_DIVE unconditionally at the
        // top, before the step (airborne.c:725); air_common covers the
        // NONE and HIT_WALL arms.
        anim: anim_id::MARIO_ANIM_DIVE,
        steer_rate: 0,         // committed (verify)
        air_drag_threshold: 32.0,
        allow_dive: false,
        allow_pound: false,
        ..base_air_config()
    },
    slot::DIVE
);

/// Slide kick entry: vy = 12, forward speed raised to at least 32.
pub fn enter_slide_kick(cx: &mut ActionCx) -> ActionResult {
    mark_jump(cx);
    cx.state.forward_speed = cx.state.forward_speed.max(32.0);
    set_air_velocity(cx, 12.0);
    cx.timeline.slot = slot::SLIDE_KICK;
    cx.events.push(Event::Jumped { velocity_y: 12.0 });
    cx.goto(ActionId::SLIDE_KICK, 0)
}

/// Forward rollout entry: vy = 30, horizontal speed kept.
pub fn enter_forward_rollout(cx: &mut ActionCx) -> ActionResult {
    set_air_velocity(cx, 30.0);
    cx.timeline.slot = slot::ROLLOUT;
    cx.goto(ActionId::FORWARD_ROLLOUT, 0)
}

/// Backward rollout entry: vy = 30, horizontal speed kept.
pub fn enter_backward_rollout(cx: &mut ActionCx) -> ActionResult {
    set_air_velocity(cx, 30.0);
    cx.timeline.slot = slot::ROLLOUT;
    cx.goto(ActionId::BACKWARD_ROLLOUT, 0)
}

/// Jump kick entry: vy = 20, horizontal velocity kept. Hitbox frames 0-7
/// are documented only: no enemies exist in the sim, so no hitboxes are
/// simulated.
pub fn enter_jump_kick(cx: &mut ActionCx) -> ActionResult {
    mark_jump(cx);
    // spec: combat.jump_kick_vy (decomp-derived)
    set_air_velocity(cx, 20.0);
    cx.timeline.slot = slot::JUMP_KICK;
    cx.events.push(Event::Jumped { velocity_y: 20.0 });
    cx.goto(ActionId::JUMP_KICK, 0)
}

/// Slide kick, airborne: after 30 frames and more than 500 above the
/// floor it becomes a freefall; the first touchdown bounces at half
/// impact speed (see air_common), the second becomes a ground slide.
pub struct SlideKick;
impl ActionHandler for SlideKick {
    fn name(&self) -> &'static str {
        "SlideKick"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        // decomp: act_slide_kick requests MARIO_ANIM_SLIDE_KICK once, on the
        // entry frame (actionState 0, actionTimer 0), before the timer
        // increment (airborne.c:1580).
        if cx.state.action_state == 0 && cx.state.action_timer == 0 {
            cx.set_mario_animation(anim_id::MARIO_ANIM_SLIDE_KICK);
        }
        // decomp: `if (++(m->actionTimer) > 30 && ...)` — pre-increment.
        cx.state.action_timer += 1;
        if cx.state.action_timer > 30 {
            let high = cx
                .world
                .find_floor(cx.state.pos.x, cx.state.pos.y, cx.state.pos.z)
                .is_none_or(|f| cx.state.pos.y - f.y > 500.0);
            if high {
                cx.timeline.slot = slot::FREEFALL;
                // decomp: ACT_FREEFALL with arg 2 (the slide-kick fall anim).
                return cx.goto(ActionId::FREEFALL, 2);
            }
        }
        if let Some(r) = air_common(cx, prev_buttons, &base_air_config()) {
            return r;
        }
        cx.timeline.slot = slot::SLIDE_KICK;
        ActionResult::Stay
    }
}

/// Forward rollout: decomp act_forward_rollout (airborne.c:1353). The first
/// tick raises actionState 0 -> 1 (the entry already set vy = 30); the anim
/// is FORWARD_SPINNING while state 1, GENERAL_FALL once the spin ends
/// (state 2, via is_anim_past_end). The anim is requested through
/// air_common's NONE/HIT_WALL arms; re-requesting the same id on a wall hit
/// is a no-op, matching the decomp (which requests nothing on HIT_WALL).
pub struct ForwardRollout;
impl ActionHandler for ForwardRollout {
    fn name(&self) -> &'static str {
        "ForwardRollout"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if cx.state.action_state == 0 {
            cx.state.action_state = 1;
        }
        let cfg = AirConfig {
            anim: if cx.state.action_state == 1 {
                anim_id::MARIO_ANIM_FORWARD_SPINNING
            } else {
                anim_id::MARIO_ANIM_GENERAL_FALL
            },
            ..base_air_config()
        };
        if let Some(r) = air_common(cx, prev_buttons, &cfg) {
            return r;
        }
        // decomp: `if (m->actionState == 1 && is_anim_past_end(m)) m->actionState = 2;`
        if cx.state.action_state == 1 && cx.is_anim_past_end() {
            cx.state.action_state = 2;
        }
        cx.timeline.slot = slot::ROLLOUT;
        ActionResult::Stay
    }
}

/// Backward rollout: decomp act_backward_rollout (airborne.c:1393). Same
/// shape as the forward rollout, but the spin is BACKWARD_SPINNING and the
/// state advance fires when the anim frame reaches 2 (a raw animFrame
/// comparison in the decomp, not is_anim_past_frame).
pub struct BackwardRollout;
impl ActionHandler for BackwardRollout {
    fn name(&self) -> &'static str {
        "BackwardRollout"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if cx.state.action_state == 0 {
            cx.state.action_state = 1;
        }
        let cfg = AirConfig {
            anim: if cx.state.action_state == 1 {
                anim_id::MARIO_ANIM_BACKWARD_SPINNING
            } else {
                anim_id::MARIO_ANIM_GENERAL_FALL
            },
            ..base_air_config()
        };
        if let Some(r) = air_common(cx, prev_buttons, &cfg) {
            return r;
        }
        // decomp: `if (m->actionState == 1 && animFrame == 2) m->actionState = 2;`
        if cx.state.action_state == 1 && cx.state.anim.frame == 2 {
            cx.state.action_state = 2;
        }
        cx.timeline.slot = slot::ROLLOUT;
        ActionResult::Stay
    }
}
air_action!(
    WaterJump,
    "WaterJump",
    cx,
    AirConfig {
        // decomp: act_water_jump requests MARIO_ANIM_SINGLE_JUMP at the top,
        // before the step (airborne.c:822); air_common covers the NONE and
        // HIT_WALL arms.
        anim: anim_id::MARIO_ANIM_SINGLE_JUMP,
        wall_slide: true,      // walls slide (forward reset 15), no bonk
        ..base_air_config()
    },
    slot::WATER_JUMP
);

/// Jump kick: B in slow air, or A on the first punch frame. Committed to
/// the kick (no dive cancel); ground pound still allowed. Landing ->
/// FREEFALL_LAND. A slow wall hit stops the kick into a decel (faster hits
/// take the shared air-hit-wall path inside air_common).
pub struct JumpKick;
impl ActionHandler for JumpKick {
    fn name(&self) -> &'static str {
        "JumpKick"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        // decomp: act_jump_kick requests MARIO_ANIM_AIR_KICK once
        // (actionState 0 -> 1), forcing the restart with animID = -1, before
        // the step (airborne.c:1631).
        if cx.state.action_state == 0 {
            cx.state.anim.cur_id = -1;
            cx.set_mario_animation(anim_id::MARIO_ANIM_AIR_KICK);
            cx.state.action_state = 1;
        }
        let cfg = AirConfig {
            allow_dive: false, // committed to the kick (verify)
            ..base_air_config()
        };
        if let Some(r) = air_common(cx, prev_buttons, &cfg) {
            return r;
        }
        // Wall -> stop: the graze path in air_common kept the character
        // against the wall; end the kick here instead of sliding along.
        if cx.state.wall.is_some() {
            cx.state.vel.x = 0.0;
            cx.state.vel.z = 0.0;
            cx.state.forward_speed = 0.0;
            cx.timeline.slot = super::ground::slot::DECEL;
            return cx.goto(ActionId::DECELERATING, 0);
        }
        cx.timeline.slot = slot::JUMP_KICK;
        ActionResult::Stay
    }
}

/// Freefall: walked off a ledge. Dive and pound allowed (verify for pound).
/// Arg 2 = dead fall (health < 0x100): no input response.
pub struct Freefall;
impl ActionHandler for Freefall {
    fn name(&self) -> &'static str {
        "Freefall"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        let arg = cx.arg();
        // Dead fall (health < 0x100): no input response. The decomp has no
        // dead fall — actionArg only picks the anim — so dead-ness keys off
        // health, not the arg. (Arg 2 is the slide-kick fall anim per
        // act_slide_kick, and it must keep its cancels.)
        let mut cfg = if cx.state.health < 0x100 {
            AirConfig {
                allow_cancels: false,
                ..base_air_config()
            }
        } else {
            base_air_config()
        };
        // decomp: act_freefall picks the animation by actionArg (airborne.c:525).
        cfg.anim = match arg {
            1 => anim_id::MARIO_ANIM_FALL_FROM_SLIDE,
            2 => anim_id::MARIO_ANIM_FALL_FROM_SLIDE_KICK,
            _ => anim_id::MARIO_ANIM_GENERAL_FALL,
        };
        if let Some(r) = air_common(cx, prev_buttons, &cfg) {
            return r;
        }
        cx.timeline.slot = slot::FREEFALL;
        ActionResult::Stay
    }
}

/// GroundPound: a rising phase, then the slam straight down.
/// action_state 0 = rise, 1 = slam.
///
/// Rise: the first 10 frames climb at (22 - 2*timer) units/frame (20 on the
/// first tick down to 2 on the tenth; the 1-based timer is shifted to match
/// the 0-based reference formula 20 - 2*t), vy pinned at -50, forward
/// zeroed. The slam starts when the timer reaches the start animation's
/// loopEnd + 4 (decomp: airborne.c:918; 15 for the arg-0 anim). Needs 320
/// headroom (checked at entry); without it the pound starts in the slam
/// phase.
/// Slam: gravity -4 from -50, terminal -75, no steering.
pub struct GroundPound;
impl ActionHandler for GroundPound {
    fn name(&self) -> &'static str {
        "GroundPound"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        // spec: ground_pound.rise_frames / ground_pound.slam_transition
        if cx.state.action_state == 0 {
            // decomp: act_ground_pound (airborne.c:918). The port's rise
            // formula `22 - 2*t` reproduces the decomp's `20 - 2*timer` frame
            // amounts with a 1-based shifted timer, so the increment comes
            // first (first rise tick: t=1 -> +20).
            cx.state.action_timer += 1;
            let t = cx.state.action_timer;
            // decomp: the start anim depends on actionArg (1 only from the
            // wing-cap flight, unimplemented in the port).
            let start_anim = if cx.arg() == 0 {
                anim_id::MARIO_ANIM_START_GROUND_POUND
            } else {
                anim_id::MARIO_ANIM_TRIPLE_JUMP_GROUND_POUND
            };
            if t <= 10 {
                cx.state.pos.y += 22.0 - 2.0 * t as f32;
            }
            cx.state.forward_speed = 0.0;
            cx.state.vel = Vec3::new(0.0, -50.0, 0.0);
            cx.set_mario_animation(start_anim);
            // decomp: `if (m->actionTimer >= curAnim->loopEnd + 4)` moves to
            // the slam. START_GROUND_POUND loopEnd = 11 -> timer 15, matching
            // the old `t > 14` heuristic exactly.
            if t >= crate::anim_table::anim_header(start_anim).loop_end as u32 + 4 {
                cx.state.action_state = 1;
            }
            cx.timeline.slot = slot::GROUND_POUND;
            return ActionResult::Stay;
        }
        // Slam phase. No dive/kick cancel mid-slam: B did nothing here
        // before the slow-air jump-kick branch existed, and the slam
        // zeroes horizontal speed anyway. Pound re-entry stays allowed.
        // decomp: the slam requests MARIO_ANIM_GROUND_POUND every frame,
        // before the step (airborne.c:918).
        cx.set_mario_animation(anim_id::MARIO_ANIM_GROUND_POUND);
        let slam_cfg = AirConfig {
            allow_dive: false,
            ..base_air_config()
        };
        if let Some(r) = air_cancels(cx, prev_buttons, &slam_cfg) {
            return r;
        }
        cx.state.vel.x = 0.0;
        cx.state.vel.z = 0.0;
        // Gravity comes from perform_air_step's internal apply_gravity (ST-07).
        let vy_entry = cx.state.vel.y;
        match perform_air_step(&mut cx.state, cx.world, 0) {
            AirStep::HitWall | AirStep::HitLavaWall => {
                // Bonk off the wall into a backwards air knockback.
                let n = cx.state.wall.map(|w| w.normal).unwrap_or(glam::Vec3::X);
                let away = crate::trig::atan2s(n.z, n.x);
                cx.state.face_yaw = away;
                cx.state.forward_speed = -16.0;
                let (fx, fz) = cx.forward_xz();
                cx.state.vel.x = fx * -16.0;
                cx.state.vel.z = fz * -16.0;
                cx.state.vel.y = 40.0;
                cx.timeline.slot = slot::AIR_KNOCKBACK;
                return cx.goto_next(ActionId::BACKWARD_AIR_KB, 0);
            }
            AirStep::Landed => {
                let fall_speed = (-vy_entry).max(0.0);
                cx.events.push(Event::Landed { fall_speed });
                cx.state.land_from = ActionId::GROUND_POUND;
                if let Some(r) = check_fall_damage(cx, fall_speed) {
                                    return r;
                }
                            cx.timeline.slot = super::ground::slot::LAND;
                return cx.goto_next(ActionId::FREEFALL_LAND, fall_speed.max(0.0) as u32);
            }
            AirStep::GrabbedLedge => {
                return cx.goto_next(ActionId::LEDGE_GRAB, 0);
            }
            AirStep::GrabbedCeiling | AirStep::None => {}
        }
        cx.timeline.slot = slot::GROUND_POUND;
        ActionResult::Stay
    }
}

/// Wall-kick flight entry: face away from the wall, forward speed raised
/// to at least 24, vertical speed `vy`. Routes to WALL_KICK_AIR.
fn enter_wall_kick_flight(cx: &mut ActionCx, wall_normal: glam::Vec3, vy: f32) -> ActionResult {
    mark_jump(cx);
    // spec: wall_kick.forward_min / wall_kick.vertical (decomp-derived)
    let vy = adjusted_jump_vy(cx, vy);
    let away = crate::trig::atan2s(wall_normal.z, wall_normal.x);
    cx.state.face_yaw = away;
    cx.state.forward_speed = cx.state.forward_speed.max(24.0);
    set_air_velocity(cx, vy);
    cx.timeline.slot = slot::WALL_KICK;
    cx.events.push(Event::Jumped { velocity_y: vy });
    cx.goto(ActionId::WALL_KICK_AIR, 0)
}

/// Wall kick entry from the kick window: vy = 62.
/// (ID verified: wiki:Wall Kick rev 19311)
pub fn enter_wall_kick(cx: &mut ActionCx, wall_normal: glam::Vec3) -> ActionResult {
    enter_wall_kick_flight(cx, wall_normal, 62.0)
}

/// Water jump entry: vy = 42, forward speed raised to at least 15.
/// (verified: decomp set_mario_action_airborne ACT_WATER_JUMP; the 62.0f set
/// in check_water_jump is overwritten by the 42.0f transition assignment)
pub fn enter_water_jump(cx: &mut ActionCx) -> ActionResult {
    mark_jump(cx);
    // spec: swim.jump_vertical / swim.jump_forward_min (verified: decomp set_mario_action_airborne)
    cx.state.forward_speed = cx.state.forward_speed.max(15.0);
    set_air_velocity(cx, 42.0);
    cx.timeline.slot = slot::WATER_JUMP;
    cx.events.push(Event::Jumped { velocity_y: 42.0 });
    cx.goto(ActionId::WATER_JUMP, 0)
}

/// Air-hit-wall entry: remove the into-wall velocity component so the
/// character stays against the wall (no bounce-away; the knockback path
/// rebuilds velocity explicitly). Forward speed is recomputed, and the
/// impact speed is stashed in the goto arg for the post-window resolution
/// (>= 38 -> knockback, else soft bonk).
fn enter_air_hit_wall(cx: &mut ActionCx, n: glam::Vec3) -> ActionResult {
    let speed = (cx.state.vel.x * cx.state.vel.x + cx.state.vel.z * cx.state.vel.z).sqrt();
    let dot = cx.state.vel.x * n.x + cx.state.vel.z * n.z;
    if dot < 0.0 {
        cx.state.vel.x -= dot * n.x;
        cx.state.vel.z -= dot * n.z;
    }
    let (fx, fz) = cx.forward_xz();
    cx.state.forward_speed = cx.state.vel.x * fx + cx.state.vel.z * fz;
    cx.timeline.slot = slot::AIR_HIT_WALL;
    cx.goto(ActionId::AIR_HIT_WALL, speed as u32)
}

/// Soft bonk entry: keeps the (bounced) forward speed, no air control.
pub fn enter_soft_bonk(cx: &mut ActionCx) -> ActionResult {
    cx.timeline.slot = slot::SOFT_BONK;
    cx.goto(ActionId::SOFT_BONK, 0)
}

/// Backwards air knockback entry. `hard` selects the hard variant
/// (action_arg = 1; consumed by Phase E ground knockbacks).
pub fn enter_backward_air_kb(
    cx: &mut ActionCx,
    wall_normal: glam::Vec3,
    hard: bool,
) -> ActionResult {
    // spec: knockback.vertical / knockback.backward_speed (verify)
    let away = crate::trig::atan2s(wall_normal.z, wall_normal.x);
    cx.state.face_yaw = away;
    cx.state.forward_speed = -15.0;
    set_air_velocity(cx, 40.0);
    cx.timeline.slot = slot::AIR_KNOCKBACK;
    let id = if hard {
        ActionId::HARD_BACKWARD_AIR_KB
    } else {
        ActionId::BACKWARD_AIR_KB
    };
    cx.goto(id, hard as u32)
}

/// Forwards air knockback entry: forward forced to +16, no air control.
/// `hard` selects the hard variant (action_arg = 1; Phase E consumes it).
/// Landing currently routes to FREEFALL_LAND (note); Phase E adds the
/// forward ground knockback.
pub fn enter_forward_air_kb(cx: &mut ActionCx, hard: bool) -> ActionResult {
    cx.state.forward_speed = 16.0;
    let (fx, fz) = cx.forward_xz();
    cx.state.vel.x = fx * 16.0;
    cx.state.vel.z = fz * 16.0;
    cx.timeline.slot = slot::AIR_KNOCKBACK;
    let id = if hard {
        ActionId::HARD_FORWARD_AIR_KB
    } else {
        ActionId::FORWARD_AIR_KB
    };
    cx.goto(id, hard as u32)
}

/// Steep jump entry: vy = 42 + forward/4; the lateral velocity component
/// is scaled by 0.75, re-aiming motion along the slope.
pub fn enter_steep_jump(cx: &mut ActionCx) -> ActionResult {
    mark_jump(cx);
    // spec: jump.steep_vertical (decomp-derived)
    let vy = adjusted_jump_vy(cx, 42.0 + cx.state.forward_speed * 0.25);
    let (fx, fz) = cx.forward_xz();
    let fwd = cx.state.vel.x * fx + cx.state.vel.z * fz;
    let side = (cx.state.vel.x * fz - cx.state.vel.z * fx) * 0.75;
    cx.state.forward_speed = fwd;
    cx.state.vel = Vec3::new(fx * fwd + fz * side, vy, fz * fwd - fx * side);
    cx.timeline.slot = slot::STEEP_JUMP;
    cx.events.push(Event::Jumped { velocity_y: vy });
    cx.goto(ActionId::STEEP_JUMP, 0)
}


/// Wall-kick flight: no-turn air model, jump-height control on, B -> dive,
/// Z -> ground pound, ledge grab allowed; landing -> JUMP_LAND.
pub struct WallKickAir;
impl ActionHandler for WallKickAir {
    fn name(&self) -> &'static str {
        "WallKickAir"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = air_common(
            cx,
            prev_buttons,
            &AirConfig {
                // decomp: act_wall_kick_air requests MARIO_ANIM_SLIDEJUMP via
                // common_air_action_step (airborne.c:618).
                anim: anim_id::MARIO_ANIM_SLIDEJUMP,
                        steer_rate: 0, // no-turn model: stick accelerates, facing holds
                ..base_air_config()
            },
        ) {
            return r;
        }
        cx.timeline.slot = slot::WALL_KICK;
        ActionResult::Stay
    }
}

/// Air hit wall: transient 2-frame bonk state. A in the window -> wall kick
/// (vy 52, turned 180 deg); after the window, impact speed >= 38 ->
/// backwards air knockback (with a 5-frame late-kick allowance), else the
/// soft bonk.
///
/// Deviation note: the reference executes the entry frame's body twice, so
/// its kickable window ("firsties") is effectively 1 frame; this
/// implementation runs the body once per frame, giving a full 2-frame
/// window. The window length is the documented behavior; the double
/// execution is not reproduced.
pub struct AirHitWall;
impl ActionHandler for AirHitWall {
    fn name(&self) -> &'static str {
        "AirHitWall"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        // decomp: `if (++(m->actionTimer) <= 2)` — pre-increment
        // (airborne.c:1309).
        cx.state.action_timer += 1;
        // Kickable window: the first two frames in this action.
        if cx.state.action_timer <= 2 {
            if cx.pressed(buttons::A, prev_buttons) {
                return enter_wall_kick_flight(cx, cx.state.wall.map(|w| w.normal).unwrap_or(glam::Vec3::X), 52.0);
            }
            // Stunned drift: no air control; gravity comes from
            // perform_air_step's internal apply_gravity (ST-07).
            let vy_entry = cx.state.vel.y;
            if perform_air_step(&mut cx.state, cx.world, AIR_STEP_CHECK_LEDGE_GRAB)
                == AirStep::Landed
            {
                let fall_speed = (-vy_entry).max(0.0);
                cx.events.push(Event::Landed { fall_speed });
                if let Some(f) = cx.state.floor {
                    cx.state.floor_kind = super::ground::surface_kind_index(f.kind);
                }
                cx.state.land_from = cx.state.action;
                cx.timeline.slot = super::ground::slot::LAND;
                return enter_landing_for(cx, fall_speed);
            }
            // decomp: the window fall-through requests
            // MARIO_ANIM_START_WALLKICK (airborne.c:1309). (The decomp's
            // missing return re-runs the action same-frame; not reproduced —
            // see the deviation note above.)
            cx.set_mario_animation(anim_id::MARIO_ANIM_START_WALLKICK);
            cx.timeline.slot = slot::AIR_HIT_WALL;
            return ActionResult::Stay;
        }
        // Window expired: resolve by impact speed (carried in the goto arg).
        if cx.arg() as f32 >= 38.0 {
            // Late-kick allowance: A still kicks for 5 more frames via the
            // wall_kick_timer path in air_cancels.
            cx.state.wall_kick_timer = 5;
            return enter_backward_air_kb(cx, cx.state.wall.map(|w| w.normal).unwrap_or(glam::Vec3::X), false);
        }
        enter_soft_bonk(cx)
    }
}

/// Shared knockback config: stunned, no air control, no cancels.
fn knockback_config() -> AirConfig {
    AirConfig {
        steer_rate: 0, // stunned (verify)
        air_drag_threshold: 32.0,
        allow_dive: false,
        allow_pound: false,
        can_ledge_grab: false,
        ..base_air_config()
    }
}

/// Backwards air knockback: stunned, drifting away from the wall.
/// Also serves the hard backward variant (action_arg = 1); behavior is
/// identical until Phase E consumes the flag.
pub struct BackwardAirKb;
impl ActionHandler for BackwardAirKb {
    fn name(&self) -> &'static str {
        "BackwardAirKb"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        // decomp: common_air_knockback_step requests MARIO_ANIM_BACKWARD_AIR_KB
        // on AIR_STEP_NONE and on AIR_STEP_HIT_WALL (airborne.c:1097).
        let cfg = AirConfig {
            anim: anim_id::MARIO_ANIM_BACKWARD_AIR_KB,
            ..knockback_config()
        };
        if let Some(r) = air_common(cx, prev_buttons, &cfg) {
            return r;
        }
        cx.timeline.slot = slot::AIR_KNOCKBACK;
        ActionResult::Stay
    }
}

/// Forwards air knockback: forward forced to +16 at entry, then stunned.
/// Also serves the hard forward variant (action_arg = 1); Phase E consumes
/// the flag for the forward ground knockback (landing is FREEFALL_LAND
/// for now).
pub struct ForwardAirKb;
impl ActionHandler for ForwardAirKb {
    fn name(&self) -> &'static str {
        "ForwardAirKb"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        // decomp: common_air_knockback_step requests MARIO_ANIM_AIR_FORWARD_KB
        // on AIR_STEP_NONE, MARIO_ANIM_BACKWARD_AIR_KB on AIR_STEP_HIT_WALL
        // (airborne.c:1097).
        let cfg = AirConfig {
            anim: anim_id::MARIO_ANIM_AIR_FORWARD_KB,
            anim_wall: Some(anim_id::MARIO_ANIM_BACKWARD_AIR_KB),
            ..knockback_config()
        };
        if let Some(r) = air_common(cx, prev_buttons, &cfg) {
            return r;
        }
        cx.timeline.slot = slot::AIR_KNOCKBACK;
        ActionResult::Stay
    }
}

/// Soft bonk: keeps the forward speed, no air control; landing ->
/// FREEFALL_LAND. This is the < 38 wall-hit outcome. Unlike the knockbacks,
/// the soft bonk leaves Mario against the wall, so ledge grabs are allowed
/// (he can catch a ledge as he falls past it).
pub struct SoftBonk;
impl ActionHandler for SoftBonk {
    fn name(&self) -> &'static str {
        "SoftBonk"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        // decomp: act_soft_bonk passes MARIO_ANIM_GENERAL_FALL to
        // common_air_knockback_step; the HIT_WALL arm requests
        // MARIO_ANIM_BACKWARD_AIR_KB (airborne.c:1097,1247).
        if let Some(r) = air_common(
            cx,
            prev_buttons,
            &AirConfig {
                anim: anim_id::MARIO_ANIM_GENERAL_FALL,
                anim_wall: Some(anim_id::MARIO_ANIM_BACKWARD_AIR_KB),
                can_ledge_grab: true,
                ..knockback_config()
            },
        ) {
            return r;
        }
        cx.timeline.slot = slot::SOFT_BONK;
        ActionResult::Stay
    }
}

/// Steep jump: forward speed decays x0.98/frame with no stick control;
/// jump-height control applies; landing while still moving backward along
/// facing slides, otherwise jump-lands; walls stop the character dead.
pub struct SteepJump;
impl ActionHandler for SteepJump {
    fn name(&self) -> &'static str {
        "SteepJump"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        let cfg = AirConfig {
                ..base_air_config()
        };
        if let Some(r) = air_cancels(cx, prev_buttons, &cfg) {
            return r;
        }
        // Forward decays x0.98/frame; no stick control.
        let (fx, fz) = cx.forward_xz();
        let fwd = (cx.state.vel.x * fx + cx.state.vel.z * fz) * 0.98;
        let side = cx.state.vel.x * fz - cx.state.vel.z * fx;
        cx.state.vel.x = fx * fwd + fz * side;
        cx.state.vel.z = fz * fwd - fx * side;
        cx.state.forward_speed = fwd;
        // Gravity (and jump-height control) come from perform_air_step's
        // internal apply_gravity (ST-07).
        let vy_entry = cx.state.vel.y;
        // decomp: act_steep_jump requests MARIO_ANIM_SINGLE_JUMP after the
        // switch, unconditionally — even on frames that transition
        // (airborne.c:888).
        match perform_air_step(&mut cx.state, cx.world, AIR_STEP_CHECK_LEDGE_GRAB) {
            AirStep::HitWall | AirStep::HitLavaWall => {
                // Stop dead; the next ground tick sorts out the floor.
                cx.state.vel.x = 0.0;
                cx.state.vel.z = 0.0;
                cx.state.forward_speed = 0.0;
                cx.timeline.slot = super::ground::slot::DECEL;
                cx.set_mario_animation(anim_id::MARIO_ANIM_SINGLE_JUMP);
                return cx.goto_next(ActionId::DECELERATING, 0);
            }
            AirStep::Landed => {
                let fall_speed = (-vy_entry).max(0.0);
                cx.events.push(Event::Landed { fall_speed });
                if let Some(f) = cx.state.floor {
                    cx.state.floor_kind = super::ground::surface_kind_index(f.kind);
                }
                cx.state.land_from = ActionId::STEEP_JUMP;
                cx.timeline.slot = super::ground::slot::LAND;
                cx.set_mario_animation(anim_id::MARIO_ANIM_SINGLE_JUMP);
                return enter_landing_for(cx, fall_speed);
            }
            AirStep::GrabbedLedge => {
                cx.set_mario_animation(anim_id::MARIO_ANIM_SINGLE_JUMP);
                return cx.goto_next(ActionId::LEDGE_GRAB, 0);
            }
            AirStep::GrabbedCeiling | AirStep::None => {}
        }
        cx.set_mario_animation(anim_id::MARIO_ANIM_SINGLE_JUMP);
        cx.timeline.slot = slot::STEEP_JUMP;
        ActionResult::Stay
    }
}

/// Ledge grab: hanging on the ledge (facing the wall).
///
/// - A with 160 units of headroom: fast climb.
/// - Z: let go, dropping away from the wall.
/// - Stick toward the wall at 10+ hang frames: slow climb.
/// - Stick more than 90 deg from the facing (away/sideways): let go.
/// - Ledge floor steeper than normal.y 0.9063: release.
///
/// (decomp-derived ledge behavior)
pub struct LedgeGrab;
impl ActionHandler for LedgeGrab {
    fn name(&self) -> &'static str {
        "LedgeGrab"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        // decomp: act_ledge_grab caps the hang timer at 10
        // (`if (m->actionTimer < 10) m->actionTimer++;`, automatic.c:543).
        if cx.state.action_timer < 10 {
            cx.state.action_timer += 1;
        }
        // Release: the ledge floor he's hanging from got too steep.
        // Probe at hand height over the LEDGE (30 inside the wall): the hang
        // point itself is 30 outside the wall, so a probe there would find
        // the ground far below instead of the ledge. Feet dangle a full
        // body-height below the grab point, so a feet-level probe would miss
        // the ledge.
        // spec: ledge.release_slope_y (decomp-derived)
        let (fx, fz) = cx.forward_xz();
        let over_ledge =
            cx.state.pos + glam::Vec3::new(fx * 30.0, cx.params.height, fz * 30.0);
        if let Some(f) = cx.world.find_floor(over_ledge.x, over_ledge.y, over_ledge.z) {
            if f.normal.y < 0.9063 {
                cx.timeline.slot = slot::FREEFALL;
                return cx.goto(ActionId::FREEFALL, 0);
            }
        }
        // A: fast climb. Decomp `act_ledge_grab` (automatic.c:558):
        // `hasSpaceForMario = (m->ceilHeight - m->floorHeight >= 160.0f)`.
        // Uses the geometry-stage ceil/floor heights, not a probe.
        let ceil_h = cx.state.ceil_y.unwrap_or(CELL_HEIGHT_LIMIT);
        let floor_h = cx.state.floor_y.unwrap_or(FLOOR_LOWER_LIMIT);
        if cx.pressed(buttons::A, prev_buttons) && ceil_h - floor_h >= 160.0 {
            return cx.goto(ActionId::LEDGE_CLIMB_FAST, 0);
        }
        // Z: let go, dropping away from the wall.
        if cx.pressed(buttons::Z, prev_buttons) {
            return ledge_let_go(cx);
        }
        // Stick: toward the wall climbs (after the hang timer), anything
        // more than 90 deg from the facing lets go.
        if let Some(iy) = cx.intended_yaw() {
            let from_facing = cx.state.face_yaw.diff_to(iy).abs();
            if from_facing <= 0x4000 {
                if cx.state.action_timer >= 10 {
                    return cx.goto(ActionId::LEDGE_CLIMB_SLOW_1, 0);
                }
            } else {
                return ledge_let_go(cx);
            }
        }
        cx.state.vel = glam::Vec3::ZERO;
        // decomp: act_ledge_grab requests MARIO_ANIM_IDLE_ON_LEDGE every
        // frame (automatic.c:543).
        cx.set_mario_animation(anim_id::MARIO_ANIM_IDLE_ON_LEDGE);
        cx.timeline.slot = slot::LEDGE_GRAB;
        ActionResult::Stay
    }
}

/// Let go of the ledge: drop with forward speed -8 (away from the wall,
/// since the facing points at the wall). The grab already snapped Mario to
/// the ledge point 30 units out from the wall, so the position stands.
fn ledge_let_go(cx: &mut ActionCx) -> ActionResult {
    cx.state.forward_speed = -8.0; // spec: ledge.drop_speed (verified)
    cx.state.vel = glam::Vec3::ZERO;
    cx.timeline.slot = slot::FREEFALL;
    // The decomp's let_go_of_ledge returns set_mario_action (TRUE), but the
    // plugin's FREEFALL would land same-tick from the ledge top; book it for
    // the next tick (the action-function-returns-FALSE semantics).
    cx.goto_next(ActionId::FREEFALL, 0)
}

/// Pull up onto the ledge after a climb: move forward (toward the wall,
/// onto the ledge top) and snap to the floor, then stand.
fn ledge_pull_up(cx: &mut ActionCx, distance: f32) -> ActionResult {
    let (fx, fz) = cx.forward_xz();
    cx.state.pos.x += fx * distance;
    cx.state.pos.z += fz * distance;
    // Snap the feet onto the ledge the hands are holding: probe at hand
    // height, since the feet dangle a full body-height below the grab point
    // and a feet-level probe would find the ground far below instead.
    let hands = cx.state.pos + glam::Vec3::new(0.0, cx.params.height, 0.0);
    if let Some(f) = cx.world.find_floor(hands.x, hands.y, hands.z) {
        cx.state.pos.y = f.y;
        cx.state.floor_y = Some(f.y);
    }
    cx.timeline.slot = super::ground::slot::IDLE;
    cx.goto(ActionId::IDLE, 0)
}

/// Fast ledge climb: an 8-frame pull-up onto the ledge.
///
/// The decomp pins m->pos to the ledge top for the whole climb and lets the
/// climb animation's root motion do the visual rise; our procedural character
/// has no root motion, so the sim itself moves the body. Up first, then over:
/// rise to the ledge top at the hang xz (30 outside the wall, so nothing
/// intersects), then travel forward at the top level onto the ledge. Never
/// moves horizontally while below the top — that's what drove the body
/// through the ledge. The +14 forward nudge at the end is the decomp's
/// climb_up_ledge.
pub struct LedgeClimbFast;
impl ActionHandler for LedgeClimbFast {
    fn name(&self) -> &'static str {
        "LedgeClimbFast"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: the climb timer advances every frame (via
        // update_ledge_climb_camera, automatic.c:531); the port's pull-up
        // trigger keys off it, so the increment is explicit here.
        cx.state.action_timer += 1;
        // decomp: update_ledge_climb requests MARIO_ANIM_FAST_LEDGE_GRAB every
        // frame (automatic.c:531).
        cx.set_mario_animation(anim_id::MARIO_ANIM_FAST_LEDGE_GRAB);
        // Pull-up check after the increment: checking before would double-move.
        if cx.state.action_timer >= 8 {
            return ledge_pull_up(cx, 14.0);
        }
        // The decomp pins m->pos to the ledge top for the whole climb
        // (check_ledge_grab places the feet AT the top); the climb animation's
        // root motion does the visual rise. Our procedural character has no
        // root motion, so we hold position here and the pull-up does the
        // +14 forward nudge (decomp climb_up_ledge) at the end.
        cx.state.vel = glam::Vec3::ZERO;
        cx.timeline.slot = slot::LEDGE_GRAB;
        ActionResult::Stay
    }
}

/// Slow ledge climb, part 1: at timer 17 the climb transitions to part 2.
pub struct LedgeClimbSlow1;
impl ActionHandler for LedgeClimbSlow1 {
    fn name(&self) -> &'static str {
        "LedgeClimbSlow1"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: the climb timer advances every frame (via
        // update_ledge_climb_camera, automatic.c:531).
        cx.state.action_timer += 1;
        // decomp: update_ledge_climb requests MARIO_ANIM_SLOW_LEDGE_GRAB every
        // frame (automatic.c:531); slow parts 1 and 2 share the decomp's
        // act_ledge_climb_slow.
        cx.set_mario_animation(anim_id::MARIO_ANIM_SLOW_LEDGE_GRAB);
        cx.timeline.slot = slot::LEDGE_GRAB;
        // Gradual pull-up like the fast climb, spread over the 17 frames, so
        // the body rises instead of teleporting up when Slow2 pulls up.
        cx.state.pos.y += cx.params.height / 17.0;
        cx.state.vel = glam::Vec3::ZERO;
        if cx.state.action_timer >= 17 {
            return cx.goto(ActionId::LEDGE_CLIMB_SLOW_2, 0);
        }
        ActionResult::Stay
    }
}

/// Slow ledge climb, part 2: at 11 frames in (28 total) any input pulls
/// up onto the ledge. The pull-up travels 54 forward: the hang is 30 outside
/// the wall, so 54 lands 24 inside, on the ledge (matching the fast climb's
/// end point). A long no-input stall falls back to idle so the character
/// can never soft-lock mid-climb.
pub struct LedgeClimbSlow2;
impl ActionHandler for LedgeClimbSlow2 {
    fn name(&self) -> &'static str {
        "LedgeClimbSlow2"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // decomp: the climb timer advances every frame (via
        // update_ledge_climb_camera, automatic.c:531). Slow part 2 shares the
        // decomp's act_ledge_climb_slow with part 1 (automatic.c:600).
        cx.state.action_timer += 1;
        cx.set_mario_animation(anim_id::MARIO_ANIM_SLOW_LEDGE_GRAB);
        cx.timeline.slot = slot::LEDGE_GRAB;
        if cx.state.action_timer >= 11 && (cx.stick_held() || cx.input.buttons != 0) {
            return ledge_pull_up(cx, 54.0);
        }
        if cx.state.action_timer >= 120 {
            return ledge_pull_up(cx, 54.0);
        }
        ActionResult::Stay
    }
}
