# Decomp Parity Plan

**Goal:** make `stepkit-core` reproduce the movement behaviour of the SM64 decompilation
(<https://github.com/n64decomp/sm64>) frame-for-frame, and prove it with automated differential tests
against the decomp's own code.

**Audience:** an implementing agent that has *not* seen the review that produced this plan. This document is
meant to be self-contained. Read sections 0–4 completely before writing code.

**Status of the project policy:** the "clean-room" constraint no longer applies. Reading, porting and
copying logic from the decomp is allowed and expected. (Licensing of the resulting code is a separate
decision for the project owner; see §1.4. Do not make that decision yourself.)

---

## 0. How to use this document

1. Read §1 (goal, scope, definition of done) and §2 (repos, layout, how to build).
2. Read §3 (how the decomp runs one frame) — every finding refers back to it.
3. Read §4 (the findings catalogue). Each finding has an ID (`FD-`, `ST-`, `GR-`, `AR-`, `AN-`). Phases in §6
   reference these IDs.
4. Do the phases in order (§6). **Phase 1 (the oracle) comes first and gates everything.** Without it you cannot
   tell a fix from a regression, and the existing golden traces cannot be trusted (see §1.3).
5. When a finding says "port X verbatim", open the decomp function and port it line by line, preserving
   evaluation order, casts and constants (rules in §5).
6. Anything marked **NOT REVIEWED** was not examined when this plan was written. Treat the decomp as the only
   source of truth there and audit it yourself before porting.

Conventions used below:

- `plugin:path:line` — a location in the plugin repo at commit `a7663eb4…` (see §2.1).
- `decomp:path:line` — a location in the decomp at commit `9921382a…`.
- "Plugin does X" means observed in code. "Decomp does Y" means read from decomp source. Where a claim was
  additionally confirmed by running code, it says **(verified by running)**.

---

## 1. Goal, scope, definition of done

### 1.1 Goal

For identical inputs (stick, buttons, camera yaw per frame) and identical geometry, `stepkit-core` must
produce, every frame, the same `MarioState`-equivalent values as the decomp: position, velocity,
`forwardVel`, slide velocity, face angles, action id, action state/timer/arg, and the other fields listed in §7.3.

### 1.2 Scope

In scope: everything in `crates/stepkit-core` plus the `stepkit-trace`/`stepkit-cli` tooling that tests it.
This covers input processing, trig, the per-frame pipeline, ground/air step routines, floor/wall/ceiling
queries, and all movement actions currently present, plus the actions the decomp has that the plugin lacks and
that the project wants (see §4.7 for the list).

Out of scope unless the project owner asks: rendering, audio, particles, camera, object interactions, cutscenes,
cannon/flying/wing-cap, holding objects, Bowser/other enemy interactions. Keep the code structured so they
can be added later (e.g. keep the `held object` hooks as no-ops).

### 1.3 Why the existing tests do not prove anything

The golden traces in `traces/golden/*.csv` were produced by the plugin's own simulation
(`tools/regen-golden` runs `stepkit trace run`). They encode the plugin's bugs. Example (verified by running
the trace analysis): `traces/golden/jump_hold_a.csv` rows 44→45 show `pos_y` going from 81.47 with
`vel_y = -44.46` to `0.0` in one frame — a mid-air teleport to the floor (finding ST-01). The golden
`jump_neutral.csv` shows a tap jump that "lands" at its apex (`pos_y = 51.0` → `0.0`).
The README's "33 scenarios" is stale; there are 52 scenario files in `traces/scenarios/`.

**All golden traces must be regenerated from the oracle (Phase 1/2), never from the plugin.**

### 1.4 Licensing note for the owner (not for the implementer to resolve)

The decomp's code is derived from Nintendo's copyrighted game. The plugin currently declares
`MIT OR Apache-2.0`. Once decomp logic is ported, that licence claim is questionable. This plan does not
change the licence. The implementer should (a) not remove the existing `LICENSE`, (b) add a short
`NOTICE`/README paragraph stating the code is derived from the decomp, and (c) flag the question to the owner
in the final report. This is not legal advice.

### 1.5 Definition of done

1. `cargo run -p stepkit-cli -- oracle diff --all` reports **zero** divergent frames, zero tolerance, across all
   scenarios (§7.2) and the fuzz suite (§7.4).
2. CI fails if any divergence appears.
3. The Godot binding (`stepkit-godot`) still builds and its public API is unchanged (§8 explains the one
   caveat about non-triangle collision backends).
4. Docs (README, `docs/*`) describe the project as decomp-derived and describe how to run the oracle diff.

---

## 2. Context: repositories and tooling

### 2.1 The plugin repo (target of the work)

- Repo: <https://github.com/01Case10/mario64_movement_plugin>, reviewed at commit
  `a7663eb410b572f7852f4d49068694db31c0b04e` ("Continuous moveset showcase…", 2026-10-08).
- Rust workspace (`Cargo.toml`): `stepkit-core`, `stepkit-anim`, `stepkit-trace`, `stepkit-godot` (GDExtension,
  needs Godot 4.7), `stepkit-cli`. Rust ≥ 1.80, `glam 0.30`.
- 30 Hz fixed-step simulation, deterministic, pure Rust.
- Key files (all under `crates/stepkit-core/src/`):

| File | What it holds | Lines |
|---|---|---|
| `step.rs` | `tick()` (per-frame driver), `step_ground()`, `step_air()`, `Timeline` | ~380 |
| `input.rs` | `RawInput`, stick magnitude/yaw, `StickMapper` | ~130 |
| `trig.rs` | sine table (interpolated, generated from `f64::sin`), `atan2` via `f64` | ~90 |
| `angles.rs` | `Angle(i16)` wrapping type, `approach()` | ~110 |
| `params.rs` | `MovementParams` (tunables, incl. invented ones) | ~130 |
| `state.rs` | `CharacterState`, `ActionId` constants | ~430 |
| `world.rs` | `CollisionWorld` trait, `SurfaceClass`, `FloorHit` | ~240 |
| `surface.rs` | `SurfaceWorld` reference backend (triangle soup, closest-point wall test) | ~450 |
| `actions.rs` | `ActionCx`, `ActionHandler`, `ActionRegistry` | ~300 |
| `actions/ground.rs` | idle, walk, brake, decel, turn, landings, crouch/crawl, slides, punches, ground knockback | ~1500 |
| `actions/air.rs` | air common model, jumps, dive, pound, wall kick, bonks, air knockback, ledge grab | ~1400 |
| `actions/water.rs` | swimming | ~370 |

- Other crates: `stepkit-trace` (scenario `.ron` format, trace CSV, diff engine, `tests/anchors.rs`),
  `stepkit-cli` (`stepkit trace run|…`, `stepkit anim …`), `stepkit-anim` (animation timing manifest +
  validator; `anim/timing_manifest.toml`, `manifest/timing.ron`).
- Data: `traces/scenarios/*.ron` (52 inputs+geometry), `traces/golden/*.csv` (+ `.sidecar.json`),
  `spec/constants.toml` (1737 lines of "provenance" for constants), `spec/actions/*.md`.
- Tooling: `tools/release-gate.sh` (fmt, clippy, tests, provenance check, similarity audit, scenario gate),
  `tools/regen-golden`, `tools/check-provenance.py`, `tools/similarity-audit.py`.
- Build/test: `cargo test --workspace --exclude stepkit-godot`; `cargo build -p stepkit-cli`;
  `./target/debug/stepkit trace run --scenario traces/scenarios/<id>.ron --out out.csv`.
- Trace CSV columns (current): `frame, stick_x, stick_y, buttons, cam_yaw, pos_x, pos_y, pos_z, vel_x, vel_y,
  vel_z, slide_vel_x, slide_vel_z, fwd_speed, face_yaw, face_pitch, face_roll, action, prev_action, land_from,
  action_state, action_timer, action_arg, floor_y, ceil_y, wall_hit, wall_kick_timer, wall_normal_yaw, health,
  squish_timer, quicksand_depth, peak_height, slide_over_cap, swim_strength, anim_slot, anim_frame`.
- The Godot crate calls `stepkit_core::step::tick(...)` from `crates/stepkit-godot/src/character.rs:327`. Keep the
  `tick` signature stable or update that caller in the same change.

### 2.2 The decomp (the reference)

- Repo: <https://github.com/n64decomp/sm64>, reviewed at commit `9921382a68bb0c865e5e45eb594d9c64db59b1af`
  (2023-08-17). Clone with `git clone https://github.com/n64decomp/sm64`.
- Default/relevant version: **US** (`VERSION_US`). `#ifdef VERSION_JP` branches are not the target.
  Sections guarded by `#if defined(VERSION_SH) || defined(VERSION_CN)` apply only to Shindou/China — the US
  build does **not** have the BLJ fix in `act_long_jump_land` (`decomp:src/game/mario_actions_moving.c:1847`).
  Be careful to follow the US branch whenever `#ifdef VERSION_*` appears.
- Files that matter (all in `src/game/` unless noted):

| File | Content |
|---|---|
| `mario.c` | `execute_mario_action`, `set_mario_action*`, input processing, floor class helpers |
| `mario_step.c` | `perform_ground_step`, `perform_air_step`, `apply_gravity`, ledge-grab check, bonk reflection, quicksand/wind helpers |
| `mario_actions_stationary.c` | idle, crouching, crawl start/stop, stopping actions, landing "stop" actions, ground-pound land |
| `mario_actions_moving.c` | walking family, slides, landings, ground knockback, `update_sliding`, `apply_slope_*` |
| `mario_actions_airborne.c` | air update functions, jumps, dive, pound, wall kick, bonks, air knockback, slide kick |
| `mario_actions_submerged.c` | swimming/water (**NOT REVIEWED**) |
| `mario_actions_automatic.c` | poles, hanging, ledge grab/climb (ledge parts reviewed only superficially) |
| `mario_actions_object.c` | punch sequence and object actions (**NOT REVIEWED**) |
| `engine/math_util.c`, `include/trig_tables.inc.c` | `sins/coss` (macros in `math_util.h`), `atan2s`, `approach_s32/f32` |
| `engine/surface_collision.c`, `engine/surface_load.c` | floor/ceiling/wall queries, surface loading and spatial partition |
| `game_init.c` | `adjust_analog_stick` (stick deadzone/cap), controller update |
| `include/sm64.h` | all `ACT_*` ids and flags, `INPUT_*`, `SURFACE_*` |
| `assets/anims/anim_XX.inc.c` | **tracked in the repo** (193 files) — animation headers incl. `loopEnd`; the file name's hex is the `MARIO_ANIM_*` id (see `include/mario_animation_ids.h`) |

- Function index (file:line) for the most relevant functions is in Appendix A.

### 2.3 Facts about the decomp that were verified on this machine

- Every one of these decomp files **compiles natively with host gcc** (x86-64, `-O1 -std=gnu99`) with only the
  flags in §7.1, no ROM and no N64 toolchain: `engine/math_util.c`, `engine/surface_collision.c`,
  `engine/surface_load.c`, `engine/graph_node.c`, `game/mario.c`, `game/mario_step.c`,
  `game/mario_actions_stationary.c`, `game/mario_actions_moving.c`, `game/mario_actions_airborne.c`,
  `game/mario_actions_submerged.c`, `game/mario_actions_automatic.c`.
- Linking them needs about **80 stub symbols** (sound, camera, object/interaction hooks, save file, debug print,
  a few globals). Exact list in Appendix D.
- `atan2s` was run natively. Results **(verified by running)**: `atan2s(1,0)=0`, `atan2s(0,1)=16384`,
  `atan2s(-64,0)=-32768`, `atan2s(0,64)=16384`, `atan2s(0,0)=0`, `atan2s(30,40)=9672`.
  So `atan2s(y, x)` measures the angle **from the +y axis toward +x**, i.e. it equals the mathematical
  `atan2(x, y)`. Its argument order is the opposite of the usual convention.
- No ROM is required to obtain animation lengths; the `assets/anims` files are in the repo.

---

## 3. How the decomp runs one frame

This is the model the plugin's `tick()` must reproduce. Source: `decomp:src/game/mario.c:1699` (`execute_mario_action`).

```
execute_mario_action():
  mario_reset_bodystate
  update_mario_inputs(m):                         # mario.c:1373
      m->input = 0
      update_mario_button_inputs                  # A/B/Z pressed+down flags; B and Z IGNORED while squishTimer != 0
      update_mario_joystick_inputs                # intendedMag, intendedYaw, INPUT_NONZERO_ANALOG
      update_mario_geometry_inputs                # PUSHES Mario out of walls, finds floor/ceil/water, sets
                                                  #   INPUT_ABOVE_SLIDE / SQUISHED / OFF_FLOOR / IN_WATER ...
      if no analog and no A pressed: INPUT_UNKNOWN_5   # NOTE: this is what "stick released" means
      wallKickTimer--, doubleJumpTimer-- (if > 0)
  mario_handle_special_floors
  mario_process_interactions
  if floor == NULL: return                         # out of bounds: no action runs
  loop:                                            # *** the action loop ***
      cancel = <execute the current action's function>
      repeat while cancel == TRUE                  # set_mario_action() returns TRUE, so a transition makes
                                                   # the NEW action run again in the SAME frame
  sink_mario_in_quicksand, squish_mario_model (decrements squishTimer), update_mario_health, ...
```

Key consequences used throughout the findings:

- **Action transitions are same-frame.** After `set_mario_action`, the new action's function runs immediately
  with `actionTimer = 0` (reset by `set_mario_action`, `mario.c:979`).
- **`actionTimer` is not auto-incremented.** Each action increments it (or not) itself.
- Many actions begin with a *group preamble* that runs before the action function:
  moving (`check_common_moving_cancels` + `mario_update_quicksand(m, 0.25f)`, `mario_actions_moving.c:1975`),
  airborne (`check_common_airborne_cancels`, `mario_actions_airborne.c:2046`: water plunge, squished,
  vertical wind, and **it sets `quicksandDepth = 0`**), stationary (`check_common_stationary_cancels`, `…stationary.c:1083`).
- `m->forwardVel` is the scalar "speed along facing"; `m->vel[0..2]` is the velocity the step routines use; and
  `m->slideVelX/Z` + `m->slideYaw` are a *separate* slide vector. Helper `mario_set_forward_vel`
  (`mario.c:374`) sets `forwardVel`, `slideVelX/Z` **and** `vel[0]/vel[2]` all from `faceAngle[1]`.
  `apply_slope_accel` (`mario_actions_moving.c:283`) does the same at the end of walking-type updates.
- Ground movement always goes through `perform_ground_step` (4 quarter steps) and air movement through
  `perform_air_step` (4 quarter steps, then `apply_gravity`). Details in ST-03/ST-04.

---

## 4. Findings catalogue

Severity: **C** critical (visibly wrong physics), **H** high, **M** medium, **L** low.
Each finding lists the plugin location, the decomp location, what each does, and the required change.

### 4.1 Foundations (`FD-*`)

#### FD-01 (H) Action loop / same-frame transitions
- Plugin: `plugin:crates/stepkit-core/src/step.rs:37-70` (`tick`) runs *one* handler per frame; a transition
  (`ActionCx::goto`, `actions.rs:72`) returns and the new action first runs next tick. It also auto-increments
  `state.action_timer` at the top of every tick (`step.rs:48`).
- Decomp: §3. The new action runs the same frame; timers are per-action.
- Effect: every transition is one frame late relative to the decomp (e.g. in `jump_hold_a.csv` the jump's first
  displacement happens one frame after the A press: `pos_y` is 0 at the press frame, 47.5 one frame later).
