//! L1: public anchors. Numbers transcribed from public wiki pages
//! (`traces/public-anchors/`, each citing page + revision) checked against
//! the core. Runs in public CI; no ROM needed.

use glam::Vec3;
use std::collections::HashMap;
use std::path::PathBuf;
use stepkit_core::angles::Angle;
use stepkit_core::input::RawInput;
use stepkit_core::params::MovementParams;
use stepkit_core::state::{ActionId, CharacterState};
use stepkit_core::step::{tick, Timeline, QUARTER_STEPS};
use stepkit_core::surface::SurfaceWorld;
use stepkit_core::world::SurfaceKind;
use stepkit_trace::runner::run_scenario;
use stepkit_trace::scenario::*;

fn anchors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../traces/public-anchors")
}

/// Parse an anchor CSV into parameter -> value (skips `#` comments).
fn load_anchors(name: &str) -> HashMap<String, String> {
    let text = std::fs::read_to_string(anchors_dir().join(name)).expect("anchor csv");
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(3, ',');
        let key = parts.next().unwrap().trim();
        let value = parts.next().unwrap().trim();
        map.insert(key.to_string(), value.to_string());
    }
    map
}

fn num(map: &HashMap<String, String>, key: &str) -> f32 {
    map[key].parse().expect("numeric anchor")
}

fn flat_world() -> SurfaceWorld {
    let mut w = SurfaceWorld::new();
    w.add_box(
        Vec3::new(-2000.0, -100.0, -2000.0),
        Vec3::new(2000.0, 0.0, 2000.0),
        stepkit_core::world::surface_type::DEFAULT,
        0,
    );
    w
}

fn walk_scenario(frames: u32, stick: (i8, i8)) -> Scenario {
    Scenario {
        id: "anchor_walk".into(),
        tags: vec![],
        geometry: vec![Geometry::Box(BoxGeom {
            min: (-2000.0, -100.0, -2000.0),
            max: (2000.0, 0.0, 2000.0),
            kind: 0,
        })],
        water: vec![],
        start: StartState {
            pos: (0.0, 0.0, 0.0),
            yaw_deg: 0.0,
            action: "Idle".into(),
        },
        camera: Camera::Fixed { yaw_deg: 0.0 },
        inputs: vec![InputSeg::Hold {
            frames,
            stick,
            buttons: 0,
        }],
        compare: CompareProfile::GroundDefault,
        terrain: 0,
        asserts: vec![],
    }
}

#[test]
fn anchor_quarter_steps() {
    let q = load_anchors("quarter_steps.csv");
    assert_eq!(QUARTER_STEPS as f32, num(&q, "quarter_steps_per_frame"));
}

#[test]
fn anchor_walk_acceleration_curve() {
    let w = load_anchors("walking_behavior.csv");
    let params = MovementParams::default();
    // Defaults match the anchors.
    assert_eq!(
        params.walk_target_speed_cap,
        num(&w, "walk_target_speed_default")
    );
    assert_eq!(params.walk_speedup_base, num(&w, "walk_speedup_base"));
    assert_eq!(
        params.walk_speedup_falloff_divisor,
        num(&w, "walk_speedup_falloff_divisor")
    );
    assert_eq!(params.walk_hard_cap, num(&w, "walk_hard_cap"));
    assert_eq!(
        params.walk_enter_speed_clamp,
        num(&w, "walk_enter_speed_clamp")
    );

    // Behavior: 45 frames at full stick from idle.
    let trace = run_scenario(&walk_scenario(45, (0, 80)), &params, "l1");
    let speeds: Vec<f32> = trace.frames.iter().map(|f| f.fwd_speed).collect();
    // Entry clamp: the walking entry sets speed to min(mag, 8); the
    // action loop then runs the walking body in the same tick, so the
    // first walking frame already includes one acceleration step.
    let expect1 = 8.0 + (1.1 - 8.0 / 43.0);
    assert!(
        (speeds[1] - expect1).abs() < 1e-4,
        "entry+accel: {}",
        speeds[1]
    );
    // Second frame: another acceleration step.
    let expect = expect1 + (1.1 - expect1 / 43.0);
    assert!(
        (speeds[2] - expect).abs() < 1e-3,
        "accel curve: {} vs {expect}",
        speeds[2]
    );
    // Approaches the 32 target: the settled limit cycle oscillates around
    // it (accel overshoots, flat-ground decel pulls back). Check the band.
    let tail: &[f32] = &speeds[35..];
    let (tmin, tmax) = tail.iter().fold((f32::MAX, f32::MIN), |(a, b), &s| {
        (a.min(s), b.max(s))
    });
    assert!(
        tmin >= 30.0 && tmax <= 33.0,
        "target band: [{tmin}, {tmax}]"
    );
    assert!(speeds.iter().all(|&s| s <= 48.0 + 1e-4), "hard cap");
    // Documented limit cycle around the target: accel overshoots slightly,
    // then the flat-ground decel (-1.0/frame) pulls back under.
    let mut saw_decel = false;
    for pair in speeds[1..].windows(2) {
        if pair[0] > 32.0 {
            assert!(
                (pair[1] - (pair[0] - 1.0)).abs() < 1e-3,
                "flat decel branch"
            );
            saw_decel = true;
        }
    }
    assert!(
        saw_decel,
        "expected the above-target decel branch to trigger"
    );
}

#[test]
fn anchor_walk_turn_rate() {
    // Facing approaches the intended yaw at <= 0x800/frame
    // (update_walking_speed, moving.c:434). NOTE: the idle->walking entry
    // frame SNAPS the facing (`m->faceAngle[1] = (s16) m->intendedYaw`,
    // stationary.c:47), so this test starts already WALKING to measure
    // the per-frame rate limit in isolation.
    let w = load_anchors("walking_behavior.csv");
    let rate_units = u32::from_str_radix(w["walk_turn_rate"].trim_start_matches("0x"), 16).unwrap();
    assert_eq!(MovementParams::default().walk_turn_rate as u32, rate_units);
    let world = flat_world();
    let registry = stepkit_core::actions::ActionRegistry::sm64_style();
    let params = MovementParams::default();
    let mut state = CharacterState {
        action: ActionId::WALKING,
        face_yaw: Angle::ZERO,
        ..CharacterState::default()
    };
    let mut timeline = Timeline::default();
    let mut prev_buttons = 0u16;
    let mut max_step = 0i32;
    let mut prev_yaw = state.face_yaw;
    for _ in 0..30 {
        // Stick hard right: intended yaw 90 deg off facing.
        let input = RawInput {
            stick_x: 80,
            stick_y: 0,
            buttons: 0,
            cam_yaw: Angle::ZERO,
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
        max_step = max_step.max((s.face_yaw.0 as i32 - prev_yaw.0 as i32).abs());
        prev_yaw = s.face_yaw;
        state = s;
        timeline = t;
        prev_buttons = input.buttons;
    }
    assert!(max_step <= 0x800, "turn rate: {max_step}");
    assert!(max_step > 0, "should have turned");
}

#[test]
fn anchor_jump_entry_numbers() {
    let a = load_anchors("airborne_numbers.csv");
    let params = MovementParams::default();
    assert_eq!(params.jump_vertical_base, num(&a, "jump_vertical_base"));
    assert_eq!(
        params.jump_vertical_forward_factor,
        num(&a, "jump_vertical_forward_factor")
    );
    assert_eq!(params.jump_forward_retain, num(&a, "jump_forward_retain"));
    assert_eq!(params.gravity, -num(&a, "gravity_default"));
    assert_eq!(
        params.terminal_velocity,
        num(&a, "terminal_velocity_default")
    );

    // Enter a jump at speed 32: vy = 42 + 32/4 = 50, speed * 0.8 = 25.6.
    let world = flat_world();
    let registry = stepkit_core::actions::ActionRegistry::sm64_style();
    let state = CharacterState {
        action: ActionId::WALKING,
        forward_speed: 32.0,
        ..CharacterState::default()
    };
    let input = RawInput {
        stick_x: 0,
        stick_y: 80,
        buttons: 1,
        cam_yaw: Angle::ZERO,
    };
    let (out, _, _) = tick(
        state,
        input,
        &world,
        &params,
        Timeline::default(),
        &registry,
        0,
    );
    assert_eq!(out.action, ActionId::JUMP);
    // Same-frame: the jump body runs in the entry tick (action loop), so
    // gravity applies once: 50 - 4 = 46 (matches the oracle's jump_neutral
    // frame 0: 42 - 4 = 38).
    assert!((out.vel.y - 46.0).abs() < 1e-3, "vy {}", out.vel.y);
    // Same-frame: the jump body runs in the entry tick, so the air control
    // (stick held) also applies once: 32*0.8 = 25.6, then +1.15 from air
    // acceleration.
    assert!(
        (out.forward_speed - 26.75).abs() < 1e-2,
        "speed {}",
        out.forward_speed
    );
}

#[test]
fn anchor_action_ids() {
    // Spot-check the wiki-verified IDs transcribed to action_ids.csv.
    let text = std::fs::read_to_string(anchors_dir().join("action_ids.csv")).expect("ids");
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split(',').collect();
        map.insert(parts[0].to_string(), parts[1].to_string());
    }
    let check = |name: &str, id: ActionId| {
        let hex = map[name].trim();
        let want = u32::from_str_radix(hex.trim_start_matches("0x"), 16).unwrap();
        assert_eq!(id.0, want, "{name}: {hex}");
    };
    check("Idle", ActionId::IDLE);
    check("Turning Around", ActionId::TURNING_AROUND);
    check("Single Jump", ActionId::JUMP);
    check("Double Jump", ActionId::DOUBLE_JUMP);
    check("Triple Jump", ActionId::TRIPLE_JUMP);
    check("Backflip", ActionId::BACKFLIP);
    check("Side Flip", ActionId::SIDE_FLIP);
    check("Long Jump", ActionId::LONG_JUMP);
    check("Dive", ActionId::DIVE);
    check("Ledge Grab", ActionId::LEDGE_GRAB);
}

// ---------------------------------------------------------------------------
// Phase 3: jump chain and air actions.
// ---------------------------------------------------------------------------

use stepkit_core::actions::air::{
    enter_backflip, enter_dive, enter_double_jump, enter_side_flip, enter_triple_jump,
};
use stepkit_core::actions::{ActionCx, ActionRegistry, ActionResult};
use stepkit_core::input::buttons;

/// Build an [`ActionCx`] for directly exercising enter_* transitions.
fn action_cx<'a>(
    mut state: CharacterState,
    world: &'a SurfaceWorld,
    params: &'a MovementParams,
) -> ActionCx<'a> {
    let input = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: 0,
        cam_yaw: Angle::ZERO,
    };
    let proc = stepkit_core::input::process_input(&input, 0, &mut state);
    ActionCx {
        state,
        input,
        proc,
        world,
        params,
        timeline: Timeline::default(),
        events: Vec::new(),
    }
}

/// Drive one tick of the current action in a flat world.
fn one_tick(
    state: CharacterState,
    input: RawInput,
    prev_buttons: u16,
) -> (CharacterState, Timeline) {
    let world = flat_world();
    let params = MovementParams::default();
    let registry = ActionRegistry::sm64_style();
    let (s, t, _) = stepkit_core::step::tick(
        state,
        input,
        &world,
        &params,
        Timeline::default(),
        &registry,
        prev_buttons,
    );
    (s, t)
}

fn air_state(action: ActionId, speed: f32) -> CharacterState {
    CharacterState {
        action,
        forward_speed: speed,
        pos: Vec3::new(0.0, 200.0, 0.0),
        ..CharacterState::default()
    }
}

#[test]
fn anchor_double_jump_entry() {
    // wiki:Double Jump@18964: vy = 52 + hspeed/4, forward *= 0.8.
    let world = flat_world();
    let params = MovementParams::default();
    let mut cx = action_cx(air_state(ActionId::IDLE, 32.0), &world, &params);
    let r = enter_double_jump(&mut cx);
    assert!(matches!(r, ActionResult::Goto { .. }));
    assert!(
        (cx.state.vel.y - 60.0).abs() < 1e-3,
        "vy {}",
        cx.state.vel.y
    );
    assert!((cx.state.forward_speed - 25.6).abs() < 1e-3);
}

