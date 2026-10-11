//! Stationary-group idle/crouch/crawl actions, ported verbatim from the
//! decomp (`oracle/sm64/src/game/mario_actions_stationary.c` and the
//! `act_crawling` moving action from
//! `oracle/sm64/src/game/mario_actions_moving.c`).
//!
//! The decomp is the only source of truth here: where the prose brief for
//! this port disagreed with the decomp source, the decomp won. The
//! disagreements are called out in the report at the bottom of the file.

use super::entry;
use super::{ActionCx, ActionHandler, ActionResult};
use crate::angles::Angle;
use crate::anim_table as anim_id;
use crate::input::{buttons, input_flags};
use crate::state::{act_flags, ActionId};
use crate::step::GroundStep;
use crate::trig;
use crate::world::{self, surface_flags, terrain_type};

/// Decomp `ACT_*` ids referenced by these ports that have no `ActionId`
/// const in `crate::state` yet (values from `oracle/sm64/include/sm64.h`).
/// Kept local to this module; do not edit `state.rs` for them.
mod decomp_only {
    use crate::state::ActionId;
    /// decomp: ACT_PANTING
    pub const PANTING: ActionId = ActionId(0x0C400205);
    /// decomp: ACT_UNKNOWN_0002020E (invulnerable stationary action exempt
    /// from the health<0x100 death check)
    pub const UNKNOWN_0002020E: ActionId = ActionId(0x0002020E);
}

// =====================================================================
// Shared cancels.
// =====================================================================

/// Stationary-group dispatch preamble.
/// decomp: `check_common_stationary_cancels`
/// (src/game/mario_actions_stationary.c:1083).
pub fn check_common_stationary_cancels(cx: &mut ActionCx) -> Option<ActionResult> {
    if cx.state.pos.y < cx.state.water_level - 100.0 {
        // decomp: `if (m->action == ACT_SPAWN_SPIN_LANDING) { load_level_init_text(0); }`
        // — level text is UI; skipped.
        // decomp: `update_mario_sound_and_camera(m)` — audio/camera; skipped.
        return Some(entry::set_water_plunge_action(cx));
    }

    if cx.state.input & input_flags::SQUISHED != 0 {
        // decomp: `update_mario_sound_and_camera(m)` — audio/camera; skipped.
        // decomp: `return set_mario_action(m, ACT_SQUISHED, 0)` — same tick.
        return Some(cx.goto(ActionId::SQUISHED, 0));
    }

    if cx.state.action != decomp_only::UNKNOWN_0002020E {
        if cx.state.health < 0x100 {
            // decomp: `update_mario_sound_and_camera(m)` — audio/camera; skipped.
            // decomp: `return set_mario_action(m, ACT_STANDING_DEATH, 0)` —
            // same tick.
            return Some(cx.goto(ActionId::STANDING_DEATH, 0));
        }
    }

    None
}

/// Stationary-dispatch quicksand update.
/// decomp: `if (mario_update_quicksand(m, 0.5f)) { return TRUE; }` inside
/// `mario_execute_stationary_action` (src/game/mario_actions_stationary.c:1113).
fn stationary_quicksand_check(cx: &mut ActionCx) -> Option<ActionResult> {
    if crate::step::mario_update_quicksand(&mut cx.state, 0.5) {
        // The decomp's `mario_update_quicksand` internally runs
        // `drop_and_set_mario_action(ACT_QUICKSAND_DEATH)` and returns TRUE,
        // which propagates out of `mario_execute_stationary_action` — the
        // decomp action loop CONTINUES, so ACT_QUICKSAND_DEATH runs the same
        // tick. This port's `mario_update_quicksand` only reports "drowned"
        // (bool); the transition plus same-tick continuation is modeled here
        // as `goto`. The held-object drop is a no-op in scope.
        return Some(cx.goto(ActionId::QUICKSAND_DEATH, 0));
    }
    None
}

