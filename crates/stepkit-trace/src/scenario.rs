//! Scenario files (RON): our own geometry, a start state, and an input
//! sequence. Geometry is always ours -- simple primitives, never level data.

use serde::{Deserialize, Serialize};
use stepkit_core::actions::ActionRegistry;
use stepkit_core::state::ActionId;

use crate::trace::TraceFrame;

/// A box volume: `kind` indexes the surface table (0 = default).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoxGeom {
    pub min: (f32, f32, f32),
    pub max: (f32, f32, f32),
    pub kind: u8,
}

/// A sloped plane (ramp) for slope tests.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RampGeom {
    /// Foot of the ramp.
    pub origin: (f32, f32, f32),
    /// Horizontal run direction and length, rise height.
    pub dir: (f32, f32),
    pub length: f32,
    pub height: f32,
    pub width: f32,
    pub kind: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Geometry {
    Box(BoxGeom),
    Ramp(RampGeom),
}

/// A water surface over an XZ rectangle.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WaterGeom {
    pub y: f32,
    pub min_x: f32,
    pub max_x: f32,
    pub min_z: f32,
    pub max_z: f32,
}

/// Character start state for a scenario.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StartState {
    pub pos: (f32, f32, f32),
    /// Facing yaw in degrees.
    pub yaw_deg: f32,
    /// Action name (resolved through the action registry).
    pub action: String,
}

/// Camera model for a scenario.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Camera {
    /// Fixed yaw in degrees.
    Fixed { yaw_deg: f32 },
}

/// One input segment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum InputSeg {
    /// Hold the same input for `frames` frames.
    Hold {
        frames: u32,
        stick: (i8, i8),
        buttons: u16,
    },
    /// A single-frame button press with optional stick.
    Press { stick: (i8, i8), buttons: u16 },
}

/// Which tolerance profile to compare with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CompareProfile {
    GroundDefault,
    AirDefault,
}

/// A semantic assertion over a recorded trace, evaluated by the scenario
/// gate alongside the golden comparison. Golden diffs can miss behavior
/// bugs when the golden itself was recorded from buggy behavior (e.g. a
/// ledge grab that snapped the feet to the ledge top); assertions check
/// ground-truth properties at authoring time instead.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Assertion {
    /// At least one recorded frame carries this action.
    ActionSeen { action: String },
    /// No recorded frame may carry this action.
    ActionNever { action: String },
    /// The frame at this index carries exactly this action.
    ActionAtFrame { frame: u32, action: String },
    /// This action appears in at least `min_frames` frames total.
    /// Catches actions that exit after a single frame.
    ActionFramesAtLeast { action: String, min_frames: u32 },
    /// On every frame carrying this action, `pos.y` lies within
    /// `expected ± tol`. Fails if the action never occurs.
    DuringActionPosY {
        action: String,
        expected: f32,
        tol: f32,
    },
    /// On every frame carrying this action, forward speed lies within
    /// `[min, max]`.
    DuringActionSpeed {
        action: String,
        min: f32,
        max: f32,
    },
    /// The vertical velocity on the first recorded frame carrying this
    /// action lies within `expected ± tol`. Encodes decomp-verified entry
    /// velocities (jump vy, rollout vy, etc.). Fails if the action never
    /// occurs.
    ActionEntryVelY {
        action: String,
        expected: f32,
        tol: f32,
    },
    /// The forward speed on the first recorded frame carrying this action
    /// lies within `expected ± tol`. Encodes decomp-verified entry
    /// multipliers (long-jump x1.5, dive +15, wall-kick minimum, etc.).
    /// Fails if the action never occurs.
    ActionEntrySpeed {
        action: String,
        expected: f32,
        tol: f32,
    },
}

