//! The step engine: one deterministic `tick` per 30 Hz frame.
//!
//! Each frame the current action runs its documented cancel checks, decides
//! its velocity, then calls a step routine that splits the move into four
//! quarter-steps (wiki:Movement steps@19723). For each quarter-step the
//! routine proposes a position, queries the world, and resolves the outcome:
//! landing, wall hit, ceiling bump, or walking off a ledge.

use crate::actions::{ActionCx, ActionRegistry, ActionResult};
use crate::angles::Angle;
use crate::events::Event;
use crate::input::{input_flags, process_input, RawInput};
use crate::params::MovementParams;
use crate::state::{act_flags, mario_flags, ActionId, CharacterState};
use crate::trig;
use crate::world::{
    mario_floor_is_slippery, surface_type, CollisionWorld, WallHit, CELL_HEIGHT_LIMIT,
    FLOOR_LOWER_LIMIT,
};
use glam::Vec3;

/// Number of sub-steps per frame (quarter steps).
/// spec: step.quarter_steps (verified: wiki:Movement steps@19723)
pub const QUARTER_STEPS: usize = 4;

/// Animation timeline state for the current slot, owned by the core so that
/// gameplay timing never depends on the renderer's animation player.
#[derive(Clone, Copy, Debug, Default)]
pub struct Timeline {
    /// Stable slot id (see `stepkit-anim` manifest).
    pub slot: u32,
    /// Current frame within the slot.
    pub frame: u32,
    /// Playback speed multiplier.
    pub speed: f32,
    /// True when a non-looping slot has finished.
    pub ended: bool,
}

/// Run one 30 Hz simulation tick, following the decomp's `execute_mario_action`
/// pipeline (plan §3; `mario.c:1699`):
///
/// 1. `update_mario_inputs`: button + joystick stages (FD-02) and the
///    geometry stage ([`update_mario_geometry_inputs`]): two wall-push
///    passes, floor/ceiling/water queries, the INPUT_ABOVE_SLIDE /
///    OFF_FLOOR / IN_WATER flags, INPUT_UNKNOWN_5, and the wall-kick /
///    double-jump timer decrements.
/// 2. Special floors / interactions: no-ops in Phase 3 (no dynamic
///    surfaces, no objects). The current in-action quicksand, fall-damage,
///    and wall handling is preserved untouched (mechanical adaptation).
/// 3. Action loop: run the action; while it signals a transition, run the
///    new action in the same tick. A 64-iteration `debug_assert` guard
///    catches transition cycles (the decomp loops unguarded).
/// 4. Post-action: quicksand sink (visual-only in the decomp — the sim's
///    `quicksand_depth` is managed by the actions), `squishTimer`
///    decrement AFTER the action (`mario.c:1205`), health update, then the
///    once-per-tick animation advance (AN-01: the decomp renderer advances
///    after `execute_mario_action`; the oracle harness mirrors it).
///
/// Event emission is preserved: every action-loop iteration appends to the
/// same event vec, so a tick that transitions twice still reports both
/// `ActionChanged` events in order.
pub fn tick(
    state: CharacterState,
    input: RawInput,
    world: &dyn CollisionWorld,
    params: &MovementParams,
    timeline: Timeline,
    registry: &ActionRegistry,
    prev_buttons: u16,
) -> (CharacterState, Timeline, Vec<Event>) {
    let mut state = state;
    state.warped = false;
    // decomp: gGlobalTimer increments at the start of the tick, before the
    // input and geometry stages (`mario.c:1314` via the game loop).
    state.global_timer = state.global_timer.wrapping_add(1);

    // ---- 1. update_mario_inputs (mario.c:1373) ----
    // Button + joystick stages (FD-02); zeroes state.input first.
    let proc = process_input(&input, prev_buttons, &mut state);
    // Area-level terrain type (decomp: m->area->terrainType); the floor
    // class helpers read it, so set it before the geometry stage.
    state.terrain_type = world.terrain_type();
    update_mario_geometry_inputs(&mut state, world);
    // m->flags &= 0xFFFFFF (mario.c:1376); particleFlags has no equivalent.
    state.flags &= 0x00FF_FFFF;
    if state.input & (input_flags::NONZERO_ANALOG | input_flags::A_PRESSED) == 0 {
        state.input |= input_flags::UNKNOWN_5;
    }
    if state.wall_kick_timer > 0 {
        state.wall_kick_timer -= 1;
    }
    if state.double_jump_timer > 0 {
        state.double_jump_timer -= 1;
    }

    // ---- 2. special floors / interactions: no-ops in Phase 3 ----

    // ---- 3. action loop (mario.c:1724) ----
    //
    // The decomp repeats while the action function returns TRUE. In the
    // plugin's model that is [`ActionResult::Goto`]; a transition that the
    // decomp's action function ends with `return FALSE` (all airborne
    // actions' step results; the ground actions' LEFT_GROUND/HIT_WALL
    // branches) is [`ActionResult::GotoNext`]: the new action is booked but
    // first runs on the next tick.
    let prev_slot = timeline.slot;
    let mut cx = ActionCx {
        state,
        input,
        proc,
        world,
        params,
        timeline,
        events: Vec::new(),
    };
    let mut in_loop = true;
    let mut iterations = 0u32;
    while in_loop {
        debug_assert!(
            iterations < 64,
            "action loop exceeded 64 iterations in one tick"
        );
        iterations += 1;
        // FD-06: no automatic timer increment. Each action manages
        // `action_timer` itself, exactly like the decomp (most actions only
        // increment on specific branches; e.g. `act_idle` never increments).
        let result = match registry.get(cx.state.action) {
            Some(handler) => handler.tick(&mut cx, prev_buttons),
            None => ActionResult::Stay, // unknown action: hold state (defensive)
        };
        in_loop = matches!(result, ActionResult::Goto(..));
    }

    // ---- 4. post-action ----
    // sink_mario_in_quicksand touches only the graphical position in the
    // decomp; the sim's quicksandDepth is managed by the actions — no-op.
    // squish_mario_model decrements the timer AFTER the action
    // (mario.c:1205); the model scale itself is renderer-only.
    if cx.state.squish_timer > 0 {
        cx.state.squish_timer -= 1;
    }
    update_mario_health(&mut cx.state);

    // AN-01: advance the animation once per tick, AFTER the action loop
    // (the decomp's renderer does it after `execute_mario_action`, and the
    // oracle harness `oracle/harness.c` mirrors that order). Actions
    // therefore observe the previous tick's advance; the trace row emitted
    // after the tick sees the new frame.
    crate::anim::advance_animation(&mut cx.state);

    // Timeline: new slot restarts the frame counter.
    if cx.timeline.slot != prev_slot {
        cx.timeline.frame = 0;
        cx.timeline.ended = false;
    } else {
        cx.timeline.frame = cx.timeline.frame.wrapping_add(1);
    }
    // IK hints: renderer output, refreshed after the action tick.
    update_ik_hints(&mut cx.state);
    (cx.state, cx.timeline, cx.events)
}

