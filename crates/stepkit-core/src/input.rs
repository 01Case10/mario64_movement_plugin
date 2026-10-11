//! Input: raw stick values, button bitfield, camera yaw.
//!
//! The core takes N64-style stick units (-128..127) and conditions them
//! exactly like the decomp: [`adjust_analog_stick`] (per-axis ±8 deadzone
//! with ∓6 offset, magnitude cap 64), then derives intended magnitude
//! (`(mag/64)^2*64`, halved — eighthed while squished) and intended yaw
//! (`atan2s(-stickY, stickX) + camera yaw`). [`process_input`] runs the
//! button and joystick stages of `update_mario_inputs` once per tick and
//! publishes the result as [`ProcessedInput`]; the geometry stages
//! (OFF_FLOOR/ABOVE_SLIDE/IN_WATER/SQUISHED/UNKNOWN_5) are OR'd in by the
//! full pipeline port (FD-01/FD-08).
//!
//! [`StickMapper`] converts a modern analog stick into those units.

use crate::angles::Angle;
use crate::state::CharacterState;

/// Buttons, as a bitfield. Values follow the community documentation's
/// button naming (A, B, Z, R); the core never reads a real controller.
pub mod buttons {
    pub const A: u16 = 0x0001;
    pub const B: u16 = 0x0002;
    pub const Z: u16 = 0x0004;
    pub const R: u16 = 0x0008;
    pub const START: u16 = 0x0010;
}

/// Per-frame input flags, mirroring the decomp's `INPUT_*` bits
/// (`oracle/sm64/include/sm64.h:52-67`). Computed by
/// `update_mario_inputs` (see `crate::step`) every tick before the action
/// runs; most actions branch on these rather than on raw buttons.
pub mod input_flags {
    /// The analog stick is held (intended magnitude > 0).
    pub const NONZERO_ANALOG: u32 = 0x0001;
    /// A was pressed this frame.
    pub const A_PRESSED: u32 = 0x0002;
    /// Mario is more than 100 units above the floor.
    pub const OFF_FLOOR: u32 = 0x0004;
    /// Mario is above a slippery slide surface.
    pub const ABOVE_SLIDE: u32 = 0x0008;
    /// First-person camera mode (always 0 here; no camera).
    pub const FIRST_PERSON: u32 = 0x0010;
    /// Neither analog input nor A pressed ("stick released").
    pub const UNKNOWN_5: u32 = 0x0020;
    /// Mario is being squished between a dynamic floor and ceiling.
    pub const SQUISHED: u32 = 0x0040;
    /// A is currently held.
    pub const A_DOWN: u32 = 0x0080;
    /// Mario is in poison gas.
    pub const IN_POISON_GAS: u32 = 0x0100;
    /// Mario is in water (more than 10 below the surface).
    pub const IN_WATER: u32 = 0x0200;
    /// Mario was stomped (Bowser attacks; always 0 here).
    pub const STOMPED: u32 = 0x0400;
    /// B was pressed this frame (gated by the squish timer).
    pub const B_PRESSED: u32 = 0x2000;
    /// Z is currently held (gated by the squish timer).
    pub const Z_DOWN: u32 = 0x4000;
    /// Z was pressed this frame (gated by the squish timer).
    pub const Z_PRESSED: u32 = 0x8000;
}

/// One frame of raw input to the core.
#[derive(Clone, Copy, Debug, Default)]
pub struct RawInput {
    /// Stick X in N64 units (-128..127).
    pub stick_x: i8,
    /// Stick Y in N64 units (-128..127).
    pub stick_y: i8,
    /// Button bitfield (see [`buttons`]); this is the decomp's
    /// `buttonDown` (currently held). The pressed edge is derived from the
    /// previous frame inside [`process_input`].
    pub buttons: u16,
    /// Camera look direction (where the camera looks, i.e. from the camera
    /// toward Mario). The decomp's `camera->yaw` points the opposite way
    /// (from Mario toward the camera); [`process_input`] converts with
    /// `+0x8000` at that boundary, mirroring what `oracle_cmd.rs` does
    /// (+180°) when generating the oracle's input files.
    pub cam_yaw: Angle,
}

