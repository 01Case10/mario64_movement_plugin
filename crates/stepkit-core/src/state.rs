//! The complete simulation state: one small `Copy` struct.
//!
//! Everything the core needs to tick fits here, so states can be snapshotted,
//! hashed for determinism checks, and serialized for netplay later.

use crate::angles::Angle;
use crate::world::{CeilingHit, FloorHit, WallHit};
use glam::Vec3;

/// Decomp `ACT_GROUP_*` and `ACT_FLAG_*` bits
/// (`oracle/sm64/include/sm64.h:140-170`). Action ids embed these; the
/// pipeline and (from Phase 5) the action bodies branch on them directly.
/// Defined here ahead of FD-07's id renumbering because the Phase-3
/// pipeline port (`update_mario_health`, the action loop dispatch) already
/// reads the flag bits.
pub mod act_flags {
    pub const GROUP_MASK: u32 = 0x000001C0;
    pub const GROUP_STATIONARY: u32 = 0x00000000;
    pub const GROUP_MOVING: u32 = 0x00000040;
    pub const GROUP_AIRBORNE: u32 = 0x00000080;
    pub const GROUP_SUBMERGED: u32 = 0x000000C0;
    pub const GROUP_CUTSCENE: u32 = 0x00000100;
    pub const GROUP_AUTOMATIC: u32 = 0x00000140;
    pub const GROUP_OBJECT: u32 = 0x00000180;

    pub const STATIONARY: u32 = 0x00000200;
    pub const MOVING: u32 = 0x00000400;
    pub const AIR: u32 = 0x00000800;
    pub const INTANGIBLE: u32 = 0x00001000;
    pub const SWIMMING: u32 = 0x00002000;
    pub const METAL_WATER: u32 = 0x00004000;
    pub const SHORT_HITBOX: u32 = 0x00008000;
    pub const RIDING_SHELL: u32 = 0x00010000;
    pub const INVULNERABLE: u32 = 0x00020000;
    pub const BUTT_OR_STOMACH_SLIDE: u32 = 0x00040000;
    pub const DIVING: u32 = 0x00080000;
    pub const ON_POLE: u32 = 0x00100000;
    pub const HANGING: u32 = 0x00200000;
    pub const IDLE: u32 = 0x00400000;
    pub const ATTACKING: u32 = 0x00800000;
    pub const ALLOW_VERTICAL_WIND_ACTION: u32 = 0x01000000;
    pub const CONTROL_JUMP_HEIGHT: u32 = 0x02000000;
    pub const ALLOW_FIRST_PERSON: u32 = 0x04000000;
    pub const PAUSE_EXIT: u32 = 0x08000000;
    pub const SWIMMING_OR_FLYING: u32 = 0x10000000;
    pub const WATER_OR_TEXT: u32 = 0x20000000;
    pub const THROWING: u32 = 0x80000000;
}

/// Decomp `MARIO_*` flags (`oracle/sm64/include/sm64.h`): the `m->flags`
/// bitfield. Phase 4 needs `UNKNOWN_08` (jump-ascent gravity strengthening)
/// and `UNKNOWN_30` (air wall hit); more bits arrive with Phase 5.
pub mod mario_flags {
    pub const NORMAL_CAP: u32 = 0x00000001;
    pub const WING_CAP: u32 = 0x00000008;
    pub const CAP_ON_HEAD: u32 = 0x00000010;
    pub const UNKNOWN_08: u32 = 0x00000100;
    /// decomp: `MARIO_UNKNOWN_25` (sm64.h:131); set by
    /// `update_ledge_climb_camera`.
    pub const UNKNOWN_25: u32 = 0x02000000;
    pub const ACTION_SOUND_PLAYED: u32 = 0x00010000;
    pub const MARIO_SOUND_PLAYED: u32 = 0x00020000;
    pub const UNKNOWN_30: u32 = 0x40000000;
}

/// Action identifiers. The IDs are the decomp's `ACT_*` values
/// (`oracle/sm64/include/sm64.h`); the high bits are the `ACT_FLAG_*`
/// action flags, which drive behavior (e.g. `ACT_FLAG_AIR` for air
/// actions, `ACT_FLAG_MOVING` for moving actions).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct ActionId(pub u32);

impl ActionId {
    // --- Decomp ACT_* values (oracle/sm64/include/sm64.h). ---

