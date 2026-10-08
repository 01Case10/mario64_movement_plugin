//! `StepChar3D`: a Node3D that runs the stepkit core on a fixed 30 Hz tick
//! with render interpolation, and emits Godot signals for core events.
//!
//! The node holds no gameplay logic; it converts Godot types to core types,
//! steps the simulation, and presents the result.

use godot::builtin::EulerOrder;
use godot::prelude::*;
use stepkit_core::actions::ActionRegistry;
use stepkit_core::events::Event;
use stepkit_core::input::RawInput;
use stepkit_core::params::MovementParams;
use stepkit_core::state::CharacterState;
use stepkit_core::state::ActionId;
use stepkit_core::step::{tick, Timeline};
use stepkit_core::surface::SurfaceWorld;

use crate::params::StepParams;
use crate::world::StepWorld3D;

/// Button bits for [`StepChar3D::set_buttons`].
pub mod buttons {
    pub const A: i64 = 0x0001;
    pub const B: i64 = 0x0002;
    pub const Z: i64 = 0x0004;
}

#[derive(GodotClass)]
#[class(base=Node3D)]
pub struct StepChar3D {
    #[var]
    pub world_node: Option<Gd<StepWorld3D>>,
    #[var]
    pub params_res: Option<Gd<StepParams>>,
    /// Fixed simulation rate (Hz). The core is authored for 30.
    #[export]
    #[var]
    pub tick_rate: f32,
    /// Godot meters per SM64 unit. Must match the world's `unit_scale`.
    #[export]
    #[var]
    pub unit_scale: f32,

    state: CharacterState,
    timeline: Timeline,
    registry: ActionRegistry,
    params: MovementParams,
    world: SurfaceWorld,
    accumulator: f32,
    render_prev_pos: Vector3,
    render_prev_yaw: f32,
    render_cur_pos: Vector3,
    render_cur_yaw: f32,
    stick_x: f32,
    stick_y: f32,
    buttons: u16,
    prev_buttons: u16,
    pressed_latch: u16,
    camera_yaw_deg: f32,
    baked: bool,
    base: Base<Node3D>,
}

#[godot_api]
impl StepChar3D {
    #[signal]
    fn action_changed(new_action: i64, old_action: i64);
    #[signal]
    fn jumped(velocity_y: f32);
    #[signal]
    fn landed(fall_speed: f32);
    #[signal]
    fn wall_hit();
    #[signal]
    fn ledge_grabbed();

    /// Set the analog stick (-1..1). Y+ is forward.
    #[func]
    pub fn set_stick(&mut self, x: f32, y: f32) {
        self.stick_x = x.clamp(-1.0, 1.0);
        self.stick_y = y.clamp(-1.0, 1.0);
    }

    /// Set the full button bitfield (see [`buttons`]).
    ///
    /// Presses are latched: a 0->1 transition is visible to the next core
    /// tick even if the game clears it before then (the core runs at 30 Hz,
    /// the game may run faster).
    #[func]
    pub fn set_buttons(&mut self, bits: i64) {
        let new = bits as u16;
        self.pressed_latch |= new & !self.buttons;
        self.buttons = new;
    }

    /// Camera yaw in degrees; the stick is relative to it.
    #[func]
    pub fn set_camera_yaw_deg(&mut self, deg: f32) {
        self.camera_yaw_deg = deg;
    }

    /// Current action id (compare against the IDs in spec/constants.toml).
    #[func]
    pub fn get_action(&self) -> i64 {
        self.state.action.0 as i64
    }

    /// Current forward speed (units/frame).
    #[func]
    pub fn get_forward_speed(&self) -> f32 {
        self.state.forward_speed
    }

    /// Current health (full = 0x880 / 2176; dead below 0x100).
    #[func]
    pub fn get_health(&self) -> i64 {
        self.state.health as i64
    }

    /// Current animation slot (drives the anim manifest in phase 6).
    #[func]
    pub fn get_anim_slot(&self) -> i64 {
        self.timeline.slot as i64
    }

