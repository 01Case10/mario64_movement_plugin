//! Air actions: SingleJump, DoubleJump, TripleJump, Backflip, SideFlip,
//! LongJump, Dive, GroundPound, Freefall.
//!
//! Entry numbers from the community wiki (see spec/actions/*.md):
//! Double Jump vy=52+hspeed/4 (wiki:Double Jump@18964), Triple vy=69
//! (wiki:Triple Jump@19300), Backflip fwd=-16/vy=62 (wiki:Backflip@19307),
//! SideFlip vy=62/fwd=8/facing=intended (wiki:Side Flip@20374),
//! Dive +15 horizontal clamped to 48 and +20 vy from ground
//! (wiki:Dive@19303). Long jump and ground pound numbers are (verify).

use super::{ActionCx, ActionHandler, ActionResult};
use crate::angles::Angle;
use crate::events::Event;
use crate::input::buttons;
use crate::state::ActionId;
use crate::step::step_air;
use glam::Vec3;

/// Animation slot ids for air actions.
pub mod slot {
    pub const JUMP: u32 = 6;
    pub const FREEFALL: u32 = 7;
    pub const DOUBLE_JUMP: u32 = 8;
    pub const TRIPLE_JUMP: u32 = 9;
    pub const BACKFLIP: u32 = 10;
    pub const SIDE_FLIP: u32 = 11;
    pub const LONG_JUMP: u32 = 12;
    pub const DIVE: u32 = 13;
    pub const GROUND_POUND: u32 = 14;
    pub const WALL_KICK: u32 = 17;
    pub const LEDGE_GRAB: u32 = 18;
    pub const AIR_KNOCKBACK: u32 = 20;
}

/// Per-action air behavior switches.
struct AirConfig {
    height_control: bool,
    gravity: f32,
    terminal: f32,
    steer_rate: u16,
    air_drag_threshold: f32,
    allow_dive: bool,
    allow_pound: bool,
    /// Whether this action may ledge-grab (documented per action).
    can_ledge_grab: bool,
}

/// Shared air cancels: B -> dive (speed > 28), Z -> ground pound.
/// Returns `Some` when a transition was taken.
fn air_cancels(cx: &mut ActionCx, prev_buttons: u16, cfg: &AirConfig) -> Option<ActionResult> {
    // Water plunge: more than 100 below the surface.
    // (verified: wiki:Water Plunge, rev 2023-07-27)
    if let Some(wl) = crate::actions::water::water_plunge_surface(cx) {
        return Some(crate::actions::water::enter_water_plunge(cx, wl));
    }
    // Wall kick: A while the kick window from a recent wall hit is open.
    // (ID verified: wiki:Wall Kick rev 19311; numbers (verify))
    if cx.pressed(buttons::A, prev_buttons) && cx.state.wall_kick_timer > 0 {
        let n = cx.state.wall_normal;
        return Some(enter_wall_kick(cx, n));
    }
    if cfg.allow_dive && cx.pressed(buttons::B, prev_buttons) && cx.state.forward_speed > 28.0 {
        // spec: dive.air_speed_threshold (verified: wiki:Dive@19303)
        return Some(enter_dive(cx, false));
    }
    if cfg.allow_pound && cx.pressed(buttons::Z, prev_buttons) {
        return Some(enter_ground_pound(cx));
    }
    None
}

