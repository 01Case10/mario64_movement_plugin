//! Collision: the core talks to the world through [`CollisionWorld`], so the
//! same code runs against test geometry and a live Godot scene.

use glam::Vec3;

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

/// Slipperiness class of a surface, driving slide behavior. The four
/// classes and their numbers follow the decomp-documented surface matrix
/// (behavioral reference only).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SurfaceClass {
    /// Ice-like: slides on ~10-degree slopes and up.
    VerySlippery,
    /// Slippery: slides on ~20-degree slopes and up.
    Slippery,
    /// Normal ground.
    #[default]
    Default,
    /// Never slippery (grippy).
    NotSlippery,
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
    /// Game-defined surface payload, readable on contact.
    pub user_data: u64,
}

/// Result of a ceiling query.
#[derive(Clone, Copy, Debug, Default)]
pub struct CeilingHit {
    pub y: f32,
    pub normal: Vec3,
}

/// Result of the wall query: push-out applied to the proposed position.
#[derive(Clone, Copy, Debug, Default)]
pub struct WallResolve {
    /// Position after pushing out of walls.
    pub pos: Vec3,
    /// True when at least one wall was hit.
    pub hit: bool,
    /// Number of walls hit (for diagnostics).
    pub num_walls: u32,
    /// Normal of the first wall hit.
    pub normal: Vec3,
}

/// The collision interface every backend implements.
///
/// `SurfaceWorld` (the reference backend, a triangle soup with a spatial
/// grid) is built on this trait in phase 2; the Godot-physics adapter
/// implements it in phase 5.
pub trait CollisionWorld {
    /// Highest floor surface at or below `pos.y + max_above`.
    fn find_floor(&self, pos: Vec3, max_above: f32) -> Option<FloorHit>;
    /// Lowest ceiling above `pos`, searching up to `pos.y + height`.
    fn find_ceiling(&self, pos: Vec3, height: f32) -> Option<CeilingHit>;
    /// Push `pos` out of walls within `radius` of the character, sampling
    /// at `offset` above the feet.
    fn resolve_walls(&self, pos: Vec3, offset: f32, radius: f32) -> WallResolve;
    /// Water surface height at (x, z), if any.
    fn water_level(&self, x: f32, z: f32) -> Option<f32>;
    /// True when a wall surface passes within `radius` of `point`.
    /// Used for ledge-grab sensing; defaults to false.
    fn wall_probe(&self, _point: Vec3, _radius: f32) -> bool {
        false
    }
}

/// An empty world: no floor, no ceiling, no walls, no water.
/// Useful for unit tests of pure action logic.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyWorld;

impl CollisionWorld for EmptyWorld {
    fn find_floor(&self, _pos: Vec3, _max_above: f32) -> Option<FloorHit> {
        None
    }
    fn find_ceiling(&self, _pos: Vec3, _height: f32) -> Option<CeilingHit> {
        None
    }
    fn resolve_walls(&self, pos: Vec3, _offset: f32, _radius: f32) -> WallResolve {
        WallResolve {
            pos,
            ..Default::default()
        }
    }
    fn water_level(&self, _x: f32, _z: f32) -> Option<f32> {
        None
    }
}

/// An infinite flat floor at `y = 0`. The workhorse of ground-action tests.
#[derive(Clone, Copy, Debug, Default)]
pub struct FlatWorld {
    pub floor_y: f32,
}

impl CollisionWorld for FlatWorld {
    fn find_floor(&self, pos: Vec3, max_above: f32) -> Option<FloorHit> {
        if self.floor_y <= pos.y + max_above {
            Some(FloorHit {
                y: self.floor_y,
                normal: Vec3::Y,
                kind: SurfaceKind::Default,
                user_data: 0,
            })
        } else {
            None
        }
    }
    fn find_ceiling(&self, _pos: Vec3, _height: f32) -> Option<CeilingHit> {
        None
    }
    fn resolve_walls(&self, pos: Vec3, _offset: f32, _radius: f32) -> WallResolve {
        WallResolve {
            pos,
            ..Default::default()
        }
    }
    fn water_level(&self, _x: f32, _z: f32) -> Option<f32> {
        None
    }
}
