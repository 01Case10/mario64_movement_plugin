//! Collision: the core talks to the world through [`CollisionWorld`], so the
//! same code runs against test geometry and a live Godot scene.

use glam::Vec3;

/// Decomp `SURFACE_*` surface types (`oracle/sm64/include/surface_terrains.h`).
/// Floors carry one of these; `mario_get_floor_class` maps them to slipperiness.
pub mod surface_type {
    /// Environment default.
    pub const DEFAULT: u16 = 0x0000;
    /// Lava / frostbite.
    pub const BURNING: u16 = 0x0001;
    /// Intangible: separates rooms; the floor query re-searches below it.
    pub const INTANGIBLE: u16 = 0x0012;
    /// Ceiling that Mario can climb on.
    pub const HANGABLE: u16 = 0x0005;
    /// Slow down Mario, unused.
    pub const SLOW: u16 = 0x0009;
    /// Very slippery, mostly used for slides.
    pub const VERY_SLIPPERY: u16 = 0x0013;
    /// Slippery.
    pub const SLIPPERY: u16 = 0x0014;
    /// Non-slippery, climbable.
    pub const NOT_SLIPPERY: u16 = 0x0015;
    /// Shallow quicksand (depth of 10 units).
    pub const SHALLOW_QUICKSAND: u16 = 0x0021;
    /// Quicksand (lethal, slow, depth of 160 units).
    pub const DEEP_QUICKSAND: u16 = 0x0022;
    /// Quicksand (lethal, instant).
    pub const INSTANT_QUICKSAND: u16 = 0x0023;
    /// Moving quicksand (flowing, depth of 160 units).
    pub const DEEP_MOVING_QUICKSAND: u16 = 0x0024;
    /// Moving quicksand (flowing, depth of 25 units).
    pub const SHALLOW_MOVING_QUICKSAND: u16 = 0x0025;
    /// Moving quicksand (60 units).
    pub const QUICKSAND: u16 = 0x0026;
    /// Moving quicksand (flowing, depth of 60 units).
    pub const MOVING_QUICKSAND: u16 = 0x0027;
    /// Horizontal wind, has parameters.
    pub const HORIZONTAL_WIND: u16 = 0x002C;
    /// Quicksand (lethal, flowing).
    pub const INSTANT_MOVING_QUICKSAND: u16 = 0x002D;
    /// Slippery ice, in snow levels and THI's water floor.
    pub const ICE: u16 = 0x002E;
    /// Vertical wind.
    pub const VERTICAL_WIND: u16 = 0x0038;
    // ---- types referenced by mario_get_floor_class (decomp values) ----
    /// Hard and slippery (always has fall damage).
    pub const HARD_SLIPPERY: u16 = 0x0035;
    /// Hard and very slippery (always has fall damage).
    pub const HARD_VERY_SLIPPERY: u16 = 0x0036;
    /// Hard and non-slippery (always has fall damage).
    pub const HARD_NOT_SLIPPERY: u16 = 0x0037;
    /// Slippery floor with noise.
    pub const NOISE_SLIPPERY: u16 = 0x002A;
    /// Very slippery floor with noise, unused.
    pub const NOISE_VERY_SLIPPERY_73: u16 = 0x0073;
    /// Very slippery floor with noise, unused.
    pub const NOISE_VERY_SLIPPERY_74: u16 = 0x0074;
    /// Very slippery floor with noise, used in CCM.
    pub const NOISE_VERY_SLIPPERY: u16 = 0x0075;
    /// Surface with no cam collision flag, slippery with noise.
    pub const NO_CAM_COL_SLIPPERY: u16 = 0x0079;
    /// Surface with no cam collision flag, very slippery with noise.
    pub const NO_CAM_COL_VERY_SLIPPERY: u16 = 0x0078;
    /// Surface with no cam collision flag, non-slippery with noise.
    pub const SWITCH: u16 = 0x007A;
    /// Intangible area, only restricts camera movement: skipped by the
    /// floor/ceiling/wall queries (never camera-checking here).
    pub const CAMERA_BOUNDARY: u16 = 0x0072;
    /// Vanish-cap walls: passable with the vanish cap. The oracle harness
    /// never sets `gCurrentObject`, so these collide normally (ST-06).
    pub const VANISH_CAP_WALLS: u16 = 0x007B;
}