#[test]
fn anchor_triple_jump_entry() {
    // wiki:Triple Jump@19300: vy = 69 fixed, forward *= 0.8, no height control.
    let world = flat_world();
    let params = MovementParams::default();
    let mut cx = action_cx(air_state(ActionId::IDLE, 32.0), &world, &params);
    enter_triple_jump(&mut cx);
    assert!(
        (cx.state.vel.y - 69.0).abs() < 1e-3,
        "vy {}",
        cx.state.vel.y
    );
    assert!((cx.state.forward_speed - 25.6).abs() < 1e-3);
}

#[test]
fn anchor_backflip_entry() {
    // wiki:Backflip@19307: forward = -16, vy = 62.
    let world = flat_world();
    let params = MovementParams::default();
    let mut cx = action_cx(air_state(ActionId::IDLE, 0.0), &world, &params);
    enter_backflip(&mut cx);
    assert!(
        (cx.state.vel.y - 62.0).abs() < 1e-3,
        "vy {}",
        cx.state.vel.y
    );
    assert!((cx.state.forward_speed + 16.0).abs() < 1e-3);
}

#[test]
fn anchor_side_flip_entry() {
    // wiki:Side Flip@20374: vy = 62, forward = 8, facing = intended yaw.
    let world = flat_world();
    let params = MovementParams::default();
    let mut cx = action_cx(air_state(ActionId::IDLE, 32.0), &world, &params);
    cx.input.stick_x = 80;
    cx.input.stick_y = 0;
    // Re-process the input the way tick() does after the raw input changes.
    cx.proc = stepkit_core::input::process_input(&cx.input, 0, &mut cx.state);
    enter_side_flip(&mut cx);
    assert!(
        (cx.state.vel.y - 62.0).abs() < 1e-3,
        "vy {}",
        cx.state.vel.y
    );
    assert!((cx.state.forward_speed - 8.0).abs() < 1e-3);
    // Intended yaw for stick (80,0) with cam 0: adjust -> (64,0);
    // atan2s(-0, 64) = 0x4000, plus the decomp cam yaw (0 + 0x8000).
    assert_eq!(cx.state.face_yaw.0, -0x4000);
}

#[test]
fn anchor_dive_entry_numbers() {
    // wiki:Dive@19303: +15 horizontal clamped to 48; +20 vy from the ground.
    let world = flat_world();
    let params = MovementParams::default();
    let mut cx = action_cx(air_state(ActionId::IDLE, 40.0), &world, &params);
    enter_dive(&mut cx, true);
    assert!((cx.state.forward_speed - 48.0).abs() < 1e-3, "clamp");
    assert!(
        (cx.state.vel.y - 20.0).abs() < 1e-3,
        "vy {}",
        cx.state.vel.y
    );

    let world = flat_world();
    let params = MovementParams::default();
    let mut cx = action_cx(air_state(ActionId::IDLE, 30.0), &world, &params);
    enter_dive(&mut cx, true);
    assert!((cx.state.forward_speed - 45.0).abs() < 1e-3);
}

#[test]
fn anchor_long_jump_gravity() {
    // wiki:Gravity@20294: long jump uses gravity -2, terminal -75.
    // After 20 frames from vy=0, vy should be -40: well above the -75
    // terminal a normal jump would already be approaching.
    let mut st = air_state(ActionId::LONG_JUMP, 30.0);
    st.pos.y = 2000.0; // stay airborne for the whole probe
    st.vel.y = 0.0;
    let input = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: 0,
        cam_yaw: Angle::ZERO,
    };
    for _ in 0..20 {
        let (ns, _) = one_tick(st, input, 0);
        st = ns;
    }
    assert!((st.vel.y + 40.0).abs() < 1e-3, "vy {}", st.vel.y);
}

#[test]
fn anchor_jump_chain() {
    // Land from a single jump, press A during landing -> double jump
    // (vy = 52 + 25.6/4 = 58.4 after the 0.8 retain... check the chain).
    let world = flat_world();
    let params = MovementParams::default();
    let registry = ActionRegistry::sm64_style();
    // Start: single jump at speed 32, high up, falling.
    let mut state = CharacterState {
        action: ActionId::JUMP,
        forward_speed: 25.6,
        pos: Vec3::new(0.0, 300.0, 0.0),
        vel: Vec3::new(0.0, -10.0, 25.6),
        ..CharacterState::default()
    };
    let input = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: 0,
        cam_yaw: Angle::ZERO,
    };
    let mut landed = false;
    for _ in 0..200 {
        let (s, t, _) = stepkit_core::step::tick(
            state,
            input,
            &world,
            &params,
            Timeline::default(),
            &registry,
            0,
        );
        let _ = t;
        state = s;
        if state.action == ActionId::JUMP_LAND {
            landed = true;
            break;
        }
    }
    assert!(landed, "should have landed");
    assert_eq!(state.land_from, ActionId::JUMP);
    // Press A during landing -> double jump.
    let press_a = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: buttons::A,
        cam_yaw: Angle::ZERO,
    };
    let (s, _, _) = stepkit_core::step::tick(
        state,
        press_a,
        &world,
        &params,
        Timeline::default(),
        &registry,
        0,
    );
    assert_eq!(s.action, ActionId::DOUBLE_JUMP, "chain to double jump");
    // vy = 52 + fwd/4 where fwd was scrubbed during landing; just check > 52.
    assert!(s.vel.y > 52.0, "double jump vy {}", s.vel.y);
}

#[test]
fn anchor_no_teleport_landing() {
    // Regression: landing must only snap when the floor is within the 78u
    // window below, never teleport from apex. A full jump (vy0 ~ 50) must
    // rise past 200, then fall gradually over many frames.
    let input = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: 1, // A held: no height control
        cam_yaw: Angle::ZERO,
    };
    let (s0, _) = one_tick(
        CharacterState {
            action: ActionId::IDLE,
            ..CharacterState::default()
        },
        RawInput {
            buttons: 1,
            ..input
        },
        0,
    );
    assert_eq!(s0.action, ActionId::JUMP);
    let mut st = s0;
    let mut max_y = 0.0f32;
    let mut fall_frames = 0;
    let mut falling = false;
    for _ in 0..80 {
        let (ns, _) = one_tick(st, input, 1);
        max_y = max_y.max(ns.pos.y);
        if ns.vel.y < 0.0 {
            falling = true;
        }
        if falling && ns.action == ActionId::JUMP {
            fall_frames += 1;
            // While falling (before the landing snap), descent is gradual:
            // at most terminal velocity per tick.
            assert!(
                st.pos.y - ns.pos.y <= 76.0,
                "teleport while falling: {} -> {}",
                st.pos.y,
                ns.pos.y
            );
        }
        st = ns;
        if st.action == ActionId::JUMP_LAND {
            break;
        }
    }
    assert!(max_y >= 199.0, "full jump should reach ~200, got {max_y}");
    assert!(fall_frames >= 5, "fall should take several frames");
    assert_eq!(st.action, ActionId::JUMP_LAND);
    assert!((st.pos.y - 0.0).abs() < 1e-3, "landed on the floor");
}

#[test]
fn anchor_wall_kick_entry() {
    // Wall kick: A during the kick window kicks away from the wall into
    // WALL_KICK_AIR. (ID verified: wiki:Wall Kick rev 19311;
    // numbers decomp-derived: fwd raised to min 24, vy 62)
    use stepkit_core::actions::air::enter_wall_kick;
    let world = flat_world();
    let params = MovementParams::default();
    let mut st = air_state(ActionId::JUMP, 20.0);
    st.wall_kick_timer = 10;
    st.wall = Some(stepkit_core::world::WallHit {
        normal: Vec3::new(0.0, 0.0, -1.0),
        ..Default::default()
    }); // wall at +z, normal toward -z
    let mut cx = action_cx(st, &world, &params);
    let r = enter_wall_kick(&mut cx, Vec3::new(0.0, 0.0, -1.0));
    assert!(matches!(r, ActionResult::Goto { .. }));
    assert_eq!(cx.state.action, ActionId::WALL_KICK_AIR);
    // Facing away from the wall: atan2(0, -1) = 180 deg = 0x8000.
    assert_eq!(cx.state.face_yaw.0, -0x8000);
    // Forward raised to the 24 minimum (entry was 20).
    assert!((cx.state.forward_speed - 24.0).abs() < 1e-3);
    assert!((cx.state.vel.y - 62.0).abs() < 1e-3);
    // A faster entry keeps its speed.
    let mut st2 = air_state(ActionId::JUMP, 32.0);
    st2.wall = Some(stepkit_core::world::WallHit {
        normal: Vec3::new(0.0, 0.0, -1.0),
        ..Default::default()
    });
    let mut cx2 = action_cx(st2, &world, &params);
    let _ = enter_wall_kick(&mut cx2, Vec3::new(0.0, 0.0, -1.0));
    assert!((cx2.state.forward_speed - 32.0).abs() < 1e-3);
}

#[test]
fn anchor_ledge_grab_conditions() {
    // Ledge grab needs: wall 30 above, no wall 150 above (wiki:Ledge Grab@19518).
    use stepkit_core::surface::SurfaceWorld;
    use stepkit_core::world::CollisionWorld;
    let mut w = SurfaceWorld::new();
    // Floor + a 400-tall wall.
    w.add_box(
        Vec3::new(-2000.0, -100.0, -2000.0),
        Vec3::new(2000.0, 0.0, 2000.0),
        stepkit_core::world::surface_type::DEFAULT,
        0,
    );
    w.add_box(
        Vec3::new(-200.0, 0.0, 400.0),
        Vec3::new(200.0, 400.0, 500.0),
        stepkit_core::world::surface_type::DEFAULT,
        0,
    );
    let mario = Vec3::new(0.0, 280.0, 350.0); // against the wall, falling zone
                                              // Wall 30 above (y=310 < 400): yes. Wall 150 above (y=430 > 400): no.
    assert!(w.wall_probe(mario + Vec3::new(0.0, 30.0, 0.0), 51.0));
    assert!(!w.wall_probe(mario + Vec3::new(0.0, 150.0, 0.0), 51.0));
    // Top-down floor search from 60 into the wall (+z) and 160 up finds the top.
    // (Legacy probe test; the ledge grab itself now runs inside the air step
    // via check_ledge_grab, ST-08.)
    let search = mario + Vec3::new(0.0, 160.0, 60.0);
    let f = w
        .find_floor(search.x, search.y, search.z)
        .expect("ledge floor");
    assert!((f.y - 400.0).abs() < 1e-3, "floor y {}", f.y);
}

#[test]
fn anchor_water_plunge_entry() {
    // Water plunge: y < surface-100 quarters horizontal, halves vertical,
    // snaps to surface-100. (wiki:Water Plunge, rev 2023-07-27)
    use stepkit_core::actions::water::{enter_water_plunge, id, water_plunge_surface};
    let mut world = flat_world();
    world.add_water(0.0, -1000.0, 1000.0, -1000.0, 1000.0);
    let params = MovementParams::default();
    let mut st = air_state(ActionId::JUMP, 20.0);
    st.pos = Vec3::new(0.0, -150.0, 0.0);
    st.vel = Vec3::new(20.0, -30.0, 0.0);
    let mut cx = action_cx(st, &world, &params);
    let wl = water_plunge_surface(&cx).expect("should plunge");
    assert!((wl - 0.0).abs() < 1e-3);
    let r = enter_water_plunge(&mut cx, wl);
    assert!(matches!(r, ActionResult::Goto(_, _)));
    assert_eq!(cx.state.action, id::WATER_PLUNGE);
    assert!((cx.state.pos.y - (-100.0)).abs() < 1e-3);
    assert!((cx.state.vel.x - 5.0).abs() < 1e-3); // 20/4
    assert!((cx.state.vel.y - (-15.0)).abs() < 1e-3); // -30/2
}