/// Stick/buttons after the decomp's per-frame conditioning: the N64
/// `Controller` fields plus Mario's derived intent. Produced by
/// [`process_input`] once per tick, before the action runs.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessedInput {
    /// `controller->stickX`: raw X with the ±8 deadzone / ∓6 offset applied,
    /// capped so the magnitude never exceeds 64.
    pub stick_x: f32,
    /// `controller->stickY`: same conditioning as `stick_x`.
    pub stick_y: f32,
    /// `controller->stickMag`: post-cap magnitude (0..64). The dive check
    /// reads this, not the raw magnitude.
    pub stick_mag: f32,
    /// `controller->buttonPressed`: buttons newly pressed this frame.
    pub button_pressed: u16,
    /// `controller->buttonDown`: buttons currently held.
    pub button_down: u16,
    /// `m->intendedYaw`: world yaw the stick intends.
    pub intended_yaw: Angle,
    /// `m->intendedMag`: reshaped magnitude (`(mag/64)^2*64`, halved —
    /// eighthed while squished).
    pub intended_mag: f32,
    /// `m->input`: INPUT_* flags from the button and joystick stages.
    /// Geometry stages (OFF_FLOOR, ABOVE_SLIDE, IN_WATER, SQUISHED,
    /// UNKNOWN_5) are OR'd in by the full `update_mario_inputs` port
    /// (FD-01/FD-08); until then this carries the FD-02 subset.
    pub input: u32,
}

/// Stick conditioning, verbatim from the decomp: per-axis deadzone ±8 with
/// a ∓6 offset, then a hard cap of the magnitude at 64.
/// decomp: src/game/game_init.c:424
pub fn adjust_analog_stick(raw_x: i8, raw_y: i8) -> (f32, f32, f32) {
    // Reset the controller's x and y floats.
    let mut stick_x = 0.0f32;
    let mut stick_y = 0.0f32;

    // Modulate the rawStickX and rawStickY to be the new f32 values by adding/subtracting 6.
    if raw_x <= -8 {
        stick_x = raw_x as f32 + 6.0;
    }

    if raw_x >= 8 {
        stick_x = raw_x as f32 - 6.0;
    }

    if raw_y <= -8 {
        stick_y = raw_y as f32 + 6.0;
    }

    if raw_y >= 8 {
        stick_y = raw_y as f32 - 6.0;
    }

    // Calculate f32 magnitude from the center by vector length.
    let mut stick_mag = (stick_x * stick_x + stick_y * stick_y).sqrt();

    // Magnitude cannot exceed 64.0f: if it does, modify the values
    // appropriately to flatten the values down to the allowed maximum value.
    if stick_mag > 64.0 {
        stick_x *= 64.0 / stick_mag;
        stick_y *= 64.0 / stick_mag;
        stick_mag = 64.0;
    }

    (stick_x, stick_y, stick_mag)
}

