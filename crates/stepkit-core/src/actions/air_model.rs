//! Air velocity model and shared step plumbing.
//!
//! Verbatim ports from the decomp (`src/game/mario_actions_airborne.c`).
//! The key insight (plan AR-01): `forwardVel` is a persistent scalar;
//! sideways speed is recomputed every frame from the stick and is NOT
//! carried over. Facing is not changed by `update_air_without_turn`.

use super::{ActionCx, ActionResult};
use crate::angles::Angle;
use crate::input::input_flags;
use crate::math_util::approach_f32;
use crate::trig::{atan2s, coss, sins};
use crate::state::{mario_flags, ActionId};
use crate::step::{perform_air_step, AirStep};
use crate::world::surface_type;

/// decomp: `check_horizontal_wind` (`src/game/mario_actions_airborne.c:149`).
/// Returns true when a horizontal wind surface overrode the velocity.
pub fn check_horizontal_wind(cx: &mut ActionCx) -> bool {
    let floor = match cx.state.floor {
        Some(f) => f,
        None => return false,
    };
    if floor.surface_type as u16 != surface_type::HORIZONTAL_WIND {
        return false;
    }
    let push_angle = Angle(((floor.force as i32) << 8) as i16);
    cx.state.slide_vel_x += 1.2 * sins(push_angle);
    cx.state.slide_vel_z += 1.2 * coss(push_angle);

    let mut speed = (cx.state.slide_vel_x * cx.state.slide_vel_x
        + cx.state.slide_vel_z * cx.state.slide_vel_z)
        .sqrt();
    if speed > 48.0 {
        cx.state.slide_vel_x = cx.state.slide_vel_x * 48.0 / speed;
        cx.state.slide_vel_z = cx.state.slide_vel_z * 48.0 / speed;
        speed = 32.0; // decomp: "This was meant to be 48?"
    } else if speed > 32.0 {
        speed = 32.0;
    }

    cx.state.vel.x = cx.state.slide_vel_x;
    cx.state.vel.z = cx.state.slide_vel_z;
    cx.state.slide_yaw = atan2s(
        cx.state.slide_vel_z,
        cx.state.slide_vel_x,
    );
    cx.state.forward_speed = speed * coss(Angle(cx.state.face_yaw.0.wrapping_sub(cx.state.slide_yaw.0)));
    // decomp: play_sound(SOUND_ENV_WIND2) on JP — audio, skipped (no flag).
    true
}

/// decomp: `update_air_with_turn` (`src/game/mario_actions_airborne.c:186`).
/// Used by butt-slide-air (and hold-butt-slide-air).
pub fn update_air_with_turn(cx: &mut ActionCx) {
    if check_horizontal_wind(cx) {
        return;
    }
    let drag_threshold = if cx.state.action == ActionId::LONG_JUMP {
        48.0
    } else {
        32.0
    };
    cx.state.forward_speed = approach_f32(cx.state.forward_speed, 0.0, 0.35, 0.35);

    if cx.state.input & input_flags::NONZERO_ANALOG != 0 {
        let intended_dyaw = cx.state.intended_yaw.0.wrapping_sub(cx.state.face_yaw.0);
        let intended_mag = cx.state.intended_mag / 32.0;
        cx.state.forward_speed += 1.5 * coss(Angle(intended_dyaw)) * intended_mag;
        cx.state.face_yaw = Angle(
            cx.state
                .face_yaw
                .0
                .wrapping_add((512.0 * sins(Angle(intended_dyaw)) * intended_mag) as i16),
        );
    }

    // decomp: "Uncapped air speed. Net positive when moving forward."
    if cx.state.forward_speed > drag_threshold {
        cx.state.forward_speed -= 1.0;
    }
    if cx.state.forward_speed < -16.0 {
        cx.state.forward_speed += 2.0;
    }

    cx.state.slide_vel_x = cx.state.forward_speed * sins(cx.state.face_yaw);
    cx.state.slide_vel_z = cx.state.forward_speed * coss(cx.state.face_yaw);
    cx.state.vel.x = cx.state.slide_vel_x;
    cx.state.vel.z = cx.state.slide_vel_z;
}

