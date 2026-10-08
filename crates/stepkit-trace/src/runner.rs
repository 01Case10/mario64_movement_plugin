//! Scenario runner: executes a [`Scenario`] through `stepkit-core` and
//! records a [`Trace`].
//!
//! Frame convention: `frames[0]` is the start state (before any tick) and
//! `frames[i].stick_x/y, buttons` is the input applied to reach `frames[i+1]`.
//! Teacher-forced comparison reconstructs the state from `frames[i]`, applies
//! that input for one tick, and compares against `frames[i+1]`.

use crate::scenario::{Geometry, Scenario};
use crate::trace::{Trace, TraceFrame, TraceSidecar};
use glam::Vec3;
use stepkit_core::angles::Angle;
use stepkit_core::input::RawInput;
use stepkit_core::params::MovementParams;
use stepkit_core::state::{ActionId, CharacterState};
use stepkit_core::step::{tick, Timeline};
use stepkit_core::surface::SurfaceWorld;
use stepkit_core::trig;

/// Bake a scenario's geometry into the reference [`SurfaceWorld`] backend.
use stepkit_core::world::SurfaceKind;

/// Map a scenario geometry `kind` to a surface kind (0 = default).
fn kind_of(kind: u8) -> SurfaceKind {
    match kind {
        1 => SurfaceKind::Slide,
        2 => SurfaceKind::Quicksand,
        _ => SurfaceKind::Default,
    }
}

pub fn bake_scenario_world(scenario: &Scenario) -> SurfaceWorld {
    let mut world = SurfaceWorld::new();
    for g in &scenario.geometry {
        match g {
            Geometry::Box(b) => world.add_box(
                Vec3::new(b.min.0, b.min.1, b.min.2),
                Vec3::new(b.max.0, b.max.1, b.max.2),
                kind_of(b.kind),
                0,
            ),
            Geometry::Ramp(r) => {
                let d = Vec3::new(r.dir.0, 0.0, r.dir.1).normalize_or_zero();
                world.add_ramp(
                    Vec3::new(r.origin.0, r.origin.1, r.origin.2),
                    d,
                    r.length,
                    r.height,
                    r.width,
                    kind_of(r.kind),
                    0,
                );
            }
        }
    }
    for w in &scenario.water {
        world.add_water(w.y, w.min_x, w.max_x, w.min_z, w.max_z);
    }
    world
}

/// Reconstruct a [`CharacterState`] from a recorded frame, for
/// teacher-forced comparison.
pub fn state_from_frame(f: &TraceFrame) -> CharacterState {
    CharacterState {
        pos: Vec3::new(f.pos_x, f.pos_y, f.pos_z),
        vel: Vec3::new(f.vel_x, f.vel_y, f.vel_z),
        forward_speed: f.fwd_speed,
        slide_vel_x: f.slide_vel_x,
        slide_vel_z: f.slide_vel_z,
        face_yaw: Angle(f.face_yaw),
        face_pitch: Angle(f.face_pitch),
        face_roll: Angle(f.face_roll),
        action: ActionId(f.action),
        prev_action: ActionId(f.prev_action),
        land_from: ActionId(f.land_from),
        last_fall_speed: 0.0,
        health: f.health,
        squish_timer: f.squish_timer,
        peak_height: f.peak_height,
        quicksand_depth: f.quicksand_depth,
        slide_over_cap: f.slide_over_cap != 0,
        action_state: f.action_state,
        action_timer: f.action_timer,
        action_arg: f.action_arg,
        floor_y: Some(f.floor_y),
        ceil_y: Some(f.ceil_y),
        floor_kind: 0,
        wall_hit: f.wall_hit != 0,
        wall_kick_timer: f.wall_kick_timer,
        wall_normal: {
            let a = stepkit_core::angles::Angle(f.wall_normal_yaw);
            Vec3::new(trig::sin(a), 0.0, trig::cos(a))
        },
        up: Vec3::Y,
        warped: false,
    }
}

fn frame_from_state(
    frame: u32,
    input: (i8, i8, u16),
    cam_yaw: Angle,
    state: &CharacterState,
    timeline: &Timeline,
) -> TraceFrame {
    TraceFrame {
        frame,
        stick_x: input.0,
        stick_y: input.1,
        buttons: input.2,
        cam_yaw: cam_yaw.0,
        pos_x: state.pos.x,
        pos_y: state.pos.y,
        pos_z: state.pos.z,
        vel_x: state.vel.x,
        vel_y: state.vel.y,
        vel_z: state.vel.z,
        slide_vel_x: state.slide_vel_x,
        slide_vel_z: state.slide_vel_z,
        fwd_speed: state.forward_speed,
        face_yaw: state.face_yaw.0,
        face_pitch: state.face_pitch.0,
        face_roll: state.face_roll.0,
        action: state.action.0,
        prev_action: state.prev_action.0,
        land_from: state.land_from.0,
        action_state: state.action_state,
        action_timer: state.action_timer,
        action_arg: state.action_arg,
        floor_y: state.floor_y.unwrap_or(0.0),
        ceil_y: state.ceil_y.unwrap_or(0.0),
        wall_hit: state.wall_hit as u8,
        wall_kick_timer: state.wall_kick_timer,
        wall_normal_yaw: trig::atan2(state.wall_normal.x, state.wall_normal.z).0,
        health: state.health,
        squish_timer: state.squish_timer,
        quicksand_depth: state.quicksand_depth,
        peak_height: state.peak_height,
        slide_over_cap: state.slide_over_cap as u8,
        anim_slot: format!("slot_{}", timeline.slot),
        anim_frame: timeline.frame,
    }
}