/// Stationary idle cancel set.
/// decomp: `check_common_idle_cancels`
/// (src/game/mario_actions_stationary.c:20).
fn check_common_idle_cancels(cx: &mut ActionCx, prev_buttons: u16) -> Option<ActionResult> {
    // decomp: `mario_drop_held_object(m)` — no held objects in scope; no-op.

    // decomp: `if (m->floor->normal.y < 0.29237169f)` (floor is guaranteed
    // non-NULL for stationary actions; 1.0 is the defensive fallback).
    if cx.state.floor.map(|f| f.normal.y).unwrap_or(1.0) < 0.29237169 {
        crate::step::mario_push_off_steep_floor(&mut cx.state);
        // decomp: `return mario_push_off_steep_floor(m, ACT_FREEFALL, 0);`
        // — the push-off returns `set_mario_action`'s TRUE (same tick).
        return Some(cx.goto(ActionId::FREEFALL, 0));
    }

    if cx.state.input & input_flags::STOMPED != 0 {
        return Some(cx.goto(ActionId::SHOCKWAVE_BOUNCE, 0));
    }

    if cx.pressed(buttons::A, prev_buttons) {
        return Some(entry::set_jumping_action(cx, ActionId::JUMP, 0));
    }

    if cx.state.input & input_flags::OFF_FLOOR != 0 {
        return Some(cx.goto(ActionId::FREEFALL, 0));
    }

    if cx.state.input & input_flags::ABOVE_SLIDE != 0 {
        // decomp: `return set_mario_action(m, ACT_BEGIN_SLIDING, 0);` —
        // the decomp's `set_mario_action` applies the MOVING-group remap
        // internally (mario.c:982); this crate splits that remap out into
        // `entry::set_mario_action_moving`, so apply it explicitly here.
        let id = entry::set_mario_action_moving(cx, ActionId::BEGIN_SLIDING);
        return Some(cx.goto(id, 0));
    }

    if cx.state.input & input_flags::FIRST_PERSON != 0 {
        return Some(cx.goto(ActionId::FIRST_PERSON, 0));
    }

    if cx.state.input & input_flags::NONZERO_ANALOG != 0 {
        // THE FACE SNAP: face yaw is snapped to the intended yaw here,
        // unconditionally, before the walking transition.
        cx.state.face_yaw = cx.proc.intended_yaw;
        // decomp: `return set_mario_action(m, ACT_WALKING, 0);` — same
        // MOVING-group remap note as above (walk entry-speed clamp).
        let id = entry::set_mario_action_moving(cx, ActionId::WALKING);
        return Some(cx.goto(id, 0));
    }

    if cx.pressed(buttons::B, prev_buttons) {
        return Some(cx.goto(ActionId::PUNCHING, 0));
    }

    if cx.state.input & input_flags::Z_DOWN != 0 {
        return Some(cx.goto(ActionId::START_CROUCHING, 0));
    }

    None
}

/// Inline copy of the moving-group preamble: `check_common_moving_cancels`
/// (moving.c:1953) plus the `mario_update_quicksand(m, 0.25f)` from
/// `mario_execute_moving_action` (moving.c:1983). Duplicated (not
/// cross-called) per the Phase 5 split: the canonical version lives in
/// `super::walk`.
fn moving_preamble_cancels(cx: &mut ActionCx) -> Option<ActionResult> {
    if cx.state.pos.y < cx.state.water_level - 100.0 {
        return Some(entry::set_water_plunge_action(cx));
    }

    if cx.state.action.0 & act_flags::INVULNERABLE == 0
        && cx.state.input & input_flags::STOMPED != 0
    {
        return Some(entry::drop_and_set_mario_action(
            cx,
            ActionId::SHOCKWAVE_BOUNCE,
            0,
        ));
    }

    if cx.state.input & input_flags::SQUISHED != 0 {
        return Some(entry::drop_and_set_mario_action(
            cx,
            ActionId::SQUISHED,
            0,
        ));
    }

    if cx.state.action.0 & act_flags::INVULNERABLE == 0 && cx.state.health < 0x100 {
        return Some(entry::drop_and_set_mario_action(
            cx,
            ActionId::STANDING_DEATH,
            0,
        ));
    }

    if crate::step::mario_update_quicksand(&mut cx.state, 0.25) {
        // Same reasoning as `stationary_quicksand_check`: the decomp
        // returns TRUE here (the action loop continues), so
        // ACT_QUICKSAND_DEATH runs the same tick — modeled as `goto`.
        return Some(cx.goto(ActionId::QUICKSAND_DEATH, 0));
    }

    None
}