/// Geometry stage of `update_mario_inputs`: the two wall-push passes,
/// floor/ceiling/water queries, and the INPUT_ABOVE_SLIDE / OFF_FLOOR /
/// IN_WATER flags.
/// decomp: src/game/mario.c:1314
///
/// Adaptations (documented, Phase 3): INPUT_SQUISHED needs
/// `SURFACE_FLAG_DYNAMIC` surfaces, which no backend produces yet;
/// INPUT_IN_POISON_GAS needs gas regions, which no scenario defines; the
/// OOB retry uses the graphical position (no equivalent here), so a
/// missing floor just records `None` and the actions' existing OOB
/// handling applies (no death warp in Phase 3).
fn update_mario_geometry_inputs(state: &mut CharacterState, world: &dyn CollisionWorld) {
    // f32_find_wall_collision, twice, with the decomp's radii. These push
    // the position but never set m->wall.
    state.pos = world
        .find_wall_collisions(state.pos.x, state.pos.y, state.pos.z, 60.0, 50.0)
        .pos;
    state.pos = world
        .find_wall_collisions(state.pos.x, state.pos.y, state.pos.z, 30.0, 24.0)
        .pos;

    // find_floor sets m->floor and m->floorHeight here.
    let floor = world.find_floor(state.pos.x, state.pos.y, state.pos.z);
    state.floor = floor;
    state.floor_y = floor.map(|f| f.y);
    if let Some(f) = floor {
        state.floor_angle = trig::atan2s(f.normal.z, f.normal.x);
    }

    // vec3f_find_ceil(pos, floorHeight): CELL_HEIGHT_LIMIT when no ceiling.
    // (The decomp passes m->floorHeight even when the floor is NULL; here
    // that is FLOOR_LOWER_LIMIT, exactly as `find_ceil`'s caller would.)
    let floor_height = state.floor_y.unwrap_or(FLOOR_LOWER_LIMIT);
    let ceil = world.find_ceil(state.pos.x, floor_height + 80.0, state.pos.z);
    state.ceil = ceil;
    state.ceil_y = Some(ceil.map(|c| c.y).unwrap_or(CELL_HEIGHT_LIMIT));

    // find_water_level: FLOOR_LOWER_LIMIT (-11000) when no water.
    state.water_level = world.find_water_level(state.pos.x, state.pos.z);

    if let Some(f) = floor {
        if state.pos.y > state.water_level - 40.0 && mario_floor_is_slippery(state) {
            state.input |= input_flags::ABOVE_SLIDE;
        }

        if state.pos.y > f.y + 100.0 {
            state.input |= input_flags::OFF_FLOOR;
        }

        if state.pos.y < state.water_level - 10.0 {
            state.input |= input_flags::IN_WATER;
        }
    }
}

/// Health update: poison-gas drain, swimming drain/regen, and the heal /
/// hurt counter drains, with the decomp's clamps.
/// decomp: src/game/mario.c:1454
///
/// Adaptations (documented, Phase 3): healCounter is not modeled (the
/// oracle never reports it and nothing sets it); the drowning-noise branch
/// is audio-only. The swimming branch is live once FD-07 lands
/// ACT_FLAG_SWIMMING on the water actions; until then only the hurtCounter
/// drain and the clamps can fire.
fn update_mario_health(state: &mut CharacterState) {
    if state.health >= 0x100 {
        // When already healing or hurting Mario, Mario's HP is not changed any more here.
        if state.hurt_counter == 0 {
            if (state.input & input_flags::IN_POISON_GAS) != 0
                && (state.action.0 & act_flags::INTANGIBLE) == 0
            {
                state.health -= 4;
            } else if (state.action.0 & act_flags::SWIMMING) != 0
                && (state.action.0 & act_flags::INTANGIBLE) == 0
            {
                let terrain_is_snow =
                    (state.terrain_type & crate::world::terrain_type::MASK)
                        == crate::world::terrain_type::SNOW;
                // When Mario is near the water surface, recover health (unless in snow),
                // when in snow terrains lose 3 health.
                if state.pos.y >= state.water_level - 140.0 && !terrain_is_snow {
                    state.health += 0x1A;
                } else {
                    state.health -= if terrain_is_snow { 3 } else { 1 };
                }
            }
        }

        if state.hurt_counter > 0 {
            state.health -= 0x40;
            state.hurt_counter -= 1;
        }

        if state.health > 0x880 {
            state.health = 0x880;
        }
        if state.health < 0x100 {
            state.health = 0xFF;
        }
    }
}

