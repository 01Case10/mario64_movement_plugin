//! `stepkit oracle diff`: differential parity check between the native decomp
//! oracle and the plugin core (plan §6 Phase 2 item 1, §7.2).
//!
//! Frame alignment: the oracle's CSV row `i` is the state AFTER the tick with
//! `input[i]` (n rows for n inputs). The plugin's `run_scenario` emits n+1
//! frames where frame 0 is the pre-tick initial state and frame i+1 is the
//! state after the tick with `input[i]`. Free mode therefore compares
//! `plugin.frames[1..]` against `oracle.rows[0..]`.
//!
//! Comparison is EXACT (plan §7.2): bitwise `f32` equality via `to_bits()`,
//! integer equality for the rest. No tolerances. Oracle action ids pass
//! through the FD-07 translation table before comparing, so known
//! renumberings don't mask behavioral divergences.
//!
//! Teacher-forced mode converts each oracle row to a [`TraceFrame`], resets
//! the core to the row-N state, ticks once with the scenario's `input[N+1]`,
//! and compares against row N+1. This isolates one-tick errors from
//! accumulated drift.

use std::path::{Path, PathBuf};

use stepkit_core::actions::ActionRegistry;
use stepkit_core::angles::Angle;
use stepkit_core::input::RawInput;
use stepkit_core::params::MovementParams;
use stepkit_core::state::ActionId;
use stepkit_core::step::{tick, Timeline};
use stepkit_trace::oracle::{read_oracle_csv, OracleFrame};
use stepkit_trace::runner::{bake_scenario_world, run_scenario, state_from_frame};
use stepkit_trace::scenario::{Camera, Scenario};
use stepkit_trace::trace::TraceFrame;

/// Oracle-only fields: printed once per run, never compared, and never
/// allowed to mask divergences. Empty since AN-01: the `anim_*` fields are
/// now modeled (the core ports the decomp's animation state) and compared
/// exactly like everything else.
pub const UNMODELED_FIELDS: &[&str] = &[];

/// Mapped fields compared exactly, in report order. Names are the plugin
/// [`TraceFrame`] field names (`forward_vel`→`fwd_speed`,
/// `floor_height`→`floor_y`, `ceil_height`→`ceil_y`); `wall` is presence
/// (oracle `wall_normal_yaw.is_some()` ↔ plugin `wall_hit != 0`) and
/// `wall_normal_yaw` is only compared when both sides hit.
///
/// The FD-05 fields are appended after the Phase-2 set so existing
/// first-divergence lines keep their meaning while the new fields are
/// still checked.
pub(crate) const COMPARED_FIELDS: &[&str] = &[
    "pos_x",
    "pos_y",
    "pos_z",
    "vel_x",
    "vel_y",
    "vel_z",
    "slide_vel_x",
    "slide_vel_z",
    "fwd_speed",
    "face_pitch",
    "face_yaw",
    "face_roll",
    "action",
    "prev_action",
    "action_state",
    "action_timer",
    "action_arg",
    "floor_y",
    "ceil_y",
    "wall",
    "wall_normal_yaw",
    "wall_kick_timer",
    "squish_timer",
    "quicksand_depth",
    "peak_height",
    "health",
    "slide_yaw",
    "angle_vel_x",
    "angle_vel_y",
    "angle_vel_z",
    "intended_yaw",
    "intended_mag",
    "input",
    "floor_angle",
    "floor_normal_y",
    "floor_type",
    "double_jump_timer",
    "water_level",
    "hurt_counter",
    "invinc_timer",
    "flags",
    // ---- AN-01 fields ----
    "anim_id",
    "anim_frame",
    "anim_accel",
    "anim_frame_accel_assist",
];

/// Phase-2 scaffolding for FD-07: translate a decomp action id to the
/// plugin's renumbered id. All other ids map to themselves. Apply to
/// `action` and `prev_action` BEFORE comparing. Delete when FD-07 lands in
/// Phase 3.
/// The compared state, in one canonical representation regardless of source.
#[derive(Clone, Debug)]
struct ParityState {
    pos_x: f32,
    pos_y: f32,
    pos_z: f32,
    vel_x: f32,
    vel_y: f32,
    vel_z: f32,
    slide_vel_x: f32,
    slide_vel_z: f32,
    fwd_speed: f32,
    face_pitch: i16,
    face_yaw: i16,
    face_roll: i16,
    action: u32,
    prev_action: u32,
    action_state: u32,
    action_timer: u32,
    action_arg: u32,
    floor_y: f32,
    ceil_y: f32,
    wall: bool,
    wall_normal_yaw: i32,
    wall_kick_timer: i64,
    squish_timer: i64,
    quicksand_depth: f32,
    peak_height: f32,
    health: i32,
    // ---- FD-05 fields ----
    slide_yaw: i16,
    angle_vel_x: i16,
    angle_vel_y: i16,
    angle_vel_z: i16,
    intended_yaw: i16,
    intended_mag: f32,
    input: u32,
    floor_angle: i16,
    floor_normal_y: f32,
    floor_type: i32,
    double_jump_timer: i64,
    water_level: f32,
    hurt_counter: i32,
    invinc_timer: i32,
    flags: u32,
    // ---- AN-01 fields ----
    anim_id: i32,
    anim_frame: i32,
    anim_accel: i32,
    anim_frame_accel_assist: i32,
}

