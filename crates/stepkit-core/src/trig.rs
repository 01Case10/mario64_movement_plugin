//! Deterministic trigonometry on 16-bit angles.
//!
//! The sine/cosine tables are generated once from `f64::sin` rounded to
//! `f32`. Rust's standard-library float math is implemented in portable Rust
//! code, so the generated tables are bit-identical on every platform. If
//! traces ever show a mismatch, the table construction (size, rounding,
//! interpolation, index shift) is adjusted until they agree -- no table is
//! ever imported from game data.

use crate::angles::Angle;
use std::sync::OnceLock;

/// Number of table entries; the index is the top bits of the angle.
const TABLE_BITS: u32 = 12;
const TABLE_SIZE: usize = 1 << TABLE_BITS; // 4096

fn sin_table() -> &'static [f32; TABLE_SIZE] {
    static TABLE: OnceLock<[f32; TABLE_SIZE]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [0.0f32; TABLE_SIZE];
        for (i, slot) in t.iter_mut().enumerate() {
            let angle_units = (i as f64) * (65536.0 / TABLE_SIZE as f64);
            let radians = angle_units * (core::f64::consts::TAU / 65536.0);
            *slot = radians.sin() as f32;
        }
        t
    })
}

/// Sine of an angle, via table lookup with linear interpolation.
pub fn sin(angle: Angle) -> f32 {
    let table = sin_table();
    let bits = (angle.0 as u16) as u32;
    let idx = bits >> (16 - TABLE_BITS);
    let frac = (bits & ((1 << (16 - TABLE_BITS)) - 1)) as f32 / (1 << (16 - TABLE_BITS)) as f32;
    let a = table[idx as usize];
    let b = table[((idx + 1) % TABLE_SIZE as u32) as usize];
    a + (b - a) * frac
}

/// Cosine of an angle, via the sine table shifted by a quarter turn.
pub fn cos(angle: Angle) -> f32 {
    sin(angle.wrapping_add(Angle::QUARTER_TURN))
}

/// Arctangent returning an [`Angle`]: the yaw whose sine/cosine ratio is
/// `y / x`, in the full 16-bit circle.
pub fn atan2(y: f32, x: f32) -> Angle {
    if x == 0.0 && y == 0.0 {
        return Angle::ZERO;
    }
    // f64 math is allowed here: this is a conversion helper, not the
    // per-frame hot path, and both platforms share Rust's portable float math.
    let radians = (y as f64).atan2(x as f64);
    let units = (radians * (65536.0 / core::f64::consts::TAU)).round() as i32;
    Angle(units as i16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cardinal_values() {
        assert!((sin(Angle::ZERO) - 0.0).abs() < 1e-4);
        assert!((sin(Angle::QUARTER_TURN) - 1.0).abs() < 1e-4);
        assert!((sin(Angle::HALF_TURN) - 0.0).abs() < 1e-4);
        assert!((cos(Angle::ZERO) - 1.0).abs() < 1e-4);
        assert!((cos(Angle::QUARTER_TURN) - 0.0).abs() < 1e-4);
    }

    #[test]
    fn matches_std_within_table_error() {
        // 4096 entries + linear interpolation: worst-case error well under 1e-6.
        let mut worst = 0.0f32;
        for i in (0..65536).step_by(7) {
            let a = Angle(i as i16);
            let expected = ((i as f64) * core::f64::consts::TAU / 65536.0).sin() as f32;
            worst = worst.max((sin(a) - expected).abs());
        }
        assert!(worst < 1e-5, "worst error {worst}");
    }

    #[test]
    fn atan2_quadrants() {
        assert_eq!(atan2(0.0, 1.0), Angle::ZERO);
        assert_eq!(atan2(1.0, 0.0), Angle::QUARTER_TURN);
        assert_eq!(atan2(0.0, -1.0), Angle::HALF_TURN);
        assert_eq!(atan2(-1.0, 0.0), Angle(-16384));
    }
}
