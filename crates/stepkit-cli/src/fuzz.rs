//! `stepkit fuzz`: random-input differential fuzzer (plan §7.4).
//!
//! Generates random input sequences over a set of parametric geometries and
//! runs each through the free-mode oracle diff (`parity_diff::diff_free`).
//! Any divergence is automatically minimised (input-prefix bisection +
//! greedy segment removal) and saved as a `.ron` scenario under the output
//! directory so it can be replayed with `stepkit oracle diff --scenario`.
//!
//! Seed discipline (plan §7.4):
//! * CI: a fixed seed set (`--seeds`), deterministic.
//! * Nightly: `--random` picks seeds from system entropy and prints them so a
//!   failing nightly run can be reproduced with `--seeds`.
//!
//! Exit code: 0 iff every case passes, 1 if any divergence (or error) occurs.

use std::path::{Path, PathBuf};

use stepkit_trace::scenario::{
    BoxGeom, Camera, CompareProfile, Geometry, InputSeg, RampGeom, Scenario, StartState, WaterGeom,
};

use crate::parity_diff;

/// xorshift64*: tiny deterministic RNG, no extra dependencies. The same seed
/// always yields the same input stream on every platform.
struct XorShift64(u64);

impl XorShift64 {
    fn new(seed: u64) -> Self {
        // Zero is a fixed point of xorshift; mix it away.
        Self(if seed == 0 { 0x9E3779B97F4A7C15 } else { seed })
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    /// Uniform in `[0, n)`.
    fn below(&mut self, n: u64) -> u64 {
        debug_assert!(n > 0);
        self.next_u64() % n
    }

    /// Uniform in `[lo, hi]` (inclusive).
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next_u64() % (hi - lo + 1) as u64) as i64
    }

    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len() as u64) as usize]
    }

    /// Bernoulli trial with probability `p` in [0,1].
    fn coin(&mut self, p: f64) -> bool {
        (self.next_u64() as f64 / u64::MAX as f64) < p
    }
}

/// One fuzz geometry preset: everything a [`Scenario`] needs except inputs.
struct GeomPreset {
    name: &'static str,
    tags: &'static [&'static str],
    geometry: Vec<Geometry>,
    water: Vec<WaterGeom>,
    start_pos: (f32, f32, f32),
    start_yaw_deg: f32,
    start_action: &'static str,
    cam_yaw_deg: f32,
    terrain: u8,
    compare: CompareProfile,
}

fn bx(min: (i32, i32, i32), max: (i32, i32, i32), kind: u8) -> Geometry {
    Geometry::Box(BoxGeom {
        min: (min.0 as f32, min.1 as f32, min.2 as f32),
        max: (max.0 as f32, max.1 as f32, max.2 as f32),
        kind,
    })
}

fn floor_box(kind: u8) -> Geometry {
    bx((-4000, -100, -4000), (4000, 0, 4000), kind)
}

