//! Comparison engine: free-running and teacher-forced modes, tolerance
//! profiles, and divergence reports (Markdown + JSON).

use crate::trace::TraceFrame;
use serde::{Deserialize, Serialize};

/// How a scenario is compared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompareMode {
    /// Run the core from the start state; report the first divergence.
    FreeRunning,
    /// Reset the core to the golden state every frame; isolates each bug to
    /// the single tick that causes it.
    TeacherForced,
}

/// What "matching" means for a scenario.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToleranceProfile {
    /// Absolute tolerance for position/velocity/speed.
    pub abs_tol: f32,
    /// Relative tolerance for position/velocity/speed.
    pub rel_tol: f32,
}

impl Default for ToleranceProfile {
    /// Plan baseline: 0.001 absolute, 1e-5 relative.
    fn default() -> Self {
        Self {
            abs_tol: 0.001,
            rel_tol: 1e-5,
        }
    }
}

/// The first field that diverged.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Divergence {
    pub frame: u32,
    pub field: String,
    pub expected: String,
    pub actual: String,
    pub delta: String,
}

/// Result of comparing one scenario.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompareResult {
    pub scenario_id: String,
    pub mode: String,
    pub passed: bool,
    pub frames_compared: u32,
    pub first_divergence: Option<Divergence>,
}

impl CompareResult {
    /// Compare two frame sequences field by field (used by the CLI and by
    /// the harness; the core-vs-golden runner lives in phase 1's tools).
    pub fn diff_frames(
        scenario_id: &str,
        mode: CompareMode,
        expected: &[TraceFrame],
        actual: &[TraceFrame],
        tol: &ToleranceProfile,
    ) -> Self {
        let mode_name = match mode {
            CompareMode::FreeRunning => "free",
            CompareMode::TeacherForced => "teacher-forced",
        };
        let n = expected.len().min(actual.len()) as u32;
        for i in 0..expected.len().min(actual.len()) {
            let e = &expected[i];
            let a = &actual[i];
            if let Some(d) = first_field_divergence(e, a, tol) {
                return CompareResult {
                    scenario_id: scenario_id.into(),
                    mode: mode_name.into(),
                    passed: false,
                    frames_compared: i as u32,
                    first_divergence: Some(d),
                };
            }
        }
        CompareResult {
            scenario_id: scenario_id.into(),
            mode: mode_name.into(),
            passed: expected.len() == actual.len(),
            frames_compared: n,
            first_divergence: None,
        }
    }
}

fn close_enough(expected: f32, actual: f32, tol: &ToleranceProfile) -> bool {
    let delta = (expected - actual).abs();
    delta <= tol.abs_tol || delta <= tol.rel_tol * expected.abs().max(actual.abs())
}

/// Exact fields: action ids, timers, angles, integer state. One mismatch fails.
fn first_field_divergence(
    e: &TraceFrame,
    a: &TraceFrame,
    tol: &ToleranceProfile,
) -> Option<Divergence> {
    macro_rules! exact {
        ($field:ident) => {
            if e.$field != a.$field {
                return Some(Divergence {
                    frame: e.frame,
                    field: stringify!($field).into(),
                    expected: format!("{:?}", e.$field),
                    actual: format!("{:?}", a.$field),
                    delta: "-".into(),
                });
            }
        };
    }
    macro_rules! numeric {
        ($field:ident) => {
            if !close_enough(e.$field, a.$field, tol) {
                return Some(Divergence {
                    frame: e.frame,
                    field: stringify!($field).into(),
                    expected: format!("{:.6}", e.$field),
                    actual: format!("{:.6}", a.$field),
                    delta: format!("{:+.6}", a.$field - e.$field),
                });
            }
        };
    }
    exact!(action);
    exact!(prev_action);
    exact!(action_state);
    exact!(action_timer);
    exact!(action_arg);
    exact!(face_yaw);
    exact!(face_pitch);
    exact!(face_roll);
    exact!(wall_hit);
    numeric!(pos_x);
    numeric!(pos_y);
    numeric!(pos_z);
    numeric!(vel_x);
    numeric!(vel_y);
    numeric!(vel_z);
    numeric!(fwd_speed);
    numeric!(floor_y);
    numeric!(ceil_y);
    None
}

/// Render a Markdown divergence report (plan appendix C shape).
pub fn render_markdown(result: &CompareResult) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Scenario: {}    Mode: {}    Result: {}\n",
        result.scenario_id,
        result.mode,
        if result.passed { "PASS" } else { "FAIL" }
    ));
    if let Some(d) = &result.first_divergence {
        out.push_str(&format!(
            "First divergence: frame {}, field {}\n  expected {}   actual {}   delta {}\n",
            d.frame, d.field, d.expected, d.actual, d.delta
        ));
    } else {
        out.push_str(&format!(
            "All {} frames compared within tolerance.\n",
            result.frames_compared
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::TraceFrame;

    fn frame(pos_x: f32, action: u32) -> TraceFrame {
        TraceFrame {
            frame: 0,
            stick_x: 0,
            stick_y: 80,
            buttons: 0,
            cam_yaw: 0,
            pos_x,
            pos_y: 0.0,
            pos_z: 0.0,
            vel_x: 0.0,
            vel_y: 0.0,
            vel_z: 0.0,
            slide_vel_x: 0.0,
            slide_vel_z: 0.0,
            fwd_speed: 0.0,
            face_yaw: 0,
            face_pitch: 0,
            face_roll: 0,
            action,
            prev_action: action,
            land_from: 0,
            action_state: 0,
            action_timer: 0,
            action_arg: 0,
            floor_y: 0.0,
            ceil_y: 0.0,
            wall_hit: 0,
            wall_kick_timer: 0,
            wall_normal_yaw: 0,
            health: 0x880,
            squish_timer: 0,
            quicksand_depth: 0.0,
            peak_height: 0.0,
            slide_over_cap: 0,
            swim_strength: 160,
            anim_slot: "idle".into(),
            anim_frame: 0,
        }
    }

    #[test]
    fn identical_traces_pass() {
        let t = vec![frame(1.0, 7), frame(2.0, 7)];
        let r = CompareResult::diff_frames(
            "s",
            CompareMode::FreeRunning,
            &t,
            &t,
            &ToleranceProfile::default(),
        );
        assert!(r.passed);
        assert!(r.first_divergence.is_none());
    }

    #[test]
    fn exact_field_mismatch_fails() {
        let e = vec![frame(1.0, 7)];
        let a = vec![frame(1.0, 8)];
        let r = CompareResult::diff_frames(
            "s",
            CompareMode::FreeRunning,
            &e,
            &a,
            &ToleranceProfile::default(),
        );
        assert!(!r.passed);
        assert_eq!(r.first_divergence.unwrap().field, "action");
    }

    #[test]
    fn numeric_within_tolerance_passes() {
        let e = vec![frame(1.0, 7)];
        let a = vec![frame(1.0005, 7)];
        let r = CompareResult::diff_frames(
            "s",
            CompareMode::FreeRunning,
            &e,
            &a,
            &ToleranceProfile::default(),
        );
        assert!(r.passed);
    }
}
