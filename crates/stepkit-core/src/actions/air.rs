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
use crate::events::Event;
use crate::input::buttons;
use crate::state::ActionId;
use crate::step::step_air;
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
}

/// Per-action air behavior switches.
struct AirConfig {
    height_control: bool,
    gravity: f32,
    terminal: f32,
    steer_rate: u16,
    air_drag_threshold: f32,
    allow_dive: bool,
    allow_pound: bool,
    /// Whether this action may ledge-grab (documented per action).
    can_ledge_grab: bool,
    /// Whether the shared cancel set (water plunge, wall kick, dive,
    /// pound) runs. Disabled for the dead fall (no input response).
    allow_cancels: bool,
}

/// Track the peak height of the current airtime (for fall damage).
fn track_peak(cx: &mut ActionCx) {
    if cx.state.pos.y > cx.state.peak_height {
        cx.state.peak_height = cx.state.pos.y;
    }
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
    if cx.pressed(buttons::A, prev_buttons) && cx.state.wall_kick_timer > 0 {
        let n = cx.state.wall_normal;
        return Some(enter_wall_kick(cx, n));
    }
    if cfg.allow_dive && cx.pressed(buttons::B, prev_buttons) && cx.state.forward_speed > 28.0 {
        // spec: dive.air_speed_threshold (verified: wiki:Dive@19303)
        return Some(enter_dive(cx, false));
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
            let dyaw = iy.diff_to(cx.state.face_yaw);
            let dyaw_rad = dyaw as f32 / 65536.0 * std::f32::consts::TAU;
            // Forward accel: 1.5 * cos(dYaw) * mag. Side accel: 10.0 * sin(dYaw) * mag.
            fwd += mag * dyaw_rad.cos() * 1.5;
            side += mag * dyaw_rad.sin() * 10.0;
            // Turn facing independently (does not rotate velocity).
            // Multiplier comes from the action config: 512 for the
            // with-turn model, 0 to disable (knockbacks, dives).
            let turn = Angle((p.steer_rate as f32 * dyaw_rad.sin() * mag) as i16);
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
    // Gravity applies after movement and collision.
    let vy_before_landing = cx.state.vel.y;
    let out = step_air(&cx.state, cx.world, cx.params);
    cx.state.pos = out.pos;
    cx.state.vel.y = out.vel_y;
    cx.state.floor_y = out.floor.map(|f| f.y);
    track_peak(cx);
    if out.wall_hit {
        cx.state.wall_hit = true;
        cx.state.wall_normal = out.wall_normal;
        cx.events.push(Event::WallHit {
            normal_yaw: crate::trig::atan2(out.wall_normal.x, out.wall_normal.z),
        });
        cx.state.wall_kick_timer = 10; // spec: wall.kick_window (verify)
                                       // Speed-dependent wall response.
        let speed = (cx.state.vel.x * cx.state.vel.x + cx.state.vel.z * cx.state.vel.z).sqrt();
        let n = out.wall_normal;
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
        // Ledge grab takes priority over the bonk (the reference checks it
        // during the step, before wall resolution): falling, no landing,
        // and the action allows it. Conditions: wiki:Ledge Grab@19518.
        if out.floor.is_none() && vy_before_landing <= 0.0 && cfg.can_ledge_grab {
            if let Some(r) = try_ledge_grab(cx, out.wall_normal) {
                return Some(r);
            }
        }
        if !stopped_dead {
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
    } else {
        cx.state.wall_hit = false;
        cx.state.wall_kick_timer = cx.state.wall_kick_timer.saturating_sub(1);
    }
    if let Some(f) = out.floor {
        // Slide kick: the first touchdown bounces at half the impact speed
        // (tracked via action_state); the second becomes a ground slide.
        // The snap window can register a touchdown at the apex (impact
        // speed ~ 0), so the bounce has a working minimum to guarantee
        // the character actually leaves the ground (verify).
        if cx.state.action == ActionId::SLIDE_KICK && cx.state.action_state == 0 {
            cx.state.action_state = 1;
            cx.state.pos = out.pos;
            cx.state.floor_y = Some(f.y);
            cx.state.vel.y = (out.fall_speed * 0.5).max(8.0);
            cx.events.push(Event::Landed {
                fall_speed: out.fall_speed,
            });
        } else {
            let fall_speed = out.fall_speed;
            cx.events.push(Event::Landed { fall_speed });
            cx.state.floor_kind = super::ground::surface_kind_index(f.kind);
            cx.state.land_from = cx.state.action;
            cx.timeline.slot = super::ground::slot::LAND;
            return Some(enter_landing_for(cx, fall_speed));
        }
    }
    // Gravity applies after movement and collision.
    // Jump-height control: A released while rising fast quarters vy.
    if p.height_control
        && cx.input.buttons & buttons::A == 0
        && cx.state.vel.y > cx.params.jump_height_control_threshold
    {
        cx.state.vel.y /= 4.0;
    }
    cx.state.vel.y = (cx.state.vel.y - p.gravity).max(p.terminal);
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
        let non_slippery = cx.world.find_floor(cx.state.pos, 1.0).is_some_and(|f| {
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
        cx.state.peak_height = 0.0;
        return r;
    }
    cx.state.peak_height = 0.0;
    // Dead: collapse to idle (input stays ignored while health < 0x100).
    if cx.state.health < 0x100 {
        return cx.goto(ActionId::IDLE, 2);
    }
    // Deep quicksand landing: the escape sequence.
    if cx.state.quicksand_depth >= 11.0 {
        return cx.goto(ActionId::QUICKSAND_JUMP_LAND, 0);
    }
    let arg = fall_speed.max(0.0) as u32;
    match cx.state.land_from {
        ActionId::JUMP => cx.goto(ActionId::JUMP_LAND, arg),
        ActionId::DOUBLE_JUMP => cx.goto(ActionId::DOUBLE_JUMP_LAND, arg),
        ActionId::TRIPLE_JUMP => cx.goto(ActionId::TRIPLE_JUMP_LAND, arg),
        ActionId::BACKFLIP => cx.goto(ActionId::BACKFLIP_LAND, arg),
        ActionId::SIDE_FLIP => cx.goto(ActionId::SIDE_FLIP_LAND, arg),
        ActionId::LONG_JUMP => cx.goto(ActionId::LONG_JUMP_LAND, arg),
        ActionId::DIVE => super::ground::enter_dive_slide(cx),
        ActionId::SLIDE_KICK => super::ground::enter_slide_kick_slide(cx),
        ActionId::WALL_KICK_AIR => cx.goto(ActionId::JUMP_LAND, arg),
        ActionId::STEEP_JUMP => {
            // Still moving backward along facing -> slide; else jump-land.
            if cx.state.forward_speed < 0.0 {
                super::ground::begin_sliding(cx)
            } else {
                cx.goto(ActionId::JUMP_LAND, arg)
            }
        }
        _ => cx.goto(ActionId::FREEFALL_LAND, arg),
    }
}

fn base_air_config() -> AirConfig {
    AirConfig {
        height_control: true,
        gravity: 4.0,
        terminal: -75.0,
        steer_rate: 512, // with-turn yaw multiplier (decomp: 512*sin(dyaw)*mag)
        air_drag_threshold: 32.0,
        allow_dive: true,
        allow_pound: true,
        can_ledge_grab: true,
        allow_cancels: true,
    }
}

/// Jump entry velocity, halved while waist-deep in quicksand (depth > 1)
/// and halved while squished. (decomp-derived quicksand/squish penalties)
fn adjusted_jump_vy(cx: &ActionCx, vy: f32) -> f32 {
    let mut v = vy;
    if cx.state.quicksand_depth > 1.0 {
        v *= 0.5;
    }
    if cx.state.squish_timer > 0 {
        v *= 0.5;
    }
    v
}

/// Shared jump entry: vertical speed = 42 + forward_speed/4, forward *= 0.8.
/// (wiki:Single Jump@19309)
pub fn enter_jump(cx: &mut ActionCx) -> ActionResult {
    let p = cx.params;
    let vy = adjusted_jump_vy(
        cx,
        p.jump_vertical_base + cx.state.forward_speed * p.jump_vertical_forward_factor,
    );
    cx.state.forward_speed *= p.jump_forward_retain;
    set_air_velocity(cx, vy);
    cx.timeline.slot = slot::JUMP;
    cx.events.push(Event::Jumped { velocity_y: vy });
    cx.goto(ActionId::JUMP, 0)
}

/// Double jump entry: vy = 52 + hspeed/4, forward *= 0.8.
/// (wiki:Double Jump@18964)
pub fn enter_double_jump(cx: &mut ActionCx) -> ActionResult {
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
    let r = cx.goto(ActionId::GROUND_POUND, 0);
    // spec: ground_pound.rise_headroom (decomp-derived)
    if cx.world.find_ceiling(cx.state.pos, 320.0).is_some() {
        cx.state.action_state = 1; // no room to rise: straight to the slam
    }
    r
}

fn set_air_velocity(cx: &mut ActionCx, vy: f32) {
    let (fx, fz) = cx.forward_xz();
    cx.state.vel = Vec3::new(fx * cx.state.forward_speed, vy, fz * cx.state.forward_speed);
}

macro_rules! air_action {
    ($name:ident, $label:literal, $cfg:expr, $slot:expr) => {
        pub struct $name;
        impl ActionHandler for $name {
            fn name(&self) -> &'static str {
                $label
            }
            fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
                if let Some(r) = air_common(cx, prev_buttons, &$cfg) {
                    return r;
                }
                cx.timeline.slot = $slot;
                ActionResult::Stay
            }
        }
    };
}

air_action!(SingleJump, "SingleJump", base_air_config(), slot::JUMP);
air_action!(
    DoubleJump,
    "DoubleJump",
    AirConfig {
        height_control: true, // (verify)
        ..base_air_config()
    },
    slot::DOUBLE_JUMP
);
air_action!(
    TripleJump,
    "TripleJump",
    AirConfig {
        height_control: false, // documented: no height control
        can_ledge_grab: false, // documented: no ledge grab or hang
        ..base_air_config()
    },
    slot::TRIPLE_JUMP
);
air_action!(
    Backflip,
    "Backflip",
    AirConfig {
        height_control: false, // (verify)
        allow_dive: false,
        can_ledge_grab: false, // documented: no ledge grab or hang
        ..base_air_config()
    },
    slot::BACKFLIP
);
air_action!(
    SideFlip,
    "SideFlip",
    AirConfig {
        height_control: false, // (verify)
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
        let gravity = cx.params.longjump_gravity;
        if let Some(r) = air_common(
            cx,
            prev_buttons,
            &AirConfig {
                height_control: false, // (verify)
                gravity,
                ..base_air_config()
            },
        ) {
            return r;
        }
        cx.timeline.slot = slot::LONG_JUMP;
        ActionResult::Stay
    }
}
air_action!(
    Dive,
    "Dive",
    AirConfig {
        height_control: false, // (verify)
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

/// Slide kick, airborne: after 30 frames and more than 500 above the
/// floor it becomes a freefall; the first touchdown bounces at half
/// impact speed (see air_common), the second becomes a ground slide.
pub struct SlideKick;
impl ActionHandler for SlideKick {
    fn name(&self) -> &'static str {
        "SlideKick"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if cx.state.action_timer >= 30 {
            let high = cx
                .world
                .find_floor(cx.state.pos, 4096.0)
                .is_none_or(|f| cx.state.pos.y - f.y > 500.0);
            if high {
                cx.timeline.slot = slot::FREEFALL;
                return cx.goto(ActionId::FREEFALL, 0);
            }
        }
        if let Some(r) = air_common(cx, prev_buttons, &base_air_config()) {
            return r;
        }
        cx.timeline.slot = slot::SLIDE_KICK;
        ActionResult::Stay
    }
}

air_action!(
    ForwardRollout,
    "ForwardRollout",
    base_air_config(),
    slot::ROLLOUT
);
air_action!(
    BackwardRollout,
    "BackwardRollout",
    base_air_config(),
    slot::ROLLOUT
);

/// Freefall: walked off a ledge. Dive and pound allowed (verify for pound).
/// Arg 2 = dead fall (health < 0x100): no input response.
pub struct Freefall;
impl ActionHandler for Freefall {
    fn name(&self) -> &'static str {
        "Freefall"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        let cfg = if cx.arg() == 2 {
            AirConfig {
                allow_cancels: false,
                ..base_air_config()
            }
        } else {
            base_air_config()
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
/// zeroed. Timer > 14 moves to the slam (simplification: the reference keys
/// this off the start animation's end). Needs 320 headroom (checked at
/// entry); without it the pound starts in the slam phase.
/// Slam: gravity -4 from -50, terminal -75, no steering.
pub struct GroundPound;
impl ActionHandler for GroundPound {
    fn name(&self) -> &'static str {
        "GroundPound"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        // spec: ground_pound.rise_frames / ground_pound.slam_transition
        if cx.state.action_state == 0 {
            let t = cx.state.action_timer;
            if t <= 10 {
                cx.state.pos.y += 22.0 - 2.0 * t as f32;
            }
            track_peak(cx);
            cx.state.forward_speed = 0.0;
            cx.state.vel = Vec3::new(0.0, -50.0, 0.0);
            if t > 14 {
                cx.state.action_state = 1;
            }
            cx.timeline.slot = slot::GROUND_POUND;
            return ActionResult::Stay;
        }
        // Slam phase.
        if let Some(r) = air_cancels(cx, prev_buttons, &base_air_config()) {
            return r;
        }
        cx.state.vel.x = 0.0;
        cx.state.vel.z = 0.0;
        cx.state.vel.y = (cx.state.vel.y - 4.0).max(-75.0);
        let out = step_air(&cx.state, cx.world, cx.params);
        cx.state.pos = out.pos;
        cx.state.vel.y = out.vel_y;
        cx.state.floor_y = out.floor.map(|f| f.y);
        track_peak(cx);
        if out.wall_hit {
            // Bonk off the wall into a backwards air knockback.
            let n = out.wall_normal;
            let away = crate::trig::atan2(n.x, n.z);
            cx.state.face_yaw = away;
            cx.state.forward_speed = -16.0;
            let (fx, fz) = cx.forward_xz();
            cx.state.vel.x = fx * -16.0;
            cx.state.vel.z = fz * -16.0;
            cx.state.vel.y = 40.0;
            cx.timeline.slot = slot::AIR_KNOCKBACK;
            return cx.goto(ActionId::BACKWARD_AIR_KB, 0);
        }
        if out.floor.is_some() {
            let fall_speed = out.fall_speed;
            cx.events.push(Event::Landed { fall_speed });
            cx.state.land_from = ActionId::GROUND_POUND;
            if let Some(r) = check_fall_damage(cx, fall_speed) {
                cx.state.peak_height = 0.0;
                return r;
            }
            cx.state.peak_height = 0.0;
            cx.timeline.slot = super::ground::slot::LAND;
            return cx.goto(ActionId::FREEFALL_LAND, fall_speed.max(0.0) as u32);
        }
        cx.timeline.slot = slot::GROUND_POUND;
        ActionResult::Stay
    }
}

/// Wall-kick flight entry: face away from the wall, forward speed raised
/// to at least 24, vertical speed `vy`. Routes to WALL_KICK_AIR.
fn enter_wall_kick_flight(cx: &mut ActionCx, wall_normal: glam::Vec3, vy: f32) -> ActionResult {
    // spec: wall_kick.forward_min / wall_kick.vertical (decomp-derived)
    let vy = adjusted_jump_vy(cx, vy);
    let away = crate::trig::atan2(wall_normal.x, wall_normal.z);
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
    let away = crate::trig::atan2(wall_normal.x, wall_normal.z);
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

/// Try a ledge grab after a wall hit while falling.
/// Conditions from wiki:Ledge Grab@19518:
/// - wall 30 units above Mario, no wall 150 units above,
/// - then a top-down floor search from 60 units into the wall and 160 up.
fn try_ledge_grab(cx: &mut ActionCx, wall_normal: glam::Vec3) -> Option<ActionResult> {
    // The wall must have displaced Mario against his velocity: the impact
    // velocity (still intact at this point in the wall handling) must
    // oppose the wall normal, which points away from the wall.
    // (decomp-derived ledge-grab condition)
    if cx.state.vel.x * wall_normal.x + cx.state.vel.z * wall_normal.z > 0.0 {
        return None;
    }
    // Probe with Mario's own radius (+1 for the contact boundary): "is there
    // a wall at this point", not "within reach".
    let probe_r = cx.params.radius + 1.0;
    let above_30 = cx.state.pos + glam::Vec3::new(0.0, 30.0, 0.0);
    let above_150 = cx.state.pos + glam::Vec3::new(0.0, 150.0, 0.0);
    // spec: ledge.wall_check_low / ledge.wall_check_high (verified)
    if !cx.world.wall_probe(above_30, probe_r) {
        return None;
    }
    if cx.world.wall_probe(above_150, probe_r) {
        return None;
    }
    // Into the wall, 60 units; the radius eats 50, leaving 10 into the wall.
    let mut into_wall = glam::Vec3::new(-wall_normal.x, 0.0, -wall_normal.z);
    if into_wall.length_squared() < 1e-8 {
        return None;
    }
    into_wall = into_wall.normalize();
    // spec: ledge.search_inward / ledge.search_up (verified)
    let search = cx.state.pos + into_wall * 60.0 + glam::Vec3::new(0.0, 160.0, 0.0);
    let floor = cx.world.find_floor(search, 0.0)?;
    // Grab: snap to the floor at the search xz (10 units into the wall,
    // per the wiki), face the wall, kill velocity.
    cx.state.pos = glam::Vec3::new(search.x, floor.y, search.z);
    cx.state.vel = glam::Vec3::ZERO;
    cx.state.forward_speed = 0.0;
    cx.state.face_yaw = crate::trig::atan2(into_wall.x, into_wall.z);
    cx.state.floor_y = Some(floor.y);
    cx.timeline.slot = slot::LEDGE_GRAB;
    cx.events.push(crate::events::Event::LedgeGrab);
    Some(cx.goto(ActionId::LEDGE_GRAB, 0))
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
                height_control: true,
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
        // Kickable window: the first two frames in this action.
        if cx.state.action_timer <= 2 {
            if cx.pressed(buttons::A, prev_buttons) {
                return enter_wall_kick_flight(cx, cx.state.wall_normal, 52.0);
            }
            // Stunned drift: no air control, gravity applies.
            cx.state.vel.y = (cx.state.vel.y - 4.0).max(-75.0);
            let out = step_air(&cx.state, cx.world, cx.params);
            cx.state.pos = out.pos;
            cx.state.vel.y = out.vel_y;
            cx.state.floor_y = out.floor.map(|f| f.y);
            track_peak(cx);
            if let Some(f) = out.floor {
                let fall_speed = out.fall_speed;
                cx.events.push(Event::Landed { fall_speed });
                cx.state.floor_kind = super::ground::surface_kind_index(f.kind);
                cx.state.land_from = cx.state.action;
                cx.timeline.slot = super::ground::slot::LAND;
                return enter_landing_for(cx, fall_speed);
            }
            cx.timeline.slot = slot::AIR_HIT_WALL;
            return ActionResult::Stay;
        }
        // Window expired: resolve by impact speed (carried in the goto arg).
        if cx.arg() as f32 >= 38.0 {
            // Late-kick allowance: A still kicks for 5 more frames via the
            // wall_kick_timer path in air_cancels.
            cx.state.wall_kick_timer = 5;
            return enter_backward_air_kb(cx, cx.state.wall_normal, false);
        }
        enter_soft_bonk(cx)
    }
}

/// Shared knockback config: stunned, no air control, no cancels.
fn knockback_config() -> AirConfig {
    AirConfig {
        height_control: false,
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
        if let Some(r) = air_common(cx, prev_buttons, &knockback_config()) {
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
        if let Some(r) = air_common(cx, prev_buttons, &knockback_config()) {
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
        if let Some(r) = air_common(
            cx,
            prev_buttons,
            &AirConfig {
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
            height_control: true, // steep jumps honor jump-height control
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
        // Gravity applies after movement (below the step).
        let out = step_air(&cx.state, cx.world, cx.params);
        cx.state.pos = out.pos;
        cx.state.vel.y = out.vel_y;
        cx.state.floor_y = out.floor.map(|f| f.y);
        track_peak(cx);
        if out.wall_hit {
            // Stop dead; the next ground tick sorts out the floor.
            cx.state.vel.x = 0.0;
            cx.state.vel.z = 0.0;
            cx.state.forward_speed = 0.0;
            cx.timeline.slot = super::ground::slot::DECEL;
            return cx.goto(ActionId::DECELERATING, 0);
        }
        if let Some(f) = out.floor {
            let fall_speed = out.fall_speed;
            cx.events.push(Event::Landed { fall_speed });
            cx.state.floor_kind = super::ground::surface_kind_index(f.kind);
            cx.state.land_from = ActionId::STEEP_JUMP;
            cx.timeline.slot = super::ground::slot::LAND;
            return enter_landing_for(cx, fall_speed);
        }
        // Jump-height control: A released while rising fast quarters vy.
        if cx.input.buttons & buttons::A == 0
            && cx.state.vel.y > cx.params.jump_height_control_threshold
        {
            cx.state.vel.y /= 4.0;
        }
        cx.state.vel.y = (cx.state.vel.y - 4.0).max(-75.0);
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
        // Release: the ledge floor got too steep.
        // spec: ledge.release_slope_y (decomp-derived)
        if let Some(f) = cx.world.find_floor(cx.state.pos, 1.0) {
            if f.normal.y < 0.9063 {
                cx.timeline.slot = slot::FREEFALL;
                return cx.goto(ActionId::FREEFALL, 0);
            }
        }
        // A: fast climb, needs 160 units of headroom.
        if cx.pressed(buttons::A, prev_buttons)
            && cx.world.find_ceiling(cx.state.pos, 160.0).is_none()
        {
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
        cx.timeline.slot = slot::LEDGE_GRAB;
        ActionResult::Stay
    }
}

/// Let go of the ledge: drop with forward speed -8 (away from the wall,
/// since the facing points at the wall). The grab already snapped Mario to
/// the ledge point 60 units out from the wall, so the position stands.
fn ledge_let_go(cx: &mut ActionCx) -> ActionResult {
    cx.state.forward_speed = -8.0; // spec: ledge.drop_speed (verified)
    cx.state.vel = glam::Vec3::ZERO;
    cx.timeline.slot = slot::FREEFALL;
    cx.goto(ActionId::FREEFALL, 0)
}

/// Pull up onto the ledge after a climb: move forward (toward the wall,
/// onto the ledge top) and snap to the floor, then stand.
fn ledge_pull_up(cx: &mut ActionCx, distance: f32) -> ActionResult {
    let (fx, fz) = cx.forward_xz();
    cx.state.pos.x += fx * distance;
    cx.state.pos.z += fz * distance;
    if let Some(f) = cx.world.find_floor(cx.state.pos, 10.0) {
        cx.state.pos.y = f.y;
        cx.state.floor_y = Some(f.y);
    }
    cx.timeline.slot = super::ground::slot::IDLE;
    cx.goto(ActionId::IDLE, 0)
}

/// Fast ledge climb: an 8-frame pull-up onto the ledge.
pub struct LedgeClimbFast;
impl ActionHandler for LedgeClimbFast {
    fn name(&self) -> &'static str {
        "LedgeClimbFast"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // 60 units over 8 frames lands fully on the ledge top.
        let (fx, fz) = cx.forward_xz();
        cx.state.pos.x += fx * 7.5;
        cx.state.pos.z += fz * 7.5;
        cx.timeline.slot = slot::LEDGE_GRAB;
        if cx.state.action_timer >= 8 {
            return ledge_pull_up(cx, 0.0);
        }
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
        cx.timeline.slot = slot::LEDGE_GRAB;
        if cx.state.action_timer >= 17 {
            return cx.goto(ActionId::LEDGE_CLIMB_SLOW_2, 0);
        }
        ActionResult::Stay
    }
}

/// Slow ledge climb, part 2: at 11 frames in (28 total) any input pulls
/// up 14 units forward onto the ledge. A long no-input stall falls back
/// to idle so the character can never soft-lock mid-climb.
pub struct LedgeClimbSlow2;
impl ActionHandler for LedgeClimbSlow2 {
    fn name(&self) -> &'static str {
        "LedgeClimbSlow2"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        cx.timeline.slot = slot::LEDGE_GRAB;
        if cx.state.action_timer >= 11 && (cx.input.stick_held() || cx.input.buttons != 0) {
            return ledge_pull_up(cx, 14.0);
        }
        if cx.state.action_timer >= 120 {
            return ledge_pull_up(cx, 14.0);
        }
        ActionResult::Stay
    }
}
