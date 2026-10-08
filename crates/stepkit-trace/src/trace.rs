//! Canonical trace format: CSV with a JSON sidecar.
//!
//! Floats are written in shortest round-trip form so they re-parse to the
//! identical `f32`. Only numbers ship -- never ROM data or assets.

use serde::{Deserialize, Serialize};

/// One logic frame of recorded state, matching the plan's trace format.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceFrame {
    pub frame: u32,
    pub stick_x: i8,
    pub stick_y: i8,
    pub buttons: u16,
    pub cam_yaw: i16,
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    pub vel_x: f32,
    pub vel_y: f32,
    pub vel_z: f32,
    pub fwd_speed: f32,
    pub face_yaw: i16,
    pub face_pitch: i16,
    pub face_roll: i16,
    pub action: u32,
    pub prev_action: u32,
    pub land_from: u32,
    pub action_state: u32,
    pub action_timer: u32,
    pub action_arg: u32,
    pub floor_y: f32,
    pub ceil_y: f32,
    pub wall_hit: u8,
    pub wall_kick_timer: u32,
    pub wall_normal_yaw: i16,
    pub anim_slot: String,
    pub anim_frame: u32,
}

/// JSON sidecar: scenario id, oracle/build provenance, tool versions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceSidecar {
    pub scenario_id: String,
    pub produced_by: String,
    pub produced_at: String,
    pub faithful_params: bool,
    pub notes: String,
}

/// A full trace: frames plus provenance.
#[derive(Clone, Debug, PartialEq)]
pub struct Trace {
    pub frames: Vec<TraceFrame>,
    pub sidecar: TraceSidecar,
}

impl Trace {
    /// Write frames to CSV (shortest round-trip floats).
    pub fn to_csv(&self) -> String {
        let mut w = csv::Writer::from_writer(vec![]);
        for f in &self.frames {
            w.serialize(f).expect("csv serialize");
        }
        String::from_utf8(w.into_inner().expect("csv flush")).expect("utf8")
    }

    /// Read frames from CSV.
    pub fn from_csv(text: &str) -> Result<Vec<TraceFrame>, csv::Error> {
        let mut r = csv::Reader::from_reader(text.as_bytes());
        r.deserialize().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> TraceFrame {
        TraceFrame {
            frame: 0,
            stick_x: 0,
            stick_y: 80,
            buttons: 0,
            cam_yaw: 0,
            pos_x: 1.5,
            pos_y: 0.0,
            pos_z: -2.25,
            vel_x: 0.1,
            vel_y: 0.0,
            vel_z: 0.2,
            fwd_speed: 0.3,
            face_yaw: 0,
            face_pitch: 0,
            face_roll: 0,
            action: 0x0C400201,
            prev_action: 0x0C400201,
            land_from: 0x0C400201,
            action_state: 0,
            action_timer: 0,
            action_arg: 0,
            floor_y: 0.0,
            ceil_y: 0.0,
            wall_hit: 0,
            wall_kick_timer: 0,
            wall_normal_yaw: 0,
            anim_slot: "idle".into(),
            anim_frame: 0,
        }
    }

    #[test]
    fn csv_round_trip_is_exact() {
        let t = Trace {
            frames: vec![frame()],
            sidecar: TraceSidecar {
                scenario_id: "t".into(),
                produced_by: "test".into(),
                produced_at: "2026-10-07".into(),
                faithful_params: true,
                notes: "".into(),
            },
        };
        let csv = t.to_csv();
        let back = Trace::from_csv(&csv).unwrap();
        assert_eq!(back, t.frames);
    }
}