/// The button and joystick stages of `update_mario_inputs`: computes the
/// pressed/down edges, conditions the stick, derives intended yaw/mag, and
/// sets the INPUT_* button/joystick flags on the state (fresh each tick —
/// the decomp zeroes `m->input` first).
/// decomp: src/game/mario.c:1253 (`update_mario_button_inputs`),
/// src/game/mario.c:1293 (`update_mario_joystick_inputs`)
pub fn process_input(
    raw: &RawInput,
    prev_buttons: u16,
    state: &mut CharacterState,
) -> ProcessedInput {
    let button_pressed = raw.buttons & !prev_buttons;
    let (stick_x, stick_y, stick_mag) = adjust_analog_stick(raw.stick_x, raw.stick_y);

    // ---- update_mario_button_inputs (mario.c:1253) ----
    let mut input: u32 = 0;
    if button_pressed & buttons::A != 0 {
        input |= input_flags::A_PRESSED;
    }

    if raw.buttons & buttons::A != 0 {
        input |= input_flags::A_DOWN;
    }

    // Don't update for these buttons if squished.
    if state.squish_timer == 0 {
        if button_pressed & buttons::B != 0 {
            input |= input_flags::B_PRESSED;
        }

        if raw.buttons & buttons::Z != 0 {
            input |= input_flags::Z_DOWN;
        }

        if button_pressed & buttons::Z != 0 {
            input |= input_flags::Z_PRESSED;
        }
    }

    if input & input_flags::A_PRESSED != 0 {
        state.frames_since_a = 0;
    } else if state.frames_since_a < 0xFF {
        state.frames_since_a += 1;
    }

    if input & input_flags::B_PRESSED != 0 {
        state.frames_since_b = 0;
    } else if state.frames_since_b < 0xFF {
        state.frames_since_b += 1;
    }

    // ---- update_mario_joystick_inputs (mario.c:1293) ----
    let mag = (stick_mag / 64.0) * (stick_mag / 64.0) * 64.0;

    let intended_mag = if state.squish_timer == 0 {
        mag / 2.0
    } else {
        mag / 8.0
    };

    let intended_yaw;
    if intended_mag > 0.0 {
        // The decomp adds the camera's yaw, which points from Mario toward
        // the camera; RawInput.cam_yaw is the look direction, so convert at
        // this boundary (+0x8000), mirroring oracle_cmd.rs's +180°.
        let decomp_cam_yaw = raw.cam_yaw.wrapping_add(Angle::HALF_TURN);
        intended_yaw = crate::trig::atan2s(-stick_y, stick_x).wrapping_add(decomp_cam_yaw);
        input |= input_flags::NONZERO_ANALOG;
    } else {
        intended_yaw = state.face_yaw;
    }

    state.intended_yaw = intended_yaw;
    state.intended_mag = intended_mag;
    // Fresh each tick (the decomp zeroes m->input at the top of
    // update_mario_inputs); geometry bits are OR'd in later by the full
    // port (FD-01/FD-08).
    state.input = input;

    ProcessedInput {
        stick_x,
        stick_y,
        stick_mag,
        button_pressed,
        button_down: raw.buttons,
        intended_yaw,
        intended_mag,
        input,
    }
}

/// Converts a modern -1.0..1.0 analog stick into N64 stick units.
#[derive(Clone, Copy, Debug)]
pub struct StickMapper {
    /// Output scale; 80.0 reproduces the N64 stick's typical max deflection.
    pub scale: f32,
    /// Inputs below this (in -1.0..1.0 units) map to zero.
    pub deadzone: f32,
}

impl StickMapper {
    /// N64-style preset: octagonal-ish response with a small deadzone.
    pub const N64_STYLE: StickMapper = StickMapper {
        scale: 80.0,
        deadzone: 0.12,
    };
    /// Plain linear preset for modern gamepads.
    pub const PLAIN: StickMapper = StickMapper {
        scale: 80.0,
        deadzone: 0.05,
    };