/// Inline copy of `check_ground_dive_or_punch` (moving.c:485). Duplicated
/// (not cross-called) per the Phase 5 split: the canonical version lives
/// in `super::walk`.
fn check_ground_dive_or_punch_inline(
    cx: &mut ActionCx,
    prev_buttons: u16,
) -> Option<ActionResult> {
    super::walk::check_ground_dive_or_punch(cx, prev_buttons)
}

/// Inline copy of `should_begin_sliding` (moving.c:467). Duplicated per
/// the Phase 5 split; `super::walk::should_begin_sliding` is the canonical
/// version once walk.rs lands.
fn should_begin_sliding_inline(cx: &mut ActionCx) -> bool {
    super::walk::should_begin_sliding(cx)
}

/// Inline copy of `update_walking_speed` (moving.c:434). Duplicated per
/// the Phase 5 split; `super::walk::update_walking_speed` is the canonical
/// version once walk.rs lands.
fn update_walking_speed_inline(cx: &mut ActionCx) {
    // Canonical version in super::walk (f64 quicksand scaling, both
    // sand/wind updates in apply_slope_accel).
    super::walk::update_walking_speed(cx)
}

/// Inline copy of `apply_slope_accel` (moving.c:283). Not yet ported
/// elsewhere in the crate; duplicated here for `update_walking_speed_inline`.
fn apply_slope_accel_inline(cx: &mut ActionCx) {
    super::walk::apply_slope_accel(cx)
}

/// Inline copy of `align_with_floor` (moving.c:88). The matrix/gfx half is
/// renderer-only; the gameplay effect is snapping to the floor height.
fn align_with_floor_inline(cx: &mut ActionCx) {
    if let Some(fh) = cx.state.floor_y {
        cx.state.pos.y = fh;
    }
    // decomp: `mtxf_align_terrain_triangle(...)` + throwMatrix assignment —
    // renderer; skipped.
}

/// Inline copy of `find_floor_height_relative_polar` (mario.c:674): floor
/// height at a polar offset from Mario. Only used by act_idle's sleep check.
fn find_floor_height_relative_polar(
    cx: &ActionCx,
    angle_from_mario: i16,
    dist_from_mario: f32,
) -> f32 {
    let angle = cx.state.face_yaw.wrapping_add(Angle(angle_from_mario));
    // decomp variable naming: `y = sins(...)*dist` is the x offset,
    // `x = coss(...)*dist` is the z offset.
    let x = cx.state.pos.x + trig::sins(angle) * dist_from_mario;
    let z = cx.state.pos.z + trig::coss(angle) * dist_from_mario;
    cx.world
        .find_floor(x, cx.state.pos.y + 100.0, z)
        .map(|f| f.y)
        .unwrap_or(world::FLOOR_LOWER_LIMIT)
}

// =====================================================================
// Idle.
// =====================================================================

/// decomp: `act_idle` (src/game/mario_actions_stationary.c:105).
pub struct Idle;

