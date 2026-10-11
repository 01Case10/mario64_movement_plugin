//! Landing actions: verbatim ports of the decomp landing behavior.
//!
//! - Moving landings: the `LandingAction` parameter tables,
//!   `common_landing_cancels`, `common_landing_action`, and
//!   `act_{jump,freefall,side_flip,long_jump,double_jump,triple_jump,backflip}_land`
//!   (src/game/mario_actions_moving.c).
//! - Stationary landings: `landing_step`, `check_common_landing_cancels`,
//!   `act_{jump,double_jump,side_flip,freefall,triple_jump,backflip,long_jump}_land_stop`,
//!   and `act_ground_pound_land`
//!   (src/game/mario_actions_stationary.c).
//!
//! Each handler inlines its group preamble (the port has no shared per-group
//! executor): the moving actions run `check_common_moving_cancels` +
//! `mario_update_quicksand(0.25)`; the stop actions run
//! `check_common_stationary_cancels`. Sound, particle, camera, and
//! render-model (gfx) effects are skipped with comments. `act_quicksand_jump_land`
//! (mario_actions_moving.c:1920) belongs to quicksand.rs and is not ported here.

use super::entry;
use super::{ActionCx, ActionHandler, ActionResult};
use crate::anim_table as anim_id;
use crate::input::input_flags;
use crate::state::{act_flags, ActionId};
use crate::step::GroundStep;
use crate::world::surface_type;

// ---------------------------------------------------------------------------
// Landing parameter tables
// ---------------------------------------------------------------------------

/// decomp: `struct LandingAction` (src/game/mario_actions_moving.c:20).
/// Field order mirrors the decomp exactly; the initializers below copy the
/// decomp's values positionally.
#[derive(Clone, Copy)]
pub struct LandingAction {
    /// Frames before the landing resolves to `end_action`.
    pub num_frames: i16,
    /// Value written to `doubleJumpTimer` on entry (0 disables chaining).
    pub unk02: i16,
    /// Target when the floor is too steep (FREEFALL in every table).
    pub very_steep_action: ActionId,
    /// Target when the landing timer expires or first-person is entered.
    pub end_action: ActionId,
    /// Target passed to the A-press callback (the triple-jump table's
    /// callback ignores it).
    pub a_pressed_action: ActionId,
    /// Target when Mario leaves the floor mid-landing.
    pub off_floor_action: ActionId,
    /// BEGIN_SLIDING (remapped to butt/stomach slide by
    /// `set_mario_action_moving`).
    pub slide_action: ActionId,
}

/// decomp: `ACT_UNINITIALIZED` (include/sm64.h:172); the triple-jump landing
/// table's A-press target. A is always masked there, so it never fires.
const ACT_UNINITIALIZED: ActionId = ActionId(0x00000000);

/// decomp: `sJumpLandAction` (src/game/mario_actions_moving.c:26).
pub const S_JUMP_LAND_ACTION: LandingAction = LandingAction {
    num_frames: 4,
    unk02: 5,
    very_steep_action: ActionId::FREEFALL,
    end_action: ActionId::JUMP_LAND_STOP,
    a_pressed_action: ActionId::DOUBLE_JUMP,
    off_floor_action: ActionId::FREEFALL,
    slide_action: ActionId::BEGIN_SLIDING,
};

/// decomp: `sFreefallLandAction` (src/game/mario_actions_moving.c:30).
pub const S_FREEFALL_LAND_ACTION: LandingAction = LandingAction {
    num_frames: 4,
    unk02: 5,
    very_steep_action: ActionId::FREEFALL,
    end_action: ActionId::FREEFALL_LAND_STOP,
    a_pressed_action: ActionId::DOUBLE_JUMP,
    off_floor_action: ActionId::FREEFALL,
    slide_action: ActionId::BEGIN_SLIDING,
};