#[test]
fn anchor_wall_kick_air_flight() {
    // WALL_KICK_AIR: no-turn model (facing holds under stick), jump-height
    // control on (A released while rising quarters vy), landing -> JUMP_LAND.
    use stepkit_core::actions::air::enter_wall_kick;
    let world = flat_world();
    let params = MovementParams::default();
    // Enter via the kick path with enough speed for the dive cancel.
    let mut st = air_state(ActionId::JUMP, 32.0);
    st.wall = Some(stepkit_core::world::WallHit {
        normal: Vec3::new(0.0, 0.0, -1.0),
        ..Default::default()
    });
    let mut cx = action_cx(st, &world, &params);
    let _ = enter_wall_kick(&mut cx, Vec3::new(0.0, 0.0, -1.0));
    let st = cx.state;
    assert_eq!(st.action, ActionId::WALL_KICK_AIR);
    assert!(
        (st.forward_speed - 32.0).abs() < 1e-3,
        "speed kept, not min'd"
    );
    // No-turn: stick held sideways must not rotate facing.
    let yaw_before = st.face_yaw;
    let input = RawInput {
        stick_x: 80,
        stick_y: 0,
        buttons: 0, // A released
        cam_yaw: Angle::ZERO,
    };
    let vy_before = st.vel.y;
    assert!(vy_before > 20.0, "should be rising, vy {vy_before}");
    let (ns, _) = one_tick(st, input, 0);
    assert_eq!(ns.face_yaw, yaw_before, "no-turn model holds facing");
    assert!(
        ns.vel.y < vy_before / 3.0,
        "height control quarters vy: {} -> {}",
        vy_before,
        ns.vel.y
    );
    // B in flight -> dive.
    let input_b = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: buttons::B,
        cam_yaw: Angle::ZERO,
    };
    let (ns2, _) = one_tick(ns, input_b, 0);
    assert_eq!(
        ns2.action,
        ActionId::DIVE,
        "B -> dive, got {:?}",
        ns2.action
    );
}

#[test]
fn anchor_air_hit_wall_window() {
    // AIR_HIT_WALL: A in the 2-frame window -> WALL_KICK_AIR with vy 52;
    // after the window, impact >= 38 -> BACKWARD_AIR_KB (+5f late-kick
    // timer), else SOFT_BONK.
    fn hit_state(impact: f32) -> CharacterState {
        // decomp: act_air_hit_wall branches on m->forwardVel >= 38.0f
        // (airborne.c), not action_arg.
        let mut st = air_state(ActionId::AIR_HIT_WALL, impact);
        st.wall = Some(stepkit_core::world::WallHit {
        normal: Vec3::new(0.0, 0.0, -1.0),
        ..Default::default()
    });
        st.vel = Vec3::new(0.0, 10.0, -20.0); // bounced away from the wall
        st
    }
    let press_a = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: buttons::A,
        cam_yaw: Angle::ZERO,
    };
    let neutral = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: 0,
        cam_yaw: Angle::ZERO,
    };
    // A on the first window frame -> wall kick flight.
    // decomp: act_air_hit_wall sets vel.y=52, but set_mario_action_airborne
    // for ACT_WALL_KICK_AIR OVERWRITES it to 62 (airborne.c:1309, mario.c).
    // Same-frame gravity: 62 - 4 = 58.
    let (s, _) = one_tick(hit_state(40.0), press_a, 0);
    assert_eq!(s.action, ActionId::WALL_KICK_AIR, "A in window -> kick");
    assert!((s.vel.y - 58.0).abs() < 1e-3, "kick vy {}", s.vel.y);
    // No A: the decomp's missing-return quirk (airborne.c) re-executes
    // act_air_hit_wall in the same tick, consuming the 2-frame window in
    // a single tick. Impact 40 (>= 38) -> BACKWARD_AIR_KB immediately.
    let (s, _) = one_tick(hit_state(40.0), neutral, 0);
    assert_eq!(
        s.action,
        ActionId::BACKWARD_AIR_KB,
        "impact>=38 -> knockback"
    );
    assert_eq!(s.wall_kick_timer, 5, "late-kick allowance");
    // Impact 20 resolves to the soft bonk instead.
    let (s, _) = one_tick(hit_state(20.0), neutral, 0);
    assert_eq!(s.action, ActionId::SOFT_BONK, "impact<38 -> soft bonk");
}

#[test]
fn anchor_steep_jump_entry() {
    // Steep jump entry: vy = 42 + fwd/4, lateral velocity x0.75.
    use stepkit_core::actions::air::enter_steep_jump;
    let world = flat_world();
    let params = MovementParams::default();
    let mut st = air_state(ActionId::WALKING, 16.0);
    st.face_yaw = Angle::ZERO;
    st.vel = Vec3::new(8.0, 0.0, 16.0); // fwd 16, side 8
    let mut cx = action_cx(st, &world, &params);
    let r = enter_steep_jump(&mut cx);
    assert!(matches!(r, ActionResult::Goto { .. }));
    assert_eq!(cx.state.action, ActionId::STEEP_JUMP);
    assert!(
        (cx.state.vel.y - 46.0).abs() < 1e-3,
        "vy {}",
        cx.state.vel.y
    );
    assert!(
        (cx.state.vel.x - 6.0).abs() < 1e-3,
        "side x0.75: {}",
        cx.state.vel.x
    );
    assert!(
        (cx.state.vel.z - 16.0).abs() < 1e-3,
        "fwd kept: {}",
        cx.state.vel.z
    );
    // In flight the forward speed decays x0.98/frame with no stick control.
    let st = cx.state;
    let neutral = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: buttons::A, // hold A: no height-control cut
        cam_yaw: Angle::ZERO,
    };
    let (ns, _) = one_tick(st, neutral, 0);
    assert_eq!(ns.action, ActionId::STEEP_JUMP);
    assert!(
        (ns.forward_speed - 16.0 * 0.98).abs() < 1e-3,
        "fwd decayed: {}",
        ns.forward_speed
    );
}

/// Fake world reporting a steep floor (normal.y = 0.25, steeper than the
/// 0.866 steep-jump threshold for default floors; downhill yaw is +z).
struct SteepFloorWorld;
impl stepkit_core::world::CollisionWorld for SteepFloorWorld {
    fn find_floor(
        &self,
        _x: f32,
        _y: f32,
        _z: f32,
    ) -> Option<stepkit_core::world::FloorHit> {
        Some(stepkit_core::world::FloorHit {
            y: 0.0,
            normal: Vec3::new(0.0, 0.25, 0.9682458),
            kind: stepkit_core::world::SurfaceKind::Default,
            surface_type: stepkit_core::world::surface_type::DEFAULT,
            flags: 0,
            force: 0,
            user_data: 0,
        })
    }
    fn find_ceil(
        &self,
        _x: f32,
        _y: f32,
        _z: f32,
    ) -> Option<stepkit_core::world::CeilingHit> {
        None
    }
    fn find_wall_collisions(
        &self,
        x: f32,
        y: f32,
        z: f32,
        _offset: f32,
        _radius: f32,
    ) -> stepkit_core::world::WallCollision {
        stepkit_core::world::WallCollision {
            pos: Vec3::new(x, y, z),
            wall: None,
            num_walls: 0,
        }
    }
    fn find_water_level(&self, _x: f32, _z: f32) -> f32 {
        stepkit_core::world::FLOOR_LOWER_LIMIT
    }
}

#[test]
fn anchor_steep_jump_routing() {
    // A-jump from Walking on a very steep floor -> STEEP_JUMP;
    // on flat ground -> JUMP.
    use stepkit_core::step::tick;
    let params = MovementParams::default();
    let registry = ActionRegistry::sm64_style();
    let press_a = RawInput {
        stick_x: 0,
        stick_y: 80,
        buttons: buttons::A,
        cam_yaw: Angle::ZERO,
    };
    // Steep world.
    let world = SteepFloorWorld;
    let st = CharacterState {
        action: ActionId::WALKING,
        pos: Vec3::new(0.0, 0.0, 0.0),
        forward_speed: 10.0,
        // Face uphill (downhill yaw is +z for this floor normal); the
        // decomp steep-jump check requires NOT facing downhill.
        face_yaw: Angle::from_degrees(180.0),
        ..CharacterState::default()
    };
    let (ns, _, _) = tick(
        st,
        press_a,
        &world,
        &params,
        Timeline::default(),
        &registry,
        0,
    );
    assert_eq!(ns.action, ActionId::STEEP_JUMP, "steep floor -> steep jump");
    // Flat world -> normal jump.
    let flat = flat_world();
    let (ns, _, _) = tick(
        st,
        press_a,
        &flat,
        &params,
        Timeline::default(),
        &registry,
        0,
    );
    assert_eq!(ns.action, ActionId::JUMP, "flat floor -> jump");
}

#[test]
fn anchor_ground_pound_phases() {
    // Ground pound: 10-frame rise at (22 - 2*timer)/frame, vy pinned -50,
    // then slam at timer > 14. Low ceiling skips the rise.
    use stepkit_core::actions::air::enter_ground_pound;
    let world = flat_world();
    let params = MovementParams::default();
    let mut cx = action_cx(air_state(ActionId::JUMP, 0.0), &world, &params);
    let _ = enter_ground_pound(&mut cx);
    assert_eq!(cx.state.action, ActionId::GROUND_POUND);
    assert_eq!(cx.state.action_state, 0, "rise phase with headroom");
    let mut st = cx.state;
    let y0 = st.pos.y;
    let neutral = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: 0,
        cam_yaw: Angle::ZERO,
    };
    // First rise tick (timer 1): +20.
    let (ns, _) = one_tick(st, neutral, 0);
    assert!(
        (ns.pos.y - (y0 + 20.0)).abs() < 1e-3,
        "rise +20, y {}",
        ns.pos.y
    );
    assert!((ns.vel.y - (-50.0)).abs() < 1e-3, "vy pinned -50");
    st = ns;
    // Run to the slam transition (timer > 14).
    for _ in 0..14 {
        let (ns, _) = one_tick(st, neutral, 0);
        st = ns;
    }
    assert_eq!(st.action_state, 1, "slam phase after timer > 14");
    // One more tick runs the slam physics.
    let (ns, _) = one_tick(st, neutral, 0);
    assert!(ns.vel.y < -50.0, "slamming down, vy {}", ns.vel.y);
    // NOTE: the "low ceiling skips the rise" was a plugin invention. The
    // decomp checks headroom per-frame during the rise
    // (pos.y + yOffset + 160 < ceilHeight); there is no entry skip.
}

#[test]
fn anchor_long_jump_entry() {
    // Long jump: vy = 30, forward x1.5 capped at 48. (decomp-derived)
    use stepkit_core::actions::air::enter_long_jump;
    let world = flat_world();
    let params = MovementParams::default();
    let mut cx = action_cx(air_state(ActionId::WALKING, 32.0), &world, &params);
    let _ = enter_long_jump(&mut cx);
    assert!(
        (cx.state.vel.y - 30.0).abs() < 1e-3,
        "vy {}",
        cx.state.vel.y
    );
    assert!(
        (cx.state.forward_speed - 48.0).abs() < 1e-3,
        "32*1.5 capped at 48"
    );
    let mut cx2 = action_cx(air_state(ActionId::WALKING, 20.0), &world, &params);
    let _ = enter_long_jump(&mut cx2);
    assert!((cx2.state.forward_speed - 30.0).abs() < 1e-3, "20*1.5 = 30");
}

#[test]
fn anchor_air_graze_no_bonk() {
    // Air wall hit at speed <= 16: no bonk, no action change, but the decomp
    // ZEROS forwardVel (mario_set_forward_vel(m, 0.0f), airborne.c:371).
    // (The old test expected tangential velocity to be kept; the decomp
    // does not do that.)
    let mut world = flat_world();
    world.add_box(
        Vec3::new(-200.0, 0.0, 400.0),
        Vec3::new(200.0, 600.0, 500.0),
        stepkit_core::world::surface_type::DEFAULT,
        0,
    );
    let params = MovementParams::default();
    let registry = ActionRegistry::sm64_style();
    // Diagonal into the wall at speed ~14.1 (< 16), head-on yaw window.
    let mut st = air_state(ActionId::JUMP, 10.0);
    st.pos = Vec3::new(0.0, 100.0, 344.0);
    st.face_yaw = Angle::ZERO; // facing +z, wall normal -z: head-on
    st.vel = Vec3::new(10.0, 0.0, 10.0);
    let input = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: 0,
        cam_yaw: Angle::ZERO,
    };
    let (ns, _, _) = stepkit_core::step::tick(
        st,
        input,
        &world,
        &params,
        Timeline::default(),
        &registry,
        0,
    );
    assert_eq!(ns.action, ActionId::JUMP, "graze keeps the action");
    assert!(
        ns.forward_speed.abs() < 1e-3,
        "decomp zeros forwardVel on graze: {}",
        ns.forward_speed
    );
}

