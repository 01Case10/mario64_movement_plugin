//! Animation state, ported from the decomp (plan §4.5 AN-01).
//!
//! In the decomp the animation frame is *gameplay state*: actions branch on
//! `is_anim_at_end` / `is_anim_past_end` / `is_anim_past_frame`, and the
//! frame advances once per rendered frame in `geo_set_animation_globals`
//! (`src/game/rendering_graph_node.c:608`), *after* `execute_mario_action`
//! — so during a tick, actions observe the previous tick's advance.
//!
//! The plugin mirrors this: [`AnimState`] lives on [`CharacterState`],
//! actions call [`set_mario_animation`] / [`set_mario_anim_with_accel`],
//! and [`advance_animation`] runs once per [`tick`](crate::step::tick),
//! after the action loop, before the trace row is emitted (exactly like
//! the oracle harness `oracle/harness.c`).
//!
//! Conventions (decomp: `src/game/mario.c:46-175`,
//! `src/engine/graph_node.c:792`, built with `-DAVOID_UB=1`):
//! - `frame` is `s16` in the decomp; stored sign-extended in an `i32` here.
//! - `GET_HIGH_S16_OF_32(v)` = `(v >> 16) as i16` (arithmetic shift, then
//!   truncate); `SET_HIGH_S16_OF_32(v, x)` = `(v & 0xFFFF) | ((x as i32) << 16)`.
//! - `load_patchable_table` (oracle stub) clamps out-of-range ids to 0 and
//!   records the clamped id (`gOracleLastAnimId`, the CSV's `anim_id`);
//!   `animInfo.animID` keeps the *unclamped* requested id for the `!=`
//!   comparison. Both are modeled.

use crate::anim_table::{
    anim_header, ANIM_FLAG_2, ANIM_FLAG_BACKWARD, ANIM_FLAG_NOLOOP,
};
use crate::state::CharacterState;

/// Per-character animation state.
/// decomp: `struct AnimInfo` (`m->marioObj->header.gfx.animInfo`,
/// `include/types.h`) plus the oracle's `gOracleLastAnimId` bookkeeping.
#[derive(Clone, Copy, Debug)]
pub struct AnimState {
    /// Last requested animation id, clamped to the table (id 0 on
    /// out-of-range). The CSV's `anim_id`.
    /// oracle: `gOracleLastAnimId` (set by `load_patchable_table`).
    /// `-1` until the first [`set_mario_animation`].
    pub id: i32,
    /// Id the frame state was (re)initialized for — the `!=` comparison
    /// target. decomp: `animInfo.animID` (zero-initialized).
    pub cur_id: i32,
    /// Current frame, sign-extended. decomp: `animInfo.animFrame` (`s16`).
    pub frame: i32,
    /// Frame acceleration. decomp: `animInfo.animAccel` (`s32`).
    pub accel: i32,
    /// 16.16 fixed-point frame accumulator.
    /// decomp: `animInfo.animFrameAccelAssist` (`s32`).
    pub frame_accel_assist: i32,
    /// Last area-update counter this object advanced on.
    /// decomp: `animInfo.animTimer` (`u16`).
    pub timer: u16,
}

impl Default for AnimState {
    fn default() -> Self {
        Self {
            id: -1,
            cur_id: 0,
            frame: 0,
            accel: 0,
            frame_accel_assist: 0,
            timer: 0,
        }
    }
}

/// Set Mario's animation without acceleration, running at its default rate.
/// Returns the (possibly reset) frame.
/// decomp: `src/game/mario.c:64` (`set_mario_animation`).
pub fn set_mario_animation(state: &mut CharacterState, target_anim_id: i32) -> i16 {
    // `load_patchable_table` (oracle stub): clamp, record the clamped id,
    // return 0 so the pointer-patching branch is skipped.
    let clamped = clamp_anim_id(target_anim_id);
    state.anim.id = clamped;
    if state.anim.cur_id != target_anim_id {
        state.anim.cur_id = target_anim_id;
        let hdr = anim_header(clamped);
        state.anim.accel = 0;
        // `animYTrans` is renderer-only; skipped.
        state.anim.frame = if hdr.flags & ANIM_FLAG_2 != 0 {
            hdr.start_frame as i32
        } else if hdr.flags & ANIM_FLAG_BACKWARD != 0 {
            hdr.start_frame as i32 + 1
        } else {
            hdr.start_frame as i32 - 1
        };
    }
    state.anim.frame as i16
}

/// Set Mario's animation with frame acceleration.
/// Returns the (possibly reset) frame.
/// decomp: `src/game/mario.c:97` (`set_mario_anim_with_accel`).
pub fn set_mario_anim_with_accel(
    state: &mut CharacterState,
    target_anim_id: i32,
    accel: i32,
) -> i16 {
    let clamped = clamp_anim_id(target_anim_id);
    state.anim.id = clamped;
    if state.anim.cur_id != target_anim_id {
        state.anim.cur_id = target_anim_id;
        let hdr = anim_header(clamped);
        let assist = if hdr.flags & ANIM_FLAG_2 != 0 {
            (hdr.start_frame as i32) << 16
        } else if hdr.flags & ANIM_FLAG_BACKWARD != 0 {
            ((hdr.start_frame as i32) << 16).wrapping_add(accel)
        } else {
            ((hdr.start_frame as i32) << 16).wrapping_sub(accel)
        };
        state.anim.frame_accel_assist = assist;
        // `(s16)(assist >> 16)`: arithmetic shift, truncate, sign-extend.
        state.anim.frame = ((assist >> 16) as i16) as i32;
    }
    state.anim.accel = accel;
    state.anim.frame as i16
}