/// decomp: `sSideFlipLandAction` (src/game/mario_actions_moving.c:34).
pub const S_SIDE_FLIP_LAND_ACTION: LandingAction = LandingAction {
    num_frames: 4,
    unk02: 5,
    very_steep_action: ActionId::FREEFALL,
    end_action: ActionId::SIDE_FLIP_LAND_STOP,
    a_pressed_action: ActionId::DOUBLE_JUMP,
    off_floor_action: ActionId::FREEFALL,
    slide_action: ActionId::BEGIN_SLIDING,
};

/// decomp: `sLongJumpLandAction` (src/game/mario_actions_moving.c:46).
pub const S_LONG_JUMP_LAND_ACTION: LandingAction = LandingAction {
    num_frames: 6,
    unk02: 5,
    very_steep_action: ActionId::FREEFALL,
    end_action: ActionId::LONG_JUMP_LAND_STOP,
    a_pressed_action: ActionId::LONG_JUMP,
    off_floor_action: ActionId::FREEFALL,
    slide_action: ActionId::BEGIN_SLIDING,
};

/// decomp: `sDoubleJumpLandAction` (src/game/mario_actions_moving.c:50).
pub const S_DOUBLE_JUMP_LAND_ACTION: LandingAction = LandingAction {
    num_frames: 4,
    unk02: 5,
    very_steep_action: ActionId::FREEFALL,
    end_action: ActionId::DOUBLE_JUMP_LAND_STOP,
    a_pressed_action: ActionId::JUMP,
    off_floor_action: ActionId::FREEFALL,
    slide_action: ActionId::BEGIN_SLIDING,
};

/// decomp: `sTripleJumpLandAction` (src/game/mario_actions_moving.c:54).
pub const S_TRIPLE_JUMP_LAND_ACTION: LandingAction = LandingAction {
    num_frames: 4,
    unk02: 0,
    very_steep_action: ActionId::FREEFALL,
    end_action: ActionId::TRIPLE_JUMP_LAND_STOP,
    a_pressed_action: ACT_UNINITIALIZED,
    off_floor_action: ActionId::FREEFALL,
    slide_action: ActionId::BEGIN_SLIDING,
};

/// decomp: `sBackflipLandAction` (src/game/mario_actions_moving.c:58).
pub const S_BACKFLIP_LAND_ACTION: LandingAction = LandingAction {
    num_frames: 4,
    unk02: 0,
    very_steep_action: ActionId::FREEFALL,
    end_action: ActionId::BACKFLIP_LAND_STOP,
    a_pressed_action: ActionId::BACKFLIP,
    off_floor_action: ActionId::FREEFALL,
    slide_action: ActionId::BEGIN_SLIDING,
};

// NOTE: `sHoldJumpLandAction` / `sHoldFreefallLandAction` (moving.c:38-44)
// are not ported: the port has no ACT_HOLD_* action ids.

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// A-press callback threaded through `common_landing_cancels`, mirroring the
/// decomp's `s32 (*setAPressAction)(struct MarioState *, u32, u32)`.
type APressFn = fn(&mut ActionCx, ActionId, u32) -> ActionResult;

/// Puts Mario in a given action, overruled by a quicksand or steep jump.
/// decomp: `set_jumping_action` (src/game/mario.c:1074).
///
/// The decomp's `set_mario_action(m, action, actionArg)` tail applies the
/// ACT_GROUP_AIRBORNE routing (`set_mario_action_airborne`, mario.c:896),
/// which performs the jump-entry velocity setup; the port owns that setup in
/// the `enter_*` helpers, so this dispatches to them instead of a bare
/// `goto`. Unlike `set_jump_from_landing`, this does NOT clear
/// `doubleJumpTimer`.
fn set_jumping_action(cx: &mut ActionCx, action: ActionId, arg: u32) -> ActionResult {
    // Canonical version in super::entry (includes the squish/quicksand
    // DOUBLE_JUMP→JUMP downgrade from set_mario_action_airborne).
    entry::set_jumping_action(cx, action, arg)
}