- Change: restructure `tick()` into: inputs/geometry → special floors → loop { run action; continue while it
  signalled a transition }. Add a loop-iteration guard (the decomp has none; use a generous cap such as 64 and
  `debug_assert!`). Remove the automatic `action_timer` increment; each action manages its own timer exactly
  like the decomp. Preserve a way to emit the existing `Event`s.

#### FD-02 (H) Input processing
Plugin: `plugin:input.rs:34-75`. Decomp: `decomp:src/game/game_init.c:424-460` (`adjust_analog_stick`) and
`decomp:src/game/mario.c:1293-1309` (`update_mario_joystick_inputs`).

| Aspect | Plugin | Decomp |
|---|---|---|
| Deadzone | stick "held" when `sqrt(x²+y²) > 8` (`input.rs:57`) | **per axis**: `stickX = rawX±(−6)` only if `|rawX| >= 8`, else 0; same for Y (`game_init.c:431-445`). So raw `(8,0)` is held (value 2); raw `(7,7)` is **not** held. |
| Offset | none | `stickX = rawX + 6` for `rawX <= -8`; `rawX - 6` for `rawX >= 8` |
| Magnitude | `sqrt(x²+y²)`, **uncapped** (up to ~181) | `stickMag = sqrtf(stickX²+stickY²)`; if `> 64`: `stickX *= 64/stickMag; stickY *= 64/stickMag; stickMag = 64` |
| intendedMag | `(raw/64)²·64 / 2` | `mag = (stickMag/64)²·64`; `intendedMag = mag/2`, or `mag/8` when `squishTimer != 0` |
| intendedYaw | `cam_yaw + atan2(-stick_x, stick_y)` (math atan2) | `atan2s(-stickY, stickX) + area->camera->yaw` (decomp `atan2s`; see §2.3). Forward stick gives `0x8000 + camYaw`. |
| "no input" | `!stick_held()` | `intendedMag > 0` → `INPUT_NONZERO_ANALOG`; else `intendedYaw = faceAngle[1]` |
| Dive threshold | `raw_magnitude() > 48` (`ground.rs:290-296`) | `controller->stickMag > 48` (after offset and cap) (`moving.c:490`) |
| Buttons | no squish gating | `B` pressed, `Z` pressed/down are ignored while `squishTimer != 0` (`mario.c:1253-1290`) |

Effect of the cap: at full N64 tilt (raw ≈ 80) the decomp gives `stickMag = 64`, `intendedMag = 32`; the plugin
gives `intendedMag ≈ 50`. Walking is capped at 32 so it hides this, but `update_sliding`, air control and
`tilt` all use `intendedMag/32` unclamped, so slides and air steering are ~56% too strong at full tilt.

Camera yaw convention: the decomp adds the *camera* yaw (the direction from Mario toward the camera), so
"forward" = `camYaw + 0x8000`. The plugin's `RawInput.cam_yaw` means "the direction the camera looks"
(see the test `intended_yaw_follows_camera`). Keep the public API (Godot callers pass the look direction) but
convert at the boundary (`decomp_cam_yaw = cam_yaw + 0x8000`) and **document it**. Do not change the
decomp-facing math.

Change: reimplement exactly. Controller input to the core should be raw `i8` stick values as today (so the
Godot `StickMapper` keeps working), then apply `adjust_analog_stick` verbatim. Add the derived input flags (§FD-05).

#### FD-03 (M) Trig tables and `atan2s`
- Plugin: `plugin:trig.rs` builds a 4096-entry table from `f64::sin`, **linearly interpolates** (`trig.rs:31-45`),
  and implements `atan2` with `f64::atan2` rounded (`trig.rs:48-58`).
- Decomp: `#define sins(x) gSineTable[(u16)(x) >> 4]` and `coss(x) gCosineTable[(u16)(x) >> 4]`
  (`decomp:src/engine/math_util.h:27-28`), tables in `decomp:include/trig_tables.inc.c`; **no interpolation**, so
  angles are quantised to 16-unit steps. `atan2s` (`decomp:src/engine/math_util.c:713`) uses `gArctanTable`
  (same file `trig_tables.inc.c`) with `(s32)(y/x*1024 + 0.5f)` indexing; it returns different values from the
  exact `atan2` by up to a few units (e.g. `atan2s(30,40)=9672` while the rounded exact value is 9673).
- Change: copy the tables (generate a Rust file from `trig_tables.inc.c`, with a build-time or test-time
  check that they match). Port `sins`, `coss`, `atan2s`, `atan2_lookup` verbatim. Rename the plugin's
  `trig::atan2(y, x)` to `atan2s(y, x)` **with the decomp's argument semantics** and update every caller. Current
  callers pass `(n.x, n.z)` meaning the math `atan2(y=x, x=z)`; with `atan2s` the same call is `atan2s(n.z, n.x)`,
  which is how the decomp writes it everywhere. Audit every call site (grep `trig::atan2`).

