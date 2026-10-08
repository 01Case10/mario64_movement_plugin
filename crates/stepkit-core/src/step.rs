//! The step engine: one deterministic `tick` per 30 Hz frame.
//!
//! Each frame the current action runs its documented cancel checks, decides
//! its velocity, then calls a step routine that splits the move into four
//! quarter-steps (wiki:Movement steps@19723). For each quarter-step the
//! routine proposes a position, queries the world, and resolves the outcome:
//! landing, wall hit, ceiling bump, or walking off a ledge.

use crate::actions::{ActionCx, ActionRegistry, ActionResult};
use crate::events::Event;
use crate::input::RawInput;
use crate::params::MovementParams;
use crate::state::CharacterState;
use crate::world::{CollisionWorld, FloorHit};
use glam::Vec3;

/// Number of sub-steps per frame (quarter steps).
/// spec: step.quarter_steps (verified: wiki:Movement steps@19723)
pub const QUARTER_STEPS: usize = 4;

/// Animation timeline state for the current slot, owned by the core so that
/// gameplay timing never depends on the renderer's animation player.
#[derive(Clone, Copy, Debug, Default)]
pub struct Timeline {
    /// Stable slot id (see `stepkit-anim` manifest).
    pub slot: u32,
    /// Current frame within the slot.
    pub frame: u32,
    /// Playback speed multiplier.
    pub speed: f32,
    /// True when a non-looping slot has finished.
    pub ended: bool,
}

/// Run one 30 Hz simulation tick: dispatch to the current action, run its
/// checks and step routine, advance timers and the timeline.
pub fn tick(
    state: CharacterState,
    input: RawInput,
    world: &dyn CollisionWorld,
    params: &MovementParams,
    timeline: Timeline,
    registry: &ActionRegistry,
    prev_buttons: u16,
) -> (CharacterState, Timeline, Vec<Event>) {
    let mut state = state;
    // action_timer counts completed ticks in this action.
    state.action_timer = state.action_timer.wrapping_add(1);
    state.warped = false;
    // Squish wears off one frame at a time, in every action.
    state.squish_timer = state.squish_timer.saturating_sub(1);

    let mut cx = ActionCx {
        state,
        input,
        world,
        params,
        timeline,
        events: Vec::new(),
    };
    let prev_slot = cx.timeline.slot;
    let result = match registry.get(cx.state.action) {
        Some(handler) => handler.tick(&mut cx, prev_buttons),
        None => ActionResult::Stay, // unknown action: hold state (defensive)
    };
    let _ = result;
    // Timeline: new slot restarts the frame counter.
    if cx.timeline.slot != prev_slot {
        cx.timeline.frame = 0;
        cx.timeline.ended = false;
    } else {
        cx.timeline.frame = cx.timeline.frame.wrapping_add(1);
    }
    // IK hints: renderer output, refreshed after the action tick.
    update_ik_hints(&mut cx.state);
    (cx.state, cx.timeline, cx.events)
}

/// Update IK hand hints (renderer output only).
///
/// During ledge-grab actions the sim publishes world-space hand targets
/// (the grabbed edge, offset laterally by 25 units) so a renderer can
/// solve arm IK. All other actions clear the hints. Output-only: never
/// read by simulation logic, never hashed, never recorded in goldens.
fn update_ik_hints(state: &mut CharacterState) {
    use crate::state::ActionId;
    let is_ledge = matches!(
        state.action,
        ActionId::LEDGE_GRAB
            | ActionId::LEDGE_CLIMB_FAST
            | ActionId::LEDGE_CLIMB_SLOW_1
            | ActionId::LEDGE_CLIMB_SLOW_2
    );
    if is_ledge {
        if let Some(edge) = state.grab_point {
            let fx = crate::trig::sin(state.face_yaw);
            let fz = crate::trig::cos(state.face_yaw);
            // Lateral axis: horizontal perpendicular of the facing.
            let offset = Vec3::new(-fz * 25.0, 0.0, fx * 25.0);
            state.ik_hand_l = Some(edge + offset);
            state.ik_hand_r = Some(edge - offset);
            return;
        }
    }
    state.ik_hand_l = None;
    state.ik_hand_r = None;
}

/// Output of the ground step routine.
pub struct GroundStepOut {
    pub pos: Vec3,
    pub floor: Option<FloorHit>,
    pub wall_hit: bool,
    pub wall_normal: Vec3,
    pub walked_off: bool,
}

