//! Ground actions: Idle, Walking, TurningAround, FinishTurningAround, Braking,
//! Decelerating, the per-action landings, Crouch, Crawl, and the slide family
//! (ButtSlide, StomachSlide, DiveSlide, CrouchSlide, SlideKickSlide).
//!
//! Walking behavior follows the decomp-documented model (behavioral
//! reference only): accel 1.1 - speed/43 toward min(intendedMag, 32),
//! -1.0/frame above target on flat ground, 0x800/frame turn rate.

use super::air::{
    enter_backflip, enter_dive, enter_double_jump, enter_jump, enter_long_jump, enter_side_flip,
    enter_slide_kick, enter_triple_jump,
};
use super::{ActionCx, ActionHandler, ActionResult};
use crate::angles::Angle;
use crate::events::Event;
use crate::input::buttons;
use crate::state::ActionId;
use crate::step::step_ground;
use crate::world::SurfaceKind;

/// Animation slot ids (owned by the core; see stepkit-anim manifest).
pub mod slot {
    pub const IDLE: u32 = 0;
    pub const WALK: u32 = 1;
    pub const TURN: u32 = 2;
    pub const BRAKE: u32 = 3;
    pub const DECEL: u32 = 4;
    pub const LAND: u32 = 5;
    pub const CROUCH: u32 = 15;
    pub const CRAWL: u32 = 16;
    pub const BUTT_SLIDE: u32 = 19;
    pub const DIVE_SLIDE: u32 = 23;
    pub const CROUCH_SLIDE: u32 = 24;
    pub const STOMACH_SLIDE: u32 = 25;
    pub const SLIDE_KICK_SLIDE: u32 = 26;
    pub const FINISH_TURN: u32 = 27;
}

/// Shared ground-tick prelude: refresh the floor query. Returns `Some(result)`
/// when the action must yield (walked off the floor).
fn ground_prelude(cx: &mut ActionCx) -> Option<ActionResult> {
    // Water plunge: more than 100 below the surface.
    // (verified: wiki:Water Plunge, rev 2023-07-27)
    if let Some(wl) = crate::actions::water::water_plunge_surface(cx) {
        return Some(crate::actions::water::enter_water_plunge(cx, wl));
    }
    let floor = cx.world.find_floor(cx.state.pos, 1.0);
    match floor {
        Some(f) => {
            if f.y < cx.state.pos.y - cx.params.ground_step_down {
                // Floor too far below: walked off.
                return Some(cx.goto(ActionId::FREEFALL, 0));
            }
            cx.state.floor_y = Some(f.y);
            cx.state.floor_kind = surface_kind_index(f.kind);
            // Snap small penetrations; the step routine handles the rest.
            if cx.state.pos.y < f.y && f.y - cx.state.pos.y <= cx.params.ground_step_up {
                cx.state.pos.y = f.y;
            }
            None
        }
        None => Some(cx.goto(ActionId::FREEFALL, 0)),
    }
}

pub(crate) fn surface_kind_index(kind: SurfaceKind) -> u8 {
    match kind {
        SurfaceKind::Default => 0,
        SurfaceKind::Slide => 1,
        SurfaceKind::Quicksand => 2,
        SurfaceKind::Custom(n) => 3 + n % 128,
    }
}

/// Horizontal velocity from facing and forward speed.
fn ground_velocity(cx: &ActionCx) -> (f32, f32) {
    let (fx, fz) = cx.forward_xz();
    (fx * cx.state.forward_speed, fz * cx.state.forward_speed)
}

/// Move toward zero at `rate` per frame, preserving sign.
fn approach_zero(speed: f32, rate: f32) -> f32 {
    if speed > 0.0 {
        (speed - rate).max(0.0)
    } else {
        (speed + rate).min(0.0)
    }
}

/// Downhill/uphill speed adjustment for the current floor, scaled by
/// steepness (the horizontal magnitude of the surface normal).
/// The normal's horizontal projection points downhill, so facing along it
/// gains speed and facing against it loses speed.
fn slope_speed_delta(cx: &ActionCx) -> f32 {
    let Some(f) = cx.world.find_floor(cx.state.pos, 1.0) else {
        return 0.0;
    };
    let steep = (f.normal.x * f.normal.x + f.normal.z * f.normal.z).sqrt();
    if steep <= 1e-4 {
        return 0.0;
    }
    let (fx, fz) = cx.forward_xz();
    let along = fx * f.normal.x + fz * f.normal.z;
    let sign = if along >= 0.0 { 1.0 } else { -1.0 };
    cx.params.slope_accel(f.kind) * steep * sign
}

