//! Rig profile `stepkit-humanoid-v1`: the bone set the validator expects.
//!
//! About 20 bones. Other rigs map their bone names onto these through a
//! bone-map file, so any humanoid can be validated and driven.

/// Bones every rig must provide (or map to).
pub const REQUIRED_BONES: &[&str] = &[
    "root",
    "pelvis",
    "spine",
    "chest",
    "neck",
    "head", //
    "shoulder_l",
    "upper_arm_l",
    "forearm_l",
    "hand_l", //
    "shoulder_r",
    "upper_arm_r",
    "forearm_r",
    "hand_r", //
    "thigh_l",
    "shin_l",
    "foot_l", //
    "thigh_r",
    "shin_r",
    "foot_r",
];

/// Feet bones, used by the contact rule (T06).
pub const FOOT_BONES: &[&str] = &["foot_l", "foot_r"];

/// Maps a foreign rig's bone names onto the profile.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct BoneMap {
    /// (profile bone, foreign bone) pairs.
    pub mappings: Vec<(String, String)>,
}

impl BoneMap {
    /// Resolve a profile bone name through the map.
    pub fn resolve<'a>(&'a self, profile_bone: &'a str) -> &'a str {
        self.mappings
            .iter()
            .find(|(p, _)| p == profile_bone)
            .map(|(_, f)| f.as_str())
            .unwrap_or(profile_bone)
    }
}