/// Decomp `SURFACE_FLAG_*` bits (`oracle/sm64/include/surface_terrains.h:163-165`).
/// Stored as `s8` on the decomp's `struct Surface`.
pub mod surface_flags {
    /// Set for surfaces belonging to objects (never set by the static loader).
    pub const DYNAMIC: i8 = 1 << 0;
    /// The camera ignores this surface for collision.
    pub const NO_CAM_COLLISION: i8 = 1 << 1;
    /// Wall point-in-triangle test projects on (z, y) instead of (x, y).
    /// Set by the loader for walls with `|normal.x| > 0.707`.
    pub const X_PROJECTION: i8 = 1 << 3;
}

/// Decomp `TERRAIN_*` area terrain types (`oracle/sm64/include/surface_terrains.h`).
/// An area-level property (not per-surface): slide terrain forces the very
/// slippery floor class.
pub mod terrain_type {
    pub const GRASS: u16 = 0x0000;
    pub const STONE: u16 = 0x0001;
    pub const SNOW: u16 = 0x0002;
    pub const SAND: u16 = 0x0003;
    pub const SPOOKY: u16 = 0x0004;
    pub const WATER: u16 = 0x0005;
    pub const SLIDE: u16 = 0x0006;
    pub const MASK: u16 = 0x0007;
}

/// What kind of surface a triangle is, classified by its normal.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SurfaceKind {
    /// Walkable ground.
    #[default]
    Default,
    /// Steep enough to slide on.
    Slide,
    /// Quicksand: sinks the character (see `quicksand_depth`); no slope
    /// assist (maps to the not-slippery class).
    Quicksand,
    /// Game-defined kinds start here; the meaning is the game's own.
    Custom(u8),
}

impl SurfaceKind {
    /// Convenience mapping from a decomp `SURFACE_*` type, for backends that
    /// only track the coarse kind. Very-slippery and slippery surfaces both
    /// map to `Slide`; the quicksand family maps to `Quicksand`.
    pub fn from_surface_type(ty: u16) -> Self {
        use surface_type::*;
        match ty {
            SLIPPERY | VERY_SLIPPERY | ICE => SurfaceKind::Slide,
            SHALLOW_QUICKSAND | DEEP_QUICKSAND | INSTANT_QUICKSAND | DEEP_MOVING_QUICKSAND
            | SHALLOW_MOVING_QUICKSAND | QUICKSAND | MOVING_QUICKSAND | INSTANT_MOVING_QUICKSAND => {
                SurfaceKind::Quicksand
            }
            _ => SurfaceKind::Default,
        }
    }
}

/// Slipperiness class of a surface, driving slide behavior. The four
/// classes and their numbers follow the decomp (`SURFACE_CLASS_*`,
/// `oracle/sm64/include/surface_terrains.h:158-161`); the class values were
/// deliberately chosen to equal the corresponding `SURFACE_*` types.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u16)]
pub enum SurfaceClass {
    /// Ice-like: slides on ~10-degree slopes and up.
    VerySlippery = surface_type::VERY_SLIPPERY,
    /// Slippery: slides on ~20-degree slopes and up.
    Slippery = surface_type::SLIPPERY,
    /// Normal ground.
    #[default]
    Default = surface_type::DEFAULT,
    /// Never slippery (grippy).
    NotSlippery = surface_type::NOT_SLIPPERY,
}

impl SurfaceClass {
    /// Slipperiness class for a surface kind. Slide-terrain maps to
    /// Slippery (steep slide ramps still trigger via the slide-terrain
    /// rule); Quicksand maps to NotSlippery (no slope assist, never
    /// slide-triggers; the sink logic keys off the kind directly).
    pub fn of(kind: SurfaceKind) -> Self {
        match kind {
            SurfaceKind::Default => SurfaceClass::Default,
            SurfaceKind::Slide => SurfaceClass::Slippery,
            SurfaceKind::Quicksand => SurfaceClass::NotSlippery,
            SurfaceKind::Custom(_) => SurfaceClass::Default,
        }
    }

