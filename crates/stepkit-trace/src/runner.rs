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
use stepkit_core::world::{CeilingHit, CollisionWorld, FloorHit, SurfaceKind, WallResolve};

/// Collision backend built from a scenario's own geometry (boxes and ramps).
/// The reference `SurfaceWorld` arrives in phase 2; this backend exists so
/// the data pipeline round-trips end to end in phase 1.
#[derive(Clone, Debug, Default)]
pub struct ScenarioWorld {
    boxes: Vec<ScenarioBox>,
    ramps: Vec<ScenarioRamp>,
}

#[derive(Clone, Debug)]
struct ScenarioBox {
    min: Vec3,
    max: Vec3,
    kind: SurfaceKind,
}

#[derive(Clone, Debug)]
struct ScenarioRamp {
    origin: Vec3,
    dir: Vec3,
    length: f32,
    height: f32,
    width: f32,
    kind: SurfaceKind,
}

impl ScenarioWorld {
    pub fn from_scenario(scenario: &Scenario) -> Self {
        let mut world = ScenarioWorld::default();
        for g in &scenario.geometry {
            match g {
                Geometry::Box(b) => world.boxes.push(ScenarioBox {
                    min: Vec3::new(b.min.0, b.min.1, b.min.2),
                    max: Vec3::new(b.max.0, b.max.1, b.max.2),
                    kind: SurfaceKind::Default,
                }),
                Geometry::Ramp(r) => {
                    let d = Vec3::new(r.dir.0, 0.0, r.dir.1);
                    world.ramps.push(ScenarioRamp {
                        origin: Vec3::new(r.origin.0, r.origin.1, r.origin.2),
                        dir: d.normalize_or_zero(),
                        length: r.length,
                        height: r.height,
                        width: r.width,
                        kind: SurfaceKind::Default,
                    });
                }
            }
        }
        world
    }

    fn floor_height_at(&self, x: f32, z: f32) -> Option<(f32, SurfaceKind)> {
        let mut best: Option<(f32, SurfaceKind)> = None;
        for b in &self.boxes {
            if x >= b.min.x && x <= b.max.x && z >= b.min.z && z <= b.max.z && b.max.y <= 0.0 + 1e6
            {
                // Top face counts as floor when the query is above it.
                let y = b.max.y;
                if best.is_none_or(|(by, _)| y > by) {
                    best = Some((y, b.kind));
                }
            }
        }
        for r in &self.ramps {
            let rel = Vec3::new(x, 0.0, z) - Vec3::new(r.origin.x, 0.0, r.origin.z);
            let along = rel.dot(r.dir);
            let across = (rel - r.dir * along).length();
            if along >= 0.0 && along <= r.length && across <= r.width * 0.5 {
                let y = r.origin.y + (along / r.length) * r.height;
                if best.is_none_or(|(by, _)| y > by) {
                    best = Some((y, r.kind));
                }
            }
        }
        best
    }
}

