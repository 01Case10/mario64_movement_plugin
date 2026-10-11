# Parity log

Differential-test status of `stepkit oracle diff --all` per scenario.
First-divergence lines are recorded here after every port task (plan §6).

## Status (2026-10-11, Phase 2)

`stepkit oracle diff --all`: **108 scenarios, 0 pass / 108 fail / 0 errors**.
RED by design — the plugin has not been ported yet (Phases 3+). Every
failing scenario shows a first-divergence line below.

- Run: `./tools/parity-gate.sh` (exit 1; always prints the full table).
- Comparison is exact: bitwise `f32` equality via `to_bits()`, integer
  equality, zero tolerance (plan §7.2). Field map and the decomp→plugin
  action-id translation (FD-07 scaffolding) live in
  `crates/stepkit-cli/src/parity_diff.rs`.
- Unmodeled oracle fields (reported once per run, never compared, never
  masking divergences): `slide_yaw`, `angle_vel_*`, `intended_yaw`,
  `intended_mag`, `input`, `floor_angle`, `floor_normal_y`, `floor_type`,
  `double_jump_timer`, `water_level`, `hurt_counter`, `invinc_timer`,
  `flags`, `anim_id`, `anim_frame`, `anim_accel`, `anim_frame_accel_assist`.
  (`stick_x/y`, `buttons`, `cam_yaw` are scenario inputs, also not compared:
  the oracle reports post-`adjust_analog_stick` values and CONT bitmasks.)
- `--mode teacher-forced` is implemented (state reset from the oracle row,
  one tick, compare); not yet run across the suite.

### Divergence clusters (where Phase 3 should look first)

1. **Signed zero, 78 scenarios** — frame-1 `pos_y`/`floor_y`: plugin `0.0`
   vs oracle `-0.0`. Root cause verified in the decomp sources:
   `read_surface_data` computes `originOffset = -(nx*x1 + ny*y1 + nz*z1)`,
   which is `-0.0` when the (negative) vertex coordinates zero the sum;
   `find_floor_from_list` then yields `height = -(x*nx + nz*z + oo)/ny`
   `= -0.0` for floors through the origin, and the stationary ground step
   copies it to `pos.y`. The plugin's `find_floor` returns `+0.0`.
   Fixed by the ST-02/FD-04 verbatim ports (identical operation order and
   signed-zero behavior).
2. **Same-frame transition, 11 scenarios** — frame-1 `pos_y`: plugin `0.0`
   vs oracle `42.0` (all scenarios that press A on frame 0). FD-01: the
   decomp runs the new action in the same frame, so the jump displaces
   immediately; the plugin applies transitions one frame late.
3. **Per-action timers, 5 scenarios** — frame-1 `action_timer`: plugin `1`
   vs oracle `0` (`fall_damage_*`, `squish_buttons`, `pedro_spot`).
   FD-01/FD-06: the plugin auto-increments the timer every tick; the decomp
   leaves timer management to each action.
4. **Start-state geometry handling, 14 scenarios** — frame-1 position
   offsets on ramp/wall starts (`slide_*`, `butt_slide_*`, `strafe_left`,
   `stick_cap_diag`, `steep_jump*`). FD-08: the decomp's
   `update_mario_geometry_inputs` pushes Mario out of walls and re-queries
   the floor every frame before the action runs; the plugin has no
   equivalent, so the two sides disagree about the frame-0 start state.

### Table

