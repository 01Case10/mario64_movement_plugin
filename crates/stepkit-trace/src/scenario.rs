//! Scenario files (RON): our own geometry, a start state, and an input
//! sequence. Geometry is always ours -- simple primitives, never level data.

use serde::{Deserialize, Serialize};

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
        };
        let text = ron::ser::to_string_pretty(&s, ron::ser::PrettyConfig::default()).unwrap();
        let back: Scenario = ron::de::from_str(&text).unwrap();
        assert_eq!(s, back);
    }
}