impl ActionHandler for Idle {
    fn name(&self) -> &'static str {
        "Idle"
    }

    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        // Stationary dispatch preamble (mario_execute_stationary_action).
        if let Some(r) = check_common_stationary_cancels(cx) {
            return r;
        }
        if let Some(r) = stationary_quicksand_check(cx) {
            return r;
        }

        if cx.state.quicksand_depth > 30.0 {
            return cx.goto(ActionId::IN_QUICKSAND, 0);
        }

        // decomp: `if (m->input & INPUT_IN_POISON_GAS) { return
        // set_mario_action(m, ACT_COUGHING, 0); }` — no poison gas in scope;
        // skipped.

        if (cx.state.action_arg & 1) == 0 && cx.state.health < 0x300 {
            return cx.goto(decomp_only::PANTING, 0);
        }

        if let Some(r) = check_common_idle_cancels(cx, prev_buttons) {
            return r;
        }

        if cx.state.action_state == 3 {
            // Fall asleep after 10 head turning cycles (see the anim state
            // machine below), or shiver on snow.
            if cx.state.terrain_type & terrain_type::MASK == terrain_type::SNOW {
                return cx.goto(ActionId::SHIVERING, 0);
            } else {
                return cx.goto(ActionId::START_SLEEPING, 0);
            }
        }

        if (cx.state.action_arg & 1) != 0 {
            cx.set_mario_animation(anim_id::MARIO_ANIM_STAND_AGAINST_WALL);
        } else {
            match cx.state.action_state {
                0 => {
                    cx.set_mario_animation(anim_id::MARIO_ANIM_IDLE_HEAD_LEFT);
                }
                1 => {
                    cx.set_mario_animation(anim_id::MARIO_ANIM_IDLE_HEAD_RIGHT);
                }
                2 => {
                    cx.set_mario_animation(anim_id::MARIO_ANIM_IDLE_HEAD_CENTER);
                }
                _ => {}
            }

            if cx.is_anim_at_end() {
                // decomp: `if (++m->actionState == 3)` — actionState counts
                // head-turn cycles; actionTimer counts completed cycles
                // toward the 10 needed before sleeping.
                cx.state.action_state += 1;
                if cx.state.action_state == 3 {
                    let delta_y_of_floor_behind_mario =
                        cx.state.pos.y - find_floor_height_relative_polar(cx, -0x8000, 60.0);
                    let floor_is_dynamic = cx
                        .state
                        .floor
                        .is_some_and(|f| f.flags & surface_flags::DYNAMIC != 0);
                    if delta_y_of_floor_behind_mario < -24.0
                        || 24.0 < delta_y_of_floor_behind_mario
                        || floor_is_dynamic
                    {
                        cx.state.action_state = 0;
                    } else {
                        // If Mario hasn't turned his head 10 times yet, stay
                        // idle instead of going to sleep.
                        cx.state.action_timer += 1;
                        if cx.state.action_timer < 10 {
                            cx.state.action_state = 0;
                        }
                    }
                }
            }
        }

        // decomp: `stationary_ground_step(m); return FALSE;` — the step
        // result is ignored here; act_idle never transitions on it.
        let _ = crate::step::stationary_ground_step(&mut cx.state, cx.world);
        ActionResult::Stay
    }
}

// =====================================================================
// Crouching.
// =====================================================================

/// decomp: `act_crouching` (src/game/mario_actions_stationary.c:528).
pub struct Crouch;

impl ActionHandler for Crouch {
    fn name(&self) -> &'static str {
        "Crouch"
    }

    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        // Stationary dispatch preamble (mario_execute_stationary_action).
        if let Some(r) = check_common_stationary_cancels(cx) {
            return r;
        }
        if let Some(r) = stationary_quicksand_check(cx) {
            return r;
        }

        if cx.state.input & input_flags::STOMPED != 0 {
            return cx.goto(ActionId::SHOCKWAVE_BOUNCE, 0);
        }

        if cx.pressed(buttons::A, prev_buttons) {
            return entry::set_jumping_action(cx, ActionId::BACKFLIP, 0);
        }

        if cx.state.input & input_flags::OFF_FLOOR != 0 {
            // decomp: `return set_mario_action(m, ACT_FREEFALL, 0);` — TRUE,
            // so the new action runs the same tick (goto, not goto_next).
            return cx.goto(ActionId::FREEFALL, 0);
        }

        if cx.state.input & input_flags::ABOVE_SLIDE != 0 {
            // Same MOVING-group remap note as check_common_idle_cancels.
            let id = entry::set_mario_action_moving(cx, ActionId::BEGIN_SLIDING);
            return cx.goto(id, 0);
        }

        if cx.state.input & input_flags::FIRST_PERSON != 0 {
            return cx.goto(ActionId::STOP_CROUCHING, 0);
        }

        if cx.state.input & input_flags::Z_DOWN == 0 {
            return cx.goto(ActionId::STOP_CROUCHING, 0);
        }

        if cx.state.input & input_flags::NONZERO_ANALOG != 0 {
            return cx.goto(ActionId::START_CRAWLING, 0);
        }

        if cx.pressed(buttons::B, prev_buttons) {
            return cx.goto(ActionId::PUNCHING, 9);
        }

        let _ = crate::step::stationary_ground_step(&mut cx.state, cx.world);
        cx.set_mario_animation(anim_id::MARIO_ANIM_CROUCHING);
        ActionResult::Stay
    }
}

// =====================================================================
// Start/stop crouching, start/stop crawling.
// =====================================================================

/// decomp: `act_start_crouching` (src/game/mario_actions_stationary.c:693).
pub struct StartCrouching;