/// Set the animation to a specific "next" frame from the frame given.
/// decomp: `src/game/mario.c:132` (`set_anim_to_frame`).
pub fn set_anim_to_frame(state: &mut CharacterState, anim_frame: i16) {
    let hdr = anim_header(state.anim.id);
    if state.anim.accel != 0 {
        if hdr.flags & ANIM_FLAG_BACKWARD != 0 {
            state.anim.frame_accel_assist =
                ((anim_frame as i32) << 16).wrapping_add(state.anim.accel);
        } else {
            state.anim.frame_accel_assist =
                ((anim_frame as i32) << 16).wrapping_sub(state.anim.accel);
        }
    } else if hdr.flags & ANIM_FLAG_BACKWARD != 0 {
        state.anim.frame = anim_frame as i32 + 1;
    } else {
        state.anim.frame = anim_frame as i32 - 1;
    }
}

/// True when the animation is on its last frame.
/// decomp: `src/game/mario.c:46` (`is_anim_at_end`).
pub fn is_anim_at_end(state: &CharacterState) -> bool {
    let hdr = anim_header(state.anim.id);
    state.anim.frame + 1 == hdr.loop_end as i32
}

/// True when the animation reached (or passed) 2 frames before its end.
/// decomp: `src/game/mario.c:55` (`is_anim_past_end`).
pub fn is_anim_past_end(state: &CharacterState) -> bool {
    let hdr = anim_header(state.anim.id);
    state.anim.frame >= hdr.loop_end as i32 - 2
}

/// True when the animation just passed `anim_frame` (within one accel step).
/// decomp: `src/game/mario.c:151` (`is_anim_past_frame`).
pub fn is_anim_past_frame(state: &CharacterState, anim_frame: i16) -> bool {
    let hdr = anim_header(state.anim.id);
    let accelerated_frame = (anim_frame as i32) << 16;
    if state.anim.accel != 0 {
        if hdr.flags & ANIM_FLAG_BACKWARD != 0 {
            state.anim.frame_accel_assist > accelerated_frame
                && accelerated_frame
                    >= state
                        .anim
                        .frame_accel_assist
                        .wrapping_sub(state.anim.accel)
        } else {
            state.anim.frame_accel_assist < accelerated_frame
                && accelerated_frame
                    <= state
                        .anim
                        .frame_accel_assist
                        .wrapping_add(state.anim.accel)
        }
    } else if hdr.flags & ANIM_FLAG_BACKWARD != 0 {
        state.anim.frame == anim_frame as i32 + 1
    } else {
        state.anim.frame + 1 == anim_frame as i32
    }
}

/// Advance the animation frame once, exactly like the decomp's per-render
/// call (`geo_set_animation_globals` → `geo_update_animation_frame`,
/// `src/game/rendering_graph_node.c:608`, `src/engine/graph_node.c:792`).
///
/// Called once per tick, *after* the action loop: actions observe the
/// previous tick's advance. The oracle harness (`oracle/harness.c`) does
/// the same ordering (action → advance → CSV row).
pub fn advance_animation(state: &mut CharacterState) {
    // The harness guards on `curAnim != NULL`: no advance until the first
    // `set_mario_animation` (anim.id == -1).
    if state.anim.id != -1 {
        let new_frame = geo_update_animation_frame(state);
        state.anim.frame = new_frame as i32;
    }
    state.anim.timer = state.area_update_counter;
    state.area_update_counter = state.area_update_counter.wrapping_add(1);
}

/// One animation-frame update.
/// decomp: `src/engine/graph_node.c:792` (`geo_update_animation_frame`),
/// built with `-DAVOID_UB=1`.
fn geo_update_animation_frame(state: &mut CharacterState) -> i16 {
    let hdr = anim_header(state.anim.id);
    // Same-frame re-entry guard (only true on the very first tick, when
    // both counters are still 0) and ANIM_FLAG_2 (never advances).
    if state.anim.timer == state.area_update_counter || hdr.flags & ANIM_FLAG_2 != 0 {
        return state.anim.frame as i16;
    }
    let mut result: i32;
    if hdr.flags & ANIM_FLAG_BACKWARD != 0 {
        result = if state.anim.accel != 0 {
            state
                .anim
                .frame_accel_assist
                .wrapping_sub(state.anim.accel)
        } else {
            (state.anim.frame - 1) << 16
        };
        // GET_HIGH_S16_OF_32(result) = (s16)(result >> 16).
        if ((result >> 16) as i16) < hdr.loop_start {
            result = if hdr.flags & ANIM_FLAG_NOLOOP != 0 {
                set_high_s16(result, hdr.loop_start)
            } else {
                set_high_s16(result, hdr.loop_end - 1)
            };
        }
    } else {
        result = if state.anim.accel != 0 {
            state
                .anim
                .frame_accel_assist
                .wrapping_add(state.anim.accel)
        } else {
            (state.anim.frame + 1) << 16
        };
        if ((result >> 16) as i16) >= hdr.loop_end {
            result = if hdr.flags & ANIM_FLAG_NOLOOP != 0 {
                set_high_s16(result, hdr.loop_end - 1)
            } else {
                set_high_s16(result, hdr.loop_start)
            };
        }
    }
    state.anim.frame_accel_assist = result;
    (result >> 16) as i16
}

