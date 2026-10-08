//! Validator rules T01..T10. One Rust rule engine works on the abstract
//! [`ClipInfo`]; the editor dock and the CLI are thin front ends.

use crate::manifest::{Slot, SlotMode, TimingManifest};
use crate::rig::{BoneMap, REQUIRED_BONES};

/// Rule identifiers, matching the plan's validator table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RuleId {
    /// T01: clip length x 30 equals the manifest frame count.
    Length,
    /// T02: loop flag matches the slot mode.
    Loop,
    /// T03: each manifest event has a marker near its frame.
    Events,
    /// T04: root horizontal travel within tolerance unless root motion.
    InPlace,
    /// T05: every required bone exists (or is mapped).
    Skeleton,
    /// T06: feet at lowest point inside contact windows (needs FK; later).
    Contacts,
    /// T07: transition pops between connectable slots (needs FK; later).
    TransitionPops,
    /// T08: keys sit on the 30 Hz grid.
    KeyGrid,
    /// T09: event names unique and in frame order.
    EventHygiene,
    /// T10: clip uses only rig-profile tracks.
    Extras,
}

/// Severity of a finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warn,
    Info,
}

/// One rule finding.
#[derive(Clone, Debug)]
pub struct Finding {
    pub rule: RuleId,
    pub severity: Severity,
    pub slot: String,
    pub clip: String,
    pub message: String,
}

/// Abstract clip description; front ends build this from Godot `Animation`
/// resources or glTF files.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ClipInfo {
    pub name: String,
    /// Length in seconds.
    pub length_seconds: f32,
    /// Frames per second of the key data.
    pub fps: f32,
    pub loop_mode: bool,
    /// (event name, frame at 30 fps).
    pub markers: Vec<(String, u32)>,
    /// Bone names present in the clip.
    pub bones: Vec<String>,
    /// Max horizontal root travel in SM64 units (for T04).
    pub root_travel: f32,
    /// Key times in seconds (for T08).
    pub key_times: Vec<f32>,
    /// Track names in the clip (for T10; usually bone names).
    pub tracks: Vec<String>,
}

/// Report for one clip against one slot.
#[derive(Clone, Debug, Default)]
pub struct ClipReport {
    pub findings: Vec<Finding>,
}

impl ClipReport {
    pub fn errors(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count()
    }

    pub fn warns(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Warn)
            .count()
    }
}

fn finding(
    rule: RuleId,
    severity: Severity,
    slot: &Slot,
    clip: &ClipInfo,
    message: String,
) -> Finding {
    Finding {
        rule,
        severity,
        slot: slot.id.clone(),
        clip: clip.name.clone(),
        message,
    }
}