/// Update IK hand hints (renderer output only).
///
/// During ledge-grab actions the sim publishes world-space hand targets
/// (the grabbed edge, offset laterally by 25 units) so a renderer can
/// solve arm IK. All other actions clear the hints. Output-only: never
/// read by simulation logic, never hashed, never recorded in goldens.
fn update_ik_hints(state: &mut CharacterState) {
    use crate::state::ActionId;
    let is_ledge = matches!(
        state.action,
        ActionId::LEDGE_GRAB
            | ActionId::LEDGE_CLIMB_FAST
            | ActionId::LEDGE_CLIMB_SLOW_1
            | ActionId::LEDGE_CLIMB_SLOW_2
    );
    if is_ledge {
        if let Some(edge) = state.grab_point {
            let fx = crate::trig::sins(state.face_yaw);
            let fz = crate::trig::coss(state.face_yaw);
            // Lateral axis: horizontal perpendicular of the facing.
            let offset = Vec3::new(-fz * 25.0, 0.0, fx * 25.0);
            state.ik_hand_l = Some(edge + offset);
            state.ik_hand_r = Some(edge - offset);
            return;
        }
    }
    state.ik_hand_l = None;
    state.ik_hand_r = None;
}

// =====================================================================
// Ground step (ST-03).
// decomp: src/game/mario_step.c:258-346 (`perform_ground_quarter_step`,
// `perform_ground_step`, `stationary_ground_step`).
// =====================================================================

/// Result of `perform_ground_step`.
/// decomp: `GROUND_STEP_*` (`include/sm64.h:69-73`).
///
/// `GROUND_STEP_HIT_WALL_STOP_QSTEPS` and `GROUND_STEP_HIT_WALL_CONTINUE_QSTEPS`
/// are internal quarter-step results; `perform_ground_step` maps both to
/// `HitWall`. The plugin keeps them as private variants so the port stays
/// line-faithful.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GroundStep {
    None = 0,
    LeftGround = 1,
    HitWall = 2,
}

/// decomp: `GROUND_STEP_HIT_WALL_STOP_QSTEPS` (internal).
const GROUND_STEP_HIT_WALL_STOP_QSTEPS: u32 = 3;
/// decomp: `GROUND_STEP_HIT_WALL_CONTINUE_QSTEPS` (internal).
const GROUND_STEP_HIT_WALL_CONTINUE_QSTEPS: u32 = 4;

/// One quarter of a ground step. Mutates `m->pos`, `m->floor`,
/// `m->floorHeight`, and `m->wall` exactly like the decomp; returns the
/// internal quarter-step result code.
/// decomp: src/game/mario_step.c:258
fn perform_ground_quarter_step(
    state: &mut CharacterState,
    world: &dyn CollisionWorld,
    next_pos: &mut Vec3,
) -> u32 {
    // `resolve_and_return_wall_collisions` pushes nextPos and returns the
    // referenced wall. Lower wall first (30, 24), then upper (60, 50),
    // both on the same nextPos.
    let lower = world.find_wall_collisions(next_pos.x, next_pos.y, next_pos.z, 30.0, 24.0);
    *next_pos = lower.pos;
    let upper = world.find_wall_collisions(next_pos.x, next_pos.y, next_pos.z, 60.0, 50.0);
    *next_pos = upper.pos;

    let floor = world.find_floor(next_pos.x, next_pos.y, next_pos.z);
    let floor_height = floor.map(|f| f.y).unwrap_or(FLOOR_LOWER_LIMIT);
    let ceil_height = world
        .find_ceil(next_pos.x, floor_height + 80.0, next_pos.z)
        .map(|c| c.y)
        .unwrap_or(CELL_HEIGHT_LIMIT);

    // m->wall = upperWall (only the upper wall is recorded).
    state.wall = upper.wall;

    if floor.is_none() {
        return GROUND_STEP_HIT_WALL_STOP_QSTEPS;
    }
    let floor = floor.unwrap();

    // The riding-shell water pseudo-floor is out of scope (no shell).
    if next_pos.y > floor_height + 100.0 {
        if next_pos.y + 160.0 >= ceil_height {
            return GROUND_STEP_HIT_WALL_STOP_QSTEPS;
        }

        state.pos = *next_pos;
        state.floor = Some(floor);
        state.floor_y = Some(floor_height);
        return GroundStep::LeftGround as u32;
    }

    if floor_height + 160.0 >= ceil_height {
        return GROUND_STEP_HIT_WALL_STOP_QSTEPS;
    }

    state.pos = Vec3::new(next_pos.x, floor_height, next_pos.z);
    state.floor = Some(floor);
    state.floor_y = Some(floor_height);

    if let Some(wall) = upper.wall {
        let wall_dyaw = trig::atan2s(wall.normal.z, wall.normal.x)
            .wrapping_sub(state.face_yaw)
            .0;
        if wall_dyaw >= 0x2AAA && wall_dyaw <= 0x5555 {
            return GroundStep::None as u32;
        }
        if wall_dyaw <= -0x2AAA && wall_dyaw >= -0x5555 {
            return GroundStep::None as u32;
        }

        return GROUND_STEP_HIT_WALL_CONTINUE_QSTEPS;
    }

    GroundStep::None as u32
}