/// Adapter for the double-jump landing's A-press callback.
/// decomp: `set_triple_jump_action` (src/game/mario_actions_moving.c:147)
/// takes `(m, action, actionArg)` and ignores the last two; the port's
/// `entry::set_triple_jump_action` takes only `cx`.
fn set_triple_jump_action(cx: &mut ActionCx, _action: ActionId, _arg: u32) -> ActionResult {
    entry::set_triple_jump_action(cx)
}

/// Moving-group preamble: water plunge, stomp, squish, death, quicksand.
/// Canonical version in `super::walk::check_common_moving_cancels` — the
/// decomp's `return TRUE` propagation runs the new action the same tick
/// (except the next-tick drops, which the shared helper owns).
fn moving_prelude(cx: &mut ActionCx) -> Option<ActionResult> {
    super::walk::check_common_moving_cancels(cx)
}

/// Stationary-group preamble: water plunge, squish, death.
/// Canonical cancel copy in `super::idle_crouch::check_common_stationary_cancels`;
/// the stationary executor also runs `mario_update_quicksand(m, 0.5f)`
/// (stationary.c:1109) after it.
fn stationary_prelude(cx: &mut ActionCx) -> Option<ActionResult> {
    if let Some(r) = super::idle_crouch::check_common_stationary_cancels(cx) {
        return Some(r);
    }
    // decomp: if (mario_update_quicksand(m, 0.5f)) return TRUE.
    if crate::step::mario_update_quicksand(&mut cx.state, 0.5) {
        return Some(cx.goto(ActionId::QUICKSAND_DEATH, 0));
    }
    None
}

/// decomp: `common_landing_cancels` (src/game/mario_actions_moving.c:1758).
///
/// NOTE (decomp comment): everything here, including floor steepness, is
/// checked before checking whether Mario is actually on the floor — this
/// leads to e.g. remote sliding.
fn common_landing_cancels(
    cx: &mut ActionCx,
    _prev_buttons: u16,
    land: &LandingAction,
    set_apress_action: APressFn,
) -> Option<ActionResult> {
    // decomp: m->floor->normal.y < 0.2923717f. The floor is guaranteed
    // non-null by the geometry stage; 1.0 is the flat-floor fallback.
    if cx.state.floor.map(|f| f.normal.y).unwrap_or(1.0) < 0.2923717 {
        // decomp: return mario_push_off_steep_floor(m, verySteepAction, 0) —
        // the push sets speed/yaw and the set_mario_action return (TRUE)
        // propagates, so the new action runs the same tick.
        crate::step::mario_push_off_steep_floor(&mut cx.state);
        let id = entry::set_mario_action_moving(cx, land.very_steep_action);
        return Some(cx.goto(id, 0));
    }

    cx.state.double_jump_timer = land.unk02 as u32;

    if super::walk::should_begin_sliding(cx) {
        // decomp: return set_mario_action(m, landingAction->slideAction, 0) —
        // the moving-group routing remaps BEGIN_SLIDING.
        let id = entry::set_mario_action_moving(cx, land.slide_action);
        return Some(cx.goto(id, 0));
    }

    if cx.state.input & input_flags::FIRST_PERSON != 0 {
        return Some(cx.goto(land.end_action, 0));
    }

    // NOTE: the timer increments BEFORE the A check (decomp order).
    cx.state.action_timer += 1;
    if cx.state.action_timer >= land.num_frames as u32 {
        return Some(cx.goto(land.end_action, 0));
    }

    if cx.state.input & input_flags::A_PRESSED != 0 {
        return Some(set_apress_action(cx, land.a_pressed_action, 0));
    }

    if cx.state.input & input_flags::OFF_FLOOR != 0 {
        // decomp: return set_mario_action(m, landingAction->offFloorAction, 0)
        // (TRUE → same tick).
        return Some(cx.goto(land.off_floor_action, 0));
    }

    None
}