/// decomp: `update_air_without_turn` (`src/game/mario_actions_airborne.c:217`).
/// Used by (nearly) every air action via `common_air_action_step`.
pub fn update_air_without_turn(cx: &mut ActionCx) {
    let mut sideways_speed = 0.0f32;
    if check_horizontal_wind(cx) {
        return;
    }
    let drag_threshold = if cx.state.action == ActionId::LONG_JUMP {
        48.0
    } else {
        32.0
    };
    cx.state.forward_speed = approach_f32(cx.state.forward_speed, 0.0, 0.35, 0.35);

    if cx.state.input & input_flags::NONZERO_ANALOG != 0 {
        let intended_dyaw = cx.state.intended_yaw.0.wrapping_sub(cx.state.face_yaw.0);
        let intended_mag = cx.state.intended_mag / 32.0;
        cx.state.forward_speed += intended_mag * coss(Angle(intended_dyaw)) * 1.5;
        sideways_speed = intended_mag * sins(Angle(intended_dyaw)) * 10.0;
    }

    // decomp: "Uncapped air speed. Net positive when moving forward."
    if cx.state.forward_speed > drag_threshold {
        cx.state.forward_speed -= 1.0;
    }
    if cx.state.forward_speed < -16.0 {
        cx.state.forward_speed += 2.0;
    }

    cx.state.slide_vel_x = cx.state.forward_speed * sins(cx.state.face_yaw);
    cx.state.slide_vel_z = cx.state.forward_speed * coss(cx.state.face_yaw);

    cx.state.slide_vel_x += sideways_speed * sins(Angle(cx.state.face_yaw.0.wrapping_add(0x4000)));
    cx.state.slide_vel_z += sideways_speed * coss(Angle(cx.state.face_yaw.0.wrapping_add(0x4000)));

    cx.state.vel.x = cx.state.slide_vel_x;
    cx.state.vel.z = cx.state.slide_vel_z;
}

/// Result of `common_air_action_step`: the step result plus whether the
/// decomp's action would return TRUE (same-tick loop) or FALSE (next-tick).
pub struct CommonAirOutcome {
    pub step: AirStep,
    /// What the decomp's calling action returns after this helper.
    /// Most callers (`act_jump` etc.) return FALSE unconditionally.
    pub transition_taken: Option<ActionResult>,
}

/// decomp: `common_air_action_step` (`src/game/mario_actions_airborne.c:371`).
///
/// Runs the air model + step, then applies the decomp's switch. When the
/// switch takes a `set_mario_action` transition, the returned
/// `ActionResult` encodes the decomp's same-tick/next-tick behavior:
/// - `AIR_STEP_LANDED` → `set_mario_action(landAction)` — the CALLER returns
///   FALSE, so this is next-tick (`goto_next`).
/// - `AIR_STEP_HIT_WALL` with wall → `ACT_AIR_HIT_WALL` — caller returns
///   FALSE → next-tick.
/// - `AIR_STEP_HIT_WALL` without wall, `forwardVel >= 38` →
///   `ACT_BACKWARD_AIR_KB` — caller returns FALSE → next-tick.
/// - `AIR_STEP_HIT_WALL` without wall, `forwardVel <= 38` →
///   `return set_mario_action(ACT_SOFT_BONK)` — TRUE → same-tick.
/// - `GRABBED_LEDGE` → `drop_and_set_mario_action(ACT_LEDGE_GRAB)` —
///   caller returns FALSE → next-tick.
/// - `GRABBED_CEILING` → `set_mario_action(ACT_START_HANGING)` — caller
///   returns FALSE → next-tick.
pub fn common_air_action_step(
    cx: &mut ActionCx,
    land_action: ActionId,
    animation: i32,
    step_arg: u32,
) -> CommonAirOutcome {
    // NOTE: quicksandDepth zeroing is now centralized in
    // `execute_airborne_action` via `check_common_airborne_cancels`.
    // Do NOT duplicate it here.
    update_air_without_turn(cx);

    let step_result = perform_air_step(&mut cx.state, cx.world, step_arg);
    let transition_taken = match step_result {
        AirStep::None => {
            cx.set_mario_animation(animation);
            None
        }
        AirStep::Landed => {
            cx.state.land_from = cx.state.action;
            if !check_fall_damage_or_get_stuck(cx, ActionId::HARD_BACKWARD_GROUND_KB) {
                let id = super::entry::set_mario_action_moving(cx, land_action);
                Some(cx.goto_next(id, 0))
            } else {
                // decomp: check_fall_damage's TRUE return (via drop_and_set_mario_action)
                // is DISCARDED by the caller (act_freefall etc. return FALSE), so the
                // hard-fall action starts next tick, not same-tick.
                // NOTE: check_fall_damage already performed the transition via
                // cx.goto_next internally. Do NOT do another goto_next here,
                // as that would double-transition and corrupt prev_action.
                // Return Stay to end the tick; the new action runs next tick.
                Some(ActionResult::Stay)
            }
        }
        AirStep::HitWall => {
            cx.set_mario_animation(animation);
            if cx.state.forward_speed > 16.0 {
                crate::step::mario_bonk_reflection(&mut cx.state, false);
                cx.state.face_yaw =
                    Angle(cx.state.face_yaw.0.wrapping_add(0x8000u16 as i16));
                if cx.state.wall.is_some() {
                    Some(cx.goto_next(ActionId::AIR_HIT_WALL, 0))
                } else {
                    if cx.state.vel.y > 0.0 {
                        cx.state.vel.y = 0.0;
                    }
                    // Hands-free holding glitch (decomp comment): bonking
                    // with no wall referenced goes to a non-holding action.
                    if cx.state.forward_speed >= 38.0 {
                        // decomp: particleFlags |= PARTICLE_VERTICAL_STAR
                        // (visual, skipped).
                        let id =
                            super::entry::set_mario_action_airborne(cx, ActionId::BACKWARD_AIR_KB, 0);
                        Some(cx.goto_next(id, 0))
                    } else {
                        if cx.state.forward_speed > 8.0 {
                            crate::step::mario_set_forward_vel(&mut cx.state, -8.0);
                        }
                        let id =
                            super::entry::set_mario_action_airborne(cx, ActionId::SOFT_BONK, 0);
                        // decomp: `return set_mario_action(...)` — the return
                        // value (TRUE) is DISCARDED by the caller
                        // (act_triple_jump etc. return FALSE), so the new
                        // action starts next tick, not same-tick.
                        Some(cx.goto_next(id, 0))
                    }
                }
            } else {
                crate::step::mario_set_forward_vel(&mut cx.state, 0.0);
                None
            }
        }
        AirStep::GrabbedLedge => {
            cx.set_mario_animation(crate::anim_table::MARIO_ANIM_IDLE_ON_LEDGE);
            // decomp: drop_and_set_mario_action(ACT_LEDGE_GRAB, 0).
            // (object drop skipped.)
            let id = super::entry::set_mario_action_airborne(cx, ActionId::LEDGE_GRAB, 0);
            Some(cx.goto_next(id, 0))
        }
        AirStep::GrabbedCeiling => {
            let id = super::entry::set_mario_action_airborne(cx, ActionId::START_HANGING, 0);
            Some(cx.goto_next(id, 0))
        }
        AirStep::HitLavaWall => {
            // decomp: lava_boost_on_wall — out of scope (no lava surfaces
            // in the scenario set).
            None
        }
    };
    CommonAirOutcome {
        step: step_result,
        transition_taken,
    }
}