    /// IK hand target (left) in Godot meters, for 2-bone arm IK during
    /// ledge-grab actions. Returns Vector3.ZERO when no hint is active
    /// (all other actions). Renderer-only output; the sim never solves IK.
    #[func]
    pub fn get_ik_hand_l(&self) -> Vector3 {
        match self.state.ik_hand_l {
            Some(p) => Vector3::new(p.x * self.unit_scale, p.y * self.unit_scale, p.z * self.unit_scale),
            None => Vector3::ZERO,
        }
    }

    /// IK hand target (right) in Godot meters, for 2-bone arm IK during
    /// ledge-grab actions. Returns Vector3.ZERO when no hint is active
    /// (all other actions). Renderer-only output; the sim never solves IK.
    #[func]
    pub fn get_ik_hand_r(&self) -> Vector3 {
        match self.state.ik_hand_r {
            Some(p) => Vector3::new(p.x * self.unit_scale, p.y * self.unit_scale, p.z * self.unit_scale),
            None => Vector3::ZERO,
        }
    }

    /// Teleport the character (resets interpolation).
    ///
    /// This is a full reset: position, velocity, action, speeds, and timers
    /// are all cleared so a teleport never leaks the previous action into
    /// the new location (important for choreographed demos).
    #[func]
    pub fn teleport(&mut self, pos: Vector3) {
        let s = 1.0 / self.unit_scale.max(1e-6);
        self.state.pos = glam::Vec3::new(pos.x * s, pos.y * s, pos.z * s);
        self.state.vel = glam::Vec3::ZERO;
        let old = self.state.action;
        self.state.forward_speed = 0.0;
        self.state.slide_vel_x = 0.0;
        self.state.slide_vel_z = 0.0;
        self.state.action = ActionId::IDLE;
        self.state.prev_action = ActionId::IDLE;
        self.state.action_state = 0;
        self.state.action_timer = 0;
        self.state.action_arg = 0;
        self.state.wall_kick_timer = 0;
        self.state.quicksand_depth = 0.0;
        self.state.squish_timer = 0;
        self.state.swim_strength = 160;
        let v = Vector3::new(pos.x, pos.y, pos.z);
        self.render_prev_pos = v;
        self.render_cur_pos = v;
        self.accumulator = 0.0;
        if old != ActionId::IDLE {
            self.base_mut().emit_signal(
                "action_changed",
                &[ActionId::IDLE.0.to_variant(), old.0.to_variant()],
            );
        }
    }

    /// (Re)bake the collision world from `world_node` and params from
    /// `params_res`. Called automatically on ready.
    #[func]
    pub fn bake(&mut self) {
        if let Some(w) = self.world_node.as_ref() {
            self.world = w.bind().baked();
        }
        if let Some(p) = self.params_res.as_ref() {
            self.params = p.bind().to_core();
        }
        self.baked = true;
    }
    #[constant]
    const ACT_IDLE: i64 = 0x0C400201;
    #[constant]
    const ACT_WALKING: i64 = 0x04000440;
    #[constant]
    const ACT_JUMP: i64 = 0x03000880;
    #[constant]
    const ACT_DOUBLE_JUMP: i64 = 0x03000881;
    #[constant]
    const ACT_TRIPLE_JUMP: i64 = 0x01000882;
    #[constant]
    const ACT_BACKFLIP: i64 = 0x01000883;
    #[constant]
    const ACT_SIDE_FLIP: i64 = 0x01000887;
    #[constant]
    const ACT_LONG_JUMP: i64 = 0x03000888;
    #[constant]
    const ACT_DIVE: i64 = 0x0188088A;
    #[constant]
    const ACT_WALL_KICK: i64 = 0x018808B0;
    #[constant]
    const ACT_LEDGE_GRAB: i64 = 0x0800034B;
    #[constant]
    const BTN_A: i64 = buttons::A;
    #[constant]
    const BTN_B: i64 = buttons::B;
    #[constant]
    const BTN_Z: i64 = buttons::Z;
}