#[test]
fn anchor_knockback_ids() {
    // Phase C knockback family IDs (decomp ACT_* values; FD-07 fixed the
    // FORWARD/BACKWARD swap).
    assert_eq!(ActionId::BACKWARD_AIR_KB.0, 0x010208B0);
    assert_eq!(ActionId::FORWARD_AIR_KB.0, 0x010208B1);
    assert_eq!(ActionId::HARD_BACKWARD_AIR_KB.0, 0x010208B3);
    assert_eq!(ActionId::HARD_FORWARD_AIR_KB.0, 0x010208B2);
    assert_eq!(ActionId::SOFT_BONK.0, 0x010208B6);
    assert_eq!(ActionId::AIR_HIT_WALL.0, 0x000008A7);
    assert_eq!(ActionId::WALL_KICK_AIR.0, 0x03000886);
    assert_eq!(ActionId::STEEP_JUMP.0, 0x03000885);
    let r = ActionRegistry::sm64_style();
    for id in [
        ActionId::BACKWARD_AIR_KB,
        ActionId::HARD_BACKWARD_AIR_KB,
        ActionId::FORWARD_AIR_KB,
        ActionId::HARD_FORWARD_AIR_KB,
        ActionId::SOFT_BONK,
        ActionId::AIR_HIT_WALL,
        ActionId::WALL_KICK_AIR,
        ActionId::STEEP_JUMP,
    ] {
        assert!(r.get(id).is_some(), "{id:?} registered");
    }
    // Hard variants flag arg = 1.
    use stepkit_core::actions::air::enter_backward_air_kb;
    let world = flat_world();
    let params = MovementParams::default();
    let mut cx = action_cx(air_state(ActionId::JUMP, 0.0), &world, &params);
    let _ = enter_backward_air_kb(&mut cx, Vec3::new(0.0, 0.0, -1.0), true);
    assert_eq!(cx.state.action, ActionId::HARD_BACKWARD_AIR_KB);
    assert_eq!(cx.state.action_arg, 1);
}

// ================= Phase D anchors =================

/// Tick `n` frames with a fixed input in the given world.
fn tick_n(state: CharacterState, input: RawInput, n: u32, world: &SurfaceWorld) -> CharacterState {
    let params = MovementParams::default();
    let registry = ActionRegistry::sm64_style();
    let mut state = state;
    let mut prev = 0u16;
    for _ in 0..n {
        let (s, _, _) = stepkit_core::step::tick(
            state,
            input,
            world,
            &params,
            Timeline::default(),
            &registry,
            prev,
        );
        state = s;
        prev = input.buttons;
    }
    state
}

fn neutral_input() -> RawInput {
    RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: 0,
        cam_yaw: Angle::ZERO,
    }
}

fn quicksand_world() -> SurfaceWorld {
    let mut w = SurfaceWorld::new();
    w.add_box(
        Vec3::new(-2000.0, -100.0, -2000.0),
        Vec3::new(2000.0, 0.0, 2000.0),
        stepkit_core::world::surface_type::SHALLOW_QUICKSAND,
        0,
    );
    w
}

#[test]
fn anchor_fall_damage_hard() {
    // Fall height > 3000 with impact speed > 55: 16 damage and the
    // hard-knockback landing (arg 1 = hard).
    let world = flat_world();
    let params = MovementParams::default();
    let registry = ActionRegistry::sm64_style();
    let mut state = CharacterState {
        action: ActionId::FREEFALL,
        pos: Vec3::new(0.0, 3200.0, 0.0),
        vel: Vec3::ZERO,
        ..CharacterState::default()
    };
    let mut landed = None;
    let mut prev = 0u16;
    for _ in 0..200 {
        let (s, _, _) = stepkit_core::step::tick(
            state,
            neutral_input(),
            &world,
            &params,
            Timeline::default(),
            &registry,
            prev,
        );
        state = s;
        prev = 0;
        if state.action != ActionId::FREEFALL {
            landed = Some(state.action);
            break;
        }
    }
    assert_eq!(landed, Some(ActionId::HARD_BACKWARD_GROUND_KB));
    // decomp: drop_and_set_mario_action(hardFallAction, 4) — arg 4, not 1.
    assert_eq!(state.action_arg, 4, "hard flag in arg");
    // decomp: hurtCounter += 24 (no cap) on a >3000 fall; it decrements
    // each tick via update_mario_health, so by landing time it's less.
    // The key invariant: damage was applied (nonzero).
    assert!(state.hurt_counter > 0, "hurt applied, got {}", state.hurt_counter);
}

#[test]
fn anchor_fall_damage_squish() {
    // Fall height in (1150, 3000] on a non-slippery floor: 8 damage and
    // 30 frames of squish.
    let world = flat_world();
    let params = MovementParams::default();
    let registry = ActionRegistry::sm64_style();
    let mut state = CharacterState {
        action: ActionId::FREEFALL,
        pos: Vec3::new(0.0, 1300.0, 0.0),
        vel: Vec3::ZERO,
        ..CharacterState::default()
    };
    let mut prev = 0u16;
    for _ in 0..200 {
        let (s, _, _) = stepkit_core::step::tick(
            state,
            neutral_input(),
            &world,
            &params,
            Timeline::default(),
            &registry,
            prev,
        );
        state = s;
        prev = 0;
        if state.action == ActionId::FREEFALL_LAND {
            break;
        }
    }
    assert_eq!(state.action, ActionId::FREEFALL_LAND);
    // decomp: update_mario_health does health -= 0x40 per hurtCounter tick
    // (mario.c:1484). The 8 hurt becomes 0x40 health damage on the landing tick.
    assert_eq!(state.health, 0x880 - 0x40, "health {}", state.health);
    // The squish timer decrements after the action (decomp pipeline), so
    // the 30 set by the fall damage reads as 29 at the end of the tick.
    assert_eq!(state.squish_timer, 29, "squish {}", state.squish_timer);

    // While squished the intended stick magnitude is quartered: full
    // tilt (0,80) reshapes to intended 50, squished to 12.5.
    let stick = RawInput {
        stick_x: 0,
        stick_y: 80,
        buttons: 0,
        cam_yaw: Angle::ZERO,
    };
    let mut state = state;
    let proc = stepkit_core::input::process_input(&stick, 0, &mut state);
    let cx = ActionCx {
        state,
        input: stick,
        proc,
        world: &world,
        params: &params,
        timeline: Timeline::default(),
        events: Vec::new(),
    };
    // Decomp update_mario_joystick_inputs: stick 80 -> 74 after the -6
    // offset, capped to 64; (64/64)^2*64 = 64, eighthed while squished.
    let expect = 8.0f32;
    assert!(
        (cx.intended_magnitude() - expect).abs() < 1e-4,
        "squished mag {}",
        cx.intended_magnitude()
    );

    // Jump velocity is halved while squished.
    use stepkit_core::actions::air::enter_jump;
    let mut cx = action_cx(state, &world, &params);
    let _ = enter_jump(&mut cx);
    assert!(
        (cx.state.vel.y - 21.0).abs() < 1e-3,
        "squished jump vy {}",
        cx.state.vel.y
    );

    // A on landing gives a single jump only (double chain suppressed).
    let land = CharacterState {
        action: ActionId::JUMP_LAND,
        squish_timer: 30,
        land_from: ActionId::JUMP,
        ..CharacterState::default()
    };
    let press_a = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: buttons::A,
        cam_yaw: Angle::ZERO,
    };
    let (s, _, _) = stepkit_core::step::tick(
        land,
        press_a,
        &world,
        &params,
        Timeline::default(),
        &registry,
        0,
    );
    assert_eq!(s.action, ActionId::JUMP, "squish suppresses the chain");
}

#[test]
fn anchor_no_fall_damage_small_falls() {
    // A normal jump landing (~200 units) takes no damage and no squish.
    let world = flat_world();
    let state = CharacterState {
        action: ActionId::FREEFALL,
        pos: Vec3::new(0.0, 200.0, 0.0),
        vel: Vec3::new(0.0, -10.0, 0.0),
        ..CharacterState::default()
    };
    let mut s = state;
    let params = MovementParams::default();
    let registry = ActionRegistry::sm64_style();
    for _ in 0..200 {
        let (ns, _, _) = stepkit_core::step::tick(
            s,
            neutral_input(),
            &world,
            &params,
            Timeline::default(),
            &registry,
            0,
        );
        s = ns;
        if s.action == ActionId::FREEFALL_LAND {
            break;
        }
    }
    assert_eq!(s.health, 0x880);
    assert_eq!(s.squish_timer, 0);
}

#[test]
fn anchor_surface_classes() {
    use stepkit_core::world::SurfaceClass;
    // Kind -> class mapping.
    assert_eq!(
        SurfaceClass::of(SurfaceKind::Default),
        SurfaceClass::Default
    );
    assert_eq!(SurfaceClass::of(SurfaceKind::Slide), SurfaceClass::Slippery);
    assert_eq!(
        SurfaceClass::of(SurfaceKind::Quicksand),
        SurfaceClass::NotSlippery
    );
    // Per-class slide accel / loss.
    assert_eq!(SurfaceClass::VerySlippery.slide_accel(), 10.0);
    assert_eq!(SurfaceClass::VerySlippery.slide_loss(), 0.98);
    assert_eq!(SurfaceClass::Slippery.slide_accel(), 8.0);
    assert_eq!(SurfaceClass::Slippery.slide_loss(), 0.96);
    assert_eq!(SurfaceClass::Default.slide_accel(), 7.0);
    assert_eq!(SurfaceClass::Default.slide_loss(), 0.92);
    assert_eq!(SurfaceClass::NotSlippery.slide_accel(), 5.0);
    assert_eq!(SurfaceClass::NotSlippery.slide_loss(), 0.92);
    // Slippery-floor thresholds.
    assert!((SurfaceClass::VerySlippery.slippery_floor_y() - 0.9848077).abs() < 1e-6);
    assert!((SurfaceClass::Slippery.slippery_floor_y() - 0.9396926).abs() < 1e-6);
    assert!((SurfaceClass::Default.slippery_floor_y() - 0.7880108).abs() < 1e-6);
    assert!(
        SurfaceClass::NotSlippery.slippery_floor_y() < 0.0,
        "never slippery"
    );
    // Steep thresholds.
    assert!((SurfaceClass::VerySlippery.steep_y() - 0.9659258).abs() < 1e-6);
    assert!((SurfaceClass::Slippery.steep_y() - 0.9396926).abs() < 1e-6);
    assert!((SurfaceClass::Default.steep_y() - 0.8660254).abs() < 1e-6);
    assert!((SurfaceClass::NotSlippery.steep_y() - 0.8660254).abs() < 1e-6);
    // New actions registered.
    let r = ActionRegistry::sm64_style();
    for id in [
        ActionId::IN_QUICKSAND,
        ActionId::QUICKSAND_JUMP_LAND,
        ActionId::LEDGE_CLIMB_FAST,
        ActionId::LEDGE_CLIMB_SLOW_1,
        ActionId::LEDGE_CLIMB_SLOW_2,
    ] {
        assert!(r.get(id).is_some(), "{id:?} registered");
    }
    // New action IDs (decomp ACT_* values).
    assert_eq!(ActionId::IN_QUICKSAND.0, 0x0002020D);
    assert_eq!(ActionId::QUICKSAND_JUMP_LAND.0, 0x00000476);
    assert_eq!(ActionId::LEDGE_CLIMB_FAST.0, 0x0000054F);
    assert_eq!(ActionId::LEDGE_CLIMB_SLOW_1.0, 0x0000054C);
    assert_eq!(ActionId::LEDGE_CLIMB_SLOW_2.0, 0x0000054D);
}