/// Apply an explicit horizontal velocity through the ground step routine
/// and record contacts.
fn apply_ground_move_with(cx: &mut ActionCx, vx: f32, vz: f32) {
    cx.state.vel.x = vx;
    cx.state.vel.z = vz;
    cx.state.vel.y = 0.0;
    let out = step_ground(&cx.state, vx, vz, cx.world, cx.params);
    cx.state.pos = out.pos;
    cx.state.floor_y = out.floor.map(|f| f.y);
    if let Some(f) = out.floor {
        cx.state.floor_kind = surface_kind_index(f.kind);
    }
    if out.walked_off {
        // Leave the transition to the next tick's prelude so the event
        // ordering stays consistent (one transition per tick).
    }
    if out.wall_hit {
        cx.state.wall_hit = true;
        cx.events.push(Event::WallHit {
            normal_yaw: crate::trig::atan2(out.wall_normal.x, out.wall_normal.z),
        });
    } else {
        cx.state.wall_hit = false;
    }
}

/// Apply the velocity through the ground step routine and record contacts.
fn apply_ground_move(cx: &mut ActionCx) {
    let (vx, vz) = ground_velocity(cx);
    apply_ground_move_with(cx, vx, vz);
}

/// Stick released while moving: BRAKING at speed >= 16 on walkable slopes,
/// DECELERATING otherwise (floor normal.y >= 0.1736 is ~80 degrees).
fn begin_braking_action(cx: &mut ActionCx) -> ActionResult {
    let steep_ok = cx
        .world
        .find_floor(cx.state.pos, 1.0)
        .is_some_and(|f| f.normal.y >= 0.1736);
    if cx.state.forward_speed >= cx.params.walk_brake_speed && steep_ok {
        cx.timeline.slot = slot::BRAKE;
        cx.goto(ActionId::BRAKING, 0)
    } else {
        cx.timeline.slot = slot::DECEL;
        cx.goto(ActionId::DECELERATING, 0)
    }
}

/// The walking speed model: accel toward min(intendedMag, cap), flat-ground
/// falloff above target, slope adjustment, facing approach, then move.
/// Shared by Walking and FinishTurningAround.
fn walk_body(cx: &mut ActionCx) {
    let p = cx.params;
    let mag = cx.intended_magnitude();
    let target = mag.min(p.walk_target_speed_cap);
    let speed = cx.state.forward_speed;
    let mut nspeed = speed;
    if nspeed <= 0.0 {
        nspeed += p.walk_speedup_base;
    } else if nspeed <= target {
        nspeed += p.walk_speedup_base - nspeed / p.walk_speedup_falloff_divisor;
    } else if cx
        .world
        .find_floor(cx.state.pos, 1.0)
        .is_some_and(|f| f.normal.y >= 0.95)
    {
        nspeed -= p.walk_decel_flat_ground;
    }
    nspeed = nspeed.min(p.walk_hard_cap);
    nspeed += slope_speed_delta(cx);
    cx.state.forward_speed = nspeed;
    // Facing approaches the intended yaw at up to walk_turn_rate/frame.
    if let Some(iy) = cx.intended_yaw() {
        cx.state.face_yaw = cx.state.face_yaw.approach(iy, p.walk_turn_rate);
    }
    apply_ground_move(cx);
    cx.timeline.slot = slot::WALK;
}

/// Transition into Walking, applying the documented entry clamp:
/// if the floor is not very slippery and speed < min(intended, 8),
/// speed becomes min(intended, 8). (wiki:Walking@19299)
fn enter_walking(cx: &mut ActionCx) -> ActionResult {
    let slippery = matches!(
        cx.world.find_floor(cx.state.pos, 1.0).map(|f| f.kind),
        Some(SurfaceKind::Slide)
    );
    if !slippery {
        let clamp = cx
            .intended_magnitude()
            .min(cx.params.walk_enter_speed_clamp);
        if cx.state.forward_speed < clamp {
            cx.state.forward_speed = clamp;
        }
    }
    cx.timeline.slot = slot::WALK;
    cx.goto(ActionId::WALKING, 0)
}

/// Idle: standing still on the ground.
pub struct Idle;
impl ActionHandler for Idle {
    fn name(&self) -> &'static str {
        "Idle"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = ground_prelude(cx) {
            return r;
        }
        // Cancel checks.
        if cx.pressed(buttons::A, prev_buttons) {
            return enter_jump(cx);
        }
        // Crouch: Z held. (wiki:Crouching@17805)
        if cx.pressed(buttons::Z, prev_buttons) {
            return enter_crouch(cx);
        }
        if cx.input.stick_held() {
            return enter_walking(cx);
        }
        // Body: stand still.
        cx.state.forward_speed = 0.0;
        cx.state.vel.x = 0.0;
        cx.state.vel.z = 0.0;
        cx.timeline.slot = slot::IDLE;
        ActionResult::Stay
    }
}