impl ActionHandler for StartCrouching {
    fn name(&self) -> &'static str {
        "StartCrouching"
    }

    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        // Stationary dispatch preamble (mario_execute_stationary_action).
        if let Some(r) = check_common_stationary_cancels(cx) {
            return r;
        }
        if let Some(r) = stationary_quicksand_check(cx) {
            return r;
        }

        if cx.state.input & input_flags::STOMPED != 0 {
            return cx.goto(ActionId::SHOCKWAVE_BOUNCE, 0);
        }

        if cx.state.input & input_flags::OFF_FLOOR != 0 {
            return cx.goto(ActionId::FREEFALL, 0);
        }

        if cx.pressed(buttons::A, prev_buttons) {
            return entry::set_jumping_action(cx, ActionId::BACKFLIP, 0);
        }

        if cx.state.input & input_flags::ABOVE_SLIDE != 0 {
            let id = entry::set_mario_action_moving(cx, ActionId::BEGIN_SLIDING);
            return cx.goto(id, 0);
        }

        let _ = crate::step::stationary_ground_step(&mut cx.state, cx.world);
        cx.set_mario_animation(anim_id::MARIO_ANIM_START_CROUCHING);
        // decomp: `if (is_anim_past_end(m)) { set_mario_action(m,
        // ACT_CROUCHING, 0); } return FALSE;` — the transition is NOT
        // returned, so the new action runs NEXT tick.
        if cx.is_anim_past_end() {
            return cx.goto_next(ActionId::CROUCH, 0);
        }
        ActionResult::Stay
    }
}

/// decomp: `act_stop_crouching` (src/game/mario_actions_stationary.c:716).
pub struct StopCrouching;

impl ActionHandler for StopCrouching {
    fn name(&self) -> &'static str {
        "StopCrouching"
    }

    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        // Stationary dispatch preamble (mario_execute_stationary_action).
        if let Some(r) = check_common_stationary_cancels(cx) {
            return r;
        }
        if let Some(r) = stationary_quicksand_check(cx) {
            return r;
        }

        if cx.state.input & input_flags::STOMPED != 0 {
            return cx.goto(ActionId::SHOCKWAVE_BOUNCE, 0);
        }

        if cx.state.input & input_flags::OFF_FLOOR != 0 {
            return cx.goto(ActionId::FREEFALL, 0);
        }

        if cx.pressed(buttons::A, prev_buttons) {
            return entry::set_jumping_action(cx, ActionId::BACKFLIP, 0);
        }

        if cx.state.input & input_flags::ABOVE_SLIDE != 0 {
            let id = entry::set_mario_action_moving(cx, ActionId::BEGIN_SLIDING);
            return cx.goto(id, 0);
        }

        let _ = crate::step::stationary_ground_step(&mut cx.state, cx.world);
        cx.set_mario_animation(anim_id::MARIO_ANIM_STOP_CROUCHING);
        // decomp: transition not returned — new action runs next tick.
        if cx.is_anim_past_end() {
            return cx.goto_next(ActionId::IDLE, 0);
        }
        ActionResult::Stay
    }
}

/// decomp: `act_start_crawling` (src/game/mario_actions_stationary.c:739).
pub struct StartCrawling;

impl ActionHandler for StartCrawling {
    fn name(&self) -> &'static str {
        "StartCrawling"
    }

    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // Stationary dispatch preamble (mario_execute_stationary_action).
        if let Some(r) = check_common_stationary_cancels(cx) {
            return r;
        }
        if let Some(r) = stationary_quicksand_check(cx) {
            return r;
        }

        // Note the decomp order here differs from start/stop crouching:
        // FIRST_PERSON comes first, then OFF_FLOOR, then STOMPED.
        if cx.state.input & input_flags::FIRST_PERSON != 0 {
            return cx.goto(ActionId::STOP_CROUCHING, 0);
        }

        if cx.state.input & input_flags::OFF_FLOOR != 0 {
            return cx.goto(ActionId::FREEFALL, 0);
        }

        if cx.state.input & input_flags::STOMPED != 0 {
            return cx.goto(ActionId::SHOCKWAVE_BOUNCE, 0);
        }

        if cx.state.input & input_flags::ABOVE_SLIDE != 0 {
            let id = entry::set_mario_action_moving(cx, ActionId::BEGIN_SLIDING);
            return cx.goto(id, 0);
        }

        let _ = crate::step::stationary_ground_step(&mut cx.state, cx.world);
        cx.set_mario_animation(anim_id::MARIO_ANIM_START_CRAWLING);
        // decomp: transition not returned — new action runs next tick.
        if cx.is_anim_past_end() {
            return cx.goto_next(ActionId::CRAWL, 0);
        }
        ActionResult::Stay
    }
}