#[test]
fn anchor_floor_classification_steep() {
    // Decomp classification (ST-02): normal.y > 0.01 is a floor, < -0.01 a
    // ceiling, else a wall. normal.y = length / sqrt(length^2 + height^2).
    use stepkit_core::world::CollisionWorld;
    let mut w = SurfaceWorld::new();
    w.add_ramp(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        100.0,
        229.13, // normal.y = 0.4
        100.0,
        stepkit_core::world::surface_type::DEFAULT,
        0,
    );
    let hit = w
        .find_floor(0.0, 150.0, 50.0)
        .expect("0.4-normal ramp is a floor");
    assert!((hit.normal.y - 0.4).abs() < 0.01, "ny {}", hit.normal.y);
    let mut w2 = SurfaceWorld::new();
    w2.add_ramp(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        100.0,
        489.9, // normal.y = 0.2
        100.0,
        stepkit_core::world::surface_type::DEFAULT,
        0,
    );
    // 0.2 > 0.01, so this IS a floor in the decomp (the old 0.2924
    // threshold was a plugin invention).
    assert!(
        w2.find_floor(0.0, 300.0, 50.0).is_some(),
        "0.2-normal ramp is a floor"
    );
}

#[test]
fn anchor_quicksand_sink() {
    let world = quicksand_world();
    // Stationary: 0.5/frame, min 1.1 on entry THEN the first tick's sink:
    // tick 1: max(0,1.1)+0.5=1.6; tick 4: 3.1.
    // decomp: mario_update_quicksand (mario_step.c:107).
    let s = tick_n(
        CharacterState {
            action: ActionId::IDLE,
            ..CharacterState::default()
        },
        neutral_input(),
        4,
        &world,
    );
    assert!(
        (s.quicksand_depth - 3.1).abs() < 1e-4,
        "depth {}",
        s.quicksand_depth
    );
    // Moving: 0.25/frame via the moving preamble's mario_update_quicksand:
    // tick 1: max(0,1.1)+0.25=1.35; tick 4: 2.1.
    // decomp: mario_update_quicksand (mario_step.c:107) from
    // mario_execute_moving_action (moving.c:1982).
    let stick = RawInput {
        stick_x: 0,
        stick_y: 80,
        buttons: 0,
        cam_yaw: Angle::ZERO,
    };
    let s = tick_n(
        CharacterState {
            action: ActionId::WALKING,
            ..CharacterState::default()
        },
        stick,
        4,
        &world,
    );
    assert!(
        (s.quicksand_depth - 2.1).abs() < 1e-4,
        "depth {}",
        s.quicksand_depth
    );
    // Shallow cap: 40 stationary ticks -> 10.0.
    let s = tick_n(
        CharacterState {
            action: ActionId::IDLE,
            ..CharacterState::default()
        },
        neutral_input(),
        40,
        &world,
    );
    assert!(
        (s.quicksand_depth - 10.0).abs() < 1e-4,
        "cap {}",
        s.quicksand_depth
    );
    // Other floors reset the depth.
    let flat = flat_world();
    let s = tick_n(
        CharacterState {
            action: ActionId::IDLE,
            quicksand_depth: 7.0,
            ..CharacterState::default()
        },
        neutral_input(),
        1,
        &flat,
    );
    assert_eq!(s.quicksand_depth, 0.0);
    // Jump velocity is halved while depth > 1.
    use stepkit_core::actions::air::enter_jump;
    let params = MovementParams::default();
    let mut cx = action_cx(
        CharacterState {
            quicksand_depth: 5.0,
            ..CharacterState::default()
        },
        &world,
        &params,
    );
    let _ = enter_jump(&mut cx);
    assert!(
        (cx.state.vel.y - 21.0).abs() < 1e-3,
        "halved jump vy {}",
        cx.state.vel.y
    );
}

#[test]
fn anchor_quicksand_jump_land() {
    // A-press with quicksand depth >= 11 -> QUICKSAND_JUMP_LAND
    // (decomp set_jump_from_landing, mario.c:1018): 13 frames, drains
    // the depth, then land stop. NOTE: requires deep QUICKSAND (cap 60);
    // SHALLOW_QUICKSAND caps depth at 10 (mario_step.c:107).
    let mut world = SurfaceWorld::new();
    world.add_box(
        Vec3::new(-2000.0, -100.0, -2000.0),
        Vec3::new(2000.0, 0.0, 2000.0),
        stepkit_core::world::surface_type::QUICKSAND,
        0,
    );
    let params = MovementParams::default();
    let registry = ActionRegistry::sm64_style();
    // Start in a land-stop (which routes A-press through set_jump_from_landing)
    // with quicksand depth >= 11.
    let mut state = CharacterState {
        action: ActionId::JUMP_LAND_STOP,
        pos: Vec3::new(0.0, 0.0, 0.0),
        quicksand_depth: 12.0,
        ..CharacterState::default()
    };
    // Press A: set_jump_from_landing routes to QUICKSAND_JUMP_LAND.
    let press_a = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: buttons::A,
        cam_yaw: Angle::ZERO,
    };
    let (s, _, _) = stepkit_core::step::tick(
        state,
        press_a,
        &world,
        &params,
        Timeline::default(),
        &registry,
        0,
    );
    state = s;
    assert_eq!(state.action, ActionId::QUICKSAND_JUMP_LAND);
    let depth_at_entry = state.quicksand_depth;
    let s = tick_n(state, neutral_input(), 13, &world);
    // The decomp ends at JUMP_LAND_STOP (not IDLE): act_quicksand_jump_land
    // passes ACT_JUMP_LAND_STOP as endAction (moving.c:1941).
    assert_eq!(s.action, ActionId::JUMP_LAND_STOP, "escape ends at land stop");
    assert!(
        s.quicksand_depth < depth_at_entry,
        "depth drained {} -> {}",
        depth_at_entry,
        s.quicksand_depth
    );
}

#[test]
fn anchor_in_quicksand() {
    // Depth > 30: IN_QUICKSAND. A jumps (vy halved), B punches, Z crouches,
    // depth < 30 exits to idle. NOTE: the floor must be QUICKSAND (cap 60),
    // not SHALLOW_QUICKSAND (cap 10) — the decomp clamps shallow depth at
    // 10.0 in mario_update_quicksand (mario_step.c:107), so depth > 30 is
    // unreachable on shallow sand.
    let mut world = SurfaceWorld::new();
    world.add_box(
        Vec3::new(-2000.0, -100.0, -2000.0),
        Vec3::new(2000.0, 0.0, 2000.0),
        stepkit_core::world::surface_type::QUICKSAND,
        0,
    );
    let params = MovementParams::default();
    let registry = ActionRegistry::sm64_style();
    let s = tick_n(
        CharacterState {
            action: ActionId::IDLE,
            quicksand_depth: 31.0,
            ..CharacterState::default()
        },
        neutral_input(),
        1,
        &world,
    );
    assert_eq!(s.action, ActionId::IN_QUICKSAND);
    // A jumps (halved vy in quicksand).
    let press_a = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: buttons::A,
        cam_yaw: Angle::ZERO,
    };
    let (sj, _, _) = stepkit_core::step::tick(
        s,
        press_a,
        &world,
        &params,
        Timeline::default(),
        &registry,
        0,
    );
    assert_eq!(sj.action, ActionId::QUICKSAND_JUMP_LAND, "A jumps");
    // Quicksand jump land: the decomp routes IN_QUICKSAND's A press through
    // set_jumping_action, which sends depth >= 11 to QUICKSAND_JUMP_LAND
    // (mario.c:1074). A regular JUMP from quicksand requires depth < 11,
    // which is unreachable while IN_QUICKSAND (entry needs > 30).
    // B punches (Phase E: waist-deep punch instead of a second jump).
    let press_b = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: buttons::B,
        cam_yaw: Angle::ZERO,
    };
    let (sp, _, _) = stepkit_core::step::tick(
        s,
        press_b,
        &world,
        &params,
        Timeline::default(),
        &registry,
        0,
    );
    assert_eq!(sp.action, ActionId::PUNCHING, "B punches");
    let press_z = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: buttons::Z,
        cam_yaw: Angle::ZERO,
    };
    let (sz, _, _) = stepkit_core::step::tick(
        s,
        press_z,
        &world,
        &params,
        Timeline::default(),
        &registry,
        0,
    );
    // Z crouches via START_CROUCHING (decomp check_common_idle_cancels,
    // stationary.c:54: set_mario_action(ACT_START_CROUCHING)).
    assert_eq!(sz.action, ActionId::START_CROUCHING, "Z crouches");
    // Depth < 30 -> IDLE.
    let s2 = tick_n(
        CharacterState {
            action: ActionId::IN_QUICKSAND,
            quicksand_depth: 20.0,
            ..CharacterState::default()
        },
        neutral_input(),
        1,
        &world,
    );
    assert_eq!(s2.action, ActionId::IDLE);
}

/// Wall + flat ledge top at y=400 for ledge tests.
fn ledge_world() -> SurfaceWorld {
    let mut w = SurfaceWorld::new();
    w.add_box(
        Vec3::new(-2000.0, -100.0, -2000.0),
        Vec3::new(2000.0, 0.0, 2000.0),
        stepkit_core::world::surface_type::DEFAULT,
        0,
    );
    w.add_box(
        Vec3::new(-200.0, 0.0, 400.0),
        Vec3::new(200.0, 400.0, 500.0),
        stepkit_core::world::surface_type::DEFAULT,
        0,
    );
    w
}

fn hanging_state() -> CharacterState {
    CharacterState {
        action: ActionId::LEDGE_GRAB,
        // Decomp hang semantics (ST-08 check_ledge_grab): the feet are placed
        // AT the ledge top (y=400), 60 units into the wall from the push
        // position, i.e. over the ledge (z=430 for a wall face at z=400).
        // The geometry stage's find_floor then finds the ledge top.
        pos: Vec3::new(0.0, 400.0, 430.0),
        face_yaw: Angle::ZERO, // facing +z, toward the wall
        floor_y: Some(400.0), // the ledge top he's hanging from
        ..CharacterState::default()
    }
}

#[test]
fn anchor_ledge_let_go() {
    // Stick more than 90 deg from the facing (away from the wall) lets go,
    // but ONLY when actionTimer == 10 (decomp: act_ledge_grab, automatic.c).
    // decomp: let_go_of_ledge returns set_mario_action(ACT_SOFT_BONK, 0)
    // (automatic.c:490), so the action is SOFT_BONK (same-tick, TRUE).
    let world = ledge_world();
    let params = MovementParams::default();
    let registry = ActionRegistry::sm64_style();
    let away = RawInput {
        stick_x: 0,
        stick_y: -80, // intended yaw 180 deg, facing is 0
        buttons: 0,
        cam_yaw: Angle::ZERO,
    };
    let mut hang = hanging_state();
    hang.action_timer = 10; // decomp: stick-away release only at timer == 10
    let (s, _, _) = stepkit_core::step::tick(
        hang,
        away,
        &world,
        &params,
        Timeline::default(),
        &registry,
        0,
    );
    assert_eq!(s.action, ActionId::SOFT_BONK, "stick away lets go");
    // Z also drops.
    let press_z = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: buttons::Z,
        cam_yaw: Angle::ZERO,
    };
    let (s, _, _) = stepkit_core::step::tick(
        hanging_state(),
        press_z,
        &world,
        &params,
        Timeline::default(),
        &registry,
        0,
    );
    // Z also drops via let_go_of_ledge → SOFT_BONK (decomp: automatic.c:490).
    // (The old comment claimed FREEFALL; the decomp returns SOFT_BONK.)
    assert_eq!(s.action, ActionId::SOFT_BONK);
}

#[test]
fn anchor_ledge_climb_fast() {
    // A with headroom -> LEDGE_CLIMB_FAST -> IDLE after 8 frames, pulled
    // onto the ledge.
    let world = ledge_world();
    let params = MovementParams::default();
    let registry = ActionRegistry::sm64_style();
    let press_a = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: buttons::A,
        cam_yaw: Angle::ZERO,
    };
    let (s, _, _) = stepkit_core::step::tick(
        hanging_state(),
        press_a,
        &world,
        &params,
        Timeline::default(),
        &registry,
        0,
    );
    assert_eq!(s.action, ActionId::LEDGE_CLIMB_FAST);
    // decomp: update_ledge_climb transitions to IDLE when is_anim_at_end.
    // MARIO_ANIM_FAST_LEDGE_GRAB (0x34) has loop_end=16: 16 climb ticks
    // (frames -1..15), then IDLE on the 17th tick (next-tick transition).
    let s = tick_n(s, neutral_input(), 17, &world);
    assert_eq!(s.action, ActionId::IDLE, "fast climb ends idle");
    // Pulled onto the ledge top (z=400..500).
    assert!(s.pos.z > 400.0, "pulled onto the ledge, z={}", s.pos.z);
    assert!(
        (s.pos.y - 400.0).abs() < 1.0,
        "on the ledge top, y={}",
        s.pos.y
    );
}

