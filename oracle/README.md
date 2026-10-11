# Phase 1 oracle — native build of the decomp's movement code

The oracle runs the **real** Super Mario 64 (decomp, `n64decomp/sm64`)
movement code one frame at a time and dumps `MarioState` fields as CSV, so
the Rust plugin's `tick()` can be diffed against ground truth frame-by-frame.

## Layout

| File | Owner | Purpose |
|---|---|---|
| `DECOMP_COMMIT` | — | Pinned decomp commit (single line). |
| `build.sh` | — | Clones/pins the decomp, compiles the 11 sources with the plan §7.1 flags, generates the anim table, compiles `stubs.c`+`harness.c`, links `build/oracle`, builds+runs `selftest_math`. |
| `harness.c` | — | Scenario-file driver: loads collision via the real `load_area_terrain`, runs `init_mario` + one `execute_mario_action` per frame, writes CSV. |
| `stubs.c` | sibling | The ~80 missing symbols (globals, sound/camera/object/save-file no-ops, `main_pool_alloc`, `load_patchable_table`, `gOracleLastAnimId`). |
| `gen_anim_table.py` | sibling | `python3 gen_anim_table.py <decomp_dir> <out>` — scans `assets/anims/anim_*.inc.c` and emits the animation-header table (`build/anim_table.inc`). |
| `selftest_math.c` | sibling | Links `math_util.o` only; checks `atan2s` against hand-verified values. |
| `sm64/` | git-ignored | Decomp checkout at the pinned commit. |
| `build/` | git-ignored | Objects, `anim_table.inc`, `oracle`, `selftest_math`. |

Neither `sm64/` nor `build/` is ever committed (see root `.gitignore`).

## Build

```sh
./oracle/build.sh            # from the repo root (works from any cwd)
DECOMP_DIR=/path/to/sm64 ./oracle/build.sh   # reuse an existing checkout
```

`build.sh` (`set -euo pipefail`) does, in order:

1. `ORACLE_DIR` from the script path; `DECOMP_DIR` defaults to `oracle/sm64`;
   `PIN` from `DECOMP_COMMIT`.
2. If `$DECOMP_DIR/.git` is missing: `git init`, add the `n64decomp/sm64`
   remote, `git fetch --depth 1 origin $PIN && git checkout FETCH_HEAD`,
   falling back to a full `git clone` + `git checkout $PIN`. If the checkout
   exists but HEAD ≠ PIN: fetch + checkout the pin.
3. Compiles the 11 §7.1 sources with cwd=`$DECOMP_DIR` and **exactly**
   `gcc -c -w -O1 -std=gnu99 -DVERSION_US=1 -DTARGET_N64=0 -D_LANGUAGE_C=1 -DNON_MATCHING=1 -DAVOID_UB=1 -ffp-contract=off -fno-stack-protector -I include -I src -I . -I include/libc`
   into `build/obj/`. (`-DTARGET_N64=0` is ABI-relevant: it selects the
   `IS_64_BIT=0` object-field layout and u32 `size_t`; `harness.c`/`stubs.c`
   use the same defines.)
4. Runs `gen_anim_table.py` → `build/anim_table.inc`.
5. Compiles `stubs.c` and `harness.c` (same defines, `-I$DECOMP_DIR/include -I$DECOMP_DIR/src -I$DECOMP_DIR`) and links everything into `build/oracle` (`-lm`).
6. Builds `build/selftest_math` from `selftest_math.c` + `math_util.o`,
   runs it, and fails the build unless it prints `OK`.
7. Prints a build summary. Re-running is safe (idempotent).

Pinned commit: `9921382a68bb0c865e5e45eb594d9c64db59b1af`.

## Running

```sh
oracle/build/oracle --in scenario.oin --out trace.csv
```

## Input format (`*.oin`, line-based)

`#` starts a comment; blank lines are ignored.

```
version 1
terrain <int>              # TERRAIN_* value: 0=GRASS … 6=SLIDE
verts <V>
<x> <y> <z>                # V lines, integers (must fit s16)
tris <T>
<type> <i0> <i1> <i2>      # T lines; type=SURFACE_* int; vertex indices.
                           # Winding decides the normal: the decomp computes
                           # n = (v1-v0) x (v2-v0)-ish, so order verts
                           # counter-clockwise seen from the desired normal.
water none                 # or: water <level_int> (fits s16)
start <px> <py> <pz> <yaw_deg> <action_hex>   # e.g. start 0 0 0 0.0 0x0C400201
poke forward_vel <f>       # zero or more; applied after init
frames <N>
<sx> <sy> <buttons> <camyaw_deg>   # N lines
```

