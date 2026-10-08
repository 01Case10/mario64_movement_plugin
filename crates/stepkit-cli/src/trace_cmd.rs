//! Implementations of the `stepkit trace` subcommands.

use std::path::Path;
use stepkit_core::params::MovementParams;
use stepkit_trace::compare::{render_markdown, CompareMode, CompareResult, ToleranceProfile};
use stepkit_trace::runner::{run_scenario, state_from_frame};
use stepkit_trace::scenario::Scenario;
use stepkit_trace::trace::{Trace, TraceFrame};

fn load_scenario(path: &str) -> Result<Scenario, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    ron::de::from_str(&text).map_err(|e| format!("parse {path}: {e}"))
}

fn load_golden(path: &str) -> Result<Vec<TraceFrame>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    Trace::from_csv(&text).map_err(|e| format!("parse {path}: {e}"))
}

fn parse_mode(s: &str) -> Result<CompareMode, String> {
    match s {
        "free" => Ok(CompareMode::FreeRunning),
        "teacher-forced" => Ok(CompareMode::TeacherForced),
        _ => Err(format!("unknown mode {s:?}; use free or teacher-forced")),
    }
}

/// `stepkit trace diff`: run a scenario through the core and compare with a
/// golden trace. In teacher-forced mode the core is reset to the golden state
/// every frame so each bug is isolated to the tick that causes it.
pub fn diff(
    scenario_path: Option<&str>,
    golden_path: Option<&str>,
    mode: &str,
    report_path: Option<&str>,
) -> Result<bool, String> {
    let scenario_path = scenario_path.ok_or("--scenario is required")?;
    let golden_path = golden_path.ok_or("--golden is required")?;
    let mode = parse_mode(mode)?;
    let scenario = load_scenario(scenario_path)?;
    let golden = load_golden(golden_path)?;
    let params = MovementParams::default();
    if !params.faithful {
        return Err("refusing to diff with non-faithful params".into());
    }

    let (mut result, assert_frames) = match mode {
        CompareMode::FreeRunning => {
            let actual = run_scenario(&scenario, &params, "stepkit trace diff");
            let r = CompareResult::diff_frames(
                &scenario.id,
                mode,
                &golden,
                &actual.frames,
                &ToleranceProfile::default(),
            );
            (r, actual.frames)
        }
        CompareMode::TeacherForced => {
            let r = teacher_forced_diff(&scenario, &golden);
            // Assertions need real multi-frame behavior (teacher-forced
            // resets state every tick), so evaluate them against a free run.
            let actual = run_scenario(&scenario, &params, "stepkit trace diff");
            (r, actual.frames)
        }
    };

    // Semantic assertions from the scenario's `asserts` list, checked
    // against the free-running trace. Failures fail the run so the
    // release gate catches them.
    let registry = stepkit_core::actions::ActionRegistry::sm64_style();
    let assertion_failures =
        stepkit_trace::scenario::check_assertions(&scenario, &assert_frames, &registry);
    for f in &assertion_failures {
        println!("{f}");
    }
    if !assertion_failures.is_empty() {
        result.passed = false;
    }

    println!("{}", render_markdown(&result));
    if let Some(path) = report_path {
        let json = serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| format!("write {path}: {e}"))?;
    }
    Ok(result.passed)
}

fn teacher_forced_diff(scenario: &Scenario, golden: &[TraceFrame]) -> CompareResult {
    use stepkit_core::actions::ActionRegistry;
    use stepkit_core::angles::Angle;
    use stepkit_core::input::RawInput;
    use stepkit_core::step::{tick, Timeline};
    use stepkit_trace::runner::bake_scenario_world;

    let world = bake_scenario_world(scenario);
    let registry = ActionRegistry::sm64_style();
    let cam_yaw = match scenario.camera {
        stepkit_trace::scenario::Camera::Fixed { yaw_deg } => Angle::from_degrees(yaw_deg),
    };
    let params = MovementParams::default();
    let mut actual_frames: Vec<TraceFrame> = Vec::with_capacity(golden.len());
    // Walk the golden frames; each step starts from the golden state.
    // Edge detection needs the button state *before* the applied input:
    // for frame i that is golden[i-1]'s input (0 when i == 0).
    for (i, w) in golden.windows(2).enumerate() {
        let (prev, next) = (&w[0], &w[1]);
        let prev_buttons = if i == 0 { 0 } else { golden[i - 1].buttons };
        let state = state_from_frame(prev);
        let input = RawInput {
            stick_x: prev.stick_x,
            stick_y: prev.stick_y,
            buttons: prev.buttons,
            cam_yaw,
        };
        let (out, _, _) = tick(
            state,
            input,
            &world,
            &params,
            Timeline::default(),
            &registry,
            prev_buttons,
        );
        let mut f = next.clone();
        f.pos_x = out.pos.x;
        f.pos_y = out.pos.y;
        f.pos_z = out.pos.z;
        f.vel_x = out.vel.x;
        f.vel_y = out.vel.y;
        f.vel_z = out.vel.z;
        f.fwd_speed = out.forward_speed;
        f.face_yaw = out.face_yaw.0;
        f.face_pitch = out.face_pitch.0;
        f.face_roll = out.face_roll.0;
        f.action = out.action.0;
        f.prev_action = out.prev_action.0;
        f.action_state = out.action_state;
        f.action_timer = out.action_timer;
        f.action_arg = out.action_arg;
        f.floor_y = out.floor_y.unwrap_or(0.0);
        f.ceil_y = out.ceil_y.unwrap_or(0.0);
        f.wall_hit = out.wall_hit as u8;
        actual_frames.push(f);
    }
    // Compare golden[1..] against the one-tick predictions.
    CompareResult::diff_frames(
        &scenario.id,
        CompareMode::TeacherForced,
        &golden[1..],
        &actual_frames,
        &ToleranceProfile::default(),
    )
}