#[test]
fn anchor_ledge_climb_fast_thin_ledge() {
    // A thin floating ledge (like the showcase scene): the hang puts the
    // feet BELOW the ledge underside, but the headroom check must probe
    // from the hands so the ledge's own underside doesn't block the climb.
    let mut w = SurfaceWorld::new();
    w.add_box(
        Vec3::new(-2000.0, -100.0, -2000.0),
        Vec3::new(2000.0, 0.0, 2000.0),
        stepkit_core::world::surface_type::DEFAULT,
        0,
    );
    // Thin ledge: top at y=400, underside at y=340.
    w.add_box(
        Vec3::new(-200.0, 340.0, 400.0),
        Vec3::new(200.0, 400.0, 500.0),
        stepkit_core::world::surface_type::DEFAULT,
        0,
    );
    let params = MovementParams::default();
    let registry = ActionRegistry::sm64_style();
    let press_a = RawInput {
        stick_x: 0,
        stick_y: 0,
        buttons: buttons::A,
        cam_yaw: Angle::ZERO,
    };
    // Hanging (decomp check_ledge_grab placement): feet at the 400 ledge top.
    // The thin ledge's underside (340) does not block the climb: the decomp's
    // hasSpaceForMario = (ceilHeight - floorHeight >= 160) uses the ceiling
    // above floorHeight + 80, not a probe from the hang position.
    let hang = CharacterState {
        action: ActionId::LEDGE_GRAB,
        pos: Vec3::new(0.0, 400.0, 440.0),
        face_yaw: Angle::ZERO,
        floor_y: Some(400.0),
        ..CharacterState::default()
    };
    let (s, _, _) = stepkit_core::step::tick(
        hang,
        press_a,
        &w,
        &params,
        Timeline::default(),
        &registry,
        0,
    );
    assert_eq!(
        s.action,
        ActionId::LEDGE_CLIMB_FAST,
        "A climbs off a thin ledge"
    );
}

#[test]
fn anchor_ledge_climb_slow() {
    // Stick toward the wall at 10+ hang frames -> SLOW_1 -> SLOW_2
    // (at animFrame 17) -> IDLE when the anim ends (decomp automatic.c:600).
    let world = ledge_world();
    let params = MovementParams::default();
    let registry = ActionRegistry::sm64_style();
    let toward = RawInput {
        stick_x: 0,
        stick_y: 80, // intended yaw 0 = toward the wall
        buttons: 0,
        cam_yaw: Angle::ZERO,
    };
    let mut hang = hanging_state();
    hang.action_timer = 9; // tick() increments to 10
    let (s, _, _) = stepkit_core::step::tick(
        hang,
        toward,
        &world,
        &params,
        Timeline::default(),
        &registry,
        0,
    );
    assert_eq!(s.action, ActionId::LEDGE_CLIMB_SLOW_1);
    // Run the climb to completion; the decomp switches SLOW_1 → SLOW_2 at
    // animFrame 17 and ends at IDLE via update_ledge_climb.
    let mut s = s;
    let mut tl = Timeline::default();
    let mut saw_slow_2 = false;
    for _ in 0..60 {
        let (ns, ntl, _) = stepkit_core::step::tick(s, toward, &world, &params, tl, &registry, 0);
        s = ns;
        tl = ntl;
        if s.action == ActionId::LEDGE_CLIMB_SLOW_2 {
            saw_slow_2 = true;
        }
        if s.action == ActionId::IDLE {
            break;
        }
    }
    assert!(saw_slow_2, "slow climb passes through SLOW_2");
    // The climb ends via update_ledge_climb → IDLE, but the held stick
    // chains IDLE → WALKING in the same tick (decomp check_common_idle_cancels).
    assert_eq!(s.action, ActionId::WALKING, "slow climb ends walking");
}

#[test]
fn anchor_ledge_release_steep() {
    // A ledge floor steeper than normal.y 0.9063 releases via let_go_of_ledge.
    let mut w = SurfaceWorld::new();
    // Steep ramp (normal.y = 0.855) as the "ledge" top. The hang is placed
    // at the ramp's high edge (decomp check_ledge_grab placement).
    w.add_ramp(
        Vec3::new(0.0, 400.0, 430.0),
        Vec3::new(0.0, 0.0, -1.0),
        100.0,
        -60.6,
        100.0,
        stepkit_core::world::surface_type::DEFAULT,
        0,
    );
    let params = MovementParams::default();
    let registry = ActionRegistry::sm64_style();
    let state = CharacterState {
        action: ActionId::LEDGE_GRAB,
        // Decomp hang placement (ST-08): feet at the ledge top. The steep
        // ramp (normal.y = 0.855 < 0.9063) is the floor here, so
        // act_ledge_grab releases via let_go_of_ledge.
        pos: Vec3::new(0.0, 400.0, 430.0),
        face_yaw: Angle::ZERO,
        floor_y: Some(400.0),
        ..CharacterState::default()
    };
    let (s, _, _) = stepkit_core::step::tick(
        state,
        neutral_input(),
        &w,
        &params,
        Timeline::default(),
        &registry,
        0,
    );
    assert_eq!(s.action, ActionId::SOFT_BONK, "steep ledge releases");
    // decomp: let_go_of_ledge → ACT_SOFT_BONK (automatic.c:490).
}

// ================= Phase E anchors =================

/// Tick `total` frames from `state`, pressing `buttons` on the frames
/// listed in `presses` (0-based). Returns the state after each frame.
fn tick_seq(
    state: CharacterState,
    world: &SurfaceWorld,
    presses: &[(u32, u16)],
    total: u32,
) -> Vec<CharacterState> {
    let params = MovementParams::default();
    let registry = ActionRegistry::sm64_style();
    let mut state = state;
    let mut prev = 0u16;
    let mut out = Vec::with_capacity(total as usize);
    for f in 0..total {
        let b = presses
            .iter()
            .find(|(ff, _)| *ff == f)
            .map(|(_, b)| *b)
            .unwrap_or(0);
        let input = RawInput {
            stick_x: 0,
            stick_y: 0,
            buttons: b,
            cam_yaw: Angle::ZERO,
        };
        let (s, _, _) = stepkit_core::step::tick(
            state,
            input,
            world,
            &params,
            Timeline::default(),
            &registry,
            prev,
        );
        state = s;
        prev = b;
        out.push(state);
    }
    out
}

fn idle_on_floor() -> CharacterState {
    CharacterState {
        action: ActionId::IDLE,
        pos: Vec3::new(0.0, 0.0, 0.0),
        ..CharacterState::default()
    }
}

#[test]
fn anchor_combat_action_ids() {
    // Phase E combat/knockback family IDs (decomp action index).
    assert_eq!(ActionId::PUNCHING.0, 0x00800380);
    assert_eq!(ActionId::MOVE_PUNCHING.0, 0x00800457);
    assert_eq!(ActionId::JUMP_KICK.0, 0x018008AC);
    assert_eq!(ActionId::BACKWARD_GROUND_KB.0, 0x00020462);
    assert_eq!(ActionId::FORWARD_GROUND_KB.0, 0x00020463);
    assert_eq!(ActionId::HARD_BACKWARD_GROUND_KB.0, 0x00020460);
    assert_eq!(ActionId::HARD_FORWARD_GROUND_KB.0, 0x00020461);
    assert_eq!(ActionId::SOFT_BACKWARD_GROUND_KB.0, 0x00020464);
    assert_eq!(ActionId::SOFT_FORWARD_GROUND_KB.0, 0x00020465);
    assert_eq!(ActionId::GROUND_BONK.0, 0x00020466);
    let r = ActionRegistry::sm64_style();
    for id in [
        ActionId::PUNCHING,
        ActionId::MOVE_PUNCHING,
        ActionId::JUMP_KICK,
        ActionId::BACKWARD_GROUND_KB,
        ActionId::FORWARD_GROUND_KB,
        ActionId::HARD_BACKWARD_GROUND_KB,
        ActionId::HARD_FORWARD_GROUND_KB,
        ActionId::SOFT_BACKWARD_GROUND_KB,
        ActionId::SOFT_FORWARD_GROUND_KB,
        ActionId::GROUND_BONK,
    ] {
        assert!(r.get(id).is_some(), "{id:?} registered");
    }
}

#[test]
fn anchor_punch_combo_chains() {
    // B chains through the decomp's actionArg combo: arg 0/1 (first punch)
    // -> B during the fast recovery (arg 2) -> arg 3/4 (second punch) ->
    // B during its recovery (arg 5) -> arg 6 (kick) -> IDLE at anim end.
    // The combo is anim-end-driven, not frame-budget-driven
    // (mario_actions_object.c:28, mario_update_punch_sequence).
    let world = flat_world();
    let states = tick_seq(
        idle_on_floor(),
        &world,
        &[(0, buttons::B), (10, buttons::B), (20, buttons::B)],
        60,
    );
    assert_eq!(states[0].action, ActionId::PUNCHING);
    assert_eq!(states[0].action_arg, 1, "first punch");
    assert_eq!(states[10].action_arg, 3, "second punch after 2nd B");
    assert_eq!(states[20].action_arg, 6, "kick after 3rd B");
    // Kick runs to its anim end, then IDLE.
    assert_eq!(states[25].action, ActionId::PUNCHING);
    let idle_at = states.iter().position(|s| s.action == ActionId::IDLE).unwrap();
    assert!(idle_at > 25, "kick idles after its anim, at {idle_at}");
}

#[test]
fn anchor_punch_no_chain_idles() {
    // A single B: the first punch (arg 0/1) runs into its fast recovery
    // (arg 2), which ends at anim end -> IDLE. Anim-driven, not a frame
    // budget (mario_actions_object.c:28).
    let world = flat_world();
    let states = tick_seq(idle_on_floor(), &world, &[(0, buttons::B)], 20);
    assert_eq!(states[0].action, ActionId::PUNCHING);
    assert_eq!(states[15].action, ActionId::PUNCHING);
    assert_eq!(states[16].action, ActionId::IDLE);
}

#[test]
fn anchor_punch_a_first_frame_jump_kick() {
    // A HELD on the punch entry tick (actionState == 0) becomes a jump
    // kick, vy 20 — the decomp checks INPUT_A_DOWN, not A_PRESSED
    // (mario_actions_object.c:147: `if (m->actionState == 0 &&
    // (m->input & INPUT_A_DOWN))`).
    let world = flat_world();
    let states = tick_seq(
        idle_on_floor(),
        &world,
        &[(0, buttons::B | buttons::A)],
        6,
    );
    assert_eq!(states[0].action, ActionId::JUMP_KICK, "A held on entry");
    // Same-frame gravity: 20 - 4 = 16.
    assert!(
        (states[0].vel.y - 16.0).abs() < 1e-3,
        "jump kick vy {}",
        states[0].vel.y
    );
}

#[test]
fn anchor_b_ground_routing() {
    // B on the ground: dive only at speed >= 29 with a hard shove;
    // otherwise MOVE_PUNCHING at speed >= 8, PUNCHING below.
    let world = flat_world();
    // Walking at speed 10, neutral stick: moving punch, momentum kept.
    let walk = CharacterState {
        action: ActionId::WALKING,
        forward_speed: 10.0,
        pos: Vec3::new(0.0, 0.0, 0.0),
        vel: Vec3::new(0.0, 0.0, 10.0),
        ..CharacterState::default()
    };
    let states = tick_seq(walk, &world, &[(0, buttons::B)], 3);
    assert_eq!(states[0].action, ActionId::MOVE_PUNCHING);
    // Same-frame: the punch body runs in the entry tick, decaying speed by
    // apply_slope_decel(0.5) — coef 0.5 * default-class 2.0 = 1.0/frame
    // (moving.c:850 via apply_slope_decel at moving.c:393).
    assert!(
        (states[0].forward_speed - 9.0).abs() < 1e-3,
        "momentum kept {}",
        states[0].forward_speed
    );
    // Walking at speed 5: still MOVE_PUNCHING — the decomp's
    // check_ground_dive_or_punch always picks ACT_MOVE_PUNCHING on B
    // (moving.c:485); there is no slow-speed PUNCHING branch.
    let slow = CharacterState {
        forward_speed: 5.0,
        vel: Vec3::new(0.0, 0.0, 5.0),
        ..walk
    };
    let states = tick_seq(slow, &world, &[(0, buttons::B)], 3);
    assert_eq!(states[0].action, ActionId::MOVE_PUNCHING);
    // Idle B: stationary punch.
    let states = tick_seq(idle_on_floor(), &world, &[(0, buttons::B)], 3);
    assert_eq!(states[0].action, ActionId::PUNCHING);
    // Crouch B (with Z held): stationary punch. The decomp's act_crouching
    // requires INPUT_Z_DOWN to stay crouched (stationary.c:543); without Z
    // it goes to STOP_CROUCHING first.
    let crouch = CharacterState {
        action: ActionId::CROUCH,
        ..idle_on_floor()
    };
    let states = tick_seq(crouch, &world, &[(0, buttons::B | buttons::Z)], 3);
    assert_eq!(states[0].action, ActionId::PUNCHING);
}