/// decomp: `common_landing_action` (src/game/mario_actions_moving.c:1722).
fn common_landing_action(cx: &mut ActionCx, anim: i32, air_action: ActionId) -> ActionResult {
    if cx.state.input & input_flags::NONZERO_ANALOG != 0 {
        super::walk::apply_landing_accel(cx, 0.98);
    } else if cx.state.forward_speed >= 16.0 {
        // decomp ignores the return value here.
        let _ = super::walk::apply_slope_decel(cx, 2.0);
    } else {
        // decomp: only vel[1] is touched; horizontal speed is left alone.
        cx.state.vel.y = 0.0;
    }

    match crate::step::perform_ground_step(&mut cx.state, cx.world) {
        GroundStep::LeftGround => {
            // decomp: set_mario_action(m, airAction, 0), then the action
            // returns FALSE — the new action first runs next tick.
            return cx.goto_next(air_action, 0);
        }
        GroundStep::HitWall => {
            // decomp: set_mario_animation(m, MARIO_ANIM_PUSHING); the landing
            // animation request below overwrites it in the same frame.
            cx.set_mario_animation(anim_id::MARIO_ANIM_PUSHING);
        }
        GroundStep::None => {}
    }

    // decomp: PARTICLE_DUST when forwardVel > 16.0f — particle flag, skipped.
    cx.set_mario_animation(anim);
    // decomp: play_mario_landing_sound_once — audio skipped, but the
    // MARIO_ACTION_SOUND_PLAYED flag is real compared state.
    crate::sound::play_mario_landing_sound_once(&mut cx.state);

    // decomp: if (m->floor->type >= SURFACE_SHALLOW_QUICKSAND &&
    //                m->floor->type <= SURFACE_MOVING_QUICKSAND)
    if let Some(f) = cx.state.floor {
        if f.surface_type >= surface_type::SHALLOW_QUICKSAND
            && f.surface_type <= surface_type::MOVING_QUICKSAND
        {
            // C unsigned arithmetic: (4u - actionTimer) wraps when the timer
            // exceeds 4 (reachable on the 6-frame long-jump landing); mirror
            // it rather than using signed math.
            let dt = 4u32.wrapping_sub(cx.state.action_timer);
            cx.state.quicksand_depth += dt as f32 * 3.5 - 0.5;
        }
    }

    ActionResult::Stay
}

/// decomp: `landing_step` (src/game/mario_actions_stationary.c:830).
fn landing_step(cx: &mut ActionCx, anim: i32, end_action: ActionId) -> ActionResult {
    crate::step::stationary_ground_step(&mut cx.state, cx.world);
    cx.set_mario_animation(anim);
    if cx.is_anim_at_end() {
        // decomp: `return set_mario_action(m, action, 0)` — but `landing_step`
        // itself returns FALSE (mario_actions_moving.c), so the new action
        // runs NEXT tick, not same-tick. Uses goto_next.
        let id = entry::set_mario_action_moving(cx, end_action);
        return cx.goto_next(id, 0);
    }
    ActionResult::Stay
}

/// decomp: `check_common_action_exits` (src/game/mario.c:1117).
///
/// The port's `entry::check_common_action_exits` reads the A *button edge*
/// via `cx.pressed`; the decomp reads the `m->input` *flag* for all four
/// exits, which matters because the backflip/long-jump land stops mask
/// `INPUT_A_PRESSED` out of the flags. This copy keeps the flag reads.
fn check_common_action_exits(cx: &mut ActionCx) -> Option<ActionResult> {
    let input = cx.state.input;
    if input & input_flags::A_PRESSED != 0 {
        // decomp: set_mario_action(m, ACT_JUMP, 0) — the airborne routing
        // applies the jump-entry velocities; the port owns those in
        // `air::enter_jump`.
        return Some(super::air::enter_jump(cx));
    }
    if input & input_flags::OFF_FLOOR != 0 {
        return Some(cx.goto(ActionId::FREEFALL, 0));
    }
    if input & input_flags::NONZERO_ANALOG != 0 {
        // decomp: set_mario_action(m, ACT_WALKING, 0) — the moving-group
        // routing runs set_mario_action_moving.
        let id = entry::set_mario_action_moving(cx, ActionId::WALKING);
        return Some(cx.goto(id, 0));
    }
    if input & input_flags::ABOVE_SLIDE != 0 {
        let id = entry::set_mario_action_moving(cx, ActionId::BEGIN_SLIDING);
        return Some(cx.goto(id, 0));
    }
    None
}

