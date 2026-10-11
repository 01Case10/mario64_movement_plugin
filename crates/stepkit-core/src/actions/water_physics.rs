//! Verbatim ports of the decomp's water-physics helpers from
//! `src/game/mario_actions_submerged.c` (lines 19-303): the surface/buoyancy
//! helpers, the water step (collision), the water-current application, the
//! gfx pitch updater, the stationary slowdown, and the swimming
//! speed/yaw/pitch updaters shared by every swim action.
//!
//! Conventions: every ported item cites its decomp line; `m->input & INPUT_*`
//! reads become `cx.state.input & input_flags::*`; stick reads use the
//! conditioned `cx.proc.stick_x`/`stick_y` (`m->controller->stickX/stickY`);
//! `m->faceAngle[0]/[1]/[2]` map to `face_pitch`/`face_yaw`/`face_roll` and
//! `m->angleVel[0]/[1]` to `angle_vel[0]/[1]`.
//!
//! NOTE: this module is not yet declared in `actions.rs` (existing files may
//! not be modified); it assumes `pub mod water_physics;` there, and
//! `water_swim.rs` reaches the shared file-statics through it.

use super::ActionCx;
use crate::angles::Angle;
use crate::math_util::{approach_f32, approach_s32};
use crate::state::act_flags;
use crate::trig;
use crate::world::{CollisionWorld, CELL_HEIGHT_LIMIT};
use glam::Vec3;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};

/// decomp: `#define MIN_SWIM_STRENGTH 160` (mario_actions_submerged.c:19).
pub(crate) const MIN_SWIM_STRENGTH: u16 = 160;
/// decomp: `#define MIN_SWIM_SPEED 16.0f` (mario_actions_submerged.c:20).
pub(crate) const MIN_SWIM_SPEED: f32 = 16.0;

/// decomp: `MARIO_METAL_CAP` (`oracle/sm64/include/sm64.h:117`). Not (yet) in
/// `crate::state::mario_flags`, so it is defined here for the metal-cap
/// checks in the buoyancy and swim actions.
pub const MARIO_METAL_CAP: u32 = 0x00000004;

/// decomp: `SURFACE_FLOWING_WATER` (`oracle/sm64/include/surface_terrains.h:13`).
const SURFACE_FLOWING_WATER: u16 = 0x000E;

/// decomp: `static s16 sWaterCurrentSpeeds[] = { 28, 12, 8, 4 };`
/// (mario_actions_submerged.c:24).
const S_WATER_CURRENT_SPEEDS: [f32; 4] = [28.0, 12.0, 8.0, 4.0];

// decomp: `static s16 sWasAtSurface` (mario_actions_submerged.c:27).
// `pub(crate)` so `water_swim.rs` can share the decomp's file-static state.
pub(crate) static S_WAS_AT_SURFACE: AtomicBool = AtomicBool::new(false);
// decomp: `static s16 sBobTimer` / `static s16 sBobIncrement`
// (mario_actions_submerged.c:26-27).
pub(crate) static S_BOB_TIMER: AtomicI32 = AtomicI32::new(0);
pub(crate) static S_BOB_INCREMENT: AtomicI32 = AtomicI32::new(0);
// decomp: `static f32 sBobHeight` (mario_actions_submerged.c:28), kept as bits.
pub(crate) static S_BOB_HEIGHT_BITS: AtomicU32 = AtomicU32::new(0.0f32.to_bits());
// decomp: `static s16 sSwimStrength = MIN_SWIM_STRENGTH` (c:23) — the core
// already models this per-character as `CharacterState::swim_strength`, so
// no static is kept here; ports read/write `cx.state.swim_strength`.

/// decomp: `WATER_STEP_*` (`oracle/sm64/include/sm64.h:85-89`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WaterStep {
    /// `WATER_STEP_NONE` (0).
    None,
    /// `WATER_STEP_HIT_FLOOR` (1).
    HitFloor,
    /// `WATER_STEP_HIT_CEILING` (2).
    HitCeiling,
    /// `WATER_STEP_CANCELLED` (3).
    Cancelled,
    /// `WATER_STEP_HIT_WALL` (4).
    HitWall,
}