/// Shared air physics + step. Returns `Some` on landing (or a cancel).
fn air_common(cx: &mut ActionCx, prev_buttons: u16, cfg: &AirConfig) -> Option<ActionResult> {
    if let Some(r) = air_cancels(cx, prev_buttons, cfg) {
        return Some(r);
    }
    let p = cfg;
    // Air control: forward drag, stick acceleration, sideways velocity.
    // (Behavioral model: forwardVel approaches 0, stick adds forward/sideways.)
    let mut fwd = cx.state.forward_speed;
    // Approach 0 by 0.35.
    if fwd > 0.0 {
        fwd = (fwd - 0.35).max(0.0);
    } else if fwd < 0.0 {
        fwd = (fwd + 0.35).min(0.0);
    }
    let mut sideways = 0.0;
    if let Some(iy) = cx.intended_yaw() {
        let mag = cx.intended_magnitude() as f32 / 32.0;
        if mag > 0.01 {
            let dyaw = iy.diff_to(cx.state.face_yaw);
            let dyaw_rad = dyaw as f32 / 65536.0 * std::f32::consts::TAU;
            // Forward: 1.5 * cos(dYaw) * mag. Sideways: 10.0 * sin(dYaw) * mag.
            fwd += mag * dyaw_rad.cos() * 1.5;
            sideways = mag * dyaw_rad.sin() * 10.0;
            // Turn facing: 512 * sin(dYaw) * mag (in angle units).
            let turn = Angle((512.0 * dyaw_rad.sin() * mag) as i16);
            cx.state.face_yaw = cx.state.face_yaw.wrapping_add(turn);
        }
    }
    // Speed drag and backward recovery.
    let drag_threshold = p.air_drag_threshold;
    if fwd > drag_threshold {
        fwd -= 1.0;
    }
    if fwd < -16.0 {
        fwd += 2.0;
    }
    cx.state.forward_speed = fwd;
    // Construct velocity from forward + sideways components.
    let (fx, fz) = cx.forward_xz();
    // Right vector: (fz, -fx) for yaw (sin, cos).
    cx.state.vel.x = fx * fwd + fz * sideways;
    cx.state.vel.z = fz * fwd - fx * sideways;
    // Gravity applies after movement (moved below the step).
    let vy_before_landing = cx.state.vel.y;
    let out = step_air(&cx.state, cx.world, cx.params);
    cx.state.pos = out.pos;
    cx.state.vel.y = out.vel_y;
    cx.state.floor_y = out.floor.map(|f| f.y);
    if out.wall_hit {
        cx.state.wall_hit = true;
        cx.state.wall_normal = out.wall_normal;
        cx.events.push(Event::WallHit {
            normal_yaw: crate::trig::atan2(out.wall_normal.x, out.wall_normal.z),
        });
        cx.state.wall_kick_timer = 10; // spec: wall.kick_window (verify)
                                       // Dive into a wall -> bonk -> backwards air knockback.
                                       // (wiki:Dive@19303; numbers (verify))
        if cx.state.action == ActionId::DIVE {
            return Some(enter_air_knockback(cx, out.wall_normal));
        }
        // Ledge grab: wall hit, falling, no landing, action allows it.
        // Conditions: wiki:Ledge Grab@19518.
        if out.floor.is_none() && vy_before_landing <= 0.0 && cfg.can_ledge_grab {
            if let Some(r) = try_ledge_grab(cx, out.wall_normal) {
                return Some(r);
            }
        }
    } else {
        cx.state.wall_hit = false;
        cx.state.wall_kick_timer = cx.state.wall_kick_timer.saturating_sub(1);
    }
    if let Some(f) = out.floor {
        let fall_speed = out.fall_speed;
        cx.events.push(Event::Landed { fall_speed });
        cx.state.floor_kind = super::ground::surface_kind_index(f.kind);
        cx.state.land_from = cx.state.action;
        cx.timeline.slot = super::ground::slot::LAND;
        return Some(cx.goto(ActionId::LANDING, fall_speed.max(0.0) as u32));
    }
    // Gravity applies after movement and collision.
    // Jump-height control: A released while rising fast quarters vy.
    if p.height_control
        && cx.input.buttons & buttons::A == 0
        && cx.state.vel.y > cx.params.jump_height_control_threshold
    {
        cx.state.vel.y /= 4.0;
    }
    cx.state.vel.y = (cx.state.vel.y - p.gravity).max(p.terminal);
    None
}

