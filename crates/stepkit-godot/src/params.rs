//! `StepParams`: a Godot Resource exposing the tunables of
//! [`MovementParams`](stepkit_core::params::MovementParams).
//!
//! Defaults are the wiki-sourced values from `spec/constants.toml`.
//! Only the most-tuned fields are exposed; everything else uses the default.

use godot::prelude::*;
use stepkit_core::params::MovementParams;

#[derive(GodotClass)]
#[class(base=Resource)]
pub struct StepParams {
    #[var]
    #[export]
    pub target_speed: f32,
    #[var]
    #[export]
    pub turn_rate_deg: f32,
    #[var]
    #[export]
    pub gravity: f32,
    #[var]
    #[export]
    pub terminal_velocity: f32,
    #[var]
    #[export]
    pub jump_vertical_base: f32,
    #[var]
    #[export]
    pub jump_forward_factor: f32,
    #[var]
    #[export]
    pub longjump_gravity: f32,
    #[var]
    #[export]
    pub dive_horizontal_gain: f32,
    #[var]
    #[export]
    pub slide_downhill_accel: f32,
    base: Base<Resource>,
}

#[godot_api]
impl StepParams {
    /// Build the core [`MovementParams`], converting degrees to angle units.
    pub fn to_core(&self) -> MovementParams {
        MovementParams {
            walk_target_speed_cap: self.target_speed,
            // turn_rate_deg is degrees/frame -> 16-bit angle units.
            walk_turn_rate: ((self.turn_rate_deg / 360.0) * 65536.0) as u16,
            gravity: self.gravity,
            terminal_velocity: self.terminal_velocity,
            jump_vertical_base: self.jump_vertical_base,
            jump_vertical_forward_factor: self.jump_forward_factor,
            longjump_gravity: self.longjump_gravity,
            dive_horizontal_gain: self.dive_horizontal_gain,
            slide_downhill_accel: self.slide_downhill_accel,
            ..MovementParams::default()
        }
    }
}

impl StepParams {
    fn init_defaults(&mut self) {
        let d = MovementParams::default();
        self.target_speed = d.walk_target_speed_cap;
        self.turn_rate_deg = (d.walk_turn_rate as f32 / 65536.0) * 360.0;
        self.gravity = d.gravity;
        self.terminal_velocity = d.terminal_velocity;
        self.jump_vertical_base = d.jump_vertical_base;
        self.jump_forward_factor = d.jump_vertical_forward_factor;
        self.longjump_gravity = d.longjump_gravity;
        self.dive_horizontal_gain = d.dive_horizontal_gain;
        self.slide_downhill_accel = d.slide_downhill_accel;
    }
}

// gdext `init` runs before we can set fields; hook _init via IResource.
#[godot_api]
impl IResource for StepParams {
    fn init(base: Base<Resource>) -> Self {
        let mut s = Self {
            target_speed: 0.0,
            turn_rate_deg: 0.0,
            gravity: 0.0,
            terminal_velocity: 0.0,
            jump_vertical_base: 0.0,
            jump_forward_factor: 0.0,
            longjump_gravity: 0.0,
            dive_horizontal_gain: 0.0,
            slide_downhill_accel: 0.0,
            base,
        };
        s.init_defaults();
        s
    }
}