/// All fuzz geometries (plan §7.4: flat, slopes of each surface class,
/// steps, ledges, pits, low/high ceilings, corners/walls, slide terrain,
/// quicksand, water). Coordinates are integers inside ±8191 (ST-02).
fn geom_presets() -> Vec<GeomPreset> {
    let mut v = Vec::new();
    let mut flat = |name: &'static str, kind: u8, terrain: u8| {
        v.push(GeomPreset {
            name,
            tags: &["fuzz", "ground"],
            geometry: vec![floor_box(kind)],
            water: vec![],
            start_pos: (0.0, 0.0, 0.0),
            start_yaw_deg: 0.0,
            start_action: "Idle",
            cam_yaw_deg: 0.0,
            terrain,
            compare: CompareProfile::GroundDefault,
        });
    };
    flat("flat_default", 0, 0);
    flat("flat_slippery", 1, 0);
    flat("flat_very_slippery", 3, 0);
    flat("flat_not_slippery", 2, 0);
    flat("flat_slow", 7, 0);
    flat("slide_terrain", 0, 6); // TERRAIN_SLIDE
    flat("quicksand_shallow", 4, 0);
    flat("quicksand_deep", 5, 0);

    // Ramps: (length, height) pairs for ~15/30/45 degrees.
    for (name, length, height, kind) in [
        ("ramp_15", 2000, 536, 0),
        ("ramp_30", 2000, 1155, 0),
        ("ramp_45", 1500, 1500, 0),
        ("ramp_30_slippery", 2000, 1155, 1),
        ("ramp_30_very_slippery", 2000, 1155, 3),
    ] {
        v.push(GeomPreset {
            name,
            tags: &["fuzz", "ground", "slope"],
            geometry: vec![
                floor_box(0),
                Geometry::Ramp(RampGeom {
                    origin: (-1500.0, 0.0, -250.0),
                    dir: (1.0, 0.0),
                    length: length as f32,
                    height: height as f32,
                    width: 500.0,
                    kind,
                }),
            ],
            water: vec![],
            start_pos: (-1400.0, 50.0, -250.0),
            start_yaw_deg: 0.0,
            start_action: "Idle",
            cam_yaw_deg: 0.0,
            terrain: 0,
            compare: CompareProfile::GroundDefault,
        });
    }

    // Steps: five 40-unit rises.
    {
        let mut g = vec![floor_box(0)];
        for i in 0..5 {
            let x0 = i * 200;
            g.push(bx((x0, -100, -4000), (x0 + 200, 40 * (i + 1), 4000), 0));
        }
        v.push(GeomPreset {
            name: "steps_40",
            tags: &["fuzz", "ground", "steps"],
            geometry: g,
            water: vec![],
            start_pos: (-600.0, 0.0, 0.0),
            start_yaw_deg: 0.0,
            start_action: "Walking",
            cam_yaw_deg: 0.0,
            terrain: 0,
            compare: CompareProfile::GroundDefault,
        });
    }

    // Inside corner (two walls) and a corridor.
    v.push(GeomPreset {
        name: "walls_corner",
        tags: &["fuzz", "ground", "wall"],
        geometry: vec![
            floor_box(0),
            bx((500, -100, -2000), (600, 400, 2000), 0),
            bx((500, -100, 500), (2000, 400, 600), 0),
        ],
        water: vec![],
        start_pos: (0.0, 0.0, 0.0),
        start_yaw_deg: 0.0,
        start_action: "Walking",
        cam_yaw_deg: 0.0,
        terrain: 0,
        compare: CompareProfile::GroundDefault,
    });
    v.push(GeomPreset {
        name: "walls_corridor",
        tags: &["fuzz", "ground", "wall"],
        geometry: vec![
            floor_box(0),
            bx((-2000, -100, 300), (2000, 400, 400), 0),
            bx((-2000, -100, -400), (2000, 400, -300), 0),
        ],
        water: vec![],
        start_pos: (0.0, 0.0, 0.0),
        start_yaw_deg: 0.0,
        start_action: "Walking",
        cam_yaw_deg: 0.0,
        terrain: 0,
        compare: CompareProfile::GroundDefault,
    });

    // Low / high ceilings over a flat floor.
    for (name, y0, y1) in [("ceiling_low", 150, 250), ("ceiling_mid", 400, 500)] {
        v.push(GeomPreset {
            name,
            tags: &["fuzz", "air", "ceiling"],
            geometry: vec![floor_box(0), bx((-2000, y0, -2000), (2000, y1, 2000), 0)],
            water: vec![],
            start_pos: (0.0, 0.0, 0.0),
            start_yaw_deg: 0.0,
            start_action: "Idle",
            cam_yaw_deg: 0.0,
            terrain: 0,
            compare: CompareProfile::AirDefault,
        });
    }

    // Ledge: 300-unit platform; Mario starts below the wall face.
    v.push(GeomPreset {
        name: "ledge_300",
        tags: &["fuzz", "air", "ledge"],
        geometry: vec![
            bx((-4000, -100, -4000), (4000, 0, 0), 0),
            bx((-2000, 0, 0), (2000, 300, 2000), 0),
        ],
        water: vec![],
        start_pos: (0.0, 0.0, -600.0),
        start_yaw_deg: 180.0,
        start_action: "Idle",
        cam_yaw_deg: 0.0,
        terrain: 0,
        compare: CompareProfile::AirDefault,
    });

    // Pit: 400-wide gap with a floor 500 below.
    v.push(GeomPreset {
        name: "pit",
        tags: &["fuzz", "air", "pit"],
        geometry: vec![
            bx((-4000, -100, -4000), (-200, 0, 4000), 0),
            bx((200, -100, -4000), (4000, 0, 4000), 0),
            bx((-200, -600, -4000), (200, -500, 4000), 0),
        ],
        water: vec![],
        start_pos: (-600.0, 0.0, 0.0),
        start_yaw_deg: 0.0,
        start_action: "Walking",
        cam_yaw_deg: 0.0,
        terrain: 0,
        compare: CompareProfile::AirDefault,
    });

    // Water pool (mirrors swim.ron at a smaller scale).
    v.push(GeomPreset {
        name: "water_pool",
        tags: &["fuzz", "water"],
        geometry: vec![
            bx((-2000, -100, -2000), (2000, 0, 200), 0),
            bx((-2000, -100, 1200), (2000, 0, 2000), 0),
            bx((-2000, -100, 200), (-300, 0, 1200), 0),
            bx((300, -100, 200), (2000, 0, 1200), 0),
            bx((-300, -300, 200), (300, -200, 1200), 0),
        ],
        water: vec![WaterGeom {
            y: 0.0,
            min_x: -300.0,
            max_x: 300.0,
            min_z: 200.0,
            max_z: 1200.0,
        }],
        start_pos: (0.0, 0.0, -400.0),
        start_yaw_deg: 0.0,
        start_action: "Idle",
        cam_yaw_deg: 0.0,
        terrain: 0,
        compare: CompareProfile::AirDefault,
    });

    v
}

