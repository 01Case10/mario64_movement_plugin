//! Water actions: plunge entry and the swim state machine.
//!
//! Water Plunge (0x300022E2) entry is documented (wiki:Water Plunge,
//! rev 2023-07-27): when Mario's height is more than 100 below the water
//! surface, forward velocity is quartered, vertical velocity halved, height
//! is set to 100 below the surface, and the action becomes water plunge.
//! The plunge ends on floor contact or after 20 frames, into
//! WATER_ACTION_END.
//!
//! The swim states (WATER_IDLE, BREASTSTROKE, SWIMMING_END, FLUTTER_KICK,
//! WATER_ACTION_END) and the water jump follow the decomp-derived behavior
//! model (see the gap plan); numeric constants cite spec: swim.* in their
//! comments. The implementation below is original Rust written from those
//! behavioral descriptions; no decompiled source appears in this repo.

use super::{ActionCx, ActionHandler, ActionResult};
use crate::input::buttons;
use crate::state::ActionId;

pub mod slot {
    pub const WATER_PLUNGE: u32 = 21;
    pub const WATER_IDLE: u32 = 22;
    pub const WATER_ACTION_END: u32 = 38;
    pub const BREASTSTROKE: u32 = 39;
    pub const SWIMMING_END: u32 = 40;
    pub const FLUTTER_KICK: u32 = 41;
}

pub mod id {
    use crate::state::ActionId;
    /// Verified: wiki:Water Plunge (rev 2023-07-27).
    pub const WATER_PLUNGE: ActionId = ActionId(0x300022E2);
    /// Treading water. (verified: decomp ACT_WATER_IDLE)
    pub const WATER_IDLE: ActionId = ActionId(0x380022C0);
    /// Recovery pose after the plunge / glide. (verified: decomp ACT_WATER_ACTION_END)
    pub const WATER_ACTION_END: ActionId = ActionId(0x300022C2);
    /// Power stroke. (verified: decomp ACT_BREASTSTROKE)
    pub const BREASTSTROKE: ActionId = ActionId(0x300024D0);
    /// Glide after the stroke. (verified: decomp ACT_SWIMMING_END)
    pub const SWIMMING_END: ActionId = ActionId(0x300024D1);
    /// Held-A fast kick. (verified: decomp ACT_FLUTTER_KICK)
    pub const FLUTTER_KICK: ActionId = ActionId(0x300024D2);
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
    // The fall is over; a later water exit must not inherit this peak.
    cx.state.peak_height = 0.0;
    cx.goto(id::WATER_PLUNGE, 0)
}

/// Water Plunge: drag to a stop, then the recovery pose. Ends on floor
/// contact or after 20 frames.
pub struct WaterPlunge;
impl ActionHandler for WaterPlunge {
    fn name(&self) -> &'static str {
        "WaterPlunge"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // NOTE: step::tick already incremented action_timer; do not add again.
        // Drag to a stop, then the recovery pose.
        cx.state.vel.x *= 0.9;
        cx.state.vel.z *= 0.9;
        cx.state.vel.y *= 0.9;
        cx.state.pos += cx.state.vel;
        // End on floor contact, or after 20 frames.
        // spec: swim.plunge_end_frames (verified: decomp-derived)
        if let Some(f) = cx.world.find_floor(cx.state.pos, 1.0) {
            if cx.state.pos.y <= f.y + 1.0 {
                cx.state.pos.y = f.y;
                return cx.goto(id::WATER_ACTION_END, 0);
            }
        }
        if cx.state.action_timer >= 20 {
            return cx.goto(id::WATER_ACTION_END, 0);
        }
        ActionResult::Stay
    }
}

/// Shared swim integration: steer by the stick, compose the 3D velocity
/// (yaw direction * forward speed, pitch * forward * 0.7, buoyancy
/// relaxation on the vertical), integrate, then clamp to the surface
/// ceiling and the floor.
///
/// Returns the water level, or `None` when the character has left the water
/// or breached the surface hard; both cases go to FREEFALL.
fn swim_move(cx: &mut ActionCx) -> Option<f32> {
    let wl = cx.world.water_level(cx.state.pos.x, cx.state.pos.z)?;
    // Steering: yaw follows the stick; pitch comes from stick_y.
    if let Some(iy) = cx.intended_yaw() {
        cx.state.face_yaw = cx.state.face_yaw.approach(iy, 0x400);
    }
    let pitch = (cx.input.stick_y as f32 / 80.0).clamp(-1.0, 1.0);
    let fwd = cx.state.forward_speed;
    let (fx, fz) = cx.forward_xz();
    cx.state.vel.x = fx * fwd;
    cx.state.vel.z = fz * fwd;
    // Buoyancy: the vertical velocity relaxes toward neutral.
    cx.state.vel.y = pitch * fwd * 0.7 * 0.95;
    cx.state.pos += cx.state.vel;
    // Surface ceiling: the swimmer may ride right under the surface, so the
    // water-jump check ("within 1.5 of the surface") is reachable.
    // Deviation note: the old placeholder clamped at level-40, which made
    // the jump condition unsatisfiable and pinned the tread 40 units down.
    // spec: swim.surface_ceiling (verified: decomp-derived)
    if cx.state.pos.y > wl - 1.5 {
        cx.state.pos.y = wl - 1.5;
        // Pushing up hard breaches out into freefall.
        // spec: swim.breach_pitch / swim.breach_speed
        if pitch > 0.7 && fwd > 10.0 {
            return None;
        }
    }
    // Don't sink through the floor.
    if let Some(f) = cx.world.find_floor(cx.state.pos, 200.0) {
        if cx.state.pos.y < f.y + 20.0 {
            cx.state.pos.y = f.y + 20.0;
            cx.state.vel.y = cx.state.vel.y.max(0.0);
        }
    }
    Some(wl)
}