/// decomp: `check_kick_or_dive_in_air`
/// (`src/game/mario_actions_airborne.c:108`).
/// Returns the transition when taken (decomp: returns TRUE → same-tick).
/// NOTE: only B_PRESSED; DIVE iff forwardVel > 28 else JUMP_KICK.
pub fn check_kick_or_dive_in_air(cx: &mut ActionCx) -> Option<ActionResult> {
    if cx.state.input & input_flags::B_PRESSED != 0 {
        let next = if cx.state.forward_speed > 28.0 {
            ActionId::DIVE
        } else {
            ActionId::JUMP_KICK
        };
        let id = super::entry::set_mario_action_airborne(cx, next, 0);
        return Some(cx.goto(id, 0));
    }
    None
}

/// decomp: `check_fall_damage` (`src/game/mario_actions_airborne.c:61`).
/// Returns true when fall damage (or death) was applied.
pub fn check_fall_damage(cx: &mut ActionCx, hard_fall_action: ActionId) -> bool {
    // decomp: only when action != ACT_TWIRLING && floor.type != SURFACE_BURNING
    // && vel[1] < -55.
    if cx.state.action == ActionId::TWIRLING {
        return false;
    }
    if let Some(f) = cx.state.floor {
        if f.surface_type as u16 == surface_type::BURNING {
            return false;
        }
    }
    if !(cx.state.vel.y < -55.0) {
        return false;
    }
    // decomp: fallHeight = peakHeight - pos[1].
    // NOTE (plan AR-10): the decomp's peakHeight is "last y while vel[1] >= 0",
    // not "max y seen". The port's peak_height must follow the decomp.
    let fall_height = cx.state.peak_height - cx.state.pos.y;
    if fall_height > 3000.0 {
        // decomp: hurtCounter += (flags & MARIO_CAP_ON_HEAD) ? 16 : 24;
        // then drop_and_set_mario_action(hardFallAction, 4).
        let dmg = if cx.state.flags & mario_flags::CAP_ON_HEAD != 0 {
            16
        } else {
            24
        };
        cx.state.hurt_counter = cx.state.hurt_counter.saturating_add(dmg as u16);
        // decomp: drop_and_set_mario_action (object interaction skipped).
        // NOTE: Use goto_next (not goto) because the decomp's act_freefall
        // returns FALSE after common_air_action_step, stopping the outer
        // loop. The new action runs next tick, not this tick.
        let id = super::entry::set_mario_action_moving(cx, hard_fall_action);
        cx.goto_next(id, 4);
        return true;
    }
    // decomp: damageHeight = 1150 (600 for ground pound — dead code due to
    // the actionState comparison); !mario_floor_is_slippery.
    let damage_height = 1150.0;
    let slippery = crate::world::mario_floor_is_slippery(&cx.state);
    if fall_height > damage_height && !slippery {
        let dmg = if cx.state.flags & mario_flags::CAP_ON_HEAD != 0 {
            8
        } else {
            12
        };
        cx.state.hurt_counter = cx.state.hurt_counter.saturating_add(dmg as u16);
        cx.state.squish_timer = 30;
    }
    false
}