/// Generate one frame's input: stick (with deadzone-edge and extreme values
/// per §7.4), button mask with realistic press/hold frequencies.
fn gen_stick_axis(rng: &mut XorShift64) -> i8 {
    let r = rng.below(100);
    if r < 35 {
        0
    } else if r < 50 {
        // Edge values: deadzone boundary (±7/±8) and extremes.
        *rng.pick(&[-127i8, -8, -7, 7, 8, 127])
    } else if r < 70 {
        // Typical play magnitudes.
        *rng.pick(&[-80i8, -64, -40, -16, 16, 40, 64, 80])
    } else {
        rng.range(-127, 127) as i8
    }
}

/// Button bits: A=0x1, B=0x2, Z=0x4. Each frame may toggle a button, which
/// produces a mix of taps, holds and releases.
fn gen_buttons(rng: &mut XorShift64, held: &mut u16) -> u16 {
    for (bit, p) in [(0x1u16, 0.06), (0x2u16, 0.03), (0x4u16, 0.03)] {
        if rng.coin(p) {
            *held ^= bit;
        }
    }
    *held
}

fn gen_inputs(rng: &mut XorShift64, frames: usize) -> Vec<(i8, i8, u16)> {
    let mut out = Vec::with_capacity(frames);
    let mut held: u16 = 0;
    for _ in 0..frames {
        let sx = gen_stick_axis(rng);
        let sy = gen_stick_axis(rng);
        out.push((sx, sy, gen_buttons(rng, &mut held)));
    }
    out
}