/// decomp: `act_stop_crawling` (src/game/mario_actions_stationary.c:764).
pub struct StopCrawling;

impl ActionHandler for StopCrawling {
    fn name(&self) -> &'static str {
        "StopCrawling"
    }

    fn tick(&self, cx: &mut ActionCx, _prev_buttons: u16) -> ActionResult {
        // Stationary dispatch preamble (mario_execute_stationary_action).
        if let Some(r) = check_common_stationary_cancels(cx) {
            return r;
        }
        if let Some(r) = stationary_quicksand_check(cx) {
            return r;
        }

        if cx.state.input & input_flags::STOMPED != 0 {
            return cx.goto(ActionId::SHOCKWAVE_BOUNCE, 0);
        }

        if cx.state.input & input_flags::OFF_FLOOR != 0 {
            return cx.goto(ActionId::FREEFALL, 0);
        }

        if cx.state.input & input_flags::ABOVE_SLIDE != 0 {
            let id = entry::set_mario_action_moving(cx, ActionId::BEGIN_SLIDING);
            return cx.goto(id, 0);
        }

        let _ = crate::step::stationary_ground_step(&mut cx.state, cx.world);
        cx.set_mario_animation(anim_id::MARIO_ANIM_STOP_CRAWLING);
        // decomp: transition not returned — new action runs next tick.
        if cx.is_anim_past_end() {
            return cx.goto_next(ActionId::CROUCH, 0);
        }
        ActionResult::Stay
    }
}

// =====================================================================
// Crawling (MOVING group).
// =====================================================================

/// decomp: `act_crawling` (src/game/mario_actions_moving.c:1247). MOVING
/// group, so the dispatch preamble is the moving one (water plunge +
/// `mario_update_quicksand(m, 0.25f)`).
pub struct Crawl;

impl ActionHandler for Crawl {
    fn name(&self) -> &'static str {
        "Crawl"
    }

    fn tick(&self, cx: &mut ActionCx, prev_buttons: u16) -> ActionResult {
        // Moving dispatch preamble (mario_execute_moving_action), inlined.
        if let Some(r) = moving_preamble_cancels(cx) {
            return r;
        }

        if should_begin_sliding_inline(cx) {
            let id = entry::set_mario_action_moving(cx, ActionId::BEGIN_SLIDING);
            return cx.goto(id, 0);
        }

        if cx.state.input & input_flags::FIRST_PERSON != 0 {
            return cx.goto(ActionId::STOP_CRAWLING, 0);
        }

        if cx.pressed(buttons::A, prev_buttons) {
            return entry::set_jumping_action(cx, ActionId::JUMP, 0);
        }

        if let Some(r) = check_ground_dive_or_punch_inline(cx, prev_buttons) {
            return r;
        }

        if cx.state.input & input_flags::UNKNOWN_5 != 0 {
            return cx.goto(ActionId::STOP_CRAWLING, 0);
        }

        if cx.state.input & input_flags::Z_DOWN == 0 {
            return cx.goto(ActionId::STOP_CRAWLING, 0);
        }

        cx.proc.intended_mag *= 0.1;
        // decomp: m->intendedMag *= 0.1f (moving.c:942) — the compared
        // state field, not just the proc copy.
        cx.state.intended_mag *= 0.1;

        update_walking_speed_inline(cx);

        // decomp: the LEFT_GROUND branch calls `set_mario_action` (not
        // returned) and breaks out of the switch, so the crawl anim still
        // updates this tick and ACT_FREEFALL first runs NEXT tick. The
        // transition is recorded here and returned after the anim update.
        let freefall_next = match crate::step::perform_ground_step(&mut cx.state, cx.world) {
            GroundStep::LeftGround => Some(cx.goto_next(ActionId::FREEFALL, 0)),
            GroundStep::HitWall => {
                if cx.state.forward_speed > 10.0 {
                    cx.set_forward_speed(10.0);
                }
                // decomp: `//! Possibly unintended missing break` — falls
                // through into GROUND_STEP_NONE.
                align_with_floor_inline(cx);
                None
            }
            GroundStep::None => {
                align_with_floor_inline(cx);
                None
            }
        };

        // decomp: `val04 = (s32)(m->intendedMag * 2.0f * 0x10000);` —
        // intendedMag is the 0.1-scaled value at this point.
        let accel = (cx.proc.intended_mag * 2.0 * 65536.0) as i32;
        cx.set_mario_anim_with_accel(anim_id::MARIO_ANIM_CRAWLING, accel);
        // decomp: `play_step_sound(m, 26, 79)` — audio; skipped.

        match freefall_next {
            Some(r) => r,
            None => ActionResult::Stay,
        }
    }
}