/// Walking: stick-held ground locomotion at any speed.
/// Spec: spec/actions/walking.md (wiki:Walking@19299).
pub struct Walking;
impl ActionHandler for Walking {
    fn name(&self) -> &'static str {
        "Walking"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = ground_prelude(cx) {
            return r;
        }
        let p = cx.params;
        // --- Cancel checks (documented order). ---
        // Long jump: A + Z while moving fast (entry threshold (verify)).
        if cx.pressed(buttons::A, prev_buttons)
            && cx.input.buttons & buttons::Z != 0
            && cx.state.forward_speed >= 16.0
        {
            return enter_long_jump(cx);
        }
        if cx.pressed(buttons::A, prev_buttons) {
            return enter_jump(cx);
        }
        // Dive: B with speed >= 29 and raw stick mag > 48.
        if cx.pressed(buttons::B, prev_buttons)
            && cx.state.forward_speed >= 29.0
            && cx.raw_magnitude() > 48.0
        {
            return enter_dive(cx, true);
        }
        // Crouch: Z while moving -> crouch slide; Z at rest -> crouch.
        if cx.pressed(buttons::Z, prev_buttons) {
            if cx.state.forward_speed > 0.0 {
                return enter_crouch_slide(cx);
            }
            return enter_crouch(cx);
        }
        // Slide: steep/slippery floor kind -> butt or stomach slide.
        if let Some(r) = check_slide(cx) {
            return r;
        }
        let intended = cx.intended_yaw();
        let speed = cx.state.forward_speed;
        if let Some(iy) = intended {
            let turn_away = cx.state.face_yaw.diff_to(iy).abs() > Angle::QUARTER_TURN.0 as i32;
            if turn_away && speed >= p.walk_turnaround_speed {
                cx.timeline.slot = slot::TURN;
                return cx.goto(ActionId::TURNING_AROUND, 0);
            }
        }
        if !cx.input.stick_held() {
            return begin_braking_action(cx);
        }
        // --- Body: the shared walking speed model. ---
        walk_body(cx);
        ActionResult::Stay
    }
}

/// TurningAround: stick held back at speed >= 16. When the facing aligns
/// with the stick, hand off to FinishTurningAround (which re-accelerates).
pub struct TurningAround;
impl ActionHandler for TurningAround {
    fn name(&self) -> &'static str {
        "TurningAround"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = ground_prelude(cx) {
            return r;
        }
        // A during TurningAround -> side flip. (wiki:Side Flip@20374)
        if cx.pressed(buttons::A, prev_buttons) {
            return enter_side_flip(cx);
        }
        if cx.pressed(buttons::Z, prev_buttons) {
            return enter_crouch(cx);
        }
        if !cx.input.stick_held() {
            return begin_braking_action(cx);
        }
        // Turn toward the stick at the walking turn rate until aligned.
        if let Some(iy) = cx.intended_yaw() {
            cx.state.face_yaw = cx.state.face_yaw.approach(iy, cx.params.walk_turn_rate);
            if cx.state.face_yaw.diff_to(iy).abs() <= cx.params.walk_turn_rate as i32 {
                cx.timeline.slot = slot::FINISH_TURN;
                return cx.goto(ActionId::FINISH_TURNING_AROUND, 0);
            }
        }
        apply_ground_move(cx);
        cx.timeline.slot = slot::TURN;
        ActionResult::Stay
    }
}

/// FinishTurningAround: the turn is done; re-accelerate with the walking
/// speed model while the facing settles onto the stick. The reference game
/// spins the *rendered* yaw 0x8000/frame here -- purely visual, so the sim
/// just turns the facing at the walk rate.
pub struct FinishTurningAround;
impl ActionHandler for FinishTurningAround {
    fn name(&self) -> &'static str {
        "FinishTurningAround"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = ground_prelude(cx) {
            return r;
        }
        if cx.pressed(buttons::A, prev_buttons) {
            return enter_side_flip(cx);
        }
        if let Some(r) = check_slide(cx) {
            return r;
        }
        if !cx.input.stick_held() {
            return begin_braking_action(cx);
        }
        walk_body(cx);
        cx.timeline.slot = slot::FINISH_TURN;
        // End once the facing has settled onto the stick.
        if let Some(iy) = cx.intended_yaw() {
            if cx.state.face_yaw.diff_to(iy).abs() <= cx.params.walk_turn_rate as i32 {
                return enter_walking(cx);
            }
        }
        ActionResult::Stay
    }
}

