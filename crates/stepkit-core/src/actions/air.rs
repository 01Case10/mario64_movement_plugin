//! Air actions: SingleJump and Freefall.
//!
//! Single jump follows wiki:Single Jump@19309 and wiki:Gravity@20294
//! (see spec/actions/jump.md). Freefall shares the air step; the jump
//! chain (double/triple), long jump, backflip, side flip, dive, and ground
//! pound arrive in phase 3.

use super::{ActionCx, ActionHandler, ActionResult};
use crate::events::Event;
use crate::input::buttons;
use crate::state::ActionId;
use crate::step::step_air;
use glam::Vec3;

/// Animation slot ids for air actions.
pub mod slot {
    pub const JUMP: u32 = 6;
    pub const FREEFALL: u32 = 7;
}

/// Shared jump entry: vertical speed = 42 + forward_speed/4, forward speed
/// multiplied by 0.8. (wiki:Single Jump@19309)
pub fn enter_jump(cx: &mut ActionCx) -> ActionResult {
    let p = cx.params;
    let vy = p.jump_vertical_base + cx.state.forward_speed * p.jump_vertical_forward_factor;
    cx.state.forward_speed *= p.jump_forward_retain;
    cx.state.vel = Vec3::new(cx.state.vel.x, vy, cx.state.vel.z);
    cx.state.vel.x = cx.forward_xz().0 * cx.state.forward_speed;
    cx.state.vel.z = cx.forward_xz().1 * cx.state.forward_speed;
    cx.timeline.slot = slot::JUMP;
    cx.events.push(Event::Jumped { velocity_y: vy });
    cx.goto(ActionId::JUMP, 0)
}

/// Apply one air tick's physics and step; returns `Some(result)` on landing.
fn air_prelude(cx: &mut ActionCx, height_control: bool) -> Option<ActionResult> {
    let p = cx.params;
    // Jump-height control: A released while rising fast quarters vy.
    // (wiki:Gravity@20294; the exact repeat behavior is (verify))
    if height_control
        && cx.input.buttons & buttons::A == 0
        && cx.state.vel.y > p.jump_height_control_threshold
    {
        cx.state.vel.y /= 4.0;
    }
    // Gravity, clamped to the terminal velocity.
    cx.state.vel.y = (cx.state.vel.y - p.gravity).max(p.terminal_velocity);
    // Air steering: facing approaches the intended yaw (verify rate).
    // spec: air.steer_rate (verify)
    const STEER_RATE: u16 = 0x800;
    if let Some(iy) = cx.intended_yaw() {
        cx.state.face_yaw = cx.state.face_yaw.approach(iy, STEER_RATE);
    }
    // Horizontal velocity follows facing and forward speed.
    let (fx, fz) = cx.forward_xz();
    cx.state.vel.x = fx * cx.state.forward_speed;
    cx.state.vel.z = fz * cx.state.forward_speed;
    let out = step_air(&cx.state, cx.world, cx.params);
    cx.state.pos = out.pos;
    cx.state.vel.y = out.vel_y;
    cx.state.floor_y = out.floor.map(|f| f.y);
    if out.wall_hit {
        cx.state.wall_hit = true;
        cx.events.push(Event::WallHit {
            normal_yaw: crate::trig::atan2(out.wall_normal.x, out.wall_normal.z),
        });
        // Wall-kick window: phase 4. For now the window timer simply runs.
        cx.state.wall_kick_timer = 10; // spec: wall.kick_window (verify)
    } else {
        cx.state.wall_hit = false;
        cx.state.wall_kick_timer = cx.state.wall_kick_timer.saturating_sub(1);
    }
    if let Some(f) = out.floor {
        let fall_speed = out.fall_speed;
        // forward_speed unchanged on landing (verify)
        cx.events.push(Event::Landed { fall_speed });
        cx.state.floor_kind = match f.kind {
            crate::world::SurfaceKind::Default => 0,
            crate::world::SurfaceKind::Slide => 1,
            crate::world::SurfaceKind::Quicksand => 2,
            crate::world::SurfaceKind::Custom(n) => 3 + n % 128,
        };
        cx.timeline.slot = super::ground::slot::LAND;
        return Some(cx.goto(ActionId::LANDING, fall_speed.max(0.0) as u32));
    }
    None
}

/// SingleJump: the basic A-press jump.
pub struct SingleJump;
impl ActionHandler for SingleJump {
    fn name(&self) -> &'static str {
        "SingleJump"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        if let Some(r) = air_prelude(cx, true) {
            return r;
        }
        // Dive / ground pound: phase 3.
        cx.timeline.slot = slot::JUMP;
        ActionResult::Stay
    }
}

/// Freefall: walked off a ledge (or a jump that expired).
pub struct Freefall;
impl ActionHandler for Freefall {
    fn name(&self) -> &'static str {
        "Freefall"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        if let Some(r) = air_prelude(cx, false) {
            return r;
        }
        cx.timeline.slot = slot::FREEFALL;
        ActionResult::Stay
    }
}
