//! Numeric helpers ported from the decomp (`src/engine/math_util.c`).
//!
//! Plan §5 rule 2 (mirror C's arithmetic conversions) and rule 3
//! (float→int truncation, s16 wrap) are enforced here: these are the only
//! sanctioned int/float conversions in simulation code.
//!
//! Named after the decomp functions; each carries its `// decomp:` line so
//! later reviews can diff them (plan §8.6).

use crate::angles::Angle;

/// Return the value `current` after it tries to approach `target`, going up
/// at most `inc` and going down at most `dec`.
/// decomp: src/engine/math_util.c:657
///
/// //! If target is close to the max or min s32, then it's possible to
/// overflow past it without stopping. (Wrapping arithmetic reproduces the
/// x86 result deterministically; realistic inputs never approach the
/// limits — plan §5 rule 8.)
#[inline]
pub fn approach_s32(current: i32, target: i32, inc: i32, dec: i32) -> i32 {
    let mut current = current;
    if current < target {
        current = current.wrapping_add(inc);
        if current > target {
            current = target;
        }
    } else {
        current = current.wrapping_sub(dec);
        if current < target {
            current = target;
        }
    }
    current
}

/// Return the value `current` after it tries to approach `target`, going up
/// at most `inc` and going down at most `dec`.
/// decomp: src/engine/math_util.c:679
#[inline]
pub fn approach_f32(current: f32, target: f32, inc: f32, dec: f32) -> f32 {
    let mut current = current;
    if current < target {
        current += inc;
        if current > target {
            current = target;
        }
    } else {
        current -= dec;
        if current < target {
            current = target;
        }
    }
    current
}

/// The decomp's walking-turn idiom:
/// `faceAngle = intendedYaw - approach_s32((s16)(intendedYaw - faceAngle),
/// 0, inc, dec)` (decomp: src/game/mario_actions_moving.c:434-465).
/// Moves `current` toward `target` along the shortest arc by at most
/// `inc`/`dec` units, never overshooting.
#[inline]
pub fn approach_angle(current: Angle, target: Angle, inc: i32, dec: i32) -> Angle {
    // (s16)(intendedYaw - faceAngle): the subtraction wraps in i16.
    let diff = target.0.wrapping_sub(current.0) as i32;
    let diff = approach_s32(diff, 0, inc, dec);
    Angle(target.0.wrapping_sub(diff as i16))
}

/// C `s16 += f32` semantics: `(s16)((f32)s16 + f32)` — float addition, then
/// truncation toward zero, then wrap into 16 bits.
/// decomp pattern, e.g. `m->faceAngle[1] += 512.0f * sins(dYaw) * intendedMag`.
/// Plan §5 rule 3. (Rust `as` saturates on out-of-range floats where C is
/// UB; realistic gameplay values never leave i16 range — rule 8.)
#[inline]
pub fn s16_add_f32(a: i16, b: f32) -> i16 {
    (a as f32 + b) as i16
}

/// C `(s16)` float→int conversion: truncation toward zero.
/// Plan §5 rule 3.
#[inline]
pub fn f32_to_s16_trunc(x: f32) -> i16 {
    x as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approach_s32_moves_both_ways() {
        assert_eq!(approach_s32(0, 10, 3, 3), 3);
        assert_eq!(approach_s32(9, 10, 3, 3), 10); // clamped, no overshoot
        assert_eq!(approach_s32(10, 0, 3, 4), 6);
        assert_eq!(approach_s32(2, 0, 3, 4), 0);
        assert_eq!(approach_s32(5, 5, 3, 3), 5);
    }

    #[test]
    fn approach_f32_matches_old_clamp_behavior() {
        assert_eq!(approach_f32(0.0, 10.0, 3.0, 3.0), 3.0);
        assert_eq!(approach_f32(9.0, 10.0, 3.0, 3.0), 10.0);
        assert_eq!(approach_f32(10.0, 0.0, 3.0, 4.0), 6.0);
    }

    #[test]
    fn approach_angle_turns_shortest_arc() {
        let a = approach_angle(Angle::ZERO, Angle::QUARTER_TURN, 0x800, 0x800);
        assert_eq!(a, Angle(0x800));
        // Overshoot clamps at the target.
        let b = approach_angle(Angle(0x3C00), Angle::QUARTER_TURN, 0x800, 0x800);
        assert_eq!(b, Angle::QUARTER_TURN);
        // Negative direction.
        let c = approach_angle(Angle::ZERO, Angle(-0x4000), 0x800, 0x800);
        assert_eq!(c, Angle(-0x800));
    }

    #[test]
    fn s16_add_f32_truncates() {
        assert_eq!(s16_add_f32(100, 0.9), 100); // 100.9 -> 100
        assert_eq!(s16_add_f32(100, -0.9), 99); // 99.1 -> 99
        assert_eq!(s16_add_f32(-100, -0.9), -100); // -100.9 -> -100
    }
}