/// Braking: hard stop with neutral stick. Speed approaches 0 at 2.0/frame
/// plus the usual slope adjustment.
pub struct Braking;
impl ActionHandler for Braking {
    fn name(&self) -> &'static str {
        "Braking"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = ground_prelude(cx) {
            return r;
        }
        if let Some(r) = check_slide(cx) {
            return r;
        }
        // Long jump: A + Z while moving fast (entry threshold (verify)).
        if cx.pressed(buttons::A, prev_buttons)
            && cx.input.buttons & buttons::Z != 0
            && cx.state.forward_speed >= 16.0
        {
            return enter_long_jump(cx);
        }
        if cx.pressed(buttons::A, prev_buttons) {
            return enter_jump(cx);
        }
        // Dive: B with speed >= 29 and raw stick mag > 48.
        if cx.pressed(buttons::B, prev_buttons)
            && cx.state.forward_speed >= 29.0
            && cx.raw_magnitude() > 48.0
        {
            return enter_dive(cx, true);
        }
        // Crouch: Z while moving -> crouch slide; Z at rest -> crouch.
        if cx.pressed(buttons::Z, prev_buttons) {
            if cx.state.forward_speed > 0.0 {
                return enter_crouch_slide(cx);
            }
            return enter_crouch(cx);
        }
        if cx.input.stick_held() {
            return enter_walking(cx);
        }
        let speed = approach_zero(cx.state.forward_speed, 2.0) + slope_speed_delta(cx);
        cx.state.forward_speed = speed;
        apply_ground_move(cx);
        if speed <= 0.0 {
            cx.timeline.slot = slot::IDLE;
            return cx.goto(ActionId::IDLE, 0);
        }
        cx.timeline.slot = slot::BRAKE;
        ActionResult::Stay
    }
}

/// Decelerating: gentle stop with neutral stick. Speed approaches 0 at
/// 1.0/frame (no slope accel, matching the reference).
pub struct Decelerating;
impl ActionHandler for Decelerating {
    fn name(&self) -> &'static str {
        "Decelerating"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = ground_prelude(cx) {
            return r;
        }
        if let Some(r) = check_slide(cx) {
            return r;
        }
        // Long jump: A + Z while moving fast (entry threshold (verify)).
        if cx.pressed(buttons::A, prev_buttons)
            && cx.input.buttons & buttons::Z != 0
            && cx.state.forward_speed >= 16.0
        {
            return enter_long_jump(cx);
        }
        if cx.pressed(buttons::A, prev_buttons) {
            return enter_jump(cx);
        }
        // Dive: B with speed >= 29 and raw stick mag > 48.
        if cx.pressed(buttons::B, prev_buttons)
            && cx.state.forward_speed >= 29.0
            && cx.raw_magnitude() > 48.0
        {
            return enter_dive(cx, true);
        }
        // Crouch: Z while moving -> crouch slide; Z at rest -> crouch.
        if cx.pressed(buttons::Z, prev_buttons) {
            if cx.state.forward_speed > 0.0 {
                return enter_crouch_slide(cx);
            }
            return enter_crouch(cx);
        }
        if cx.input.stick_held() {
            return enter_walking(cx);
        }
        let speed = approach_zero(cx.state.forward_speed, 1.0);
        cx.state.forward_speed = speed;
        apply_ground_move(cx);
        if speed <= 0.0 {
            cx.timeline.slot = slot::IDLE;
            return cx.goto(ActionId::IDLE, 0);
        }
        cx.timeline.slot = slot::DECEL;
        ActionResult::Stay
    }
}

/// How A chains out of a landing.
enum LandChain {
    /// A -> double jump.
    DoubleJump,
    /// A -> triple jump if speed > 20, else single jump.
    SpeedChain,
    /// A does nothing.
    Suppressed,
    /// A -> backflip, but only while Z is held (A is stripped otherwise).
    BackflipHeldZ,
    /// A -> long jump, but only while Z is held (A is ignored otherwise).
    LongJumpHeldZ,
}

