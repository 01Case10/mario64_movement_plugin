//! Ground actions: Idle, Walking, TurningAround, Braking, Decelerating, Landing.
//!
//! Walking behavior follows wiki:Walking@19299 (see spec/actions/walking.md).
//! Actions without a community page (Braking, Decelerating, Landing) use
//! provisional IDs and clearly-marked working assumptions.

use super::air::{
    enter_backflip, enter_dive, enter_double_jump, enter_jump, enter_long_jump, enter_side_flip,
    enter_triple_jump,
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

/// Apply the velocity through the ground step routine and record contacts.
fn apply_ground_move(cx: &mut ActionCx) {
    let (vx, vz) = ground_velocity(cx);
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
        // Dive: B with speed >= 29 and stick mag > 48. (wiki:Dive@19303)
        if cx.pressed(buttons::B, prev_buttons)
            && cx.state.forward_speed >= 29.0
            && cx.intended_magnitude() > 48.0
        {
            return enter_dive(cx, true);
        }
        // Crouch: Z held. (wiki:Crouching@17805)
        if cx.pressed(buttons::Z, prev_buttons) {
            return enter_crouch(cx);
        }
        // Slide: steep/slippery floor kind -> butt slide.
        if let Some(r) = check_slide(cx) {
            return r;
        }
        let intended = cx.intended_yaw();
        let mag = cx.intended_magnitude();
        let speed = cx.state.forward_speed;
        if let Some(iy) = intended {
            let turn_away = cx.state.face_yaw.diff_to(iy).abs() > Angle::QUARTER_TURN.0 as i32;
            if turn_away && speed >= p.walk_turnaround_speed {
                cx.timeline.slot = slot::TURN;
                return cx.goto(ActionId::TURNING_AROUND, 0);
            }
        }
        if !cx.input.stick_held() {
            if speed >= p.walk_brake_speed {
                cx.timeline.slot = slot::BRAKE;
                return cx.goto(ActionId::BRAKING, 0);
            } else {
                cx.timeline.slot = slot::DECEL;
                return cx.goto(ActionId::DECELERATING, 0);
            }
        }
        // --- Body: speed update (wiki:Walking@19299). ---
        let target = mag.min(p.walk_target_speed_cap);
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
        // Slope: downhill/uphill adjustment scaled by steepness.
        // Steepness is the horizontal magnitude of the (unit) surface normal:
        // sqrt(nx^2 + nz^2). (Pure vector math; 1 - ny understates it.)
        if let Some(f) = cx.world.find_floor(cx.state.pos, 1.0) {
            let steep = (f.normal.x * f.normal.x + f.normal.z * f.normal.z).sqrt();
            if steep > 1e-4 {
                let downhill = (-f.normal.x, -f.normal.z);
                let (fx, fz) = cx.forward_xz();
                let along = fx * downhill.0 + fz * downhill.1;
                let sign = if along >= 0.0 { 1.0 } else { -1.0 };
                nspeed += p.slope_accel(f.kind) * steep * sign;
            }
        }
        cx.state.forward_speed = nspeed;
        // Facing approaches the intended yaw at up to walk_turn_rate/frame.
        if let Some(iy) = intended {
            cx.state.face_yaw = cx.state.face_yaw.approach(iy, p.walk_turn_rate);
        }
        apply_ground_move(cx);
        cx.timeline.slot = slot::WALK;
        ActionResult::Stay
    }
}

/// TurningAround: stick held back at speed >= 16.
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
        let speed = cx.state.forward_speed;
        if !cx.input.stick_held() {
            let slot = if speed >= cx.params.walk_brake_speed {
                slot::BRAKE
            } else {
                slot::DECEL
            };
            let id = if speed >= cx.params.walk_brake_speed {
                ActionId::BRAKING
            } else {
                ActionId::DECELERATING
            };
            cx.timeline.slot = slot;
            return cx.goto(id, 0);
        }
        // Turn toward the stick at the (verify) turnaround rate until aligned.
        // spec: actions.turning_around.turn_rate (verify)
        const TURN_RATE: u16 = 0x1000;
        if let Some(iy) = cx.intended_yaw() {
            cx.state.face_yaw = cx.state.face_yaw.approach(iy, TURN_RATE);
            if cx.state.face_yaw.diff_to(iy).abs() <= cx.params.walk_turn_rate as i32 {
                return enter_walking(cx);
            }
        }
        apply_ground_move(cx);
        cx.timeline.slot = slot::TURN;
        ActionResult::Stay
    }
}

/// Braking: hard stop from speed >= 16 with neutral stick.
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
        // Dive: B with speed >= 29 and stick mag > 48. (wiki:Dive@19303)
        if cx.pressed(buttons::B, prev_buttons)
            && cx.state.forward_speed >= 29.0
            && cx.intended_magnitude() > 48.0
        {
            return enter_dive(cx, true);
        }
        // Crouch: Z held. (wiki:Crouching@17805)
        if cx.pressed(buttons::Z, prev_buttons) {
            return enter_crouch(cx);
        }
        if cx.input.stick_held() {
            return enter_walking(cx);
        }
        // spec: actions.braking.decel (verify)
        const DECEL: f32 = 4.0;
        let speed = (cx.state.forward_speed - DECEL).max(0.0);
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