/// Run-length encode per-frame inputs into [`InputSeg`]s.
fn encode_inputs(frames: &[(i8, i8, u16)]) -> Vec<InputSeg> {
    let mut segs = Vec::new();
    let mut i = 0;
    while i < frames.len() {
        let (sx, sy, b) = frames[i];
        let mut n = 1u32;
        while i + (n as usize) < frames.len() && frames[i + n as usize] == (sx, sy, b) {
            n += 1;
        }
        if n == 1 {
            segs.push(InputSeg::Press {
                stick: (sx, sy),
                buttons: b,
            });
        } else {
            segs.push(InputSeg::Hold {
                frames: n,
                stick: (sx, sy),
                buttons: b,
            });
        }
        i += n as usize;
    }
    segs
}

fn build_scenario(
    seed: u64,
    geom: &GeomPreset,
    cam_yaw_deg: f32,
    inputs: Vec<(i8, i8, u16)>,
) -> Scenario {
    Scenario {
        id: format!("fuzz-{}-seed{seed}", geom.name),
        tags: geom
            .tags
            .iter()
            .map(|t| t.to_string())
            .chain(std::iter::once("fuzz".to_string()))
            .collect(),
        geometry: geom.geometry.clone(),
        water: geom.water.clone(),
        start: StartState {
            pos: geom.start_pos,
            yaw_deg: geom.start_yaw_deg,
            action: geom.start_action.to_string(),
        },
        camera: Camera::Fixed {
            yaw_deg: cam_yaw_deg,
        },
        inputs: encode_inputs(&inputs),
        compare: geom.compare.clone(),
        terrain: geom.terrain,
        asserts: vec![],
    }
}

fn write_scenario(scenario: &Scenario, dir: &Path) -> Result<PathBuf, String> {
    let cfg = ron::ser::PrettyConfig::new();
    let text =
        ron::ser::to_string_pretty(scenario, cfg).map_err(|e| format!("ron serialize: {e}"))?;
    let path = dir.join(format!("{}.ron", scenario.id));
    std::fs::write(&path, text).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}

/// Run one fuzz case: build the scenario, diff it, and minimise on failure.
/// Returns the minimised repro on divergence.
fn fuzz_case(
    seed: u64,
    geom: &GeomPreset,
    frames: usize,
    cam_yaw_deg: f32,
    minimize: bool,
    tmp_dir: &Path,
    out_dir: &Path,
) -> Result<Option<Minimised>, String> {
    let mut rng = XorShift64::new(seed ^ 0xF00DCAFE);
    let inputs = gen_inputs(&mut rng, frames);
    let scenario = build_scenario(seed, geom, cam_yaw_deg, inputs.clone());
    let scenario_path = write_scenario(&scenario, tmp_dir)?;
    let outcome = parity_diff::diff_free(&scenario, &scenario_path, parity_diff::COMPARED_FIELDS);
    let _ = std::fs::remove_file(&scenario_path);
    let rep = match outcome {
        Ok(None) => return Ok(None),
        Ok(Some(rep)) => rep,
        Err(e) => return Err(e),
    };
    let anchor = FirstDivergence {
        frame: rep.frame,
        field: rep.field,
        plugin: rep.plugin,
        oracle: rep.oracle,
    };
    if !minimize {
        return Ok(Some(Minimised {
            frame: anchor.frame,
            field: anchor.field,
            plugin: anchor.plugin,
            oracle: anchor.oracle,
            path: PathBuf::from("(minimisation disabled)"),
        }));
    }
    minimize_divergence(seed, geom, cam_yaw_deg, &inputs, &anchor, tmp_dir, out_dir)
}

struct Minimised {
    frame: u32,
    field: String,
    plugin: String,
    oracle: String,
    path: PathBuf,
}

/// The first divergence of an input sequence: the anchor that minimisation
/// must preserve. (`plugin`/`oracle` values are informational; the anchor is
/// the (frame, field) pair.)
struct FirstDivergence {
    frame: u32,
    field: String,
    plugin: String,
    oracle: String,
}