/// Shared landing tick: per-action frame count, A-chain, and end state.
/// Landing physics: stick held -> speed approaches 0 at 0.98/frame;
/// no stick at speed >= 16 -> slope decel 2.0/frame; floors steeper than
/// normal.y 0.2924 (~73 deg) push the character off instead of landing.
fn landing_tick(
    cx: &mut ActionCx,
    prev_buttons: u16,
    frames: u32,
    chain: LandChain,
    end: ActionId,
) -> ActionResult {
    if let Some(r) = ground_prelude(cx) {
        return r;
    }
    // Record the landing impact speed from the goto arg (fall damage and
    // squish consume this; see plan Phase D).
    cx.state.last_fall_speed = cx.arg() as f32;
    if let Some(f) = cx.world.find_floor(cx.state.pos, 1.0) {
        if f.normal.y < 0.2924 {
            return cx.goto(ActionId::FREEFALL, 0);
        }
    }
    // A chains only during the landing itself (the reference keeps a
    // 5-frame double-jump timer; the landing is 4-6 frames).
    if cx.pressed(buttons::A, prev_buttons) && cx.state.action_timer < frames {
        match chain {
            LandChain::DoubleJump => return enter_double_jump(cx),
            LandChain::SpeedChain => {
                if cx.state.forward_speed > 20.0 {
                    return enter_triple_jump(cx);
                }
                return enter_jump(cx);
            }
            LandChain::Suppressed => {}
            LandChain::BackflipHeldZ => {
                if cx.input.buttons & buttons::Z != 0 {
                    return enter_backflip(cx);
                }
            }
            LandChain::LongJumpHeldZ => {
                if cx.input.buttons & buttons::Z != 0 {
                    return enter_long_jump(cx);
                }
            }
        }
    }
    if cx.input.stick_held() {
        cx.state.forward_speed =
            approach_zero(cx.state.forward_speed, 0.98) + slope_speed_delta(cx);
    } else if cx.state.forward_speed >= 16.0 {
        cx.state.forward_speed = approach_zero(cx.state.forward_speed, 2.0) + slope_speed_delta(cx);
    }
    apply_ground_move(cx);
    if cx.state.action_timer >= frames {
        return cx.goto(end, 0);
    }
    cx.timeline.slot = slot::LAND;
    ActionResult::Stay
}

macro_rules! landing_action {
    ($name:ident, $label:literal, $frames:expr, $chain:expr, $end:expr) => {
        pub struct $name;
        impl ActionHandler for $name {
            fn name(&self) -> &'static str {
                $label
            }
            fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
                landing_tick(cx, prev_buttons, $frames, $chain, $end)
            }
        }
    };
}

landing_action!(
    JumpLand,
    "JumpLand",
    4,
    LandChain::DoubleJump,
    ActionId::IDLE
);
landing_action!(
    DoubleJumpLand,
    "DoubleJumpLand",
    4,
    LandChain::SpeedChain,
    ActionId::IDLE
);
landing_action!(
    TripleJumpLand,
    "TripleJumpLand",
    4,
    LandChain::Suppressed,
    ActionId::IDLE
);
landing_action!(
    BackflipLand,
    "BackflipLand",
    4,
    LandChain::BackflipHeldZ,
    ActionId::IDLE
);
landing_action!(
    SideFlipLand,
    "SideFlipLand",
    4,
    LandChain::DoubleJump,
    ActionId::IDLE
);
landing_action!(
    FreefallLand,
    "FreefallLand",
    4,
    LandChain::DoubleJump,
    ActionId::IDLE
);
landing_action!(
    LongJumpLand,
    "LongJumpLand",
    6,
    LandChain::LongJumpHeldZ,
    ActionId::CROUCH
);

/// Crouch entry. (wiki:Crouching@17805)
fn enter_crouch(cx: &mut ActionCx) -> ActionResult {
    cx.state.forward_speed = 0.0;
    cx.timeline.slot = slot::CROUCH;
    cx.goto(ActionId::CROUCH, 0)
}

/// Crouching: Z held on the ground. A -> backflip, Z released -> stand,
/// stick -> crawl. (wiki:Crouching@17805)
pub struct Crouch;
impl ActionHandler for Crouch {
    fn name(&self) -> &'static str {
        "Crouch"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = ground_prelude(cx) {
            return r;
        }
        if cx.pressed(buttons::A, prev_buttons) {
            return enter_backflip(cx);
        }
        // Z released -> stop crouching.
        if cx.input.buttons & buttons::Z == 0 {
            if cx.intended_yaw().is_some() {
                return enter_walking(cx);
            }
            return cx.goto(ActionId::IDLE, 0);
        }
        // Stick held -> crawl.
        if cx.intended_yaw().is_some() {
            cx.timeline.slot = slot::CRAWL;
            return cx.goto(ActionId::CRAWL, 0);
        }
        cx.timeline.slot = slot::CROUCH;
        ActionResult::Stay
    }
}