/// Full ground step: four quarter steps.
/// decomp: src/game/mario_step.c:317
///
/// Each quarter step's intended position builds on `m->pos` (which the
/// previous quarter step committed) scaled by the CURRENT `m->floor`
/// normal's y. Breaks on `LEFT_GROUND` / `HIT_WALL_STOP_QSTEPS`;
/// `HIT_WALL_CONTINUE_QSTEPS` keeps stepping and maps to `HIT_WALL` at the
/// end (only the 4th quarter step's CONTINUE surfaces).
pub fn perform_ground_step(
    state: &mut CharacterState,
    world: &dyn CollisionWorld,
) -> GroundStep {
    let mut step_result = GroundStep::None as u32;

    for _ in 0..QUARTER_STEPS {
        // m->floor is guaranteed non-NULL by the callers (the geometry
        // stage bails OOB before the action runs); 1.0 is the defensive
        // fallback, matching a flat floor.
        let ny = state.floor.map(|f| f.normal.y).unwrap_or(1.0);
        let mut intended = Vec3::new(
            state.pos.x + state.vel.x / 4.0 * ny,
            state.pos.y,
            state.pos.z + state.vel.z / 4.0 * ny,
        );

        step_result = perform_ground_quarter_step(state, world, &mut intended);
        if step_result == GroundStep::LeftGround as u32
            || step_result == GROUND_STEP_HIT_WALL_STOP_QSTEPS
        {
            break;
        }
    }

    // m->terrainSoundAddend and the gfx pos/angle copies are audio/renderer
    // state; skipped.

    if step_result == GROUND_STEP_HIT_WALL_CONTINUE_QSTEPS {
        step_result = GroundStep::HitWall as u32;
    }
    match step_result {
        0 => GroundStep::None,
        1 => GroundStep::LeftGround,
        _ => GroundStep::HitWall,
    }
}

/// Stationary ground step: sand/wind may push Mario (then a real ground
/// step runs); otherwise Mario just snaps to the floor height.
/// decomp: src/game/mario_step.c:236
pub fn stationary_ground_step(
    state: &mut CharacterState,
    world: &dyn CollisionWorld,
) -> GroundStep {
    // decomp: mario_set_forward_vel(m, 0.0f) (mario_step.c:240).
    mario_set_forward_vel(state, 0.0);
    let mut take_step = mario_update_moving_sand(state);
    take_step |= mario_update_windy_ground(state);
    if take_step {
        perform_ground_step(state, world)
    } else {
        // This is responsible for several stationary downwarps.
        if let Some(fh) = state.floor_y {
            state.pos.y = fh;
        }
        GroundStep::None
    }
}

// =====================================================================
// Air step (ST-04).
// decomp: src/game/mario_step.c:388-581 (`perform_air_quarter_step`,
// `perform_air_step`, `apply_gravity`, `should_strengthen_gravity_for_jump_ascent`,
// `apply_twirl_gravity`, `apply_vertical_wind`).
// =====================================================================

/// Step-arg flags for `perform_air_step`.
/// decomp: `AIR_STEP_CHECK_LEDGE_GRAB`, `AIR_STEP_CHECK_HANG` (`include/sm64.h:75-76`).
pub const AIR_STEP_CHECK_LEDGE_GRAB: u32 = 0x00000001;
pub const AIR_STEP_CHECK_HANG: u32 = 0x00000002;

/// Result of `perform_air_step`.
/// decomp: `AIR_STEP_*` (`include/sm64.h:78-83`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AirStep {
    None = 0,
    Landed = 1,
    HitWall = 2,
    GrabbedLedge = 3,
    GrabbedCeiling = 4,
    HitLavaWall = 6,
}