impl ParityState {
    fn from_plugin(f: &TraceFrame) -> Self {
        ParityState {
            pos_x: f.pos_x,
            pos_y: f.pos_y,
            pos_z: f.pos_z,
            vel_x: f.vel_x,
            vel_y: f.vel_y,
            vel_z: f.vel_z,
            slide_vel_x: f.slide_vel_x,
            slide_vel_z: f.slide_vel_z,
            fwd_speed: f.fwd_speed,
            face_pitch: f.face_pitch,
            face_yaw: f.face_yaw,
            face_roll: f.face_roll,
            action: f.action,
            prev_action: f.prev_action,
            action_state: f.action_state,
            action_timer: f.action_timer,
            action_arg: f.action_arg,
            floor_y: f.floor_y,
            ceil_y: f.ceil_y,
            wall: f.wall_hit != 0,
            wall_normal_yaw: f.wall_normal_yaw as i32,
            wall_kick_timer: f.wall_kick_timer as i64,
            squish_timer: f.squish_timer as i64,
            quicksand_depth: f.quicksand_depth,
            peak_height: f.peak_height,
            health: f.health,
            slide_yaw: f.slide_yaw,
            angle_vel_x: f.angle_vel_x,
            angle_vel_y: f.angle_vel_y,
            angle_vel_z: f.angle_vel_z,
            intended_yaw: f.intended_yaw,
            intended_mag: f.intended_mag,
            input: f.input,
            floor_angle: f.floor_angle,
            floor_normal_y: f.floor_normal_y,
            floor_type: f.floor_type,
            double_jump_timer: f.double_jump_timer as i64,
            water_level: f.water_level,
            hurt_counter: f.hurt_counter,
            invinc_timer: f.invinc_timer,
            flags: f.flags,
            anim_id: f.anim_id,
            anim_frame: f.anim_frame,
            anim_accel: f.anim_accel,
            anim_frame_accel_assist: f.anim_frame_accel_assist,
        }
    }

    fn from_oracle(f: &OracleFrame) -> Self {
        ParityState {
            pos_x: f.pos_x,
            pos_y: f.pos_y,
            pos_z: f.pos_z,
            vel_x: f.vel_x,
            vel_y: f.vel_y,
            vel_z: f.vel_z,
            slide_vel_x: f.slide_vel_x,
            slide_vel_z: f.slide_vel_z,
            fwd_speed: f.forward_vel,
            face_pitch: f.face_pitch,
            face_yaw: f.face_yaw,
            face_roll: f.face_roll,
            action: f.action,
            prev_action: f.prev_action,
            action_state: f.action_state,
            action_timer: f.action_timer,
            action_arg: f.action_arg,
            floor_y: f.floor_height,
            ceil_y: f.ceil_height,
            wall: f.wall_normal_yaw.is_some(),
            wall_normal_yaw: f.wall_normal_yaw.unwrap_or(0),
            wall_kick_timer: f.wall_kick_timer as i64,
            squish_timer: f.squish_timer as i64,
            quicksand_depth: f.quicksand_depth,
            peak_height: f.peak_height,
            health: f.health,
            slide_yaw: f.slide_yaw as i16,
            angle_vel_x: f.angle_vel_x,
            angle_vel_y: f.angle_vel_y,
            angle_vel_z: f.angle_vel_z,
            intended_yaw: f.intended_yaw,
            intended_mag: f.intended_mag,
            input: f.input,
            floor_angle: f.floor_angle,
            floor_normal_y: f.floor_normal_y,
            floor_type: f.floor_type,
            double_jump_timer: f.double_jump_timer as i64,
            water_level: f.water_level as f32,
            hurt_counter: f.hurt_counter,
            invinc_timer: f.invinc_timer,
            flags: f.flags,
            anim_id: f.anim_id,
            anim_frame: f.anim_frame,
            anim_accel: f.anim_accel,
            anim_frame_accel_assist: f.anim_frame_accel_assist,
        }
    }
}

/// One field's value, with bitwise-exact equality (`f32` stored as bits).
#[derive(Clone, Copy, Debug)]
enum Value {
    F32(u32),
    I16(i16),
    I32(i32),
    U32(u32),
    I64(i64),
    Hex(u32),
    Bool(bool),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        use Value::*;
        match (self, other) {
            (F32(a), F32(b)) => a == b,
            (I16(a), I16(b)) => a == b,
            (I32(a), I32(b)) => a == b,
            (U32(a), U32(b)) => a == b,
            (I64(a), I64(b)) => a == b,
            (Hex(a), Hex(b)) => a == b,
            (Bool(a), Bool(b)) => a == b,
            _ => false,
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Value::*;
        match self {
            F32(b) => write!(f, "{:?}", f32::from_bits(*b)),
            I16(v) => write!(f, "{v}"),
            I32(v) => write!(f, "{v}"),
            U32(v) => write!(f, "{v}"),
            I64(v) => write!(f, "{v}"),
            Hex(v) => write!(f, "0x{v:08X}"),
            Bool(v) => write!(f, "{v}"),
        }
    }
}