    /// Idle: standing still on the ground.
    /// spec: actions.idle (verified: wiki:Idle@20401)
    pub const IDLE: ActionId = ActionId(0x0C400201);
    /// Walking: stick-held ground locomotion (covers all speeds; there is no
    /// separate community-documented "running" action).
    /// spec: actions.walking (verified: decomp ACT_WALKING)
    pub const WALKING: ActionId = ActionId(0x04000440);
    /// Turning around while moving.
    /// spec: actions.turning_around (verified: wiki:Turning Around@18182)
    pub const TURNING_AROUND: ActionId = ActionId(0x00000443);
    /// Finish turning around: re-accelerate after the turn.
    /// spec: actions.finish_turning_around (verified: decomp ACT_FINISH_TURNING_AROUND)
    pub const FINISH_TURNING_AROUND: ActionId = ActionId(0x00000444);
    /// Braking: hard stop from speed >= 16 with neutral stick.
    /// spec: actions.braking (verified: decomp ACT_BRAKING)
    pub const BRAKING: ActionId = ActionId(0x04000445);
    /// Decelerating: gentle stop from speed < 16 with neutral stick.
    /// spec: actions.decelerating (verified: decomp ACT_DECELERATING)
    pub const DECELERATING: ActionId = ActionId(0x0400044A);
    /// Braking stop: stationary end of the brake, animation-driven.
    /// decomp: ACT_BRAKING_STOP
    pub const BRAKING_STOP: ActionId = ActionId(0x0C00023D);
    /// Begin sliding: dispatcher that picks butt vs stomach slide.
    /// decomp: ACT_BEGIN_SLIDING
    pub const BEGIN_SLIDING: ActionId = ActionId(0x00000050);
    /// Standing against a wall after pushing into it while walking.
    /// decomp: ACT_STANDING_AGAINST_WALL
    pub const STANDING_AGAINST_WALL: ActionId = ActionId(0x0C400209);
    /// Butt slide stop: stationary end of the butt slide, animation-driven.
    /// decomp: ACT_BUTT_SLIDE_STOP
    pub const BUTT_SLIDE_STOP: ActionId = ActionId(0x0C00023E);
    /// Slide-kick slide stop: stationary end of the slide-kick slide.
    /// decomp: ACT_SLIDE_KICK_SLIDE_STOP
    pub const SLIDE_KICK_SLIDE_STOP: ActionId = ActionId(0x08000225);
    /// Landing from a single jump.
    /// spec: actions.jump_land (verified: decomp ACT_JUMP_LAND)
    pub const JUMP_LAND: ActionId = ActionId(0x04000470);
    pub const JUMP_LAND_STOP: ActionId = ActionId(0x0C000230);
    pub const DOUBLE_JUMP_LAND_STOP: ActionId = ActionId(0x0C000231);
    pub const FREEFALL_LAND_STOP: ActionId = ActionId(0x0C000232);
    pub const SIDE_FLIP_LAND_STOP: ActionId = ActionId(0x0C000233); // decomp: ACT_SIDE_FLIP_LAND_STOP
    pub const BACKFLIP_LAND_STOP: ActionId = ActionId(0x0800022F); // decomp: ACT_BACKFLIP_LAND_STOP
    pub const TRIPLE_JUMP_LAND_STOP: ActionId = ActionId(0x0800023A); // decomp: ACT_TRIPLE_JUMP_LAND_STOP
    pub const LONG_JUMP_LAND_STOP: ActionId = ActionId(0x0800023B); // decomp: ACT_LONG_JUMP_LAND_STOP
    /// Start sleeping (idle timeout). Not yet ported (Phase 7); the id is
    /// here so `act_idle`'s sleep branch targets the right action.
    /// decomp: ACT_START_SLEEPING
    pub const START_SLEEPING: ActionId = ActionId(0x0C400202);
    /// Shivering (idle timeout on snow). Not yet ported (Phase 7).
    /// decomp: ACT_SHIVERING
    pub const SHIVERING: ActionId = ActionId(0x0C40020B);
    /// Landing from a freefall.
    /// spec: actions.freefall_land (verified: decomp ACT_FREEFALL_LAND)
    pub const FREEFALL_LAND: ActionId = ActionId(0x04000471);
    /// Landing from a double jump.
    /// spec: actions.double_jump_land (verified: decomp ACT_DOUBLE_JUMP_LAND)
    pub const DOUBLE_JUMP_LAND: ActionId = ActionId(0x04000472);
    /// Landing from a side flip.
    /// spec: actions.side_flip_land (verified: decomp ACT_SIDE_FLIP_LAND)
    pub const SIDE_FLIP_LAND: ActionId = ActionId(0x04000473);
    /// Landing from a triple jump (A press suppressed).
    /// spec: actions.triple_jump_land (verified: decomp ACT_TRIPLE_JUMP_LAND)
    pub const TRIPLE_JUMP_LAND: ActionId = ActionId(0x04000478);
    /// Landing from a long jump (ends crouching, keeps Z-stance).
    /// spec: actions.long_jump_land (verified: decomp ACT_LONG_JUMP_LAND)
    pub const LONG_JUMP_LAND: ActionId = ActionId(0x00000479);
    /// Landing from a backflip (A stripped unless Z held).
    /// spec: actions.backflip_land (verified: decomp ACT_BACKFLIP_LAND)
    pub const BACKFLIP_LAND: ActionId = ActionId(0x0400047A);
    /// Single jump, airborne.
    /// spec: actions.jump (verified: wiki:Single Jump@19309)
    pub const JUMP: ActionId = ActionId(0x03000880);
    pub const SPECIAL_TRIPLE_JUMP: ActionId = ActionId(0x030008AF);
    pub const VERTICAL_WIND: ActionId = ActionId(0x1008089C);
    pub const HEAD_STUCK_IN_GROUND: ActionId = ActionId(0x0002033A);
    pub const BUTT_STUCK_IN_GROUND: ActionId = ActionId(0x0002033B);
    pub const DIVE_PICKING_UP: ActionId = ActionId(0x00000385);
    pub const HOLD_JUMP: ActionId = ActionId(0x030008A0);
    pub const FLYING_TRIPLE_JUMP: ActionId = ActionId(0x03000894);
    pub const TOP_OF_POLE_JUMP: ActionId = ActionId(0x0300088D);
    pub const RIDING_SHELL_JUMP: ActionId = ActionId(0x0281089A);
    pub const BURNING_JUMP: ActionId = ActionId(0x010208B4);
    pub const HOLD_WATER_JUMP: ActionId = ActionId(0x010008A3);
    pub const START_HANGING: ActionId = ActionId(0x08200348);
    /// decomp: ACT_HANGING (0x00200349)
    pub const HANGING: ActionId = ActionId(0x00200349);
    /// Double jump, airborne.
    /// spec: actions.double_jump (verified: wiki:Double Jump@18964)
    pub const DOUBLE_JUMP: ActionId = ActionId(0x03000881);
    /// Triple jump, airborne.
    /// spec: actions.triple_jump (verified: wiki:Triple Jump@19300)
    pub const TRIPLE_JUMP: ActionId = ActionId(0x01000882);
    /// Backflip, airborne.
    /// spec: actions.backflip (verified: wiki:Backflip@19307)
    pub const BACKFLIP: ActionId = ActionId(0x01000883);
    /// Side flip, airborne.
    /// spec: actions.side_flip (verified: wiki:Side Flip@20374)
    pub const SIDE_FLIP: ActionId = ActionId(0x01000887);
    /// Twirling (wing-cap spin descent).
    /// decomp: ACT_TWIRLING (0x108008A4)
    pub const TWIRLING: ActionId = ActionId(0x108008A4);
    /// decomp: ACT_TWIRL_LAND (0x18800238)
    pub const TWIRL_LAND: ActionId = ActionId(0x18800238);
    /// decomp: ACT_HANG_MOVING (0x0020054A)
    pub const HANG_MOVING: ActionId = ActionId(0x0020054A);
    /// Shot from a cannon.
    /// decomp: ACT_SHOT_FROM_CANNON (0x00880898)
    pub const SHOT_FROM_CANNON: ActionId = ActionId(0x00880898);
    /// Flying (wing cap).
    /// decomp: ACT_FLYING (0x10880899)
    pub const FLYING: ActionId = ActionId(0x10880899);
    /// Lava boost (knocked skyward by lava).
    /// decomp: ACT_LAVA_BOOST (0x010208B7)
    pub const LAVA_BOOST: ActionId = ActionId(0x010208B7);
    /// Getting blown (tornado / wind).
    /// decomp: ACT_GETTING_BLOWN (0x010208B8)
    pub const GETTING_BLOWN: ActionId = ActionId(0x010208B8);
    /// BBH spinning (enter spin).
    /// decomp: ACT_BBH_ENTER_SPIN (0x00001535)
    pub const BBH_ENTER_SPIN: ActionId = ActionId(0x00001535);
    /// Falling after a star grab.
    /// decomp: ACT_FALL_AFTER_STAR_GRAB (0x00001904)
    pub const FALL_AFTER_STAR_GRAB: ActionId = ActionId(0x00001904);
    /// Quicksand death.
    /// decomp: ACT_QUICKSAND_DEATH (0x00021312)
    pub const QUICKSAND_DEATH: ActionId = ActionId(0x00021312);
    /// Long jump, airborne.
    /// spec: actions.long_jump (verified: wiki:Long Jump@14374)
    pub const LONG_JUMP: ActionId = ActionId(0x03000888);
    /// Dive, airborne.
    /// spec: actions.dive (verified: wiki:Dive@19303)
    pub const DIVE: ActionId = ActionId(0x0188088A);
    /// Slide kick, airborne (B during a fast crouch slide).
    /// spec: actions.slide_kick (verified: decomp ACT_SLIDE_KICK)
    pub const SLIDE_KICK: ActionId = ActionId(0x018008AA);
    /// Forward rollout: exit from dive/slide-kick slides.
    /// spec: actions.forward_rollout (verified: decomp ACT_FORWARD_ROLLOUT)
    pub const FORWARD_ROLLOUT: ActionId = ActionId(0x010008A6);
    /// Backward rollout: exit from dive/slide-kick slides.
    /// spec: actions.backward_rollout (verified: decomp ACT_BACKWARD_ROLLOUT)
    pub const BACKWARD_ROLLOUT: ActionId = ActionId(0x010008AD);
    /// Freefall: walked off a ledge or jump expired.
    /// spec: actions.freefall (PROVISIONAL)
    pub const FREEFALL: ActionId = ActionId(0x0100088C); // decomp: ACT_FREEFALL
    /// Ledge grab.
    /// spec: actions.ledge_grab (verified: wiki:Ledge Grab@19518)
    pub const LEDGE_GRAB: ActionId = ActionId(0x0800034B);
    /// Crouching (Z held on the ground).
    /// spec: actions.crouch (PROVISIONAL)
    pub const CROUCH: ActionId = ActionId(0x0C008220); // decomp: ACT_CROUCHING
    /// Start crouching: animation-driven entry into the crouch.
    /// decomp: ACT_START_CROUCHING
    pub const START_CROUCHING: ActionId = ActionId(0x0C008221);
    /// Stop crouching: animation-driven exit from the crouch.
    /// decomp: ACT_STOP_CROUCHING
    pub const STOP_CROUCHING: ActionId = ActionId(0x0C008222);
    /// Start crawling: animation-driven entry into the crawl.
    /// decomp: ACT_START_CRAWLING
    pub const START_CRAWLING: ActionId = ActionId(0x0C008223);
    /// Stop crawling: animation-driven exit from the crawl.
    /// decomp: ACT_STOP_CRAWLING
    pub const STOP_CRAWLING: ActionId = ActionId(0x0C008224);
    /// Crawling (stick held while crouching).
    /// spec: actions.crawl (PROVISIONAL)
    pub const CRAWL: ActionId = ActionId(0x04008448); // decomp: ACT_CRAWLING
    /// Ground pound.
    /// spec: actions.ground_pound (PROVISIONAL -- no community page)
    pub const GROUND_POUND: ActionId = ActionId(0x008008A9); // decomp: ACT_GROUND_POUND
    /// Wall kick.
    /// spec: actions.wall_kick (verified: wiki:Wall Kick rev 19311)
    pub const WALL_KICK: ActionId = ActionId(0x018808B0);
    /// Wall-kick flight: the airborne action after a wall kick.
    /// spec: actions.wall_kick_air (verified: decomp ACT_WALL_KICK_AIR)
    pub const WALL_KICK_AIR: ActionId = ActionId(0x03000886);
    /// Air hit wall: transient 2-frame bonk state entered on air wall hits
    /// at speed > 16. A in the window -> wall kick; then knockback/soft-bonk.
    /// spec: actions.air_hit_wall (verified: decomp ACT_AIR_HIT_WALL)
    pub const AIR_HIT_WALL: ActionId = ActionId(0x000008A7);
    /// Steep jump: jump from a very steep floor (normal.y < 0.2924).
    /// spec: actions.steep_jump (verified: decomp ACT_STEEP_JUMP)
    pub const STEEP_JUMP: ActionId = ActionId(0x03000885);
    /// Soft bonk: low-speed air wall-hit outcome; keeps forward speed.
    /// spec: actions.soft_bonk (verified: decomp ACT_SOFT_BONK)
    pub const SOFT_BONK: ActionId = ActionId(0x010208B6);
    /// Backwards air knockback (bonk).
    /// spec: actions.backward_air_kb (verified: decomp ACT_BACKWARD_AIR_KB)
    pub const BACKWARD_AIR_KB: ActionId = ActionId(0x010208B0);
    /// Forwards air knockback.
    /// spec: actions.forward_air_kb (verified: decomp ACT_FORWARD_AIR_KB)
    pub const FORWARD_AIR_KB: ActionId = ActionId(0x010208B1);
    /// Hard backwards air knockback (arg 1 = hard; Phase E consumes it).
    /// spec: actions.hard_backward_air_kb (verified: decomp)
    pub const HARD_BACKWARD_AIR_KB: ActionId = ActionId(0x010208B3); // decomp: ACT_HARD_BACKWARD_AIR_KB
    /// Hard forwards air knockback (arg 1 = hard; Phase E consumes it).
    /// spec: actions.hard_forward_air_kb (verified: decomp)
    pub const HARD_FORWARD_AIR_KB: ActionId = ActionId(0x010208B2); // decomp: ACT_HARD_FORWARD_AIR_KB
    /// Butt slide.
    /// spec: actions.butt_slide (verified: decomp ACT_BUTT_SLIDE)
    pub const BUTT_SLIDE: ActionId = ActionId(0x00840452);
    /// decomp: ACT_WATER_IDLE (0x380022C0)
    pub const WATER_IDLE: ActionId = ActionId(0x380022C0);
    /// decomp: ACT_WATER_ACTION_END (0x300022C2)
    pub const WATER_ACTION_END: ActionId = ActionId(0x300022C2);
    /// decomp: ACT_DROWNING (0x300032C4)
    pub const DROWNING: ActionId = ActionId(0x300032C4);
    /// decomp: ACT_BACKWARD_WATER_KB (0x300222C5)
    pub const BACKWARD_WATER_KB: ActionId = ActionId(0x300222C5);
    /// decomp: ACT_FORWARD_WATER_KB (0x300222C6)
    pub const FORWARD_WATER_KB: ActionId = ActionId(0x300222C6);
    /// decomp: ACT_WATER_DEATH (0x300032C7)
    pub const WATER_DEATH: ActionId = ActionId(0x300032C7);
    /// decomp: ACT_WATER_SHOCKED (0x300222C8)
    pub const WATER_SHOCKED: ActionId = ActionId(0x300222C8);
    /// decomp: ACT_BREASTSTROKE (0x300024D0)
    pub const BREASTSTROKE: ActionId = ActionId(0x300024D0);
    /// decomp: ACT_SWIMMING_END (0x300024D1)
    pub const SWIMMING_END: ActionId = ActionId(0x300024D1);
    /// decomp: ACT_FLUTTER_KICK (0x300024D2)
    pub const FLUTTER_KICK: ActionId = ActionId(0x300024D2);
    /// decomp: ACT_WATER_PUNCH (0x300024E1)
    pub const WATER_PUNCH: ActionId = ActionId(0x300024E1);
    /// decomp: ACT_WATER_PLUNGE (0x300022E2)
    pub const WATER_PLUNGE: ActionId = ActionId(0x300022E2);
    /// decomp: ACT_METAL_WATER_STANDING (0x080042F0)
    pub const METAL_WATER_STANDING: ActionId = ActionId(0x080042F0);
    /// decomp: ACT_METAL_WATER_WALKING (0x000044F2)
    pub const METAL_WATER_WALKING: ActionId = ActionId(0x000044F2);
    /// decomp: ACT_METAL_WATER_FALLING (0x000042F4)
    pub const METAL_WATER_FALLING: ActionId = ActionId(0x000042F4);
    /// decomp: ACT_METAL_WATER_FALL_LAND (0x000042F6)
    pub const METAL_WATER_FALL_LAND: ActionId = ActionId(0x000042F6);
    /// decomp: ACT_METAL_WATER_JUMP (0x000044F8)
    pub const METAL_WATER_JUMP: ActionId = ActionId(0x000044F8);
    /// decomp: ACT_METAL_WATER_JUMP_LAND (0x000044FA)
    pub const METAL_WATER_JUMP_LAND: ActionId = ActionId(0x000044FA);
    /// Stomach slide (slide trigger while facing uphill/flat).
    /// spec: actions.stomach_slide (verified: decomp ACT_STOMACH_SLIDE)
    pub const STOMACH_SLIDE: ActionId = ActionId(0x008C0453);
    /// Dive slide: landing from a dive.
    /// spec: actions.dive_slide (verified: decomp ACT_DIVE_SLIDE)
    pub const DIVE_SLIDE: ActionId = ActionId(0x00880456);
    /// Crouch slide: Z while moving.
    /// spec: actions.crouch_slide (verified: decomp ACT_CROUCH_SLIDE)
    pub const CROUCH_SLIDE: ActionId = ActionId(0x04808459);
    /// Slide-kick slide: ground continuation of an airborne slide kick.
    /// spec: actions.slide_kick_slide (verified: decomp ACT_SLIDE_KICK_SLIDE)
    pub const SLIDE_KICK_SLIDE: ActionId = ActionId(0x0080045A);
    /// Stomach slide stop: stationary end of the stomach slide.
    /// decomp: ACT_STOMACH_SLIDE_STOP
    pub const STOMACH_SLIDE_STOP: ActionId = ActionId(0x00000386);
    /// Butt slide air: airborne continuation of a butt slide that left the
    /// ground (bounces once on landing). Body is Phase 6 (AR-03).
    /// decomp: ACT_BUTT_SLIDE_AIR
    pub const BUTT_SLIDE_AIR: ActionId = ActionId(0x0300088E);
    /// Waist-deep in quicksand: can still jump and crouch.
    /// spec: actions.in_quicksand (verified: decomp ACT_IN_QUICKSAND)
    pub const IN_QUICKSAND: ActionId = ActionId(0x0002020D);
    /// Landing from a jump while deep in quicksand: 13-frame escape.
    /// spec: actions.quicksand_jump_land (verified: decomp ACT_QUICKSAND_JUMP_LAND)
    pub const QUICKSAND_JUMP_LAND: ActionId = ActionId(0x00000476); // decomp: ACT_QUICKSAND_JUMP_LAND
    /// Fast ledge pull-up (A with headroom).
    /// spec: actions.ledge_climb_fast (verified: decomp ACT_LEDGE_CLIMB_FAST)
    pub const LEDGE_CLIMB_FAST: ActionId = ActionId(0x0000054F);
    /// Slow ledge climb, part 1 (stick toward the wall).
    /// spec: actions.ledge_climb_slow_1 (verified: decomp ACT_LEDGE_CLIMB_SLOW_1)
    pub const LEDGE_CLIMB_SLOW_1: ActionId = ActionId(0x0000054C);
    /// Slow ledge climb, part 2.
    /// spec: actions.ledge_climb_slow_2 (verified: decomp ACT_LEDGE_CLIMB_SLOW_2)
    pub const LEDGE_CLIMB_SLOW_2: ActionId = ActionId(0x0000054D);
    /// First-person mode (dead branch in scope: INPUT_FIRST_PERSON never set).
    /// decomp: ACT_FIRST_PERSON
    pub const FIRST_PERSON: ActionId = ActionId(0x0C000227);
    /// Ground-pound landing (stationary). Ported with the landing system.
    /// decomp: ACT_GROUND_POUND_LAND
    pub const GROUND_POUND_LAND: ActionId = ActionId(0x0080023C);
    /// Climb down from a ledge (from push_or_sidle_wall's ledge check).
    /// decomp: ACT_LEDGE_CLIMB_DOWN
    pub const LEDGE_CLIMB_DOWN: ActionId = ActionId(0x0000054E);
    /// Stationary punch: B on the ground at speed < 8. Three-hit combo
    /// (punch, punch, kick) chained by B; combo stage in action_state.
    /// spec: actions.punching (verified: decomp ACT_PUNCHING)
    pub const PUNCHING: ActionId = ActionId(0x00800380);
    /// Moving punch: B on the ground at speed >= 8. Same combo, keeps
    /// momentum; ends walking if the stick is held.
    /// spec: actions.move_punching (verified: decomp ACT_MOVE_PUNCHING)
    pub const MOVE_PUNCHING: ActionId = ActionId(0x00800457);
    /// Jump kick: B in slow air, or A on the first punch frame. vy = 20.
    /// spec: actions.jump_kick (verified: decomp ACT_JUMP_KICK)
    pub const JUMP_KICK: ActionId = ActionId(0x018008AC);
    /// Backwards ground knockback: wall bonk / air-KB landing outcome.
    /// spec: actions.backward_ground_kb (verified: decomp ACT_BACKWARD_GROUND_KB)
    pub const BACKWARD_GROUND_KB: ActionId = ActionId(0x00020462);
    /// Forwards ground knockback.
    /// spec: actions.forward_ground_kb (verified: decomp ACT_FORWARD_GROUND_KB)
    pub const FORWARD_GROUND_KB: ActionId = ActionId(0x00020463);
    /// Hard backwards ground knockback (arg 1 = hard from the air).
    /// spec: actions.hard_backward_ground_kb (verified: decomp)
    pub const HARD_BACKWARD_GROUND_KB: ActionId = ActionId(0x00020460);
    /// Hard forwards ground knockback (arg 1 = hard from the air).
    /// spec: actions.hard_forward_ground_kb (verified: decomp)
    pub const HARD_FORWARD_GROUND_KB: ActionId = ActionId(0x00020461);
    /// Soft backwards ground knockback (from a soft-bonk landing).
    /// spec: actions.soft_backward_ground_kb (verified: decomp)
    pub const SOFT_BACKWARD_GROUND_KB: ActionId = ActionId(0x00020464);
    /// Soft forwards ground knockback (from a soft-bonk landing).
    /// spec: actions.soft_forward_ground_kb (verified: decomp)
    pub const SOFT_FORWARD_GROUND_KB: ActionId = ActionId(0x00020465);
    /// Ground bonk: fast slide wall-hit outcome; reflected slide vector
    /// decays over 32 frames.
    /// spec: actions.ground_bonk (verified: decomp ACT_GROUND_BONK)
    pub const GROUND_BONK: ActionId = ActionId(0x00020466);
    /// Water jump: A near the surface while swimming. vy = 62, forward
    /// speed raised to at least 15; walls slide (forward reset to 15).
    /// spec: actions.water_jump (verified: decomp ACT_WATER_JUMP)
    pub const WATER_JUMP: ActionId = ActionId(0x01000889);
    /// Standing death: health below 0x100 while in a moving action.
    /// Minimal stub in Phase 5 (death presentation is out of scope).
    /// decomp: ACT_STANDING_DEATH
    pub const STANDING_DEATH: ActionId = ActionId(0x00021311);
    /// Shockwave bounce: stomped while in a moving action.
    /// Minimal stub in Phase 5 (no stomp sources in scope).
    /// decomp: ACT_SHOCKWAVE_BOUNCE
    pub const SHOCKWAVE_BOUNCE: ActionId = ActionId(0x00020226);
    /// Squished: flattened by a squish source while moving.
    /// Minimal stub in Phase 5 (no squish sources in scope).
    /// decomp: ACT_SQUISHED
    pub const SQUISHED: ActionId = ActionId(0x00020339);

