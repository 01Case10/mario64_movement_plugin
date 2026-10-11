//! Sound stubs: replicate the decomp's flag side effects without audio.
//!
//! The decomp's `play_*` functions are not pure audio: `play_sound_if_no_flag`,
//! `play_mario_action_sound` and `play_mario_jump_sound` set
//! `MARIO_ACTION_SOUND_PLAYED` / `MARIO_MARIO_SOUND_PLAYED` in `m->flags`,
//! which is compared gameplay state (cleared by `set_mario_action`).
//! These stubs reproduce exactly the flag behavior and skip the audio.
//! decomp: src/game/mario.c:240-370.

use crate::state::{mario_flags, CharacterState};

/// decomp: `play_sound_if_no_flag` (mario.c:240).
/// Audio skipped; the flag is real state.
pub fn play_sound_if_no_flag(state: &mut CharacterState, flag: u32) {
    if state.flags & flag == 0 {
        state.flags |= flag;
    }
}

/// decomp: `play_mario_action_sound` (mario.c:304).
/// Audio and particles skipped; the flag is real state.
pub fn play_mario_action_sound(state: &mut CharacterState) {
    if state.flags & mario_flags::ACTION_SOUND_PLAYED == 0 {
        state.flags |= mario_flags::ACTION_SOUND_PLAYED;
    }
}

/// decomp: `play_mario_landing_sound_once` (mario.c:319).
/// The metal-cap branch only selects which sound plays; the flag effect
/// is identical, so the branch is not modeled.
pub fn play_mario_landing_sound_once(state: &mut CharacterState) {
    play_mario_action_sound(state);
}

/// decomp: `play_mario_heavy_landing_sound_once` (mario.c:333).
pub fn play_mario_heavy_landing_sound_once(state: &mut CharacterState) {
    play_mario_action_sound(state);
}

/// decomp: `play_mario_jump_sound` (mario.c:250).
/// Audio skipped; the flag is real state.
pub fn play_mario_jump_sound(state: &mut CharacterState) {
    if state.flags & mario_flags::MARIO_SOUND_PLAYED == 0 {
        state.flags |= mario_flags::MARIO_SOUND_PLAYED;
    }
}