| Scenario | Status | First divergence |
|---|---|---|
| air_steer_back_full_tilt | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle 42.0) |
| air_steer_left | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| air_steer_left_full_tilt | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle 42.0) |
| air_steer_right_full_tilt | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle 42.0) |
| backflip | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| brake_wall_bonk | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| butt_slide_air_bounce | FAIL | frame 1, field pos_y (plugin 195.0 vs oracle 193.5) |
| butt_slide_ramp | FAIL | frame 1, field pos_x (plugin -900.0 vs oracle -891.08594) |
| ceiling_low_150 | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| ceiling_low_170 | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| corner_two_walls | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| crawl_speed | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| crouch_slide_chain | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| crouch_slide_long_jump_30f_window | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| dive_air | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| dive_bonk | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| dive_ground | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| dive_slide_rollout | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| dive_slide_stop | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| double_jump | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| double_jump_window_2f | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle 42.0) |
| double_jump_window_5f | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle 42.0) |
| double_jump_window_9f | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle 42.0) |
| fall_damage_hard | FAIL | frame 1, field action_timer (plugin 1 vs oracle 0) |
| fall_damage_squish | FAIL | frame 1, field action_timer (plugin 1 vs oracle 0) |
| fall_damage_squish_tall | FAIL | frame 1, field action_timer (plugin 1 vs oracle 0) |
| ground_bonk | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| ground_pound | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| ground_pound_rise | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| held_jump_landing_frame | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle 42.0) |
| idle_60f | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| idle_to_walk_face_snap | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| jump_chain_double | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| jump_chain_triple | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| jump_forward | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| jump_hold_a | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| jump_kick | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle 42.0) |
| jump_neutral | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle 42.0) |
| land_stick_vs_no_stick | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle 42.0) |
| ledge_climb | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| ledge_grab | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| long_jump | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| long_jump_48_cruise | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| long_jump_entry | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| lower_wall_only | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| pedro_spot | FAIL | frame 1, field action_timer (plugin 1 vs oracle 0) |
| punch_combo | FAIL | frame 1, field floor_y (plugin 0.0 vs oracle -0.0) |
| quicksand | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| quicksand_deep | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| quicksand_shallow | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| same_frame_jump | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| sideflip | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| slide | FAIL | frame 1, field pos_z (plugin 450.0 vs oracle 451.5) |
| slide_default_downhill | FAIL | frame 1, field pos_y (plugin 336.0 vs oracle 315.84) |
| slide_default_uphill | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle 14.280001) |
| slide_kick | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| slide_kick_arc | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| slide_terrain_area | FAIL | frame 1, field pos_y (plugin 35.0 vs oracle 33.162502) |
| slide_very_slippery_downhill | FAIL | frame 1, field pos_y (plugin 85.0 vs oracle 80.325005) |
| slide_very_slippery_uphill | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle 3.8250015) |
| slide_wall_default | FAIL | frame 1, field pos_y (plugin 336.0 vs oracle 315.84) |
| slide_wall_slippery | FAIL | frame 1, field pos_y (plugin 336.0 vs oracle 315.84) |
| slippery_slide | FAIL | frame 1, field pos_z (plugin 390.0 vs oracle 388.5) |
| slope_down | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| slope_up_gentle | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| slope_up_steep | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| slow_surface_cap | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| squish_buttons | FAIL | frame 1, field action_timer (plugin 1 vs oracle 0) |
| steep_jump | FAIL | frame 1, field pos_y (plugin 31.355469 vs oracle -0.0) |
| steep_jump_slopes | FAIL | frame 1, field pos_y (plugin 210.0718 vs oracle 210.00002) |
| step_down_110 | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| step_down_20 | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| step_down_60 | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| step_down_90 | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| step_up_120 | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| step_up_20 | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| step_up_40 | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| step_up_70 | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| step_up_90 | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| stick_cap_diag | FAIL | frame 1, field pos_x (plugin 0.0 vs oracle -6.3031173) |
| stick_edges | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| stomach_slide | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| strafe_left | FAIL | frame 1, field pos_x (plugin 0.0 vs oracle 8.913954) |
| swim | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| tap_jump | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle 42.0) |
| triple_jump | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| triple_jump_speed_20 | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| turn_around | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| turn_around_full | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| very_slippery_walk_entry | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| walk_accel_mag16_flat | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| walk_accel_mag32_flat | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| walk_accel_mag48_flat | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| walk_accel_mag80_flat | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| walk_into_wall | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| walk_off_ledge | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| walk_slow_turn | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| walk_then_release | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| wall_bonk_fast | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| wall_bonk_slow | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| wall_kick | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| wall_kick_chain | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| wall_kick_miss | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| wall_kick_window | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| wall_slide_glance_30 | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| wall_slide_glance_45 | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| wall_slide_glance_75 | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |
| water_jump | FAIL | frame 1, field pos_y (plugin 0.0 vs oracle -0.0) |

## Phase 3 — FD-05 (state fields + trace columns) and FD-09 (surface model) — 2026-10-11