fn base_air_config() -> AirConfig {
    AirConfig {
        height_control: true,
        gravity: 4.0,
        terminal: -75.0,
        steer_rate: 0x800, // spec: air.steer_rate (verify)
        air_drag_threshold: 32.0,
        allow_dive: true,
        allow_pound: true,
        can_ledge_grab: true,
    }
}

/// Shared jump entry: vertical speed = 42 + forward_speed/4, forward *= 0.8.
/// (wiki:Single Jump@19309)
pub fn enter_jump(cx: &mut ActionCx) -> ActionResult {
    let p = cx.params;
    let vy = p.jump_vertical_base + cx.state.forward_speed * p.jump_vertical_forward_factor;
    cx.state.forward_speed *= p.jump_forward_retain;
    set_air_velocity(cx, vy);
    cx.timeline.slot = slot::JUMP;
    cx.events.push(Event::Jumped { velocity_y: vy });
    cx.goto(ActionId::JUMP, 0)
}

/// Double jump entry: vy = 52 + hspeed/4, forward *= 0.8.
/// (wiki:Double Jump@18964)
pub fn enter_double_jump(cx: &mut ActionCx) -> ActionResult {
    // spec: jump.double_vertical_base (verified: wiki:Double Jump@18964)
    let vy = 52.0 + cx.state.forward_speed * 0.25;
    cx.state.forward_speed *= 0.8;
    set_air_velocity(cx, vy);
    cx.timeline.slot = slot::DOUBLE_JUMP;
    cx.events.push(Event::Jumped { velocity_y: vy });
    cx.goto(ActionId::DOUBLE_JUMP, 0)
}

/// Triple jump entry: vy = 69 fixed, forward *= 0.8, no height control.
/// (wiki:Triple Jump@19300)
pub fn enter_triple_jump(cx: &mut ActionCx) -> ActionResult {
    // spec: jump.triple_vertical (verified: wiki:Triple Jump@19300)
    let vy = 69.0;
    cx.state.forward_speed *= 0.8;
    set_air_velocity(cx, vy);
    cx.timeline.slot = slot::TRIPLE_JUMP;
    cx.events.push(Event::Jumped { velocity_y: vy });
    cx.goto(ActionId::TRIPLE_JUMP, 0)
}

/// Backflip entry: forward = -16, vy = 62. (wiki:Backflip@19307)
pub fn enter_backflip(cx: &mut ActionCx) -> ActionResult {
    // spec: jump.backflip_forward / jump.backflip_vertical (verified)
    cx.state.forward_speed = -16.0;
    set_air_velocity(cx, 62.0);
    cx.timeline.slot = slot::BACKFLIP;
    cx.events.push(Event::Jumped { velocity_y: 62.0 });
    cx.goto(ActionId::BACKFLIP, 0)
}

/// Side flip entry: vy = 62, forward = 8, facing = intended yaw.
/// (wiki:Side Flip@20374)
pub fn enter_side_flip(cx: &mut ActionCx) -> ActionResult {
    // spec: jump.sideflip_vertical / jump.sideflip_forward (verified)
    if let Some(iy) = cx.intended_yaw() {
        cx.state.face_yaw = iy;
    }
    cx.state.forward_speed = 8.0;
    set_air_velocity(cx, 62.0);
    cx.timeline.slot = slot::SIDE_FLIP;
    cx.events.push(Event::Jumped { velocity_y: 62.0 });
    cx.goto(ActionId::SIDE_FLIP, 0)
}

/// Long jump entry. Numbers are (verify); gravity -2 is documented
/// (wiki:Gravity@20294).
pub fn enter_long_jump(cx: &mut ActionCx) -> ActionResult {
    // spec: jump.longjump_vertical / jump.longjump_forward_gain (verify)
    let vy = 28.0;
    cx.state.forward_speed += 18.0; // unbounded by design (verify)
    set_air_velocity(cx, vy);
    cx.timeline.slot = slot::LONG_JUMP;
    cx.events.push(Event::Jumped { velocity_y: vy });
    cx.goto(ActionId::LONG_JUMP, 0)
}