/// decomp: src/game/mario_actions_submerged.c:31
/// (`set_swimming_at_surface_particles`).
///
/// The decomp's `particleFlag` parameter only feeds
/// `m->particleFlags |= particleFlag`; the core has no particle flags, so
/// the parameter is dropped and the particle write is skipped (no gameplay
/// effect). The `sWasAtSurface` edge latch is kept.
pub fn set_swimming_at_surface_particles(cx: &mut ActionCx) {
    let at_surface = cx.state.pos.y >= cx.state.water_level - 130.0;

    if at_surface {
        // m->particleFlags |= particleFlag — no particle flags in the core; skipped.
        if at_surface ^ S_WAS_AT_SURFACE.load(Ordering::Relaxed) {
            // play_sound(SOUND_ACTION_UNKNOWN431, ...): plain `play_sound`
            // sets no MARIO_*_SOUND_PLAYED flag — pure audio, skipped.
        }
    }

    S_WAS_AT_SURFACE.store(at_surface, Ordering::Relaxed);
}

/// decomp: src/game/mario_actions_submerged.c:44 (`swimming_near_surface`).
pub fn swimming_near_surface(cx: &ActionCx) -> bool {
    if cx.state.flags & MARIO_METAL_CAP != 0 {
        return false;
    }

    (cx.state.water_level - 80.0) - cx.state.pos.y < 400.0
}

/// decomp: src/game/mario_actions_submerged.c:52 (`get_buoyancy`).
pub fn get_buoyancy(cx: &ActionCx) -> f32 {
    let mut buoyancy = 0.0;

    if cx.state.flags & MARIO_METAL_CAP != 0 {
        if cx.state.action.0 & act_flags::INVULNERABLE != 0 {
            buoyancy = -2.0;
        } else {
            buoyancy = -18.0;
        }
    } else if swimming_near_surface(cx) {
        buoyancy = 1.25;
    } else if cx.state.action.0 & act_flags::MOVING == 0 {
        buoyancy = -2.0;
    }

    buoyancy
}

/// decomp: src/game/mario_actions_submerged.c:70 (`perform_water_full_step`).
pub fn perform_water_full_step(cx: &mut ActionCx, next_pos: Vec3) -> WaterStep {
    // decomp: `wall = resolve_and_return_wall_collisions(nextPos, 10.0f, 110.0f)`
    // — the call pushes `nextPos` out of walls in place.
    let walls = cx
        .world
        .find_wall_collisions(next_pos.x, next_pos.y, next_pos.z, 10.0, 110.0);
    let wall = walls.wall;
    let next_pos = walls.pos;

    // decomp: `floorHeight = find_floor(nextPos[0], nextPos[1], nextPos[2], &floor)`.
    let Some(floor) = cx.world.find_floor(next_pos.x, next_pos.y, next_pos.z) else {
        return WaterStep::Cancelled;
    };
    let floor_height = floor.y;

    // decomp: `ceilHeight = vec3f_find_ceil(nextPos, floorHeight, &ceil)`
    // = `find_ceil(x, floorHeight + 80.0f, z)` (mario.c:547-551);
    // CELL_HEIGHT_LIMIT when no ceiling is found.
    let ceil_height = cx
        .world
        .find_ceil(next_pos.x, floor_height + 80.0, next_pos.z)
        .map(|c| c.y)
        .unwrap_or(CELL_HEIGHT_LIMIT);

    if next_pos.y >= floor_height {
        if ceil_height - next_pos.y >= 160.0 {
            cx.state.pos = next_pos;
            cx.state.floor = Some(floor);
            cx.state.floor_y = Some(floor_height);

            if wall.is_some() {
                return WaterStep::HitWall;
            } else {
                return WaterStep::None;
            }
        }

        if ceil_height - floor_height < 160.0 {
            return WaterStep::Cancelled;
        }

        // Water ceiling downwarp (decomp comment, verbatim).
        cx.state.pos = Vec3::new(next_pos.x, ceil_height - 160.0, next_pos.z);
        cx.state.floor = Some(floor);
        cx.state.floor_y = Some(floor_height);
        WaterStep::HitCeiling
    } else {
        if ceil_height - floor_height < 160.0 {
            return WaterStep::Cancelled;
        }

        cx.state.pos = Vec3::new(next_pos.x, floor_height, next_pos.z);
        cx.state.floor = Some(floor);
        cx.state.floor_y = Some(floor_height);
        WaterStep::HitFloor
    }
}

