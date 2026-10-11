//! Minimal interaction system port.
//!
//! Verbatim ports from the decomp (`src/game/interaction.c`) for the parts
//! that affect `CharacterState` flags. The scenarios contain no objects, so
//! `mario_process_interactions` is a no-op and `bounce_off_object` is never
//! called — but the flag lifecycle is documented here for completeness.
//!
//! - `MARIO_UNKNOWN_08` (0x100): set by `set_mario_action` (mario.c:888) on
//!   every action transition; cleared by `bounce_off_object` (interaction.c:519).
//!   Gates `should_strengthen_gravity_for_jump_ascent`.
//! - `MARIO_UNKNOWN_18` (0x40000): set by `play_far_fall_sound` when fall height
//!   exceeds 1150; cleared by `set_mario_action` when the new action is not AIR.

use crate::state::{mario_flags, CharacterState};

/// decomp: `bounce_off_object` (`src/game/interaction.c:515`).
/// Clears `MARIO_UNKNOWN_08`. In the full game this is called when Mario
/// bounces off an enemy or object. Never called in the scenario set (no objects).
pub fn bounce_off_object(state: &mut CharacterState, vel_y: f32) {
    // decomp: m->pos[1] = o->oPosY + o->hitboxHeight (object-dependent, skipped).
    state.vel.y = vel_y;
    state.flags &= !mario_flags::UNKNOWN_08;
    // decomp: play_sound(SOUND_ACTION_BOUNCE_OFF_OBJECT) — audio, skipped.
}

/// decomp: `mario_process_interactions` (`src/game/interaction.c:1780`).
/// Processes Mario's interactions with objects. With no objects in the
/// scenario set, the interaction dispatch is a no-op, but the flag clearing
/// (interaction.c:1808) still applies: PUNCHING/KICKING/TRIPPING are wiped
/// every frame after the action runs.
pub fn mario_process_interactions(state: &mut CharacterState) {
    // No objects — interaction dispatch skipped.
    // decomp: m->flags &= ~MARIO_PUNCHING & ~MARIO_KICKING & ~MARIO_TRIPPING
    // (interaction.c:1808). The action sets these during its tick; they are
    // cleared before the next frame.
    state.flags &= !(mario_flags::PUNCHING | mario_flags::KICKING | mario_flags::TRIPPING);
}