/// decomp: `check_common_landing_cancels`
/// (src/game/mario_actions_stationary.c:839).
/// `a_action`: `None` selects `set_jump_from_landing` (the decomp passes 0);
/// `Some(id)` selects `set_jumping_action(m, id, 0)`.
fn check_common_landing_cancels(
    cx: &mut ActionCx,
    _prev_buttons: u16,
    a_action: Option<ActionId>,
) -> Option<ActionResult> {
    let input = cx.state.input;
    if input & input_flags::STOMPED != 0 {
        return Some(cx.goto(ActionId::SHOCKWAVE_BOUNCE, 0));
    }
    if input & input_flags::FIRST_PERSON != 0 {
        return Some(cx.goto(ActionId::IDLE, 0));
    }
    if input & input_flags::A_PRESSED != 0 {
        return Some(match a_action {
            None => entry::set_jump_from_landing(cx),
            Some(id) => set_jumping_action(cx, id, 0),
        });
    }
    if input
        & (input_flags::NONZERO_ANALOG
            | input_flags::A_PRESSED
            | input_flags::OFF_FLOOR
            | input_flags::ABOVE_SLIDE)
        != 0
    {
        return check_common_action_exits(cx);
    }
    if input & input_flags::B_PRESSED != 0 {
        return Some(cx.goto(ActionId::PUNCHING, 0));
    }
    None
}

// ---------------------------------------------------------------------------
// Moving landing actions
// ---------------------------------------------------------------------------

/// Single-jump landing.
/// decomp: `act_jump_land` (src/game/mario_actions_moving.c:1792).
pub struct JumpLand;
impl ActionHandler for JumpLand {
    fn name(&self) -> &'static str {
        "JumpLand"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = moving_prelude(cx) {
            return r;
        }
        if let Some(r) =
            common_landing_cancels(cx, prev_buttons, &S_JUMP_LAND_ACTION, set_jumping_action)
        {
            return r;
        }
        common_landing_action(cx, anim_id::MARIO_ANIM_LAND_FROM_SINGLE_JUMP, ActionId::FREEFALL)
    }
}

/// Freefall landing.
/// decomp: `act_freefall_land` (src/game/mario_actions_moving.c:1801).
pub struct FreefallLand;
impl ActionHandler for FreefallLand {
    fn name(&self) -> &'static str {
        "FreefallLand"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = moving_prelude(cx) {
            return r;
        }
        if let Some(r) =
            common_landing_cancels(cx, prev_buttons, &S_FREEFALL_LAND_ACTION, set_jumping_action)
        {
            return r;
        }
        common_landing_action(cx, anim_id::MARIO_ANIM_GENERAL_LAND, ActionId::FREEFALL)
    }
}

/// Side-flip landing.
/// decomp: `act_side_flip_land` (src/game/mario_actions_moving.c:1810).
pub struct SideFlipLand;
impl ActionHandler for SideFlipLand {
    fn name(&self) -> &'static str {
        "SideFlipLand"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = moving_prelude(cx) {
            return r;
        }
        if let Some(r) =
            common_landing_cancels(cx, prev_buttons, &S_SIDE_FLIP_LAND_ACTION, set_jumping_action)
        {
            return r;
        }
        // decomp: when the ground step did NOT hit a wall,
        // m->marioObj->header.gfx.angle[1] += 0x8000 — render-model rotation
        // only, skipped.
        common_landing_action(cx, anim_id::MARIO_ANIM_SLIDEFLIP_LAND, ActionId::FREEFALL)
    }
}

