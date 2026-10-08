//! 16-bit wrapping angles: 65_536 units per full turn.
//!
//! Wraparound and truncation are observable behavior, so angles stay in this
//! integer domain and are never converted to radians inside the simulation.

use core::fmt;

/// An angle where 65_536 units make one full turn. Wraps on overflow, exactly
/// like the 16-bit angle arithmetic of the reference hardware.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Angle(pub i16);

impl Angle {
    /// 0 degrees.
    pub const ZERO: Angle = Angle(0);
    /// 90 degrees (16_384 units).
    pub const QUARTER_TURN: Angle = Angle(16384);
    /// 180 degrees (32_768 units, represented as -32_768).
    pub const HALF_TURN: Angle = Angle(-32768);

    /// One full turn in angle units.
    pub const FULL_TURN_UNITS: i32 = 65536;

    /// Add, wrapping around the 16-bit circle.
    #[inline]
    pub fn wrapping_add(self, rhs: Angle) -> Angle {
        Angle(self.0.wrapping_add(rhs.0))
    }

    /// Subtract, wrapping around the 16-bit circle.
    #[inline]
    pub fn wrapping_sub(self, rhs: Angle) -> Angle {
        Angle(self.0.wrapping_sub(rhs.0))
    }

    /// Shortest signed difference from `self` to `other`, in angle units,
    /// in the range [-32768, 32767].
    #[inline]
    pub fn diff_to(self, other: Angle) -> i32 {
        (other.0.wrapping_sub(self.0)) as i32
    }

    /// Move from `self` toward `target` by at most `step` units along the
    /// shortest arc. Never overshoots.
    pub fn approach(self, target: Angle, step: u16) -> Angle {
        let diff = self.diff_to(target);
        if diff == 0 {
            return target;
        }
        let step = step as i32;
        let clamped = diff.clamp(-step, step);
        Angle(self.0.wrapping_add(clamped as i16))
    }

    /// Build from degrees, rounding to the nearest angle unit.
    pub fn from_degrees(deg: f32) -> Angle {
        let units = (deg * (65536.0 / 360.0)).round() as i32;
        Angle(units as i16)
    }

    /// Convert to degrees in (-180, 180].
    pub fn to_degrees(self) -> f32 {
        (self.0 as f32) * (360.0 / 65536.0)
    }
}

impl fmt::Debug for Angle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Angle({:.2}°)", self.to_degrees())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_around_full_turn() {
        let a = Angle(32767).wrapping_add(Angle(1));
        assert_eq!(a, Angle(-32768));
    }

    #[test]
    fn shortest_diff_picks_arc() {
        assert_eq!(Angle::ZERO.diff_to(Angle(16384)), 16384);
        assert_eq!(Angle(16384).diff_to(Angle::ZERO), -16384);
        // 200° vs -160°: shortest path is -160° -> 200° = -40°? check both ways
        let a = Angle::from_degrees(170.0);
        let b = Angle::from_degrees(-170.0);
        assert_eq!(a.diff_to(b), 3640); // +20° the short way
        assert_eq!(b.diff_to(a), -3640);
    }

    #[test]
    fn approach_never_overshoots() {
        let start = Angle::ZERO;
        let target = Angle::from_degrees(90.0);
        let reached = start.approach(target, 20000);
        assert_eq!(reached, target);
        let partial = start.approach(target, 1000);
        assert_eq!(partial.0, 1000);
    }

    #[test]
    fn degrees_round_trip() {
        for deg in [-180.0, -90.0, -45.0, 0.0, 45.0, 90.0, 135.0, 179.0] {
            let a = Angle::from_degrees(deg);
            let back = a.to_degrees();
            assert!((back - deg).abs() < 0.01, "{deg} -> {a:?} -> {back}");
        }
    }
}