/// Water jump check, run in every swim state: A pressed near the surface,
/// level-or-up pitch, stick pushed up hard.
///
/// Deviation note: the reference checks stickY < -60, but its N64 convention
/// maps pull-down to swim-up, while this core's pitch = stick_y/80 maps
/// push-up to swim-up; the translated threshold is stick_y > +60. The
/// reference's pitch >= 0 requirement is unsatisfiable with a literal -60
/// under this core's model, so the translated sign is used.
/// spec: swim.jump_surface_dist / swim.jump_stick_threshold (decomp-derived)
fn check_water_jump(cx: &mut ActionCx, prev_buttons: u16, wl: f32) -> Option<ActionResult> {
    let pitch = (cx.input.stick_y as f32 / 80.0).clamp(-1.0, 1.0);
    if cx.pressed(buttons::A, prev_buttons)
        && wl - cx.state.pos.y <= 1.5
        && pitch >= 0.0
        && cx.input.stick_y > 60
    {
        return Some(super::air::enter_water_jump(cx));
    }
    None
}

/// Enter the breaststroke: the stroke timer restarts; swim strength carries
/// over so chained strokes keep their built-up speed.
pub fn enter_breaststroke(cx: &mut ActionCx) -> ActionResult {
    cx.timeline.slot = slot::BREASTSTROKE;
    cx.goto(id::BREASTSTROKE, 0)
}

/// Enter the flutter kick: strength resets; speed approaches 12.0.
pub fn enter_flutter_kick(cx: &mut ActionCx) -> ActionResult {
    // spec: swim.strength_default (verified: decomp-derived)
    cx.state.swim_strength = 160;
    cx.timeline.slot = slot::FLUTTER_KICK;
    cx.goto(id::FLUTTER_KICK, 0)
}

/// WATER_IDLE: treading water. With the stick held, speed eases toward 16;
/// without it, the swimmer drifts to a stop.
pub struct WaterIdle;
impl ActionHandler for WaterIdle {
    fn name(&self) -> &'static str {
        "WaterIdle"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        let wl = match cx.world.water_level(cx.state.pos.x, cx.state.pos.z) {
            Some(wl) => wl,
            None => return cx.goto(ActionId::FREEFALL, 0),
        };
        if let Some(r) = check_water_jump(cx, prev_buttons, wl) {
            return r;
        }
        // A tap strokes; A held (not just pressed) flutters; B strokes too
        // (no water punch yet).
        if cx.pressed(buttons::A, prev_buttons) {
            return enter_breaststroke(cx);
        }
        if cx.input.buttons & buttons::A != 0 {
            return enter_flutter_kick(cx);
        }
        if cx.pressed(buttons::B, prev_buttons) {
            return enter_breaststroke(cx);
        }
        // spec: swim.idle_target_speed / swim.idle_approach (decomp-derived)
        if cx.intended_yaw().is_some() {
            let s = cx.state.forward_speed;
            cx.state.forward_speed = if s < 16.0 {
                (s + 1.0).min(16.0)
            } else {
                (s - 1.0).max(16.0)
            };
        } else {
            // spec: swim.idle_drift (decomp-derived)
            cx.state.forward_speed *= 0.98;
        }
        if swim_move(cx).is_none() {
            return cx.goto(ActionId::FREEFALL, 0);
        }
        cx.timeline.slot = slot::WATER_IDLE;
        ActionResult::Stay
    }
}