/// Compute the first divergence of an input sequence, if any. Oracle errors
/// are returned as `Err`; callers treat them conservatively.
fn first_divergence(
    seed: u64,
    geom: &GeomPreset,
    cam_yaw_deg: f32,
    frames: &[(i8, i8, u16)],
    tmp_dir: &Path,
) -> Result<Option<FirstDivergence>, String> {
    let scenario = build_scenario(seed, geom, cam_yaw_deg, frames.to_vec());
    let path = write_scenario(&scenario, tmp_dir)?;
    let r = parity_diff::diff_free(&scenario, &path, parity_diff::COMPARED_FIELDS);
    let _ = std::fs::remove_file(&path);
    match r {
        Ok(None) => Ok(None),
        Ok(Some(rep)) => Ok(Some(FirstDivergence {
            frame: rep.frame,
            field: rep.field,
            plugin: rep.plugin,
            oracle: rep.oracle,
        })),
        Err(e) => Err(e),
    }
}

/// Predicate: does this input sequence still exhibit the anchored
/// divergence? Only the (frame, field) pair is compared: the tail
/// minimisation never touches frames `<= anchor.frame`, so a preserved
/// anchor is the *same* divergence, not merely another one.
fn keeps_anchor(
    anchor: &FirstDivergence,
    seed: u64,
    geom: &GeomPreset,
    cam_yaw_deg: f32,
    frames: &[(i8, i8, u16)],
    tmp_dir: &Path,
) -> bool {
    matches!(
        first_divergence(seed, geom, cam_yaw_deg, frames, tmp_dir),
        Ok(Some(d)) if d.frame == anchor.frame && d.field == anchor.field
    )
}

