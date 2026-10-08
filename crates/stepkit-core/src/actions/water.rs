//! Water actions: plunge entry and swimming.
//!
//! Water Plunge (0x300022E2) entry is documented (wiki:Water Plunge,
//! rev 2023-07-27): when Mario's height is more than 100 below the water
//! surface, forward velocity is quartered, vertical velocity halved, height
//! is set to 100 below the surface, and the action becomes water plunge.
//! Swimming locomotion numbers are working assumptions (verify).

use super::{ActionCx, ActionHandler, ActionResult};
use crate::input::buttons;
use crate::state::ActionId;

pub mod slot {
    pub const WATER_PLUNGE: u32 = 21;
    pub const SWIMMING: u32 = 22;
}

pub mod id {
    use crate::state::ActionId;
    /// Verified: wiki:Water Plunge (rev 2023-07-27).
    pub const WATER_PLUNGE: ActionId = ActionId(0x300022E2);
    /// STEPKIT-PROVISIONAL (bit 30): no community page found.
    pub const SWIMMING: ActionId = ActionId(0x40000E3);
}

/// Check for water-plunge entry. Returns the water surface y if Mario is
/// more than 100 units below it.
pub fn water_plunge_surface(cx: &ActionCx) -> Option<f32> {
    let wl = cx.world.water_level(cx.state.pos.x, cx.state.pos.z)?;
    (cx.state.pos.y < wl - 100.0).then_some(wl)
}

/// Enter Water Plunge: quarter horizontal, halve vertical, snap to
/// water-100. (wiki:Water Plunge)
pub fn enter_water_plunge(cx: &mut ActionCx, surface_y: f32) -> ActionResult {
    cx.state.vel.x /= 4.0;
    cx.state.vel.z /= 4.0;
    cx.state.vel.y /= 2.0;
    cx.state.pos.y = surface_y - 100.0;
    cx.state.forward_speed /= 4.0;
    cx.goto(id::WATER_PLUNGE, 0)
}

/// Water Plunge: brief entry, then swimming.
pub struct WaterPlunge;
impl ActionHandler for WaterPlunge {
    fn name(&self) -> &'static str {
        "WaterPlunge"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        cx.state.action_timer += 1;
        // Drag to a stop, then swim.
        cx.state.vel.x *= 0.9;
        cx.state.vel.z *= 0.9;
        cx.state.vel.y *= 0.9;
        cx.state.pos += cx.state.vel;
        if cx.state.action_timer >= 10 {
            return cx.goto(id::SWIMMING, 0);
        }
        ActionResult::Stay
    }
}

/// Swimming: 3D water locomotion. Numbers are working assumptions (verify),
/// except the B-stroke cap which is informed by a documented 0xA challenge
/// noting B-only swimming caps around speed 7.
pub struct Swimming;
impl ActionHandler for Swimming {
    fn name(&self) -> &'static str {
        "Swimming"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        let wl = match cx.world.water_level(cx.state.pos.x, cx.state.pos.z) {
            Some(wl) => wl,
            None => {
                // Left the water: back to air.
                return cx.goto(ActionId::FREEFALL, 0);
            }
        };

        // Steering: yaw from stick, pitch from stick_y.
        let intended = cx.intended_yaw();
        if let Some(iy) = intended {
            cx.state.face_yaw = cx.state.face_yaw.approach(iy, 0x400); // (verify)
        }

        // Strokes. (verify)
        if cx.pressed(buttons::B, prev_buttons) {
            cx.state.forward_speed = (cx.state.forward_speed + 3.0).min(8.0);
        }
        if cx.pressed(buttons::A, prev_buttons) {
            cx.state.forward_speed = (cx.state.forward_speed + 5.0).min(24.0);
        }
        // Water drag.
        cx.state.forward_speed *= 0.98;

        // Pitch: stick_y controls vertical direction.
        let pitch = (cx.input.stick_y as f32 / 80.0).clamp(-1.0, 1.0);
        let yaw_rad = cx.state.face_yaw.0 as f32 / 65536.0 * std::f32::consts::TAU;
        let fwd = cx.state.forward_speed;
        cx.state.vel.x = yaw_rad.sin() * fwd;
        cx.state.vel.z = yaw_rad.cos() * fwd;
        cx.state.vel.y = pitch * fwd * 0.7;
        // Buoyancy: drift toward neutral.
        cx.state.vel.y *= 0.95;

        cx.state.pos += cx.state.vel;

        // Don't breach the surface by much; near-surface + up -> freefall out.
        if cx.state.pos.y > wl - 40.0 && cx.state.vel.y > 0.0 {
            cx.state.pos.y = wl - 40.0;
            // If pushing up hard, exit the water.
            if pitch > 0.7 && cx.state.forward_speed > 10.0 {
                return cx.goto(ActionId::FREEFALL, 0);
            }
        }
        // Don't sink below the floor.
        if let Some(f) = cx.world.find_floor(cx.state.pos, 200.0) {
            if cx.state.pos.y < f.y + 20.0 {
                cx.state.pos.y = f.y + 20.0;
                cx.state.vel.y = cx.state.vel.y.max(0.0);
            }
        }

        ActionResult::Stay
    }
}