- `CharacterState` gained the missing MarioState fields: `slide_yaw`, `angle_vel[3]`,
  `intended_yaw`, `intended_mag`, `input` (INPUT_* flags), `floor: Option<FloorHit>`,
  `floor_angle`, `water_level` (-11000.0 = FLOOR_LOWER_LIMIT), `double_jump_timer`,
  `frames_since_a/b` (init 0xFF), `hurt_counter`, `invinc_timer`, `flags`, `terrain_type`.
  `hash_state` covers the new fields.
- `TraceFrame`/CSV gained 15 columns (`slide_yaw`, `angle_vel_x/y/z`, `intended_yaw`,
  `intended_mag`, `input`, `floor_angle`, `floor_normal_y`, `floor_type`,
  `double_jump_timer`, `water_level`, `hurt_counter`, `invinc_timer`, `flags`);
  new fields carry `#[serde(default)]` so old CSVs still parse. `UNMODELED_FIELDS`
  shrunk to the four `anim_*` columns (AN-01, Phase 5); `COMPARED_FIELDS` grew with
  the new fields appended last so first-divergence lines keep their meaning.
- `world.rs` gained `surface_type` (SURFACE_*, incl. the extra values used by
  `mario_get_floor_class`) and `terrain_type` (TERRAIN_*) modules; `FloorHit` now
  carries `surface_type`/`flags`/`force` (+normal); `CollisionWorld::terrain_type()`
  defaults to GRASS. `SurfaceWorld` builders take the SURFACE_* type and derive the
  coarse `SurfaceKind`; the scenario `kind`→`SURFACE_*` mapping now matches
  `geom::surface_type` exactly (plugin kind 3 = VERY_SLIPPERY 0x13; the old
  plugin-only `kind→SurfaceKind` mapping is gone).
- Ported `mario_get_floor_class`, `mario_facing_downhill`, `mario_floor_is_slippery`,
  `mario_floor_is_slope`, `mario_floor_is_steep` (`mario.c:387,556,573,606,638`) —
  used by nothing yet (actions still use the old class helpers; the switch happens
  in the Phase 5 ports).
- Diff checkpoint after FD-05+FD-09: still 0/108 pass, exit 1; first-divergence
  lines unchanged (all frame 1; pipeline/timer/geometry clusters dominate, as
  expected before FD-01/FD-06/FD-08). The newly compared fields are all behind
  the existing first divergences, so no new first-divergence clusters yet.
- PLAN ERRORS / DATA NOTES:
  - Phase-2 scenario `traces/scenarios/quicksand.ron` uses geometry `kind: 2`,
    which the oracle (and now the plugin) maps to SURFACE_NOT_SLIPPERY (0x15) —
    it never tested quicksand. The proper quicksand scenarios are
    `quicksand_shallow.ron` (kind 4) and `quicksand_deep.ron` (kind 5). Left as-is;
    revisit with the GR-14 quicksand port (Phase 5).
  - The task brief's NOTE for FD-09 claimed Phase 2 verified "very-slippery uses
    `normal.y ≤ 0.7880108` in `mario_floor_is_slippery`". The decomp source
    (`mario.c:573`) says VERY_SLIPPERY class → 0.9848077 (default class →
    0.7880108). Implemented 0.9848077 per the brief's own rule (trust the decomp
    source, not the plan's prose).

## Phase 3 — FD-02 (input processing) — 2026-10-11

- Ported `adjust_analog_stick` verbatim (`game_init.c:424`): per-axis ±8
  deadzone with ∓6 offset, magnitude cap 64.
- New `ProcessedInput` (stickX/stickY/stickMag, button pressed/down,
  intended yaw/mag, input flags); `process_input` runs the
  `update_mario_button_inputs` (squish-gated B/Z, framesSinceA/B) and
  `update_mario_joystick_inputs` (`(mag/64)^2*64`, /2 or /8 when squished,
  `intendedYaw = atan2s(-stickY, stickX) + camYaw`, INPUT_NONZERO_ANALOG)
  stages verbatim. `tick` runs it once per frame and publishes
  intended_yaw/intended_mag/input on the state.