    /// Custom action IDs start here; the registry enforces the range.
    pub const CUSTOM_BASE: u32 = 0x0100_0000;

    pub fn custom(slot: u32) -> ActionId {
        ActionId(Self::CUSTOM_BASE + slot)
    }

    pub fn is_custom(self) -> bool {
        self.0 >= Self::CUSTOM_BASE && !self.is_provisional()
    }

    /// True for stepkit-provisional built-in IDs (bit 30 set): not community
    /// values, replaced when the community documents the real ones.
    pub fn is_provisional(self) -> bool {
        self.0 & 0x4000_0000 != 0
    }
}

/// The full per-character simulation state.
///
/// Positions and velocities are in SM64 units (the public documentation
/// gives the hitbox as radius 50, height 160). Angles are 16-bit.
#[derive(Clone, Copy, Debug)]
pub struct CharacterState {
    /// Feet position.
    pub pos: Vec3,
    /// Current velocity.
    pub vel: Vec3,
    /// Signed speed along the facing direction.
    pub forward_speed: f32,
    /// Sliding velocity vector (independent of facing for authentic slides).
    pub slide_vel_x: f32,
    pub slide_vel_z: f32,
    /// Slide yaw: direction of the slide velocity vector.
    /// decomp: m->slideYaw
    pub slide_yaw: Angle,
    /// Twirl yaw accumulator. decomp: `m->twirlYaw` (`s16`).
    pub twirl_yaw: i16,
    /// Facing yaw / pitch / roll.
    pub face_yaw: Angle,
    pub face_pitch: Angle,
    pub face_roll: Angle,
    /// Angular velocity (pitch/yaw/roll).
    /// decomp: m->angleVel
    pub angle_vel: [Angle; 3],
    /// Stick-intended yaw and magnitude, from `update_mario_joystick_inputs`.
    /// decomp: m->intendedYaw, m->intendedMag
    pub intended_yaw: Angle,
    pub intended_mag: f32,
    /// Per-frame input flags (`INPUT_*`, see [`crate::input::input_flags`]).
    /// decomp: m->input
    pub input: u32,
    /// The floor surface Mario stands on (`None` when airborne or OOB).
    /// decomp: m->floor
    pub floor: Option<FloorHit>,
    /// Yaw of the floor normal: `atan2s(floor.normal.z, floor.normal.x)`.
    /// decomp: m->floorAngle
    pub floor_angle: Angle,
    /// Water surface height; `FLOOR_LOWER_LIMIT` (-11000) when no water.
    /// decomp: m->waterLevel
    pub water_level: f32,
    /// Double-jump chain window; decremented each frame while > 0.
    /// decomp: m->doubleJumpTimer
    pub double_jump_timer: u32,
    /// Frames since A / B were last pressed (saturate at 0xFF).
    /// decomp: m->framesSinceA, m->framesSinceB
    pub frames_since_a: u8,
    pub frames_since_b: u8,
    /// Fall-damage accumulator; converted to health in `update_mario_health`.
    /// decomp: m->hurtCounter
    pub hurt_counter: u16,
    /// Invincibility frames after taking damage.
    /// decomp: m->invincTimer
    pub invinc_timer: u16,
    /// Mario status flags (`MARIO_*`; only bits movement reads are set).
    /// decomp: m->flags
    pub flags: u32,
    /// Gravity applied while getting blown (`ACT_GETTING_BLOWN`).
    /// decomp: m->gettingBlownGravity
    pub getting_blown_gravity: f32,
    /// Area-level terrain type (`TERRAIN_*`, see
    /// [`crate::world::terrain_type`]); drives the floor slipperiness class.
    /// decomp: m->area->terrainType
    pub terrain_type: u16,
    /// Current and previous action.
    pub action: ActionId,
    pub prev_action: ActionId,
    /// Action-local state machine value, timer, and argument.
    pub action_state: u32,
    pub action_timer: u32,
    pub action_arg: u32,
    /// Floor and ceiling heights found on the last step (`None` if none).
    pub floor_y: Option<f32>,
    pub ceil_y: Option<f32>,
    /// Surface kind underfoot (index into the surface table).
    pub floor_kind: u8,
    /// Decomp `m->wall` (`struct Surface *`): the referenced wall from the
    /// last step routine, or `None` when no wall was hit. Replaces the
    /// pre-Phase-4 `wall_hit`/`wall_normal` booleans (ST-01).
    pub wall: Option<WallHit>,
    /// Decomp `m->ceil` (`struct Surface *`): the ceiling referenced by the
    /// last ceiling check (used by the hang check in the air step), `None`
    /// when there is none (ST-01).
    pub ceil: Option<CeilingHit>,
    /// Decomp `gGlobalTimer`: incremented at the start of every tick,
    /// before the geometry/action stages (used by windy ground).
    pub global_timer: u32,
    /// Wall-kick window timer (frames remaining).
    pub wall_kick_timer: u32,
    /// decomp: `oMarioLongJumpIsSlow` (mario.c:866) — set at long-jump entry
    /// as `forwardVel > 16 ? FALSE : TRUE` (before the 1.5x multiplier).
    pub long_jump_is_slow: bool,
    /// The air action the last landing came from (for jump chaining).
    pub land_from: ActionId,
    /// Downward speed at the last landing (from the landing goto arg).
    /// Consumed by fall-damage/squish logic.
    pub last_fall_speed: f32,
    /// Health. Full is 0x880 (2176); death below 0x100.
    /// spec: damage.health_full
    pub health: i32,
    /// Frames of squish remaining (0 = not squished). While > 0 the
    /// intended stick magnitude is quartered, jump velocity halved, and
    /// double/triple jump chains are suppressed.
    pub squish_timer: u32,
    /// Highest Y reached during the current airtime; fall damage compares
    /// it against the landing height. Reset on landing and by ground
    /// actions.
    pub peak_height: f32,
    /// Quicksand sink depth. Grows while standing on quicksand, resets to
    /// 0 on other floors.
    pub quicksand_depth: f32,
    /// Slide speed-cap latch: the 100-unit cap applies one frame late, so
    /// a slide may exceed it for exactly one tick.
    pub slide_over_cap: bool,
    /// Swim strength for the breaststroke chain: 160 default, +10 per
    /// chained stroke, capped at 280. Swim speed = strength / 10.
    /// spec: swim.strength_default (verified: decomp-derived)
    pub swim_strength: u16,
    /// Gravity direction. Fixed to +Y in v1 (see open decision 7); stored so
    /// planetary gravity can be added later without rewriting every action.
    pub up: Vec3,
    /// Set by warps so the renderer skips interpolation across the jump.
    pub warped: bool,
    /// IK hand targets (world-space, sim units). Set during ledge-grab
    /// actions so a renderer can solve arm IK; `None` in all other actions.
    /// Output-only: never read by simulation logic, never hashed.
    pub ik_hand_l: Option<Vec3>,
    /// IK hand targets (world-space, sim units). Set during ledge-grab
    /// actions so a renderer can solve arm IK; `None` in all other actions.
    /// Output-only: never read by simulation logic, never hashed.
    pub ik_hand_r: Option<Vec3>,
    /// Grabbed ledge edge point (world-space, sim units), recorded by
    /// `try_ledge_grab`. Feeds `ik_hand_l`/`ik_hand_r`.
    /// Output-only: never read by simulation logic, never hashed.
    pub grab_point: Option<Vec3>,
    /// Animation state: the decomp's `AnimInfo` plus the oracle's
    /// `gOracleLastAnimId` bookkeeping (see [`crate::anim`]). Actions
    /// observe the previous tick's advance; [`crate::anim::advance_animation`]
    /// runs once per tick after the action loop.
    /// decomp: `m->marioObj->header.gfx.animInfo`
    pub anim: crate::anim::AnimState,
    /// Area-update counter driving the animation advance guard.
    /// decomp: `gAreaUpdateCounter` (`u16`); the oracle harness increments
    /// it once per frame after the advance.
    pub area_update_counter: u16,
}