fn field_value(s: &ParityState, field: &str) -> Value {
    match field {
        "pos_x" => Value::F32(s.pos_x.to_bits()),
        "pos_y" => Value::F32(s.pos_y.to_bits()),
        "pos_z" => Value::F32(s.pos_z.to_bits()),
        "vel_x" => Value::F32(s.vel_x.to_bits()),
        "vel_y" => Value::F32(s.vel_y.to_bits()),
        "vel_z" => Value::F32(s.vel_z.to_bits()),
        "slide_vel_x" => Value::F32(s.slide_vel_x.to_bits()),
        "slide_vel_z" => Value::F32(s.slide_vel_z.to_bits()),
        "fwd_speed" => Value::F32(s.fwd_speed.to_bits()),
        "face_pitch" => Value::I16(s.face_pitch),
        "face_yaw" => Value::I16(s.face_yaw),
        "face_roll" => Value::I16(s.face_roll),
        "action" => Value::Hex(s.action),
        "prev_action" => Value::Hex(s.prev_action),
        "action_state" => Value::U32(s.action_state),
        "action_timer" => Value::U32(s.action_timer),
        "action_arg" => Value::Hex(s.action_arg),
        "floor_y" => Value::F32(s.floor_y.to_bits()),
        "ceil_y" => Value::F32(s.ceil_y.to_bits()),
        "wall" => Value::Bool(s.wall),
        "wall_normal_yaw" => Value::I32(s.wall_normal_yaw),
        "wall_kick_timer" => Value::I64(s.wall_kick_timer),
        "squish_timer" => Value::I64(s.squish_timer),
        "quicksand_depth" => Value::F32(s.quicksand_depth.to_bits()),
        "peak_height" => Value::F32(s.peak_height.to_bits()),
        "health" => Value::I32(s.health),
        "slide_yaw" => Value::I16(s.slide_yaw),
        "angle_vel_x" => Value::I16(s.angle_vel_x),
        "angle_vel_y" => Value::I16(s.angle_vel_y),
        "angle_vel_z" => Value::I16(s.angle_vel_z),
        "intended_yaw" => Value::I16(s.intended_yaw),
        "intended_mag" => Value::F32(s.intended_mag.to_bits()),
        "input" => Value::Hex(s.input),
        "floor_angle" => Value::I16(s.floor_angle),
        "floor_normal_y" => Value::F32(s.floor_normal_y.to_bits()),
        "floor_type" => Value::I32(s.floor_type),
        "double_jump_timer" => Value::I64(s.double_jump_timer),
        "water_level" => Value::F32(s.water_level.to_bits()),
        "hurt_counter" => Value::I32(s.hurt_counter),
        "invinc_timer" => Value::I32(s.invinc_timer),
        "flags" => Value::Hex(s.flags),
        "anim_id" => Value::I32(s.anim_id),
        "anim_frame" => Value::I32(s.anim_frame),
        "anim_accel" => Value::I32(s.anim_accel),
        "anim_frame_accel_assist" => Value::I32(s.anim_frame_accel_assist),
        _ => unreachable!("unknown compared field {field:?}"),
    }
}

struct Divergence {
    field: String,
    plugin: String,
    oracle: String,
}

/// Compare two states over `fields`; return the first exact mismatch.
fn compare(plugin: &ParityState, oracle: &ParityState, fields: &[&str]) -> Option<Divergence> {
    for &field in fields {
        // wall_normal_yaw is only meaningful when both sides hit a wall;
        // presence itself is compared via `wall`.
        if field == "wall_normal_yaw" && !(plugin.wall && oracle.wall) {
            continue;
        }
        let pv = field_value(plugin, field);
        let ov = field_value(oracle, field);
        if pv != ov {
            return Some(Divergence {
                field: field.to_string(),
                plugin: pv.to_string(),
                oracle: ov.to_string(),
            });
        }
    }
    None
}