#### FD-04 (H) Numeric semantics (what "exact" means)
Do all gameplay math with `f32` and the same operation order as the C code. Rules in §5. Concretely the plugin
violates them in: `trig.rs` (uses `f64`), `air.rs` air-control (uses `dyaw_rad.cos()` and radians instead of
`coss(s16)`), `ground.rs::update_slide_vector` (uses `f32::consts::TAU` and `cos()`), `Angle::approach`
(generic clamp instead of the decomp's `approach_s32`), and `Angle::from_degrees` (round-to-nearest). Replace
them with ports of the decomp helpers (`approach_s32/f32` at `decomp:src/engine/math_util.c:657/679`).

#### FD-05 (H) State model: missing fields
The plugin's `CharacterState` (`plugin:state.rs`) lacks several `MarioState` fields the decomp relies on.
Add at least (names are suggestions):

`forward_vel` (keep `forward_speed`), `vel` (kept), `slide_vel_x/z` (kept), `slide_yaw`, `face_angle[3]`
(pitch/yaw/roll — currently `face_pitch`, `face_roll`), `angle_vel[3]`, `intended_yaw`, `intended_mag`,
`input` (bit flags, see below), `floor` (index/handle to the *surface*, not just `floor_y`, because logic reads its
`normal`, `type`, `flags`, `force`), `floor_height`, `floor_angle` (= `atan2s(floor.normal.z, floor.normal.x)`),
`ceil_height`, `wall` (the referenced wall surface, may be `None`), `water_level`, `prev_action`,
`action_arg`, `action_state`, `action_timer`, `double_jump_timer`, `wall_kick_timer`, `frames_since_a/b`,
`squish_timer`, `quicksand_depth`, `peak_height`, `health`, `hurt_counter`, `invinc_timer`, `flags`
(only the bits movement reads), `terrain_type` (area property: slide/snow/sand/default).

Input flags to define (`decomp:include/sm64.h`, search `INPUT_`): `NONZERO_ANALOG, A_PRESSED, OFF_FLOOR, ABOVE_SLIDE,
FIRST_PERSON (always 0 here), UNKNOWN_5, SQUISHED, A_DOWN, Z_DOWN, Z_PRESSED, B_PRESSED, IN_WATER, IN_POISON_GAS,
STOMPED`.

`quicksand` (`quicksandDepth`) and `squish` are ground-truth inputs to jump velocity and walking speed, so they
must be modelled as in the decomp, not simplified.

#### FD-06 (M) Per-action timers and `actionState`
Port each action's own use of `actionTimer`/`actionState`/`actionArg`. Examples where the plugin's meaning
differs: butt/stomach slides check `actionTimer == 5` *before* incrementing (`moving.c:1432, 1520`), crouch slide
increments while `< 30` (`moving.c:1452`), landing actions pre-increment `++actionTimer >= numFrames`
(`moving.c:1760`), `act_air_hit_wall` uses `++actionTimer <= 2` (`airborne.c:1309`). The plugin's blanket
auto-increment makes these off by one in places.

#### FD-07 (L) `ActionId` values
`plugin:state.rs` defines ids by hand. Compared with `ACT_*` in `decomp:include/sm64.h` (script run during the
review): 50 of 58 match. Mismatches:

| Plugin const | Plugin value | Decomp name | Decomp value |
|---|---|---|---|
| `FREEFALL` | `0x4000088C` (provisional) | `ACT_FREEFALL` | `0x0100088C` |
| `CROUCH` | `0x40000010` | `ACT_CROUCHING` | `0x0C008220` |
| `CRAWL` | `0x40000011` | `ACT_CRAWLING` | `0x04008448` |
| `GROUND_POUND` | `0x400008A8` | `ACT_GROUND_POUND` | `0x008008A9` |
| `HARD_BACKWARD_AIR_KB` | `0x010208B2` | `ACT_HARD_BACKWARD_AIR_KB` | `0x010208B3` (**plugin has FORWARD/BACKWARD swapped**) |
| `HARD_FORWARD_AIR_KB` | `0x010208B3` | `ACT_HARD_FORWARD_AIR_KB` | `0x010208B2` |
| `QUICKSAND_JUMP_LAND` | `0x04000476` | `ACT_QUICKSAND_JUMP_LAND` | `0x00000476` |
| `WALL_KICK` | `0x018808B0` | *(no decomp equivalent; `ACT_WALL_KICK_AIR` = `0x03000886` already exists as `WALL_KICK_AIR`)* | — |

Change: use the decomp values and **flags** (the high bits drive behaviour: `ACT_FLAG_AIR`,
`ACT_FLAG_CONTROL_JUMP_HEIGHT`, `ACT_FLAG_INVULNERABLE`, `ACT_FLAG_MOVING`…, `decomp:include/sm64.h:149-170`). The
decomp reads action flags in `apply_gravity`/`should_strengthen_gravity_for_jump_ascent` (`mario_step.c:519`),
the group dispatch in `execute_mario_action`, etc. Remove the bit-30 "provisional" ids. Update
`spec/actions` and any code/test that compares ids. Add the missing stop/start actions (§4.7).

#### FD-08 (H) Per-frame geometry inputs
Decomp `update_mario_geometry_inputs` (`mario.c:1314-1371`), run **every frame before the action**:

1. `f32_find_wall_collision(&pos, 60, 50)` then `(…, 30, 24)` — pushes Mario's *position* out of walls.
2. `floorHeight = find_floor(pos)`; if `floor == NULL`, copy `pos` from the graphical position (`marioObj->header.gfx.pos`)
   and look again. `ceilHeight = vec3f_find_ceil(pos, floorHeight)` (see ST-05).
3. `waterLevel`, `gasLevel`.
4. If floor exists: `floorAngle = atan2s(floor.normal.z, floor.normal.x)`; `INPUT_ABOVE_SLIDE` if
   `pos.y > waterLevel - 40 && mario_floor_is_slippery(m)`; `INPUT_SQUISHED` for dynamic floors with ceil-floor
   in [0,150]; `INPUT_OFF_FLOOR` if `pos.y > floorHeight + 100`; `INPUT_IN_WATER` if `pos.y < waterLevel - 10`.
5. If `floor == NULL`: `level_trigger_warp(DEATH)` and `execute_mario_action` returns before any action.

The plugin has none of this (it queries floors inside each action's `ground_prelude`, `ground.rs:45`). The
flags `INPUT_ABOVE_SLIDE`/`INPUT_OFF_FLOOR` are what most actions branch on.

#### FD-09 (M) Surface model
- Plugin: `SurfaceKind {Default, Slide, Quicksand, Custom(u8)}` mapped to a `SurfaceClass`
  (`plugin:world.rs:7-70`). `SurfaceKind::Slide` is used to mean "slide terrain".
- Decomp: floors have a `type` (`SURFACE_*`), `flags`, `force`, `normal`, `originOffset`; the slipperiness class
  comes from `mario_get_floor_class` (`mario.c:387`) which depends on both the surface type **and the area
  `terrainType`** (`TERRAIN_SLIDE` → at least `VERY_SLIPPERY` default class). The area terrain type is a property
  of the area, not of a surface. Several helpers read `(m->area->terrainType & TERRAIN_MASK) == TERRAIN_SLIDE`
  (`mario_floor_is_slippery/slope`, `should_begin_sliding`).
- Change: introduce a `SurfaceType` mirroring the `SURFACE_*` values the project wants (start with: default,
  slippery, very slippery (incl. ice), not slippery, the quicksand family, burning, horizontal/vertical wind,
  hangable ceiling, slow, instant-warp can be ignored), carry `flags` and `force`, and add an area-level
  `terrain_type`. Keep `SurfaceKind` as a convenience on top if the Godot adapter needs it.

### 4.2 Collision and step routines (`ST-*`)

#### ST-01 (C) Air landing rule — mid-air teleport  **(verified by running traces)**
- Plugin: `plugin:step.rs:244-267`. Landing is taken when
  `vy <= 0 && find_floor(proposed, 0.0)` exists and `proposed.y - f.y <= params.air_landing_snap_window (78)`
  (`params.rs:100`). I.e. the character lands whenever it is **within 78 units above** a floor while falling, and
  is teleported down to it.
- Decomp: `perform_air_quarter_step` (`decomp:mario_step.c:388-502`) lands only when
  `nextPos[1] <= floorHeight` (`mario_step.c:~432`): the position is at or below the floor.
  The "78" in the decomp is the **floor search tolerance upward**: `find_floor_from_list` accepts a floor whose
  height is `<= y + 78` (`decomp:src/engine/surface_collision.c:459`, `if (y - (height + -78.0f) < 0.0f) continue;`).
  The wiki phrase the plugin cited was misread.
- Evidence: golden `jump_hold_a.csv`: frame 44 `pos_y = 81.47`, `vel_y = -44.46`; frame 45 `pos_y = 0.0`, action
  `JUMP_LAND`. Golden `jump_neutral.csv`: a one-frame A tap rises 42 → 48.5 → 51.0 and "lands" at frame 5 at the apex.
  Simulating a held jump from `y=0, vy=42` with four quarter-steps and gravity 4 (small script run during the review):
  the decomp lands on air frame **22**; the plugin's rule lands on frame **21**.
- Also: the plugin guards landing with `vy <= 0`; the decomp has no such guard (see `ST-04` for the full order
  of checks).
- Change: delete `air_landing_snap_window`; port `perform_air_quarter_step` and its helpers verbatim (ST-04).
  Remove the "pedro spot" re-implementation in `step.rs:253-266` in favour of the decomp's version
  (`if (ceilHeight - floorHeight > 160.0f) { update xz, floor }` else keep xz; y still snaps to floor).

#### ST-02 (H) Floor query semantics
- Plugin: `CollisionWorld::find_floor(pos, max_above)` returns the highest floor with `height <= pos.y + max_above`.
  Callers use `max_above = 1.0` almost everywhere (`step.rs:128,168`, many in `ground.rs`) or `0.0` (`step.rs:250`).
- Decomp `find_floor(x, y, z, &floor)` (`surface_collision.c:~500-580`):
  - coordinates are **truncated to `s16`** (`TerrainData = s16`) before the lookup: "`TerrainData x = (TerrainData) xPos`";
  - out-of-range `|x|,|z| >= LEVEL_BOUNDARY_MAX (8192)` → no floor, height `FLOOR_LOWER_LIMIT (-11000)`;
  - floors are stored per spatial cell **sorted by height descending**, and the *first* one whose triangle contains
    `(x,z)` and satisfies `height <= y + 78` is returned (`find_floor_from_list`, `:401-460`; so higher floors can hide
    lower ones — "surface cucking");
  - `height = -(x*nx + z*nz + originOffset)/ny` using the integer `x,z`;
  - a floor of type `SURFACE_INTANGIBLE` is skipped by re-searching from `height - 200` unless `gFindFloorIncludeSurfaceIntangible`;
  - dynamic (object) surfaces are checked too and win if higher.
- Effect: the plugin cannot step onto anything taller than ~1 unit via the floor query and instead depends on its
  own `f.y > pos.y + 100` check (`step.rs:170`), which the decomp does not have in that form.
- Change: replace the trait method with the decomp contract, e.g. `find_floor(x: f32, y: f32, z: f32) -> Option<FloorHit>`
  with the +78 tolerance, `s16` truncation and ordering rules *inside the reference backend*. Port
  `surface_load.c` partitioning (cell size 1024, 16×16 cells, `LEVEL_BOUNDARY_MAX = 8192`, see
  `decomp:src/engine/surface_load.c` and `surface_collision.h`) so overlapping geometry resolves identically.

#### ST-03 (H) Ground step
- Plugin: `plugin:step.rs:112-190` (`step_ground`). Decomp: `perform_ground_quarter_step` / `perform_ground_step`
  (`decomp:mario_step.c:258-346`).
- Differences:
  1. Wall resolution. Decomp: `lowerWall = resolve_and_return_wall_collisions(nextPos, 30, 24)` **then**
     `upperWall = resolve_and_return_wall_collisions(nextPos, 60, 50)` — both run in sequence on the same `nextPos`
     (each pushes it). `m->wall = upperWall` (the lower wall is **never** the referenced wall). Plugin: runs both
     queries on the *original* proposed position, uses the lower result if it hit, else the upper (`step.rs:139-141`),
     so a lower-only hit becomes a "wall hit" and an upper push is skipped.
  2. Floor. Decomp: `floor = find_floor(nextPos)`; if `NULL` → `GROUND_STEP_HIT_WALL_STOP_QSTEPS` (a **wall stop**, not a
     walk-off). Plugin: `None` ⇒ `walked_off = true` and continues (`step.rs:162-166`).
  3. Leaving the ground. Decomp: if `nextPos.y > floorHeight + 100`: if `nextPos.y + 160 >= ceilHeight` → stop (wall);
     else `GROUND_STEP_LEFT_GROUND` (stay at `nextPos`, break out of the quarter-step loop). Plugin: leaves when floor is more than
     `params.ground_step_down = 24` below (`params.rs:103`, marked "(verify)" in the plugin).
  4. Otherwise: if `floorHeight + 160 >= ceilHeight` → stop; else snap `pos = (x, floorHeight, z)`, `m->floor = floor`,
     and if `upperWall != NULL` compute `wallDYaw = atan2s(upperWall.normal.z, upperWall.normal.x) - faceAngle[1]`;
     within `[0x2AAA, 0x5555]` or `[-0x5555, -0x2AAA]` → `GROUND_STEP_NONE`, else `GROUND_STEP_HIT_WALL_CONTINUE_QSTEPS`.
     The plugin does the equivalent window on `abs(delta)` (`step.rs:148-152`) but on a different wall.
  5. Result codes. The loop breaks only on `LEFT_GROUND` or `HIT_WALL_STOP_QSTEPS`; `CONTINUE` continues. Final
     `CONTINUE → HIT_WALL`. The plugin returns booleans.
  6. Movement scale per quarter step: `m->floor->normal.y * (m->vel[0] / 4.0f)` using the **current `m->floor`** (updated each quarter
     step). The plugin uses `ny` of the local `floor` variable too, initial value from a separate query. Use `m->floor`.
  7. Terrain sound addend etc. — ignore.
- Change: port `perform_ground_quarter_step`, `perform_ground_step`, `stationary_ground_step`
  (`mario_step.c:236`), `mario_update_moving_sand` and `mario_update_windy_ground` (`:172,:190`). The step returns an
  enum `{None, LeftGround, HitWall, HitWallStopQSteps(internal)}`.

#### ST-04 (H) Air step
- Plugin: `plugin:step.rs:225-292`. Decomp `perform_air_step` / `perform_air_quarter_step` (`mario_step.c:388-501, 610-657`).
- Differences (in the decomp's order of evaluation):
  1. Two wall queries on a copy of the intended position: `upperWall = resolve(nextPos, 150, 50)` then
     `lowerWall = resolve(nextPos, 30, 50)`. Plugin: one query at `height*0.5 = 80` with radius `params.radius = 50`
     (`step.rs:236`).
  2. `floor = find_floor(nextPos)`; `ceilHeight = vec3f_find_ceil(nextPos, floorHeight)` (ST-05).
  3. `floor == NULL`: if `nextPos.y <= m->floorHeight` → `pos.y = m->floorHeight`, `AIR_STEP_LANDED`; else
     `pos.y = nextPos.y`, `AIR_STEP_HIT_WALL`. (Plugin: no floor ⇒ keeps falling.)
  4. `nextPos.y <= floorHeight` ⇒ **landed** (ST-01) including the pedro-spot rule.
  5. `nextPos.y + 160 > ceilHeight`: if `vel[1] >= 0` → `vel[1] = 0` and (hang check) return `NONE`/`GRABBED_CEILING`;
     else if `nextPos.y <= m->floorHeight` → landed; else `pos.y = nextPos.y`, `HIT_WALL`. (Plugin: ceiling window 160 above
     the *position*, only zeros upward velocity and clamps `y`; `step.rs:228-233`.)
  6. Ledge grab (only if the action passed `AIR_STEP_CHECK_LEDGE_GRAB`): `upperWall == NULL && lowerWall != NULL` →
     `check_ledge_grab` (`mario_step.c:348`); on success `GRABBED_LEDGE`, otherwise **commit `nextPos`** and return `NONE`.
  7. Commit `pos = nextPos`, `floor`, `floorHeight`. If any wall: `m->wall = upperWall ?? lowerWall`;
     `wallDYaw = atan2s(wall.normal.z, wall.normal.x) - faceAngle[1]`; `SURFACE_BURNING` wall → `HIT_LAVA_WALL`;
     `|wallDYaw| > 0x6000` → `m->flags |= MARIO_UNKNOWN_30`, `AIR_STEP_HIT_WALL`.
  8. `perform_air_step` runs 4 quarter-steps with `intendedPos = pos + vel/4`; the returned `stepResult` is the **last
     non-NONE** quarter result; it breaks on `LANDED`, `GRABBED_LEDGE`, `GRABBED_CEILING`, `HIT_LAVA_WALL`. After the loop:
     `if (vel[1] >= 0) peakHeight = pos[1]`, then `apply_gravity` (unless `ACT_FLYING`), then `apply_vertical_wind`.
- Plugin's `peak_height` is "max y seen" (`air.rs::track_peak`), decomp's is "last y while `vel[1] >= 0`". Fall damage (AR-10) uses it.
- Change: port verbatim. Return an enum `AirStep {None, Landed, HitWall, GrabbedLedge, GrabbedCeiling, HitLavaWall}`.

#### ST-05 (M) Ceiling query
- Decomp: `vec3f_find_ceil(pos, height, &ceil)` calls `find_ceil(pos.x, height + 80, pos.z, &ceil)`
  (`mario.c:547`), always passed `floorHeight` as `height`: "the lowest ceiling above `floorHeight + 80`" (not above Mario).
  `find_ceil` truncates coordinates to `s16` like `find_floor` (see `surface_collision.c`, `find_ceil`).
- Plugin: `find_ceiling(pos, height)` returns the lowest ceiling in `[pos.y, pos.y + height]` (`surface.rs:245`),
  with `ceiling_zero_vel_window = 160` (`params.rs:101`).
- Change: replace with the decomp contract; delete `ceiling_zero_vel_window`.

#### ST-06 (H) Wall collision algorithm
- Plugin: `plugin:surface.rs:263-308` pushes along the **closest point on the triangle** with two relaxation passes,
  3D distance `< radius`.
- Decomp: `find_wall_collisions_from_list` (`decomp:src/engine/surface_collision.c:19-150`):
  - y-window test `y < lowerY || y > upperY` (`lowerY = minY - 5`, `upperY = maxY + 5`, `surface_load.c:374`);
  - signed **plane distance** `offset = n·p + originOffset`, skip if `|offset| > radius`;
  - point-in-triangle in a projection plane: walls with `SURFACE_FLAG_X_PROJECTION` project on (z,y), others on (x,y);
    classification set in `surface_load.c:113-124` (floor `normal.y > 0.01`, ceiling `< -0.01`, else wall; projection
    chosen by `|normal.x| > |normal.z|`);
  - push `x += n.x * (radius - offset)`, `z += n.z * (radius - offset)` **without updating the local x,z** (so multiple
    walls over-push — "wall overlaps"); at most 4 walls recorded; `numWalls - 1` is returned as "the wall";
  - radius capped at 200; camera-only/vanish-cap special cases.
- Change: port verbatim into the reference backend (`SurfaceWorld`), including the spatial partition lookup
  (`find_wall_collisions` iterates the dynamic then static wall lists of the cell).

#### ST-07 (H) Gravity chain and jump-height control
- Decomp `apply_gravity` (`mario_step.c:535-581`) is an `if/else-if` chain in this order:
  `TWIRLING && vel[1] < 0` → twirl gravity; `SHOT_FROM_CANNON` → −1/cap −75; `LONG_JUMP || SLIDE_KICK || BBH_ENTER_SPIN` →
  **−2**/cap −75; `LAVA_BOOST || FALL_AFTER_STAR_GRAB` → −3.2/cap −65; `GETTING_BLOWN` → `-gettingBlownGravity`/cap −75;
  **`should_strengthen_gravity_for_jump_ascent` → `vel[1] /= 4.0f` (and no other gravity this frame)**;
  `ACT_FLAG_METAL_WATER` → −1.6/cap −16; wing cap glide; default −4/cap −75.
- `should_strengthen_gravity_for_jump_ascent` (`mario_step.c:519`): requires `m->flags & MARIO_UNKNOWN_08` (set in
  `set_mario_action_airborne`), not `INTANGIBLE|INVULNERABLE`, `!(input & A_DOWN)`, `vel[1] > 20`, and the action having
  `ACT_FLAG_CONTROL_JUMP_HEIGHT` (jump, double jump, steep jump, wall kick air, long jump (irrelevant: earlier branch), top of pole jump, butt slide air,
  flying triple jump, riding shell jump, hold jump, air throw, special triple jump — `sm64.h:256-289`).
- Plugin: `plugin:air.rs:250-258` quarters `vy` and **then also subtracts gravity** the same frame; per-action
  `height_control` booleans are hand-set (e.g. `air_action!(DoubleJump …)` "(verify)"); `SlideKick` uses gravity 4
  (it should be 2). Also `steep jump` duplicates the logic (`air.rs:1229`).
- Change: port `apply_gravity`, `should_strengthen_gravity_for_jump_ascent`, `apply_twirl_gravity`,
  `apply_vertical_wind` exactly; derive jump-height control from the action flags.

#### ST-08 (M) Other step helpers to port
`mario_bonk_reflection` (`mario_step.c:89`), `mario_push_off_steep_floor` (`:158`), `mario_update_quicksand` (`:107`; its depth caps
10/25/60/160 by surface type and the death/instant rules), `stop_and_set_height_to_floor` (`:223`), `check_ledge_grab` (`:348`),
`set_vel_from_yaw/pitch_and_yaw` (`:659,:665`).

### 4.3 Ground actions (`GR-*`)

For each ground action the plugin file is `actions/ground.rs`. The decomp reference is `mario_actions_moving.c`
unless noted.

#### GR-01 (M) Walking speed update
- Decomp `update_walking_speed` (`moving.c:434-465`): max target 24 if `floor.type == SURFACE_SLOW` else 32;
  `targetSpeed = min(intendedMag, max)`; `if quicksandDepth > 10: targetSpeed *= 6.25 / quicksandDepth` (note `6.25` is a
  C **double**); then `forwardVel <= 0 → += 1.1`; `<= targetSpeed → += 1.1 - forwardVel/43`; `else if floor.normal.y >= 0.95 → -= 1`;
  cap 48; **face angle** `= intendedYaw - approach_s32((s16)(intendedYaw - faceAngle), 0, 0x800, 0x800)`; **then** `apply_slope_accel`.
- `apply_slope_accel` (`moving.c:283-332`): only applies slope speed if `mario_floor_is_slope(m)` (normal.y ≤ 0.9659258 for
  default class, 0.9848077 slippery, 0.9961947 very slippery, 0.9396926 not slippery); direction by
  `floorDYaw = floorAngle - faceAngle[1]` within ±0x4000; `slopeAccel` 1.7/2.7/5.3/0 by class (class forced to default for
  soft ground knockbacks); then sets `slideYaw`, `slideVelX/Z`, `vel`, and calls `mario_update_moving_sand`/`windy_ground`.
- Plugin: `walk_body` (`ground.rs:210`) and `slope_speed_delta` (`ground.rs:144`):
  - no `SURFACE_SLOW` cap, no quicksand scaling;
  - slope speed is added whenever the floor has any tilt (`steep > 1e-4`), scaled by `steepness`, **not gated by
    `mario_floor_is_slope`**: on a 5° default floor the decomp adds nothing, the plugin adds ~0.15/frame;
  - slope accel is applied **before** the turn and uses the *old* facing; decomp applies it after the turn using the *new* facing;
  - `slide_vel_*`, `slide_yaw` are not updated by walking.
- Change: port `update_walking_speed`, `apply_slope_accel`, `apply_slope_decel` (`:393`), `apply_landing_accel` (`:334`),
  `update_decelerating_speed` (`:420`), `mario_floor_is_slope/slippery/steep`, `mario_facing_downhill`
  (`mario.c:556-672`) verbatim.

#### GR-02 (M) Walking cancel order and facing on entry
- Decomp `act_walking` (`moving.c:777`), in order: drop held object; `should_begin_sliding` → `ACT_BEGIN_SLIDING`;
  `INPUT_FIRST_PERSON`; `A_PRESSED` → `set_jump_from_landing`; `check_ground_dive_or_punch`; `INPUT_UNKNOWN_5` →
  `begin_braking_action`; `analog_stick_held_back && forwardVel >= 16` → `TURNING_AROUND`; `Z_PRESSED` → `CROUCH_SLIDE`;
  then `actionState = 0`; `update_walking_speed`; `perform_ground_step`: `LEFT_GROUND` → `FREEFALL`, `HIT_WALL` →
  `push_or_sidle_wall` + `actionTimer = 0`; `check_ledge_climb_down`.
- `analog_stick_held_back`: `intendedDYaw = intendedYaw - faceAngle[1]`; true if `< -0x471C || > 0x471C`.
- Plugin `Walking::tick` (`ground.rs:345-398`): long jump from `A pressed && Z held && speed >= 16` (not in the decomp —
  long jump is only reached from `act_crouch_slide`, `moving.c:1452`); jump before B before Z; slide check
  **after** jump/B/Z (decomp: first); turn-around threshold `> 0x4000` (decomp `0x471C`); `Z` while `forward_speed > 0` →
  crouch slide else crouch (decomp: always `CROUCH_SLIDE`); "stick released" uses `!stick_held()` and `begin_braking_action`
  *after* the turn-around check (decomp: `INPUT_UNKNOWN_5`, i.e. no analog **and** no A, *before* it).
- `begin_braking_action` (`moving.c:501`): `actionState == 1` → `STANDING_AGAINST_WALL` (facing = `actionArg`);
  `forwardVel >= 16 && floor.normal.y >= 0.17364818` → `BRAKING`; else `DECELERATING`. The plugin's constant `0.1736` (`ground.rs:193-204`) is a
  truncation of `0.17364818`.
- `check_ground_dive_or_punch` (`moving.c:485`): `B_PRESSED` → if `forwardVel >= 29 && stickMag > 48` then `vel[1] = 20`,
  `ACT_DIVE` arg 1; else `ACT_MOVE_PUNCHING` (the plugin chooses stationary vs moving punch by speed ≥ 8 — **NOT REVIEWED**
  whether that exists in the decomp for this path; read `check_common_idle_cancels` which sends idle to `ACT_PUNCHING`).
- Change: port `act_walking` as written. Remove the "A+Z held" long-jump shortcut. Port `push_or_sidle_wall`
  (`moving.c:670`) and `check_ledge_climb_down` (`moving.c:100`) (needed for `STANDING_AGAINST_WALL`/`LEDGE_CLIMB_DOWN`).

#### GR-03 (M) Idle and entry transitions
- Decomp `act_idle` (`stationary.c:105`) + `check_common_idle_cancels` (`stationary.c:20`): order is: `quicksandDepth > 30`
  → `IN_QUICKSAND`; poison gas; panting if `health < 0x300` (`actionArg & 1 == 0`); then `check_common_idle_cancels`:
  steep floor (`normal.y < 0.29237169`) → `mario_push_off_steep_floor`; stomped; `A_PRESSED → set_jumping_action(ACT_JUMP)`
  (which routes steep floors to `set_steep_jump_action`, quicksand to `QUICKSAND_JUMP_LAND`); `OFF_FLOOR`; `ABOVE_SLIDE`;
  first person; **`NONZERO_ANALOG → faceAngle[1] = intendedYaw; ACT_WALKING`**; `B_PRESSED → ACT_PUNCHING`;
  `Z_DOWN → ACT_START_CROUCHING`. Then the head-turn animation state machine, sleeping after 10 cycles, and
  `stationary_ground_step`.
- Plugin `Idle` (`ground.rs:301`): jumps via `enter_jump` (no steep/quicksand routing), Z *pressed* (decomp `Z_DOWN`), and `enter_walking`
  never snaps `face_yaw` to `intendedYaw` (the walk then rotates 0x800/frame, the decomp snaps instantly).
- `set_mario_action_moving` (`mario.c:896`) for `ACT_WALKING`: if floor class is not very slippery and `0 <= forwardVel < min(intendedMag, 8)`, set
  `forwardVel = min(intendedMag, 8)`. Plugin (`ground.rs:241-260`) omits the `0 <=` condition (negative speeds get clamped up).
  Likewise `ACT_HOLD_WALKING`, `ACT_BEGIN_SLIDING → BUTT_SLIDE / STOMACH_SLIDE` by `mario_facing_downhill` (strict `-0x4000 < d < 0x4000`).
- Change: port; implement `set_mario_action_moving/airborne` as the decomp does (entry velocities, e.g. §AR-01 list).

#### GR-05 (H) Braking (`act_braking`, `moving.c:1037`)
- Decomp: if not first-person and `input & (NONZERO_ANALOG | A_PRESSED | OFF_FLOOR | ABOVE_SLIDE)` → `check_common_action_exits`
  (`mario.c:1117`: `A → ACT_JUMP` (direct), `OFF_FLOOR → FREEFALL`, `NONZERO_ANALOG → WALKING`, `ABOVE_SLIDE → BEGIN_SLIDING`).
  Then `apply_slope_decel(m, 2.0f)`; stopped → `ACT_BRAKING_STOP` (stationary action, anim-driven, `stationary.c:620`);
  `B_PRESSED → ACT_MOVE_PUNCHING`; `perform_ground_step`: `LEFT_GROUND → FREEFALL`, `HIT_WALL →
  slide_bonk(m, ACT_BACKWARD_GROUND_KB, ACT_BRAKING_STOP)` (`moving.c:137`).
- **`apply_slope_decel(m, 2.0)` decelerates by `2.0 * 2.0 = 4.0` per frame on default floors**: `decel = coef * {0.2 very slippery, 0.7 slippery, 2.0 default, 3.0 not slippery}`,
  then `forwardVel = approach_f32(forwardVel, 0, decel, decel)`, then `apply_slope_accel`.
- Plugin `Braking` (`ground.rs:470-518`): `approach_zero(speed, 2.0)` ignoring class, then adds slope delta; no wall bonk; allows long-jump/Z/dive,
  checks slide first, ends in `IDLE` directly.
- Change: port `act_braking` + `act_braking_stop` (and `stopping_step`, `stationary.c:612`).

#### GR-06 (M) Decelerating (`act_decelerating`, `moving.c:1072`)
- Decomp order: `!FIRST_PERSON`: sliding; `A_PRESSED → set_jump_from_landing`; `check_ground_dive_or_punch`; `NONZERO_ANALOG → WALKING`; `Z_PRESSED →
  CROUCH_SLIDE`. Then `update_decelerating_speed` (`approach_f32(forwardVel, 0, 1, 1)`, `mario_set_forward_vel`, sand, wind); stopped →
  `IDLE`; ground step: `HIT_WALL` → very-slippery floors `mario_bonk_reflection(TRUE)` else `mario_set_forward_vel(0)`.
- Plugin (`ground.rs:520-575`): same 1.0/frame decay (correct) but Z is checked before the stick and creates `crouch` vs `crouch_slide` by speed; no wall handling.

#### GR-07 (H) Turning around (`moving.c:962-1035`)
- Decomp `act_turning_around`: `ABOVE_SLIDE → BEGIN_SLIDING`; `A_PRESSED → set_jumping_action(ACT_SIDE_FLIP)`;
  `UNKNOWN_5 → ACT_BRAKING`; `!analog_stick_held_back → ACT_WALKING`; **`apply_slope_decel(m, 2.0)`** each frame; when it reports stopped →
  `begin_walking_action(m, 8.0f, ACT_FINISH_TURNING_AROUND, 0)` (which sets `faceAngle[1] = intendedYaw` and `mario_set_forward_vel(8)`).
  Facing is **not rotated** while turning. Animation: `forwardVel >= 18 → TURNING_PART1`, else `TURNING_PART2` and at anim end:
  `begin_walking_action(m, forwardVel > 0 ? -forwardVel : 8, ACT_WALKING)`.
- `act_finish_turning_around`: sliding; `A_PRESSED → SIDE_FLIP`; `update_walking_speed`; ground step (`LEFT_GROUND → FREEFALL`);
  at `is_anim_at_end` → `ACT_WALKING`; render yaw `+= 0x8000` (visual only).
- Plugin (`ground.rs:401-468`): rotates facing toward the stick at 0x800/frame, never decelerates, switches to
  `FINISH_TURNING_AROUND` when aligned, and Z crouches; threshold `0x4000`.
- Change: port both; they depend on animation length (AN-01).

#### GR-08 (H) Landings (`moving.c:1722-1950`, `stationary.c:830-1050`)
- Decomp structure: `common_landing_cancels` (`moving.c:1760`) runs first: `floor.normal.y < 0.2923717 → mario_push_off_steep_floor(verySteepAction)`
  (sets `forwardVel = ±16`, `faceAngle = floorAngle(+0x8000)`; the plugin goes to freefall with no push, constant `0.2924` vs `0.2923717`);
  `doubleJumpTimer = unk02` (5, or 0 for triple/backflip); `should_begin_sliding → slideAction`; first person; `++actionTimer >= numFrames → endAction`
  (the `*_LAND_STOP` action, see below); `A_PRESSED → setAPressAction(aPressedAction)`; `OFF_FLOOR → offFloorAction`.
  Per-landing tables: `sJumpLandAction` etc. (`moving.c:20-60`): numFrames 4 (6 for long jump), `aPressedAction` = double jump after
  jump/freefall/side flip landings; **jump after double-jump landing uses `set_triple_jump_action` (`forwardVel > 20`)**; triple/backflip: none.
- `common_landing_action` (`moving.c:1722`): `NONZERO_ANALOG → apply_landing_accel(m, 0.98f)`; else `forwardVel >= 16 →
  apply_slope_decel(m, 2.0f)`; else `vel[1] = 0` only (no horizontal update — velocity stays whatever the air step left);
  ground step (`LEFT_GROUND → airAction`); quicksand adds `(4 - actionTimer) * 3.5 - 0.5` to `quicksandDepth`.
- `apply_landing_accel(m, f)` (`moving.c:334`): `apply_slope_accel`; **if not on a slope: `forwardVel *= f`** (multiplicative!) and zero if `forwardVel² < 1`.
- Plugin `landing_tick` (`ground.rs:586-672`): `approach_zero(speed, 0.98)` (subtractive 0.98), `approach_zero(speed, 2.0)` for the no-stick case
  (decomp 4.0 on default floor), `slope_speed_delta` always added (decomp: only off-slope friction, slope accel inside `apply_slope_accel`), A-chain only while
  `action_timer < frames` (see below), ends in `IDLE` (decomp ends in `*_LAND_STOP`, the anim-driven stationary actions).
- **Double-jump window.** The decomp keeps `doubleJumpTimer = 5` at the last landing frame and `set_jump_from_landing` (`mario.c:1018`) uses
  `doubleJumpTimer != 0` and `prevAction` (`JUMP_LAND`, `FREEFALL_LAND`, `SIDE_FLIP_LAND_STOP` → double jump; `DOUBLE_JUMP_LAND` → triple if `forwardVel > 20`,
  else jump; wing cap special case). The timer persists into `*_LAND_STOP` and `IDLE`, so the window is landing (4 frames) + up to ~4 more.
  The plugin has no timer (`grep double_jump_timer` finds nothing); its comment (`ground.rs` near `landing_tick`) claims the landing covers the window. It does not.
  `check_common_landing_cancels` for the stop actions: `stationary.c:839`.
- Change: port `common_landing_cancels`, `common_landing_action`, each `act_*_land` and `act_*_land_stop`, `landing_step` (`stationary.c:830`),
  `set_jump_from_landing`, `set_jumping_action`, `set_triple_jump_action`, `set_steep_jump_action`.

#### GR-09 (M) Crouch / crawl
- Decomp `act_crouching` (`stationary.c:528`): `A_PRESSED → set_jumping_action(ACT_BACKFLIP)`; `OFF_FLOOR`; `ABOVE_SLIDE`;
  `!Z_DOWN → STOP_CROUCHING`; `NONZERO_ANALOG → START_CRAWLING`; `B_PRESSED → ACT_PUNCHING arg 9`. `act_start_crouching`/`stop_crouching`/
  `start_crawling`/`stop_crawling` are anim-driven (`stationary.c:693-790`).
- `act_crawling` (`moving.c:1247`): sliding; first person; `A_PRESSED → set_jumping_action(ACT_JUMP)`; `check_ground_dive_or_punch`; `UNKNOWN_5 → STOP_CRAWLING`;
  `!Z_DOWN → STOP_CRAWLING`; **`intendedMag *= 0.1f`; `update_walking_speed`** (so target speed ≈ 3.2 at full tilt, same accel curve); step: `HIT_WALL` caps
  speed at 10 and falls through to `align_with_floor`.
- Plugin `Crawl` (`ground.rs:768`): `target = min(intended_mag, 10)` (≈ 10 at full tilt vs decomp ≈ 3.2), linear ±2/frame, no wall handling, Z released → idle.
  `Crouch` (`ground.rs:734`) is instantaneous; the decomp has start/stop animations.

#### GR-10 (H) Slides (`moving.c:159-282, 1353-1590`)
Decomp pieces to port verbatim: `update_sliding`, `update_sliding_angle`, `common_slide_action`, `common_slide_action_with_jump`, `act_butt_slide`,
`stomach_slide_action`, `act_stomach_slide`, `act_dive_slide`, `act_crouch_slide`, `act_slide_kick_slide`, `should_begin_sliding` (`:467`),
`tilt_body_butt_slide` (visual). Plugin equivalents: `update_slide_vector` (`ground.rs:911`) and the `ButtSlide`, `StomachSlide`, `DiveSlide`, `CrouchSlide`,
`SlideKickSlide` structs.

Key mismatches:

| Aspect | Plugin (`ground.rs`) | Decomp |
|---|---|---|
| Order | slope accel → steer → loss | **steer rotation first** (`update_sliding`), then `update_sliding_angle` adds slope accel and multiplies by loss |
| Accel & loss | per class 10/8/7/5 and 0.98/0.96/0.92/0.92 (**correct**) | same constants, loss = `base + intendedMag/32 * forward * 0.02` with `forward = coss(intendedYaw - slideYaw)`; **10k glitch**: `if (forward < 0 && forwardVel >= 0) forward *= 0.5 + 0.5*forwardVel/100` |
| Steering | rotates slide vector toward `intendedYaw` by `0x400*mag` (`approach`) | rotates by `slideVelX += slideVelZ * (intendedMag/32) * sins(dYaw) * 0.05; slideVelZ -= slideVelX * (intendedMag/32) * sins(dYaw) * 0.05` (note: uses the *new* X for Z), then renormalises to the old speed |
| Facing | approaches intended yaw by 0x400, or downhill if no input | derived from slide direction: `faceAngle[1] = slideYaw + newFacingDYaw` where `newFacingDYaw` is nudged ±0x200/frame toward 0 or ±0x8000 depending on quadrant (`moving.c:174-196`) |
| Speed cap | latch-based "one frame late" cap, `slide_over_cap` | `forwardVel = |slideVel|`; `if > 100` scale `slideVel` to 100 **after** `vel` was already set (so this frame is uncapped). Backwards slide: if `newFacingDYaw` beyond ±0x4000, `forwardVel *= -1` |
| Stop condition | `speed < 4 && action_timer > 5`; ignores floor | `!mario_floor_is_slope(m) && forwardVel² < stopSpeed²` (stopSpeed 4 for butt/stomach/crouch slide, 8 for dive slide **and also requires `is_anim_at_end`**, 1 for slide-kick slide) |
| Jump/roll | butt slide: A any time (`enter_jump`); stomach: `action_timer == 5`; crouch slide `< 30` | `actionTimer == 5` gate (`else actionTimer++`) for butt, stomach (A or B, only if `!INPUT_ABOVE_SLIDE`, rollout by `forwardVel >= 0`); dive slide: A/B (no timer) |
| Wall hit | always `slide_bonk` (>16: `GROUND_BONK`, else `DECELERATING`) | `common_slide_action`: **if floor not slippery** → `slide_bonk(ACT_GROUND_BONK, endAction)` (`forwardVel > 16` → reflection + bonk; else stop into `endAction`); **if slippery and a wall is referenced** → bounce: `slideSpeed = max(0.9*|slideVel|, 4)`, `slideYaw = wallAngle - (slideYaw - wallAngle) + 0x8000`, slide continues |
| Air transition | — | `LEFT_GROUND → airAction` (butt: `ACT_BUTT_SLIDE_AIR`, which bounces once, see AR-03); plus `MARIO_HOOHOO` sound if speed > 50 |
| Entry | `capture_slide_vector` copies `vel` | `ACT_BEGIN_SLIDING` picks butt/stomach via `mario_facing_downhill` (strict), then `slideVel` is whatever `apply_slope_accel`/`mario_set_forward_vel` last wrote |
| Class source | surface kind + `SurfaceKind::Slide` special-case | `mario_get_floor_class` incl. area terrain type (FD-09) |

#### GR-11 (H) Ground knockback, bonk, `slide_bonk`
- Decomp `common_ground_knockback_action` (`moving.c:1588`): clamps `forwardVel` to ±32 **every frame**; `animFrame = set_mario_animation(anim)`;
  **`if animFrame < arg2` → `apply_landing_accel(m, 0.9f)` else `mario_set_forward_vel(±0.1)`** (sign of `forwardVel`); ground step: `LEFT_GROUND` → `FORWARD_AIR_KB`
  (if `forwardVel >= 0`) / `BACKWARD_AIR_KB`; else at anim end → `IDLE` (or `STANDING_DEATH` if `health < 0x100`; `invincTimer = 30` if `arg4 > 0`).
  `arg2` per action: hard backward 43, hard forward 21, backward 22, forward 20, soft backward 100, soft forward 100, ground bonk 32 (`moving.c:1641-1695`).
- **The plugin uses these `arg2` numbers as the total action length** (`ground_kb_action!(…, 22/20/43/21/100/100)`, `GroundBonk: >= 32`, `ground.rs:1381-1411,1482`).
  They are only the animation frame at which deceleration stops; the action ends when the animation ends (AN-01).
- Plugin's decay is subtractive `approach_zero(speed, 0.9)`; decomp is multiplicative `×0.9` (GR-08).
- `slide_bonk` (`moving.c:137`): `forwardVel > 16` → `mario_bonk_reflection(TRUE)` then `drop_and_set_mario_action(fastAction)`; else `forwardVel = 0` and
  `set_mario_action(slowAction)`. `mario_bonk_reflection(m, TRUE)` (`mario_step.c:89`): `faceAngle[1] = wallAngle - (faceAngle[1] - wallAngle)`, `forwardVel = -forwardVel`
  (via `mario_set_forward_vel`). The plugin's `enter_ground_bonk` (`ground.rs:1423`) reflects the *slide vector* across the wall normal instead, clamps to 32 at entry only,
  and the slow case goes to `DECELERATING` (decomp: the supplied `slowAction`, e.g. `BRAKING_STOP`, `BUTT_SLIDE_STOP`).
- Change: port `common_ground_knockback_action`, all `act_*_ground_kb`, `act_ground_bonk`, `slide_bonk`, `mario_bonk_reflection`.

#### GR-12 (M) Punching / ground dive (`act_punching`, `act_move_punching`, `mario_update_punch_sequence`) — **NOT REVIEWED**
The plugin's `PUNCH_STAGE_FRAMES = [8, 8, 12]` and "punch_decel 0.5" are labelled "(decomp-derived)" but were not checked. Audit
`decomp:src/game/mario_actions_object.c` (`mario_update_punch_sequence`), `act_punching` (`stationary.c`-area, grep), `act_move_punching` (`moving.c:840-873`),
which uses `apply_slope_decel(m, 0.5f)`, `actionState` and animation frames.

#### GR-13 (M) Long jump entry
- Decomp: only from `act_crouch_slide` while `actionTimer < 30` and `A_PRESSED && forwardVel > 10` (`moving.c:1452-1480`). Entry
  (`set_mario_action_airborne`, `mario.c:776`): `vel[1] = 30` (via `set_mario_y_vel_based_on_fspeed(30, 0)`), `oMarioLongJumpIsSlow = forwardVel > 16 ? FALSE : TRUE`,
  `forwardVel *= 1.5` capped at 48 (negative speeds are **not** capped: the BLJ). `set_mario_y_vel_based_on_fspeed` halves `vel[1]` when `squishTimer != 0 || quicksandDepth > 1`.
- Plugin: also enters from walking/braking/decel with A+Z-held (remove), and `enter_long_jump` (`air.rs:482`) caps with `.min(48.0)` which also caps nothing for
  negative speeds (matching) but computes `adjusted_jump_vy` with `quicksand_depth > 1 || squish` (matching).
- Decomp `act_long_jump_land` for US: **no** `forwardVel<0 → 0` fix, `A_PRESSED` stripped unless `Z_DOWN` (check the `#if defined(VERSION_SH)…` block at `moving.c:1847-1870` and follow the US branch).

#### GR-14 (M) Quicksand
Plugin: simplified, "v1 treats all quicksand as shallow" (`ground.rs:45-110`), with `IN_QUICKSAND`/`QUICKSAND_JUMP_LAND` approximations
(`ground.rs:1167-1228`) and a 0.95 subtractive decel. Decomp: `mario_update_quicksand` (`mario_step.c:107`) with per-type caps and instant death types,
`sink_mario_in_quicksand` (`mario.c:1545`), `act_in_quicksand` (`stationary.c:509`), `quicksand_jump_land_action` (`moving.c:1920`, multiplicative 0.95 via
`apply_landing_accel`), walking-speed scaling (GR-01), jump halving (`set_mario_y_vel_based_on_fspeed`). Port all.

### 4.4 Air actions (`AR-*`)

#### AR-01 (C) Air velocity model
- Decomp: two update functions (`decomp:mario_actions_airborne.c:186-251`):

```
update_air_without_turn(m):      # used by: jump, double jump, triple jump, backflip, freefall, side flip, wall kick air,
                                 #   long jump (via common_air_action_step), dive, steep... (see call list at the end of this entry)
    if !check_horizontal_wind(m):
        dragThreshold = (action == ACT_LONG_JUMP) ? 48 : 32
        forwardVel = approach_f32(forwardVel, 0, 0.35, 0.35)
        sidewaysSpeed = 0
        if input & NONZERO_ANALOG:
            dYaw = intendedYaw - faceAngle[1]; mag = intendedMag/32
            forwardVel += mag * coss(dYaw) * 1.5
            sidewaysSpeed = mag * sins(dYaw) * 10.0
        if forwardVel > dragThreshold: forwardVel -= 1
        if forwardVel < -16:           forwardVel += 2
        slideVelX = forwardVel*sins(face) ; slideVelZ = forwardVel*coss(face)
        slideVelX += sidewaysSpeed * sins(face + 0x4000) ; slideVelZ += sidewaysSpeed * coss(face + 0x4000)
        vel[0] = slideVelX ; vel[2] = slideVelZ

update_air_with_turn(m):         # used by: butt slide air, hold butt slide air (lines 1440, 1479)
    same, but: forwardVel += 1.5*coss(dYaw)*mag ; faceAngle[1] += 512.0f * sins(dYaw) * mag ;
    vel[0] = slideVelX = forwardVel*sins(face) ; vel[2] = slideVelZ = forwardVel*coss(face)   # no sideways term
```
  Call sites of `update_air_without_turn` (all call `common_air_action_step` or call it directly): `act_jump` (446),
  `act_double_jump`, `act_triple_jump`, `act_backflip`, `act_freefall`, `act_hold_*`, `act_side_flip`, `act_wall_kick_air`, `act_long_jump`, `act_riding_shell_air`,
  `act_dive` (725), `act_air_throw`, `act_water_jump`, `act_steep_jump` (note: steep jump calls `mario_set_forward_vel(0.98*forwardVel)` and `perform_air_step` directly, not the update function),
  `act_forward/backward_rollout`, `act_slide_kick`, `act_jump_kick`, `act_flying_triple_jump`, `act_vertical_wind`, … (grep `update_air_without_turn`).
- Key facts: **forwardVel is a persistent scalar; sideways speed is recomputed every frame from the stick and is not carried over.** Facing is **not**
  changed by `without_turn`.
- Plugin `air_common` (`air.rs:102-175`): decomposes the velocity into forward and sideways relative to facing and **keeps the sideways component**
  (`side *= 0.95`, then `side += mag*sin*10` every frame — an integrator whose steady state is up to ~200× larger than the decomp's 10),
  uses radians/`cos()`, applies a forward drag of 0.35 as `fwd - 0.35` toward zero (matches `approach_f32` only for `|fwd| ≥ 0.35`),
  *and* **rotates facing by `steer_rate(512) * sin * mag` in nearly every air action** (`base_air_config` sets `steer_rate: 512`, only `Dive`, `WallKickAir`,
  knockbacks set 0). That is the `with_turn` behaviour, which the decomp only uses for butt-slide-air.
- Change: port both functions and `check_horizontal_wind` (`:149`); delete the decomposed-velocity model and `AirConfig.steer_rate/air_drag_threshold`.
  The per-action differences become the call sites (§ per-action ports). `vel` is rebuilt from facing and `forwardVel` each frame.

#### AR-02 (M) Long jump drag threshold
Plugin `LongJump` (`air.rs:582`) keeps `air_drag_threshold: 32` from `base_air_config()`; decomp uses 48 for `ACT_LONG_JUMP` (inside `update_air_*`). Consequence: a
long jump at 48 loses 1/frame in the plugin. Superseded by AR-01's port.

#### AR-03 (M) Slide kick
Decomp `act_slide_kick` (`airborne.c:1580`): gravity −2 (ST-07), entry `vel[1] = 12`, `forwardVel = max(forwardVel, 32)`, after 30 frames and
`pos.y - floorHeight > 500` → freefall; first landing bounce: `if actionState == 0 && vel[1] < 0 && floor.normal.y >= 0.9848077 { vel[1] = -vel[1]/2; actionState = 1 }`
(read the function; same pattern as `act_butt_slide_air`, `airborne.c:1435`, whose bounce is `-vel[1]/2`). Plugin (`air.rs:655`) uses `max(fall_speed*0.5, 8.0)` ("working
minimum (verify)") and gravity 4. `act_butt_slide_air` exists in the decomp (uses `update_air_with_turn`) but the plugin has no equivalent; slides that leave the
ground go to `BUTT_SLIDE_AIR`, not `FREEFALL`.

#### AR-04 (H) Jump entry velocities — correct, but the mechanism must be ported
All entry numbers the plugin uses match `set_mario_action_airborne` (`mario.c:776-894`): jump `42 + 0.25*fwd` then `fwd *= 0.8`, double `52 + 0.25*fwd` then `*0.8`, triple `69` then `*0.8`,
backflip `fwd = -16`, `vy = 62`, side flip `vy = 62`, `fwd = 8`, `faceAngle[1] = intendedYaw`, wall kick `vy = 62`, `forwardVel >= 24`, steep jump `42 + 0.25*fwd`,
dive `fwd + 15` capped at 48, long jump (GR-13), slide kick, jump kick `vy = 20`, water jump `42` when `actionArg == 0`. Note `set_mario_action_airborne` also does: (a) turns
`DOUBLE_JUMP`/`TWIRLING` into `ACT_JUMP` when `squishTimer != 0 || quicksandDepth >= 1.0f` (**`>=`, vs `>` in the plugin's `adjusted_jump_vy`**) and (b) sets
`peakHeight = pos[1]`, `flags |= MARIO_UNKNOWN_08`. Port the function wholesale.

#### AR-05 (M) Wall hits in `common_air_action_step` and wall kick
- `common_air_action_step` (`airborne.c:371-445`): `AIR_STEP_LANDED` → `check_fall_damage_or_get_stuck(…, ACT_HARD_BACKWARD_GROUND_KB)` then `set_mario_action(landAction)`;
  `AIR_STEP_HIT_WALL`: if `forwardVel > 16`: `mario_bonk_reflection(FALSE)`, `faceAngle[1] += 0x8000`, and if a wall is referenced `ACT_AIR_HIT_WALL`; else (no wall)
  `vel[1] = 0 if > 0`, then `forwardVel >= 38 → BACKWARD_AIR_KB` else `forwardVel > 8 → mario_set_forward_vel(-8)` and `SOFT_BONK`;
  **if `forwardVel <= 16`: `mario_set_forward_vel(0)`** (the plugin instead slides along the wall: "graze (speed <= 16): …no dampening", `air.rs:216-226`).
  Also `GRABBED_LEDGE → ACT_LEDGE_GRAB`, `GRABBED_CEILING → START_HANGING`, `HIT_LAVA_WALL → lava_boost_on_wall`.
- `act_air_hit_wall` (`airborne.c:1309`): `++actionTimer <= 2`: `A_PRESSED → vel[1] = 52; faceAngle[1] += 0x8000; ACT_WALL_KICK_AIR` (the airborne entry then
  overwrites `vel[1]` to 62 and enforces `forwardVel >= 24`); after that `forwardVel >= 38 → wallKickTimer = 5; vel[1] = min(vel[1],0); BACKWARD_AIR_KB`, else
  `wallKickTimer = 5; vel[1]<=0; forwardVel > 8 → -8; SOFT_BONK`. Notorious quirk: missing `return` makes it run twice per frame.
- `check_wall_kick` (`airborne.c:1147`): `A_PRESSED && wallKickTimer != 0 && prevAction == ACT_AIR_HIT_WALL` → `faceAngle[1] += 0x8000; ACT_WALL_KICK_AIR`.
  It is called only from `act_backward_air_kb`, `act_forward_air_kb`, `act_soft_bonk` (lines 1157, 1171, 1248).
- Plugin: `wall_kick_timer = 5` on **any** wall hit in **any** air action (`air.rs:164`), and `air_cancels` (`air.rs:74-99`) lets A wall-kick from every air action while that timer > 0 (no `prevAction` condition).
- Change: port `act_air_hit_wall`, `check_wall_kick`, the knockback/bonk actions.

#### AR-06 (H) Air knockback (`common_air_knockback_step`, `airborne.c:1097-1146`; `act_backward_air_kb` 1156…)
- Decomp: every frame `mario_set_forward_vel(m, speed)` with `speed = -16` (backward), `+16` (forward), `m->forwardVel` (soft bonk, thrown): **constant horizontal speed, no drag, no steering**,
  then `perform_air_step`; `HIT_WALL`: `mario_bonk_reflection(FALSE)`, `vel[1] = min(vel[1], 0)`, `mario_set_forward_vel(-speed)`; landed →
  `check_fall_damage_or_get_stuck` then ground KB with `actionArg`.
- Plugin: entry speeds are **−15** (`enter_backward_air_kb`, `air.rs:894-925`) and **+16**, and entry sets **`vy = 40`** ("(verify)"); the decomp sets no vertical speed on this path
  (it is `0` when positive after a bonk, or whatever damage interactions set). The plugin then runs the general `air_common` (drag, side accel).
- Change: port `act_*_air_kb`, `act_soft_bonk`, `act_hard_*_air_kb`, `common_air_knockback_step`.

#### AR-07 (M) Steep jump
Decomp `set_steep_jump_action` (`mario.c:741`): if `forwardVel > 0`: `angleTemp = floorAngle + 0x8000; faceAngleTemp = faceAngle[1] - angleTemp; y = sins(faceAngleTemp)*forwardVel; x = coss(faceAngleTemp)*forwardVel*0.75;`
`forwardVel = sqrtf(y*y + x*x); faceAngle[1] = atan2s(x, y) + angleTemp;` then `ACT_STEEP_JUMP` (`mario_floor_is_steep` thresholds 0.9659258 / 0.9396926 / 0.8660254,
**and only when not facing downhill**). `act_steep_jump` (`airborne.c:888`): `mario_set_forward_vel(0.98*forwardVel)` each frame, `perform_air_step(0)`, on landing → `BEGIN_SLIDING`
if `forwardVel < 0` else `JUMP_LAND`, on wall `forwardVel = 0`. The plugin's thresholds match but its velocity transform (`enter_steep_jump`, `air.rs:933`) is a different rotation
(`0.75` on the sideways component in the facing frame). Port the decomp's.

#### AR-08 (M) Dive
`act_dive` (`airborne.c:725`): `update_air_without_turn`; `perform_air_step(0)`; `NONE`: `if vel[1] < 0 && faceAngle[0] > -0x2AAA` pitch `-= 0x200`; `LANDED`: stuck-in-ground check
(`should_get_stuck_in_ground`: snow/sand terrain), else `check_fall_damage` (hard action `ACT_HARD_FORWARD_GROUND_KB`) then `ACT_DIVE_SLIDE`; `HIT_WALL`:
`mario_bonk_reflection(TRUE)`, `vel[1] = min(vel[1], 0)`, `BACKWARD_AIR_KB` (via `drop_and_set_mario_action`). From the ground `check_ground_dive_or_punch` sets `vel[1] = 20` and `ACT_DIVE arg 1`.
Plugin's dive: steer disabled and is otherwise the common model (AR-01).

#### AR-09 (M) Ground pound — **NOT REVIEWED**
Plugin `GroundPound` (`air.rs:767-880`) invents a rise-phase formula `22 - 2*t` for 10 frames, "transition at timer > 14", 320 headroom check, vy −50 then
`-4`. These are labelled "(decomp-derived)". Port `act_ground_pound` (`airborne.c:918-985`) and `act_ground_pound_land` (`stationary.c:1037`) after reading them; both
are animation/frame-count driven.

#### AR-10 (M) Fall damage and squish
Decomp `check_fall_damage` (`airborne.c:61`): only when `action != TWIRLING && floor.type != BURNING && vel[1] < -55`; `fallHeight = peakHeight - pos[1]`; `> 3000` → `hurtCounter += (CAP_ON_HEAD) ? 16 : 24` and `drop_and_set_mario_action(hardFallAction, 4)`;
`> damageHeight (1150, or 600 for ground pound — dead code due to the comparison with `actionState`) && !mario_floor_is_slippery` → `hurtCounter += 8 or 12`, `squishTimer = 30`.
`hurtCounter` is converted to health in `update_mario_health` (`mario.c:1454`). Plugin (`air.rs:284-322`) subtracts 16/8 health directly and uses `peak_height` as max-y; uses
the `fall_speed` argument instead of `vel[1]`. Plus `should_get_stuck_in_ground` (`airborne.c:115`). Port `check_fall_damage_or_get_stuck`, `check_fall_damage`, `update_mario_health`.

#### AR-11 (M) Ledge grab
- `check_ledge_grab` (`mario_step.c:348`): requires `vel[1] <= 0` (not `> 0`); wall displacement must oppose `vel` (dot product); `ledgePos = nextPos - wall.normal * 60` (xz),
  `ledgePos.y = find_floor(ledgePos.x, nextPos.y + 160, ledgePos.z)`; grab only if `ledgePos.y - nextPos.y > 100`; sets `pos = ledgePos`, `floor`, `floorAngle`,
  `faceAngle[1] = atan2s(wall.normal.z, wall.normal.x) + 0x8000`. Only attempted when `upperWall == NULL && lowerWall != NULL` (ST-04).
- Plugin `try_ledge_grab` (`air.rs:950-1034`) is a bespoke algorithm with `wall_probe`, and **deliberately places the character 30 units outside the wall and hangs it `height` below
  the ledge** (its comments say the decomp "clips" so it was changed). Then `LedgeGrab`/`LedgeClimb*` (`air.rs:1248-1410`) are custom. The decomp's `act_ledge_grab`,
  `act_ledge_climb_slow/fast/down`, `update_ledge_climb` (`mario_actions_automatic.c:490-655`) are anim-driven.
- Decision for the owner: exact parity (decomp placement) vs. a visual-friendly deviation. Default for this plan: **match the decomp**; if the Godot demo's rig needs an offset, apply
  it in `stepkit-godot` presentation, not in the core.

#### AR-12 Water/submerged — **NOT REVIEWED**
`plugin:actions/water.rs` (370 lines) vs `mario_actions_submerged.c` (1576 lines). Audit and port when the other phases are done (Phase 7).

### 4.5 Animation-driven timing (`AN-*`)

#### AN-01 (H) Actions that end or change phase on animation state
In the decomp, many actions branch on `is_anim_at_end(m)` (`mario.c:46`, `(animFrame + 1) == curAnim->loopEnd`), `is_anim_past_end` (`animFrame >= loopEnd - 2`), or `animFrame` values.
The animation advances **once per frame outside the action code**, in rendering: `geo_set_animation_globals` → `geo_update_animation_frame`
(`decomp:src/game/rendering_graph_node.c:608-612`, function at `decomp:src/engine/graph_node.c:792`), gated by `animTimer == gAreaUpdateCounter`. `set_mario_animation`
(`mario.c:64`) resets the frame on a new anim id to `startFrame - 1` (so the next advance lands on `startFrame`), and `set_mario_anim_with_accel` (`mario.c:97`) uses a 16.16
fixed-point accumulator (`animFrameAccelAssist`, `animAccel`).

Affected actions (non-exhaustive): idle head-turn cycle and sleep (`stationary.c:105`), `finish_turning_around`, `turning_around` end, all `*_ground_kb`, `ground_bonk`,
all `*_LAND_STOP` and `landing_step`, `braking_stop`, `butt_slide_stop`, `slide_kick_slide_stop`, `start/stop_crouching`, `start/stop_crawling`, `dive_slide` stop, `slide_kick_slide`
stop, ledge actions, punches, ground-pound, swimming.

Data: `assets/anims/anim_XX.inc.c` begins with `static const struct Animation anim_XX[] = { flags, animYTransDivisor, startFrame, loopStart, loopEnd, …}`. Example
`anim_03.inc.c` (id `0x03` = `MARIO_ANIM_DYING_ON_BACK`): flags=1, divisor=189, start=0, loopStart=0, **loopEnd=0x40 (64)**. The hex in the file name is the
`MARIO_ANIM_*` id (`decomp:include/mario_animation_ids.h`). `tools/mario_anims_converter.py` in the decomp builds the runtime table from these files (some files hold two anims,
e.g. `anim_01_02.inc.c`).

- Plugin: `stepkit-anim` + `anim/timing_manifest.toml` + `manifest/timing.ron` approximate lengths and the core tracks a `Timeline { slot, frame, speed, ended }` (`step.rs:21-34`).
- Change: build a generated table `MARIO_ANIM_ID → {flags, startFrame, loopStart, loopEnd}` from `assets/anims` (a small script, committed output), model `animFrame`
  + `animFrameAccelAssist` + `animAccel` in `CharacterState` (they are state: the decomp's gameplay reads them), port `set_mario_animation`, `set_mario_anim_with_accel`,
  `geo_update_animation_frame`, `is_anim_at_end`, `is_anim_past_end`, `is_anim_past_frame` (`mario.c:151`). Keep the rig-facing `Timeline` as a derived view for Godot.
  Each action must request the same animation id the decomp requests, because the end frame depends on it.

### 4.6 Smaller/other differences to fix while porting
- `tick()` clears `warped`, decrements `squish_timer` at the top of every tick (`step.rs:49-51`); the decomp decrements `squishTimer` in `squish_mario_model` after the action (`mario.c:1205`).
- `ground_prelude` water plunge: decomp uses `m->pos[1] < m->waterLevel - 100` via `check_common_moving_cancels` / `check_common_airborne_cancels` and `set_water_plunge_action` (`mario.c:1174`).
- Sliding trigger: plugin treats `SurfaceKind::Slide` as "always slide"; decomp `should_begin_sliding` (`moving.c:467`) = `INPUT_ABOVE_SLIDE && (slideTerrain || forwardVel <= -1 || facingDownhill)`, and `INPUT_ABOVE_SLIDE` itself requires `mario_floor_is_slippery`
  which for slide terrain is `normal.y < 0.9998477` (`mario.c:573`).
- Plugin `begin_sliding` (`ground.rs:806`): stomach slide if the facing is *more than* 0x4000 from downhill; decomp `mario_facing_downhill` is `-0x4000 < d < 0x4000` (strict), so exactly ±0x4000 is stomach in the decomp.
- Plugin `slope_speed_delta` is also added in punches/KB/landing/brake with slightly different rules each; replace by the decomp helper in each port.
- `params.rs` `MovementParams` mixes real constants with invented ones (`ground_step_up 12`, `ground_step_down 24`, `air_landing_snap_window 78`, `ceiling_zero_vel_window 160`,
  `walk_brake_speed 16`, …). After porting, constants that exist only as literals in the decomp should live as literals in the port (or `const`s with the decomp name); remove
  tunables that no longer correspond to anything. If tunables are kept for game-feel (the Godot users may want them), keep them **outside** the parity path (`faithful` flag) and
  make parity tests run with defaults only (the code already has this concept).

### 4.7 Decomp actions the plugin does not implement (port as the project needs them)
Stationary: `START_CROUCHING`, `STOP_CROUCHING`, `START_CRAWLING`, `STOP_CRAWLING`, `BRAKING_STOP`, `BUTT_SLIDE_STOP`, `SLIDE_KICK_SLIDE_STOP`, `JUMP_LAND_STOP`,
`DOUBLE_JUMP_LAND_STOP`, `SIDE_FLIP_LAND_STOP`, `FREEFALL_LAND_STOP`, `TRIPLE_JUMP_LAND_STOP`, `BACKFLIP_LAND_STOP`, `LONG_JUMP_LAND_STOP`, `GROUND_POUND_LAND`,
`STANDING_AGAINST_WALL`, `START_SLEEPING/SLEEPING/WAKING_UP`, `PANTING`, `COUGHING`, `SHIVERING`, `IN_QUICKSAND`, `SHOCKWAVE_BOUNCE`, `FIRST_PERSON`.
Moving: `BEGIN_SLIDING` (dispatcher), `BURNING_GROUND`, `RIDING_SHELL_GROUND`, `HOLD_*`. Airborne: `BUTT_SLIDE_AIR`, `FORWARD/BACKWARD_ROLLOUT` (exist as custom), `GETTING_BLOWN`,
`LAVA_BOOST`, `BURNING_JUMP/FALL`, `TWIRLING`, `SHOT_FROM_CANNON`, `FLYING*`, `SPECIAL_TRIPLE_JUMP`, `TOP_OF_POLE_JUMP`, `VERTICAL_WIND`, `THROWN_*`, `HOLD_*`. Ask the owner which are wanted;
suggested minimum for a movement toolkit: everything in the first two lines plus `BUTT_SLIDE_AIR`, `TWIRLING`, `LEDGE_*`, `HANGING`, water.

---

## 5. Numeric fidelity rules (read before porting anything)

1. Gameplay state is `f32` and angles are `s16` (`i16`) — the decomp's `m->faceAngle` is `s16`, `forwardVel` is `f32`, positions are `f32`, `intendedYaw` is `s16`.
2. **Mirror C's arithmetic conversions.** Many decomp literals are doubles (`6.25`, `0.9`, `0.9f` vs `0.9`, `0.5f`, `1.1f`): `x *= 0.9` on an `f32` is computed in double and
   rounded back. Check `f` suffixes in every expression and replicate them (`(x as f64 * 0.9) as f32` vs `x * 0.9f32`). Example: `slideSpeed *= 0.9` (`moving.c:~1390`) is double;
   `m->forwardVel -= 1.0f` is float; `targetSpeed *= 6.25 / m->quicksandDepth` is double.
3. `s16 += f32` compiles to `(s16)((f32)s16 + f32)`: e.g. `m->faceAngle[1] += 512.0f * sins(dYaw) * intendedMag` → `face = ((face as f32 + expr) as i32) as i16` (truncate toward zero,
   then wrap). Replicate float→int truncation and wrap exactly. Intermediate s16 casts (e.g. `(s16)(m->intendedYaw - m->faceAngle[1])`) wrap.
4. Keep the **order of operations and the exact mix of `f32`/`s32`** (e.g. `approach_s32` takes `s32`s). Do not "simplify" algebra.
5. No `f64` trig, no radians, no `sin()/cos()/atan2()` in simulation code. Use the ported tables only.
6. Rust does not contract `a*b+c` into FMA by default; ensure the **oracle** is compiled with `-ffp-contract=off` so C does not either. On x86-64, `gcc` uses SSE for `float`, which is IEEE-754 single;
   do not use `-ffast-math`, `-m32 -mfpmath=387` or `-march=native`.
7. `sqrtf` ↔ `f32::sqrt` (both IEEE correctly rounded). Division is IEEE.
8. Out-of-range `float → s16/s32` casts are UB in C; the oracle's behaviour on x86 (`cvttss2si` giving `0x80000000`) differs from the N64's saturating behaviour. Avoid generating such inputs in tests (the fuzzer should clamp
   to realistic speeds) and note any place where it matters.
9. Pointer/size assumptions in the decomp: use integer vertex data (`s16`) for surfaces (ST-02).
10. Any place where the decomp has a deliberately buggy behaviour (comments starting with `//!`) is **expected behaviour** — replicate it (e.g. "Quantum Tunneling", "10k glitch",
    "Speed is capped a frame late", "Missing return statement" in `act_air_hit_wall`, `act_crawling` fall-through).

---

## 6. Phases

Dependencies: P0 → P1 → P2 → (P3 → P4) → (P5 ∥ P6) → P7 → P8. Do not start P3+ until P2's diff tool is working; after every task, run the diff and record the first divergence in `docs/parity-log.md`.

### Phase 0 — Project housekeeping (small)
- Update `README.md`, `docs/clean-room.md`, `docs/legal*.md`, `spec/provenance/*`: state the project is decomp-derived; delete the claim "no decompilation was read".
- Remove the clean-room gating from `tools/release-gate.sh` (`check-provenance.py`, `similarity-audit.py`) or turn them into optional tools. Do **not** delete `spec/constants.toml`
  until its values are superseded by code in later phases; mark it deprecated.
- Add a top-level `PARITY.md` (or `docs/parity-log.md`) to record per-scenario status and the first divergence.
- Add `docs/decomp-parity-plan.md` (this file) if not already present.
- Exit: docs consistent; `cargo test --workspace --exclude stepkit-godot` still passes (it will be red later by design: see P2 about retiring old goldens).

### Phase 1 — The oracle (native build of the decomp's movement code)
Feasibility was verified (§2.3). Deliverable: a small C program + static library, `oracle/`, that runs the decomp's real movement code one frame at a time and dumps `MarioState` fields as CSV/binary.

Tasks:
1. Create `oracle/` with a `Makefile`/`build.sh` that clones (or points to) the decomp (`DECOMP_DIR`, default `../sm64`) and compiles the files in §7.1 with the flags in §7.1. Pin the decomp commit in a file (`oracle/DECOMP_COMMIT`).
2. Write `oracle/stubs.c` providing the ~80 missing symbols (Appendix D). Most are no-ops; the ones that need real behaviour are listed in §7.1.
3. Write `oracle/harness.c`: initialise globals (`gMarioState`, `gMarioObject`, `gCurrentArea` with `terrainType`, `camera->yaw`, `gControllers`), load surfaces via the real `load_area_terrain`
   from a collision-data array built from the scenario geometry, run frames (§7.1 describes the per-frame sequence), and print the state.
4. Expose a C ABI used by the Rust side. Either (a) run `oracle` as a subprocess reading a scenario file and writing a CSV (simplest, recommended first), or (b) link a static library into a Rust crate (`stepkit-oracle`, `build.rs` + `cc`).
5. Self-test the oracle against hand-checkable facts: single jump from standing on flat ground lands on air frame 22 (§4 ST-01 script); `atan2s` values in §2.3; walking from rest reaches ~speed 22 after 20 frames at full tilt; BLJ-style speeds do not clamp.
- Exit: `oracle run --scenario traces/scenarios/jump_neutral.ron --out /tmp/o.csv` produces a CSV with the agreed schema (§7.3) in a clean checkout with no ROM.
- Risks: see §7.1 "known pitfalls".

### Phase 2 — Diff tooling and scenario set
1. Extend `stepkit-trace` to read the new CSV schema (§7.3) and add `stepkit oracle diff --scenario <id>|--all [--fields …]` which runs the scenario in both and prints the **first** diverging frame
   and field with a few frames of context, plus an overall table (scenario × status).
2. Convert scenario geometry to integer-coordinate triangles with correct winding/normals (the decomp loader requires `s16` vertices); reject non-integer geometry in the scenario format validator.
3. Regenerate all goldens from the oracle (`tools/regen-golden` → oracle), keep the old ones under `traces/legacy-plugin-golden/` for reference, and make the plugin-golden comparison non-gating.
4. Add scenarios for each finding (list in Appendix C).
5. Add the table to `docs/parity-log.md` and wire `cargo test` (or a CI script) to run the diff.
- Exit: `oracle diff --all` runs end-to-end and **fails** (as expected); every failing scenario shows a first-divergence line.

### Phase 3 — Foundations (FD-01…FD-09)
Order: FD-03 (trig) → FD-04 (numeric helpers, `approach_*`) → FD-05 (state) → FD-02 (inputs) → FD-09 (surface types) → FD-01/FD-06/FD-08 (pipeline, timers, geometry inputs) → FD-07 (ids/flags).
Concretely: port `math_util` helpers; extend `CharacterState`/CSV; implement `adjust_analog_stick` + `update_mario_joystick_inputs` + `update_mario_button_inputs`; implement the surface model, `mario_get_floor_class`,
`mario_floor_is_slope/slippery/steep`, `mario_facing_downhill`; rewrite `tick()` as in §3; implement `update_mario_geometry_inputs` with a stub world until P4.
- Exit: `idle_60f`, `walk_accel_mag{16,32,48,80}_flat`, `strafe_left`, `walk_slow_turn` diverge only for reasons tracked to P4/P5 (flat-floor walking should already match once the step routine is ported).

### Phase 4 — Collision and step routines (ST-01…ST-08)
Port `surface_collision.c`/`surface_load.c` semantics into `SurfaceWorld` (partition, `find_floor`, `find_ceil`, `find_wall_collisions`), then `perform_ground_step`/`perform_air_step`, `apply_gravity`, helpers (ST-08).
- Exit: `jump_neutral`, `jump_hold_a`, `jump_forward`, `walk_off_ledge`, `walk_into_wall`, `slope_*`, `ledge_*` (the geometry part) match; add and match the scenarios for step-up heights (e.g. 30/70/90 units), ceilings, pedro spots.

### Phase 5 — Ground and stationary actions (GR-01…GR-14)
Order: shared helpers (GR-01) → walking family, idle, crouch/crawl → landings and stop actions (needs AN-01 stubs, see below) → slides → knockback/bonk → quicksand.
For anything that needs animation data, implement AN-01's table and `is_anim_at_end` first (it is a prerequisite for the stationary stop actions and for knockback end).
- Exit: all ground scenarios match, including `turn_around`, `walk_then_release`, `slide`, `slippery_slide`, `stomach_slide`, `butt_slide_ramp`, `crouch_slide_chain`, `ground_bonk`, `quicksand`.

### Phase 6 — Air actions (AR-01…AR-11)
Order: `set_mario_action_airborne` + `update_air_*` + `common_air_action_step` → jump family, freefall, side flip, backflip, long jump → wall kick/air hit wall/knockbacks/soft bonk → dive, slide kick, rollouts, steep jump →
ground pound → fall damage → ledge grab.
- Exit: all air scenarios match (`double_jump`, `jump_chain_*`, `triple_jump`, `long_jump*`, `sideflip`, `backflip`, `air_steer_left`, `steep_jump`, `dive_*`, `slide_kick`, `wall_kick*`, `ground_pound*`, `fall_damage_*`, `ledge_*`).

### Phase 7 — Remaining actions and water
Water/submerged (AR-12), punch sequence (GR-12), any actions from §4.7 the owner wants (twirl, butt-slide-air, hanging, etc.). Repeat the audit → port → add scenarios → diff loop.

### Phase 8 — Hardening
1. Random-input fuzzer (§7.4) over several geometries; run in CI with a fixed seed set, and nightly with random seeds; minimise failing inputs automatically.
2. Make `oracle diff --all` + fuzz mandatory CI.
3. Remove `air_landing_snap_window`, `ceiling_zero_vel_window`, `ground_step_up/down`, and any tunable that has no decomp equivalent; or relegate to an explicit "non-faithful" feature (`faithful = false`).
4. Godot adapter contract (§8). Keep demo working; update `demo/` README.
5. Final report to the owner: remaining divergences, un-ported actions, licence note (§1.4).

---

## 7. The oracle and test design in detail

### 7.1 Building and running the decomp natively

Verified compile command (run from the decomp root; produced `.o` files for all listed sources with no errors, warnings suppressed with `-w`):

```
gcc -c -w -O1 -std=gnu99 -DVERSION_US=1 -DTARGET_N64=0 -D_LANGUAGE_C=1 -DNON_MATCHING=1 -DAVOID_UB=1 \
    -I include -I src -I . -I include/libc  -o <out>.o <src>.c
```

Use for the final build, in addition: `-ffp-contract=off -fno-fast-math -fwrapv`? (Do **not** add `-fwrapv`; keep default semantics except where UB bites.) Consider `-m32` if multilib is available; the decomp
assumes 32-bit pointers in a few places (`u32` casts of pointers in `segmented_to_virtual`, `VIRTUAL_TO_PHYSICAL` in `set_mario_animation`). If `-m32` is unavailable, avoid those paths (see animation notes).

Sources to compile: `src/engine/math_util.c`, `src/engine/surface_collision.c`, `src/engine/surface_load.c`, `src/engine/graph_node.c` (for `geo_update_animation_frame`),
`src/game/mario.c`, `src/game/mario_step.c`, `src/game/mario_actions_{stationary,moving,airborne,submerged,automatic}.c`, plus (later, only if wanted) `game_init.c`'s `adjust_analog_stick` — it is a
single function; **copy** it into the harness rather than compiling `game_init.c` (which drags in the whole OS layer).

Per-frame sequence in the harness (mirror the real game; see §3):
1. Set `gControllers[0]`: `rawStickX/Y` (s16), `buttonDown`, `buttonPressed = down & ~prevDown`; call `adjust_analog_stick`. Set `gMarioState->area->camera->yaw` (the harness's `cam_yaw` input, in decomp convention).
   Increment `gGlobalTimer` (used by `mario_update_windy_ground`).
2. Call `execute_mario_action(gMarioObject)`.
3. Advance animation once: emulate `geo_set_animation_globals(&marioObj->header.gfx.animInfo, TRUE)` (`rendering_graph_node.c:608`): `animInfo.animFrame = geo_update_animation_frame(&animInfo, &animInfo.animFrameAccelAssist); animInfo.animTimer = gAreaUpdateCounter` and increment `gAreaUpdateCounter` per frame. Verify against the function in the decomp.
4. Dump the state (§7.3).

Initial state: copy `init_mario()`/`init_mario_from_save_file()` (`mario.c:1788,1871`) minimally — set `action`, `pos`, `faceAngle`, `health = 0x880`, `squishTimer = 0`, `floor`/`floorHeight`, `controller`, `area`, `marioObj`, `animList`.

Stub behaviour that is **not** a no-op:
- `load_patchable_table(animList, id)`: copy the animation's header (from the generated table, AN-01) into `animList->bufTarget` and return `0` so the pointer-patching branch in `set_mario_animation` is skipped (avoids the 64-bit pointer problem).
- `gMarioAnimsBuf`, `gMarioState->animList`: provide a valid `struct MarioAnimDmaRelatedThing`/`animList` with `bufTarget` pointing to a static `struct Animation`.
- `mario_process_interactions`, `mario_handle_special_floors`: no-ops for the first version (the harness has no objects). `mario_check_object_grab` returns 0. `mario_update_punch_sequence` needs a real implementation once punches are in scope (it lives in `mario_actions_object.c`, which can be compiled too).
- `find_water_level`/`find_poison_gas_level` come from `surface_collision.c` and read `gEnvironmentRegions` (loaded by `load_area_terrain`); scenarios with water must supply a region.
- `play_sound`, `spawn_object`, particles, `set_camera_mode`, `level_trigger_warp`, `save_file_*`: no-ops.
- `gCurrentArea`: allocate a `struct Area` with `terrainType` (e.g. `TERRAIN_GRASS`; slide scenarios use `TERRAIN_SLIDE`), `camera` pointer to a `struct Camera` with `yaw`.

Surface loading: build a `TerrainData` array in the format `COL_INIT(), COL_VERTEX_INIT(n), COL_VERTEX(x,y,z)…, COL_TRI_INIT(type, n), COL_TRI(a,b,c)…, COL_TRI_STOP(), COL_END()` (see macros in
`include/macros.h`/`levels/*/…/collision.inc.c` examples, and `read_surface_data` in `surface_load.c:296` for normal/flags computation) and call `load_area_terrain(0, data, NULL, NULL)` (stubbing `main_pool_alloc`, `spawn_macro_objects`, `spawn_special_objects`, etc.).
Winding order determines the normal: copy the decomp's convention (a floor's normal must point up; check `read_surface_data` — it computes the cross product `(v2-v1) × (v3-v2)` and normalises, and flips if `mag < 0.0001`).

Known pitfalls:
- `s16` coordinates: verts and query positions truncate (ST-02). Scenario geometry must be integer and within ±8191.
- `-O1` vs the N64's `-O2`: floating-point results are IEEE-identical for `f32` ops, but code that depends on UB or on intermediate extended precision must be checked; none is known.
- 64-bit pointers: avoid `VIRTUAL_TO_PHYSICAL` paths (above). `struct Surface` contains pointers but is fine natively.
- The animation advance (step 3 above) is easy to get subtly wrong; validate with `idle` head-turn timing (`act_idle` sleeps after 10 head-turn cycles; cycle length depends on `anim_XX` loopEnd) and the `landing` timings.
- Alternative oracle: <https://github.com/libsm64/libsm64> compiles most of the same code into a shared library, but it loads Mario's animation data from a user-supplied US ROM at runtime
  and exposes a different API; the custom harness above avoids the ROM requirement. Use libsm64 only as a cross-check if ever convenient.

### 7.2 What to compare, and how

For each scenario, run it in both implementations frame by frame and compare **all** fields in §7.3 exactly (bitwise `f32` equality via `to_bits`, integer equality for the rest). On mismatch report:
`scenario, frame, field, plugin value, oracle value, previous frame's state of both`, plus the inputs of the last 3 frames. A scenario "passes" at zero mismatches over its full length.
If a divergence is legitimately caused by something outside scope (e.g. an object interaction), the scenario must be rewritten to avoid it; do not add per-field tolerances.

Teacher-forced mode (useful while debugging): at frame N, overwrite the plugin's state with the oracle's frame-N state and step once, then compare frame N+1. This isolates one-frame errors from accumulated drift.
The existing `stepkit-trace` already has a "teacher-forced" comparison concept (`66 checks = 33 × 2`); reuse it.

### 7.3 State schema to dump (oracle → CSV, mirrored by the plugin)

Per frame, after the full frame: `frame`, inputs (`stick_x`, `stick_y`, `buttons`, `cam_yaw`), `pos[3]`, `vel[3]`, `forward_vel`, `slide_vel_x`, `slide_vel_z`, `slide_yaw`, `face_angle[3]`,
`angle_vel[3]`, `intended_yaw`, `intended_mag`, `action`, `prev_action`, `action_state`, `action_timer`, `action_arg`, `input` (flags), `floor_height`, `floor_angle`, `floor_normal_y`, `floor_type`,
`ceil_height`, `wall_normal_yaw` (or `none`), `water_level`, `wall_kick_timer`, `double_jump_timer`, `squish_timer`, `quicksand_depth`, `peak_height`, `health`, `hurt_counter`, `invinc_timer`, `flags`,
`anim_id`, `anim_frame`, `anim_accel`, `anim_frame_accel_assist`. `f32` fields should be printed with enough digits to round-trip (or as hex bits).
Bump a `schema_version` in the sidecar `.json`.

### 7.4 Fuzzing
Generate input sequences: random stick (including edge values `±7, ±8, ±127`, diagonals, dead centre), random button presses with realistic frequency, camera yaw constant or slowly rotating; geometry from a set: flat, 15°/20°/30°/45° slopes of each
surface class, steps of 20/40/70/90/120 units, ledges, pits, low and high ceilings, corner/wall configurations, slide terrain area, quicksand, water. Run N frames (≥ 600). Seeds recorded; failing seeds auto-minimised
(delta-debug the input sequence) and saved as new scenarios.

### 7.5 Anchors
`crates/stepkit-trace/tests/anchors.rs` and `traces/public-anchors/` check numbers transcribed from a wiki. Keep only those the oracle also confirms; otherwise delete (the oracle is the authority).

---

## 8. Architecture notes and decisions for the implementer

1. **Collision backends.** `CollisionWorld` (`world.rs`) is implemented by `SurfaceWorld` (triangle soup, reference) and by the Godot adapter (`stepkit-godot/src/world.rs`, uses Godot physics queries).
   Exact parity is only attainable with a triangle-based world that implements the decomp's algorithms (ST-02..06). Define the trait to expose the decomp's operations
   (`find_floor(x,y,z)`, `find_ceil(x,y,z)`, `find_wall_collisions(x,y,z,offset,radius)`, `find_water_level`), implement them exactly in `SurfaceWorld`, and make the Godot adapter implement them
   **best-effort** (documented as non-exact), or give it a mode that exports scene geometry as integer triangles into a `SurfaceWorld`.
2. **API stability.** `step::tick` and `MovementParams`/`CharacterState` are used by `stepkit-godot`; changing them is expected, but keep the Godot crate compiling in the same PR (it needs Godot 4.7 libs; CI that
   excludes `stepkit-godot` should at least `cargo check -p stepkit-godot` where possible).
3. **Camera yaw convention** (FD-02). Keep the public meaning; convert at the boundary; document.
4. **Interactions with held objects, cutscenes, sound, particles** are stubs. Keep the structure (e.g. `mario_drop_held_object` as a no-op method) so porting later is mechanical.
5. **Do not "fix" decomp bugs.** Glitches are behaviour (§5 rule 10).
6. **Prefer faithful ports over refactors.** Name Rust functions after the decomp functions (`update_walking_speed`, `perform_air_step`…), keep one-to-one structure, and add a comment `// decomp: src/game/mario_step.c:388` at the top of each so later reviews can diff them.
7. **Commits.** One commit per ported function group with its scenarios; run `oracle diff --all` before each commit and record the status in `docs/parity-log.md`.

---

## Appendix A — Decomp function index (file:line)

`src/game/mario.c`: `is_anim_at_end 46`, `is_anim_past_end 55`, `set_mario_animation 64`, `set_mario_anim_with_accel 97`, `is_anim_past_frame 151`, `mario_set_forward_vel 374`,
`mario_get_floor_class 387`, `resolve_and_return_wall_collisions 521`, `vec3f_find_ceil 547`, `mario_facing_downhill 556`, `mario_floor_is_slippery 573`, `mario_floor_is_slope 606`,
`mario_floor_is_steep 638`, `find_floor_slope 689`, `set_steep_jump_action 741`, `set_mario_y_vel_based_on_fspeed 763`, `set_mario_action_airborne 776`, `set_mario_action_moving 896`,
`set_mario_action_submerged 941`, `set_mario_action 979`, `set_jump_from_landing 1018`, `set_jumping_action 1074`, `drop_and_set_mario_action 1098`, `hurt_and_set_mario_action 1107`,
`check_common_action_exits 1117`, `set_water_plunge_action 1174`, `squish_mario_model 1205`, `update_mario_button_inputs 1253`, `update_mario_joystick_inputs 1293`,
`update_mario_geometry_inputs 1314`, `update_mario_inputs 1373`, `update_mario_health 1454`, `sink_mario_in_quicksand 1545`, `execute_mario_action 1699`, `init_mario 1788`.

`src/game/mario_step.c`: `mario_bonk_reflection 89`, `mario_update_quicksand 107`, `mario_push_off_steep_floor 158`, `mario_update_moving_sand 172`, `mario_update_windy_ground 190`,
`stop_and_set_height_to_floor 223`, `stationary_ground_step 236`, `perform_ground_quarter_step 258`, `perform_ground_step 322`, `check_ledge_grab 348`, `perform_air_quarter_step 388`,
`apply_twirl_gravity 503`, `should_strengthen_gravity_for_jump_ascent 519`, `apply_gravity 535`, `apply_vertical_wind 583`, `perform_air_step 610`, `set_vel_from_pitch_and_yaw 659`, `set_vel_from_yaw 665`.

`src/game/mario_actions_moving.c`: sliding/landing tables `20-60`, `check_ledge_climb_down 100`, `slide_bonk 137`, `set_triple_jump_action 147`, `update_sliding_angle 159`, `update_sliding 219`,
`apply_slope_accel 283`, `apply_landing_accel 334`, `update_shell_speed 350`, `apply_slope_decel 393`, `update_decelerating_speed 420`, `update_walking_speed 434`, `should_begin_sliding 467`,
`analog_stick_held_back 480`, `check_ground_dive_or_punch 485`, `begin_braking_action 501`, `push_or_sidle_wall 670`, `act_walking 777`, `act_move_punching 840`, `act_turning_around 962`,
`act_finish_turning_around 1013`, `act_braking 1037`, `act_decelerating 1072`, `act_crawling 1247`, `common_slide_action 1359`, `act_butt_slide 1432`, `act_crouch_slide 1452`,
`act_slide_kick_slide 1489`, `stomach_slide_action 1520`, `act_dive_slide 1557`, `common_ground_knockback_action 1588`, `act_ground_bonk 1691`, `common_landing_action 1722`,
`act_jump_land 1792`, `act_long_jump_land 1847`, `act_backflip_land 1897`, `quicksand_jump_land_action 1920`, `check_common_moving_cancels 1953`, `mario_execute_moving_action 1975`.

`src/game/mario_actions_airborne.c`: `check_fall_damage 61`, `check_kick_or_dive_in_air 108`, `should_get_stuck_in_ground 115`, `check_horizontal_wind 149`, `update_air_with_turn 186`, `update_air_without_turn 216`,
`common_air_action_step 371`, `act_jump 446`, `act_double_jump 461`, `act_triple_jump 480`, `act_backflip 509`, `act_freefall 525`, `act_side_flip 595`, `act_wall_kick_air 618`, `act_long_jump 632`,
`act_dive 725`, `act_steep_jump 888`, `act_ground_pound 918`, `check_wall_kick 1147`, `act_backward_air_kb 1156`, `act_soft_bonk 1247`, `act_air_hit_wall 1309`, `act_forward_rollout 1353`,
`act_butt_slide_air 1435`, `act_slide_kick 1580`, `act_jump_kick 1631`, `check_common_airborne_cancels 2046`, `mario_execute_airborne_action 2063`; `common_air_knockback_step 1097`.

`src/game/mario_actions_stationary.c`: `check_common_idle_cancels 20`, `act_idle 105`, `act_in_quicksand 509`, `act_crouching 528`, `stopping_step 612`, `act_braking_stop 620`, `act_start_crouching 693`,
`act_start_crawling 743`, `landing_step 830`, `check_common_landing_cancels 839`, `act_jump_land_stop 867`, `act_ground_pound_land 1037`, `check_common_stationary_cancels 1083`.

`src/game/mario_actions_automatic.c`: `act_ledge_grab 543`, `act_ledge_climb_slow 600`, `act_ledge_climb_down 626`, `act_ledge_climb_fast 639`, `update_ledge_climb 531`, `act_start_hanging 385`.

`src/engine/math_util.c`: `approach_s32 657`, `approach_f32 679`, `atan2_lookup 698`, `atan2s 713`; `src/engine/surface_collision.c`: `find_wall_collisions_from_list 19`, `f32_find_wall_collision 159`,
`find_floor_from_list 401` (78-unit tolerance at 459), `find_floor ~510`; `src/engine/surface_load.c`: `read_surface_data 296`, `load_area_terrain 585`; `src/game/game_init.c`: `adjust_analog_stick 424`.

## Appendix B — Constants comparison (plugin vs decomp)

| Quantity | Plugin | Decomp | Status |
|---|---|---|---|
| Gravity / terminal | 4 / −75 | 4 / −75 | ok |
| Long jump / slide kick gravity | 2 / **4** | 2 / 2 | slide kick wrong |
| Walk accel | 1.1 − v/43 | same | ok |
| Walk decel (flat, above target) | 1.0 | 1.0 (only if `normal.y >= 0.95`) | ok |
| Walk cap / target cap | 48 / 32 | 48 / 32 (24 on `SURFACE_SLOW`) | slow floor missing |
| Walk turn rate | 0x800 | 0x800 via `approach_s32` | ok |
| Walk entry clamp | `min(mag, 8)` (also applies to negative speed) | applies only if `0 <= v < min(mag,8)` and floor class ≠ very slippery | minor |
| Turn-around stick threshold | 0x4000 | 0x471C | wrong |
| Turn-around min speed | 16 | 16 | ok |
| Brake entry | speed ≥ 16, `normal.y ≥ 0.1736` | speed ≥ 16, `normal.y ≥ 0.17364818` | approx |
| Brake decel | 2.0 (flat) | `apply_slope_decel(2.0)` = 4.0 default | wrong |
| Decelerating | 1.0 | 1.0 | ok |
| Landing friction (stick) | −0.98 | ×0.98 | wrong |
| Landing decel (no stick, ≥16) | −2.0 | `apply_slope_decel(2.0)` = −4.0 | wrong |
| Knockback friction | −0.9 | ×0.9 | wrong |
| Quicksand jump land | −0.95 | ×0.95 | wrong |
| Crawl speed | min(mag,10), ±2/frame | `0.1 × mag` through walking model (~3.2) | wrong |
| Slide accel | 10/8/7/5 | 10/8/7/5 | ok |
| Slide loss base | 0.98/0.96/0.92/0.92 | same | ok |
| Slide speed cap | 100 (latch) | 100 (scaled after vel set) | approx |
| Slope accel | 5.3/2.7/1.7/0 | 5.3/2.7/1.7/0 | ok but gating wrong |
| Air drag threshold | 32 for all | 32; 48 for long jump | wrong |
| Air forward drag | 0.35 | 0.35 | ok |
| Air steering | persistent side velocity + facing turn 512·sin | memoryless side speed 10·sin, no facing change (butt-slide-air turns) | wrong |
| Air landing | within 78 above floor | `nextPos.y <= floorHeight` | wrong (C) |
| Wall hit angle (air) | `> 0x6000` | `|dYaw| > 0x6000` | ok |
| Ground wall window | 0x2AAA..0x5555 | same | ok |
| Walk-off | floor > 24 below | floor > 100 below | wrong |
| Step-up / floor tolerance | 1 (query), 100 check | floor search tolerance +78 | wrong |
| Air-hit-wall windows | 2 frames | `++actionTimer <= 2` | ok-ish |
| Knockback speeds | −15 / +16 | −16 / +16 | backward wrong |
| Knockback vertical | 40 (invented) | none set here | wrong |
| Jump entries | 42+v/4, 52+v/4, 69, −16/62, 62/8 | same | ok |
| Dive | +15 cap 48, vy 20 from ground | same | ok |
| Long jump | vy 30, ×1.5 cap 48 | same | ok |
| Slide kick | vy 12, fwd ≥ 32 | same | ok |
| Squish intended mag | /4 of intended | `mag/8` (= /4 of normal) | ok |
| Squish/quicksand jump halving | `quicksand > 1` | `>= 1` for double/twirl downgrade, `> 1` for halving | minor |
| Fall damage | 16 / 8 health | `hurtCounter` 24/16, 12/8 (cap-dependent) | wrong |
| Steep floor push-off | → freefall, no push | ±16 speed + facing = floor angle | wrong |
| Steep landing threshold | 0.2924 | 0.2923717 | approx |

## Appendix C — Scenarios to add (in addition to the 52 existing)

Each should be a short `.ron` scenario with integer geometry; names are suggestions.

- Input pipeline: `stick_edges` (±7/±8/±64/±127 on each axis; held/not-held; magnitudes), `stick_cap_diag` (full-diagonal 80,80 walking and sliding), `squish_buttons` (B/Z ignored while squished).
- Pipeline: `same_frame_jump` (A on a frame from idle, walking, braking), `idle_to_walk_face_snap` (stick while facing opposite), `turn_around_full` (stop → 8 → finish), `brake_wall_bonk`.
- Steps: `step_up_{20,40,70,90,120}`, `step_down_{20,60,90,110}` (walk-off height boundary 100), `ceiling_low_{150,170}`, `pedro_spot`, `corner_two_walls`, `lower_wall_only`, `wall_slide_glance_{30,45,75}deg`.
- Air: `tap_jump`, `held_jump_landing_frame`, `long_jump_48_cruise` (drag threshold), `slide_kick_arc`, `air_steer_{left,right,back}_full_tilt`, `wall_bonk_{slow,fast}`, `wall_kick_window`, `steep_jump_slopes`, `dive_slide_stop`, `butt_slide_air_bounce`, `fall_damage_squish` (with real `hurtCounter`).
- Ground: `double_jump_window_{0..9}` (A pressed N frames after landing), `triple_jump_speed_20`, `land_stick_vs_no_stick`, `slide_{default,slippery,very_slippery,not_slippery}_{downhill,uphill,sideways}`, `slide_wall_{slippery,default}`, `crawl_speed`, `crouch_slide_long_jump_30f_window`.
- Surfaces: `very_slippery_walk_entry`, `slow_surface_cap`, `quicksand_{shallow,deep,moving}`, `slide_terrain_area` (area `terrainType = SLIDE`).
- Fuzz seeds directory `traces/fuzz/`.

## Appendix D — Symbols to stub for the native oracle

The 80 symbols that remained unresolved when linking the movement sources listed in §7.1 (from `nm -u` vs defined symbols; groups, not exhaustive of types):

- Sound/camera/particles: `play_sound`, `stop_sound`, `set_sound_moving_speed`, `fadeout_cap_music`, `play_shell_music`, `stop_shell_music`, `stop_cap_music`, `play_infinite_stairs_music`, `raise_background_noise`, `lower_background_noise`, `set_camera_mode`, `set_camera_shake_from_hit`, `spawn_wind_particles`, `gAudioRandom`, `gGlobalSoundSource`, `gCameraMovementFlags`, `gPlayerCameraState`.
- Level/save/warp: `level_trigger_warp`, `load_level_init_text`, `save_file_get_cap_pos`, `save_file_get_flags`, `save_file_get_total_star_count`, `gCurrLevelNum`, `gCurrAreaIndex`, `gCurrSaveFileNum`, `gMarioSpawnInfo`, `gPlayerSpawnInfos`, `gDebugLevelSelect`, `gHudDisplay`.
- Objects/interactions: `spawn_object`, `mario_blow_off_cap`, `mario_check_object_grab`, `mario_drop_held_object`, `mario_grab_used_object`, `mario_get_collided_object`, `mario_process_interactions`, `mario_handle_special_floors`, `mario_stop_riding_and_holding`, `mario_stop_riding_object`, `mario_throw_held_object`, `mario_update_punch_sequence` (needs a real implementation for punches), `mario_execute_cutscene_action`, `mario_execute_object_action`, `bhvGiantPole`, `bhvJumpingBox`, `bhvKoopaShellUnderwater`, `bhvNormalCap`, `bhvTree`, `gSpecialTripleJump`, `gCurrentObject`, `gMarioObject`, `gMarioState`, `gBodyStates`.
- Animation/graph: `load_patchable_table` (needs real behaviour, §7.1), `retrieve_animation_index`, `geo_update_animation_frame` (compile `graph_node.c` instead of stubbing), `gMarioAnimsBuf`, `gAreaUpdateCounter`, `segmented_to_virtual`, `virtual_to_segmented`, `guMtxF2L`, `gVec3fZero`, `lower_background_noise`.
- Collision globals (defined in `surface_load.c` if compiled): `gStaticSurfacePartition`, `gDynamicSurfacePartition`, `gSurfaceNodesAllocated`, `gSurfacesAllocated`, `gNumStaticSurfaces`, `gNumStaticSurfaceNodes`, `gEnvironmentRegions`, `gCheckingSurfaceCollisionsForCamera`, `gFindFloorIncludeSurfaceIntangible`, `gNumCalls`, `gNumFindFloorMisses`, `main_pool_alloc` (provide a `calloc` wrapper), `spawn_macro_objects`, `spawn_special_objects`.
- Debug/print/misc: `print_debug_top_down_mapinfo`, `print_text_fmt_int`, `set_text_array_x_y`, `gShowDebugText`, `gGlobalTimer`, `gControllers`, `gCurrentArea`, `gTimeStopState`, `__stack_chk_fail` (compile with `-fno-stack-protector`).

Regenerate this list yourself with `nm -u *.o | grep ' U ' | sort -u` versus `nm *.o | grep -E ' [TDBRC] '`; it is the authoritative list for your build.

## Appendix E — Review provenance (what was actually checked when this plan was written)

**Read against the decomp:** `mario_step.c` (all), `mario.c` (input handling, geometry inputs, floor-class helpers, `set_mario_action*`, `set_jump_from_landing`, `set_steep_jump_action`, `execute_mario_action`, `set_mario_y_vel_based_on_fspeed`),
`mario_actions_moving.c` (`1-1330`, `1330-2035` skimmed: slides, knockback, landings, quicksand land), `mario_actions_airborne.c` (`1-830`, knockback/steep/air-hit-wall/butt-slide-air excerpts), `mario_actions_stationary.c` (`idle`, `crouching`, `braking_stop`, stop helpers),
`surface_collision.c` (wall/floor search), `game_init.c` (`adjust_analog_stick`), `math_util.h/c` (trig macros, `atan2s` run natively).
**Read in the plugin:** `step.rs`, `input.rs`, `trig.rs`, `angles.rs`, `params.rs`, `world.rs`, `actions.rs` (head), all of `actions/ground.rs` and `actions/air.rs`, key parts of `surface.rs`, trace/golden files `jump_neutral`, `jump_hold_a`, `air_steer_left`.
**Not reviewed:** `water.rs` and `mario_actions_submerged.c`; punch sequence (`mario_actions_object.c`); `act_ground_pound` internals; ledge/climb/hang action bodies in `mario_actions_automatic.c`; `act_steep_jump` was read (AR-07) but not `act_jump_kick`, `act_*_rollout`, `act_forward/backward_rollout`; `stationary.c` actions other than those named; `mario_update_quicksand` interplay with `sink_mario_in_quicksand`; `stepkit-godot`; `stepkit-anim`; `surface.rs` beyond the wall/floor/ceiling functions quoted; spatial partitioning details.
Any statement in this plan about those areas is an inference to be checked against the decomp.
