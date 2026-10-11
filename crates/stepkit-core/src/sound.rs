//! Sound event system.
//!
//! The Rust sim core is headless — it does not play audio. Instead, actions
//! push `SoundEvent`s to a per-tick buffer in `CharacterState`. The Godot
//! binding drains the buffer each tick and plays the corresponding audio.
//!
//! This mirrors the decomp's `play_sound` / `play_sound_if_no_flag` /
//! `play_mario_action_sound` functions, which set the
//! `MARIO_ACTION_SOUND_PLAYED` / `MARIO_MARIO_SOUND_PLAYED` flags. Getting
//! those flags right is required for parity.
//! decomp: src/game/mario.c:240-370.

use crate::state::{mario_flags, CharacterState};

/// A sound event emitted by the sim. The Godot binding maps these to audio streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SoundEvent {
    /// Jump (SOUND_MARIO_WAH2)
    Jump = 0,
    /// Double jump (SOUND_MARIO_UH / SOUND_MARIO_WAH2)
    DoubleJump = 1,
    /// Triple jump (SOUND_MARIO_WAH2)
    TripleJump = 2,
    /// Backflip / side flip (SOUND_MARIO_WAH2)
    Flip = 3,
    /// Dive (SOUND_ACTION_THROW)
    Dive = 4,
    /// Punch (SOUND_MARIO_PUNCH)
    Punch = 5,
    /// Kick (SOUND_MARIO_KICK)
    Kick = 6,
    /// Ground pound (SOUND_ACTION_GROUND_POUND)
    GroundPound = 7,
    /// Wall bonk (SOUND_MARIO_DOH)
    Bonk = 8,
    /// Landing (SOUND_ACTION_TERRAIN_LANDING)
    Landing = 9,
    /// Footstep (SOUND_ACTION_TERRAIN_STEP)
    Step = 10,
    /// Fall scream (SOUND_MARIO_WAAAOOOW) — the UNKNOWN_18 flag
    FallScream = 11,
    /// Swim stroke (SOUND_ACTION_SWIM)
    Swim = 12,
    /// Slide (SOUND_ACTION_TERRAIN_SLIDE)
    Slide = 13,
}

impl SoundEvent {
    /// All events, for UI population.
    pub const ALL: [SoundEvent; 14] = [
        SoundEvent::Jump,
        SoundEvent::DoubleJump,
        SoundEvent::TripleJump,
        SoundEvent::Flip,
        SoundEvent::Dive,
        SoundEvent::Punch,
        SoundEvent::Kick,
        SoundEvent::GroundPound,
        SoundEvent::Bonk,
        SoundEvent::Landing,
        SoundEvent::Step,
        SoundEvent::FallScream,
        SoundEvent::Swim,
        SoundEvent::Slide,
    ];

    /// Human-readable name for UI.
    pub fn name(self) -> &'static str {
        match self {
            SoundEvent::Jump => "Jump",
            SoundEvent::DoubleJump => "Double Jump",
            SoundEvent::TripleJump => "Triple Jump",
            SoundEvent::Flip => "Flip",
            SoundEvent::Dive => "Dive",
            SoundEvent::Punch => "Punch",
            SoundEvent::Kick => "Kick",
            SoundEvent::GroundPound => "Ground Pound",
            SoundEvent::Bonk => "Bonk",
            SoundEvent::Landing => "Landing",
            SoundEvent::Step => "Step",
            SoundEvent::FallScream => "Fall Scream",
            SoundEvent::Swim => "Swim",
            SoundEvent::Slide => "Slide",
        }
    }
}

/// decomp: `play_sound_if_no_flag` (mario.c:240).
/// Pushes the event and sets the flag, but only if the flag is not already
/// set. This is how the decomp prevents duplicate sounds within an action.
pub fn play_sound_if_no_flag(state: &mut CharacterState, event: SoundEvent, flag: u32) {
    if state.flags & flag == 0 {
        if (state.sound_count as usize) < state.sounds.len() {
            state.sounds[state.sound_count as usize] = Some(event);
            state.sound_count += 1;
        }
        state.flags |= flag;
    }
}

/// decomp: `play_mario_action_sound` (mario.c:304).
/// Uses MARIO_ACTION_SOUND_PLAYED flag.
pub fn play_mario_action_sound(state: &mut CharacterState, event: SoundEvent) {
    play_sound_if_no_flag(state, event, mario_flags::ACTION_SOUND_PLAYED);
}

/// decomp: `play_mario_landing_sound_once` (mario.c:319).
/// The metal-cap branch only selects which sound plays; the flag effect
/// is identical, so the branch is not modeled.
pub fn play_mario_landing_sound_once(state: &mut CharacterState, event: SoundEvent) {
    play_mario_action_sound(state, event);
}

/// decomp: `play_mario_heavy_landing_sound_once` (mario.c:333).
pub fn play_mario_heavy_landing_sound_once(state: &mut CharacterState, event: SoundEvent) {
    play_mario_action_sound(state, event);
}

/// decomp: `play_mario_jump_sound` (mario.c:250).
/// Uses MARIO_MARIO_SOUND_PLAYED flag.
pub fn play_mario_jump_sound(state: &mut CharacterState, event: SoundEvent) {
    play_sound_if_no_flag(state, event, mario_flags::MARIO_SOUND_PLAYED);
}