/// Decelerating: gentle stop from speed < 16 with neutral stick.
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
        // Dive: B with speed >= 29 and stick mag > 48. (wiki:Dive@19303)
        if cx.pressed(buttons::B, prev_buttons)
            && cx.state.forward_speed >= 29.0
            && cx.intended_magnitude() > 48.0
        {
            return enter_dive(cx, true);
        }
        // Crouch: Z held. (wiki:Crouching@17805)
        if cx.pressed(buttons::Z, prev_buttons) {
            return enter_crouch(cx);
        }
        if cx.input.stick_held() {
            return enter_walking(cx);
        }
        // spec: actions.decelerating.decel (verify)
        const DECEL: f32 = 0.5;
        let speed = (cx.state.forward_speed - DECEL).max(0.0);
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

/// Landing: brief recovery after touching down from the air.
/// spec: actions.landing.duration_frames (verify)
pub struct Landing;
impl ActionHandler for Landing {
    fn name(&self) -> &'static str {
        "Landing"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = ground_prelude(cx) {
            return r;
        }
        // Jump chaining: single/sideflip/freefall land -> double jump,
        // double jump land with speed > 20 -> triple jump, else single jump.
        // (wiki:Double Jump@18964, wiki:Triple Jump@19300)
        if cx.pressed(buttons::A, prev_buttons) {
            return match cx.state.land_from {
                ActionId::JUMP | ActionId::SIDE_FLIP | ActionId::FREEFALL => enter_double_jump(cx),
                ActionId::DOUBLE_JUMP if cx.state.forward_speed > 20.0 => enter_triple_jump(cx),
                _ => enter_jump(cx),
            };
        }
        const DURATION: u32 = 4;
        // Forward speed is preserved through the landing (the triple jump
        // chain needs speed > 20 after a double-jump land); the exact
        // recovery behavior is (verify).
        apply_ground_move(cx);
        if cx.state.action_timer >= DURATION {
            if cx.input.stick_held() {
                return enter_walking(cx);
            }
            // No stick: roll into a decel rather than stopping dead.
            cx.timeline.slot = slot::DECEL;
            return cx.goto(ActionId::DECELERATING, 0);
        }
        cx.timeline.slot = slot::LAND;
        ActionResult::Stay
    }
}

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

/// Slide check for ground actions: Slide-kind floor -> butt slide.
fn check_slide(cx: &mut ActionCx) -> Option<ActionResult> {
    if cx
        .world
        .find_floor(cx.state.pos, 1.0)
        .is_some_and(|f| f.kind == SurfaceKind::Slide)
    {
        return Some(enter_butt_slide(cx));
    }
    None
}

/// Butt slide entry: keep speed and facing.
fn enter_butt_slide(cx: &mut ActionCx) -> ActionResult {
    cx.timeline.slot = slot::BUTT_SLIDE;
    cx.goto(ActionId::BUTT_SLIDE, 0)
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
        let floor = cx.world.find_floor(cx.state.pos, 1.0);
        let on_slide = floor.is_some_and(|f| f.kind == SurfaceKind::Slide);
        if !on_slide {
            // Left the slide: walk or decel out.
            if cx.input.stick_held() {
                return enter_walking(cx);
            }
            cx.timeline.slot = slot::DECEL;
            return cx.goto(ActionId::DECELERATING, 0);
        }
        let n = floor.unwrap().normal;
        let steep = (n.x * n.x + n.z * n.z).sqrt();
        // Downhill direction (xz).
        let mut dh = (-n.x, -n.z);
        let dh_len = (dh.0 * dh.0 + dh.1 * dh.1).sqrt();
        if dh_len > 1e-4 {
            dh.0 /= dh_len;
            dh.1 /= dh_len;
        }
        let (fx, fz) = cx.forward_xz();
        let along = fx * dh.0 + fz * dh.1;
        // spec: slide.downhill_accel / slide.friction (verify)
        if steep > 0.02 {
            cx.state.forward_speed +=
                cx.params.slide_downhill_accel * steep * along.signum().max(0.2);
        } else {
            cx.state.forward_speed = (cx.state.forward_speed - 1.0).max(0.0);
        }
        cx.state.forward_speed = cx.state.forward_speed.min(48.0);
        // Limited steering while sliding (verify rate).
        if let Some(iy) = cx.intended_yaw() {
            cx.state.face_yaw = cx.state.face_yaw.approach(iy, 0x400);
        } else if dh_len > 1e-4 {
            let dh_yaw = crate::trig::atan2(dh.0, dh.1);
            cx.state.face_yaw = cx.state.face_yaw.approach(dh_yaw, 0x400);
        }
        if cx.state.forward_speed < 2.0 {
            if cx.input.stick_held() {
                return enter_walking(cx);
            }
            cx.timeline.slot = slot::IDLE;
            return cx.goto(ActionId::IDLE, 0);
        }
        apply_ground_move(cx);
        cx.timeline.slot = slot::BUTT_SLIDE;
        ActionResult::Stay
    }
}