/// One quarter of an air step. Mutates `m->pos`, `m->vel[1]`, `m->floor`,
/// `m->floorHeight`, `m->wall`, and `m->flags` exactly like the decomp.
/// decomp: src/game/mario_step.c:388
fn perform_air_quarter_step(
    state: &mut CharacterState,
    world: &dyn CollisionWorld,
    intended_pos: Vec3,
    step_arg: u32,
) -> AirStep {
    let mut next_pos = intended_pos;

    // Upper wall (150, 50) first, then lower (30, 50); both push nextPos.
    let upper = world.find_wall_collisions(next_pos.x, next_pos.y, next_pos.z, 150.0, 50.0);
    next_pos = upper.pos;
    let lower = world.find_wall_collisions(next_pos.x, next_pos.y, next_pos.z, 30.0, 50.0);
    next_pos = lower.pos;

    let floor = world.find_floor(next_pos.x, next_pos.y, next_pos.z);
    let floor_height = floor.map(|f| f.y).unwrap_or(FLOOR_LOWER_LIMIT);
    let ceil_height = world
        .find_ceil(next_pos.x, floor_height + 80.0, next_pos.z)
        .map(|c| c.y)
        .unwrap_or(CELL_HEIGHT_LIMIT);

    state.wall = None;

    if floor.is_none() {
        if next_pos.y <= state.floor_y.unwrap_or(FLOOR_LOWER_LIMIT) {
            state.pos.y = state.floor_y.unwrap_or(FLOOR_LOWER_LIMIT);
            return AirStep::Landed;
        }

        state.pos.y = next_pos.y;
        return AirStep::HitWall;
    }
    let floor = floor.unwrap();

    // The riding-shell water pseudo-floor is out of scope (no shell).
    if next_pos.y <= floor_height {
        if ceil_height - floor_height > 160.0 {
            state.pos.x = next_pos.x;
            state.pos.z = next_pos.z;
            state.floor = Some(floor);
            state.floor_y = Some(floor_height);
        }

        // When ceilHeight - floorHeight <= 160, the step result says that
        // Mario landed, but his movement is cancelled and his referenced
        // floor isn't updated (pedro spots).
        state.pos.y = floor_height;
        return AirStep::Landed;
    }

    if next_pos.y + 160.0 > ceil_height {
        if state.vel.y >= 0.0 {
            state.vel.y = 0.0;

            // Uses the REFERENCED ceiling (m->ceil), not the local one
            // (ceiling hang upwarp).
            if (step_arg & AIR_STEP_CHECK_HANG) != 0
                && state
                    .ceil
                    .is_some_and(|c| c.surface_type == surface_type::HANGABLE)
            {
                return AirStep::GrabbedCeiling;
            }

            return AirStep::None;
        }

        if next_pos.y <= state.floor_y.unwrap_or(FLOOR_LOWER_LIMIT) {
            state.pos.y = state.floor_y.unwrap_or(FLOOR_LOWER_LIMIT);
            return AirStep::Landed;
        }

        state.pos.y = next_pos.y;
        return AirStep::HitWall;
    }

    // When the wall is not completely vertical or there is a slight wall
    // misalignment, you can activate these conditions in unexpected situations.
    if (step_arg & AIR_STEP_CHECK_LEDGE_GRAB) != 0 && upper.wall.is_none() && lower.wall.is_some() {
        if check_ledge_grab(state, world, &lower.wall.unwrap(), intended_pos, next_pos) {
            return AirStep::GrabbedLedge;
        }

        state.pos = next_pos;
        state.floor = Some(floor);
        state.floor_y = Some(floor_height);
        return AirStep::None;
    }

    state.pos = next_pos;
    state.floor = Some(floor);
    state.floor_y = Some(floor_height);

    if upper.wall.is_some() || lower.wall.is_some() {
        let wall = upper.wall.or(lower.wall).unwrap();
        state.wall = Some(wall);
        let wall_dyaw = trig::atan2s(wall.normal.z, wall.normal.x)
            .wrapping_sub(state.face_yaw)
            .0;

        if wall.surface_type == surface_type::BURNING {
            return AirStep::HitLavaWall;
        }

        if wall_dyaw < -0x6000 || wall_dyaw > 0x6000 {
            state.flags |= mario_flags::UNKNOWN_30;
            return AirStep::HitWall;
        }
    }

    AirStep::None
}

/// Full air step: four quarter steps, then gravity, then vertical wind.
/// decomp: src/game/mario_step.c:613
///
/// The last non-NONE quarter-step result wins; LANDED / GRABBED_LEDGE /
/// GRABBED_CEILING / HIT_LAVA_WALL break the loop (HIT_WALL does not — it
/// is stored and stepping continues).
pub fn perform_air_step(
    state: &mut CharacterState,
    world: &dyn CollisionWorld,
    step_arg: u32,
) -> AirStep {
    let mut step_result = AirStep::None;

    state.wall = None;

    for _ in 0..QUARTER_STEPS {
        let intended = Vec3::new(
            state.pos.x + state.vel.x / 4.0,
            state.pos.y + state.vel.y / 4.0,
            state.pos.z + state.vel.z / 4.0,
        );

        let quarter = perform_air_quarter_step(state, world, intended, step_arg);

        // On one qf, hit OOB/ceil/wall to store the 2 return value, and
        // continue getting 0s until your last qf. Graze a wall on your last
        // qf, and it will return the stored 2 with a sharply angled
        // reference wall. (some gwks)
        if quarter != AirStep::None {
            step_result = quarter;
        }

        if quarter == AirStep::Landed
            || quarter == AirStep::GrabbedLedge
            || quarter == AirStep::GrabbedCeiling
            || quarter == AirStep::HitLavaWall
        {
            break;
        }
    }

    if state.vel.y >= 0.0 {
        // peakHeight tracks the apex: only written while rising (the old
        // max-y tracking is replaced by this).
        state.peak_height = state.pos.y;
    }

    // m->terrainSoundAddend is audio; skipped.

    if state.action != ActionId::FLYING {
        apply_gravity(state);
    }
    apply_vertical_wind(state);

    // The gfx pos/angle copies are renderer state; skipped.

    step_result
}

