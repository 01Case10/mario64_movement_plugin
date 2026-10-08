//! Ground actions: Idle, Walking, TurningAround, Braking, Decelerating, Landing.
//!
//! Walking behavior follows wiki:Walking@19299 (see spec/actions/walking.md).
//! Actions without a community page (Braking, Decelerating, Landing) use
//! provisional IDs and clearly-marked working assumptions.

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
}

/// Shared ground-tick prelude: refresh the floor query. Returns `Some(result)`
/// when the action must yield (walked off the floor).
fn ground_prelude(cx: &mut ActionCx) -> Option<ActionResult> {
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

fn surface_kind_index(kind: SurfaceKind) -> u8 {
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
            return super::air::enter_jump(cx);
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
        if cx.pressed(buttons::A, prev_buttons) {
            return super::air::enter_jump(cx);
        }
        // B dive: phase 4 (needs dive action); noted, not implemented.
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
        if let Some(f) = cx.world.find_floor(cx.state.pos, 1.0) {
            let steep = (1.0 - f.normal.y).max(0.0);
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
        if cx.pressed(buttons::A, prev_buttons) {
            return super::air::enter_jump(cx);
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
        if cx.pressed(buttons::A, prev_buttons) {
            return super::air::enter_jump(cx);
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
        if cx.pressed(buttons::A, prev_buttons) {
            return super::air::enter_jump(cx);
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
        if cx.pressed(buttons::A, prev_buttons) {
            return super::air::enter_jump(cx);
        }
        const DURATION: u32 = 4;
        // Scrub remaining speed quickly (verify rate).
        cx.state.forward_speed *= 0.5;
        if cx.state.forward_speed < 0.5 {
            cx.state.forward_speed = 0.0;
        }
        apply_ground_move(cx);
        if cx.state.action_timer >= DURATION {
            if cx.input.stick_held() {
                return enter_walking(cx);
            }
            cx.timeline.slot = slot::IDLE;
            return cx.goto(ActionId::IDLE, 0);
        }
        cx.timeline.slot = slot::LAND;
        ActionResult::Stay
    }
}