/// Delta-debug the failing input while preserving the anchored
/// (frame, field) divergence:
/// 1. bisect the tail: smallest prefix length whose first divergence is
///    still the anchor (frames `<= anchor.frame` are never removed);
/// 2. greedily drop run-length segments lying entirely *after* the anchor
///    frame (removing them cannot affect frames `<= anchor.frame`).
/// The minimised input is saved as a replayable scenario. Head segments
/// (frames before the anchor) are deliberately NOT removed: dropping them
/// shifts the input stream and can surface a different divergence, which
/// would mislabel the repro.
fn minimize_divergence(
    seed: u64,
    geom: &GeomPreset,
    cam_yaw_deg: f32,
    inputs: &[(i8, i8, u16)],
    anchor: &FirstDivergence,
    tmp_dir: &Path,
    out_dir: &Path,
) -> Result<Option<Minimised>, String> {
    // 1. Smallest prefix that keeps the anchor. The predicate is monotone:
    // for L >= anchor.frame the first `anchor.frame` frames are identical
    // to the full run, so the first divergence is still the anchor; for
    // L < anchor.frame it cannot be.
    let mut lo = anchor.frame as usize;
    let mut hi = inputs.len();
    if !keeps_anchor(anchor, seed, geom, cam_yaw_deg, &inputs[..hi], tmp_dir) {
        return Err("minimisation lost the divergence before starting".into());
    }
    while lo < hi {
        let mid = (lo + hi) / 2;
        if keeps_anchor(anchor, seed, geom, cam_yaw_deg, &inputs[..mid], tmp_dir) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    let mut frames: Vec<(i8, i8, u16)> = inputs[..lo].to_vec();

    // 2. Greedy removal of segments entirely after the anchor frame.
    for _ in 0..2 {
        let segs = encode_inputs(&frames);
        // Frame offset of each segment's first frame (1-based, matching
        // diff frame numbers: input[i] produces plugin frame i+1).
        let mut offsets = Vec::with_capacity(segs.len());
        let mut acc = 1u32;
        for s in &segs {
            offsets.push(acc);
            acc += match s {
                InputSeg::Hold { frames: n, .. } => *n,
                InputSeg::Press { .. } => 1,
            };
        }
        let mut removed_any = false;
        let mut i = 0;
        while i < segs.len() {
            // Only segments starting after the anchor frame are eligible.
            if offsets[i] <= anchor.frame {
                i += 1;
                continue;
            }
            let mut trial_segs = segs.clone();
            trial_segs.remove(i);
            let trial_frames: Vec<(i8, i8, u16)> = expand_segs(&trial_segs);
            if trial_frames.len() < anchor.frame as usize {
                i += 1;
                continue;
            }
            if keeps_anchor(anchor, seed, geom, cam_yaw_deg, &trial_frames, tmp_dir) {
                frames = trial_frames;
                removed_any = true;
                break; // recompute offsets; indices shifted
            }
            i += 1;
        }
        if !removed_any {
            break;
        }
    }

    let scenario = build_scenario(seed, geom, cam_yaw_deg, frames);
    let final_div = first_divergence(seed, geom, cam_yaw_deg, &scenario.expand_inputs(), tmp_dir)?
        .ok_or("minimisation lost the divergence")?;
    if final_div.frame != anchor.frame || final_div.field != anchor.field {
        return Err("minimisation changed the divergence".into());
    }
    let out_path = out_dir.join(format!("{}.ron", scenario.id));
    let cfg = ron::ser::PrettyConfig::new();
    let text =
        ron::ser::to_string_pretty(&scenario, cfg).map_err(|e| format!("ron serialize: {e}"))?;
    std::fs::create_dir_all(out_dir).map_err(|e| format!("mkdir {}: {e}", out_dir.display()))?;
    std::fs::write(&out_path, text).map_err(|e| format!("write {}: {e}", out_path.display()))?;
    Ok(Some(Minimised {
        frame: final_div.frame,
        field: final_div.field,
        plugin: final_div.plugin,
        oracle: final_div.oracle,
        path: out_path,
    }))
}

/// Expand run-length segments back to per-frame inputs.
fn expand_segs(segs: &[InputSeg]) -> Vec<(i8, i8, u16)> {
    let mut out = Vec::new();
    for s in segs {
        match s {
            InputSeg::Hold {
                frames: n,
                stick,
                buttons,
            } => out.extend(std::iter::repeat((stick.0, stick.1, *buttons)).take(*n as usize)),
            InputSeg::Press { stick, buttons } => out.push((stick.0, stick.1, *buttons)),
        }
    }
    out
}

/// Camera yaw choices per case: fixed yaws exercise the stick→intendedYaw
/// mapping at several rotations. (The scenario format only supports a fixed
/// camera, so slow rotation is not representable here.)
const CAM_YAWS: [f32; 4] = [0.0, 90.0, 180.0, 270.0];

/// `stepkit fuzz` entry point.
pub fn fuzz(
    seeds: Option<&str>,
    seed_base: u64,
    seed_count: u32,
    random: bool,
    frames: u32,
    geoms: Option<&str>,
    no_minimize: bool,
    list_geoms: bool,
    out_dir: &str,
    quiet: bool,
) -> Result<bool, String> {
    let all_geoms = geom_presets();
    if list_geoms {
        for g in &all_geoms {
            println!("{}", g.name);
        }
        return Ok(true);
    }
    let geoms: Vec<&GeomPreset> = match geoms {
        Some(spec) => {
            let want: Vec<&str> = spec.split(',').map(str::trim).collect();
            let mut v = Vec::new();
            for w in want {
                match all_geoms.iter().find(|g| g.name == w) {
                    Some(g) => v.push(g),
                    None => return Err(format!("unknown geometry {w:?} (see --list-geoms)")),
                }
            }
            v
        }
        None => all_geoms.iter().collect(),
    };
    if geoms.is_empty() {
        return Err("no geometries selected".into());
    }

    let seeds: Vec<u64> = if let Some(list) = seeds {
        list.split(',')
            .map(|s| {
                s.trim()
                    .parse::<u64>()
                    .map_err(|_| format!("bad seed {s:?}"))
            })
            .collect::<Result<_, _>>()?
    } else if random {
        // Nightly mode: fresh entropy, printed for reproducibility.
        let mut buf = [0u8; 8];
        getrandom_fill(&mut buf)?;
        let base = u64::from_le_bytes(buf);
        println!("# nightly seeds: base={base}");
        (0..seed_count.max(1))
            .map(|i| base.wrapping_add(i as u64))
            .collect()
    } else {
        (0..seed_count.max(1))
            .map(|i| seed_base.wrapping_add(i as u64))
            .collect()
    };
    if seeds.is_empty() {
        return Err("empty seed set".into());
    }

    let out_dir = PathBuf::from(out_dir);
    let tmp_dir = out_dir.join("tmp");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("mkdir {}: {e}", tmp_dir.display()))?;

    let minimize = !no_minimize;
    let mut failed = 0usize;
    let mut errored = 0usize;
    for (case_idx, seed) in seeds.iter().enumerate() {
        let geom = geoms[case_idx % geoms.len()];
        // Vary the camera yaw across cases deterministically.
        let cam_yaw_deg = CAM_YAWS[case_idx % CAM_YAWS.len()];
        match fuzz_case(
            *seed,
            geom,
            frames as usize,
            cam_yaw_deg,
            minimize,
            &tmp_dir,
            &out_dir,
        ) {
            Ok(None) => {
                if !quiet {
                    println!("seed {seed:>6} {:<24} PASS", geom.name);
                }
            }
            Ok(Some(min)) => {
                failed += 1;
                println!(
                    "seed {seed:>6} {:<24} FAIL frame {} field {} (plugin {} vs oracle {})",
                    geom.name, min.frame, min.field, min.plugin, min.oracle
                );
                println!("           minimized: {}", min.path.display());
            }
            Err(e) => {
                errored += 1;
                println!("seed {seed:>6} {:<24} ERROR {e}", geom.name);
            }
        }
    }

    println!();
    println!(
        "fuzz: {} cases, {} divergences, {} errors ({} frames each, {} geometries)",
        seeds.len(),
        failed,
        errored,
        frames,
        geoms.len()
    );
    Ok(failed == 0 && errored == 0)
}