/// A complete scenario definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    pub id: String,
    pub tags: Vec<String>,
    pub geometry: Vec<Geometry>,
    #[serde(default)]
    pub water: Vec<WaterGeom>,
    pub start: StartState,
    pub camera: Camera,
    pub inputs: Vec<InputSeg>,
    pub compare: CompareProfile,
    /// Semantic assertions checked by the scenario gate. Defaults to empty
    /// so older .ron files keep parsing.
    #[serde(default)]
    pub asserts: Vec<Assertion>,
}

impl Scenario {
    /// Expand the input segments into per-frame `(stick_x, stick_y, buttons)`.
    pub fn expand_inputs(&self) -> Vec<(i8, i8, u16)> {
        let mut out = Vec::new();
        for seg in &self.inputs {
            match seg {
                InputSeg::Hold {
                    frames,
                    stick,
                    buttons,
                } => {
                    for _ in 0..*frames {
                        out.push((stick.0, stick.1, *buttons));
                    }
                }
                InputSeg::Press { stick, buttons } => out.push((stick.0, stick.1, *buttons)),
            }
        }
        out
    }

    /// Total frame count of the scenario.
    pub fn frame_count(&self) -> usize {
        self.expand_inputs().len()
    }
}

/// Evaluate a scenario's semantic assertions against recorded frames.
/// Returns one human-readable failure message per violated assertion; an
/// empty vec means every assertion passed. Unknown action names are
/// reported as failures (authoring errors must not silently pass).
pub fn check_assertions(
    scenario: &Scenario,
    frames: &[TraceFrame],
    registry: &ActionRegistry,
) -> Vec<String> {
    let mut failures: Vec<String> = Vec::new();
    let resolve = |action: &str| registry.id_by_name(action);

    for a in &scenario.asserts {
        match a {
            Assertion::ActionSeen { action } => {
                let Some(id) = resolve(action) else {
                    failures.push(format!(
                        "assertion failed: ActionSeen{{action: \"{action}\"}}: unknown action \"{action}\""
                    ));
                    continue;
                };
                if !frames.iter().any(|f| f.action == id.0) {
                    failures.push(format!(
                        "assertion failed: ActionSeen{{action: \"{action}\"}}: action never occurred in {} frames",
                        frames.len()
                    ));
                }
            }
            Assertion::ActionNever { action } => {
                let Some(id) = resolve(action) else {
                    failures.push(format!(
                        "assertion failed: ActionNever{{action: \"{action}\"}}: unknown action \"{action}\""
                    ));
                    continue;
                };
                if let Some(f) = frames.iter().find(|f| f.action == id.0) {
                    failures.push(format!(
                        "assertion failed: ActionNever{{action: \"{action}\"}}: occurred at frame {}",
                        f.frame
                    ));
                }
            }
            Assertion::ActionAtFrame { frame, action } => {
                let Some(id) = resolve(action) else {
                    failures.push(format!(
                        "assertion failed: ActionAtFrame{{frame: {frame}, action: \"{action}\"}}: unknown action \"{action}\""
                    ));
                    continue;
                };
                match frames.get(*frame as usize) {
                    None => failures.push(format!(
                        "assertion failed: ActionAtFrame{{frame: {frame}, action: \"{action}\"}}: frame {frame} out of range ({} frames)",
                        frames.len()
                    )),
                    Some(f) if f.action != id.0 => failures.push(format!(
                        "assertion failed: ActionAtFrame{{frame: {frame}, action: \"{action}\"}}: frame {frame} has action \"{}\", expected \"{action}\"",
                        registry.action_name(ActionId(f.action))
                    )),
                    Some(_) => {}
                }
            }
            Assertion::ActionFramesAtLeast { action, min_frames } => {
                let Some(id) = resolve(action) else {
                    failures.push(format!(
                        "assertion failed: ActionFramesAtLeast{{action: \"{action}\", min_frames: {min_frames}}}: unknown action \"{action}\""
                    ));
                    continue;
                };
                let count = frames.iter().filter(|f| f.action == id.0).count();
                if (count as u32) < *min_frames {
                    failures.push(format!(
                        "assertion failed: ActionFramesAtLeast{{action: \"{action}\", min_frames: {min_frames}}}: occurred in {count} frames"
                    ));
                }
            }
            Assertion::DuringActionPosY {
                action,
                expected,
                tol,
            } => {
                let Some(id) = resolve(action) else {
                    failures.push(format!(
                        "assertion failed: DuringActionPosY{{action: \"{action}\"}}: unknown action \"{action}\""
                    ));
                    continue;
                };
                let hits: Vec<&TraceFrame> =
                    frames.iter().filter(|f| f.action == id.0).collect();
                if hits.is_empty() {
                    failures.push(format!(
                        "assertion failed: DuringActionPosY{{action: \"{action}\"}}: action never occurred"
                    ));
                    continue;
                }
                for f in hits {
                    if (f.pos_y - expected).abs() > *tol {
                        failures.push(format!(
                            "assertion failed: DuringActionPosY{{action: \"{action}\"}}: frame {} pos.y={:.1}, expected {:.1}±{:.1}",
                            f.frame, f.pos_y, expected, tol
                        ));
                        break;
                    }
                }
            }
            Assertion::DuringActionSpeed { action, min, max } => {
                let Some(id) = resolve(action) else {
                    failures.push(format!(
                        "assertion failed: DuringActionSpeed{{action: \"{action}\"}}: unknown action \"{action}\""
                    ));
                    continue;
                };
                for f in frames.iter().filter(|f| f.action == id.0) {
                    if f.fwd_speed < *min || f.fwd_speed > *max {
                        failures.push(format!(
                            "assertion failed: DuringActionSpeed{{action: \"{action}\"}}: frame {} fwd_speed={:.1}, expected within [{min:.1}, {max:.1}]",
                            f.frame, f.fwd_speed
                        ));
                        break;
                    }
                }
            }
            Assertion::ActionEntryVelY {
                action,
                expected,
                tol,
            } => {
                let Some(id) = resolve(action) else {
                    failures.push(format!(
                        "assertion failed: ActionEntryVelY{{action: \"{action}\"}}: unknown action \"{action}\""
                    ));
                    continue;
                };
                let Some(first) = frames.iter().find(|f| f.action == id.0) else {
                    failures.push(format!(
                        "assertion failed: ActionEntryVelY{{action: \"{action}\"}}: action never occurred"
                    ));
                    continue;
                };
                if (first.vel_y - expected).abs() > *tol {
                    failures.push(format!(
                        "assertion failed: ActionEntryVelY{{action: \"{action}\"}}: frame {} vel_y={:.1}, expected {:.1}±{:.1}",
                        first.frame, first.vel_y, expected, tol
                    ));
                }
            }
            Assertion::ActionEntrySpeed {
                action,
                expected,
                tol,
            } => {
                let Some(id) = resolve(action) else {
                    failures.push(format!(
                        "assertion failed: ActionEntrySpeed{{action: \"{action}\"}}: unknown action \"{action}\""
                    ));
                    continue;
                };
                let Some(first) = frames.iter().find(|f| f.action == id.0) else {
                    failures.push(format!(
                        "assertion failed: ActionEntrySpeed{{action: \"{action}\"}}: action never occurred"
                    ));
                    continue;
                };
                if (first.fwd_speed - expected).abs() > *tol {
                    failures.push(format!(
                        "assertion failed: ActionEntrySpeed{{action: \"{action}\"}}: frame {} fwd_speed={:.1}, expected {:.1}±{:.1}",
                        first.frame, first.fwd_speed, expected, tol
                    ));
                }
            }
        }
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_hold_and_press() {
        let s = Scenario {
            id: "t".into(),
            tags: vec![],
            geometry: vec![],
            water: vec![],
            start: StartState {
                pos: (0.0, 0.0, 0.0),
                yaw_deg: 0.0,
                action: "Idle".into(),
            },
            camera: Camera::Fixed { yaw_deg: 0.0 },
            inputs: vec![
                InputSeg::Hold {
                    frames: 3,
                    stick: (0, 80),
                    buttons: 0,
                },
                InputSeg::Press {
                    stick: (0, 0),
                    buttons: 1,
                },
            ],
            compare: CompareProfile::GroundDefault,
            asserts: vec![],
        };
        let frames = s.expand_inputs();
        assert_eq!(frames.len(), 4);
        assert_eq!(frames[3], (0, 0, 1));
    }

    #[test]
    fn scenario_ron_round_trip() {
        let s = Scenario {
            id: "walk_accel_mag80_flat".into(),
            tags: vec!["ground".into(), "walking".into(), "anchor".into()],
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
                frames: 45,
                stick: (0, 80),
                buttons: 0,
            }],
            compare: CompareProfile::GroundDefault,
            asserts: vec![],
        };
        let text = ron::ser::to_string_pretty(&s, ron::ser::PrettyConfig::default()).unwrap();
        let back: Scenario = ron::de::from_str(&text).unwrap();
        assert_eq!(s, back);
    }

    fn tframe(action: u32, pos_y: f32, fwd_speed: f32, idx: u32) -> TraceFrame {
        TraceFrame {
            frame: idx,
            stick_x: 0,
            stick_y: 0,
            buttons: 0,
            cam_yaw: 0,
            pos_x: 0.0,
            pos_y,
            pos_z: 0.0,
            vel_x: 0.0,
            vel_y: 0.0,
            vel_z: 0.0,
            slide_vel_x: 0.0,
            slide_vel_z: 0.0,
            fwd_speed,
            face_yaw: 0,
            face_pitch: 0,
            face_roll: 0,
            action,
            prev_action: 0,
            land_from: 0,
            action_state: 0,
            action_timer: 0,
            action_arg: 0,
            floor_y: 0.0,
            ceil_y: 0.0,
            wall_hit: 0,
            wall_kick_timer: 0,
            wall_normal_yaw: 0,
            health: 0,
            squish_timer: 0,
            quicksand_depth: 0.0,
            peak_height: 0.0,
            slide_over_cap: 0,
            swim_strength: 0,
            anim_slot: String::new(),
            anim_frame: 0,
        }
    }

    fn scenario_with(asserts: Vec<Assertion>) -> Scenario {
        Scenario {
            id: "assert_test".into(),
            tags: vec![],
            geometry: vec![],
            water: vec![],
            start: StartState {
                pos: (0.0, 0.0, 0.0),
                yaw_deg: 0.0,
                action: "Idle".into(),
            },
            camera: Camera::Fixed { yaw_deg: 0.0 },
            inputs: vec![],
            compare: CompareProfile::GroundDefault,
            asserts,
        }
    }

    #[test]
    fn assertions_action_seen_and_never() {
        use stepkit_core::actions::ActionRegistry;
        let reg = ActionRegistry::sm64_style();
        let grab = ActionId::LEDGE_GRAB.0;
        let frames = vec![
            tframe(ActionId::IDLE.0, 0.0, 0.0, 0),
            tframe(grab, 240.0, 0.0, 1),
        ];
        let s = scenario_with(vec![
            Assertion::ActionSeen {
                action: "LedgeGrab".into(),
            },
            Assertion::ActionNever {
                action: "ButtSlide".into(),
            },
        ]);
        assert!(check_assertions(&s, &frames, &reg).is_empty());

        // Seen fails when absent; Never fails when present.
        let s = scenario_with(vec![Assertion::ActionSeen {
            action: "ButtSlide".into(),
        }]);
        let fails = check_assertions(&s, &frames, &reg);
        assert_eq!(fails.len(), 1);
        assert!(fails[0].contains("never occurred"), "{}", fails[0]);

        let s = scenario_with(vec![Assertion::ActionNever {
            action: "LedgeGrab".into(),
        }]);
        let fails = check_assertions(&s, &frames, &reg);
        assert_eq!(fails.len(), 1);
        assert!(fails[0].contains("frame 1"), "{}", fails[0]);
    }

    #[test]
    fn assertions_pos_y_catches_wrong_hang() {
        use stepkit_core::actions::ActionRegistry;
        let reg = ActionRegistry::sm64_style();
        let grab = ActionId::LEDGE_GRAB.0;
        let a = Assertion::DuringActionPosY {
            action: "LedgeGrab".into(),
            expected: 240.0,
            tol: 5.0,
        };
        let s = scenario_with(vec![a.clone()]);
        // Correct hang passes.
        let frames = vec![tframe(grab, 240.0, 0.0, 0)];
        assert!(check_assertions(&s, &frames, &reg).is_empty());
        // Feet snapped to the ledge top (the old bug) fails.
        let frames = vec![tframe(grab, 400.0, 0.0, 3)];
        let fails = check_assertions(&s, &frames, &reg);
        assert_eq!(fails.len(), 1);
        assert!(fails[0].contains("frame 3"), "{}", fails[0]);
        assert!(fails[0].contains("pos.y=400.0"), "{}", fails[0]);
        assert!(fails[0].contains("240.0±5.0"), "{}", fails[0]);
        // Action never occurring fails too.
        let frames = vec![tframe(ActionId::IDLE.0, 0.0, 0.0, 0)];
        let fails = check_assertions(&s, &frames, &reg);
        assert_eq!(fails.len(), 1);
        assert!(fails[0].contains("never occurred"), "{}", fails[0]);
    }

    #[test]
    fn assertions_frames_at_least_and_at_frame() {
        use stepkit_core::actions::ActionRegistry;
        let reg = ActionRegistry::sm64_style();
        let slide = ActionId::BUTT_SLIDE.0;
        // One frame (the old standstill bug) fails a 20-frame minimum.
        let frames = vec![tframe(slide, 0.0, 10.0, 0)];
        let s = scenario_with(vec![Assertion::ActionFramesAtLeast {
            action: "ButtSlide".into(),
            min_frames: 20,
        }]);
        let fails = check_assertions(&s, &frames, &reg);
        assert_eq!(fails.len(), 1);
        assert!(fails[0].contains("1 frames"), "{}", fails[0]);
        // Twenty frames pass.
        let frames: Vec<_> = (0..20).map(|i| tframe(slide, 0.0, 10.0, i)).collect();
        assert!(check_assertions(&s, &frames, &reg).is_empty());

        // Exact frame check.
        let s = scenario_with(vec![Assertion::ActionAtFrame {
            frame: 1,
            action: "ButtSlide".into(),
        }]);
        let frames = vec![
            tframe(ActionId::IDLE.0, 0.0, 0.0, 0),
            tframe(slide, 0.0, 10.0, 1),
        ];
        assert!(check_assertions(&s, &frames, &reg).is_empty());
        let frames = vec![
            tframe(ActionId::IDLE.0, 0.0, 0.0, 0),
            tframe(ActionId::IDLE.0, 0.0, 0.0, 1),
        ];
        let fails = check_assertions(&s, &frames, &reg);
        assert_eq!(fails.len(), 1);
        assert!(fails[0].contains("\"Idle\""), "{}", fails[0]);
    }

    #[test]
    fn assertions_unknown_action_is_a_failure() {
        use stepkit_core::actions::ActionRegistry;
        let reg = ActionRegistry::sm64_style();
        let s = scenario_with(vec![Assertion::ActionSeen {
            action: "NotARealAction".into(),
        }]);
        let fails = check_assertions(&s, &[], &reg);
        assert_eq!(fails.len(), 1);
        assert!(fails[0].contains("unknown action"), "{}", fails[0]);
    }

    #[test]
    fn assertions_entry_vel_y() {
        use stepkit_core::actions::ActionRegistry;
        let reg = ActionRegistry::sm64_style();
        let s = scenario_with(vec![Assertion::ActionEntryVelY {
            action: "SingleJump".into(),
            expected: 42.0,
            tol: 1.0,
        }]);
        let mut jump_frame = tframe(ActionId::JUMP.0, 0.0, 0.0, 1);
        jump_frame.vel_y = 42.0;
        let frames = vec![tframe(ActionId::IDLE.0, 0.0, 0.0, 0), jump_frame];
        assert!(check_assertions(&s, &frames, &reg).is_empty());
        // Wrong entry velocity fails.
        let mut bad_frame = tframe(ActionId::JUMP.0, 0.0, 0.0, 1);
        bad_frame.vel_y = 62.0;
        let frames = vec![tframe(ActionId::IDLE.0, 0.0, 0.0, 0), bad_frame];
        let fails = check_assertions(&s, &frames, &reg);
        assert_eq!(fails.len(), 1);
        assert!(fails[0].contains("vel_y=62.0"), "{}", fails[0]);
        // Action never occurring fails.
        let frames = vec![tframe(ActionId::IDLE.0, 0.0, 0.0, 0)];
        let fails = check_assertions(&s, &frames, &reg);
        assert_eq!(fails.len(), 1);
        assert!(fails[0].contains("never occurred"), "{}", fails[0]);
    }

    #[test]
    fn assertions_entry_speed() {
        use stepkit_core::actions::ActionRegistry;
        let reg = ActionRegistry::sm64_style();
        let s = scenario_with(vec![Assertion::ActionEntrySpeed {
            action: "LongJump".into(),
            expected: 48.0,
            tol: 1.0,
        }]);
        let frames = vec![
            tframe(ActionId::IDLE.0, 0.0, 0.0, 0),
            tframe(ActionId::LONG_JUMP.0, 0.0, 48.0, 1),
        ];
        assert!(check_assertions(&s, &frames, &reg).is_empty());
        // Wrong entry speed fails.
        let frames = vec![
            tframe(ActionId::IDLE.0, 0.0, 0.0, 1),
            tframe(ActionId::LONG_JUMP.0, 0.0, 30.0, 1),
        ];
        let fails = check_assertions(&s, &frames, &reg);
        assert_eq!(fails.len(), 1);
        assert!(fails[0].contains("fwd_speed=30.0"), "{}", fails[0]);
        // Action never occurring fails.
        let frames = vec![tframe(ActionId::IDLE.0, 0.0, 0.0, 0)];
        let fails = check_assertions(&s, &frames, &reg);
        assert_eq!(fails.len(), 1);
        assert!(fails[0].contains("never occurred"), "{}", fails[0]);
    }

    #[test]
    fn assertions_ron_round_trip() {
        let s = scenario_with(vec![
            Assertion::ActionSeen {
                action: "LedgeGrab".into(),
            },
            Assertion::DuringActionPosY {
                action: "LedgeGrab".into(),
                expected: 240.0,
                tol: 5.0,
            },
        ]);
        let text = ron::ser::to_string_pretty(&s, ron::ser::PrettyConfig::default()).unwrap();
        assert!(text.contains("ActionSeen"), "{text}");
        let back: Scenario = ron::de::from_str(&text).unwrap();
        assert_eq!(s, back);
        // The compact hand-written form used in .ron files parses too.
        let compact = "(id: \"x\", tags: [], geometry: [], water: [], start: (pos: (0.0, 0.0, 0.0), yaw_deg: 0.0, action: \"Idle\"), camera: Fixed(yaw_deg: 0.0), inputs: [], compare: GroundDefault, asserts: [ActionSeen(action: \"LedgeGrab\")])";
        let parsed: Scenario = ron::de::from_str(compact).unwrap();
        assert_eq!(parsed.asserts.len(), 1);
    }
}
