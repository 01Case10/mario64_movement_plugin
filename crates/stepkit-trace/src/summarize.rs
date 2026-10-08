//! `stepkit trace summarize`: per-action duration tables and curve fits.
//!
//! Feeds the spec author and the timing manifest: how many frames each action
//! lasts under each condition (evidence for manifest frame counts), and
//! acceleration/cap curve fits from input sweeps.

use crate::trace::TraceFrame;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How long an action ran, per scenario.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActionDuration {
    pub scenario_id: String,
    pub action: u32,
    pub frames: u32,
    pub start_frame: u32,
}

/// Summary over a set of traces.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TraceSummary {
    pub durations: Vec<ActionDuration>,
}

impl TraceSummary {
    /// Build from named frame sequences.
    pub fn from_traces(traces: &[(&str, &[TraceFrame])]) -> Self {
        let mut durations = Vec::new();
        for (id, frames) in traces {
            let mut iter = frames.iter().peekable();
            while let Some(f) = iter.next() {
                let action = f.action;
                let start = f.frame;
                let mut count = 1u32;
                while iter.peek().is_some_and(|n| n.action == action) {
                    iter.next();
                    count += 1;
                }
                durations.push(ActionDuration {
                    scenario_id: id.to_string(),
                    action,
                    frames: count,
                    start_frame: start,
                });
            }
        }
        Self { durations }
    }

    /// Longest observed run of each action (candidate manifest frame counts).
    pub fn longest_runs(&self) -> BTreeMap<u32, u32> {
        let mut map = BTreeMap::new();
        for d in &self.durations {
            let e = map.entry(d.action).or_insert(0);
            *e = (*e).max(d.frames);
        }
        map
    }
}

/// Least-squares fit of `speed` over frames to `v = cap * (1 - exp(-k*t))`,
/// returning `(cap, k)`. Used on acceleration sweeps to extract caps.
/// Coarse-to-fine grid search on the sum of squared errors: robust for the
/// small, clean sweep data this tool consumes.
pub fn fit_exponential_approach(samples: &[(f32, f32)]) -> Option<(f32, f32)> {
    if samples.len() < 3 {
        return None;
    }
    let vmax = samples.iter().map(|(_, v)| *v).fold(0.0f32, f32::max);
    if vmax <= 0.0 {
        return None;
    }
    let sse = |cap: f32, k: f32| -> f32 {
        samples
            .iter()
            .map(|&(t, v)| {
                let pred = cap * (1.0 - (-k * t).exp());
                (pred - v) * (pred - v)
            })
            .sum()
    };
    // Coarse grid.
    let mut best = (vmax, 0.1f32);
    let mut best_err = f32::INFINITY;
    for ci in 0..25 {
        let cap = vmax * (0.98 + ci as f32 * 0.02);
        for ki in 0..25 {
            let k = 0.02 * (1.25f32).powi(ki);
            let err = sse(cap, k);
            if err < best_err {
                best_err = err;
                best = (cap, k);
            }
        }
    }
    // Refine around the best point.
    for _ in 0..3 {
        let (cap0, k0) = best;
        for ci in -4..=4 {
            for ki in -4..=4 {
                let cap = cap0 * (1.0 + ci as f32 * 0.005);
                let k = k0 * (1.0 + ki as f32 * 0.05);
                let err = sse(cap, k);
                if err < best_err {
                    best_err = err;
                    best = (cap, k);
                }
            }
        }
    }
    Some(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(f: u32, action: u32) -> TraceFrame {
        TraceFrame {
            frame: f,
            stick_x: 0,
            stick_y: 0,
            buttons: 0,
            cam_yaw: 0,
            pos_x: 0.0,
            pos_y: 0.0,
            pos_z: 0.0,
            vel_x: 0.0,
            vel_y: 0.0,
            vel_z: 0.0,
            fwd_speed: 0.0,
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
            anim_slot: String::new(),
            anim_frame: 0,
        }
    }

    #[test]
    fn durations_detect_runs() {
        let frames: Vec<TraceFrame> = (0..10)
            .map(|i| frame(i, if i < 4 { 1 } else { 2 }))
            .collect();
        let s = TraceSummary::from_traces(&[("s", &frames)]);
        assert_eq!(s.durations.len(), 2);
        assert_eq!(s.durations[0].frames, 4);
        assert_eq!(s.durations[1].frames, 6);
        assert_eq!(s.longest_runs()[&1], 4);
    }

    #[test]
    fn exponential_fit_recovers_cap() {
        // v = 48 * (1 - exp(-0.2 t))
        let samples: Vec<(f32, f32)> = (1..=40)
            .map(|t| (t as f32, 48.0 * (1.0 - (-0.2 * t as f32).exp())))
            .collect();
        let (cap, k) = fit_exponential_approach(&samples).unwrap();
        assert!((cap - 48.0).abs() < 2.0, "cap {cap}");
        assert!((k - 0.2).abs() < 0.05, "k {k}");
    }
}
