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
use crate::anim_table;
use crate::input::buttons;
use crate::state::{act_flags, ActionId};
use crate::world::FLOOR_LOWER_LIMIT;

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
    let wl = cx.world.find_water_level(cx.state.pos.x, cx.state.pos.z);
    if wl <= FLOOR_LOWER_LIMIT {
        return None;
    }
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
        // Decomp `act_water_plunge` (mario_actions_submerged.c:953): the timer
        // increments at the top of the tick (`m->actionTimer++`, line 963).
        cx.state.action_timer += 1;
        // Decomp lines 969-980: one-shot splash latch on the first frame
        // (`actionState` 0 -> 1; sound/particles have no core equivalent).
        if cx.state.action_state == 0 {
            cx.state.action_state = 1;
        }
        // Drag to a stop, then the recovery pose.
        cx.state.vel.x *= 0.9;
        cx.state.vel.z *= 0.9;
        cx.state.vel.y *= 0.9;
        cx.state.pos += cx.state.vel;
        // End on floor contact, or when the timer passes 20 frames.
        // Decomp line 985: `m->actionTimer > 20` (post-increment), so the
        // exit fires on the 21st tick.
        // NOTE: the decomp discards every `set_mario_action` return here and
        // falls through to `return FALSE`, so its exit is next-tick; this
        // core keeps the historical same-tick `goto` (flagged, not changed).
        // NOTE: the decomp's vel-based exit (`vel.y >= endVSpeed`, 0.0 near
        // the surface / -5.0 deep) and the diving-entry exit target
        // (ACT_FLUTTER_KICK, case 2) belong to the later velocity/step port.
        if let Some(f) = cx.world.find_floor(cx.state.pos.x, cx.state.pos.y, cx.state.pos.z) {
            if cx.state.pos.y <= f.y + 1.0 {
                cx.state.pos.y = f.y;
                return cx.goto(id::WATER_ACTION_END, 0);
            }
        }
        if cx.state.action_timer > 20 {
            return cx.goto(id::WATER_ACTION_END, 0);
        }
        // Decomp lines 1003-1021: the plunge anim follows `stateFlags` every
        // tick — FLUTTERKICK when entering from a diving action or holding A,
        // WATER_ACTION_END otherwise. (Held-object/metal cases 1,3,4,5 have
        // no core equivalent yet.)
        let diving_entry = cx.state.prev_action.0 & act_flags::DIVING != 0
            || cx.input.buttons & buttons::A != 0;
        cx.set_mario_animation(if diving_entry {
            anim_table::MARIO_ANIM_FLUTTERKICK
        } else {
            anim_table::MARIO_ANIM_WATER_ACTION_END
        });
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
    let wl = cx.world.find_water_level(cx.state.pos.x, cx.state.pos.z);
    if wl <= FLOOR_LOWER_LIMIT {
        return None;
    }
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
    if let Some(f) = cx.world.find_floor(cx.state.pos.x, cx.state.pos.y, cx.state.pos.z) {
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
        let wl = cx.world.find_water_level(cx.state.pos.x, cx.state.pos.z);
        if wl <= FLOOR_LOWER_LIMIT {
            return cx.goto(ActionId::FREEFALL, 0);
        }
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
        // Decomp `common_idle_step` (mario_actions_submerged.c:305-327), as
        // called by `act_water_idle` (lines 341-344): the tread always uses
        // `set_mario_anim_with_accel` with 0x10000, or 0x30000 when the face
        // pitch is below -0x1000 (pitched up hard).
        // NOTE: the swim step does not maintain `face_pitch` yet (later
        // phase), so this currently always selects 0x10000.
        let accel = if cx.state.face_pitch.0 < -0x1000 {
            0x30000
        } else {
            0x10000
        };
        cx.set_mario_anim_with_accel(anim_table::MARIO_ANIM_WATER_IDLE, accel);
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
        let wl = cx.world.find_water_level(cx.state.pos.x, cx.state.pos.z);
        if wl <= FLOOR_LOWER_LIMIT {
            return cx.goto(ActionId::FREEFALL, 0);
        }
        // Decomp `act_breaststroke` line 519: `if (++m->actionTimer == 14)` —
        // a pre-increment, and the frame-14 exit is checked before the rest
        // of the body runs.
        cx.state.action_timer += 1;
        // NOTE: the decomp exits to ACT_FLUTTER_KICK at frame 14
        // (lines 519-521); this core keeps the historical SWIMMING_END
        // target — flagged, later phase (transition, not timer discipline).
        // NOTE: the decomp's chain is an `actionState` flag set on A during
        // frames 2-5, consumed at frame 9 with `set_anim_to_frame(0)`,
        // `actionTimer = 1`, and a strength reset (lines 532-541); this core
        // keeps its immediate re-trigger (+10 strength) model — flagged.
        // NOTE: the decomp resets strength when `actionArg == 0`
        // (line 513-515) and adds +0.5/frame for timer < 6 (line 528);
        // both are strength/velocity behavior tied to the chain model above —
        // flagged, not ported.
        if cx.state.action_timer == 14 {
            return cx.goto(id::SWIMMING_END, 0);
        }
        if let Some(r) = check_water_jump(cx, prev_buttons, wl) {
            return r;
        }
        // Chain window: A during frames 2-5 builds strength and re-triggers
        // the stroke from the top. The window reads the post-increment
        // timer, matching the decomp's `>= 2 && < 6` on the incremented value
        // (line 532).
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
        // The stroke's rated speed: strength/10.
        let cap = cx.state.swim_strength as f32 / 10.0;
        cx.state.forward_speed = cx.state.forward_speed.min(cap);
        // Decomp line 556: the stroke requests MARIO_ANIM_SWIM_PART1 every
        // tick (before `common_swimming_step`).
        cx.set_mario_animation(anim_table::MARIO_ANIM_SWIM_PART1);
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
        let wl = cx.world.find_water_level(cx.state.pos.x, cx.state.pos.z);
        if wl <= FLOOR_LOWER_LIMIT {
            return cx.goto(ActionId::FREEFALL, 0);
        }
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
        // Decomp `act_swimming_end` line 596: `m->actionTimer++` comes AFTER
        // the exit/re-chain checks, so they observe the pre-increment value —
        // the frame-15 exit fires on the 16th tick.
        // NOTE: the decomp re-chains on A *held* (not pressed) with arg 1,
        // adds +10 strength only at timer == 7, and resets strength at
        // timer >= 7 (lines 588-594); those are cancel/strength behavior —
        // flagged, not ported.
        cx.state.action_timer += 1;
        // Decomp line 599: the glide requests MARIO_ANIM_SWIM_PART2 every
        // tick (before `common_swimming_step`).
        cx.set_mario_animation(anim_table::MARIO_ANIM_SWIM_PART2);
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
        let wl = cx.world.find_water_level(cx.state.pos.x, cx.state.pos.z);
        if wl <= FLOOR_LOWER_LIMIT {
            return cx.goto(ActionId::FREEFALL, 0);
        }
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
        // Decomp `act_flutter_kick` line 625: the timer is pinned at 1 while
        // A is held (no increment); the release branch above is the only
        // other path.
        // NOTE: the decomp grants the +10 release bonus only when the timer
        // is still 0 (released on the entry frame, line 617) and resets
        // strength every tick (line 626); this core keeps its edge-detected
        // release bonus and entry-only reset — flagged, not ported.
        cx.state.action_timer = 1;
        // Decomp lines 627-630: the kick anim only plays below 14.0 speed
        // (the swim-noise call is sound, skipped).
        if cx.state.forward_speed < 14.0 {
            cx.set_mario_animation(anim_table::MARIO_ANIM_FLUTTERKICK);
        }
        if swim_move(cx).is_none() {
            return cx.goto(ActionId::FREEFALL, 0);
        }
        cx.timeline.slot = slot::FLUTTER_KICK;
        ActionResult::Stay
    }
}

