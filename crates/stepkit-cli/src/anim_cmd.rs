//! `stepkit anim` subcommands: validate clips, generate reference clips.

use std::path::{Path, PathBuf};

use stepkit_anim::manifest::TimingManifest;
use stepkit_anim::rig::{BoneMap, REQUIRED_BONES};
use stepkit_anim::validator::{validate_clip, ClipInfo};

fn load_manifest(path: &str) -> Result<TimingManifest, String> {
    let s = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    TimingManifest::from_toml_str(&s).map_err(|e| format!("parse {path}: {e}"))
}

/// Validate every `*.clip.json` in `dir` against the manifest.
pub fn validate(manifest: &str, dir: &str, bone_map: Option<&str>) -> Result<(), String> {
    let m = load_manifest(manifest)?;
    let bm = match bone_map {
        Some(p) => {
            let s = std::fs::read_to_string(p).map_err(|e| format!("read bone map {p}: {e}"))?;
            serde_json::from_str::<BoneMap>(&s).map_err(|e| format!("parse bone map {p}: {e}"))?
        }
        None => BoneMap::default(),
    };
    let dir = Path::new(dir);
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("read {dir:?}: {e}"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    files.sort();
    if files.is_empty() {
        return Err(format!("no clip JSON files in {dir:?}"));
    }
    let mut total_errors = 0;
    for f in files {
        let s = std::fs::read_to_string(&f).map_err(|e| format!("read {}: {e}", f.display()))?;
        let clip: ClipInfo =
            serde_json::from_str(&s).map_err(|e| format!("parse {}: {e}", f.display()))?;
        // Match clip to slot by name convention: "<slot>.clip.json".
        let stem = f
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .trim_end_matches(".clip");
        let slot = m
            .slot(stem)
            .ok_or_else(|| format!("{}: no slot '{}' in manifest", f.display(), stem))?;
        let report = validate_clip(&m, slot, &clip, &bm);
        for fd in &report.findings {
            let sev = match fd.severity {
                stepkit_anim::validator::Severity::Error => "ERROR",
                stepkit_anim::validator::Severity::Warn => "WARN",
                stepkit_anim::validator::Severity::Info => "INFO",
            };
            println!("{} [{sev}] {:?}: {}", f.display(), fd.rule, fd.message);
        }
        total_errors += report.errors();
        if report.errors() == 0 {
            println!("{}: PASS ({} warnings)", f.display(), report.warns());
        }
    }
    if total_errors > 0 {
        Err(format!("{total_errors} validation errors"))
    } else {
        Ok(())
    }
}

/// Generate a passing reference [`ClipInfo`] JSON per slot.
pub fn generate_reference(manifest: &str, out: &str) -> Result<(), String> {
    let m = load_manifest(manifest)?;
    let out = Path::new(out);
    std::fs::create_dir_all(out).map_err(|e| format!("mkdir {out:?}: {e}"))?;
    let bones: Vec<String> = REQUIRED_BONES.iter().map(|s| s.to_string()).collect();
    for slot in &m.slots {
        let n = slot.frames;
        let clip = ClipInfo {
            name: slot.id.clone(),
            length_seconds: n as f32 / 30.0,
            fps: 30.0,
            loop_mode: slot.mode == stepkit_anim::manifest::SlotMode::Loop,
            markers: slot
                .events
                .iter()
                .map(|e| (e.name.clone(), e.frame))
                .collect(),
            bones: bones.clone(),
            root_travel: 0.0,
            key_times: (0..n).map(|f| f as f32 / 30.0).collect(),
            tracks: bones.clone(),
        };
        let path = out.join(format!("{}.clip.json", slot.id));
        let s = serde_json::to_string_pretty(&clip).map_err(|e| e.to_string())?;
        std::fs::write(&path, s).map_err(|e| format!("write {}: {e}", path.display()))?;
        println!("wrote {}", path.display());
    }
    Ok(())
}