/// BREASTSTROKE: the power stroke. +0.5 on the entry frame, +1.5/frame from
/// frame 9, ending in the glide at frame 14. A pressed during frames 2-5
/// chains: strength +10 (cap 280) and the stroke re-triggers. Speed is
/// capped at strength/10 (16.0 -> 28.0 max).
pub struct Breaststroke;
impl ActionHandler for Breaststroke {
    fn name(&self) -> &'static str {
        "Breaststroke"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        let wl = match cx.world.water_level(cx.state.pos.x, cx.state.pos.z) {
            Some(wl) => wl,
            None => return cx.goto(ActionId::FREEFALL, 0),
        };
        if let Some(r) = check_water_jump(cx, prev_buttons, wl) {
            return r;
        }
        // Chain window: A during frames 2-5 builds strength and re-triggers
        // the stroke from the top.
        // spec: swim.chain_window / swim.chain_bonus / swim.strength_max
        if cx.pressed(buttons::A, prev_buttons) && (2..=5).contains(&cx.state.action_timer) {
            cx.state.swim_strength = (cx.state.swim_strength + 10).min(280);
            cx.state.action_timer = 0;
        } else if cx.pressed(buttons::B, prev_buttons) {
            // B re-strokes without building strength (no water punch yet).
            cx.state.action_timer = 0;
        }
        let t = cx.state.action_timer;
        if t == 1 {
            // spec: swim.stroke_entry_gain (verified: decomp-derived)
            cx.state.forward_speed += 0.5;
        } else if t >= 9 {
            // spec: swim.stroke_power_gain / swim.stroke_power_frame
            cx.state.forward_speed += 1.5;
        }
        // spec: swim.stroke_end_frame (verified: decomp-derived)
        if t >= 14 {
            return cx.goto(id::SWIMMING_END, 0);
        }
        // The stroke's rated speed: strength/10.
        let cap = cx.state.swim_strength as f32 / 10.0;
        cx.state.forward_speed = cx.state.forward_speed.min(cap);
        if swim_move(cx).is_none() {
            return cx.goto(ActionId::FREEFALL, 0);
        }
        cx.timeline.slot = slot::BREASTSTROKE;
        ActionResult::Stay
    }
}

/// SWIMMING_END: the glide. Speed decays 0.25/frame; A after frame 7
/// re-chains into the stroke keeping the current strength; frame 15 falls
/// back to the recovery pose.
pub struct SwimmingEnd;
impl ActionHandler for SwimmingEnd {
    fn name(&self) -> &'static str {
        "SwimmingEnd"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        let wl = match cx.world.water_level(cx.state.pos.x, cx.state.pos.z) {
            Some(wl) => wl,
            None => return cx.goto(ActionId::FREEFALL, 0),
        };
        if let Some(r) = check_water_jump(cx, prev_buttons, wl) {
            return r;
        }
        // spec: swim.glide_decay (verified: decomp-derived)
        cx.state.forward_speed = (cx.state.forward_speed - 0.25).max(0.0);
        if cx.pressed(buttons::A, prev_buttons) && cx.state.action_timer > 7 {
            return enter_breaststroke(cx);
        }
        // spec: swim.glide_end_frame (verified: decomp-derived)
        if cx.state.action_timer >= 15 {
            return cx.goto(id::WATER_ACTION_END, 0);
        }
        if swim_move(cx).is_none() {
            return cx.goto(ActionId::FREEFALL, 0);
        }
        cx.timeline.slot = slot::SWIMMING_END;
        ActionResult::Stay
    }
}

/// FLUTTER_KICK: held-A fast kick. Speed approaches 12.0 (0.1 up, 0.15
/// down); releasing A ends in the glide, applying the chain bonus.
pub struct FlutterKick;
impl ActionHandler for FlutterKick {
    fn name(&self) -> &'static str {
        "FlutterKick"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        let wl = match cx.world.water_level(cx.state.pos.x, cx.state.pos.z) {
            Some(wl) => wl,
            None => return cx.goto(ActionId::FREEFALL, 0),
        };
        if let Some(r) = check_water_jump(cx, prev_buttons, wl) {
            return r;
        }
        // A released: glide, with the chain bonus if below the max.
        if cx.input.buttons & buttons::A == 0 && prev_buttons & buttons::A != 0 {
            cx.state.swim_strength = (cx.state.swim_strength + 10).min(280);
            return cx.goto(id::SWIMMING_END, 0);
        }
        // spec: swim.flutter_target / swim.flutter_accel_up / swim.flutter_accel_down
        let s = cx.state.forward_speed;
        cx.state.forward_speed = if s < 12.0 {
            (s + 0.1).min(12.0)
        } else {
            (s - 0.15).max(12.0)
        };
        if swim_move(cx).is_none() {
            return cx.goto(ActionId::FREEFALL, 0);
        }
        cx.timeline.slot = slot::FLUTTER_KICK;
        ActionResult::Stay
    }
}

/// WATER_ACTION_END: brief recovery pose after the plunge or the glide.
/// Speed decays; 10 frames, then treading water. A/B stroke out early.
pub struct WaterActionEnd;
impl ActionHandler for WaterActionEnd {
    fn name(&self) -> &'static str {
        "WaterActionEnd"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if cx
            .world
            .water_level(cx.state.pos.x, cx.state.pos.z)
            .is_none()
        {
            return cx.goto(ActionId::FREEFALL, 0);
        }
        if cx.pressed(buttons::A, prev_buttons) || cx.pressed(buttons::B, prev_buttons) {
            return enter_breaststroke(cx);
        }
        // spec: swim.action_end_decay (verified: decomp-derived)
        cx.state.forward_speed *= 0.95;
        // spec: swim.action_end_frames (verified: decomp-derived)
        if cx.state.action_timer >= 10 {
            return cx.goto(id::WATER_IDLE, 0);
        }
        // The recovery pose still floats; the jump check is deliberately
        // skipped here (the reference only jumps from active swim states).
        if swim_move(cx).is_none() {
            return cx.goto(ActionId::FREEFALL, 0);
        }
        cx.timeline.slot = slot::WATER_ACTION_END;
        ActionResult::Stay
    }
}