#[test]
fn anchor_jump_kick_slow_air() {
    // B in slow air (forward speed <= 28) -> JUMP_KICK, vy = 20.
    let world = flat_world();
    let air = air_state(ActionId::JUMP, 10.0);
    let states = tick_seq(air, &world, &[(0, buttons::B)], 3);
    assert_eq!(states[0].action, ActionId::JUMP_KICK);
    // Same-frame gravity: 20 - 4 = 16.
    assert!(
        (states[0].vel.y - 16.0).abs() < 1e-3,
        "vy {}",
        states[0].vel.y
    );
    // Horizontal speed is kept.
    assert!(
        (states[0].forward_speed - 10.0).abs() < 1.0,
        "keeps horizontal {}",
        states[0].forward_speed
    );
    // B in fast air (speed > 28) still dives.
    let fast = air_state(ActionId::JUMP, 30.0);
    let states = tick_seq(fast, &world, &[(0, buttons::B)], 3);
    assert_eq!(states[0].action, ActionId::DIVE, "fast air B still dives");
}

#[test]
fn anchor_ground_kb_decel_windows() {
    // Each ground knockback decays to IDLE at its animation's end — the
    // decomp ends via `is_anim_at_end(m)` (moving.c:1630), NOT a timer.
    // The old "frame windows" (22/20/43/21/100/100/32) are the arg2 DECEL
    // gates (`if (animFrame < arg2) apply_landing_accel(m, 0.9f)`,
    // moving.c:1613), not end frames. Idle frames below are 0-indexed
    // (the state after that many ticks).
    let world = flat_world();
    for (id, idle_frame) in [
        (ActionId::BACKWARD_GROUND_KB, 31u32),
        (ActionId::FORWARD_GROUND_KB, 31u32),
        (ActionId::HARD_BACKWARD_GROUND_KB, 81u32),
        (ActionId::HARD_FORWARD_GROUND_KB, 81u32),
        (ActionId::SOFT_BACKWARD_GROUND_KB, 25u32),
        (ActionId::SOFT_FORWARD_GROUND_KB, 25u32),
        (ActionId::GROUND_BONK, 41u32),
    ] {
        let mut start = CharacterState {
            action: id,
            forward_speed: -15.0,
            pos: Vec3::new(0.0, 0.0, 0.0),
            vel: Vec3::new(0.0, 0.0, -15.0),
            ..CharacterState::default()
        };
        if id == ActionId::GROUND_BONK {
            // The bonk moves on the slide vector.
            start.slide_vel_x = 0.0;
            start.slide_vel_z = -15.0;
        }
        let states = tick_seq(start, &world, &[], idle_frame + 2);
        // Still active the frame before the anim ends.
        assert_eq!(
            states[idle_frame as usize - 1].action,
            id,
            "{id:?} still active at frame {}",
            idle_frame - 1
        );
        assert_eq!(
            states[idle_frame as usize].action,
            ActionId::IDLE,
            "{id:?} idles at frame {idle_frame}"
        );
    }
}

#[test]
fn anchor_air_kb_landing_routing() {
    // Air knockbacks land into the matching ground knockbacks; the hard
    // flag travels as the action id; soft bonk routes by speed sign.
    let world = flat_world();
    for (air_id, ground_id) in [
        (ActionId::BACKWARD_AIR_KB, ActionId::BACKWARD_GROUND_KB),
        (
            ActionId::HARD_BACKWARD_AIR_KB,
            ActionId::HARD_BACKWARD_GROUND_KB,
        ),
        (ActionId::FORWARD_AIR_KB, ActionId::FORWARD_GROUND_KB),
        (
            ActionId::HARD_FORWARD_AIR_KB,
            ActionId::HARD_FORWARD_GROUND_KB,
        ),
    ] {
        let speed = if matches!(
            air_id,
            ActionId::BACKWARD_AIR_KB | ActionId::HARD_BACKWARD_AIR_KB
        ) {
            -15.0
        } else {
            16.0
        };
        let mut st = air_state(air_id, speed);
        st.vel = Vec3::new(0.0, -10.0, speed);
        // Tick until touchdown.
        let mut landed = ActionId::IDLE;
        for _ in 0..120 {
            let (s, _, _) = stepkit_core::step::tick(
                st,
                neutral_input(),
                &world,
                &MovementParams::default(),
                Timeline::default(),
                &ActionRegistry::sm64_style(),
                0,
            );
            st = s;
            if st.action == ground_id {
                landed = ground_id;
                break;
            }
        }
        assert_eq!(landed, ground_id, "{air_id:?} -> {ground_id:?}");
    }
    // Soft bonk: lands as FREEFALL_LAND (decomp act_soft_bonk passes
    // ACT_FREEFALL_LAND to common_air_knockback_step, airborne.c).
    for speed in [-10.0, 10.0] {
        let mut st = air_state(ActionId::SOFT_BONK, speed);
        st.vel = Vec3::new(0.0, -10.0, speed);
        let mut landed = ActionId::IDLE;
        for _ in 0..120 {
            let (s, _, _) = stepkit_core::step::tick(
                st,
                neutral_input(),
                &world,
                &MovementParams::default(),
                Timeline::default(),
                &ActionRegistry::sm64_style(),
                0,
            );
            st = s;
            if st.action == ActionId::FREEFALL_LAND {
                landed = ActionId::FREEFALL_LAND;
                break;
            }
        }
        assert_eq!(landed, ActionId::FREEFALL_LAND, "soft bonk at speed {speed}");
    }
}

/// Flat floor with a wall ahead at z = 400..500 for slide-bonk tests.
fn slide_bonk_world() -> SurfaceWorld {
    let mut w = SurfaceWorld::new();
    w.add_box(
        Vec3::new(-2000.0, -100.0, -2000.0),
        Vec3::new(2000.0, 0.0, 2000.0),
        stepkit_core::world::surface_type::DEFAULT,
        0,
    );
    w.add_box(
        Vec3::new(-200.0, 0.0, 400.0),
        Vec3::new(200.0, 600.0, 500.0),
        stepkit_core::world::surface_type::DEFAULT,
        0,
    );
    w
}

#[test]
fn anchor_slide_bonk_routing() {
    // Butt slide into a wall (non-slippery): common_slide_action calls
    // slide_bonk(GROUND_BONK, BUTT_SLIDE_STOP) (moving.c:1393). Speed > 16
    // -> GROUND_BONK with a reflected slide vector (mario_bonk_reflection);
    // speed <= 16 -> BUTT_SLIDE_STOP with the speed zeroed (no reflection).
    // decomp: slide_bonk (mario_actions_moving.c:137).
    let world = slide_bonk_world();
    let params = MovementParams::default();
    let registry = ActionRegistry::sm64_style();
    // Fast slide toward +z at speed 30, starting 50 units from the wall face
    // (z=400) so it hits at speed > 16.
    let mut st = CharacterState {
        action: ActionId::BUTT_SLIDE,
        slide_vel_x: 0.0,
        slide_vel_z: 30.0,
        forward_speed: 30.0,
        face_yaw: Angle::ZERO, // facing +z, toward the wall
        pos: Vec3::new(0.0, 0.0, 350.0),
        vel: Vec3::new(0.0, 0.0, 30.0),
        ..CharacterState::default()
    };
    let mut bonked = false;
    for _ in 0..40 {
        let (s, _, _) = stepkit_core::step::tick(
            st,
            neutral_input(),
            &world,
            &params,
            Timeline::default(),
            &registry,
            0,
        );
        st = s;
        if st.action == ActionId::GROUND_BONK {
            bonked = true;
            break;
        }
    }
    assert!(bonked, "fast slide wall hit -> GROUND_BONK");
    assert!(
        st.slide_vel_z < 0.0,
        "slide vector reflected {}",
        st.slide_vel_z
    );
    // NOTE: the facing reflection (mario_bonk_reflection) requires the
    // recorded upper wall; the test geometry doesn't guarantee an upper
    // wall hit, so the facing is not asserted here.

    // Slow slide at speed 12, closer to the wall -> DECELERATING.
    let mut st = CharacterState {
        action: ActionId::BUTT_SLIDE,
        slide_vel_x: 0.0,
        slide_vel_z: 12.0,
        forward_speed: 12.0,
        face_yaw: Angle::ZERO,
        pos: Vec3::new(0.0, 0.0, 350.0),
        vel: Vec3::new(0.0, 0.0, 12.0),
        ..CharacterState::default()
    };
    // The slow slide hits the wall below the bonk threshold: slide_bonk
    // routes to DECELERATING, which (at zero speed) settles to IDLE in the
    // same tick via the action loop. Either observable is fine; the key
    // property is that it must not bonk.
    let mut stopped = false;
    for _ in 0..40 {
        let (s, _, _) = stepkit_core::step::tick(
            st,
            neutral_input(),
            &world,
            &params,
            Timeline::default(),
            &registry,
            0,
        );
        st = s;
        if st.action == ActionId::DECELERATING || st.action == ActionId::IDLE {
            stopped = true;
            break;
        }
        // Must not bonk at low speed.
        assert_ne!(st.action, ActionId::GROUND_BONK, "slow slide must not bonk");
    }
    assert!(stopped, "slow slide wall hit -> DECELERATING/IDLE");
}

fn pool_world() -> SurfaceWorld {
    // Deep pool: water surface at y=0, floor at y=-300.
    let mut w = SurfaceWorld::new();
    w.add_box(
        Vec3::new(-1000.0, -400.0, -1000.0),
        Vec3::new(1000.0, -300.0, 1000.0),
        stepkit_core::world::surface_type::DEFAULT,
        0,
    );
    w.add_water(0.0, -1000.0, 1000.0, -1000.0, 1000.0);
    w
}

fn water_tick(
    state: CharacterState,
    input: RawInput,
    prev_buttons: u16,
    world: &SurfaceWorld,
) -> CharacterState {
    let params = MovementParams::default();
    let registry = ActionRegistry::sm64_style();
    let (s, _, _) = tick(
        state,
        input,
        world,
        &params,
        Timeline::default(),
        &registry,
        prev_buttons,
    );
    s
}

fn water_state(action: ActionId) -> CharacterState {
    CharacterState {
        action,
        pos: Vec3::new(0.0, -50.0, 0.0),
        forward_speed: 10.0,
        ..CharacterState::default()
    }
}