/// Run rules T01..T05 and T08..T10 against one clip.
/// T06/T07 need forward kinematics and arrive in a later milestone.
pub fn validate_clip(
    manifest: &TimingManifest,
    slot: &Slot,
    clip: &ClipInfo,
    bone_map: &BoneMap,
) -> ClipReport {
    let mut report = ClipReport::default();
    let tol = &manifest.tolerances;

    // T01 Length: clip length * 30 == manifest frame count.
    let clip_frames = (clip.length_seconds * 30.0).round() as i32;
    if clip_frames != slot.frames as i32 {
        report.findings.push(finding(
            RuleId::Length,
            Severity::Error,
            slot,
            clip,
            format!(
                "T01 length {:.3}s = {} frames, manifest expects {}",
                clip.length_seconds, clip_frames, slot.frames
            ),
        ));
    }

    // T02 Loop: loop flag matches the slot mode.
    let want_loop = slot.mode == SlotMode::Loop;
    if clip.loop_mode != want_loop {
        report.findings.push(finding(
            RuleId::Loop,
            Severity::Error,
            slot,
            clip,
            format!(
                "T02 loop_mode={} but slot '{}' is {:?}",
                clip.loop_mode, slot.id, slot.mode
            ),
        ));
    }

    // T03 Events: each manifest event has a marker within tolerance.
    for ev in &slot.events {
        let hit = clip.markers.iter().any(|(name, frame)| {
            name == &ev.name
                && (*frame as i32 - ev.frame as i32).abs() <= tol.event_frame_tolerance as i32
        });
        if !hit {
            report.findings.push(finding(
                RuleId::Events,
                Severity::Error,
                slot,
                clip,
                format!(
                    "T03 event '{}' has no marker within {} frames of {}",
                    ev.name, tol.event_frame_tolerance, ev.frame
                ),
            ));
        }
    }

    // T04 InPlace: root travel within tolerance for in-place slots.
    if matches!(slot.root, crate::manifest::RootPolicy::InPlace)
        && clip.root_travel > tol.in_place_travel
    {
        report.findings.push(finding(
            RuleId::InPlace,
            Severity::Error,
            slot,
            clip,
            format!(
                "T04 root travel {:.1}u exceeds {:.1}u for in-place slot '{}'",
                clip.root_travel, tol.in_place_travel, slot.id
            ),
        ));
    }

    // T05 Skeleton: every required bone exists (or is mapped).
    for bone in REQUIRED_BONES {
        let resolved = bone_map.resolve(bone);
        if !clip.bones.iter().any(|b| b == resolved) {
            report.findings.push(finding(
                RuleId::Skeleton,
                Severity::Error,
                slot,
                clip,
                format!(
                    "T05 required bone '{}' (mapped '{}') missing",
                    bone, resolved
                ),
            ));
        }
    }

    // T08 KeyGrid: keys on the 30 Hz grid.
    for kt in &clip.key_times {
        let frame = kt * 30.0;
        if (frame - frame.round()).abs() > 1e-3 {
            report.findings.push(finding(
                RuleId::KeyGrid,
                Severity::Warn,
                slot,
                clip,
                format!("T08 key at {:.4}s is off the 30 Hz grid", kt),
            ));
            break; // one warning is enough
        }
    }

    // T09 EventHygiene: marker names unique and in frame order.
    let mut seen = std::collections::HashSet::new();
    let mut last_frame = 0u32;
    for (name, frame) in &clip.markers {
        if !seen.insert(name) {
            report.findings.push(finding(
                RuleId::EventHygiene,
                Severity::Error,
                slot,
                clip,
                format!("T09 duplicate marker name '{}'", name),
            ));
        }
        if *frame < last_frame {
            report.findings.push(finding(
                RuleId::EventHygiene,
                Severity::Error,
                slot,
                clip,
                format!(
                    "T09 marker '{}' at {} is before frame {}",
                    name, frame, last_frame
                ),
            ));
        }
        last_frame = *frame;
    }

    // T10 Extras: tracks must be rig-profile bones (or mapped).
    for track in &clip.tracks {
        let known = REQUIRED_BONES.contains(&track.as_str())
            || bone_map.mappings.iter().any(|(_, f)| f == track);
        if !known {
            report.findings.push(finding(
                RuleId::Extras,
                Severity::Warn,
                slot,
                clip,
                format!("T10 track '{}' is not a rig-profile bone", track),
            ));
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::*;

    fn slot() -> Slot {
        Slot {
            id: "walk_cycle".into(),
            frames: 24,
            mode: SlotMode::Loop,
            speed: SpeedModel::Fixed(1.0),
            events: vec![AnimEvent {
                name: "footstep_l".into(),
                frame: 0,
            }],
            root: RootPolicy::InPlace,
            contacts: vec![],
        }
    }

    fn manifest() -> TimingManifest {
        TimingManifest {
            fps: 30,
            slots: vec![slot()],
            tolerances: ToleranceProfile::default(),
        }
    }

    fn good_clip() -> ClipInfo {
        ClipInfo {
            name: "Walk".into(),
            length_seconds: 0.8,
            fps: 30.0,
            loop_mode: true,
            markers: vec![("footstep_l".into(), 0)],
            bones: REQUIRED_BONES.iter().map(|s| s.to_string()).collect(),
            root_travel: 0.5,
            key_times: vec![0.0, 1.0 / 30.0, 2.0 / 30.0],
            tracks: REQUIRED_BONES.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn t01_length_mismatch_is_error() {
        let clip = ClipInfo {
            length_seconds: 0.9,
            ..good_clip()
        };
        let report = validate_clip(&manifest(), &slot(), &clip, &BoneMap::default());
        assert!(report.findings.iter().any(|f| f.rule == RuleId::Length));
    }

    #[test]
    fn good_clip_passes() {
        let report = validate_clip(&manifest(), &slot(), &good_clip(), &BoneMap::default());
        assert_eq!(report.errors(), 0, "{:?}", report.findings);
    }

    #[test]
    fn t02_loop_mismatch_is_error() {
        let clip = ClipInfo {
            loop_mode: false,
            ..good_clip()
        };
        let report = validate_clip(&manifest(), &slot(), &clip, &BoneMap::default());
        assert!(report.findings.iter().any(|f| f.rule == RuleId::Loop));
    }

    #[test]
    fn t03_missing_event_is_error() {
        let clip = ClipInfo {
            markers: vec![],
            ..good_clip()
        };
        let report = validate_clip(&manifest(), &slot(), &clip, &BoneMap::default());
        assert!(report.findings.iter().any(|f| f.rule == RuleId::Events));
    }

    #[test]
    fn t04_excess_travel_is_error() {
        let clip = ClipInfo {
            root_travel: 10.0,
            ..good_clip()
        };
        let report = validate_clip(&manifest(), &slot(), &clip, &BoneMap::default());
        assert!(report.findings.iter().any(|f| f.rule == RuleId::InPlace));
    }

    #[test]
    fn t05_missing_bone_is_error() {
        let clip = ClipInfo {
            bones: vec!["root".into()],
            tracks: vec!["root".into()],
            ..good_clip()
        };
        let r = validate_clip(&manifest(), &slot(), &clip, &BoneMap::default());
        assert!(r.findings.iter().any(|f| f.rule == RuleId::Skeleton));
    }

    #[test]
    fn t08_off_grid_key_warns() {
        let clip = ClipInfo {
            key_times: vec![0.0123],
            ..good_clip()
        };
        let report = validate_clip(&manifest(), &slot(), &clip, &BoneMap::default());
        assert!(report.findings.iter().any(|f| f.rule == RuleId::KeyGrid));
    }

    #[test]
    fn t09_duplicate_marker_is_error() {
        let clip = ClipInfo {
            markers: vec![("a".into(), 0), ("a".into(), 5)],
            ..good_clip()
        };
        // 'a' is not the expected event, so T03 also fires; check T09 specifically.
        let report = validate_clip(&manifest(), &slot(), &clip, &BoneMap::default());
        assert!(report
            .findings
            .iter()
            .any(|f| f.rule == RuleId::EventHygiene));
    }

    #[test]
    fn t10_unknown_track_warns() {
        let mut clip = good_clip();
        clip.tracks.push("cape_bone".into());
        let report = validate_clip(&manifest(), &slot(), &clip, &BoneMap::default());
        assert!(report.findings.iter().any(|f| f.rule == RuleId::Extras));
    }
}