/// `SET_HIGH_S16_OF_32(result, x)`: `result = (result & 0xFFFF) | ((x as i32) << 16)`.
/// decomp: `include/sm64.h:32` (the `-DAVOID_UB=1` branch).
fn set_high_s16(result: i32, x: i16) -> i32 {
    (result & 0xFFFF) | ((x as i32) << 16)
}

/// `load_patchable_table`'s id policy (oracle stub `oracle/stubs.c:128`):
/// out-of-range ids (including negative) fall back to id 0.
fn clamp_anim_id(id: i32) -> i32 {
    if id < 0 || (id as usize) >= crate::anim_table::ANIM_COUNT {
        0
    } else {
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anim_table::MARIO_ANIM_IDLE_HEAD_LEFT;

    fn fresh() -> CharacterState {
        CharacterState::default()
    }

    #[test]
    fn set_and_advance_idle_head_turn() {
        // MARIO_ANIM_IDLE_HEAD_LEFT (0xC3): flags=1 (NOLOOP), start 0,
        // loopEnd 60-ish. First request resets frame to startFrame - 1.
        let mut s = fresh();
        let f = set_mario_animation(&mut s, MARIO_ANIM_IDLE_HEAD_LEFT);
        assert_eq!(s.anim.id, MARIO_ANIM_IDLE_HEAD_LEFT);
        assert_eq!(f, -1);
        assert_eq!(s.anim.frame, -1);
        // Frame 0's advance is skipped (animTimer == areaUpdateCounter == 0),
        // mirroring the oracle harness.
        advance_animation(&mut s);
        assert_eq!(s.anim.frame, -1);
        // Frame 1 advances to 0.
        advance_animation(&mut s);
        assert_eq!(s.anim.frame, 0);
    }

    #[test]
    fn accel_anim_uses_assist() {
        let mut s = fresh();
        // MARIO_ANIM_WALKING with accel: assist = (start<<16) - accel.
        let f = set_mario_anim_with_accel(&mut s, crate::anim_table::MARIO_ANIM_WALKING, 0x10000);
        assert_eq!(s.anim.accel, 0x10000);
        let _ = f;
        // Re-requesting with a new accel does not reset the frame.
        let before = s.anim.frame;
        set_mario_anim_with_accel(&mut s, crate::anim_table::MARIO_ANIM_WALKING, 0x20000);
        assert_eq!(s.anim.frame, before);
        assert_eq!(s.anim.accel, 0x20000);
    }

    #[test]
    fn anim_table_matches_decomp_source() {
        // Re-parse one decomp header when the checkout is present and
        // compare against the checked-in table (catches stale regeneration).
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../oracle/sm64/assets/anims/anim_00.inc.c");
        if !path.is_file() {
            return;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let brace = text.find('{').unwrap();
        let after = &text[brace + 1..];
        // Scan the first 5 integer literals (decimal or 0x hex, signed).
        let mut nums: Vec<i16> = Vec::new();
        let bytes = after.as_bytes();
        let mut i = 0;
        while i < bytes.len() && nums.len() < 5 {
            let c = bytes[i] as char;
            if c == '-' || c.is_ascii_digit() {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    let d = bytes[i] as char;
                    if d.is_ascii_hexdigit() || d == 'x' || d == 'X' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                let tok = &after[start..i];
                let v = if let Some(h) = tok
                    .strip_prefix("-0x")
                    .or_else(|| tok.strip_prefix("-0X"))
                {
                    -i16::from_str_radix(h, 16).unwrap()
                } else if let Some(h) = tok
                    .strip_prefix("0x")
                    .or_else(|| tok.strip_prefix("0X"))
                {
                    i16::from_str_radix(h, 16).unwrap()
                } else {
                    tok.parse().unwrap()
                };
                nums.push(v);
            } else {
                i += 1;
            }
        }
        assert_eq!(nums.len(), 5);
        let h = crate::anim_table::ANIM_TABLE[0];
        assert_eq!(
            (h.flags, h.y_trans_divisor, h.start_frame, h.loop_start, h.loop_end),
            (nums[0], nums[1], nums[2], nums[3], nums[4]),
            "anim_table.rs is stale: re-run tools/gen_anim_table.py"
        );
    }
}