#[godot_api]
impl INode3D for StepChar3D {
    fn init(base: Base<Node3D>) -> Self {
        Self {
            world_node: None,
            params_res: None,
            tick_rate: 30.0,
            unit_scale: 0.01,
            state: CharacterState::default(),
            timeline: Timeline::default(),
            registry: ActionRegistry::sm64_style(),
            params: MovementParams::default(),
            world: SurfaceWorld::new(),
            accumulator: 0.0,
            render_prev_pos: Vector3::ZERO,
            render_prev_yaw: 0.0,
            render_cur_pos: Vector3::ZERO,
            render_cur_yaw: 0.0,
            stick_x: 0.0,
            stick_y: 0.0,
            buttons: 0,
            prev_buttons: 0,
            pressed_latch: 0,
            camera_yaw_deg: 0.0,
            baked: false,
            base,
        }
    }

    fn ready(&mut self) {
        self.bake();
        let p = self.base().get_global_position();
        self.teleport(p);
    }

    fn physics_process(&mut self, delta: f64) {
        if !self.baked {
            return;
        }
        let dt = 1.0 / self.tick_rate.max(1.0);
        self.accumulator += delta as f32;
        let mut steps = 0;
        while self.accumulator >= dt && steps < 4 {
            self.physics_tick();
            self.accumulator -= dt;
            steps += 1;
        }
        if steps == 4 {
            self.accumulator = 0.0; // spiral-of-death guard
        }
    }

    fn process(&mut self, _delta: f64) {
        if !self.baked {
            return;
        }
        let dt = 1.0 / self.tick_rate.max(1.0);
        let alpha = (self.accumulator / dt).clamp(0.0, 1.0);
        let pos = self.render_prev_pos.lerp(self.render_cur_pos, alpha);
        let mut dyaw = self.render_cur_yaw - self.render_prev_yaw;
        while dyaw > std::f32::consts::PI {
            dyaw -= std::f32::consts::TAU;
        }
        while dyaw < -std::f32::consts::PI {
            dyaw += std::f32::consts::TAU;
        }
        let yaw = self.render_prev_yaw + dyaw * alpha;
        // Model faces +Z at yaw 0 (documented).
        self.base_mut().set_global_transform(Transform3D::new(
            Basis::from_euler(EulerOrder::YXZ, Vector3::new(0.0, yaw, 0.0)),
            pos,
        ));
    }
}

impl StepChar3D {
    fn physics_tick(&mut self) {
        // N64 stick units: -1..1 maps to -80..80.
        let sx = (self.stick_x * 80.0) as i8;
        let sy = (self.stick_y * 80.0) as i8;
        let cam_yaw = stepkit_core::angles::Angle(((self.camera_yaw_deg / 360.0) * 65536.0) as i16);
        let effective = self.buttons | self.pressed_latch;
        self.pressed_latch = 0;
        let input = RawInput {
            stick_x: sx,
            stick_y: sy,
            buttons: effective,
            cam_yaw,
        };
        let (next, next_timeline, events) = tick(
            self.state,
            input,
            &self.world,
            &self.params,
            self.timeline,
            &self.registry,
            self.prev_buttons,
        );
        self.prev_buttons = effective;
        self.render_prev_pos = self.render_cur_pos;
        self.render_prev_yaw = self.render_cur_yaw;
        let us = self.unit_scale;
        self.render_cur_pos = Vector3::new(next.pos.x * us, next.pos.y * us, next.pos.z * us);
        self.render_cur_yaw = (next.face_yaw.0 as f32 / 65536.0) * std::f32::consts::TAU;
        self.state = next;
        self.timeline = next_timeline;
        for e in events {
            // Borrow the base once per tick for signal emission.
            match e {
                Event::ActionChanged { old, new } => {
                    self.base_mut()
                        .emit_signal("action_changed", &[new.0.to_variant(), old.0.to_variant()]);
                }
                Event::Jumped { velocity_y } => {
                    self.base_mut()
                        .emit_signal("jumped", &[velocity_y.to_variant()]);
                }
                Event::Landed { fall_speed } => {
                    self.base_mut()
                        .emit_signal("landed", &[fall_speed.to_variant()]);
                }
                Event::WallHit { .. } => {
                    self.base_mut().emit_signal("wall_hit", &[]);
                }
                Event::LedgeGrab => {
                    self.base_mut().emit_signal("ledge_grabbed", &[]);
                }
                Event::Damaged { amount } => {
                    self.base_mut()
                        .emit_signal("damaged", &[amount.to_variant()]);
                }
            }
        }
    }
}