/// Crawling: slow ground movement. Numbers are (verify).
pub struct Crawl;
impl ActionHandler for Crawl {
    fn name(&self) -> &'static str {
        "Crawl"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = ground_prelude(cx) {
            return r;
        }
        if cx.input.buttons & buttons::Z == 0 {
            return cx.goto(ActionId::IDLE, 0);
        }
        if cx.pressed(buttons::A, prev_buttons) {
            return enter_jump(cx); // (verify)
        }
        let Some(intended) = cx.intended_yaw() else {
            cx.timeline.slot = slot::CROUCH;
            return cx.goto(ActionId::CROUCH, 0);
        };
        // spec: crawl.target_speed / crawl.accel (verify)
        let target = cx.intended_magnitude().min(10.0);
        let speed = cx.state.forward_speed;
        cx.state.forward_speed = if speed < target {
            (speed + 2.0).min(target)
        } else {
            (speed - 2.0).max(target)
        };
        cx.state.face_yaw = cx.state.face_yaw.approach(intended, 0x800);
        apply_ground_move(cx);
        cx.timeline.slot = slot::CRAWL;
        ActionResult::Stay
    }
}

/// BEGIN_SLIDING dispatcher: the reference game routes slide triggers here,
/// not to a per-frame action. Facing within +/-0x4000 (+/-90 deg) of
/// downhill -> butt slide; otherwise -> stomach slide. On near-flat
/// slide floors the downhill direction is degenerate, so default to butt.
fn begin_sliding(cx: &mut ActionCx) -> ActionResult {
    let downhill_yaw = cx.world.find_floor(cx.state.pos, 1.0).and_then(|f| {
        let steep = (f.normal.x * f.normal.x + f.normal.z * f.normal.z).sqrt();
        if steep > 0.02 {
            // The normal's horizontal projection points downhill.
            Some(crate::trig::atan2(f.normal.x, f.normal.z))
        } else {
            None
        }
    });
    match downhill_yaw {
        Some(dy) if cx.state.face_yaw.diff_to(dy).abs() > 0x4000 => enter_stomach_slide(cx),
        _ => enter_butt_slide(cx),
    }
}

/// Slide check for non-slide ground actions: Slide-kind floor -> the
/// begin-sliding dispatcher.
fn check_slide(cx: &mut ActionCx) -> Option<ActionResult> {
    if cx
        .world
        .find_floor(cx.state.pos, 1.0)
        .is_some_and(|f| f.kind == SurfaceKind::Slide)
    {
        return Some(begin_sliding(cx));
    }
    None
}

/// Initialize the slide vector from the current horizontal velocity.
fn capture_slide_vector(cx: &mut ActionCx) {
    cx.state.slide_vel_x = cx.state.vel.x;
    cx.state.slide_vel_z = cx.state.vel.z;
}

/// Butt slide entry: initialize slide velocity from current motion.
fn enter_butt_slide(cx: &mut ActionCx) -> ActionResult {
    capture_slide_vector(cx);
    cx.timeline.slot = slot::BUTT_SLIDE;
    cx.goto(ActionId::BUTT_SLIDE, 0)
}

/// Stomach slide entry.
fn enter_stomach_slide(cx: &mut ActionCx) -> ActionResult {
    capture_slide_vector(cx);
    cx.timeline.slot = slot::STOMACH_SLIDE;
    cx.goto(ActionId::STOMACH_SLIDE, 0)
}

/// Dive slide entry, from a dive landing: the slide vector is the
/// touchdown velocity.
pub fn enter_dive_slide(cx: &mut ActionCx) -> ActionResult {
    capture_slide_vector(cx);
    cx.timeline.slot = slot::DIVE_SLIDE;
    cx.goto(ActionId::DIVE_SLIDE, 0)
}

/// Slide-kick slide entry, from the second touchdown of a slide kick.
pub fn enter_slide_kick_slide(cx: &mut ActionCx) -> ActionResult {
    capture_slide_vector(cx);
    cx.timeline.slot = slot::SLIDE_KICK_SLIDE;
    cx.goto(ActionId::SLIDE_KICK_SLIDE, 0)
}

/// Crouch slide entry: Z while moving. Captures the current ground
/// velocity as the slide vector.
fn enter_crouch_slide(cx: &mut ActionCx) -> ActionResult {
    let (vx, vz) = ground_velocity(cx);
    cx.state.slide_vel_x = vx;
    cx.state.slide_vel_z = vz;
    cx.timeline.slot = slot::CROUCH_SLIDE;
    cx.goto(ActionId::CROUCH_SLIDE, 0)
}