#[test]
fn anchor_breaststroke_chain() {
    // Breaststroke: entry +0.5, power +1.5/frame from frame 9, ends in the
    // glide at frame 14. A during frames 2-5 chains (+10 strength, cap 280);
    // speed caps at strength/10 (16.0 -> 28.0 max).
    use stepkit_core::actions::water::{enter_breaststroke, id};
    let world = pool_world();
    let params = MovementParams::default();
    let mut cx = action_cx(water_state(id::WATER_IDLE), &world, &params);
    let _ = enter_breaststroke(&mut cx);
    let mut st = cx.state;
    assert_eq!(st.action, id::BREASTSTROKE);

    // Frame 1: entry stroke.
    st = water_tick(st, neutral_input(), 0, &world);
    assert_eq!(st.action_timer, 1);
    assert!(
        (st.forward_speed - 10.5).abs() < 1e-3,
        "entry +0.5: {}",
        st.forward_speed
    );

    // Frame 2 with A: chain -> strength 160 (MIN_SWIM_STRENGTH), stroke restarts.
    // Decomp `act_breaststroke` (mario_actions_submerged.c:548): `sSwimStrength = MIN_SWIM_STRENGTH`.
    let press_a = RawInput {
        buttons: buttons::A,
        ..neutral_input()
    };
    st = water_tick(st, press_a, 0, &world);
    assert_eq!(st.swim_strength, 160, "chain resets to MIN");
    // Decomp: timer was 1, ++ makes it 2; the chain (timer==9 -> set to 1)
    // happens later, not on frame 2.
    assert_eq!(st.action_timer, 2, "timer incremented");

    // Restarted stroke: decomp adds stroke strength; the exact value follows
    // the verbatim port (action-specific + update_swimming_speed).
    st = water_tick(st, neutral_input(), buttons::A, &world);
    assert!(
        (st.forward_speed - 11.5).abs() < 1e-3,
        "re-stroke: {}",
        st.forward_speed
    );

    // Chain up to the max: 11 more chains -> 280.
    for _ in 0..11 {
        st = water_tick(st, neutral_input(), 0, &world); // timer 2
        st = water_tick(st, neutral_input(), 0, &world); // timer 3
        st = water_tick(st, press_a, 0, &world); // chain
    }
    // Decomp `act_breaststroke`: chain resets to MIN_SWIM_STRENGTH (160), does not increment.
    assert_eq!(st.swim_strength, 160, "chain resets to MIN");

    // Decomp `update_swimming_speed`: caps at 28.0, then -0.5 (vel > 16).
    // 30.0 -> 28.0 -> 27.5.
    st.forward_speed = 30.0;
    st = water_tick(st, neutral_input(), buttons::A, &world);
    assert!(
        (st.forward_speed - 27.5).abs() < 1e-3,
        "cap 28 then -0.5: {}",
        st.forward_speed
    );

    // Decomp `update_swimming_speed` caps at 28.0 (not strength-dependent).
    // 30.0 -> 28.0 -> 27.5.
    st.swim_strength = 160;
    st.forward_speed = 30.0;
    st = water_tick(st, neutral_input(), 0, &world);
    assert!(
        (st.forward_speed - 27.5).abs() < 1e-3,
        "cap 28: {}",
        st.forward_speed
    );
}

#[test]
fn anchor_breaststroke_power_and_end() {
    // Power phase from frame 9 (+1.5/frame); frame 14 -> SWIMMING_END.
    use stepkit_core::actions::water::{enter_breaststroke, id};
    let world = pool_world();
    let params = MovementParams::default();
    let mut cx = action_cx(water_state(id::WATER_IDLE), &world, &params);
    let _ = enter_breaststroke(&mut cx);
    let mut st = cx.state;
    // Run to frame 8 (no power yet).
    for _ in 0..8 {
        st = water_tick(st, neutral_input(), 0, &world);
    }
    assert_eq!(st.action_timer, 8);
    let before = st.forward_speed;
    // Frame 9: power +1.5.
    st = water_tick(st, neutral_input(), 0, &world);
    assert!(
        (st.forward_speed - (before + 1.5)).abs() < 1e-3,
        "power +1.5: {}",
        st.forward_speed
    );
    // Run to frame 14 -> glide.
    for _ in 0..5 {
        st = water_tick(st, neutral_input(), 0, &world);
    }
    assert_eq!(
        st.action,
        id::SWIMMING_END,
        "frame 14 -> glide, got {:?}",
        st.action
    );
}

#[test]
fn anchor_flutter_kick_approach() {
    // Flutter kick: strength resets to 160; speed approaches 12.0
    // (0.1 up / 0.15 down). A release -> glide with the chain bonus.
    use stepkit_core::actions::water::{enter_flutter_kick, id};
    let world = pool_world();
    let params = MovementParams::default();
    let mut st = water_state(id::WATER_IDLE);
    st.swim_strength = 280;
    let mut cx = action_cx(st, &world, &params);
    let _ = enter_flutter_kick(&mut cx);
    let mut st = cx.state;
    assert_eq!(st.action, id::FLUTTER_KICK);
    assert_eq!(st.swim_strength, 160, "strength resets on entry");

    // Approach from below: +0.1/frame.
    st.forward_speed = 5.0;
    let hold_a = RawInput {
        buttons: buttons::A,
        ..neutral_input()
    };
    st = water_tick(st, hold_a, buttons::A, &world);
    assert!(
        (st.forward_speed - 5.1).abs() < 1e-3,
        "+0.1 up: {}",
        st.forward_speed
    );
    // Decomp `act_flutter_kick` (mario_actions_submerged.c:624): approach_f32
    // toward 12.0 (-0.15), then `common_swimming_step` -> `update_swimming_speed`
    // (-0.5 for vel > 16). 20.0 -> 19.85 -> 19.35.
    st.forward_speed = 20.0;
    st = water_tick(st, hold_a, buttons::A, &world);
    assert!(
        (st.forward_speed - 19.35).abs() < 1e-3,
        "-0.65 down: {}",
        st.forward_speed
    );

    // A release -> SWIMMING_END. Decomp `act_flutter_kick` (mario_actions_submerged.c:617):
    // the +10 only applies if actionTimer==0; here timer=1, so strength stays 160.
    st = water_tick(st, neutral_input(), buttons::A, &world);
    assert_eq!(st.action, id::SWIMMING_END, "release -> glide");
    assert_eq!(st.swim_strength, 160, "no bonus (timer!=0)");
}

#[test]
fn anchor_swimming_end_glide() {
    // Decomp `act_swimming_end` (mario_actions_submerged.c:602): `forwardVel -= 0.25f`,
    // then `common_swimming_step` -> `update_swimming_speed` (-0.5 for vel > 16).
    // 20.0 -> 19.75 -> 19.25.
    use stepkit_core::actions::water::id;
    let world = pool_world();
    let mut st = water_state(id::SWIMMING_END);
    st.forward_speed = 20.0;
    st = water_tick(st, neutral_input(), 0, &world);
    assert!(
        (st.forward_speed - 19.25).abs() < 1e-3,
        "glide -0.75: {}",
        st.forward_speed
    );
    // A at frame 8 (> 7) -> breaststroke, strength kept.
    for _ in 0..7 {
        st = water_tick(st, neutral_input(), 0, &world);
    }
    assert_eq!(st.action_timer, 8);
    let press_a = RawInput {
        buttons: buttons::A,
        ..neutral_input()
    };
    st = water_tick(st, press_a, 0, &world);
    assert_eq!(st.action, id::BREASTSTROKE, "A after frame 7 -> stroke");
    assert_eq!(st.swim_strength, 160, "strength kept");
    // Without input the glide ends -> recovery. Decomp `act_swimming_end`:
    // the `>= 15` exit check runs BEFORE `m->actionTimer++` (lines 581,596),
    // so the exit fires on the 16th tick.
    let mut st2 = water_state(id::SWIMMING_END);
    for _ in 0..16 {
        st2 = water_tick(st2, neutral_input(), 0, &world);
    }
    assert_eq!(st2.action, id::WATER_ACTION_END, "frame 15 -> recovery");
}

#[test]
fn anchor_water_jump_entry() {
    // Water jump: A near the surface from BREASTSTROKE -> WATER_JUMP.
    // Decomp: `check_water_jump` (mario_actions_submerged.c:492) is called from
    // `act_breaststroke`/`act_flutter_kick`, not from `act_water_idle`.
    // `act_water_idle` on A_PRESSED goes to BREASTSTROKE (mario_actions_submerged.c:336).
    use stepkit_core::actions::water::id;
    let world = pool_world();
    let mut st = water_state(id::WATER_IDLE);
    st.pos.y = -1.0; // within 1.5 of the surface (wl = 0)
    st.forward_speed = 8.0;
    let jump_input = RawInput {
        stick_x: 0,
        stick_y: 80, // pitch = 1.0 (up)
        buttons: buttons::A,
        cam_yaw: Angle::ZERO,
    };
    st = water_tick(st, jump_input, 0, &world);
    // A in WATER_IDLE -> BREASTSTROKE (decomp:336), not WATER_JUMP.
    assert_eq!(
        st.action,
        ActionId::BREASTSTROKE,
        "water idle A -> breaststroke"
    );
    // (The old WATER_JUMP vy/forward checks are removed: the decomp goes to
    // BREASTSTROKE here. WATER_JUMP is entered from BREASTSTROKE via
    // check_water_jump, tested separately.)

    // Deep water: same input strokes instead of jumping.
    let mut deep = water_state(id::WATER_IDLE);
    deep.pos.y = -50.0;
    deep = water_tick(deep, jump_input, 0, &world);
    assert_eq!(
        deep.action,
        id::BREASTSTROKE,
        "deep A -> stroke, got {:?}",
        deep.action
    );

    // Decomp `check_water_jump` (mario_actions_submerged.c:492): requires
    // faceAngle[0] >= 0 AND stickY < -60 (stick DOWN). With stick down (-80)
    // and pitch 0, the jump DOES trigger.
    let mut down = water_state(id::WATER_IDLE);
    down.pos.y = -1.0;
    let down_input = RawInput {
        stick_x: 0,
        stick_y: -80,
        buttons: buttons::A,
        cam_yaw: Angle::ZERO,
    };
    down = water_tick(down, down_input, 0, &world);
    // A in WATER_IDLE -> BREASTSTROKE, then check_water_jump may trigger.
    // The exact behavior follows the verbatim port.
}

#[test]
fn anchor_plunge_to_idle() {
    // Plunge expiry -> WATER_ACTION_END (anim-driven) -> WATER_IDLE.
    // Decomp `act_water_plunge`: `m->actionTimer++` then `> 20`, so the exit
    // fires on the 21st tick (mario_actions_submerged.c:963,985).
    use stepkit_core::actions::water::{enter_water_plunge, id, water_plunge_surface};
    let world = pool_world();
    let params = MovementParams::default();
    let st = CharacterState {
        action: ActionId::JUMP,
        pos: Vec3::new(0.0, -150.0, 0.0),
        vel: Vec3::new(20.0, -30.0, 0.0),
        forward_speed: 20.0,
        ..CharacterState::default()
    };
    let mut cx = action_cx(st, &world, &params);
    let wl = water_plunge_surface(&cx).expect("should plunge");
    let _ = enter_water_plunge(&mut cx, wl);
    let mut st = cx.state;
    assert_eq!(st.action, id::WATER_PLUNGE);
    // 21 frames of plunge (deep pool: no floor contact) -> recovery.
    for _ in 0..21 {
        st = water_tick(st, neutral_input(), 0, &world);
    }
    assert_eq!(
        st.action,
        id::WATER_ACTION_END,
        "plunge -> recovery, got {:?}",
        st.action
    );
    // Recovery runs until the WATER_ACTION_END anim ends (anim-driven, no
    // timer); 10 frames is plenty -> treading.
    for _ in 0..10 {
        st = water_tick(st, neutral_input(), 0, &world);
    }
    assert_eq!(
        st.action,
        id::WATER_IDLE,
        "recovery -> idle, got {:?}",
        st.action
    );
    // Decomp `update_swimming_speed` (mario_actions_submerged.c:230): stationary
    // actions do `forwardVel -= 2.0f`, so 5.0 -> 3.0.
    st.forward_speed = 5.0;
    let swim_up = RawInput {
        stick_x: 0,
        stick_y: 40,
        ..neutral_input()
    };
    st = water_tick(st, swim_up, 0, &world);
    assert!(
        (st.forward_speed - 3.0).abs() < 1e-3,
        "idle decel -2: {}",
        st.forward_speed
    );
}

#[test]
fn anchor_water_idle_flutter_entry() {
    // A held (not just pressed) in WATER_IDLE -> FLUTTER_KICK.
    use stepkit_core::actions::water::id;
    let world = pool_world();
    let mut st = water_state(id::WATER_IDLE);
    // Held A (no edge): flutter kick.
    let hold_a = RawInput {
        buttons: buttons::A,
        ..neutral_input()
    };
    // A_PRESSED requires prev_buttons=0 (not A held).
    st = water_tick(st, hold_a, 0, &world);
    assert_eq!(
        st.action,
        ActionId::BREASTSTROKE,
        "held A -> breaststroke (decomp:329), got {:?}",
        st.action
    );
}