/// Long-jump landing.
/// decomp: `act_long_jump_land` (src/game/mario_actions_moving.c:1847).
/// US branch: the `#if defined(VERSION_SH) || defined(VERSION_CN)` BLJ clamp
/// (`forwardVel < 0 → 0`) does not apply (oracle builds VERSION_US).
pub struct LongJumpLand;
impl ActionHandler for LongJumpLand {
    fn name(&self) -> &'static str {
        "LongJumpLand"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = moving_prelude(cx) {
            return r;
        }
        // decomp: if (!(m->input & INPUT_Z_DOWN)) m->input &= ~INPUT_A_PRESSED.
        if cx.state.input & input_flags::Z_DOWN == 0 {
            cx.state.input &= !input_flags::A_PRESSED;
        }
        if let Some(r) =
            common_landing_cancels(cx, prev_buttons, &S_LONG_JUMP_LAND_ACTION, set_jumping_action)
        {
            return r;
        }
        // decomp: play_sound_if_no_flag(SOUND_MARIO_UH2_2, MARIO_MARIO_SOUND_PLAYED)
        // when the stick is released — audio skipped, flag is real state.
        crate::sound::play_sound_if_no_flag(
            &mut cx.state,
            crate::state::mario_flags::MARIO_SOUND_PLAYED,
        );
        // The FAST/SLOW anim picks on oMarioLongJumpIsSlow (not modeled in
        // the port); the fast variant is used.
        common_landing_action(
            cx,
            anim_id::MARIO_ANIM_CROUCH_FROM_FAST_LONGJUMP,
            ActionId::FREEFALL,
        )
    }
}

/// Double-jump landing.
/// decomp: `act_double_jump_land` (src/game/mario_actions_moving.c:1874).
pub struct DoubleJumpLand;
impl ActionHandler for DoubleJumpLand {
    fn name(&self) -> &'static str {
        "DoubleJumpLand"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = moving_prelude(cx) {
            return r;
        }
        if let Some(r) = common_landing_cancels(
            cx,
            prev_buttons,
            &S_DOUBLE_JUMP_LAND_ACTION,
            set_triple_jump_action,
        ) {
            return r;
        }
        common_landing_action(cx, anim_id::MARIO_ANIM_LAND_FROM_DOUBLE_JUMP, ActionId::FREEFALL)
    }
}

/// Triple-jump landing.
/// decomp: `act_triple_jump_land` (src/game/mario_actions_moving.c:1882).
pub struct TripleJumpLand;
impl ActionHandler for TripleJumpLand {
    fn name(&self) -> &'static str {
        "TripleJumpLand"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = moving_prelude(cx) {
            return r;
        }
        // decomp: m->input &= ~INPUT_A_PRESSED (unconditional).
        cx.state.input &= !input_flags::A_PRESSED;
        if let Some(r) =
            common_landing_cancels(cx, prev_buttons, &S_TRIPLE_JUMP_LAND_ACTION, set_jumping_action)
        {
            return r;
        }
        // decomp: play_sound_if_no_flag(SOUND_MARIO_HAHA, MARIO_MARIO_SOUND_PLAYED)
        // when the stick is released — audio skipped, flag is real state.
        crate::sound::play_sound_if_no_flag(
            &mut cx.state,
            crate::state::mario_flags::MARIO_SOUND_PLAYED,
        );
        common_landing_action(cx, anim_id::MARIO_ANIM_TRIPLE_JUMP_LAND, ActionId::FREEFALL)
    }
}