/// `stepkit trace summarize`: per-action duration tables over golden traces.
pub fn summarize(golden_dir: &str, out: &str) -> Result<(), String> {
    let mut traces: Vec<(String, Vec<TraceFrame>)> = Vec::new();
    let dir = std::fs::read_dir(golden_dir).map_err(|e| format!("read {golden_dir}: {e}"))?;
    let mut paths: Vec<_> = dir
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "csv"))
        .collect();
    paths.sort();
    for path in paths {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        let frames = load_golden(path.to_str().unwrap())?;
        traces.push((name, frames));
    }
    let refs: Vec<(&str, &[TraceFrame])> = traces
        .iter()
        .map(|(n, f)| (n.as_str(), f.as_slice()))
        .collect();
    let summary = stepkit_trace::summarize::TraceSummary::from_traces(&refs);
    let mut csv = String::from("scenario_id,action_hex,frames,start_frame\n");
    for d in &summary.durations {
        csv.push_str(&format!(
            "{},{:#010x},{},{}\n",
            d.scenario_id, d.action, d.frames, d.start_frame
        ));
    }
    csv.push_str("\n# longest runs per action (candidate manifest frame counts)\n");
    for (action, frames) in summary.longest_runs() {
        csv.push_str(&format!("# action {action:#010x}: {frames} frames\n"));
    }
    std::fs::write(out, csv).map_err(|e| format!("write {out}: {e}"))?;
    println!(
        "summarized {} traces -> {out} ({} action runs)",
        traces.len(),
        summary.durations.len()
    );
    Ok(())
}

/// `stepkit trace pack`: convert a directory of CSV traces to `.pack` files.
pub fn pack(input_dir: &str, out_path: &str) -> Result<(), String> {
    let dir = std::fs::read_dir(input_dir).map_err(|e| format!("read {input_dir}: {e}"))?;
    let mut all: Vec<u8> = Vec::new();
    let mut count = 0u32;
    let mut paths: Vec<_> = dir
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "csv"))
        .collect();
    paths.sort();
    for path in paths {
        let frames = load_golden(path.to_str().unwrap())?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .as_bytes();
        all.push(name.len().min(255) as u8);
        all.extend_from_slice(&name[..name.len().min(255)]);
        let packed = stepkit_trace::pack::pack_frames(&frames);
        all.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        all.extend_from_slice(&packed);
        count += 1;
    }
    let mut out = b"STPKPACK\x01".to_vec();
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&all);
    std::fs::write(out_path, &out).map_err(|e| format!("write {out_path}: {e}"))?;
    println!("packed {count} traces -> {out_path} ({} bytes)", out.len());
    Ok(())
}

/// `stepkit trace import-m64`: convert a TAS input movie to a scenario.
pub fn import_m64(input: &str, frames: Option<&str>, out: &str) -> Result<(), String> {
    let data = std::fs::read(input).map_err(|e| format!("read {input}: {e}"))?;
    let (num_frames, samples) = crate::m64::parse_m64(&data)?;
    let range = frames
        .map(|s| {
            let (a, b) = s
                .split_once("..")
                .ok_or(format!("bad range {s:?}, use a..b"))?;
            Ok::<(u32, u32), String>((
                a.parse().map_err(|_| format!("bad {a}"))?,
                b.parse().map_err(|_| format!("bad {b}"))?,
            ))
        })
        .transpose()?;
    let id = Path::new(input)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("replay");
    let scenario = crate::m64::samples_to_scenario(id, &samples, range);
    let text = ron::ser::to_string_pretty(&scenario, ron::ser::PrettyConfig::default())
        .map_err(|e| e.to_string())?;
    std::fs::write(out, text).map_err(|e| format!("write {out}: {e}"))?;
    println!(
        "imported {input} ({num_frames} video frames) -> {out} ({} scenario frames)",
        scenario.frame_count()
    );
    Ok(())
}

/// `stepkit trace run`: execute a scenario and write the trace CSV plus a
/// JSON sidecar next to it.
pub fn run(scenario_path: &str, out: &str) -> Result<(), String> {
    let scenario = load_scenario(scenario_path)?;
    let params = MovementParams::default();
    let trace = run_scenario(&scenario, &params, "stepkit trace run");
    std::fs::write(out, trace.to_csv()).map_err(|e| format!("write {out}: {e}"))?;
    let sidecar_path = format!("{out}.sidecar.json");
    let sidecar = serde_json::to_string_pretty(&trace.sidecar).map_err(|e| e.to_string())?;
    std::fs::write(&sidecar_path, sidecar).map_err(|e| format!("write {sidecar_path}: {e}"))?;
    println!(
        "ran {} ({} frames) -> {out}",
        scenario.id,
        trace.frames.len()
    );
    Ok(())
}