/// Dive entry: +15 horizontal (clamped to 48); +20 vy when from the ground.
/// (wiki:Dive@19303)
pub fn enter_dive(cx: &mut ActionCx, from_ground: bool) -> ActionResult {
    // spec: dive.horizontal_gain / dive.vertical_gain_ground (verified)
    cx.state.forward_speed = (cx.state.forward_speed + cx.params.dive_horizontal_gain).min(48.0);
    let vy = if from_ground { 20.0 } else { cx.state.vel.y };
    set_air_velocity(cx, vy);
    cx.timeline.slot = slot::DIVE;
    cx.events.push(Event::Jumped { velocity_y: vy });
    cx.goto(ActionId::DIVE, 0)
}

/// Ground pound entry: kill momentum, hover, then slam. All numbers (verify).
pub fn enter_ground_pound(cx: &mut ActionCx) -> ActionResult {
    cx.state.forward_speed = 0.0;
    cx.state.vel = Vec3::ZERO;
    cx.timeline.slot = slot::GROUND_POUND;
    cx.goto(ActionId::GROUND_POUND, 0)
}

fn set_air_velocity(cx: &mut ActionCx, vy: f32) {
    let (fx, fz) = cx.forward_xz();
    cx.state.vel = Vec3::new(fx * cx.state.forward_speed, vy, fz * cx.state.forward_speed);
}

macro_rules! air_action {
    ($name:ident, $label:literal, $cfg:expr, $slot:expr) => {
        pub struct $name;
        impl ActionHandler for $name {
            fn name(&self) -> &'static str {
                $label
            }
            fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
                if let Some(r) = air_common(cx, prev_buttons, &$cfg) {
                    return r;
                }
                cx.timeline.slot = $slot;
                ActionResult::Stay
            }
        }
    };
}

air_action!(SingleJump, "SingleJump", base_air_config(), slot::JUMP);
air_action!(
    DoubleJump,
    "DoubleJump",
    AirConfig {
        height_control: true, // (verify)
        ..base_air_config()
    },
    slot::DOUBLE_JUMP
);
air_action!(
    TripleJump,
    "TripleJump",
    AirConfig {
        height_control: false, // documented: no height control
        can_ledge_grab: false, // documented: no ledge grab or hang
        ..base_air_config()
    },
    slot::TRIPLE_JUMP
);
air_action!(
    Backflip,
    "Backflip",
    AirConfig {
        height_control: false, // (verify)
        allow_dive: false,
        can_ledge_grab: false, // documented: no ledge grab or hang
        ..base_air_config()
    },
    slot::BACKFLIP
);
air_action!(
    SideFlip,
    "SideFlip",
    AirConfig {
        height_control: false, // (verify)
        ..base_air_config()
    },
    slot::SIDE_FLIP
);
/// LongJump: gravity comes from params (spec: physics.longjump_gravity).
pub struct LongJump;
impl ActionHandler for LongJump {
    fn name(&self) -> &'static str {
        "LongJump"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        let gravity = cx.params.longjump_gravity;
        if let Some(r) = air_common(
            cx,
            prev_buttons,
            &AirConfig {
                height_control: false, // (verify)
                gravity,
                ..base_air_config()
            },
        ) {
            return r;
        }
        cx.timeline.slot = slot::LONG_JUMP;
        ActionResult::Stay
    }
}
air_action!(
    Dive,
    "Dive",
    AirConfig {
        height_control: false, // (verify)
        steer_rate: 0,         // committed (verify)
        air_drag_threshold: 32.0,
        allow_dive: false,
        allow_pound: false,
        ..base_air_config()
    },
    slot::DIVE
);