impl CollisionWorld for ScenarioWorld {
    fn find_floor(&self, pos: Vec3) -> Option<FloorHit> {
        // Highest surface at or below pos.y (with a small tolerance).
        let mut best: Option<FloorHit> = None;
        // Sample the point plus a small cross so edges are not missed.
        for (ox, oz) in [(0.0, 0.0), (1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            if let Some((y, kind)) = self.floor_height_at(pos.x + ox, pos.z + oz) {
                if y <= pos.y + 1.0 && best.is_none_or(|b: FloorHit| y > b.y) {
                    // Ramp normal from the slope; boxes are flat.
                    let normal = Vec3::Y;
                    best = Some(FloorHit {
                        y,
                        normal,
                        kind,
                        user_data: 0,
                    });
                }
            }
        }
        best
    }

    fn find_ceiling(&self, pos: Vec3, height: f32) -> Option<CeilingHit> {
        let mut best: Option<CeilingHit> = None;
        for b in &self.boxes {
            if pos.x >= b.min.x && pos.x <= b.max.x && pos.z >= b.min.z && pos.z <= b.max.z {
                let y = b.min.y;
                if y >= pos.y && y <= pos.y + height && best.is_none_or(|c: CeilingHit| y < c.y) {
                    best = Some(CeilingHit {
                        y,
                        normal: Vec3::NEG_Y,
                    });
                }
            }
        }
        best
    }

    fn resolve_walls(&self, pos: Vec3, _offset: f32, radius: f32) -> WallResolve {
        let mut p = pos;
        let mut hit = false;
        let mut num_walls = 0;
        let mut normal = Vec3::ZERO;
        // Push out of box sides when the feet are within the box's y-range.
        for b in &self.boxes {
            if p.y + 1.0 < b.min.y || p.y > b.max.y {
                continue;
            }
            let cx = p.x.clamp(b.min.x, b.max.x);
            let cz = p.z.clamp(b.min.z, b.max.z);
            let dx = p.x - cx;
            let dz = p.z - cz;
            let dist_sq = dx * dx + dz * dz;
            if dist_sq < radius * radius {
                // Inside or within radius: push out along the smallest penetration axis.
                let pen_x = radius - (p.x - b.min.x).abs().min((p.x - b.max.x).abs());
                let pen_z = radius - (p.z - b.min.z).abs().min((p.z - b.max.z).abs());
                if pen_x < pen_z {
                    let side = if p.x < (b.min.x + b.max.x) * 0.5 {
                        -1.0
                    } else {
                        1.0
                    };
                    p.x += side * (pen_x + 0.01);
                    normal = Vec3::new(side, 0.0, 0.0);
                } else {
                    let side = if p.z < (b.min.z + b.max.z) * 0.5 {
                        -1.0
                    } else {
                        1.0
                    };
                    p.z += side * (pen_z + 0.01);
                    normal = Vec3::new(0.0, 0.0, side);
                }
                hit = true;
                num_walls += 1;
            }
        }
        WallResolve {
            pos: p,
            hit,
            num_walls,
            normal,
        }
    }

    fn water_level(&self, _x: f32, _z: f32) -> Option<f32> {
        None
    }
}

/// Reconstruct a [`CharacterState`] from a recorded frame, for
/// teacher-forced comparison.
pub fn state_from_frame(f: &TraceFrame) -> CharacterState {
    CharacterState {
        pos: Vec3::new(f.pos_x, f.pos_y, f.pos_z),
        vel: Vec3::new(f.vel_x, f.vel_y, f.vel_z),
        forward_speed: f.fwd_speed,
        face_yaw: Angle(f.face_yaw),
        face_pitch: Angle(f.face_pitch),
        face_roll: Angle(f.face_roll),
        action: ActionId(f.action),
        prev_action: ActionId(f.prev_action),
        action_state: f.action_state,
        action_timer: f.action_timer,
        action_arg: f.action_arg,
        floor_y: Some(f.floor_y),
        ceil_y: Some(f.ceil_y),
        floor_kind: 0,
        wall_hit: f.wall_hit != 0,
        wall_kick_timer: 0,
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
        fwd_speed: state.forward_speed,
        face_yaw: state.face_yaw.0,
        face_pitch: state.face_pitch.0,
        face_roll: state.face_roll.0,
        action: state.action.0,
        prev_action: state.prev_action.0,
        action_state: state.action_state,
        action_timer: state.action_timer,
        action_arg: state.action_arg,
        floor_y: state.floor_y.unwrap_or(0.0),
        ceil_y: state.ceil_y.unwrap_or(0.0),
        wall_hit: state.wall_hit as u8,
        anim_slot: format!("slot_{}", timeline.slot),
        anim_frame: timeline.frame,
    }
}

/// Run a scenario through the core and record the trace.
pub fn run_scenario(scenario: &Scenario, params: &MovementParams, produced_by: &str) -> Trace {
    let world = ScenarioWorld::from_scenario(scenario);
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
    for (i, &(sx, sy, buttons)) in inputs.iter().enumerate() {
        let input = RawInput {
            stick_x: sx,
            stick_y: sy,
            buttons,
            cam_yaw,
        };
        let (next, next_timeline, _events) = tick(state, input, &world, params, timeline);
        state = next;
        timeline = next_timeline;
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
    fn scenario_world_finds_flat_floor() {
        let s = flat_walk_scenario(1);
        let world = ScenarioWorld::from_scenario(&s);
        let hit = world.find_floor(Vec3::new(10.0, 5.0, -3.0)).unwrap();
        assert_eq!(hit.y, 0.0);
        assert!(world.find_floor(Vec3::new(10.0, -200.0, -3.0)).is_none());
    }
}