/// Run a scenario through the core and record the trace.
pub fn run_scenario(scenario: &Scenario, params: &MovementParams, produced_by: &str) -> Trace {
    use stepkit_core::actions::ActionRegistry;
    let world = bake_scenario_world(scenario);
    let registry = ActionRegistry::sm64_style();
    let cam_yaw = match scenario.camera {
        crate::scenario::Camera::Fixed { yaw_deg } => Angle::from_degrees(yaw_deg),
    };
    let mut state = CharacterState {
        pos: Vec3::new(
            scenario.start.pos.0,
            scenario.start.pos.1,
            scenario.start.pos.2,
        ),
        face_yaw: Angle::from_degrees(scenario.start.yaw_deg),
        ..CharacterState::default()
    };
    // Resolve the action name through the known built-ins.
    state.action = match scenario.start.action.as_str() {
        "Idle" => ActionId::IDLE,
        "Walking" => ActionId::WALKING,
        "Jump" => ActionId::JUMP,
        "Freefall" => ActionId::FREEFALL,
        _ => ActionId::IDLE,
    };
    state.prev_action = state.action;

    let mut timeline = Timeline::default();
    let inputs = scenario.expand_inputs();
    let mut frames = Vec::with_capacity(inputs.len() + 1);
    // Frame 0: start state, carrying the first input.
    let first_input = inputs.first().copied().unwrap_or((0, 0, 0));
    frames.push(frame_from_state(0, first_input, cam_yaw, &state, &timeline));
    let mut prev_buttons = 0u16;
    for (i, &(sx, sy, buttons)) in inputs.iter().enumerate() {
        let input = RawInput {
            stick_x: sx,
            stick_y: sy,
            buttons,
            cam_yaw,
        };
        let (next, next_timeline, _events) = tick(
            state,
            input,
            &world,
            params,
            timeline,
            &registry,
            prev_buttons,
        );
        state = next;
        timeline = next_timeline;
        prev_buttons = buttons;
        let next_input = inputs.get(i + 1).copied().unwrap_or((0, 0, 0));
        frames.push(frame_from_state(
            i as u32 + 1,
            next_input,
            cam_yaw,
            &state,
            &timeline,
        ));
    }
    Trace {
        frames,
        sidecar: TraceSidecar {
            scenario_id: scenario.id.clone(),
            produced_by: produced_by.into(),
            produced_at: "2026-10-07".into(),
            faithful_params: params.faithful,
            notes: "phase 1 pipeline run".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::*;

    fn flat_walk_scenario(frames: u32) -> Scenario {
        Scenario {
            id: "test_walk".into(),
            tags: vec![],
            geometry: vec![Geometry::Box(BoxGeom {
                min: (-2000.0, -100.0, -2000.0),
                max: (2000.0, 0.0, 2000.0),
                kind: 0,
            })],
            water: vec![],
            start: StartState {
                pos: (0.0, 0.0, 0.0),
                yaw_deg: 0.0,
                action: "Idle".into(),
            },
            camera: Camera::Fixed { yaw_deg: 0.0 },
            inputs: vec![InputSeg::Hold {
                frames,
                stick: (0, 80),
                buttons: 0,
            }],
            compare: CompareProfile::GroundDefault,
        }
    }

    #[test]
    fn run_produces_n_plus_one_frames() {
        let s = flat_walk_scenario(45);
        let trace = run_scenario(&s, &MovementParams::default(), "test");
        assert_eq!(trace.frames.len(), 46);
        assert_eq!(trace.frames[0].frame, 0);
        assert_eq!(trace.frames[45].frame, 45);
    }

    #[test]
    fn baked_world_finds_flat_floor() {
        use stepkit_core::world::CollisionWorld;
        let s = flat_walk_scenario(1);
        let world = bake_scenario_world(&s);
        let hit = world.find_floor(Vec3::new(10.0, 5.0, -3.0), 1.0).unwrap();
        assert_eq!(hit.y, 0.0);
        assert!(world
            .find_floor(Vec3::new(10.0, -500.0, -3.0), 1.0)
            .is_none());
    }
}