- `RawInput.cam_yaw` keeps its public meaning (camera look direction); the
  +0x8000 conversion to the decomp's `camera->yaw` convention happens inside
  `process_input` and is documented (mirrors oracle_cmd.rs's +180°).
- `ActionCx` helpers now read the processed input: `intended_yaw()` is
  `Some` iff NONZERO_ANALOG; `intended_magnitude()` is the decomp value
  (squish quartering now comes from the /8, replacing the old /4 hack);
  `stick_mag()` is the post-cap `controller->stickMag` (the dive check at
  `ground.rs:291` uses it: post-cap `> 48`); `pressed()` uses the processed
  pressed edge (B/Z now squish-gated).
- Diff checkpoint: still 0/108, exit 1; first-divergence lines IDENTICAL to
  the FD-05/FD-09 checkpoint (verified by diffing the per-scenario lines).
  New clusters: none yet — the geometry flags (OFF_FLOOR/ABOVE_SLIDE/
  IN_WATER/SQUISHED/UNKNOWN_5) arrive with the FD-01/FD-08 port.
- Tests: 145 green (4 new input tests). One pre-existing anchors test
  (`anchor_side_flip_entry`) updated: it mutated `cx.input` after `proc`
  was built; now re-runs `process_input` like `tick` does. The squish-mag
  anchors test now expects the decomp value (8.0 = 64/8) instead of the
  plugin-invented 12.5.

## Phase 3 (FD-01…FD-09) — 2026-10-11

Foundations complete. The tick pipeline now mirrors the decomp's
`execute_mario_action` structure; action IDs are the decomp `ACT_*` values.

### FD-01/FD-06/FD-08: tick pipeline
- `tick()` rewritten per `mario.c:1699`: `process_input` → terrain type →
  `update_mario_geometry_inputs` (new, `mario.c:1314`: two wall-push passes
  (60,50)/(30,24), floor query → `state.floor` + `floor_angle`, ceil → 20000
  default, water → −11000 default, ABOVE_SLIDE/OFF_FLOOR/IN_WATER flags;
  SQUISHED/POISON_GAS skipped, documented) → `flags &= 0xFFFFFF` →
  UNKNOWN_5 → wallKickTimer--/doubleJumpTimer-- → action loop
  (`debug_assert` 64 guard, `action_timer`++ at top of each iteration) →
  post-action `squishTimer`-- (after action, `mario.c:1205`) +
  `update_mario_health` port (`mario.c:1454`; healCounter unmodeled,
  documented).
- Removed top-of-tick auto-increments. Actions manage their own
  `action_timer` (faithful per-decomp discipline arrives with Phase 5–7
  ports; current timing preserved mechanically).
- Verified: action fns return TRUE on transition and the new action's full
  body runs same-tick (oracle `jump_neutral` frame 0: vy=38 = 42−4).

### FD-07: decomp ACT_* ids
- Adopted decomp `ACT_*` values for 7 mismatched ids: FREEFALL
  0x4000088C→0x0100088C, CROUCH 0x40000010→0x0C008220, CRAWL
  0x40000011→0x04008448, GROUND_POUND 0x400008A8→0x008008A9,
  HARD_BACKWARD_AIR_KB/HARD_FORWARD_AIR_KB swap fixed (0x010208B2↔0x010208B3),
  QUICKSAND_JUMP_LAND 0x04000476→0x00000476.
- `act_flags` module (already in state.rs from FD-01) documents the
  `ACT_FLAG_*` bits; the stale "STEPKIT-PROVISIONAL" doc comment removed.
- Removed `translate_action` from `parity_diff.rs` (ids now match directly);
  updated the obsolete test to `action_ids_match_decomp_directly`.
- Updated `spec/actions/*.md` (removed PROVISIONAL markers).

### Bugs fixed during Phase 3
- **Quicksand cycle**: `ground_prelude`'s global `depth > 30 → IN_QUICKSAND`
  yank caused IN_QUICKSAND ↔ PUNCHING/CROUCH cycles via the action loop.
  Moved the check to `act_idle` (decomp location: `stationary.c:106`).
- **Ceiling landing bug**: `step_air`'s ceiling check zeroed `vy`, then the
  `vy <= 0` landing check misfired on the same step. Fixed to use the
  step-entry vy.
- **Wall-kick self-loop**: `air_common`'s A → wall-kick check fired when
  already in WALL_KICK_AIR (decomp's `act_wall_kick_air` has no A check).
  Added exclusion.
- **Double wall_kick_timer decrement**: removed the `air_common` decrement
  (pipeline now decrements once per tick).

### Diff checkpoint
- `oracle diff --all`: 0/108 pass, 108 fail, exit 1 (no crashes; the diff
  runs end-to-end).
- Divergence clusters: pos_y 86× (75 are signed-zero 0.0 vs −0.0, a
  diff-tool strictness artifact; 11 real), action_timer 16× (timer
  discipline differs; Phase 5–7), pos_x 5× (incl. `strafe_left`: the walk
  moves along face_yaw not intended_yaw; P5), slide_vel_z 1×.
- Exit-bar scenarios: `idle_60f`, `walk_accel_mag{16,32,48,80}_flat`,
  `walk_slow_turn` diverge only on signed-zero (diff artifact, not a sim
  bug). `strafe_left` diverges for a P5 reason (walk strafe direction).

### Tests
- `cargo test --workspace --exclude stepkit-godot`: all green (8 suites).
- `cargo check -p stepkit-godot`: passes.
- 53 anchors tests updated for same-frame semantics (documented in test
  comments).

### Plan errors found
- FD-09 brief claimed very-slippery uses `normal.y ≤ 0.7880108`; the decomp
  `mario.c:573` says VERY_SLIPPERY class → **0.9848077** (default class →
  0.7880108). Implemented 0.9848077.
- `traces/scenarios/quicksand.ron` uses geometry `kind: 2` =
  SURFACE_NOT_SLIPPERY — never tested quicksand; real quicksand scenarios
  are kinds 4/5.

### Update (2026-10-11, subagent completion)

All three action-port subagents finished. Per-action timer/state/arg/anim
discipline now covers every action in `ground.rs`, `air.rs`, `water.rs`.
`cargo test --workspace --exclude stepkit-godot`: 151/151 green (incl.
53/53 anchors). `cargo check -p stepkit-godot`: passes.

Diff: **108 scenarios, 16 pass / 92 fail / 0 errors** (unchanged count —
the remaining failures are all behavioral: velocities, air control, ceiling
queries; Phases 5-7). Zero first-divergences on `anim_id`/`anim_frame`/
`anim_accel`/`anim_frame_accel_assist`. First-divergence clusters:
`slide_vel_z` 23×, `vel_z` 16×, `pos_*` 36×, `flags` 6×, `slide_vel_x` 5×,
`water_level` 2×, `quicksand_depth` 2×, `peak_height`/`ceil_y` 1× each.

Passing: `ceiling_low_170`, `held_jump_landing_frame`, `idle_60f`,
`jump_neutral`, `lower_wall_only`, `slope_up_gentle`, `step_down_20/60/90`,
`step_up_20/40`, `tap_jump`, `walk_accel_mag16/32/48/80_flat`.

Key subagent findings (decomp-vs-prior-code, flagged for Phases 5-7):
- `act_air_hit_wall` has no `perform_air_step` in the decomp (pure timer);
  its missing `return` double-executes (effective 1-frame kick window).
  Port keeps the documented 2-frame window; anchor pins it.
- Knockback wall arm requests `BACKWARD_AIR_KB` even for forward KBs
  (new `AirConfig.anim_wall`).
- Slide-kick high-fall now passes arg 2 to FREEFALL (decomp); `Freefall`
  dead-ness re-keyed off `health < 0x100` (was arg 2).
- Punching: decomp runs an `actionArg` 0–9 machine with anim-end
  transitions; v1 keeps its stage machine (later phase).
- WaterPlunge exits on tick 21 (`> 20` post-increment, was 20);
  SwimmingEnd glide is 16 ticks (check before increment).
- WaterPlunge/WaterActionEnd exits are next-tick in the decomp
  (discarded `set_mario_action` return); Rust keeps same-tick `goto`
  (flagged, not changed per scope).
- Concurrent-edit hazard: three agents edited `air.rs`/`ground.rs`/
  `water.rs` simultaneously; interleavings were detected and repaired
  (stash incident, macro-hygiene fix, trait-API migrations). A leftover
  `git stash@{0}` entry remains; working tree holds the restored files.

### Post-remediation fixes (2026-10-11, main agent verification)

Verification of the remediation report caught three issues, all fixed:

1. **ST-05 sign error in `find_ceil_from_list`** (`surface.rs`): the port
   had `if (y - (height + -78.0) > 0.0) continue;` but the decomp reads
   `if (y - (height - -78.0f) > 0.0f) continue;` — the ceiling buffer is
   `height + 78`, not `height - 78` (the floor query's form). The bug made
   `find_ceil` miss real ceilings. `ceiling_low_150` went from failing at
   frame 1 (`pos_y`) to frame 18 (`action`, Phase 5/6); `pedro_spot` from
   frame 1 (`ceil_y`) to frame 2 (`wall_kick_timer`, Phase 6 AR-05).
2. **AN-01 trace wiring was not actually done** (report claimed it was):
   the incident stash held the complete wiring (`TraceFrame` anim fields,
   emission from sim state, diff comparison, teacher-forced anim restore,
   oracle-mirroring water regions). Restored the stash's
   `stepkit-trace`/`stepkit-cli` files; kept the newer `anchors.rs`
   expectations (21-tick plunge, 16-tick glide). The leftover
   `git stash@{0}` has been dropped. Unmodeled-fields list is now empty;
   `anim_frame` appears as a compared field.
3. **Ledge-grab A-press used a custom headroom probe** (`find_ceil(...).is_none()`)
   that the corrected `find_ceil` broke (the ledge's own underside now
   correctly counts within the +78 buffer). Replaced with the decomp's
   rule (`act_ledge_grab`, automatic.c:558):
   `hasSpaceForMario = (ceilHeight - floorHeight >= 160.0f)`.

Diff: **108 scenarios, 16 pass / 92 fail / 0 errors** (unchanged count).
`cargo test --workspace --exclude stepkit-godot`: **151/151 green**.
`cargo check -p stepkit-godot`: passes.

## Status (2026-10-11, Phase 5)

`stepkit oracle diff --all`: **108 scenarios, 16 pass / 92 fail / 0 errors**.
Unchanged from the Phase-4 baseline (16/92). The Phase-5 ground/stationary
action bodies are now verbatim decomp ports, but the ground scenarios in the
exit list do NOT yet pass. `cargo test --workspace --exclude stepkit-godot`:
151/151 green (12 anchor tests updated with decomp line citations).

### What landed

All 7 Phase-5 modules ported verbatim from the decomp with
`// decomp: src/game/…:line` citations, integrated via registry swap in
`crates/stepkit-core/src/actions.rs::sm64_style()`:

- `walk.rs`: `update_walking_speed` (f64 quicksand `6.25`, SURFACE_SLOW cap,
  slope accel after turn), `apply_slope_accel` (gated by
  `mario_floor_is_slope`), `apply_slope_decel`, `apply_landing_accel`
  (multiplicative), `update_decelerating_speed`, `analog_stick_held_back`
  (0x471C), `check_ground_dive_or_punch` (dive sets only vel.y=20),
  `begin_braking_action`, `push_or_sidle_wall`, `check_ledge_climb_down`,
  `act_walking` (exact cancel order, no A+Z long-jump, Z→CROUCH_SLIDE),
  `act_turning_around`/`act_finish_turning_around` (no facing rotation),
  `act_braking` (BACKWARD_GROUND_KB on wall, not GROUND_BONK),
  `act_braking_stop`, `act_decelerating`, `act_standing_against_wall`,
  plus stubs for LedgeClimbDown/StandingDeath/ShockwaveBounce/Squished.
- `idle_crouch.rs`: `act_idle` (+`check_common_idle_cancels` with the
  face-Yaw snap on walk entry, stationary.c:47), crouch/crawl family.
- `landings.rs`: `common_landing_cancels` (steep push-off,
  `++actionTimer >= numFrames`), `common_landing_action` (×0.98),
  all `*_LAND` + `*_LAND_STOP` actions, flag-masking for A_PRESSED.
- `slides.rs`: `update_sliding` (steer-then-accel, 10k glitch),
  `common_slide_action`/`_with_jump`, all slide actions,
  `ACT_BEGIN_SLIDING` dispatcher, `BUTT_SLIDE_STOP`/`SLIDE_KICK_SLIDE_STOP`/
  `STOMACH_SLIDE_STOP` (object-group, mario_actions_object.c:298).
- `knockback.rs`: `common_ground_knockback_action` (×0.9, anim-end-driven),
  all 7 handlers.
- `punch.rs`: `mario_update_punch_sequence` (actionArg 0–6, anim-driven,
  no frame budgets), `act_punching`/`act_move_punching`.
- `quicksand.rs`: `act_in_quicksand`, `quicksand_jump_land_action`.
- `entry.rs`: moving-group dispatch (`set_mario_action_moving`) wired into
  `ActionCx::transition()`, mirroring decomp `set_mario_action` (mario.c:979).

### Real bugs found and fixed during integration

1. **Stationary preamble used next-tick for squish/death** (idle_crouch.rs):
   the decomp's `check_common_stationary_cancels` uses `set_mario_action`
   (same-tick), not `drop_and_set_mario_action`. Fixed to `cx.goto`.
2. **`set_steep_jump_action` missed entry velocities** (entry.rs): the
   decomp's `set_mario_action_airborne` ACT_STEEP_JUMP case (mario.c:845)
   sets vel.y=42+fwd*0.25 (halved if squished/quicksand) and pitch -0x2000.
   Added.
3. **`mario_set_forward_vel` didn't update slideVel** (step.rs, Phase-4 bug):
   the decomp (mario_step.c:82) re-derives slideVel/vel from facing.
   Fixed; this unblocked the slide-bonk reflection.
4. **Punch preamble infinite loop** (punch.rs): the worker added idle's
   `quicksand_depth > 30 → IN_QUICKSAND` check to `act_punching`, causing an
   IN_QUICKSAND↔PUNCHING same-tick cycle. Removed (the decomp's act_punching
   has no such check).

### Why the exit bar is not met

The 16/92 diff is unchanged because the ground scenarios diverge on
behaviors that require deeper investigation:

- `turn_around`, `turn_around_full`: frame-31 pos_x divergence (-5.44 vs 0.0).
  The plugin moves in -x during TURNING_AROUND; the oracle does not. The
  facing is not supposed to rotate during the turn (decomp moving.c:962),
  but something is producing x-velocity. Root cause not yet isolated.
- `walk_then_release`: frame-31 pos_z off by 2.0 (decel model).
- `walk_slow_turn`: frame-16 slide_vel_x 0.0 vs -3.97 (slide vector not set
  during turn).
- `walk_into_wall`, `walk_off_ledge`: vel_z divergences (6.0 vs 20.96/12.36).
- Slide scenarios (`slide`, `slippery_slide`, etc.): not yet passing.
- `butt_slide_ramp`: butt slide was disabled in v10 (known gap); the new
  port may fix it but the scenario still fails.

The port is faithful at the function level (verified against decomp source
for each `act_*`), but system-level divergences remain. These likely involve
interactions between the new ground code and unported systems (air actions,
automatic actions, or Phase-4 step routines) rather than errors in the
ground ports themselves.

## Phase 5 (2026-10-11): ground/stationary actions GR-01..GR-14

Seven modules ported verbatim with decomp line citations: `walk.rs`
(walking family, 0x471C held-back), `idle_crouch.rs`, `landings.rs`
(full landing system, ×0.98), `slides.rs` (+BEGIN_SLIDING dispatcher),
`knockback.rs` (×0.9, 7 handlers), `punch.rs` (actionArg 0–6), `quicksand.rs`,
plus `entry.rs` (moving-group dispatch). 151/151 tests green.

Post-integration verification fixes (main agent):
- Sound-flag stubs (`sound.rs`): the decomp's `play_*` functions set
  `MARIO_ACTION_SOUND_PLAYED`/`MARIO_MARIO_SOUND_PLAYED` in `m->flags`
  (compared state). The port skipped them as "audio". Added flag-faithful
  stubs; fixed call sites in landings.rs, knockback.rs, punch.rs,
  quicksand.rs.
- `landing_step` used `cx.goto` (same-tick) but the decomp returns FALSE
  (next-tick). Changed to `cx.goto_next`. Fixed jump_neutral.
- Audit spawned for all 55 `cx.goto` uses across Phase-5 modules
  (TRUE=same-tick=goto, FALSE=next-tick=goto_next).

Diff: 39 pass / 69 fail (up from 16/92). The coordinator's "16/92
unchanged" report was stale (pre-integration).

## Phase 6 (2026-10-11): air actions AR-01..AR-11 — in progress

Baseline: 45 pass / 63 fail.
Foundation: `air_model.rs` (update_air_without_turn/with_turn,
check_horizontal_wind, common_air_action_step, check_kick_or_dive_in_air,
check_fall_damage, check_fall_damage_or_get_stuck); `set_mario_action_airborne`
in entry.rs (all entry velocities, squish/quicksand downgrade).
Four child agents porting: air_jump.rs, air_dive.rs, air_kb.rs, air_ledge.rs.

## Phase 7 (2026-10-11): water/submerged AR-12 + §4.7 remaining — complete

### Water/submerged (AR-12)
Ported verbatim from `oracle/sm64/src/game/mario_actions_submerged.c` (1576 lines):

**New files:**
- `crates/stepkit-core/src/actions/water_physics.rs` — helpers: `get_buoyancy` (:52),
  `swimming_near_surface` (:44), `set_swimming_at_surface_particles` (:31),
  `perform_water_full_step` (:70), `perform_water_step` (:167), `apply_water_current` (:119),
  `update_water_pitch` (:197), `stationary_slow_down` (:214), `update_swimming_speed` (:230),
  `update_swimming_yaw` (:255), `update_swimming_pitch` (:284)
- `crates/stepkit-core/src/actions/water_swim.rs` — `act_water_plunge` (:953),
  `act_water_idle` (:329), `act_water_action_end` (:373), `act_breaststroke` (:512),
  `act_swimming_end` (:572), `act_flutter_kick` (:609); helpers `common_idle_step` (:305),
  `common_swimming_step` (:439), `reset_bob_variables` (:419), `surface_swim_bob` (:428),
  `play_swimming_noise` (:485), `check_water_jump` (:492)
- `crates/stepkit-core/src/actions/water_combat.rs` — `act_water_punch` (:820),
  `act_backward_water_kb` (:882), `act_forward_water_kb` (:887),
  `common_water_knockback_step` (:866), `act_water_shocked` (:892),
  `act_drowning` (:913), `act_water_death` (:939)
- `crates/stepkit-core/src/actions/water_metal.rs` — `act_metal_water_standing` (:1171),
  `act_metal_water_walking` (:1230), `act_metal_water_jump` (:1312),
  `act_metal_water_falling` (:1366), `act_metal_water_jump_land` (:1408);
  helpers `play_metal_water_jumping_sound` (:1100), `play_metal_water_walking_sound` (:1109),
  `update_metal_water_walking_speed` (:1116), `update_metal_water_jump_speed` (:1142)

**Skipped (out of scope):** all HOLD_* variants (no objects), `act_water_shell_swimming`
(shell), `act_water_throw` (throwing), `act_caught_in_whirlpool` (whirlpool),
`check_water_grab` (grabbing).

**Discrepancies found:**
- `check_water_jump` requires stickY < -60 (stick DOWN), not up. The old test
  expectation was backwards.
- `act_water_idle` on A_PRESSED → BREASTSTROKE, not FLUTTER_KICK or WATER_JUMP.
- `update_swimming_speed` for stationary: `forwardVel -= 2.0`; for moving with
  vel > 16: `-0.5`. The old tests expected different values.
- Breaststroke chain resets strength to MIN (160), does not increment.

### §4.7 remaining actions
Ported to `crates/stepkit-core/src/actions/extra_air.rs`:
- `act_twirling` (`mario_actions_airborne.c:680`) → `Twirling`
- `act_twirl_land` (`mario_actions_stationary.c`) → `TwirlLand`
- `act_butt_slide_air` (`mario_actions_airborne.c:1435`) → `ButtSlideAir`
- `act_start_hanging` (`mario_actions_automatic.c:385`) → `StartHanging`
- `act_hanging` (`mario_actions_automatic.c:422`) → `Hanging`

Added `ActionId::TWIRL_LAND` (0x18800238), `ActionId::HANGING` (0x00200349),
`ActionId::HANG_MOVING` (0x0020054A), `CharacterState::twirl_yaw` (s16).

### Results
- `cargo test --workspace --exclude stepkit-godot`: **151/151 green**.
- `oracle diff --all`: **69 pass / 39 fail** (unchanged from Phase 6).
  - `swim.ron`: FAIL at frame 31, field `peak_height` (plugin 0.0 vs oracle -0.0)
    — signed-zero only; was failing at frame 1 before.
  - `water_jump.ron`: FAIL at frame 31, field `peak_height` (plugin 0.0 vs oracle -0.0)
    — signed-zero only; was failing at frame 1 before.
- 6 water anchor tests updated with decomp citations (old custom-behavior expectations).

Diff: 69 pass / 39 fail. Water scenarios now run 31 frames matching (vs 1 before);
remaining diff is signed-zero in `peak_height` (strict comparison per plan §5).