/// WATER_ACTION_END: brief recovery pose after the plunge or the glide.
/// Speed decays; exits to treading water when the recovery animation ends
/// (anim-driven, no timer). A/B stroke out early.
pub struct WaterActionEnd;
impl ActionHandler for WaterActionEnd {
    fn name(&self) -> &'static str {
        "WaterActionEnd"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if cx.world.find_water_level(cx.state.pos.x, cx.state.pos.z) <= FLOOR_LOWER_LIMIT {
            return cx.goto(ActionId::FREEFALL, 0);
        }
        if cx.pressed(buttons::A, prev_buttons) || cx.pressed(buttons::B, prev_buttons) {
            return enter_breaststroke(cx);
        }
        // spec: swim.action_end_decay (verified: decomp-derived)
        cx.state.forward_speed *= 0.95;
        // Decomp `act_water_action_end` (mario_actions_submerged.c:373):
        // `common_idle_step(m, MARIO_ANIM_WATER_ACTION_END, 0)` requests the
        // anim (arg 0 -> plain `set_mario_animation`, lines 305-327), and the
        // exit to WATER_IDLE is anim-driven — `is_anim_at_end` (lines 386-388).
        // There is NO timer in this action.
        // NOTE: the decomp discards the `set_mario_action` return and falls
        // through to `return FALSE`, so its exit is next-tick; this core
        // keeps the historical same-tick `goto` (flagged, not changed).
        cx.set_mario_animation(anim_table::MARIO_ANIM_WATER_ACTION_END);
        if cx.is_anim_at_end() {
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