/// Gravity selection: the full if/else-if chain, in order.
/// decomp: src/game/mario_step.c:535
pub fn apply_gravity(state: &mut CharacterState) {
    if state.action == ActionId::TWIRLING && state.vel.y < 0.0 {
        apply_twirl_gravity(state);
    } else if state.action == ActionId::SHOT_FROM_CANNON {
        state.vel.y -= 1.0;
        if state.vel.y < -75.0 {
            state.vel.y = -75.0;
        }
    } else if state.action == ActionId::LONG_JUMP
        || state.action == ActionId::SLIDE_KICK
        || state.action == ActionId::BBH_ENTER_SPIN
    {
        state.vel.y -= 2.0;
        if state.vel.y < -75.0 {
            state.vel.y = -75.0;
        }
    } else if state.action == ActionId::LAVA_BOOST || state.action == ActionId::FALL_AFTER_STAR_GRAB {
        state.vel.y -= 3.2;
        if state.vel.y < -65.0 {
            state.vel.y = -65.0;
        }
    } else if state.action == ActionId::GETTING_BLOWN {
        state.vel.y -= state.getting_blown_gravity;
        if state.vel.y < -75.0 {
            state.vel.y = -75.0;
        }
    } else if should_strengthen_gravity_for_jump_ascent(state) {
        // Strengthen gravity when jumping.
        state.vel.y /= 4.0;
    } else if (state.action.0 & act_flags::METAL_WATER) != 0 {
        state.vel.y -= 1.6;
        if state.vel.y < -16.0 {
            state.vel.y = -16.0;
        }
    } else if (state.flags & mario_flags::WING_CAP) != 0
        && state.vel.y < 0.0
        && (state.input & input_flags::A_DOWN) != 0
    {
        // (wingFlutter is animation state; skipped.)
        state.vel.y -= 2.0;
        if state.vel.y < -37.5 {
            state.vel.y += 4.0;
            if state.vel.y > -37.5 {
                state.vel.y = -37.5;
            }
        }
    } else {
        state.vel.y -= 4.0;
        if state.vel.y < -75.0 {
            state.vel.y = -75.0;
        }
    }
}

/// Whether releasing A during a fast jump ascent quarters the upward
/// velocity instead of applying gravity.
/// decomp: src/game/mario_step.c:519
pub fn should_strengthen_gravity_for_jump_ascent(state: &CharacterState) -> bool {
    if (state.flags & mario_flags::UNKNOWN_08) == 0 {
        return false;
    }

    if (state.action.0 & (act_flags::INTANGIBLE | act_flags::INVULNERABLE)) != 0 {
        return false;
    }

    if (state.input & input_flags::A_DOWN) == 0 && state.vel.y > 20.0 {
        return (state.action.0 & act_flags::CONTROL_JUMP_HEIGHT) != 0;
    }

    false
}

/// Twirl gravity: heaviness scales with spin speed.
/// decomp: src/game/mario_step.c:505
pub fn apply_twirl_gravity(state: &mut CharacterState) {
    let mut heaviness = 1.0f32;
    let yaw_vel = state.angle_vel[1].0;
    if yaw_vel > 1024 {
        heaviness = 1024.0 / yaw_vel as f32;
    }
    let terminal = -75.0 * heaviness;
    state.vel.y -= 4.0 * heaviness;
    if state.vel.y < terminal {
        state.vel.y = terminal;
    }
}

/// Vertical wind pushes Mario up when over a vertical-wind floor.
/// decomp: src/game/mario_step.c:582
pub fn apply_vertical_wind(state: &mut CharacterState) {
    if state.action == ActionId::GROUND_POUND {
        return;
    }
    let offset_y = state.pos.y - -1500.0;
    let on_wind = state
        .floor
        .is_some_and(|f| f.surface_type == surface_type::VERTICAL_WIND);
    if on_wind && -3000.0 < offset_y && offset_y < 2000.0 {
        let max_vel_y = if offset_y >= 0.0 {
            10000.0 / (offset_y + 200.0)
        } else {
            50.0
        };
        if state.vel.y < max_vel_y {
            state.vel.y += max_vel_y / 8.0;
            if state.vel.y > max_vel_y {
                state.vel.y = max_vel_y;
            }
        }
    }
}

// =====================================================================
// ST-08 helpers.
// decomp: src/game/mario_step.c:89-223 (`mario_bonk_reflection`,
// `mario_update_quicksand`, `mario_push_off_steep_floor`,
// `mario_update_moving_sand`, `mario_update_windy_ground`,
// `stop_and_set_height_to_floor`, `check_ledge_grab`), plus
// `mario_set_forward_vel` (src/game/mario.c:374) and
// `set_vel_from_yaw` / `set_vel_from_pitch_and_yaw` (mario_step.c:659).
// =====================================================================

/// Set the forward speed and derive the velocity from the facing yaw.
/// decomp: src/game/mario.c:374
pub fn mario_set_forward_vel(state: &mut CharacterState, forward_vel: f32) {
    // decomp: mario_set_forward_vel (mario_step.c:82) sets forwardVel AND
    // re-derives slideVel/vel from the facing yaw.
    state.forward_speed = forward_vel;
    state.slide_vel_x = forward_vel * trig::sins(state.face_yaw);
    state.slide_vel_z = forward_vel * trig::coss(state.face_yaw);
    state.vel.x = state.slide_vel_x;
    state.vel.z = state.slide_vel_z;
}

/// Reflect the facing angle off the referenced wall; optionally negate the
/// forward speed.
/// decomp: src/game/mario_step.c:89
pub fn mario_bonk_reflection(state: &mut CharacterState, negate_speed: bool) {
    if let Some(wall) = state.wall {
        let wall_angle = trig::atan2s(wall.normal.z, wall.normal.x);
        // s16 arithmetic: wallAngle - (faceAngle - wallAngle).
        state.face_yaw = wall_angle.wrapping_sub(state.face_yaw.wrapping_sub(wall_angle));
    }
    // (Sound selection is audio; skipped.)

    if negate_speed {
        mario_set_forward_vel(state, -state.forward_speed);
    } else {
        state.face_yaw = state.face_yaw.wrapping_add(Angle::HALF_TURN);
    }
}