/// Shared slide-vector physics: downhill accel, input steering of the slide
/// direction, friction, 48 cap. Writes vel from the slide vector (not from
/// facing) and sets forward_speed to the facing projection for
/// compatibility. Returns the slide speed after the update.
fn update_slide_vector(cx: &mut ActionCx) -> f32 {
    let n = cx
        .world
        .find_floor(cx.state.pos, 1.0)
        .map(|f| f.normal)
        .unwrap_or(glam::Vec3::Y);
    let steep = (n.x * n.x + n.z * n.z).sqrt();
    // Downhill direction (xz): the normal's horizontal projection.
    let mut dh = (n.x, n.z);
    let dh_len = (dh.0 * dh.0 + dh.1 * dh.1).sqrt();
    if dh_len > 1e-4 {
        dh.0 /= dh_len;
        dh.1 /= dh_len;
    }
    // Slope accelerates the slide vector downhill.
    if steep > 0.02 {
        let accel = cx.params.slide_downhill_accel * steep;
        cx.state.slide_vel_x += dh.0 * accel;
        cx.state.slide_vel_z += dh.1 * accel;
    }
    // Input steers the slide vector (rotates, does not replace).
    if let Some(iy) = cx.intended_yaw() {
        let mag = cx.intended_magnitude() / 32.0;
        let yaw_rad = iy.0 as f32 / 65536.0 * std::f32::consts::TAU;
        let (ifx, ifz) = (yaw_rad.sin(), yaw_rad.cos());
        // Blend slide direction toward input (5% per frame, scaled by mag).
        let blend = 0.05 * mag;
        let cur_len = (cx.state.slide_vel_x * cx.state.slide_vel_x
            + cx.state.slide_vel_z * cx.state.slide_vel_z)
            .sqrt()
            .max(0.01);
        let cur_dx = cx.state.slide_vel_x / cur_len;
        let cur_dz = cx.state.slide_vel_z / cur_len;
        let new_dx = cur_dx + (ifx - cur_dx) * blend;
        let new_dz = cur_dz + (ifz - cur_dz) * blend;
        let new_len = (new_dx * new_dx + new_dz * new_dz).sqrt().max(0.01);
        cx.state.slide_vel_x = new_dx / new_len * cur_len;
        cx.state.slide_vel_z = new_dz / new_len * cur_len;
        // Face turns toward input independently.
        cx.state.face_yaw = cx.state.face_yaw.approach(iy, 0x400);
    } else if dh_len > 1e-4 {
        let dh_yaw = crate::trig::atan2(dh.0, dh.1);
        cx.state.face_yaw = cx.state.face_yaw.approach(dh_yaw, 0x400);
    }
    // Friction.
    let speed = (cx.state.slide_vel_x * cx.state.slide_vel_x
        + cx.state.slide_vel_z * cx.state.slide_vel_z)
        .sqrt();
    if speed > 0.01 {
        let friction = 0.98; // (verify)
        cx.state.slide_vel_x *= friction;
        cx.state.slide_vel_z *= friction;
    }
    let speed = (cx.state.slide_vel_x * cx.state.slide_vel_x
        + cx.state.slide_vel_z * cx.state.slide_vel_z)
        .sqrt()
        .min(48.0);
    // Forward speed is projection onto facing (for compatibility).
    let (fx, fz) = cx.forward_xz();
    cx.state.forward_speed = cx.state.slide_vel_x * fx + cx.state.slide_vel_z * fz;
    speed
}

/// Butt slide: sliding on steep/slippery ground. All numbers are (verify).
pub struct ButtSlide;
impl ActionHandler for ButtSlide {
    fn name(&self) -> &'static str {
        "ButtSlide"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = ground_prelude(cx) {
            return r;
        }
        if cx.pressed(buttons::A, prev_buttons) {
            return enter_jump(cx); // (verify)
        }
        let on_slide = cx
            .world
            .find_floor(cx.state.pos, 1.0)
            .is_some_and(|f| f.kind == SurfaceKind::Slide);
        if !on_slide {
            // Left the slide: walk or decel out.
            if cx.input.stick_held() {
                return enter_walking(cx);
            }
            cx.timeline.slot = slot::DECEL;
            return cx.goto(ActionId::DECELERATING, 0);
        }
        let speed = update_slide_vector(cx);
        if speed < 2.0 {
            if cx.input.stick_held() {
                return enter_walking(cx);
            }
            cx.timeline.slot = slot::IDLE;
            return cx.goto(ActionId::IDLE, 0);
        }
        apply_ground_move_with(cx, cx.state.slide_vel_x, cx.state.slide_vel_z);
        cx.timeline.slot = slot::BUTT_SLIDE;
        ActionResult::Stay
    }
}