/// decomp: `check_fall_damage_or_get_stuck`
/// (`src/game/mario_actions_airborne.c:115`).
pub fn check_fall_damage_or_get_stuck(
    cx: &mut ActionCx,
    hard_fall_action: ActionId,
) -> bool {
    // decomp: should_get_stuck_in_ground — snow/sand terrain check.
    // Terrain types are not modeled; always false in the scenario set.
    check_fall_damage(cx, hard_fall_action)
}

/// Water-plunge check shared by air actions.
/// decomp: water plunge is entered from the air via `mario_execute_airborne_action`?
/// Actually the decomp checks it in `check_common_airborne_cancels`? No —
/// it's in the individual actions via `air_cancels`. Placed here for reuse.
pub fn check_water_plunge(cx: &mut ActionCx) -> Option<ActionResult> {
    if let Some(wl) = crate::actions::water::water_plunge_surface(cx) {
        return Some(crate::actions::water::enter_water_plunge(cx, wl));
    }
    None
}

/// decomp: `check_common_airborne_cancels` (`src/game/mario_actions_airborne.c:2050`).
/// Returns the transition if a cancel was taken, None otherwise.
/// Centralized here — handlers must NOT duplicate this logic.
pub fn check_common_airborne_cancels(cx: &mut ActionCx) -> Option<ActionResult> {
    // decomp: if (m->input & INPUT_SQUISHED) return drop_and_set_mario_action(m, ACT_SQUISHED, 0);
    // No squish input in the scenario set — skipped, but the structure is preserved.
    
    // decomp: vertical wind check — no vertical wind surfaces in the scenario set.
    
    // decomp: m->quicksandDepth = 0.0f (mario_actions_airborne.c:2059).
    // This is the SINGLE place where airborne actions zero quicksand depth.
    // Handlers must not duplicate this.
    cx.state.quicksand_depth = 0.0;
    
    None
}

/// decomp: `play_far_fall_sound` (`src/game/mario_actions_airborne.c:24`).
/// Sets MARIO_UNKNOWN_18 when falling far (peakHeight - pos.y > 1150).
/// Called from `mario_execute_airborne_action` before dispatching.
/// Centralized here — handlers must NOT duplicate this logic.
pub fn play_far_fall_sound(cx: &mut ActionCx) {
    let action = cx.state.action;
    // decomp: if (!(action & ACT_FLAG_INVULNERABLE) && action != ACT_TWIRLING
    //          && action != ACT_FLYING && !(m->flags & MARIO_UNKNOWN_18))
    let invuln = action.0 & crate::state::act_flags::INVULNERABLE != 0;
    if !invuln
        && action != ActionId::TWIRLING
        && action != ActionId::FLYING
        && cx.state.flags & mario_flags::UNKNOWN_18 == 0
    {
        if cx.state.peak_height - cx.state.pos.y > 1150.0 {
            // decomp: play_sound(SOUND_MARIO_WAAAOOOW, ...) — audio skipped,
            // the flag is real state. Also push the sound event.
            cx.state.flags |= mario_flags::UNKNOWN_18;
            if (cx.state.sound_count as usize) < cx.state.sounds.len() {
                cx.state.sounds[cx.state.sound_count as usize] =
                    Some(crate::sound::SoundEvent::FallScream);
                cx.state.sound_count += 1;
            }
        }
    }
}

/// decomp: `mario_execute_airborne_action` (`src/game/mario_actions_airborne.c:2063`).
/// Centralized dispatcher for all airborne actions. Runs the common cancels,
/// the far-fall sound, then delegates to the specific action handler.
///
/// This is the SINGLE entry point for airborne actions. The individual
/// handlers must NOT duplicate the cancels or fall-sound logic.
pub fn execute_airborne_action(
    cx: &mut ActionCx,
    prev_buttons: u16,
    registry: &super::ActionRegistry,
) -> ActionResult {
    // decomp: if (check_common_airborne_cancels(m)) return TRUE;
    if let Some(result) = check_common_airborne_cancels(cx) {
        return result;
    }
    
    // decomp: play_far_fall_sound(m);
    play_far_fall_sound(cx);
    
    // decomp: switch (m->action) { case ACT_JUMP: cancel = act_jump(m); break; ... }
    // The registry dispatch replaces the switch.
    match registry.get(cx.state.action) {
        Some(handler) => handler.tick(cx, prev_buttons),
        None => ActionResult::Stay,
    }
}