    /// Slipperiness class for a `floor_kind` index (see
    /// `surface_kind_index`): 0 = Default, 1 = Slide, 2 = Quicksand,
    /// 3+ = Custom.
    pub fn of_kind_index(index: u8) -> Self {
        match index {
            0 => SurfaceClass::Default,
            1 => SurfaceClass::Slippery,
            2 => SurfaceClass::NotSlippery,
            _ => SurfaceClass::Default,
        }
    }

    /// A floor counts as slippery for slide purposes when its normal.y is
    /// at or below this. spec: surface.class_slippery_y
    pub fn slippery_floor_y(self) -> f32 {
        match self {
            SurfaceClass::VerySlippery => 0.9848077,
            SurfaceClass::Slippery => 0.9396926,
            SurfaceClass::Default => 0.7880108,
            // NotSlippery: never slippery (threshold below any valid normal).
            SurfaceClass::NotSlippery => -1.0,
        }
    }

    /// Normal.y at or below which a jump from this floor becomes a steep
    /// jump (when not facing downhill). Decomp `mario_floor_is_steep`
    /// thresholds: ~cos(15 deg) for very slippery, ~cos(20 deg) for
    /// slippery, ~cos(30 deg) otherwise.
    pub fn steep_jump_y(self) -> f32 {
        match self {
            SurfaceClass::VerySlippery => 0.9659258,
            SurfaceClass::Slippery => 0.9396926,
            SurfaceClass::Default => 0.8660254,
            SurfaceClass::NotSlippery => 0.8660254,
        }
    }

    /// Normal.y at or below which the floor counts as a slope (for
    /// slope-speed purposes). spec: surface.class_slope_y
    pub fn slope_y(self) -> f32 {
        match self {
            SurfaceClass::VerySlippery => 0.9961947,
            SurfaceClass::Slippery => 0.9848077,
            SurfaceClass::Default => 0.9659258,
            SurfaceClass::NotSlippery => 0.9396926,
        }
    }

    /// Normal.y at or below which the floor counts as steep (slide
    /// activation band). spec: surface.class_steep_y
    pub fn steep_y(self) -> f32 {
        match self {
            SurfaceClass::VerySlippery => 0.9659258,
            SurfaceClass::Slippery => 0.9396926,
            SurfaceClass::Default | SurfaceClass::NotSlippery => 0.8660254,
        }
    }

    /// Downhill acceleration applied to slides, per frame.
    /// spec: surface.class_slide_accel
    pub fn slide_accel(self) -> f32 {
        match self {
            SurfaceClass::VerySlippery => 10.0,
            SurfaceClass::Slippery => 8.0,
            SurfaceClass::Default => 7.0,
            SurfaceClass::NotSlippery => 5.0,
        }
    }

    /// Base per-frame velocity retention for slides (stick input modulates
    /// this in `update_slide_vector`). spec: surface.class_slide_loss
    pub fn slide_loss(self) -> f32 {
        match self {
            SurfaceClass::VerySlippery => 0.98,
            SurfaceClass::Slippery => 0.96,
            SurfaceClass::Default | SurfaceClass::NotSlippery => 0.92,
        }
    }
}

/// Result of a floor query: the highest surface at or below a position.
#[derive(Clone, Copy, Debug, Default)]
pub struct FloorHit {
    pub y: f32,
    pub normal: Vec3,
    pub kind: SurfaceKind,
    /// Decomp `SURFACE_*` type of the surface (see [`surface_type`]).
    pub surface_type: u16,
    /// Decomp surface flags (`SURFACE_FLAG_*`, `s8` in the decomp).
    pub flags: i8,
    /// Decomp surface force (wind/quicksand parameter; `s16` in the decomp).
    pub force: i16,
    /// Game-defined surface payload, readable on contact.
    pub user_data: u64,
}

/// Result of a ceiling query.
#[derive(Clone, Copy, Debug, Default)]
pub struct CeilingHit {
    pub y: f32,
    pub normal: Vec3,
    /// Decomp `SURFACE_*` type of the surface.
    pub surface_type: u16,
    /// Decomp surface flags (`s8` in the decomp).
    pub flags: i8,
    /// Game-defined surface payload, readable on contact.
    pub user_data: u64,
}