/// Backflip landing.
/// decomp: `act_backflip_land` (src/game/mario_actions_moving.c:1897).
pub struct BackflipLand;
impl ActionHandler for BackflipLand {
    fn name(&self) -> &'static str {
        "BackflipLand"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = moving_prelude(cx) {
            return r;
        }
        // decomp: if (!(m->input & INPUT_Z_DOWN)) m->input &= ~INPUT_A_PRESSED.
        if cx.state.input & input_flags::Z_DOWN == 0 {
            cx.state.input &= !input_flags::A_PRESSED;
        }
        if let Some(r) =
            common_landing_cancels(cx, prev_buttons, &S_BACKFLIP_LAND_ACTION, set_jumping_action)
        {
            return r;
        }
        // decomp: play_sound_if_no_flag(SOUND_MARIO_HAHA, MARIO_MARIO_SOUND_PLAYED)
        // when the stick is released — audio skipped, flag is real state.
        crate::sound::play_sound_if_no_flag(
            &mut cx.state,
            crate::state::mario_flags::MARIO_SOUND_PLAYED,
        );
        common_landing_action(cx, anim_id::MARIO_ANIM_TRIPLE_JUMP_LAND, ActionId::FREEFALL)
    }
}

// ---------------------------------------------------------------------------
// Stationary landing stops + ground pound land
// ---------------------------------------------------------------------------

/// Single-jump land stop.
/// decomp: `act_jump_land_stop` (src/game/mario_actions_stationary.c:867).
pub struct JumpLandStop;
impl ActionHandler for JumpLandStop {
    fn name(&self) -> &'static str {
        "JumpLandStop"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = stationary_prelude(cx) {
            return r;
        }
        if let Some(r) = check_common_landing_cancels(cx, prev_buttons, None) {
            return r;
        }
        landing_step(cx, anim_id::MARIO_ANIM_LAND_FROM_SINGLE_JUMP, ActionId::IDLE)
    }
}

/// Double-jump land stop.
/// decomp: `act_double_jump_land_stop`
/// (src/game/mario_actions_stationary.c:876).
pub struct DoubleJumpLandStop;
impl ActionHandler for DoubleJumpLandStop {
    fn name(&self) -> &'static str {
        "DoubleJumpLandStop"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = stationary_prelude(cx) {
            return r;
        }
        if let Some(r) = check_common_landing_cancels(cx, prev_buttons, None) {
            return r;
        }
        landing_step(cx, anim_id::MARIO_ANIM_LAND_FROM_DOUBLE_JUMP, ActionId::IDLE)
    }
}

/// Side-flip land stop.
/// decomp: `act_side_flip_land_stop` (src/game/mario_actions_stationary.c:885).
pub struct SideFlipLandStop;
impl ActionHandler for SideFlipLandStop {
    fn name(&self) -> &'static str {
        "SideFlipLandStop"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = stationary_prelude(cx) {
            return r;
        }
        if let Some(r) = check_common_landing_cancels(cx, prev_buttons, None) {
            return r;
        }
        // decomp: m->marioObj->header.gfx.angle[1] += 0x8000 after the step —
        // render-model rotation only, skipped.
        landing_step(cx, anim_id::MARIO_ANIM_SLIDEFLIP_LAND, ActionId::IDLE)
    }
}

/// Freefall land stop.
/// decomp: `act_freefall_land_stop`
/// (src/game/mario_actions_stationary.c:895).
pub struct FreefallLandStop;
impl ActionHandler for FreefallLandStop {
    fn name(&self) -> &'static str {
        "FreefallLandStop"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = stationary_prelude(cx) {
            return r;
        }
        if let Some(r) = check_common_landing_cancels(cx, prev_buttons, None) {
            return r;
        }
        landing_step(cx, anim_id::MARIO_ANIM_GENERAL_LAND, ActionId::IDLE)
    }
}