/// Fill 8 bytes from OS entropy without adding a dependency.
fn getrandom_fill(buf: &mut [u8; 8]) -> Result<(), String> {
    let mut f =
        std::fs::File::open("/dev/urandom").map_err(|e| format!("open /dev/urandom: {e}"))?;
    use std::io::Read as _;
    f.read_exact(buf)
        .map_err(|e| format!("read /dev/urandom: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic() {
        let mut a = XorShift64::new(42);
        let mut b = XorShift64::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        let mut c = XorShift64::new(43);
        assert_ne!(a.next_u64(), c.next_u64());
    }

    #[test]
    fn encode_round_trips() {
        let frames = vec![(0i8, 0i8, 0u16), (0, 0, 0), (80, 0, 1), (0, 0, 0)];
        let segs = encode_inputs(&frames);
        assert_eq!(segs.len(), 3);
        assert!(matches!(segs[0], InputSeg::Hold { frames: 2, .. }));
        assert!(matches!(segs[1], InputSeg::Press { .. }));
    }

    #[test]
    fn stick_edge_values_appear() {
        let mut rng = XorShift64::new(7);
        let mut seen_edge = false;
        for _ in 0..2000 {
            let v = gen_stick_axis(&mut rng);
            if v == -127 || v == 127 || v == 7 || v == -8 {
                seen_edge = true;
                break;
            }
        }
        assert!(seen_edge, "edge stick values should appear");
    }

    #[test]
    fn geoms_validate() {
        for g in geom_presets() {
            let s = build_scenario(1, &g, 0.0, vec![(0, 0, 0); 10]);
            s.validate_geometry()
                .unwrap_or_else(|e| panic!("geom {} invalid: {e}", g.name));
        }
    }
}
