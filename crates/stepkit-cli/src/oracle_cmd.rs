//! `stepkit oracle run`: convert a scenario to the native oracle's `.oin`
//! input format, run the oracle binary over it, and write the oracle's CSV.
//!
//! The `.oin` format is line-based (`#` comments and blank lines are
//! ignored by the oracle):
//! ```text
//! version 1
//! terrain <int>              # TERRAIN_* (0 = GRASS)
//! verts <V>
//! <x> <y> <z>                # V lines, integers
//! tris <T>
//! <type> <i0> <i1> <i2>      # T lines; type = SURFACE_* int; vertex indices
//! water none                 # or: water <level_int>
//! start <px> <py> <pz> <yaw_deg> <action_hex>
//! poke forward_vel <f>       # zero or more
//! frames <N>
//! <sx> <sy> <buttons> <camyaw_deg>   # N lines
//! ```
//!
//! Geometry conversion lives in `stepkit_trace::geom` (strict integer
//! contracts, plan §6 Phase 2 ST-02); the scenario is validated with
//! [`Scenario::validate_geometry`] before conversion.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use stepkit_trace::geom::{self, Mesh};
use stepkit_trace::scenario::{Camera, Geometry, Scenario};

fn load_scenario(path: &str) -> Result<Scenario, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    ron::de::from_str(&text).map_err(|e| format!("parse {path}: {e}"))
}

fn water_line(s: &Scenario) -> Result<String, String> {
    if s.water.is_empty() {
        return Ok("water none".into());
    }
    // Phase-1 simplification: the oracle fills its water regions uniformly,
    // so only a single shared water level is passed through.
    let mut level: Option<i32> = None;
    for w in &s.water {
        let y = geom::int_coord(w.y, "water.y")?;
        match level {
            None => level = Some(y),
            Some(prev) if prev != y => {
                return Err(format!(
                    "water rects must share the same y (got {prev} and {y})"
                ));
            }
            _ => {}
        }
    }
    Ok(format!("water {}", level.unwrap()))
}

fn action_hex(name: &str) -> Result<u32, String> {
    match name {
        "Idle" => Ok(0x0C400201),
        "Freefall" => Ok(0x0100088C),
        "Walking" => Ok(0x04000440),
        _ => Err(format!("unsupported start action {name:?}")),
    }
}

fn start_line(s: &Scenario) -> Result<String, String> {
    // Strict: start positions were validated by Scenario::validate_geometry,
    // so int_coord cannot fail here on a validated scenario.
    let px = geom::int_coord(s.start.pos.0, "start.x")?;
    let py = geom::int_coord(s.start.pos.1, "start.y")?;
    let pz = geom::int_coord(s.start.pos.2, "start.z")?;
    let action = action_hex(&s.start.action)?;
    Ok(format!(
        "start {px} {py} {pz} {:.1} 0x{action:08X}",
        s.start.yaw_deg
    ))
}

/// Plugin button bits (A=0x1, B=0x2, Z=0x4, R=0x8, START=0x10) mapped to the
/// decomp controller bitmask. Values verified against
/// oracle/sm64/include/PR/os_cont.h: CONT_A=0x8000, CONT_B=0x4000,
/// CONT_G (Z_TRIG)=0x2000, CONT_R (R_TRIG)=0x0010, CONT_START=0x1000.
fn map_buttons(b: u16) -> u16 {
    let mut out = 0u16;
    if b & 0x1 != 0 {
        out |= 0x8000;
    }
    if b & 0x2 != 0 {
        out |= 0x4000;
    }
    if b & 0x4 != 0 {
        out |= 0x2000;
    }
    if b & 0x8 != 0 {
        out |= 0x0010;
    }
    if b & 0x10 != 0 {
        out |= 0x1000;
    }
    out
}

fn scenario_to_oin(s: &Scenario) -> Result<String, String> {
    let mut mesh = Mesh::default();
    for g in &s.geometry {
        match g {
            Geometry::Box(b) => geom::push_box(&mut mesh, b)?,
            Geometry::Ramp(r) => geom::push_ramp(&mut mesh, r)?,
        }
    }

    let mut oin = String::new();
    writeln!(oin, "version 1").unwrap();
    if s.terrain > 15 {
        return Err(format!(
            "terrain={} exceeds 15 (decomp TERRAIN_* values are small; TERRAIN_SLIDE=6)",
            s.terrain
        ));
    }
    writeln!(oin, "terrain {}", s.terrain).unwrap();
    writeln!(oin, "verts {}", mesh.verts.len()).unwrap();
    for &(x, y, z) in &mesh.verts {
        writeln!(oin, "{x} {y} {z}").unwrap();
    }
    writeln!(oin, "tris {}", mesh.tris.len()).unwrap();
    for &(surf, a, b, c) in &mesh.tris {
        writeln!(oin, "{surf} {a} {b} {c}").unwrap();
    }
    writeln!(oin, "{}", water_line(s)?).unwrap();
    // No pokes: the scenario format has no poke concept, so zero poke lines.
    writeln!(oin, "{}", start_line(s)?).unwrap();
    let frames = s.expand_inputs();
    let Camera::Fixed { yaw_deg } = &s.camera;
    // The oracle expects cam yaw in DECOMP convention (the direction from
    // Mario toward the camera); Camera::Fixed is the direction the camera
    // looks, so add 180 degrees.
    let decomp_yaw = yaw_deg + 180.0;
    writeln!(oin, "frames {}", frames.len()).unwrap();
    for &(sx, sy, buttons) in &frames {
        writeln!(oin, "{sx} {sy} {} {decomp_yaw:.1}", map_buttons(buttons)).unwrap();
    }
    Ok(oin)
}