/// Triple-jump land stop.
/// decomp: `act_triple_jump_land_stop`
/// (src/game/mario_actions_stationary.c:904).
pub struct TripleJumpLandStop;
impl ActionHandler for TripleJumpLandStop {
    fn name(&self) -> &'static str {
        "TripleJumpLandStop"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = stationary_prelude(cx) {
            return r;
        }
        if let Some(r) = check_common_landing_cancels(cx, prev_buttons, Some(ActionId::JUMP)) {
            return r;
        }
        landing_step(cx, anim_id::MARIO_ANIM_TRIPLE_JUMP_LAND, ActionId::IDLE)
    }
}

/// Backflip land stop.
/// decomp: `act_backflip_land_stop`
/// (src/game/mario_actions_stationary.c:913).
pub struct BackflipLandStop;
impl ActionHandler for BackflipLandStop {
    fn name(&self) -> &'static str {
        "BackflipLandStop"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = stationary_prelude(cx) {
            return r;
        }
        // decomp: if (!(m->input & INPUT_Z_DOWN) ||
        //              m->marioObj->header.gfx.animInfo.animFrame >= 6)
        //             m->input &= ~INPUT_A_PRESSED.
        if cx.state.input & input_flags::Z_DOWN == 0 || cx.state.anim.frame >= 6 {
            cx.state.input &= !input_flags::A_PRESSED;
        }
        if let Some(r) = check_common_landing_cancels(cx, prev_buttons, Some(ActionId::BACKFLIP)) {
            return r;
        }
        landing_step(cx, anim_id::MARIO_ANIM_TRIPLE_JUMP_LAND, ActionId::IDLE)
    }
}

/// Long-jump land stop.
/// decomp: `act_long_jump_land_stop`
/// (src/game/mario_actions_stationary.c:937).
pub struct LongJumpLandStop;
impl ActionHandler for LongJumpLandStop {
    fn name(&self) -> &'static str {
        "LongJumpLandStop"
    }
    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        if let Some(r) = stationary_prelude(cx) {
            return r;
        }
        // decomp: m->input &= ~INPUT_B_PRESSED.
        cx.state.input &= !input_flags::B_PRESSED;
        if let Some(r) = check_common_landing_cancels(cx, prev_buttons, Some(ActionId::JUMP)) {
            return r;
        }
        // decomp end action ACT_CROUCHING = the port's ActionId::CROUCH.
        // The FAST/SLOW anim picks on oMarioLongJumpIsSlow (not modeled in
        // the port); the fast variant is used.
        landing_step(
            cx,
            anim_id::MARIO_ANIM_CROUCH_FROM_FAST_LONGJUMP,
            ActionId::CROUCH,
        )
    }
}

/// Ground-pound landing.
/// decomp: `act_ground_pound_land`
/// (src/game/mario_actions_stationary.c:1037).
///
/// NOTE: unlike the other stops, this has no stationary preamble and no
/// `check_common_landing_cancels` — only its own checks plus `landing_step`.
/// The port has no `ActionId::GROUND_POUND_LAND` yet (decomp
/// `ACT_GROUND_POUND_LAND` = 0x0080023C); registry wiring is left to the
/// caller.
pub struct GroundPoundLand;
impl ActionHandler for GroundPoundLand {
    fn name(&self) -> &'static str {
        "GroundPoundLand"
    }
    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        cx.state.action_state = 1;
        let input = cx.state.input;
        if input & input_flags::STOMPED != 0 {
            return entry::drop_and_set_mario_action(cx, ActionId::SHOCKWAVE_BOUNCE, 0);
        }
        if input & input_flags::OFF_FLOOR != 0 {
            return cx.goto(ActionId::FREEFALL, 0);
        }
        if input & input_flags::ABOVE_SLIDE != 0 {
            // decomp: set_mario_action(m, ACT_BUTT_SLIDE, 0) directly.
            let id = entry::set_mario_action_moving(cx, ActionId::BUTT_SLIDE);
            return cx.goto(id, 0);
        }
        landing_step(
            cx,
            anim_id::MARIO_ANIM_GROUND_POUND_LANDING,
            ActionId::BUTT_SLIDE_STOP,
        )
    }
}