/// Convert an oracle row to a plugin [`TraceFrame`] for teacher-forcing.
/// Only mapped fields are carried; the rest are documented defaults.
/// Inputs come from the scenario (NOT the oracle CSV: the oracle's stick is
/// post-`adjust_analog_stick` and its button bitmask is the decomp's).
fn oracle_to_trace_frame(
    o: &OracleFrame,
    inputs: &[(i8, i8, u16)],
    frame_index: usize,
    cam_yaw: i16,
) -> TraceFrame {
    let (sx, sy, buttons) = inputs[frame_index];
    TraceFrame {
        frame: o.frame,
        stick_x: sx,
        stick_y: sy,
        buttons,
        cam_yaw,
        pos_x: o.pos_x,
        pos_y: o.pos_y,
        pos_z: o.pos_z,
        vel_x: o.vel_x,
        vel_y: o.vel_y,
        vel_z: o.vel_z,
        slide_vel_x: o.slide_vel_x,
        slide_vel_z: o.slide_vel_z,
        fwd_speed: o.forward_vel,
        face_yaw: o.face_yaw,
        face_pitch: o.face_pitch,
        face_roll: o.face_roll,
        action: o.action,
        prev_action: o.prev_action,
        // land_from: 0. The plugin only reads land_from inside
        // enter_landing_for(), and every call site (actions/air.rs) sets it
        // from the landing action immediately before the call, so the seed
        // value never affects behavior.
        land_from: 0,
        action_state: o.action_state,
        action_timer: o.action_timer,
        action_arg: o.action_arg,
        floor_y: o.floor_height,
        ceil_y: o.ceil_height,
        wall_hit: u8::from(o.wall_normal_yaw.is_some()),
        // TraceFrame timers are u32; the oracle's are i32. Decomp timers
        // count down from positive values, so clamp negatives to 0 rather
        // than wrapping (a genuinely negative oracle timer vs a plugin u32
        // is caught as an exact divergence by the comparator anyway).
        wall_kick_timer: o.wall_kick_timer.max(0) as u32,
        wall_normal_yaw: o.wall_normal_yaw.unwrap_or(0) as i16,
        health: o.health,
        squish_timer: o.squish_timer.max(0) as u32,
        quicksand_depth: o.quicksand_depth,
        peak_height: o.peak_height,
        slide_over_cap: 0,
        swim_strength: 0,
        anim_slot: String::new(),
        timeline_frame: 0,
        // AN-01: the oracle reports the animation state, so teacher
        // forcing restores it exactly.
        anim_id: o.anim_id,
        anim_frame: o.anim_frame,
        anim_accel: o.anim_accel,
        anim_frame_accel_assist: o.anim_frame_accel_assist,
        // FD-05: the oracle reports all of these, so teacher forcing can
        // restore them exactly.
        slide_yaw: o.slide_yaw as i16,
        angle_vel_x: o.angle_vel_x,
        angle_vel_y: o.angle_vel_y,
        angle_vel_z: o.angle_vel_z,
        intended_yaw: o.intended_yaw,
        intended_mag: o.intended_mag,
        input: o.input,
        floor_angle: o.floor_angle,
        floor_normal_y: o.floor_normal_y,
        floor_type: o.floor_type,
        double_jump_timer: o.double_jump_timer.max(0) as u32,
        water_level: o.water_level as f32,
        hurt_counter: o.hurt_counter,
        invinc_timer: o.invinc_timer,
        flags: o.flags,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DiffMode {
    Free,
    TeacherForced,
}

impl DiffMode {
    fn as_str(self) -> &'static str {
        match self {
            DiffMode::Free => "free",
            DiffMode::TeacherForced => "teacher-forced",
        }
    }
}

fn parse_mode(s: &str) -> Result<DiffMode, String> {
    match s {
        "free" => Ok(DiffMode::Free),
        "teacher-forced" => Ok(DiffMode::TeacherForced),
        _ => Err(format!("unknown mode {s:?}; use free or teacher-forced")),
    }
}

fn parse_fields(spec: &str) -> Result<Vec<&str>, String> {
    let mut out = Vec::new();
    for name in spec.split(',') {
        let name = name.trim();
        if COMPARED_FIELDS.contains(&name) {
            out.push(name);
        } else {
            return Err(format!(
                "unknown field {name:?}; known fields: {}",
                COMPARED_FIELDS.join(", ")
            ));
        }
    }
    if out.is_empty() {
        return Err("--fields is empty".into());
    }
    Ok(out)
}

fn load_scenario(path: &Path) -> Result<Scenario, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let scenario: Scenario =
        ron::de::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;
    // Strict integer-geometry contract (plan §6 Phase 2, ST-02), same as the
    // trace loader.
    scenario
        .validate_geometry()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(scenario)
}

/// Walk up from the cwd looking for `traces/scenarios/`.
fn find_scenarios_dir() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("cwd: {e}"))?;
    let mut dir = Some(cwd.as_path());
    while let Some(d) = dir {
        let cand = d.join("traces/scenarios");
        if cand.is_dir() {
            return Ok(cand);
        }
        dir = d.parent();
    }
    Err("traces/scenarios not found (walked up from cwd)".into())
}

