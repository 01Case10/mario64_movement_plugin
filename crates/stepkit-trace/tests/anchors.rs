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
        SurfaceKind::Default,
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
    // Entry clamp: first walking frame sets speed to min(mag, 8).
    assert!((speeds[1] - 8.0).abs() < 1e-4, "entry clamp: {}", speeds[1]);
    // Second frame: 8 + (1.1 - 8/43).
    let expect = 8.0 + (1.1 - 8.0 / 43.0);
    assert!(
        (speeds[2] - expect).abs() < 1e-3,
        "accel curve: {} vs {expect}",
        speeds[2]
    );
    // Approaches the 32 target, never exceeds the 48 hard cap.
    assert!((speeds[45] - 32.0).abs() < 0.6, "target: {}", speeds[45]);
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
    // Facing approaches the intended yaw at <= 0x800/frame.
    let w = load_anchors("walking_behavior.csv");
    let rate_units = u32::from_str_radix(w["walk_turn_rate"].trim_start_matches("0x"), 16).unwrap();
    assert_eq!(MovementParams::default().walk_turn_rate as u32, rate_units);
    let world = flat_world();
    let registry = stepkit_core::actions::ActionRegistry::sm64_style();
    let params = MovementParams::default();
    let mut state = CharacterState::default();
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
    assert!((out.vel.y - 50.0).abs() < 1e-3, "vy {}", out.vel.y);
    assert!(
        (out.forward_speed - 25.6).abs() < 1e-3,
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
    state: CharacterState,
    world: &'a SurfaceWorld,
    params: &'a MovementParams,
) -> ActionCx<'a> {
    ActionCx {
        state,
        input: RawInput {
            stick_x: 0,
            stick_y: 0,
            buttons: 0,
            cam_yaw: Angle::ZERO,
        },
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
    enter_side_flip(&mut cx);
    assert!(
        (cx.state.vel.y - 62.0).abs() < 1e-3,
        "vy {}",
        cx.state.vel.y
    );
    assert!((cx.state.forward_speed - 8.0).abs() < 1e-3);
    // Intended yaw for stick (80,0) with cam 0: atan2(-80, 0) = -90 deg.
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
    st.wall_normal = Vec3::new(0.0, 0.0, -1.0); // wall at +z, normal toward -z
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
    st2.wall_normal = Vec3::new(0.0, 0.0, -1.0);
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
        SurfaceKind::Default,
        0,
    );
    w.add_box(
        Vec3::new(-200.0, 0.0, 400.0),
        Vec3::new(200.0, 400.0, 500.0),
        SurfaceKind::Default,
        0,
    );
    let mario = Vec3::new(0.0, 280.0, 350.0); // against the wall, falling zone
                                              // Wall 30 above (y=310 < 400): yes. Wall 150 above (y=430 > 400): no.
    assert!(w.wall_probe(mario + Vec3::new(0.0, 30.0, 0.0), 51.0));
    assert!(!w.wall_probe(mario + Vec3::new(0.0, 150.0, 0.0), 51.0));
    // Top-down floor search from 60 into the wall (+z) and 160 up finds the top.
    let search = mario + Vec3::new(0.0, 160.0, 60.0);
    let f = w.find_floor(search, 0.0).expect("ledge floor");
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
    st.wall_normal = Vec3::new(0.0, 0.0, -1.0);
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
    fn hit_state(impact: u32) -> CharacterState {
        let mut st = air_state(ActionId::AIR_HIT_WALL, 20.0);
        st.action_arg = impact;
        st.wall_normal = Vec3::new(0.0, 0.0, -1.0);
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
    // A on the first window frame -> wall kick flight at vy 52.
    let (s, _) = one_tick(hit_state(40), press_a, 0);
    assert_eq!(s.action, ActionId::WALL_KICK_AIR, "A in window -> kick");
    assert!((s.vel.y - 52.0).abs() < 1e-3, "kick vy {}", s.vel.y);
    // No A: after 2 window frames the impact-40 hit resolves to knockback.
    let (s, _) = one_tick(hit_state(40), neutral, 0);
    assert_eq!(s.action, ActionId::AIR_HIT_WALL);
    let (s, _) = one_tick(s, neutral, 0);
    assert_eq!(s.action, ActionId::AIR_HIT_WALL);
    let (s, _) = one_tick(s, neutral, 0);
    assert_eq!(
        s.action,
        ActionId::BACKWARD_AIR_KB,
        "impact>=38 -> knockback"
    );
    assert_eq!(s.wall_kick_timer, 5, "late-kick allowance");
    // Impact 20 resolves to the soft bonk instead.
    let (s, _) = one_tick(hit_state(20), neutral, 0);
    let (s, _) = one_tick(s, neutral, 0);
    let (s, _) = one_tick(s, neutral, 0);
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

/// Fake world reporting a very steep floor (normal.y = 0.25 < 0.2924).
struct SteepFloorWorld;
impl stepkit_core::world::CollisionWorld for SteepFloorWorld {
    fn find_floor(&self, _pos: Vec3, _max_above: f32) -> Option<stepkit_core::world::FloorHit> {
        Some(stepkit_core::world::FloorHit {
            y: 0.0,
            normal: Vec3::new(0.0, 0.25, 0.9682458),
            kind: stepkit_core::world::SurfaceKind::Default,
            user_data: 0,
        })
    }
    fn find_ceiling(&self, _pos: Vec3, _height: f32) -> Option<stepkit_core::world::CeilingHit> {
        None
    }
    fn resolve_walls(
        &self,
        pos: Vec3,
        _offset: f32,
        _radius: f32,
    ) -> stepkit_core::world::WallResolve {
        stepkit_core::world::WallResolve {
            pos,
            hit: false,
            num_walls: 0,
            normal: Vec3::ZERO,
        }
    }
    fn water_level(&self, _x: f32, _z: f32) -> Option<f32> {
        None
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
    // Low ceiling: entry skips the rise.
    let mut low = flat_world();
    low.add_box(
        Vec3::new(-2000.0, 250.0, -2000.0),
        Vec3::new(2000.0, 350.0, 2000.0),
        SurfaceKind::Default,
        0,
    );
    let mut cx2 = action_cx(air_state(ActionId::JUMP, 0.0), &low, &params);
    let _ = enter_ground_pound(&mut cx2);
    assert_eq!(cx2.state.action_state, 1, "low ceiling skips the rise");
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
    // Air wall hit at speed <= 16: no bonk, no action change, no dampening;
    // the into-wall component is removed so the character slides along.
    let mut world = flat_world();
    world.add_box(
        Vec3::new(-200.0, 0.0, 400.0),
        Vec3::new(200.0, 600.0, 500.0),
        SurfaceKind::Default,
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
        (ns.vel.x - 10.0).abs() < 1.0,
        "tangential velocity kept (not x0.8): {}",
        ns.vel.x
    );
    assert!(
        ns.vel.z.abs() < 1.0,
        "into-wall component removed: {}",
        ns.vel.z
    );
}

#[test]
fn anchor_knockback_ids() {
    // Phase C knockback family IDs (decomp action index).
    assert_eq!(ActionId::BACKWARD_AIR_KB.0, 0x010208B0);
    assert_eq!(ActionId::FORWARD_AIR_KB.0, 0x010208B1);
    assert_eq!(ActionId::HARD_BACKWARD_AIR_KB.0, 0x010208B2);
    assert_eq!(ActionId::HARD_FORWARD_AIR_KB.0, 0x010208B3);
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