/// Freefall: walked off a ledge. Dive and pound allowed (verify for pound).
pub struct Freefall;
impl ActionHandler for Freefall {
    fn name(&self) -> &'static str {
        "Freefall"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = air_common(cx, prev_buttons, &base_air_config()) {
            return r;
        }
        cx.timeline.slot = slot::FREEFALL;
        ActionResult::Stay
    }
}

/// GroundPound: hover, then slam straight down. Numbers (verify).
pub struct GroundPound;
impl ActionHandler for GroundPound {
    fn name(&self) -> &'static str {
        "GroundPound"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        // spec: ground_pound.hover_frames / ground_pound.slam_speed (verify)
        const HOVER_FRAMES: u32 = 8;
        if cx.state.action_timer < HOVER_FRAMES {
            cx.state.vel = Vec3::ZERO;
            cx.timeline.slot = slot::GROUND_POUND;
            return ActionResult::Stay;
        }
        if let Some(r) = air_cancels(cx, prev_buttons, &base_air_config()) {
            return r;
        }
        // Slam: fall fast with no steering.
        cx.state.vel.x = 0.0;
        cx.state.vel.z = 0.0;
        cx.state.vel.y = (cx.state.vel.y - 4.0).max(-75.0);
        let out = step_air(&cx.state, cx.world, cx.params);
        cx.state.pos = out.pos;
        cx.state.vel.y = out.vel_y;
        if out.floor.is_some() {
            let fall_speed = out.fall_speed;
            cx.events.push(Event::Landed { fall_speed });
            cx.state.land_from = ActionId::GROUND_POUND;
            cx.timeline.slot = super::ground::slot::LAND;
            return cx.goto(ActionId::LANDING, fall_speed.max(0.0) as u32);
        }
        cx.timeline.slot = slot::GROUND_POUND;
        ActionResult::Stay
    }
}

/// Wall kick entry: kick away from the wall. Numbers are (verify);
/// the ID is verified (wiki:Wall Kick rev 19311).
pub fn enter_wall_kick(cx: &mut ActionCx, wall_normal: glam::Vec3) -> ActionResult {
    // spec: wall_kick.vertical / wall_kick.forward (verify)
    let away = crate::trig::atan2(wall_normal.x, wall_normal.z);
    cx.state.face_yaw = away;
    cx.state.forward_speed = 20.0;
    set_air_velocity(cx, 52.0);
    cx.timeline.slot = slot::WALL_KICK;
    cx.events.push(Event::Jumped { velocity_y: 52.0 });
    cx.goto(ActionId::WALL_KICK, 0)
}

/// Backwards air knockback (dive bonk). Numbers are (verify).
pub fn enter_air_knockback(cx: &mut ActionCx, wall_normal: glam::Vec3) -> ActionResult {
    // spec: knockback.vertical / knockback.backward_speed (verify)
    let away = crate::trig::atan2(wall_normal.x, wall_normal.z);
    cx.state.face_yaw = away;
    cx.state.forward_speed = -15.0;
    set_air_velocity(cx, 40.0);
    cx.timeline.slot = slot::AIR_KNOCKBACK;
    cx.goto(ActionId::AIR_KNOCKBACK, 0)
}

