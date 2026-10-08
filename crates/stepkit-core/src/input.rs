//! Input: raw stick values, button bitfield, camera yaw.
//!
//! The core takes N64-style stick units (-128..127, commonly cited caps at
//! magnitudes like 48 and 54) and derives intended magnitude/direction
//! itself. [`StickMapper`] converts a modern analog stick into those units.

use crate::angles::Angle;

/// Buttons, as a bitfield. Values follow the community documentation's
/// button naming (A, B, Z, R); the core never reads a real controller.
pub mod buttons {
    pub const A: u16 = 0x0001;
    pub const B: u16 = 0x0002;
    pub const Z: u16 = 0x0004;
    pub const R: u16 = 0x0008;
    pub const START: u16 = 0x0010;
}

/// One frame of raw input to the core.
#[derive(Clone, Copy, Debug, Default)]
pub struct RawInput {
    /// Stick X in N64 units (-128..127).
    pub stick_x: i8,
    /// Stick Y in N64 units (-128..127).
    pub stick_y: i8,
    /// Button bitfield (see [`buttons`]).
    pub buttons: u16,
    /// Camera yaw supplied by the caller; the stick is relative to it.
    pub cam_yaw: Angle,
}

impl RawInput {
    /// Raw stick magnitude in stick units (0.0 .. ~181.0).
    pub fn raw_magnitude(&self) -> f32 {
        let x = self.stick_x as f32;
        let y = self.stick_y as f32;
        (x * x + y * y).sqrt()
    }

    /// Stick magnitude reshaped the way the reference game does it:
    /// quadratic, `(raw/64)^2 * 64`. Full N64 tilt (~80) lands near 100;
    /// synthetic full deflection (181) higher. Compare against
    /// [`RawInput::intended_mag`] for speed targets.
    pub fn magnitude(&self) -> f32 {
        let raw = self.raw_magnitude();
        (raw / 64.0).powi(2) * 64.0
    }

    /// Intended speed magnitude: reshaped magnitude halved, so full tilt
    /// feeds the walker's 32 target (before its own cap).
    pub fn intended_mag(&self) -> f32 {
        self.magnitude() / 2.0
    }

    /// True when the stick is pushed far enough to count as held.
    /// spec: input.stick_deadzone (verify)
    pub fn stick_held(&self) -> bool {
        self.raw_magnitude() > 8.0
    }

    /// Intended world-space yaw: camera yaw plus stick direction.
    pub fn intended_yaw(&self) -> Option<Angle> {
        if !self.stick_held() {
            return None;
        }
        let yaw = crate::trig::atan2(-(self.stick_x as f32), self.stick_y as f32);
        Some(self.cam_yaw.wrapping_add(yaw))
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
    fn intended_yaw_follows_camera() {
        let input = RawInput {
            stick_x: 0,
            stick_y: 80,
            buttons: 0,
            cam_yaw: Angle::from_degrees(90.0),
        };
        // Stick "up" means away from camera; atan2(-0, 80) = 0.
        let yaw = input.intended_yaw().unwrap();
        assert!((yaw.to_degrees() - 90.0).abs() < 0.1);
    }
}