/// A wall surface referenced by a collision query.
/// decomp: `struct Surface *` (the decomp's wall pointer, e.g. `m->wall`).
#[derive(Clone, Copy, Debug, Default)]
pub struct WallHit {
    pub normal: Vec3,
    /// Decomp `SURFACE_*` type of the surface.
    pub surface_type: u16,
    /// Decomp surface flags (`s8` in the decomp).
    pub flags: i8,
    /// Decomp surface force (`s16` in the decomp).
    pub force: i16,
    /// Game-defined surface payload, readable on contact.
    pub user_data: u64,
}

/// Result of a wall query: the pushed position plus the referenced wall.
/// decomp: `resolve_and_return_wall_collisions` (`src/game/mario.c:521`),
/// which mutates the position and returns `walls[numWalls - 1]`.
#[derive(Clone, Copy, Debug, Default)]
pub struct WallCollision {
    /// Position after pushing out of walls (y is never modified).
    pub pos: Vec3,
    /// The referenced wall (`walls[numWalls - 1]`), if any wall collided.
    pub wall: Option<WallHit>,
    /// Number of walls recorded (capped at 4, like the decomp).
    pub num_walls: u32,
}

/// Decomp collision constants (`src/engine/surface_collision.h`).
/// `FLOOR_LOWER_LIMIT` is the height `find_floor`/`find_water_level` report
/// when nothing is found; `CELL_HEIGHT_LIMIT` is what `find_ceil` reports.
pub const FLOOR_LOWER_LIMIT: f32 = -11000.0;
pub const CELL_HEIGHT_LIMIT: f32 = 20000.0;

/// The collision interface every backend implements.
///
/// This mirrors the decomp's query functions (`src/engine/surface_collision.c`):
/// exact parity is only attainable with a triangle-based world implementing
/// the decomp's algorithms (plan §8.1, ST-02..ST-06). `SurfaceWorld` (the
/// reference backend) implements them exactly; other backends implement
/// best-effort (documented as non-exact).
pub trait CollisionWorld {
    /// Decomp `find_floor(x, y, z, &floor)`: the first floor (in per-cell
    /// height-descending order) whose triangle contains `(x, z)` with
    /// `height <= y + 78`. Coordinates are truncated to `s16`; queries with
    /// `|x|,|z| >= 8192` find nothing. `SURFACE_INTANGIBLE` floors trigger a
    /// re-search from `height - 200` (ST-02).
    fn find_floor(&self, x: f32, y: f32, z: f32) -> Option<FloorHit>;
    /// Decomp `find_ceil(x, y, z, &ceil)`: the first ceiling (in per-cell
    /// height-ascending order) whose triangle contains `(x, z)` with
    /// `height >= y - 78`.
    fn find_ceil(&self, x: f32, y: f32, z: f32) -> Option<CeilingHit>;
    /// Decomp `resolve_and_return_wall_collisions(pos, offset, radius)`:
    /// pushes the position out of walls sampled at `y + offset`, returning
    /// the pushed position and the referenced wall. Radius is capped at 200;
    /// multiple walls over-push (the local x/z are not updated between
    /// walls); at most 4 walls are recorded (ST-06).
    fn find_wall_collisions(
        &self,
        x: f32,
        y: f32,
        z: f32,
        offset: f32,
        radius: f32,
    ) -> WallCollision;
    /// Decomp `find_water_level(x, z)`: the water height when a water region
    /// covers `(x, z)`, else `FLOOR_LOWER_LIMIT`.
    fn find_water_level(&self, x: f32, z: f32) -> f32;
    /// True when a wall surface passes within `radius` of `point`.
    /// Used for ledge-grab sensing; defaults to false.
    fn wall_probe(&self, _point: Vec3, _radius: f32) -> bool {
        false
    }
    /// Area-level terrain type (`TERRAIN_*`, see [`terrain_type`]).
    /// The decomp stores this on the area, not on surfaces; the slipperiness
    /// class depends on it (slide terrain forces very slippery).
    fn terrain_type(&self) -> u16 {
        terrain_type::GRASS
    }
}