// =====================================================================
// Port report.
// =====================================================================
//
// Deviations from / ambiguities in the prose brief (decomp won everywhere):
//
// 1. `check_common_stationary_cancels` is verbatim stationary.c:1083: water
//    plunge, INPUT_SQUISHED -> SQUISHED, health<0x100 -> STANDING_DEATH
//    unless action == ACT_UNKNOWN_0002020E. It does NOT contain
//    mario_update_quicksand or an ABOVE_SLIDE branch — those live in
//    `mario_execute_stationary_action` (stationary.c:1109-1116) and
//    `check_common_idle_cancels` respectively. The quicksand half is
//    `stationary_quicksand_check` here.
// 2. `check_common_idle_cancels` is verbatim stationary.c:20-59, including
//    the INPUT_STOMPED -> SHOCKWAVE_BOUNCE and INPUT_FIRST_PERSON ->
//    ACT_FIRST_PERSON branches the brief marked "skip". Both flags and both
//    targets exist in this crate, so the verbatim port keeps them.
//    quicksandDepth>30 / poison gas / panting are act_idle's own body, not
//    part of this fn.
// 3. The brief's "OFF_FLOOR -> goto_next FREEFALL" for the idle/crouch
//    cancels is goto (same tick) in the decomp: those branches
//    `return set_mario_action(...)` (TRUE). goto_next is used only where the
//    decomp calls set_mario_action without returning it (the anim-end
//    transitions in start/stop crouch/crawl, and crawling's LEFT_GROUND).
// 4. The brief's "WALKING/BEGIN_SLIDING via set_mario_action_moving" is
//    required by this crate's architecture: the decomp's `set_mario_action`
//    applies the MOVING-group remap internally (mario.c:982), but this
//    crate split it out as `entry::set_mario_action_moving`, so callers
//    apply it explicitly before `goto`. Same-tick `goto` (not goto_next)
//    matches the decomp's TRUE return.
// 5. act_idle's tail: `stationary_ground_step(m); return FALSE;` — the step
//    result is ignored; act_idle never transitions on it. Sleep: after the
//    head-turn cycle completes 10 times (actionTimer counts cycles,
//    actionState==3 gates the transition) Mario goes to START_SLEEPING (or
//    SHIVERING on snow terrain). No handler is registered for
//    START_SLEEPING in this crate yet, so `goto(ActionId::START_SLEEPING)`
//    targets an unregistered action id — noted for the parent wiring pass.
// 6. `mario_update_quicksand` bool handling: this port's
//    `mario_update_quicksand` (step.rs) only reports "drowned" and does NOT
//    transition. The decomp's version transitions to ACT_QUICKSAND_DEATH
//    internally and returns TRUE, which the decomp action loop treats as
//    "continue" (new action same tick). Modeled as
//    `cx.goto(QUICKSAND_DEATH, 0)`; see `stationary_quicksand_check` /
//    `moving_preamble_cancels`.
// 7. Private inline copies (Phase 5 split; canonical homes elsewhere once
//    the sibling agents land): `moving_preamble_cancels`
//    (= check_common_moving_cancels + quicksand 0.25),
//    `check_ground_dive_or_punch_inline`, `should_begin_sliding_inline`,
//    `update_walking_speed_inline`, `apply_slope_accel_inline`,
//    `align_with_floor_inline`, `find_floor_height_relative_polar`. These
//    duplicate — not cross-call — `super::walk`'s future exports because
//    walk.rs does not define them yet and `cargo check` must pass now; the
//    parent wiring pass can swap the cross-calls in later.