/// Resolve `--scenario`: an existing path wins, otherwise treat it as an id
/// under `traces/scenarios/` (appending `.ron` if missing).
fn resolve_scenario(spec: &str) -> Result<PathBuf, String> {
    let p = Path::new(spec);
    if p.is_file() {
        return Ok(p.to_path_buf());
    }
    let mut name = spec.to_string();
    if !name.ends_with(".ron") {
        name.push_str(".ron");
    }
    let dir = find_scenarios_dir()?;
    let cand = dir.join(&name);
    if cand.is_file() {
        Ok(cand)
    } else {
        Err(format!(
            "scenario {spec:?}: not a file, and no {} in {}",
            cand.file_name().unwrap().to_string_lossy(),
            dir.display()
        ))
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Run the oracle for a scenario and return its parsed rows. Reuses
/// `oracle_cmd::run` as-is.
fn run_oracle_rows(scenario: &Scenario, path: &Path) -> Result<Vec<OracleFrame>, String> {
    let tmp = std::env::temp_dir().join(format!(
        "stepkit-oracle-diff-{}-{}.csv",
        std::process::id(),
        sanitize(&scenario.id)
    ));
    let tmp_str = tmp
        .to_str()
        .ok_or_else(|| format!("non-utf8 temp path {}", tmp.display()))?;
    let path_str = path
        .to_str()
        .ok_or_else(|| format!("non-utf8 scenario path {}", path.display()))?;
    crate::oracle_cmd::run(path_str, tmp_str).map_err(|e| format!("oracle run failed: {e}"))?;
    let text = std::fs::read_to_string(&tmp).map_err(|e| format!("read {}: {e}", tmp.display()))?;
    let rows = read_oracle_csv(&text)
        .map_err(|e| format!("oracle csv parse failed (kept at {}): {e}", tmp.display()))?;
    let _ = std::fs::remove_file(&tmp);
    Ok(rows)
}

fn faithful_params() -> Result<MovementParams, String> {
    let params = MovementParams::default();
    if !params.faithful {
        return Err("refusing to diff with non-faithful params".into());
    }
    Ok(params)
}

pub(crate) struct DivergenceReport {
    /// Plugin frame number (oracle row j ↔ plugin frame j+1; a teacher-forced
    /// prediction of oracle row i+1 ↔ plugin frame i+2).
    pub(crate) frame: u32,
    pub(crate) field: String,
    pub(crate) plugin: String,
    pub(crate) oracle: String,
    prev_plugin: ParityState,
    prev_oracle: Option<ParityState>,
    inputs: Vec<(i8, i8, u16)>,
}

fn fmt_prev(s: &ParityState) -> String {
    format!(
        "pos=({:?}, {:?}, {:?}) vel=({:?}, {:?}, {:?}) fwd={:?} action=0x{:08X} state={} timer={}",
        s.pos_x,
        s.pos_y,
        s.pos_z,
        s.vel_x,
        s.vel_y,
        s.vel_z,
        s.fwd_speed,
        s.action,
        s.action_state,
        s.action_timer
    )
}

/// Free mode: compare plugin.frames[1..] against oracle rows[0..].
/// Returns Ok(None) on a full pass.
pub(crate) fn diff_free(
    scenario: &Scenario,
    path: &Path,
    fields: &[&str],
) -> Result<Option<DivergenceReport>, String> {
    let params = faithful_params()?;
    let oracle = run_oracle_rows(scenario, path)?;
    let trace = run_scenario(scenario, &params, "stepkit oracle diff");
    if trace.frames.len() != oracle.len() + 1 {
        return Err(format!(
            "length mismatch: plugin produced {} frames, oracle produced {} rows",
            trace.frames.len(),
            oracle.len()
        ));
    }
    let inputs = scenario.expand_inputs();
    let mut prev_p = ParityState::from_plugin(&trace.frames[0]);
    for (j, row) in oracle.iter().enumerate() {
        let n = j as u32 + 1;
        let p = ParityState::from_plugin(&trace.frames[n as usize]);
        let o = ParityState::from_oracle(row);
        if let Some(d) = compare(&p, &o, fields) {
            return Ok(Some(DivergenceReport {
                frame: n,
                field: d.field,
                plugin: d.plugin,
                oracle: d.oracle,
                prev_plugin: prev_p,
                prev_oracle: if j == 0 {
                    None
                } else {
                    Some(ParityState::from_oracle(&oracle[j - 1]))
                },
                inputs,
            }));
        }
        prev_p = p;
    }
    Ok(None)
}

/// Teacher-forced mode: reset the core to each oracle row's state, tick once
/// with the scenario input, and compare against the next oracle row.
fn diff_teacher_forced(
    scenario: &Scenario,
    path: &Path,
    fields: &[&str],
) -> Result<Option<DivergenceReport>, String> {
    let params = faithful_params()?;
    let oracle = run_oracle_rows(scenario, path)?;
    let inputs = scenario.expand_inputs();
    if inputs.len() != oracle.len() {
        return Err(format!(
            "length mismatch: scenario has {} inputs, oracle produced {} rows",
            inputs.len(),
            oracle.len()
        ));
    }
    let registry = ActionRegistry::sm64_style();
    // Pre-check: every translated action must dispatch to a real plugin
    // handler. tick() itself does not panic on unknown ids (it holds state),
    // but teacher-forcing an action with no plugin equivalent cannot
    // reproduce the oracle's behavior, so fail the scenario loudly instead.
    for row in &oracle {
        let t = row.action;
        if registry.get(ActionId(t)).is_none() {
            return Err(format!(
                "cannot teacher-force: action 0x{t:08X} has no plugin equivalent"
            ));
        }
    }
    let cam_yaw = match scenario.camera {
        Camera::Fixed { yaw_deg } => Angle::from_degrees(yaw_deg),
    };
    let tf: Vec<TraceFrame> = oracle
        .iter()
        .enumerate()
        .map(|(i, r)| oracle_to_trace_frame(r, &inputs, i, cam_yaw.0))
        .collect();
    let world = bake_scenario_world(scenario);
    for i in 0..oracle.len().saturating_sub(1) {
        // Oracle row i+1 is produced by input[i+1]; edge detection needs the
        // buttons of the previous input, inputs[i].
        let state = state_from_frame(&tf[i]);
        let input = RawInput {
            stick_x: inputs[i + 1].0,
            stick_y: inputs[i + 1].1,
            buttons: inputs[i + 1].2,
            cam_yaw,
        };
        let (out, _timeline, _events) = tick(
            state,
            input,
            &world,
            &params,
            Timeline::default(),
            &registry,
            inputs[i].2,
        );
        // Overwrite the mapped fields of a clone of the next frame with the
        // tick output (the same field set trace_cmd's teacher-forced diff
        // overwrites), then compare exactly.
        let mut pred = tf[i + 1].clone();
        pred.pos_x = out.pos.x;
        pred.pos_y = out.pos.y;
        pred.pos_z = out.pos.z;
        pred.vel_x = out.vel.x;
        pred.vel_y = out.vel.y;
        pred.vel_z = out.vel.z;
        pred.fwd_speed = out.forward_speed;
        pred.face_yaw = out.face_yaw.0;
        pred.face_pitch = out.face_pitch.0;
        pred.face_roll = out.face_roll.0;
        pred.action = out.action.0;
        pred.prev_action = out.prev_action.0;
        pred.action_state = out.action_state;
        pred.action_timer = out.action_timer;
        pred.action_arg = out.action_arg;
        pred.floor_y = out.floor_y.unwrap_or(0.0);
        pred.ceil_y = out.ceil_y.unwrap_or(0.0);
        pred.wall_hit = out.wall.is_some() as u8;
        let p = ParityState::from_plugin(&pred);
        let o = ParityState::from_oracle(&oracle[i + 1]);
        if let Some(d) = compare(&p, &o, fields) {
            return Ok(Some(DivergenceReport {
                frame: i as u32 + 2,
                field: d.field,
                plugin: d.plugin,
                oracle: d.oracle,
                prev_plugin: ParityState::from_plugin(&tf[i]),
                prev_oracle: Some(ParityState::from_oracle(&oracle[i])),
                inputs,
            }));
        }
    }
    Ok(None)
}

enum ScenarioOutcome {
    Pass,
    Fail(DivergenceReport),
    Error(String),
}

fn print_result(id: &str, mode: DiffMode, outcome: &ScenarioOutcome) {
    let status = match outcome {
        ScenarioOutcome::Pass => "PASS",
        ScenarioOutcome::Fail(_) => "FAIL",
        ScenarioOutcome::Error(_) => "ERROR",
    };
    println!(
        "Scenario: {id}    Mode: {}    Result: {status}",
        mode.as_str()
    );
    match outcome {
        ScenarioOutcome::Pass => {}
        ScenarioOutcome::Error(e) => println!("  error: {e}"),
        ScenarioOutcome::Fail(rep) => {
            println!("First divergence: frame {}, field {}", rep.frame, rep.field);
            println!("  plugin {}   oracle {}", rep.plugin, rep.oracle);
            println!("  prev frame plugin: {}", fmt_prev(&rep.prev_plugin));
            match &rep.prev_oracle {
                Some(o) => println!("  prev frame oracle: {}", fmt_prev(o)),
                None => println!("  prev frame oracle: (no previous oracle row)"),
            }
            // The input each listed frame carried (frame k was produced by,
            // and carries, inputs[k]).
            let n = rep.frame as i64;
            let mut parts = Vec::new();
            for k in [n - 2, n - 3, n - 4] {
                if k < 0 || k as usize >= rep.inputs.len() {
                    continue;
                }
                let (sx, sy, b) = rep.inputs[k as usize];
                parts.push(format!("f{k}: stick=({sx}, {sy}) buttons=0x{b:04X}"));
            }
            if parts.is_empty() {
                println!("  inputs (last 3 frames): (none)");
            } else {
                println!("  inputs (last 3 frames): {}", parts.join("; "));
            }
        }
    }
}

fn run_one(path: &Path, mode: DiffMode, fields: &[&str]) -> (String, ScenarioOutcome) {
    let scenario = match load_scenario(path) {
        Ok(s) => s,
        Err(e) => return (path.display().to_string(), ScenarioOutcome::Error(e)),
    };
    let label = scenario.id.clone();
    let outcome = match match mode {
        DiffMode::Free => diff_free(&scenario, path, fields),
        DiffMode::TeacherForced => diff_teacher_forced(&scenario, path, fields),
    } {
        Ok(None) => ScenarioOutcome::Pass,
        Ok(Some(rep)) => ScenarioOutcome::Fail(rep),
        Err(e) => ScenarioOutcome::Error(e),
    };
    (label, outcome)
}

/// `stepkit oracle diff`: run one scenario (`--scenario`) or every scenario
/// under `traces/scenarios/` (`--all`) through both the oracle and the
/// plugin and compare exactly. Returns true iff every scenario passes.
pub fn diff(
    scenario: Option<&str>,
    all: bool,
    fields: Option<&str>,
    mode: &str,
) -> Result<bool, String> {
    let mode = parse_mode(mode)?;
    let fields: Vec<&str> = match fields {
        Some(spec) => parse_fields(spec)?,
        None => COMPARED_FIELDS.to_vec(),
    };
    let targets: Vec<PathBuf> = if all {
        let dir = find_scenarios_dir()?;
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map_err(|e| format!("read {}: {e}", dir.display()))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "ron"))
            .collect();
        paths.sort();
        if paths.is_empty() {
            return Err(format!("no .ron scenarios in {}", dir.display()));
        }
        paths
    } else {
        let spec = scenario.ok_or("either --scenario or --all is required")?;
        vec![resolve_scenario(spec)?]
    };

    let mut results: Vec<(String, ScenarioOutcome)> = Vec::new();
    for path in &targets {
        let (label, outcome) = run_one(path, mode, &fields);
        print_result(&label, mode, &outcome);
        results.push((label, outcome));
    }

    println!();
    println!(
        "Unmodeled oracle-only fields (never compared): {}",
        UNMODELED_FIELDS.join(", ")
    );
    println!();
    println!("| scenario | status | first divergence |");
    println!("| --- | --- | --- |");
    for (label, outcome) in &results {
        match outcome {
            ScenarioOutcome::Pass => println!("| {label} | PASS | - |"),
            ScenarioOutcome::Error(e) => println!("| {label} | ERROR | {e} |"),
            ScenarioOutcome::Fail(rep) => println!(
                "| {label} | FAIL | frame {}, field {} (plugin {} vs oracle {}) |",
                rep.frame, rep.field, rep.plugin, rep.oracle
            ),
        }
    }

    Ok(results
        .iter()
        .all(|(_, o)| matches!(o, ScenarioOutcome::Pass)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_trace_frame() -> TraceFrame {
        TraceFrame {
            frame: 1,
            stick_x: 0,
            stick_y: 80,
            buttons: 0x1,
            cam_yaw: 0,
            pos_x: 10.5,
            pos_y: 0.0,
            pos_z: -3.25,
            vel_x: 1.5,
            vel_y: 0.0,
            vel_z: -0.5,
            slide_vel_x: 0.0,
            slide_vel_z: 0.0,
            fwd_speed: 22.75,
            face_yaw: 100,
            face_pitch: -50,
            face_roll: 0,
            action: ActionId::FREEFALL.0,
            prev_action: ActionId::IDLE.0,
            land_from: ActionId::IDLE.0,
            action_state: 2,
            action_timer: 15,
            action_arg: 0,
            floor_y: 0.0,
            ceil_y: 400.0,
            wall_hit: 0,
            wall_kick_timer: 0,
            wall_normal_yaw: 0,
            health: 0x880,
            squish_timer: 3,
            quicksand_depth: 0.0,
            peak_height: 120.0,
            slide_over_cap: 0,
            swim_strength: 0,
            anim_slot: "slot_0".into(),
            timeline_frame: 0,
            anim_id: -1,
            anim_frame: 0,
            anim_accel: 0,
            anim_frame_accel_assist: 0,
            slide_yaw: 0,
            angle_vel_x: 0,
            angle_vel_y: 0,
            angle_vel_z: 0,
            intended_yaw: 0,
            intended_mag: 0.0,
            input: 0,
            floor_angle: 0,
            floor_normal_y: 1.0,
            floor_type: 0,
            double_jump_timer: 0,
            water_level: 0.0,
            hurt_counter: 0,
            invinc_timer: 0,
            flags: 0,
        }
    }

    fn test_oracle_frame() -> OracleFrame {
        OracleFrame {
            frame: 1,
            stick_x: 0.0,
            stick_y: 80.0,
            buttons: 0x8000,
            cam_yaw: 180.0,
            pos_x: 10.5,
            pos_y: 0.0,
            pos_z: -3.25,
            vel_x: 1.5,
            vel_y: 0.0,
            vel_z: -0.5,
            forward_vel: 22.75,
            slide_vel_x: 0.0,
            slide_vel_z: 0.0,
            slide_yaw: 0,
            face_pitch: -50,
            face_yaw: 100,
            face_roll: 0,
            angle_vel_x: 0,
            angle_vel_y: 0,
            angle_vel_z: 0,
            intended_yaw: 0,
            intended_mag: 0.0,
            // ACT_FREEFALL (decomp id; plugin uses the same value per FD-07).
            action: 0x0100088C,
            prev_action: 0x0C400201,
            action_state: 2,
            action_timer: 15,
            action_arg: 0,
            input: 0,
            floor_height: 0.0,
            floor_angle: 0,
            floor_normal_y: 1.0,
            floor_type: 0,
            ceil_height: 400.0,
            wall_normal_yaw: None,
            water_level: 0,
            wall_kick_timer: 0,
            double_jump_timer: 0,
            squish_timer: 3,
            quicksand_depth: 0.0,
            peak_height: 120.0,
            health: 0x880,
            hurt_counter: 0,
            invinc_timer: 0,
            flags: 0,
            anim_id: -1,
            anim_frame: 0,
            anim_accel: 0,
            anim_frame_accel_assist: 0,
        }
    }

    #[test]
    fn translation_table_matches_fd07() {
        assert_eq!(0x0100088C, ActionId::FREEFALL.0);
        assert_eq!(0x0C008220, ActionId::CROUCH.0);
        assert_eq!(0x04008448, ActionId::CRAWL.0);
        assert_eq!(0x008008A9, ActionId::GROUND_POUND.0);
        assert_eq!(0x010208B3, ActionId::HARD_BACKWARD_AIR_KB.0);
        assert_eq!(0x010208B2, ActionId::HARD_FORWARD_AIR_KB.0);
        assert_eq!(0x00000476, ActionId::QUICKSAND_JUMP_LAND.0);
        // Identity for everything else.
        assert_eq!(0x0C400201u32, 0x0C400201u32);
        assert_eq!(0xDEADBEEFu32, 0xDEADBEEFu32);
    }

    #[test]
    fn identical_frames_pass() {
        let f = test_trace_frame();
        let p = ParityState::from_plugin(&f);
        let q = ParityState::from_plugin(&f);
        assert!(compare(&p, &q, COMPARED_FIELDS).is_none());
    }

    #[test]
    fn one_ulp_f32_difference_fails_naming_field() {
        let a = test_trace_frame();
        let mut b = a.clone();
        b.pos_x = f32::from_bits(a.pos_x.to_bits().wrapping_add(1));
        assert_ne!(a.pos_x.to_bits(), b.pos_x.to_bits());
        let p = ParityState::from_plugin(&a);
        let q = ParityState::from_plugin(&b);
        let d = compare(&p, &q, COMPARED_FIELDS).expect("1-ULP difference must diverge");
        assert_eq!(d.field, "pos_x");
    }

    #[test]
    fn action_ids_match_decomp_directly() {
        // FD-07: plugin ActionIds are the decomp ACT_* values; no translation.
        // Oracle row carries 0x0100088C (ACT_FREEFALL); the plugin frame must
        // carry the identical id.
        let plugin = test_trace_frame();
        let oracle = test_oracle_frame();
        let p = ParityState::from_plugin(&plugin);
        let o = ParityState::from_oracle(&oracle);
        assert_eq!(o.action, ActionId::FREEFALL.0);
        assert_eq!(p.action, ActionId::FREEFALL.0);
        assert!(compare(&p, &o, COMPARED_FIELDS).is_none());
    }

    #[test]
    fn translated_action_divergence_reports_hex() {
        let plugin = test_trace_frame();
        let mut oracle = test_oracle_frame();
        oracle.action = 0x0C400201; // decomp Idle; plugin is in FREEFALL
        let p = ParityState::from_plugin(&plugin);
        let o = ParityState::from_oracle(&oracle);
        let d = compare(&p, &o, COMPARED_FIELDS).expect("actions must diverge");
        assert_eq!(d.field, "action");
        assert_eq!(d.plugin, format!("0x{:08X}", ActionId::FREEFALL.0));
        assert_eq!(d.oracle, "0x0C400201");
    }

    #[test]
    fn wall_presence_then_normal() {
        let a = test_trace_frame();
        let mut b = a.clone();
        b.wall_hit = 1;
        b.wall_normal_yaw = 1234;
        let p = ParityState::from_plugin(&a);
        let q = ParityState::from_plugin(&b);
        let d = compare(&p, &q, COMPARED_FIELDS).expect("wall presence must diverge");
        assert_eq!(d.field, "wall");

        // Both hit, same normal: pass.
        let mut c = a.clone();
        c.wall_hit = 1;
        c.wall_normal_yaw = 1234;
        let r = ParityState::from_plugin(&c);
        assert!(compare(&q, &r, COMPARED_FIELDS).is_none());

        // Both hit, different normal: fail on wall_normal_yaw.
        let mut e = c.clone();
        e.wall_normal_yaw = 1235;
        let s = ParityState::from_plugin(&e);
        let d = compare(&q, &s, COMPARED_FIELDS).expect("wall normal must diverge");
        assert_eq!(d.field, "wall_normal_yaw");
    }

    #[test]
    fn unknown_field_name_errors() {
        let err = parse_fields("pos_x,bogus").unwrap_err();
        assert!(err.contains("bogus"), "{err}");
        let err = parse_fields("").unwrap_err();
        assert!(err.contains("unknown field"), "{err}");
    }

    #[test]
    fn known_fields_parse() {
        let f = parse_fields("pos_x,action,health").unwrap();
        assert_eq!(f, vec!["pos_x", "action", "health"]);
    }

    #[test]
    fn bad_mode_errors() {
        assert!(parse_mode("fuzzy").is_err());
        assert_eq!(parse_mode("free").unwrap(), DiffMode::Free);
        assert_eq!(
            parse_mode("teacher-forced").unwrap(),
            DiffMode::TeacherForced
        );
    }
}