impl Default for CharacterState {
    fn default() -> Self {
        Self {
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
            forward_speed: 0.0,
            slide_vel_x: 0.0,
            slide_vel_z: 0.0,
            slide_yaw: Angle::ZERO,
            twirl_yaw: 0,
            face_yaw: Angle::ZERO,
            face_pitch: Angle::ZERO,
            face_roll: Angle::ZERO,
            angle_vel: [Angle::ZERO; 3],
            intended_yaw: Angle::ZERO,
            intended_mag: 0.0,
            input: 0,
            floor: None,
            floor_angle: Angle::ZERO,
            water_level: -11000.0, // FLOOR_LOWER_LIMIT: no water
            double_jump_timer: 0,
            frames_since_a: 0xFF,
            frames_since_b: 0xFF,
            hurt_counter: 0,
            invinc_timer: 0,
            flags: 0x11, // MARIO_NORMAL_CAP | MARIO_CAP_ON_HEAD (oracle initial state)
            getting_blown_gravity: 0.0,
            terrain_type: crate::world::terrain_type::GRASS,
            action: ActionId::IDLE,
            prev_action: ActionId::IDLE,
            action_state: 0,
            action_timer: 0,
            action_arg: 0,
            floor_y: None,
            ceil_y: None,
            floor_kind: 0,
            wall: None,
            ceil: None,
            global_timer: 0,
            wall_kick_timer: 0,
            long_jump_is_slow: false,
            land_from: ActionId::IDLE,
            last_fall_speed: 0.0,
            health: 0x880, // full health (verified: decomp health model)
            squish_timer: 0,
            peak_height: 0.0,
            quicksand_depth: 0.0,
            slide_over_cap: false,
            swim_strength: 160, // spec: swim.strength_default
            up: Vec3::Y,
            warped: false,
            ik_hand_l: None,
            ik_hand_r: None,
            grab_point: None,
            anim: crate::anim::AnimState::default(),
            area_update_counter: 0,
        }
    }
}