/// decomp: src/game/mario_actions_submerged.c:119 (`apply_water_current`).
///
/// The decomp mutates the caller's `Vec3f step` in place; the port takes
/// `step` by value and returns the modified vector instead.
pub fn apply_water_current(cx: &ActionCx, mut step: Vec3) -> Vec3 {
    // decomp: `if (m->floor->type == SURFACE_FLOWING_WATER)` — the decomp
    // dereferences m->floor unconditionally; a missing floor is treated as
    // non-flowing here.
    if let Some(floor) = cx.state.floor {
        if floor.surface_type == SURFACE_FLOWING_WATER {
            // decomp: `s16 currentAngle = m->floor->force << 8;`
            // (C promotes to int, then truncates back to s16).
            let current_angle = Angle(floor.force.wrapping_shl(8));
            // decomp: `f32 currentSpeed = sWaterCurrentSpeeds[m->floor->force >> 8];`
            // — no bounds check in the decomp; the index is trusted here too.
            let current_speed = S_WATER_CURRENT_SPEEDS[(floor.force >> 8) as usize];

            step.x += current_speed * trig::sins(current_angle);
            step.z += current_speed * trig::coss(current_angle);
        }
    }

    // Whirlpools (`gCurrentArea->whirlpools[2]`, decomp lines 132-162,
    // including the DDD-area-2 radius override): area whirlpool data is not
    // exposed by `CollisionWorld`, so the loop has no core equivalent and is
    // skipped — noted, not re-derived.

    step
}

/// decomp: src/game/mario_actions_submerged.c:167 (`perform_water_step`).
pub fn perform_water_step(cx: &mut ActionCx) -> WaterStep {
    let mut step = cx.state.vel;

    if cx.state.action.0 & act_flags::SWIMMING != 0 {
        step = apply_water_current(cx, step);
    }

    let mut next_pos = cx.state.pos + step;

    if next_pos.y > cx.state.water_level - 80.0 {
        next_pos.y = cx.state.water_level - 80.0;
        cx.state.vel.y = 0.0;
    }

    let step_result = perform_water_full_step(cx, next_pos);

    // `vec3f_copy(marioObj->header.gfx.pos, m->pos)` and the
    // `vec3s_set(marioObj->header.gfx.angle, ...)` — the render mirror; no
    // core equivalent, skipped.

    step_result
}

/// decomp: src/game/mario_actions_submerged.c:197 (`update_water_pitch`).
///
/// NOTE: every statement in the decomp body reads/writes only
/// `m->marioObj->header.gfx` (the render mirror: `gfx.angle[0]`,
/// `gfx.pos[1]`). It never touches simulation state, so the verbatim port
/// is a no-op in the core. The body is quoted here for reference:
///
/// ```c
/// if (marioObj->header.gfx.angle[0] > 0) {
///     marioObj->header.gfx.pos[1] +=
///         60.0f * sins(marioObj->header.gfx.angle[0]) * sins(marioObj->header.gfx.angle[0]);
/// }
/// if (marioObj->header.gfx.angle[0] < 0) {
///     marioObj->header.gfx.angle[0] = marioObj->header.gfx.angle[0] * 6 / 10;
/// }
/// if (marioObj->header.gfx.angle[0] > 0) {
///     marioObj->header.gfx.angle[0] = marioObj->header.gfx.angle[0] * 10 / 8;
/// }
/// ```
pub fn update_water_pitch(_cx: &mut ActionCx) {
    // gfx-only; no core equivalent.
}