/// Quicksand sink update. Returns true when Mario drowned (the caller must
/// transition to `ACT_QUICKSAND_DEATH`); the decomp returns
/// `drop_and_set_mario_action(...)` there.
/// decomp: src/game/mario_step.c:107
pub fn mario_update_quicksand(state: &mut CharacterState, sinking_speed: f32) -> bool {
    if (state.action.0 & act_flags::RIDING_SHELL) != 0 {
        state.quicksand_depth = 0.0;
    } else {
        if state.quicksand_depth < 1.1 {
            state.quicksand_depth = 1.1;
        }
        // m->floor is guaranteed non-NULL by the callers here.
        let floor_type = state.floor.map(|f| f.surface_type).unwrap_or(0);
        match floor_type {
            surface_type::SHALLOW_QUICKSAND => {
                state.quicksand_depth += sinking_speed;
                if state.quicksand_depth >= 10.0 {
                    state.quicksand_depth = 10.0;
                }
            }
            surface_type::SHALLOW_MOVING_QUICKSAND => {
                state.quicksand_depth += sinking_speed;
                if state.quicksand_depth >= 25.0 {
                    state.quicksand_depth = 25.0;
                }
            }
            surface_type::QUICKSAND | surface_type::MOVING_QUICKSAND => {
                state.quicksand_depth += sinking_speed;
                if state.quicksand_depth >= 60.0 {
                    state.quicksand_depth = 60.0;
                }
            }
            surface_type::DEEP_QUICKSAND | surface_type::DEEP_MOVING_QUICKSAND => {
                state.quicksand_depth += sinking_speed;
                if state.quicksand_depth >= 160.0 {
                    return true;
                }
            }
            surface_type::INSTANT_QUICKSAND | surface_type::INSTANT_MOVING_QUICKSAND => {
                return true;
            }
            _ => {
                state.quicksand_depth = 0.0;
            }
        }
    }
    false
}

/// Push off a steep floor: face up/down the slope at ±16 speed. The caller
/// transitions to the given action (the decomp returns
/// `set_mario_action(m, action, actionArg)`, i.e. TRUE → same-tick).
/// decomp: src/game/mario_step.c:158
pub fn mario_push_off_steep_floor(state: &mut CharacterState) {
    let floor_dyaw = state.floor_angle.wrapping_sub(state.face_yaw).0;
    if floor_dyaw > -0x4000 && floor_dyaw < 0x4000 {
        state.forward_speed = 16.0;
        state.face_yaw = state.floor_angle;
    } else {
        state.forward_speed = -16.0;
        state.face_yaw = state.floor_angle.wrapping_add(Angle::HALF_TURN);
    }
}

/// Moving sand pushes Mario along `force << 8` at a speed from
/// `sMovingSandSpeeds = { 12, 8, 4, 0 }[force >> 8]`. Returns whether Mario
/// takes a step this frame.
/// decomp: src/game/mario_step.c:172
pub fn mario_update_moving_sand(state: &mut CharacterState) -> bool {
    const MOVING_SAND_SPEEDS: [f32; 4] = [12.0, 8.0, 4.0, 0.0];
    // m->floor is guaranteed non-NULL by the callers here.
    let (floor_type, force) = state
        .floor
        .map(|f| (f.surface_type, f.force))
        .unwrap_or((0, 0));
    if floor_type == surface_type::DEEP_MOVING_QUICKSAND
        || floor_type == surface_type::SHALLOW_MOVING_QUICKSAND
        || floor_type == surface_type::MOVING_QUICKSAND
        || floor_type == surface_type::INSTANT_MOVING_QUICKSAND
    {
        let push_angle = Angle((force << 8) as i16);
        let push_speed = MOVING_SAND_SPEEDS[((force >> 8) & 3) as usize];
        state.vel.x += push_speed * trig::sins(push_angle);
        state.vel.z += push_speed * trig::coss(push_angle);
        return true;
    }
    false
}

/// Horizontal wind pushes Mario. Returns whether Mario takes a step.
/// decomp: src/game/mario_step.c:190
pub fn mario_update_windy_ground(state: &mut CharacterState) -> bool {
    // m->floor is guaranteed non-NULL by the callers here.
    let (floor_type, force) = state
        .floor
        .map(|f| (f.surface_type, f.force))
        .unwrap_or((0, 0));
    if floor_type == surface_type::HORIZONTAL_WIND {
        let push_angle = Angle((force << 8) as i16);
        let push_speed = if (state.action.0 & act_flags::MOVING) != 0 {
            let push_dyaw = state.face_yaw.wrapping_sub(push_angle);
            let mut push_speed = if state.forward_speed > 0.0 {
                -state.forward_speed * 0.5
            } else {
                -8.0
            };
            if push_dyaw.0 > -0x4000 && push_dyaw.0 < 0x4000 {
                push_speed *= -1.0;
            }
            push_speed * trig::coss(push_dyaw)
        } else {
            3.2 + (state.global_timer % 4) as f32
        };
        state.vel.x += push_speed * trig::sins(push_angle);
        state.vel.z += push_speed * trig::coss(push_angle);
        return true;
    }
    false
}

/// Stop horizontal motion and snap to the floor height.
/// decomp: src/game/mario_step.c:223
pub fn stop_and_set_height_to_floor(state: &mut CharacterState) {
    mario_set_forward_vel(state, 0.0);
    state.vel.y = 0.0;
    // This is responsible for some downwarps.
    if let Some(fh) = state.floor_y {
        state.pos.y = fh;
    }
}

