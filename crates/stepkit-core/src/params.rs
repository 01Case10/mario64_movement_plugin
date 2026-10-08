//! Tunable movement parameters.
//!
//! Defaults come from `spec/constants.toml`; every field documents its spec
//! key. Changing any default flags the instance as non-faithful, and the
//! harness only ever runs with defaults.

/// Every tunable that shapes movement. Units are SM64 units and frames.
#[derive(Clone, Debug)]
pub struct MovementParams {
    // ---- physics (spec: physics.*) ----
    /// Gravity applied per frame while airborne. spec: physics.gravity
    pub gravity: f32,
    /// Terminal fall speed. spec: physics.terminal_velocity
    pub terminal_velocity: f32,
    // ---- walking (spec: walk.*) ----
    /// Stick intended magnitude capped to this for the walk target speed.
    /// spec: walk.target_speed_cap
    pub walk_target_speed_cap: f32,
    /// Per-frame speed gain from rest. spec: walk.speedup_base
    pub walk_speedup_base: f32,
    /// Accel falloff: accel = base - speed / this. spec: walk.speedup_falloff_divisor
    pub walk_speedup_falloff_divisor: f32,
    /// Per-frame speed loss when above target on flat ground. spec: walk.decel_flat_ground
    pub walk_decel_flat_ground: f32,
    /// Forward speed never exceeds this. spec: walk.hard_cap
    pub walk_hard_cap: f32,
    /// Facing approach rate toward intended yaw (angle units/frame). spec: walk.turn_rate
    pub walk_turn_rate: u16,
    /// Walk entry clamp: min(intended magnitude, this). spec: walk.enter_speed_clamp
    pub walk_enter_speed_clamp: f32,
    /// Stick-back + speed >= this enters Turning Around. spec: walk.turnaround_speed
    pub walk_turnaround_speed: f32,
    /// Neutral stick + speed >= this enters Braking. spec: walk.brake_speed
    pub walk_brake_speed: f32,
    // ---- slopes (spec: slope.*) ----
    /// Downhill/uphill speed adjustment per frame by floor class, scaled by
    /// steepness. spec: slope.accel_default / slippery / very_slippery / not_slippery
    pub slope_accel_default: f32,
    pub slope_accel_slippery: f32,
    pub slope_accel_very_slippery: f32,
    pub slope_accel_not_slippery: f32,
    // ---- jumps (spec: jump.*) ----
    /// Vertical speed on jump entry: base + forward_speed * factor. spec: jump.vertical_base
    pub jump_vertical_base: f32,
    /// spec: jump.vertical_forward_factor
    pub jump_vertical_forward_factor: f32,
    /// Forward speed multiplier on jump entry. spec: jump.forward_retain
    pub jump_forward_retain: f32,
    /// Jump-height control: vy > this and A not held -> vy quartered. spec: jump.height_control_threshold
    pub jump_height_control_threshold: f32,
    /// Gravity during a long jump. spec: physics.longjump_gravity
    pub longjump_gravity: f32,
    /// Horizontal speed gained on dive entry. spec: dive.horizontal_gain
    pub dive_horizontal_gain: f32,
    /// Downhill accel while butt sliding. spec: slide.downhill_accel
    pub slide_downhill_accel: f32,
    // ---- steps (spec: step.*) ----
    /// Air landing window: floor within this below the quarter-step position lands. spec: step.air_landing_snap_window
    pub air_landing_snap_window: f32,
    /// Ceiling window that zeroes upward velocity. spec: step.ceiling_zero_vel_window
    pub ceiling_zero_vel_window: f32,
    /// Ground step-up height. spec: step.ground_step_up (verify)
    pub ground_step_up: f32,
    /// Ground step-down snap distance; further drops walk off. spec: step.ground_step_down (verify)
    pub ground_step_down: f32,
    // ---- collision (spec: collision.*) ----
    /// Character hitbox radius. spec: collision.radius
    pub radius: f32,
    /// Character hitbox height. spec: collision.height
    pub height: f32,
    /// `true` when every field still holds its default.
    pub faithful: bool,
}

impl Default for MovementParams {
    fn default() -> Self {
        Self {
            gravity: 4.0,
            terminal_velocity: -75.0,
            walk_target_speed_cap: 32.0,
            walk_speedup_base: 1.1,
            walk_speedup_falloff_divisor: 43.0,
            walk_decel_flat_ground: 1.0,
            walk_hard_cap: 48.0,
            walk_turn_rate: 0x800,
            walk_enter_speed_clamp: 8.0,
            walk_turnaround_speed: 16.0,
            walk_brake_speed: 16.0,
            slope_accel_default: 1.7,
            slope_accel_slippery: 2.7,
            slope_accel_very_slippery: 5.3,
            slope_accel_not_slippery: 0.0,
            jump_vertical_base: 42.0,
            jump_vertical_forward_factor: 0.25,
            jump_forward_retain: 0.8,
            jump_height_control_threshold: 20.0,
            longjump_gravity: 2.0,
            dive_horizontal_gain: 15.0,
            slide_downhill_accel: 2.5,
            air_landing_snap_window: 78.0,
            ceiling_zero_vel_window: 160.0,
            ground_step_up: 12.0,
            ground_step_down: 24.0,
            radius: 50.0,
            height: 160.0,
            faithful: true,
        }
    }
}

impl MovementParams {
    /// Slope acceleration for a surface kind.
    pub fn slope_accel(&self, kind: crate::world::SurfaceKind) -> f32 {
        use crate::world::SurfaceKind::*;
        match kind {
            Default => self.slope_accel_default,
            Slide => self.slope_accel_slippery,
            Quicksand => self.slope_accel_not_slippery,
            Custom(_) => self.slope_accel_default,
        }
    }

    /// Builder-style setter that clears the faithful flag.
    pub fn set_gravity(&mut self, v: f32) -> &mut Self {
        self.gravity = v;
        self.faithful = false;
        self
    }
}
