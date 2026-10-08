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
    /// Kills horizontal speed (quicksand-like). v1: no slope assist
    /// (maps to the not-slippery accel); the full quicksand sink/depth
    /// system lands in plan Phase D.
    Quicksand,
    /// Game-defined kinds start here; the meaning is the game's own.
    Custom(u8),
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