/// Ground step: move horizontally in quarter-steps, snap to the floor,
/// push out of walls, detect walking off ledges.
pub fn step_ground(
    state: &CharacterState,
    vx: f32,
    vz: f32,
    world: &dyn CollisionWorld,
    params: &MovementParams,
) -> GroundStepOut {
    let mut pos = state.pos;
    let mut floor = world.find_floor(pos, 1.0);
    let mut wall_hit = false;
    let mut wall_normal = Vec3::ZERO;
    let mut walked_off = false;
    for _ in 0..QUARTER_STEPS {
        // Project horizontal velocity onto the slope via floor normal.
        let ny = floor.map(|f| f.normal.y).unwrap_or(1.0);
        let step = Vec3::new(vx * 0.25 * ny, 0.0, vz * 0.25 * ny);
        let mut proposed = pos + step;
        // Walls: dual radii (upper/lower).
        // Lower wall: offset 30, radius 24. Upper wall: offset 60, radius 50.
        let wr_low = world.resolve_walls(proposed, 30.0, 24.0);
        let wr_high = world.resolve_walls(proposed, 60.0, 50.0);
        let wr = if wr_low.hit { wr_low } else { wr_high };
        if wr.hit {
            // Wall yaw window: glancing hits (wall-normal yaw vs facing
            // between 0x2AAA and 0x5555, ~30-75 deg) slide along -- keep
            // stepping with the resolved position and no wall event.
            // Head-on hits stop as before.
            let normal_yaw = crate::trig::atan2(wr.normal.x, wr.normal.z);
            let delta = state.face_yaw.diff_to(normal_yaw).abs();
            proposed = wr.pos;
            if !(0x2AAA..=0x5555).contains(&delta) {
                wall_hit = true;
                wall_normal = wr.normal;
            }
        }
        // Floor with +100 step-up test.
        match world.find_floor(proposed, 1.0) {
            None => {
                walked_off = true;
                pos = proposed;
            }
            Some(f) => {
                // Ceiling check: +160.
                if let Some(c) = world.find_ceiling(proposed, 160.0) {
                    if f.y + 160.0 >= c.y {
                        wall_hit = true;
                        // (keep pre-step pos)
                        continue;
                    }
                }
                if f.y > pos.y + 100.0 {
                    // Too tall: treat as wall, revert.
                    wall_hit = true;
                    // (keep pre-step pos)
                } else if f.y < pos.y - params.ground_step_down {
                    walked_off = true;
                    pos = proposed;
                } else {
                    pos = Vec3::new(proposed.x, f.y, proposed.z);
                    floor = Some(f);
                }
            }
        }
    }
    GroundStepOut {
        pos,
        floor,
        wall_hit,
        wall_normal,
        walked_off,
    }
}

/// Output of the air step routine.
pub struct AirStepOut {
    pub pos: Vec3,
    /// Vertical velocity after ceiling checks.
    pub vel_y: f32,
    /// `Some` when a landing happened this frame.
    pub floor: Option<FloorHit>,
    /// Downward speed at the moment of landing.
    pub fall_speed: f32,
    pub wall_hit: bool,
    pub wall_normal: Vec3,
}