/// Locate the oracle binary: `STEPKIT_ORACLE_BIN` wins, otherwise walk up
/// from the scenario's directory looking for `oracle/build/oracle`.
fn find_oracle(scenario_path: &str) -> Result<PathBuf, String> {
    if let Some(p) = std::env::var("STEPKIT_ORACLE_BIN")
        .ok()
        .filter(|s| !s.is_empty())
    {
        let p = PathBuf::from(&p);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!("STEPKIT_ORACLE_BIN={p:?} is not a file"));
    }
    let canon = Path::new(scenario_path)
        .canonicalize()
        .map_err(|e| format!("canonicalize {scenario_path}: {e}"))?;
    let mut tried = Vec::new();
    let mut dir = canon.parent().map(Path::to_path_buf);
    while let Some(d) = dir {
        let cand = d.join("oracle/build/oracle");
        tried.push(cand.display().to_string());
        if cand.is_file() {
            return Ok(cand);
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    Err(format!(
        "oracle binary not found (looked for {}); build it with ./oracle/build.sh",
        tried.join(", ")
    ))
}

/// `stepkit oracle run`: validate the scenario's geometry, convert it to
/// `.oin`, run the oracle binary, and leave the oracle's CSV at `out`.
pub fn run(scenario_path: &str, out: &str) -> Result<(), String> {
    let scenario = load_scenario(scenario_path)?;
    scenario
        .validate_geometry()
        .map_err(|e| format!("{scenario_path}: {e}"))?;
    let oin = scenario_to_oin(&scenario)?;
    let oracle = find_oracle(scenario_path)?;
    let tmp = std::env::temp_dir().join(format!("stepkit-oracle-{}.oin", std::process::id()));
    std::fs::write(&tmp, &oin).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    let status = Command::new(&oracle)
        .arg("--in")
        .arg(&tmp)
        .arg("--out")
        .arg(out)
        .status()
        .map_err(|e| format!("run {}: {e}", oracle.display()))?;
    if status.success() {
        let _ = std::fs::remove_file(&tmp);
        Ok(())
    } else {
        eprintln!("stepkit oracle: input kept at {}", tmp.display());
        Err(format!("oracle exited with status {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_mapping_matches_os_cont() {
        // Values verified against oracle/sm64/include/PR/os_cont.h.
        assert_eq!(map_buttons(0x1), 0x8000);
        assert_eq!(map_buttons(0x2), 0x4000);
        assert_eq!(map_buttons(0x4), 0x2000);
        assert_eq!(map_buttons(0x8), 0x0010);
        assert_eq!(map_buttons(0x10), 0x1000);
        assert_eq!(map_buttons(0x1 | 0x4), 0x8000 | 0x2000);
        assert_eq!(map_buttons(0), 0);
    }

    #[test]
    fn action_hex_known_actions() {
        assert_eq!(action_hex("Idle").unwrap(), 0x0C400201);
        assert_eq!(action_hex("Freefall").unwrap(), 0x0100088C);
        assert_eq!(action_hex("Walking").unwrap(), 0x04000440);
        assert!(action_hex("TripleJump").is_err());
    }

    #[test]
    fn all_scenarios_convert_to_oin() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../traces/scenarios");
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("ron"))
            .collect();
        paths.sort();
        assert!(!paths.is_empty(), "no scenarios in {}", dir.display());
        let mut count = 0;
        for path in &paths {
            let text = std::fs::read_to_string(path).unwrap();
            let scenario: Scenario = ron::de::from_str(&text)
                .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
            scenario
                .validate_geometry()
                .unwrap_or_else(|e| panic!("validate {}: {e}", path.display()));
            let oin = scenario_to_oin(&scenario)
                .unwrap_or_else(|e| panic!("convert {}: {e}", path.display()));
            let n = scenario.expand_inputs().len();
            assert!(
                oin.contains(&format!("frames {n}\n")),
                "{}: frame count mismatch",
                path.display()
            );
            assert!(
                oin.starts_with(&format!("version 1\nterrain {}\n", scenario.terrain)),
                "{}: bad header",
                path.display()
            );
            count += 1;
        }
        assert!(
            count >= 52,
            "expected at least 52 scenarios, converted {count}"
        );
    }
}