/// Ledge grab check. Places Mario at the ledge (feet ON the ledge top),
/// facing away from the wall.
/// decomp: src/game/mario_step.c:348
pub fn check_ledge_grab(
    state: &mut CharacterState,
    world: &dyn CollisionWorld,
    wall: &WallHit,
    intended_pos: Vec3,
    next_pos: Vec3,
) -> bool {
    if state.vel.y > 0.0 {
        return false;
    }

    let displacement_x = next_pos.x - intended_pos.x;
    let displacement_z = next_pos.z - intended_pos.z;

    // Only ledge grab if the wall displaced Mario in the opposite direction
    // of his velocity.
    if displacement_x * state.vel.x + displacement_z * state.vel.z > 0.0 {
        return false;
    }

    // Since the search for floors starts at y + 160, we will sometimes grab
    // a higher ledge than expected (glitchy ledge grab).
    let ledge_x = next_pos.x - wall.normal.x * 60.0;
    let ledge_z = next_pos.z - wall.normal.z * 60.0;
    let ledge_floor = world.find_floor(ledge_x, next_pos.y + 160.0, ledge_z);
    let ledge_y = ledge_floor.map(|f| f.y).unwrap_or(FLOOR_LOWER_LIMIT);

    if ledge_y - next_pos.y <= 100.0 {
        return false;
    }

    state.pos = Vec3::new(ledge_x, ledge_y, ledge_z);
    state.floor = ledge_floor;
    state.floor_y = Some(ledge_y);

    if let Some(f) = ledge_floor {
        state.floor_angle = trig::atan2s(f.normal.z, f.normal.x);
    }

    state.face_yaw = trig::atan2s(wall.normal.z, wall.normal.x).wrapping_add(Angle::HALF_TURN);
    true
}

/// Derive velocity from the facing yaw and forward speed.
/// decomp: src/game/mario_step.c:659
pub fn set_vel_from_yaw(state: &mut CharacterState) {
    state.vel.x = state.forward_speed * trig::sins(state.face_yaw);
    state.vel.z = state.forward_speed * trig::coss(state.face_yaw);
}

/// Derive velocity from the facing pitch and yaw.
/// decomp: src/game/mario_step.c:665
pub fn set_vel_from_pitch_and_yaw(state: &mut CharacterState) {
    state.vel.x = state.forward_speed * trig::coss(state.face_pitch) * trig::sins(state.face_yaw);
    state.vel.y = state.forward_speed * trig::sins(state.face_pitch);
    state.vel.z = state.forward_speed * trig::coss(state.face_pitch) * trig::coss(state.face_yaw);
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ActionRegistry;
    use crate::input::RawInput;
    use crate::surface::SurfaceWorld;

    fn flat_world() -> SurfaceWorld {
        let mut w = SurfaceWorld::new();
        w.add_box(
            Vec3::new(-2000.0, -100.0, -2000.0),
            Vec3::new(2000.0, 0.0, 2000.0),
            crate::world::surface_type::DEFAULT,
            0,
        );
        w
    }

    #[test]
    fn ik_hints_set_during_ledge_grab() {
        use crate::state::ActionId;
        // Simulate a grab: ledge action + recorded edge point.
        let mut state = CharacterState {
            action: ActionId::LEDGE_GRAB,
            grab_point: Some(Vec3::new(0.0, 400.0, 400.0)),
            face_yaw: crate::angles::Angle::ZERO, // facing +z
            ..Default::default()
        };
        update_ik_hints(&mut state);
        assert!(state.ik_hand_l.is_some());
        assert!(state.ik_hand_r.is_some());
        let l = state.ik_hand_l.unwrap();
        let r = state.ik_hand_r.unwrap();
        // Hands offset laterally (±25) from the edge, at edge height.
        assert!((l.y - 400.0).abs() < 0.01);
        assert!((r.y - 400.0).abs() < 0.01);
        assert!((l - r).length() > 49.0 && (l - r).length() < 51.0);
        // Leaving the ledge clears the hints.
        state.action = ActionId::IDLE;
        update_ik_hints(&mut state);
        assert!(state.ik_hand_l.is_none());
        assert!(state.ik_hand_r.is_none());
    }

    #[test]
    fn idle_tick_stays_idle_without_input() {
        let world = flat_world();
        let registry = ActionRegistry::sm64_style();
        let (out, _, events) = tick(
            CharacterState::default(),
            RawInput::default(),
            &world,
            &MovementParams::default(),
            Timeline::default(),
            &registry,
            0,
        );
        assert_eq!(out.action, crate::state::ActionId::IDLE);
        assert!(events.is_empty());
    }

    #[test]
    fn determinism_10k_frames() {
        // Phase 2 gate: a scripted 10,000-frame scenario hashes identically
        // across runs (CI also builds for aarch64).
        let world = flat_world();
        let registry = ActionRegistry::sm64_style();
        let params = MovementParams::default();
        let run = || {
            let mut state = CharacterState::default();
            let mut timeline = Timeline::default();
            let mut prev_buttons = 0u16;
            for frame in 0..10_000u32 {
                let stick_y = if frame < 5000 { 80 } else { 0 };
                let input = RawInput {
                    stick_x: 0,
                    stick_y,
                    buttons: 0,
                    cam_yaw: crate::angles::Angle::ZERO,
                };
                let (s, t, _) = tick(
                    state,
                    input,
                    &world,
                    &params,
                    timeline,
                    &registry,
                    prev_buttons,
                );
                state = s;
                timeline = t;
                prev_buttons = input.buttons;
            }
            state.hash_state()
        };
        let h1 = run();
        let h2 = run();
        assert_eq!(h1, h2);
        assert_ne!(h1, CharacterState::default().hash_state());
    }
}