/// An empty world: no floor, no ceiling, no walls, no water.
/// Useful for unit tests of pure action logic.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyWorld;

impl CollisionWorld for EmptyWorld {
    fn find_floor(&self, _x: f32, _y: f32, _z: f32) -> Option<FloorHit> {
        None
    }
    fn find_ceil(&self, _x: f32, _y: f32, _z: f32) -> Option<CeilingHit> {
        None
    }
    fn find_wall_collisions(
        &self,
        x: f32,
        y: f32,
        z: f32,
        _offset: f32,
        _radius: f32,
    ) -> WallCollision {
        WallCollision {
            pos: Vec3::new(x, y, z),
            wall: None,
            num_walls: 0,
        }
    }
    fn find_water_level(&self, _x: f32, _z: f32) -> f32 {
        FLOOR_LOWER_LIMIT
    }
}

/// An infinite flat floor at `y = 0`. The workhorse of ground-action tests.
#[derive(Clone, Copy, Debug, Default)]
pub struct FlatWorld {
    pub floor_y: f32,
}

impl CollisionWorld for FlatWorld {
    fn find_floor(&self, x: f32, y: f32, z: f32) -> Option<FloorHit> {
        // Decomp boundary rejection (ST-02): truncate to s16 first.
        let xi = x as i16;
        let zi = z as i16;
        if xi <= -8192 || xi >= 8192 || zi <= -8192 || zi >= 8192 {
            return None;
        }
        // Decomp +78 upward tolerance.
        if self.floor_y <= y + 78.0 {
            Some(FloorHit {
                y: self.floor_y,
                normal: Vec3::Y,
                kind: SurfaceKind::Default,
                surface_type: surface_type::DEFAULT,
                flags: 0,
                force: 0,
                user_data: 0,
            })
        } else {
            None
        }
    }
    fn find_ceil(&self, _x: f32, _y: f32, _z: f32) -> Option<CeilingHit> {
        None
    }
    fn find_wall_collisions(
        &self,
        x: f32,
        y: f32,
        z: f32,
        _offset: f32,
        _radius: f32,
    ) -> WallCollision {
        WallCollision {
            pos: Vec3::new(x, y, z),
            wall: None,
            num_walls: 0,
        }
    }
    fn find_water_level(&self, _x: f32, _z: f32) -> f32 {
        FLOOR_LOWER_LIMIT
    }
}

// ---------------------------------------------------------------------------
// Floor-class helpers, ported from the decomp (plan §4.1 FD-09).
// Named after the decomp functions; each carries its `// decomp:` line so
// later reviews can diff them (plan §8.6).
// ---------------------------------------------------------------------------

use crate::angles::Angle;
use crate::state::{ActionId, CharacterState};

/// Returns the slipperiness class of Mario's floor.
/// decomp: src/game/mario.c:387
pub fn mario_get_floor_class(state: &CharacterState) -> SurfaceClass {
    use surface_type::*;
    // The slide terrain type defaults to slide slipperiness.
    // This doesn't matter too much since normally the slide terrain
    // is checked for anyways.
    let mut floor_class = if (state.terrain_type & terrain_type::MASK) == terrain_type::SLIDE {
        SurfaceClass::VerySlippery
    } else {
        SurfaceClass::Default
    };

    if let Some(floor) = &state.floor {
        floor_class = match floor.surface_type {
            NOT_SLIPPERY | HARD_NOT_SLIPPERY | SWITCH => SurfaceClass::NotSlippery,
            SLIPPERY | NOISE_SLIPPERY | HARD_SLIPPERY | NO_CAM_COL_SLIPPERY => {
                SurfaceClass::Slippery
            }
            VERY_SLIPPERY | ICE | HARD_VERY_SLIPPERY | NOISE_VERY_SLIPPERY_73
            | NOISE_VERY_SLIPPERY_74 | NOISE_VERY_SLIPPERY | NO_CAM_COL_VERY_SLIPPERY => {
                SurfaceClass::VerySlippery
            }
            _ => floor_class,
        };
    }

    // Crawling allows Mario to not slide on certain steeper surfaces.
    if state.action == ActionId::CRAWL
        && state
            .floor
            .is_some_and(|f| f.normal.y > 0.5)
        && floor_class == SurfaceClass::Default
    {
        floor_class = SurfaceClass::NotSlippery;
    }

    floor_class
}