/// Stomach slide: the slide trigger while facing uphill or flat. At
/// action_timer == 5, A or B with no stick held rolls out in the direction
/// of travel; stopping ends idle.
pub struct StomachSlide;
impl ActionHandler for StomachSlide {
    fn name(&self) -> &'static str {
        "StomachSlide"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = ground_prelude(cx) {
            return r;
        }
        if cx.state.action_timer == 5
            && (cx.pressed(buttons::A, prev_buttons) || cx.pressed(buttons::B, prev_buttons))
            && !cx.input.stick_held()
        {
            if cx.state.forward_speed >= 0.0 {
                return super::air::enter_forward_rollout(cx);
            }
            return super::air::enter_backward_rollout(cx);
        }
        let speed = update_slide_vector(cx);
        if speed < 4.0 {
            cx.timeline.slot = slot::IDLE;
            return cx.goto(ActionId::IDLE, 0);
        }
        apply_ground_move_with(cx, cx.state.slide_vel_x, cx.state.slide_vel_z);
        cx.timeline.slot = slot::STOMACH_SLIDE;
        ActionResult::Stay
    }
}

/// Dive slide: the landing from a dive. A or B at any time rolls out in
/// the direction of travel; stopping ends idle.
pub struct DiveSlide;
impl ActionHandler for DiveSlide {
    fn name(&self) -> &'static str {
        "DiveSlide"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = ground_prelude(cx) {
            return r;
        }
        if cx.pressed(buttons::A, prev_buttons) || cx.pressed(buttons::B, prev_buttons) {
            if cx.state.forward_speed >= 0.0 {
                return super::air::enter_forward_rollout(cx);
            }
            return super::air::enter_backward_rollout(cx);
        }
        let speed = update_slide_vector(cx);
        if speed < 8.0 {
            cx.timeline.slot = slot::IDLE;
            return cx.goto(ActionId::IDLE, 0);
        }
        apply_ground_move_with(cx, cx.state.slide_vel_x, cx.state.slide_vel_z);
        cx.timeline.slot = slot::DIVE_SLIDE;
        ActionResult::Stay
    }
}

/// Crouch slide: Z while moving. In the first 30 frames A (speed > 10)
/// goes to long jump and B (speed >= 10) to slide kick; B below 10 drops
/// to crouch. Afterwards A is a plain jump. Stopping ends crouching.
pub struct CrouchSlide;
impl ActionHandler for CrouchSlide {
    fn name(&self) -> &'static str {
        "CrouchSlide"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = ground_prelude(cx) {
            return r;
        }
        // Slide-class floor under a crouch slide -> the slide dispatcher.
        if let Some(r) = check_slide(cx) {
            return r;
        }
        let speed = (cx.state.slide_vel_x * cx.state.slide_vel_x
            + cx.state.slide_vel_z * cx.state.slide_vel_z)
            .sqrt();
        if cx.state.action_timer < 30 {
            if cx.pressed(buttons::A, prev_buttons) && speed > 10.0 {
                return enter_long_jump(cx);
            }
            if cx.pressed(buttons::B, prev_buttons) {
                if speed >= 10.0 {
                    return enter_slide_kick(cx);
                }
                cx.timeline.slot = slot::CROUCH;
                return cx.goto(ActionId::CROUCH, 0);
            }
        } else if cx.pressed(buttons::A, prev_buttons) {
            return enter_jump(cx);
        }
        let speed = update_slide_vector(cx);
        if speed < 4.0 {
            cx.timeline.slot = slot::CROUCH;
            return cx.goto(ActionId::CROUCH, 0);
        }
        apply_ground_move_with(cx, cx.state.slide_vel_x, cx.state.slide_vel_z);
        cx.timeline.slot = slot::CROUCH_SLIDE;
        ActionResult::Stay
    }
}

/// Slide-kick slide: ground continuation of an airborne slide kick.
/// A rolls forward; a wall stops into a decel (Phase E routes bonks to
/// BACKWARD_GROUND_KB); stopping ends crouching.
pub struct SlideKickSlide;
impl ActionHandler for SlideKickSlide {
    fn name(&self) -> &'static str {
        "SlideKickSlide"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = ground_prelude(cx) {
            return r;
        }
        if cx.pressed(buttons::A, prev_buttons) {
            return super::air::enter_forward_rollout(cx);
        }
        let speed = update_slide_vector(cx);
        apply_ground_move_with(cx, cx.state.slide_vel_x, cx.state.slide_vel_z);
        if cx.state.wall_hit {
            cx.timeline.slot = slot::DECEL;
            return cx.goto(ActionId::DECELERATING, 0);
        }
        if speed < 1.0 {
            cx.timeline.slot = slot::CROUCH;
            return cx.goto(ActionId::CROUCH, 0);
        }
        cx.timeline.slot = slot::SLIDE_KICK_SLIDE;
        ActionResult::Stay
    }
}