- `terrain` is the area's `terrainType` (`TERRAIN_GRASS`/`TERRAIN_SLIDE`/…).
- The terrain array is built with the decomp's own `COL_INIT()`,
  `COL_VERTEX_INIT(n)`, `COL_VERTEX(x,y,z)`, `COL_TRI_INIT(type,n)`,
  `COL_TRI(a,b,c)`, `COL_TRI_STOP()`, `COL_END()` macros (consecutive
  same-type tris are grouped, as in the decomp's `collision.inc.c` files) and
  loaded with the real `load_area_terrain(0, data, NULL, NULL)`.
- `sx`/`sy` are raw stick ints (s16). `buttons` is the decomp bitmask
  (accepts decimal or `0x…`): `A=0x8000 B=0x4000 Z=0x2000 START=0x1000 R=0x0010`.
- `camyaw_deg` is in **DECOMP camera convention**: the direction **from Mario
  toward the camera** (the opposite of where the camera looks). The CLI that
  generates these files adds 180° to the plugin's look-direction yaw.
- `action_hex` is a full `ACT_*` value, e.g. `0x0C400201` (`ACT_IDLE`).
- `start` coordinates should be integers (they are rounded to s16 for the
  spawn info `init_mario()` reads).
- Triangle types with a force parameter (`SURFACE_HORIZONTAL_WIND`,
  `SURFACE_VERTICAL_WIND`, …) are **not** supported by this format — the
  loader reads an extra data word for those, which the input cannot express.

## CSV schema

First line is the header; one row per frame (0-based `frame`).

```
frame,stick_x,stick_y,buttons,cam_yaw,pos_x,pos_y,pos_z,vel_x,vel_y,vel_z,
forward_vel,slide_vel_x,slide_vel_z,slide_yaw,face_pitch,face_yaw,face_roll,
angle_vel_x,angle_vel_y,angle_vel_z,intended_yaw,intended_mag,
action,prev_action,action_state,action_timer,action_arg,input,
floor_height,floor_angle,floor_normal_y,floor_type,ceil_height,
wall_normal_yaw,water_level,wall_kick_timer,double_jump_timer,squish_timer,
quicksand_depth,peak_height,health,hurt_counter,invinc_timer,flags,
anim_id,anim_frame,anim_accel,anim_frame_accel_assist
```

- `stick_x`/`stick_y` are the **post-`adjust_analog_stick`** floats (deadzone
  and 64.0 magnitude cap applied), `%.9g`.
- `cam_yaw` echoes the input degrees (`%g`).
- `action`, `prev_action`, `action_arg`, `input`, `flags`: `0x%08X`.
- `floor_type`: `-1` when `m->floor == NULL`; `floor_normal_y` is `0.0` then.
- `wall_normal_yaw`: `atan2s(wall->normal.z, wall->normal.x)` as `%d`, or the
  literal `none` when `m->wall == NULL`. (Note `atan2s(y, x)` measures the
  angle **from the +y axis toward +x** — argument order is the opposite of
  the usual `atan2`.)
- `anim_id`: the id most recently passed to `load_patchable_table`
  (`gOracleLastAnimId`, `-1` until the first). `anim_frame`/`anim_accel`/
  `anim_frame_accel_assist` come from the emulated `geo_set_animation_globals`
  (see below).
- All other f32 fields: `%.9g`; s16/s32/u8 fields: `%d`.

## Per-frame sequence (mirrors the real game)

1. `gControllers[0]`: `rawStickX/Y`, `buttonDown`, `buttonPressed = down & ~prevDown`; then the copied `adjust_analog_stick()`. `gMarioState->area->camera->yaw` ← `camyaw_deg`. `gGlobalTimer++`.
2. `execute_mario_action(gMarioObject)`.
3. Animation advance, emulating `geo_set_animation_globals` (`rendering_graph_node.c:608`): `animFrame = geo_update_animation_frame(ai, &ai->animFrameAccelAssist)` (the real function from `graph_node.o`); `animTimer = gAreaUpdateCounter; gAreaUpdateCounter++`.
4. Write one CSV row.

`adjust_analog_stick` is copied **verbatim** from
`src/game/game_init.c` into `harness.c`; `game_init.c` itself is not compiled
(it drags in the OS layer).

Initial state: the harness calls the real `init_mario()` (`mario.c:1788`)
after pointing `gMarioSpawnInfo` at the `start` position/yaw. `init_mario()`
does not set `controller`/`health`/`animList`/`marioBodyState`/
`statusForCamera`/`spawnInfo` (the real game sets those in
`init_mario_from_save_file()`), so the harness sets them explicitly
afterwards (`health = 0x880`, `controller = &gControllers[0]`,
`animList = &gMarioAnimsBuf` with `bufTarget` → a static `struct Animation`),
then overrides `pos`/`faceAngle[1]`/`action`/`prevAction` from `start`,
syncs the Mario object's `gfx.pos`/`oPos`, and applies `poke`s.

## Known deviations from plan §7.1

- **Water is a uniform region fill (Phase-1 simplification).** Instead of
  embedding `TERRAIN_LOAD_ENVIRONMENT` data in the collision stream, the
  harness fills one region (`[1, 0, loX, loZ, hiX, hiZ, level]`, `val < 50`
  ⇒ water per `find_water_level`) covering the geometry's AABB ± 1 unit
  (the lookup uses strict inequalities), or zero regions for `water none`.
  Note `load_area_terrain()` resets `gEnvironmentRegions` to NULL, so the
  regions are installed after that call.
- **Anim table generated at build time.** `gen_anim_table.py` scans
  `assets/anims/anim_*.inc.c` (no ROM needed) and `stubs.c` includes the
  result; `load_patchable_table` copies the 5-s16 header into
  `animList->bufTarget` and returns 0, skipping the 64-bit-unsafe pointer
  patching in `set_mario_animation`.
- **Host-libc compatibility shims in `harness.c`.** With `-DTARGET_N64=0`,
  `PR/ultratypes.h` typedefs `size_t`/`ssize_t`/`uintptr_t`/`intptr_t`/
  `ptrdiff_t` to 32-bit types and `PR/os_libc.h` redeclares `bcopy`/`bzero`
  with them, clashing with the host headers the harness needs for file I/O.
  `harness.c` renames those six symbols (`decomp_size_t`, …) before
  including decomp headers; layouts stay ABI-identical to the objects.
- **`ai->curAnim` NULL guard** around `geo_update_animation_frame`: with
  working stubs `curAnim` is always set by `set_mario_animation` first; the
  guard only avoids a segfault if a scenario's start action never sets an
  animation.
- `anim_accel` is printed from the struct's real type (`s32`), not s16.
- `atan2s` is used with its true signature `s16 atan2s(f32 y, f32 x)`
  (via `engine/math_util.h`); an `s16`-parameter declaration would be an
  x86-64 ABI mismatch.

## Self-tests

- `build.sh` builds and runs `selftest_math` (must print `OK`); it checks
  `atan2s` against the hand-verified values in plan §2.3
  (`atan2s(1,0)=0`, `atan2s(0,1)=16384`, `atan2s(-64,0)=-32768`,
  `atan2s(0,64)=16384`, `atan2s(0,0)=0`, `atan2s(30,40)=9672`).
- Hand-checkable scenario (plan §7.1/§4 ST-01): flat 4000×4000 floor at y=0,
  start idle at origin, A **held** (`buttonDown=0x8000` on every frame) —
  the first `ACT_JUMP_LAND` (`0x04000470`) row is frame **21** (0-based;
  this is the plan's "air frame 22" in 1-based counting). If A is only
  tapped on frame 0, the decomp's real variable jump height
  (`should_strengthen_gravity_for_jump_ascent`: `vel[1] /= 4` when A is
  released while ascending) produces a short hop landing at frame 11 —
  that is faithful behavior, not a bug. See the example below.

Example jump scenario (`jump.oin`):

```
version 1
terrain 0
verts 4
-2000 0 -2000
2000 0 -2000
2000 0 2000
-2000 0 2000
tris 2
0 0 2 1
0 0 3 2
water none
start 0 0 0 0.0 0x0C400201
frames 51
0 0 32768 0.0
... (50 more identical lines: A held the whole time)
```

(`32768` = `0x8000` = A, held: `buttonDown` set on every frame, so
`buttonPressed` fires only on frame 0. The winding `0 2 1` / `0 3 2` gives
a +Y normal. First `0x04000470` row: frame 21.)