/// decomp: src/game/mario_actions_submerged.c:214 (`stationary_slow_down`).
pub fn stationary_slow_down(cx: &mut ActionCx) {
    let buoyancy = get_buoyancy(cx);

    cx.state.angle_vel[0] = Angle(0);
    cx.state.angle_vel[1] = Angle(0);

    cx.state.forward_speed = approach_f32(cx.state.forward_speed, 0.0, 1.0, 1.0);
    cx.state.vel.y = approach_f32(cx.state.vel.y, buoyancy, 2.0, 1.0);

    cx.state.face_pitch = Angle(approach_s32(cx.state.face_pitch.0 as i32, 0, 0x200, 0x200) as i16);
    cx.state.face_roll = Angle(approach_s32(cx.state.face_roll.0 as i32, 0, 0x100, 0x100) as i16);

    cx.state.vel.x =
        cx.state.forward_speed * trig::coss(cx.state.face_pitch) * trig::sins(cx.state.face_yaw);
    cx.state.vel.z =
        cx.state.forward_speed * trig::coss(cx.state.face_pitch) * trig::coss(cx.state.face_yaw);
}

/// decomp: src/game/mario_actions_submerged.c:230 (`update_swimming_speed`).
pub fn update_swimming_speed(cx: &mut ActionCx, decel_threshold: f32) {
    let buoyancy = get_buoyancy(cx);
    let max_speed = 28.0;

    if cx.state.action.0 & act_flags::STATIONARY != 0 {
        cx.state.forward_speed -= 2.0;
    }

    if cx.state.forward_speed < 0.0 {
        cx.state.forward_speed = 0.0;
    }

    if cx.state.forward_speed > max_speed {
        cx.state.forward_speed = max_speed;
    }

    if cx.state.forward_speed > decel_threshold {
        cx.state.forward_speed -= 0.5;
    }

    cx.state.vel.x =
        cx.state.forward_speed * trig::coss(cx.state.face_pitch) * trig::sins(cx.state.face_yaw);
    cx.state.vel.y = cx.state.forward_speed * trig::sins(cx.state.face_pitch) + buoyancy;
    cx.state.vel.z =
        cx.state.forward_speed * trig::coss(cx.state.face_pitch) * trig::coss(cx.state.face_yaw);
}

/// decomp: src/game/mario_actions_submerged.c:255 (`update_swimming_yaw`).
pub fn update_swimming_yaw(cx: &mut ActionCx) {
    // decomp: `-(s16)(10.0f * m->controller->stickX)` (f32→s16 truncates).
    let target_yaw_vel = -((10.0 * cx.proc.stick_x) as i16);
    let mut av = cx.state.angle_vel[1].0 as i32;

    if target_yaw_vel > 0 {
        if av < 0 {
            av += 0x40;
            if av > 0x10 {
                av = 0x10;
            }
        } else {
            av = approach_s32(av, target_yaw_vel as i32, 0x10, 0x20);
        }
    } else if target_yaw_vel < 0 {
        if av > 0 {
            av -= 0x40;
            if av < -0x10 {
                av = -0x10;
            }
        } else {
            av = approach_s32(av, target_yaw_vel as i32, 0x20, 0x10);
        }
    } else {
        av = approach_s32(av, 0, 0x40, 0x40);
    }

    cx.state.angle_vel[1] = Angle(av as i16);
    cx.state.face_yaw = cx.state.face_yaw.wrapping_add(Angle(av as i16));
    // decomp: `m->faceAngle[2] = -m->angleVel[1] * 8;` (s16 math in C promotes
    // to int; computed in i32 here, then truncated on store).
    cx.state.face_roll = Angle((-(av) * 8) as i16);
}

/// decomp: src/game/mario_actions_submerged.c:284 (`update_swimming_pitch`).
pub fn update_swimming_pitch(cx: &mut ActionCx) {
    // decomp: `-(s16)(252.0f * m->controller->stickY)` (f32→s16 truncates).
    let target_pitch = -((252.0 * cx.proc.stick_y) as i16);

    let pitch_vel: i16 = if cx.state.face_pitch.0 < 0 {
        0x100
    } else {
        0x200
    };

    let mut pitch = cx.state.face_pitch.0 as i32;
    if pitch < target_pitch as i32 {
        // C: `if ((m->faceAngle[0] += pitchVel) > targetPitch)` — the add
        // happens first, then the comparison clamps to the target.
        pitch += pitch_vel as i32;
        if pitch > target_pitch as i32 {
            pitch = target_pitch as i32;
        }
    } else if pitch > target_pitch as i32 {
        pitch -= pitch_vel as i32;
        if pitch < target_pitch as i32 {
            pitch = target_pitch as i32;
        }
    }
    cx.state.face_pitch = Angle(pitch as i16);
}