/// Air step: move in quarter-steps; land when within the snap window below
/// a floor (wiki:Movement steps@19723), zero upward velocity on ceilings
/// within the ceiling window, push out of walls.
pub fn step_air(
    state: &CharacterState,
    world: &dyn CollisionWorld,
    params: &MovementParams,
) -> AirStepOut {
    let mut pos = state.pos;
    let mut vy = state.vel.y;
    let vx = state.vel.x;
    let vz = state.vel.z;
    let mut floor = None;
    let mut fall_speed = 0.0;
    let mut wall_hit = false;
    let mut wall_normal = Vec3::ZERO;
    for _ in 0..QUARTER_STEPS {
        let mut proposed = pos + Vec3::new(vx * 0.25, vy * 0.25, vz * 0.25);
        // Ceiling: within the window above and moving up -> stop rising.
        // spec: step.ceiling_zero_vel_window (verified: wiki:Movement steps@19723)
        if let Some(c) = world.find_ceiling(proposed, params.ceiling_zero_vel_window) {
            if vy >= 0.0 {
                vy = 0.0;
            }
            if proposed.y > c.y - 1.0 {
                proposed.y = c.y - 1.0;
            }
        }
        // Walls. Only hits more head-on than 0x6000 (67.5 deg) count as
        // wall hits; shallower angles graze (slide along, no wall event).
        let wr = world.resolve_walls(proposed, params.height * 0.5, params.radius);
        if wr.hit {
            let normal_yaw = crate::trig::atan2(wr.normal.x, wr.normal.z);
            let delta = state.face_yaw.diff_to(normal_yaw).abs();
            proposed = wr.pos;
            if delta > 0x6000 {
                wall_hit = true;
                wall_normal = wr.normal;
            }
        }
        // Landing: the highest floor at/below us must be within the snap
        // window (78) below, and we must not be rising.
        // spec: step.air_landing_snap_window (verified: wiki:Movement steps@19723)
        if vy <= 0.0 {
            if let Some(f) = world.find_floor(proposed, 0.0) {
                if proposed.y - f.y <= params.air_landing_snap_window {
                    fall_speed = -vy;
                    // Pedro-spot rule: only move horizontally onto the floor
                    // when there is headroom (ceiling - floor > 160);
                    // otherwise land vertically at the current xz.
                    let headroom_ok = world
                        .find_ceiling(Vec3::new(proposed.x, f.y, proposed.z), 4096.0)
                        .is_none_or(|c| c.y - f.y > 160.0);
                    pos = if headroom_ok {
                        Vec3::new(proposed.x, f.y, proposed.z)
                    } else {
                        Vec3::new(pos.x, f.y, pos.z)
                    };
                    floor = Some(f);
                    vy = 0.0;
                    break;
                }
            }
        }
        pos = proposed;
    }
    AirStepOut {
        pos,
        vel_y: vy,
        floor,
        fall_speed,
        wall_hit,
        wall_normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ActionRegistry;
    use crate::input::RawInput;
    use crate::surface::SurfaceWorld;

    fn flat_world() -> SurfaceWorld {
        let mut w = SurfaceWorld::new();
        w.add_box(
            Vec3::new(-2000.0, -100.0, -2000.0),
            Vec3::new(2000.0, 0.0, 2000.0),
            crate::world::SurfaceKind::Default,
            0,
        );
        w
    }

    #[test]
    fn ik_hints_set_during_ledge_grab() {
        use crate::state::ActionId;
        // Simulate a grab: ledge action + recorded edge point.
        let mut state = CharacterState {
            action: ActionId::LEDGE_GRAB,
            grab_point: Some(Vec3::new(0.0, 400.0, 400.0)),
            face_yaw: crate::angles::Angle::ZERO, // facing +z
            ..Default::default()
        };
        update_ik_hints(&mut state);
        assert!(state.ik_hand_l.is_some());
        assert!(state.ik_hand_r.is_some());
        let l = state.ik_hand_l.unwrap();
        let r = state.ik_hand_r.unwrap();
        // Hands offset laterally (±25) from the edge, at edge height.
        assert!((l.y - 400.0).abs() < 0.01);
        assert!((r.y - 400.0).abs() < 0.01);
        assert!((l - r).length() > 49.0 && (l - r).length() < 51.0);
        // Leaving the ledge clears the hints.
        state.action = ActionId::IDLE;
        update_ik_hints(&mut state);
        assert!(state.ik_hand_l.is_none());
        assert!(state.ik_hand_r.is_none());
    }

    #[test]
    fn idle_tick_stays_idle_without_input() {
        let world = flat_world();
        let registry = ActionRegistry::sm64_style();
        let (out, _, events) = tick(
            CharacterState::default(),
            RawInput::default(),
            &world,
            &MovementParams::default(),
            Timeline::default(),
            &registry,
            0,
        );
        assert_eq!(out.action, crate::state::ActionId::IDLE);
        assert!(events.is_empty());
    }

    #[test]
    fn determinism_10k_frames() {
        // Phase 2 gate: a scripted 10,000-frame scenario hashes identically
        // across runs (CI also builds for aarch64).
        let world = flat_world();
        let registry = ActionRegistry::sm64_style();
        let params = MovementParams::default();
        let run = || {
            let mut state = CharacterState::default();
            let mut timeline = Timeline::default();
            let mut prev_buttons = 0u16;
            for frame in 0..10_000u32 {
                let stick_y = if frame < 5000 { 80 } else { 0 };
                let input = RawInput {
                    stick_x: 0,
                    stick_y,
                    buttons: 0,
                    cam_yaw: crate::angles::Angle::ZERO,
                };
                let (s, t, _) = tick(
                    state,
                    input,
                    &world,
                    &params,
                    timeline,
                    &registry,
                    prev_buttons,
                );
                state = s;
                timeline = t;
                prev_buttons = input.buttons;
            }
            state.hash_state()
        };
        let h1 = run();
        let h2 = run();
        assert_eq!(h1, h2);
        assert_ne!(h1, CharacterState::default().hash_state());
    }
}