/// Try a ledge grab after a wall hit while falling.
/// Conditions from wiki:Ledge Grab@19518:
/// - wall 30 units above Mario, no wall 150 units above,
/// - then a top-down floor search from 60 units into the wall and 160 up.
fn try_ledge_grab(cx: &mut ActionCx, wall_normal: glam::Vec3) -> Option<ActionResult> {
    // Probe with Mario's own radius (+1 for the contact boundary): "is there
    // a wall at this point", not "within reach".
    let probe_r = cx.params.radius + 1.0;
    let above_30 = cx.state.pos + glam::Vec3::new(0.0, 30.0, 0.0);
    let above_150 = cx.state.pos + glam::Vec3::new(0.0, 150.0, 0.0);
    // spec: ledge.wall_check_low / ledge.wall_check_high (verified)
    if !cx.world.wall_probe(above_30, probe_r) {
        return None;
    }
    if cx.world.wall_probe(above_150, probe_r) {
        return None;
    }
    // Into the wall, 60 units; the radius eats 50, leaving 10 into the wall.
    let mut into_wall = glam::Vec3::new(-wall_normal.x, 0.0, -wall_normal.z);
    if into_wall.length_squared() < 1e-8 {
        return None;
    }
    into_wall = into_wall.normalize();
    // spec: ledge.search_inward / ledge.search_up (verified)
    let search = cx.state.pos + into_wall * 60.0 + glam::Vec3::new(0.0, 160.0, 0.0);
    let floor = cx.world.find_floor(search, 0.0)?;
    // Grab: snap to the floor at the search xz (10 units into the wall,
    // per the wiki), face the wall, kill velocity.
    cx.state.pos = glam::Vec3::new(search.x, floor.y, search.z);
    cx.state.vel = glam::Vec3::ZERO;
    cx.state.forward_speed = 0.0;
    cx.state.face_yaw = crate::trig::atan2(into_wall.x, into_wall.z);
    cx.state.floor_y = Some(floor.y);
    cx.timeline.slot = slot::LEDGE_GRAB;
    Some(cx.goto(ActionId::LEDGE_GRAB, 0))
}

/// Wall kick: a jump away from the wall; can chain off further walls.
pub struct WallKick;
impl ActionHandler for WallKick {
    fn name(&self) -> &'static str {
        "WallKick"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = air_common(
            cx,
            prev_buttons,
            &AirConfig {
                height_control: false, // (verify)
                can_ledge_grab: true,  // (verify)
                ..base_air_config()
            },
        ) {
            return r;
        }
        cx.timeline.slot = slot::WALL_KICK;
        ActionResult::Stay
    }
}

/// Backwards air knockback: stunned, drifting away from the wall.
pub struct AirKnockback;
impl ActionHandler for AirKnockback {
    fn name(&self) -> &'static str {
        "AirKnockback"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = air_common(
            cx,
            prev_buttons,
            &AirConfig {
                height_control: false,
                steer_rate: 0, // stunned (verify)
                air_drag_threshold: 32.0,
                allow_dive: false,
                allow_pound: false,
                can_ledge_grab: false,
                ..base_air_config()
            },
        ) {
            return r;
        }
        cx.timeline.slot = slot::AIR_KNOCKBACK;
        ActionResult::Stay
    }
}

/// Ledge grab: hanging on the ledge. A climbs up, Z drops.
/// Simplified from wiki:Ledge Grab@19518 (no slow-climb variants in v1).
pub struct LedgeGrab;
impl ActionHandler for LedgeGrab {
    fn name(&self) -> &'static str {
        "LedgeGrab"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        // A -> climb onto the ledge.
        if cx.pressed(buttons::A, prev_buttons) {
            // v1: stand up in place; the ground step keeps us on the floor.
            cx.timeline.slot = super::ground::slot::IDLE;
            return cx.goto(ActionId::IDLE, 0);
        }
        // Z -> let go: drop away from the wall.
        if cx.pressed(buttons::Z, prev_buttons) {
            cx.state.forward_speed = -8.0; // spec: ledge.drop_speed (verified)
            cx.state.vel = glam::Vec3::ZERO;
            cx.timeline.slot = slot::FREEFALL;
            return cx.goto(ActionId::FREEFALL, 0);
        }
        // Auto-climb after the hang timer (simplified).
        if cx.state.action_timer >= 10 {
            cx.timeline.slot = super::ground::slot::IDLE;
            return cx.goto(ActionId::IDLE, 0);
        }
        cx.state.vel = glam::Vec3::ZERO;
        cx.timeline.slot = slot::LEDGE_GRAB;
        ActionResult::Stay
    }
}