    pub fn map(&self, x: f32, y: f32) -> (i8, i8) {
        let mag = (x * x + y * y).sqrt();
        if mag < self.deadzone {
            return (0, 0);
        }
        let clamped = mag.min(1.0);
        let scale = self.scale * clamped / mag.max(f32::EPSILON);
        (
            (x * scale).round().clamp(-128.0, 127.0) as i8,
            (y * scale).round().clamp(-128.0, 127.0) as i8,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadzone_maps_to_zero() {
        assert_eq!(StickMapper::N64_STYLE.map(0.05, 0.0), (0, 0));
    }

    #[test]
    fn full_deflection_maps_to_scale() {
        assert_eq!(StickMapper::N64_STYLE.map(1.0, 0.0), (80, 0));
        assert_eq!(StickMapper::N64_STYLE.map(0.0, -1.0), (0, -80));
    }

    #[test]
    fn adjust_deadzone_and_offset() {
        // Inside ±8: zero.
        assert_eq!(adjust_analog_stick(7, -7), (0.0, 0.0, 0.0));
        // At ±8 the ∓6 offset applies.
        assert_eq!(adjust_analog_stick(8, 0), (2.0, 0.0, 2.0));
        assert_eq!(adjust_analog_stick(-8, 0), (-2.0, 0.0, 2.0));
        assert_eq!(adjust_analog_stick(0, -8), (0.0, -2.0, 2.0));
        // Full N64 tilt minus the offset.
        let (x, y, m) = adjust_analog_stick(80, 0);
        assert_eq!((x, y, m), (64.0, 0.0, 64.0));
    }

    #[test]
    fn adjust_caps_magnitude_at_64() {
        // Diagonal full tilt would be ~104.6 pre-cap; it flattens to 64.
        let (x, y, m) = adjust_analog_stick(80, 80);
        assert!((m - 64.0).abs() < 1e-6, "mag {m}");
        assert!((x - 64.0 / std::f32::consts::SQRT_2).abs() < 1e-4, "x {x}");
        assert!((y - 64.0 / std::f32::consts::SQRT_2).abs() < 1e-4, "y {y}");
    }

    #[test]
    fn process_input_derives_intent() {
        let raw = RawInput {
            stick_x: 0,
            stick_y: 80,
            buttons: buttons::A,
            cam_yaw: Angle::from_degrees(90.0),
        };
        let mut state = CharacterState::default();
        let proc = process_input(&raw, 0, &mut state);
        // stickY = 80 - 6 = 74, capped to 64; intendedMag = (64/64)^2*64/2.
        let expect_mag = 32.0f32;
        assert!((proc.intended_mag - expect_mag).abs() < 1e-4, "{}", proc.intended_mag);
        // atan2s(-64, 0) = 0x8000; decomp cam yaw = look (90°) + 180°.
        let expect_yaw = Angle(0x8000u16 as i16).wrapping_add(Angle::from_degrees(270.0));
        assert_eq!(proc.intended_yaw, expect_yaw);
        assert_eq!(proc.input & input_flags::NONZERO_ANALOG, input_flags::NONZERO_ANALOG);
        assert_eq!(proc.input & input_flags::A_PRESSED, input_flags::A_PRESSED);
        assert_eq!(proc.input & input_flags::A_DOWN, input_flags::A_DOWN);
        // State mirrors the processed input.
        assert_eq!(state.intended_yaw, proc.intended_yaw);
        assert_eq!(state.intended_mag, proc.intended_mag);
        assert_eq!(state.input, proc.input);
        assert_eq!(state.frames_since_a, 0);
    }

    #[test]
    fn process_input_squish_gates_buttons_and_quarters_mag() {
        let raw = RawInput {
            stick_x: 0,
            stick_y: 80,
            buttons: buttons::B | buttons::Z,
            cam_yaw: Angle::ZERO,
        };
        let mut state = CharacterState::default();
        state.squish_timer = 5;
        let proc = process_input(&raw, 0, &mut state);
        // B/Z are swallowed while squished; mag is eighthed, not halved.
        assert_eq!(proc.input & input_flags::B_PRESSED, 0);
        assert_eq!(proc.input & input_flags::Z_DOWN, 0);
        let expect_mag = 8.0f32; // 64 / 8 while squished
        assert!((proc.intended_mag - expect_mag).abs() < 1e-4, "{}", proc.intended_mag);
    }

    #[test]
    fn process_input_no_analog_keeps_face_yaw() {
        let raw = RawInput {
            stick_x: 0,
            stick_y: 0,
            buttons: 0,
            cam_yaw: Angle::ZERO,
        };
        let mut state = CharacterState::default();
        state.face_yaw = Angle::from_degrees(45.0);
        let proc = process_input(&raw, 0, &mut state);
        assert_eq!(proc.intended_mag, 0.0);
        assert_eq!(proc.intended_yaw, state.face_yaw);
        assert_eq!(proc.input & input_flags::NONZERO_ANALOG, 0);
    }
}