/// Determines if Mario is facing downhill. `turn_yaw` is always passed as
/// zero in practice (kept for signature fidelity).
/// decomp: src/game/mario.c:556
pub fn mario_facing_downhill(state: &CharacterState, turn_yaw: i32) -> bool {
    let mut face_angle_yaw = state.face_yaw.0;

    // This is never used in practice, as turnYaw is
    // always passed as zero.
    if turn_yaw != 0 && state.forward_speed < 0.0 {
        face_angle_yaw = face_angle_yaw.wrapping_add(Angle::HALF_TURN.0);
    }

    // (s16)(m->floorAngle - faceAngleYaw): wraps in i16.
    let d = state.floor_angle.0.wrapping_sub(face_angle_yaw);
    (-0x4000 < d) && (d < 0x4000)
}

/// Determines if a surface is slippery based on the surface class.
/// decomp: src/game/mario.c:573
///
/// NOTE: the decomp's switch matches on the *class* values, whose numeric
/// values deliberately equal the corresponding `SURFACE_*` types
/// (`SURFACE_CLASS_VERY_SLIPPERY == SURFACE_VERY_SLIPPERY == 0x0013`, etc.).
pub fn mario_floor_is_slippery(state: &CharacterState) -> bool {
    let floor = match &state.floor {
        Some(f) => f,
        None => return false,
    };
    if (state.terrain_type & terrain_type::MASK) == terrain_type::SLIDE
        && floor.normal.y < 0.9998477
    {
        // ~cos(1 deg)
        return true;
    }

    let norm_y = match mario_get_floor_class(state) {
        SurfaceClass::VerySlippery => 0.9848077, // ~cos(10 deg)
        SurfaceClass::Slippery => 0.9396926,     // ~cos(20 deg)
        SurfaceClass::Default => 0.7880108,      // ~cos(38 deg)
        SurfaceClass::NotSlippery => 0.0,
    };

    floor.normal.y <= norm_y
}

/// Determines if a surface is a slope based on the surface class.
/// decomp: src/game/mario.c:606
pub fn mario_floor_is_slope(state: &CharacterState) -> bool {
    let floor = match &state.floor {
        Some(f) => f,
        None => return false,
    };
    if (state.terrain_type & terrain_type::MASK) == terrain_type::SLIDE
        && floor.normal.y < 0.9998477
    {
        // ~cos(1 deg)
        return true;
    }

    let norm_y = match mario_get_floor_class(state) {
        SurfaceClass::VerySlippery => 0.9961947, // ~cos(5 deg)
        SurfaceClass::Slippery => 0.9848077,     // ~cos(10 deg)
        SurfaceClass::Default => 0.9659258,      // ~cos(15 deg)
        SurfaceClass::NotSlippery => 0.9396926,   // ~cos(20 deg)
    };

    floor.normal.y <= norm_y
}

/// Determines if a surface is steep based on the surface class.
/// decomp: src/game/mario.c:638
pub fn mario_floor_is_steep(state: &CharacterState) -> bool {
    let floor = match &state.floor {
        Some(f) => f,
        None => return false,
    };
    let mut result = false;

    // Interestingly, this function does not check for the
    // slide terrain type. This means that steep behavior persists for
    // non-slippery and slippery surfaces.
    // This does not matter in vanilla game practice.
    if !mario_facing_downhill(state, 0) {
        let norm_y = match mario_get_floor_class(state) {
            SurfaceClass::VerySlippery => 0.9659258, // ~cos(15 deg)
            SurfaceClass::Slippery => 0.9396926,     // ~cos(20 deg)
            SurfaceClass::Default | SurfaceClass::NotSlippery => 0.8660254, // ~cos(30 deg)
        };

        result = floor.normal.y <= norm_y;
    }

    result
}