impl CharacterState {
    /// Hash the state for cross-platform determinism checks (L0).
    /// Uses the raw bit patterns so NaNs and -0.0 are distinguished.
    pub fn hash_state(&self) -> u64 {
        use core::hash::{Hash, Hasher};
        let mut h = crate::state::fnv::Fnv1a64::new();
        for v in [
            self.pos.x, self.pos.y, self.pos.z, self.vel.x, self.vel.y, self.vel.z,
        ] {
            v.to_bits().hash(&mut h);
        }
        self.forward_speed.to_bits().hash(&mut h);
        self.slide_vel_x.to_bits().hash(&mut h);
        self.slide_vel_z.to_bits().hash(&mut h);
        self.slide_yaw.0.hash(&mut h);
        self.face_yaw.0.hash(&mut h);
        self.face_pitch.0.hash(&mut h);
        self.face_roll.0.hash(&mut h);
        for a in self.angle_vel {
            a.0.hash(&mut h);
        }
        self.intended_yaw.0.hash(&mut h);
        self.intended_mag.to_bits().hash(&mut h);
        self.input.hash(&mut h);
        // The floor handle: hash the surface identity the sim reads.
        match self.floor {
            Some(f) => {
                1u8.hash(&mut h);
                f.y.to_bits().hash(&mut h);
                f.normal.x.to_bits().hash(&mut h);
                f.normal.y.to_bits().hash(&mut h);
                f.normal.z.to_bits().hash(&mut h);
                f.surface_type.hash(&mut h);
                f.flags.hash(&mut h);
                f.force.hash(&mut h);
            }
            None => 0u8.hash(&mut h),
        }
        self.floor_angle.0.hash(&mut h);
        self.water_level.to_bits().hash(&mut h);
        self.double_jump_timer.hash(&mut h);
        self.frames_since_a.hash(&mut h);
        self.frames_since_b.hash(&mut h);
        self.hurt_counter.hash(&mut h);
        self.invinc_timer.hash(&mut h);
        self.flags.hash(&mut h);
        self.getting_blown_gravity.to_bits().hash(&mut h);
        self.terrain_type.hash(&mut h);
        self.action.0.hash(&mut h);
        self.prev_action.0.hash(&mut h);
        self.action_state.hash(&mut h);
        self.action_timer.hash(&mut h);
        self.action_arg.hash(&mut h);
        self.floor_y.map(f32::to_bits).hash(&mut h);
        self.ceil_y.map(f32::to_bits).hash(&mut h);
        self.floor_kind.hash(&mut h);
        self.wall
            .map(|w| {
                (
                    w.normal.x.to_bits(),
                    w.normal.y.to_bits(),
                    w.normal.z.to_bits(),
                    w.surface_type,
                )
            })
            .hash(&mut h);
        self.ceil
            .map(|c| {
                (
                    c.y.to_bits(),
                    c.normal.x.to_bits(),
                    c.normal.y.to_bits(),
                    c.normal.z.to_bits(),
                )
            })
            .hash(&mut h);
        self.global_timer.hash(&mut h);
        self.wall_kick_timer.hash(&mut h);
        self.land_from.0.hash(&mut h);
        self.last_fall_speed.to_bits().hash(&mut h);
        self.health.hash(&mut h);
        self.squish_timer.hash(&mut h);
        self.peak_height.to_bits().hash(&mut h);
        self.quicksand_depth.to_bits().hash(&mut h);
        self.slide_over_cap.hash(&mut h);
        self.swim_strength.hash(&mut h);
        h.finish()
    }
}

/// Minimal FNV-1a 64 used by [`CharacterState::hash_state`]; the core does
/// not depend on the platform hasher (which is intentionally randomized).
pub(crate) mod fnv {
    use core::hash::Hasher;
    pub struct Fnv1a64(u64);
    impl Fnv1a64 {
        pub fn new() -> Self {
            Self(0xcbf29ce484222325)
        }
    }
    impl Hasher for Fnv1a64 {
        fn write(&mut self, bytes: &[u8]) {
            for b in bytes {
                self.0 ^= *b as u64;
                self.0 = self.0.wrapping_mul(0x100000001b3);
            }
        }
        fn finish(&self) -> u64 {
            self.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_sane() {
        let s = CharacterState::default();
        assert_eq!(s.action, ActionId::IDLE);
        assert_eq!(s.up, Vec3::Y);
        assert!(!s.warped);
    }

    #[test]
    fn hash_is_stable_and_sensitive() {
        let a = CharacterState::default();
        let b = CharacterState::default();
        assert_eq!(a.hash_state(), b.hash_state());
        let mut c = a;
        c.pos.x = 0.5;
        assert_ne!(a.hash_state(), c.hash_state());
    }
}
