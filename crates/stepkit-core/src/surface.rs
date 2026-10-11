//! `SurfaceWorld`: the reference collision backend.
//!
//! Verbatim port of the decomp's surface pipeline: `surface_load.c`
//! (triangle loading, normal computation, spatial partition) and
//! `surface_collision.c` (floor/ceiling/wall/water queries). Every numeric
//! operation mirrors the C code's `f32` operation order (plan §5),
//! including signed-zero behavior — the oracle diff compares bitwise, so
//! `originOffset` `-0.0` vs `0.0` matters.
//!
//! Decomp references: `src/engine/surface_load.c`, `src/engine/surface_collision.c`,
//! `src/engine/surface_collision.h`, `src/engine/surface_load.h`.

use crate::world::{
    surface_flags, surface_type, CeilingHit, CollisionWorld, FloorHit, SurfaceKind, WallCollision,
    WallHit, CELL_HEIGHT_LIMIT, FLOOR_LOWER_LIMIT,
};
use glam::Vec3;

// ---- Decomp constants (surface_collision.h / surface_load.h) ----
const LEVEL_BOUNDARY_MAX: i32 = 8192;
const CELL_SIZE: i32 = 1024;
const NUM_CELLS: i32 = 16;
const NUM_CELLS_INDEX: i32 = 15;

// Partition list indices (decomp `SPATIAL_PARTITION_*`).
const SP_FLOORS: usize = 0;
const SP_CEILS: usize = 1;
const SP_WALLS: usize = 2;

/// A loaded surface.
/// decomp: `struct Surface` (`include/types.h:221`).
///
/// Vertices are `s16` (`Vec3Terrain`) in the decomp; stored as `i32` here
/// (all real inputs are within `s16` range, so the values are identical).
#[derive(Clone, Debug)]
struct Surface {
    /// The three vertices, `[x, y, z]`.
    v: [[i32; 3]; 3],
    /// Unit normal, computed by `read_surface_data`'s `(v2-v1)x(v3-v2)`.
    normal: [f32; 3],
    /// Plane constant: `-(nx*x1 + ny*y1 + nz*z1)`.
    origin_offset: f32,
    /// `minY - 5` / `maxY + 5` (`TerrainData` in the decomp).
    lower_y: i32,
    upper_y: i32,
    /// Decomp `SURFACE_*` type (`TerrainData`).
    surf_type: i16,
    /// Decomp surface force (`TerrainData`).
    force: i16,
    /// Decomp surface flags (`s8`).
    flags: i8,
    /// Convenience kind derived from the type.
    kind: SurfaceKind,
    user_data: u64,
}

impl Surface {
    fn is_floor(&self) -> bool {
        self.normal[1] > 0.01
    }
    fn is_ceil(&self) -> bool {
        self.normal[1] < -0.01
    }
}

/// Water region.
/// decomp: `gEnvironmentRegions` entries (`find_water_level`).
#[derive(Clone, Debug)]
struct WaterRegion {
    /// Region value; `< 50` means water (gas/others are `>= 50`).
    val: i32,
    lo_x: f32,
    lo_z: f32,
    hi_x: f32,
    hi_z: f32,
    level: f32,
}

/// Reference collision backend: decomp surface pipeline.
///
/// Surfaces are stored in insertion order; each of the 16x16 cells holds
/// three lists (floors, ceilings, walls). Floors sort by `vertex1.y`
/// descending, ceilings ascending, walls keep insertion order — all stable
/// (`add_surface_to_cell`, `surface_load.c`).
#[derive(Clone, Debug, Default)]
pub struct SurfaceWorld {
    surfaces: Vec<Surface>,
    /// `cells[z][x][list]` → surface indices.
    cells: Vec<Vec<[Vec<usize>; 3]>>,
    water: Vec<WaterRegion>,
    /// Area-level `TERRAIN_*` type (decomp: `area->terrainType`). Defaults to
    /// grass; the scenario runner sets it from the scenario's `terrain`.
    terrain: u16,
}

impl SurfaceWorld {
    pub fn new() -> Self {
        Self {
            surfaces: Vec::new(),
            cells: vec![
                vec![[Vec::new(), Vec::new(), Vec::new()]; NUM_CELLS as usize];
                NUM_CELLS as usize
            ],
            water: Vec::new(),
            terrain: crate::world::terrain_type::GRASS,
        }
    }

    /// Set the area-level `TERRAIN_*` type (e.g. `TERRAIN_SLIDE`).
    pub fn set_terrain_type(&mut self, terrain: u16) {
        self.terrain = terrain;
    }

    /// Number of triangles (diagnostic).
    pub fn triangle_count(&self) -> usize {
        self.surfaces.len()
    }

    /// Load one triangle from `s16` vertices, computing the normal exactly
    /// like `read_surface_data` (`surface_load.c:296`).
    ///
    /// Returns `None` for degenerate triangles (`mag < 0.0001`), which the
    /// decomp drops.
    fn load_surface(
        v: [[i32; 3]; 3],
        surf_type: u16,
        force: i16,
        user_data: u64,
    ) -> Option<Surface> {
        let (x1, y1, z1) = (v[0][0] as i64, v[0][1] as i64, v[0][2] as i64);
        let (x2, y2, z2) = (v[1][0] as i64, v[1][1] as i64, v[1][2] as i64);
        let (x3, y3, z3) = (v[2][0] as i64, v[2][1] as i64, v[2][2] as i64);
        // (v2 - v1) x (v3 - v2), in s32 (exact for in-range verts), then f32.
        let mut nx = ((y2 - y1) * (z3 - z2) - (z2 - z1) * (y3 - y2)) as f32;
        let mut ny = ((z2 - z1) * (x3 - x2) - (x2 - x1) * (z3 - z2)) as f32;
        let mut nz = ((x2 - x1) * (y3 - y2) - (y2 - y1) * (x3 - x2)) as f32;
        let mag = (nx * nx + ny * ny + nz * nz).sqrt();
        // Checking to make sure no DIV/0.
        if mag < 0.0001 {
            return None;
        }
        let inv = 1.0f32 / mag;
        nx *= inv;
        ny *= inv;
        nz *= inv;

        let min_y = y1.min(y2).min(y3) as i32;
        let max_y = y1.max(y2).max(y3) as i32;

        // decomp: surface_load.c `load_static_surfaces` + `add_surface_to_cell`.
        let mut flags: i8 = match surf_type {
            surface_type::SWITCH => surface_flags::NO_CAM_COLLISION,
            _ => 0,
        };
        let is_wall = !(ny > 0.01) && !(ny < -0.01);
        if is_wall && (nx < -0.707 || nx > 0.707) {
            flags |= surface_flags::X_PROJECTION;
        }

        Some(Surface {
            v,
            normal: [nx, ny, nz],
            origin_offset: -(nx * x1 as f32 + ny * y1 as f32 + nz * z1 as f32),
            lower_y: min_y - 5,
            upper_y: max_y + 5,
            surf_type: surf_type as i16,
            force,
            flags,
            kind: SurfaceKind::from_surface_type(surf_type),
            user_data,
        })
    }

    /// Insert a surface into the partition (`add_surface`, `surface_load.c`).
    fn add_surface(&mut self, idx: usize) {
        let s = &self.surfaces[idx];
        let min_x = s.v[0][0].min(s.v[1][0]).min(s.v[2][0]);
        let max_x = s.v[0][0].max(s.v[1][0]).max(s.v[2][0]);
        let min_z = s.v[0][2].min(s.v[1][2]).min(s.v[2][2]);
        let max_z = s.v[0][2].max(s.v[1][2]).max(s.v[2][2]);
        let (cx0, cx1) = (lower_cell_index(min_x), upper_cell_index(max_x));
        let (cz0, cz1) = (lower_cell_index(min_z), upper_cell_index(max_z));
        for cz in cz0..=cz1 {
            for cx in cx0..=cx1 {
                self.add_surface_to_cell(cx, cz, idx);
            }
        }
    }

    /// Insert into one cell's list (`add_surface_to_cell`, `surface_load.c`).
    fn add_surface_to_cell(&mut self, cell_x: i32, cell_z: i32, idx: usize) {
        let ny = self.surfaces[idx].normal[1];
        let (list_index, sort_dir) = if ny > 0.01 {
            (SP_FLOORS, 1) // highest to lowest, then insertion order
        } else if ny < -0.01 {
            (SP_CEILS, -1) // lowest to highest, then insertion order
        } else {
            (SP_WALLS, 0) // insertion order
        };
        // Surfaces are sorted by the height of their first vertex ("surface
        // cucking": vertices aren't ordered by height, so lower triangles
        // can sort higher).
        let priority = self.surfaces[idx].v[0][1] * sort_dir;
        let list = &mut self.cells[cell_z as usize][cell_x as usize][list_index];
        // Stable insertion: before the first strictly-lower priority.
        let pos = list
            .iter()
            .position(|&i| priority > self.surfaces[i].v[0][1] * sort_dir)
            .unwrap_or(list.len());
        list.insert(pos, idx);
    }

    /// Add one triangle. Integer vertices; winding determines the normal via
    /// the decomp's `(v2-v1)x(v3-v2)` formula (see `load_surface`).
    pub fn add_triangle(
        &mut self,
        a: [i32; 3],
        b: [i32; 3],
        c: [i32; 3],
        surface_type: u16,
        user_data: u64,
    ) -> u32 {
        let idx = self.surfaces.len();
        if let Some(s) = Self::load_surface([a, b, c], surface_type, 0, user_data) {
            self.surfaces.push(s);
            self.add_surface(idx);
        }
        idx as u32
    }

    /// Add one triangle from float vertices (truncated to integer, like the
    /// decomp's `TerrainData` collision data).
    pub fn add_triangle_f(
        &mut self,
        a: Vec3,
        b: Vec3,
        c: Vec3,
        surface_type: u16,
        user_data: u64,
    ) -> u32 {
        self.add_triangle(
            [a.x as i32, a.y as i32, a.z as i32],
            [b.x as i32, b.y as i32, b.z as i32],
            [c.x as i32, c.y as i32, c.z as i32],
            surface_type,
            user_data,
        )
    }

    /// Add an axis-aligned box as 12 triangles.
    ///
    /// Triangle order and winding match `stepkit_trace::geom::push_box`
    /// (the oracle-side builder) exactly — top, bottom, z-min, z-max,
    /// x-min, x-max — because the decomp's per-cell wall order (insertion
    /// order) and floor/ceiling sort (stable by `vertex1.y`) depend on it.
    pub fn add_box(&mut self, min: Vec3, max: Vec3, surface_type: u16, user_data: u64) {
        let (x0, y0, z0) = (min.x as i32, min.y as i32, min.z as i32);
        let (x1, y1, z1) = (max.x as i32, max.y as i32, max.z as i32);
        // Corner order (mirrors geom.rs):
        // 0:(minx,miny,minz) 1:(maxx,miny,minz) 2:(maxx,miny,maxz) 3:(minx,miny,maxz)
        // 4:(minx,maxy,minz) 5:(maxx,maxy,minz) 6:(maxx,maxy,maxz) 7:(minx,maxy,maxz)
        let c = [
            [x0, y0, z0],
            [x1, y0, z0],
            [x1, y0, z1],
            [x0, y0, z1],
            [x0, y1, z0],
            [x1, y1, z0],
            [x1, y1, z1],
            [x0, y1, z1],
        ];
        const TRIS: [(usize, usize, usize); 12] = [
            (4, 7, 6),
            (4, 6, 5), // top (+y)
            (0, 1, 2),
            (0, 2, 3), // bottom (-y)
            (1, 0, 4),
            (1, 4, 5), // z-min wall (-z)
            (3, 2, 6),
            (3, 6, 7), // z-max wall (+z)
            (0, 3, 7),
            (0, 7, 4), // x-min wall (-x)
            (2, 1, 5),
            (2, 5, 6), // x-max wall (+x)
        ];
        for &(a, b, d) in &TRIS {
            self.add_triangle(c[a], c[b], c[d], surface_type, user_data);
        }
    }

    /// Add a sloped quad from `origin` along `dir` (XZ), `length` long,
    /// rising `height`, `width` across.
    ///
    /// Corner computation and the winding fix-up mirror
    /// `stepkit_trace::geom::push_ramp` exactly (the oracle-side builder),
    /// so both sides load identical surfaces.
    #[allow(clippy::too_many_arguments)] // geometry builder: explicit params beat a struct here
    pub fn add_ramp(
        &mut self,
        origin: Vec3,
        dir: Vec3,
        length: f32,
        height: f32,
        width: f32,
        surface_type: u16,
        user_data: u64,
    ) {
        // geom.rs push_ramp, verbatim in f32.
        let len = (dir.x * dir.x + dir.z * dir.z).sqrt();
        if !len.is_finite() || len <= 0.0 {
            return;
        }
        let dirn_x = dir.x / len;
        let dirn_z = dir.z / len;
        let px = -dirn_z;
        let pz = dirn_x;
        let hw = width * 0.5;
        let c0 = [
            (origin.x - px * hw) as i32,
            origin.y as i32,
            (origin.z - pz * hw) as i32,
        ];
        let c1 = [
            (origin.x + px * hw) as i32,
            origin.y as i32,
            (origin.z + pz * hw) as i32,
        ];
        let c2 = [
            (origin.x - px * hw + dirn_x * length) as i32,
            (origin.y + height) as i32,
            (origin.z - pz * hw + dirn_z * length) as i32,
        ];
        let c3 = [
            (origin.x + px * hw + dirn_x * length) as i32,
            (origin.y + height) as i32,
            (origin.z + pz * hw + dirn_z * length) as i32,
        ];
        // Fix the winding so the decomp normal has positive y.
        if decomp_normal_y(c0, c1, c3) >= 0 {
            self.add_triangle(c0, c1, c3, surface_type, user_data);
            self.add_triangle(c0, c3, c2, surface_type, user_data);
        } else {
            self.add_triangle(c1, c0, c2, surface_type, user_data);
            self.add_triangle(c1, c2, c3, surface_type, user_data);
        }
    }

    /// XZ bounding box over all triangle vertices (for oracle-style water
    /// regions, which cover the geometry bounding box).
    pub fn vertex_bounds_xz(&self) -> Option<(f32, f32, f32, f32)> {
        let mut it = self.surfaces.iter().flat_map(|s| s.v.iter());
        let first = it.next()?;
        let (mut lo_x, mut hi_x, mut lo_z, mut hi_z) = (
            first[0] as f32,
            first[0] as f32,
            first[2] as f32,
            first[2] as f32,
        );
        for v in it {
            lo_x = lo_x.min(v[0] as f32);
            hi_x = hi_x.max(v[0] as f32);
            lo_z = lo_z.min(v[2] as f32);
            hi_z = hi_z.max(v[2] as f32);
        }
        Some((lo_x, hi_x, lo_z, hi_z))
    }

    /// Add a water volume: flat surface at `y` over an XZ rectangle.
    /// Stored as a decomp environment region (`val = 0` < 50 means water).
    pub fn add_water(&mut self, y: f32, min_x: f32, max_x: f32, min_z: f32, max_z: f32) {
        self.water.push(WaterRegion {
            val: 0,
            lo_x: min_x,
            lo_z: min_z,
            hi_x: max_x,
            hi_z: max_z,
            level: y,
        });
    }

    /// Add a raw environment region (decomp `gEnvironmentRegions` entry).
    pub fn add_water_region(
        &mut self,
        val: i32,
        lo_x: f32,
        lo_z: f32,
        hi_x: f32,
        hi_z: f32,
        level: f32,
    ) {
        self.water.push(WaterRegion {
            val,
            lo_x,
            lo_z,
            hi_x,
            hi_z,
            level,
        });
    }

    fn floor_hit_of(&self, idx: usize, height: f32) -> FloorHit {
        let s = &self.surfaces[idx];
        FloorHit {
            y: height,
            normal: Vec3::new(s.normal[0], s.normal[1], s.normal[2]),
            kind: s.kind,
            surface_type: s.surf_type as u16,
            flags: s.flags,
            force: s.force,
            user_data: s.user_data,
        }
    }

    fn ceil_hit_of(&self, idx: usize, height: f32) -> CeilingHit {
        let s = &self.surfaces[idx];
        CeilingHit {
            y: height,
            normal: Vec3::new(s.normal[0], s.normal[1], s.normal[2]),
            surface_type: s.surf_type as u16,
            flags: s.flags,
            user_data: s.user_data,
        }
    }

    fn wall_hit_of(&self, idx: usize) -> WallHit {
        let s = &self.surfaces[idx];
        WallHit {
            normal: Vec3::new(s.normal[0], s.normal[1], s.normal[2]),
            surface_type: s.surf_type as u16,
            flags: s.flags,
            force: s.force,
            user_data: s.user_data,
        }
    }

    /// `find_floor_from_list` (`surface_collision.c`): first floor in the
    /// list whose triangle contains `(x, z)` with `height <= y + 78`.
    /// All coordinates are integers (`s32` in the decomp).
    fn find_floor_from_list(&self, list: &[usize], x: i32, y: i32, z: i32) -> (Option<usize>, f32) {
        for &idx in list {
            let s = &self.surfaces[idx];
            let x1 = s.v[0][0];
            let z1 = s.v[0][2];
            let x2 = s.v[1][0];
            let z2 = s.v[1][2];
            // Check that the point is within the triangle bounds.
            if (z1 - z) * (x2 - x1) - (x1 - x) * (z2 - z1) < 0 {
                continue;
            }
            let x3 = s.v[2][0];
            let z3 = s.v[2][2];
            if (z2 - z) * (x3 - x2) - (x2 - x) * (z3 - z2) < 0 {
                continue;
            }
            if (z3 - z) * (x1 - x3) - (x3 - x) * (z1 - z3) < 0 {
                continue;
            }
            // Never camera-checking here: ignore camera-only floors.
            if s.surf_type as u16 == surface_type::CAMERA_BOUNDARY {
                continue;
            }
            let (nx, ny, nz, oo) = (s.normal[0], s.normal[1], s.normal[2], s.origin_offset);
            // If a wall, ignore it. Likely a remnant, should never occur.
            if ny == 0.0 {
                continue;
            }
            // Find the height of the floor at the point.
            let height = -((x as f32) * nx + nz * (z as f32) + oo) / ny;
            // Checks for floor interaction with a 78 unit buffer.
            if (y as f32) - (height + -78.0) < 0.0 {
                continue;
            }
            return (Some(idx), height);
        }
        (None, FLOOR_LOWER_LIMIT)
    }

    /// `find_ceil_from_list` (`surface_collision.c`): first ceiling in the
    /// list (height-ascending) whose triangle contains `(x, z)` with
    /// `height >= y - 78`.
    fn find_ceil_from_list(&self, list: &[usize], x: i32, y: i32, z: i32) -> (Option<usize>, f32) {
        for &idx in list {
            let s = &self.surfaces[idx];
            let x1 = s.v[0][0];
            let z1 = s.v[0][2];
            let x2 = s.v[1][0];
            let z2 = s.v[1][2];
            // Checking if point is in bounds of the triangle laterally.
            if (z1 - z) * (x2 - x1) - (x1 - x) * (z2 - z1) > 0 {
                continue;
            }
            let x3 = s.v[2][0];
            let z3 = s.v[2][2];
            if (z2 - z) * (x3 - x2) - (x2 - x) * (z3 - z2) > 0 {
                continue;
            }
            if (z3 - z) * (x1 - x3) - (x3 - x) * (z1 - z3) > 0 {
                continue;
            }
            if s.surf_type as u16 == surface_type::CAMERA_BOUNDARY {
                continue;
            }
            let (nx, ny, nz, oo) = (s.normal[0], s.normal[1], s.normal[2], s.origin_offset);
            // If a wall, ignore it. Likely a remnant, should never occur.
            if ny == 0.0 {
                continue;
            }
            // Find the ceil height at the specific point.
            let height = -((x as f32) * nx + nz * (z as f32) + oo) / ny;
            // Checks for ceiling interaction with a 78 unit buffer.
            // decomp: `if (y - (height - -78.0f) > 0.0f) continue;`
            // (surface_collision.c) — note the double negation: the buffer
            // is `height + 78`, not `height - 78` as for floors.
            if (y as f32) - (height - -78.0) > 0.0 {
                continue;
            }
            return (Some(idx), height);
        }
        (None, CELL_HEIGHT_LIMIT)
    }

    /// Point-in-triangle test for walls, in the projection plane.
    /// decomp: `find_wall_collisions_from_list` (`surface_collision.c:19`).
    /// Returns false when the wall is skipped.
    fn wall_contains(s: &Surface, x: f32, y: f32, z: f32) -> bool {
        if s.flags & surface_flags::X_PROJECTION != 0 {
            let w1 = -(s.v[0][2] as f32);
            let w2 = -(s.v[1][2] as f32);
            let w3 = -(s.v[2][2] as f32);
            let y1 = s.v[0][1] as f32;
            let y2 = s.v[1][1] as f32;
            let y3 = s.v[2][1] as f32;
            // `(w1 - -pz)`: negation is exact, so this is `w1 + pz` bitwise.
            if s.normal[0] > 0.0 {
                if (y1 - y) * (w2 - w1) - (w1 + z) * (y2 - y1) > 0.0 {
                    return false;
                }
                if (y2 - y) * (w3 - w2) - (w2 + z) * (y3 - y2) > 0.0 {
                    return false;
                }
                if (y3 - y) * (w1 - w3) - (w3 + z) * (y1 - y3) > 0.0 {
                    return false;
                }
            } else {
                if (y1 - y) * (w2 - w1) - (w1 + z) * (y2 - y1) < 0.0 {
                    return false;
                }
                if (y2 - y) * (w3 - w2) - (w2 + z) * (y3 - y2) < 0.0 {
                    return false;
                }
                if (y3 - y) * (w1 - w3) - (w3 + z) * (y1 - y3) < 0.0 {
                    return false;
                }
            }
        } else {
            let w1 = s.v[0][0] as f32;
            let w2 = s.v[1][0] as f32;
            let w3 = s.v[2][0] as f32;
            let y1 = s.v[0][1] as f32;
            let y2 = s.v[1][1] as f32;
            let y3 = s.v[2][1] as f32;
            if s.normal[2] > 0.0 {
                if (y1 - y) * (w2 - w1) - (w1 - x) * (y2 - y1) > 0.0 {
                    return false;
                }
                if (y2 - y) * (w3 - w2) - (w2 - x) * (y3 - y2) > 0.0 {
                    return false;
                }
                if (y3 - y) * (w1 - w3) - (w3 - x) * (y1 - y3) > 0.0 {
                    return false;
                }
            } else {
                if (y1 - y) * (w2 - w1) - (w1 - x) * (y2 - y1) < 0.0 {
                    return false;
                }
                if (y2 - y) * (w3 - w2) - (w2 - x) * (y3 - y2) < 0.0 {
                    return false;
                }
                if (y3 - y) * (w1 - w3) - (w3 - x) * (y1 - y3) < 0.0 {
                    return false;
                }
            }
        }
        true
    }
}

/// `lower_cell_index` (`surface_load.c`): the lower cell for a coordinate,
/// with the 50-unit border buffer.
fn lower_cell_index(coord: i32) -> i32 {
    // Move from range [-0x2000, 0x2000) to [0, 0x4000).
    let mut c = coord + LEVEL_BOUNDARY_MAX;
    if c < 0 {
        c = 0;
    }
    let mut index = c / CELL_SIZE;
    // Include extra cell if close to boundary.
    if c % CELL_SIZE < 50 {
        index -= 1;
    }
    if index < 0 {
        index = 0;
    }
    index
}

/// `upper_cell_index` (`surface_load.c`).
fn upper_cell_index(coord: i32) -> i32 {
    let mut c = coord + LEVEL_BOUNDARY_MAX;
    if c < 0 {
        c = 0;
    }
    let mut index = c / CELL_SIZE;
    if c % CELL_SIZE > CELL_SIZE - 50 {
        index += 1;
    }
    if index > NUM_CELLS_INDEX {
        index = NUM_CELLS_INDEX;
    }
    index
}

/// Query cell for a position (`find_floor`/`find_ceil`/`find_wall_collisions`).
/// The decomp truncates to `s16` (`TerrainData`) before the lookup.
fn query_cell(v: f32) -> Option<i32> {
    let t = v as i16;
    if t <= -LEVEL_BOUNDARY_MAX as i16 || t >= LEVEL_BOUNDARY_MAX as i16 {
        return None;
    }
    Some(((t as i32 + LEVEL_BOUNDARY_MAX) / CELL_SIZE) & NUM_CELLS_INDEX)
}

/// Decomp normal's y component (sign only), for the ramp winding fix-up.
/// Mirrors `stepkit_trace::geom::decomp_normal` (`(v2-v1)x(v3-v2)`, i64).
fn decomp_normal_y(v1: [i32; 3], v2: [i32; 3], v3: [i32; 3]) -> i64 {
    let ax = v2[0] as i64 - v1[0] as i64;
    let ay = v2[1] as i64 - v1[1] as i64;
    let az = v2[2] as i64 - v1[2] as i64;
    let bx = v3[0] as i64 - v2[0] as i64;
    let by = v3[1] as i64 - v2[1] as i64;
    let bz = v3[2] as i64 - v2[2] as i64;
    az * bx - ax * bz
}

impl CollisionWorld for SurfaceWorld {
    fn find_floor(&self, x: f32, y: f32, z: f32) -> Option<FloorHit> {
        // decomp: src/engine/surface_collision.c: find_floor
        // "Parallel Universes": position is cast to s16.
        let xi = x as i16;
        let yi = y as i16;
        let zi = z as i16;
        if xi <= -LEVEL_BOUNDARY_MAX as i16 || xi >= LEVEL_BOUNDARY_MAX as i16 {
            return None;
        }
        if zi <= -LEVEL_BOUNDARY_MAX as i16 || zi >= LEVEL_BOUNDARY_MAX as i16 {
            return None;
        }
        let cell_x = ((xi as i32 + LEVEL_BOUNDARY_MAX) / CELL_SIZE) & NUM_CELLS_INDEX;
        let cell_z = ((zi as i32 + LEVEL_BOUNDARY_MAX) / CELL_SIZE) & NUM_CELLS_INDEX;
        // No dynamic surfaces in this backend; the static list only.
        let list = &self.cells[cell_z as usize][cell_x as usize][SP_FLOORS];
        let (mut floor, mut height) =
            self.find_floor_from_list(list, xi as i32, yi as i32, zi as i32);
        // SURFACE_INTANGIBLE re-search (`gFindFloorIncludeSurfaceIntangible`
        // is FALSE in the oracle harness, so this branch is always live).
        // Decomp quirk: when the re-search finds nothing, the returned
        // height is still the intangible floor's height.
        if floor.is_some_and(|i| self.surfaces[i].surf_type == surface_type::INTANGIBLE as i16) {
            let (f2, h2) =
                self.find_floor_from_list(list, xi as i32, (height - 200.0) as i32, zi as i32);
            floor = f2;
            if f2.is_some() {
                height = h2;
            }
        }
        floor.map(|i| self.floor_hit_of(i, height))
    }

    fn find_ceil(&self, x: f32, y: f32, z: f32) -> Option<CeilingHit> {
        // decomp: src/engine/surface_collision.c: find_ceil
        let xi = x as i16;
        let yi = y as i16;
        let zi = z as i16;
        if xi <= -LEVEL_BOUNDARY_MAX as i16 || xi >= LEVEL_BOUNDARY_MAX as i16 {
            return None;
        }
        if zi <= -LEVEL_BOUNDARY_MAX as i16 || zi >= LEVEL_BOUNDARY_MAX as i16 {
            return None;
        }
        let cell_x = ((xi as i32 + LEVEL_BOUNDARY_MAX) / CELL_SIZE) & NUM_CELLS_INDEX;
        let cell_z = ((zi as i32 + LEVEL_BOUNDARY_MAX) / CELL_SIZE) & NUM_CELLS_INDEX;
        let list = &self.cells[cell_z as usize][cell_x as usize][SP_CEILS];
        let (ceil, height) = self.find_ceil_from_list(list, xi as i32, yi as i32, zi as i32);
        ceil.map(|i| self.ceil_hit_of(i, height))
    }

    fn find_wall_collisions(
        &self,
        x: f32,
        y: f32,
        z: f32,
        offset: f32,
        radius: f32,
    ) -> WallCollision {
        // decomp: src/engine/surface_collision.c: find_wall_collisions,
        // find_wall_collisions_from_list; src/game/mario.c:521
        // resolve_and_return_wall_collisions.
        let cell_x = match query_cell(x) {
            Some(c) => c,
            None => {
                return WallCollision {
                    pos: Vec3::new(x, y, z),
                    wall: None,
                    num_walls: 0,
                }
            }
        };
        let cell_z = match query_cell(z) {
            Some(c) => c,
            None => {
                return WallCollision {
                    pos: Vec3::new(x, y, z),
                    wall: None,
                    num_walls: 0,
                }
            }
        };
        // Max collision radius = 200.
        let radius = radius.min(200.0);
        let yw = y + offset;
        let mut data_x = x;
        let mut data_z = z;
        let mut walls: [Option<usize>; 4] = [None; 4];
        let mut num_walls = 0usize;
        // No dynamic surfaces in this backend; the static wall list only.
        // NOTE: the local x/z are intentionally NOT updated between walls
        // ("wall overlaps"): every wall pushes from the same base position.
        for &idx in &self.cells[cell_z as usize][cell_x as usize][SP_WALLS] {
            let s = &self.surfaces[idx];
            // Exclude a large number of walls immediately to optimize.
            if yw < s.lower_y as f32 || yw > s.upper_y as f32 {
                continue;
            }
            let off = s.normal[0] * x + s.normal[1] * yw + s.normal[2] * z + s.origin_offset;
            if off < -radius || off > radius {
                continue;
            }
            if !Self::wall_contains(s, x, yw, z) {
                continue;
            }
            // Never camera-checking here: ignore camera-only surfaces.
            if s.surf_type as u16 == surface_type::CAMERA_BOUNDARY {
                continue;
            }
            // Vanish-cap walls: the harness leaves `gCurrentObject` NULL,
            // so neither vanish-cap skip applies; they collide normally.
            data_x += s.normal[0] * (radius - off);
            data_z += s.normal[2] * (radius - off);
            // Only the first four walls are referenced ("unreferenced walls").
            if num_walls < 4 {
                walls[num_walls] = Some(idx);
                num_walls += 1;
            }
        }
        let wall = if num_walls > 0 {
            walls[num_walls - 1].map(|i| self.wall_hit_of(i))
        } else {
            None
        };
        WallCollision {
            pos: Vec3::new(data_x, y, data_z),
            wall,
            num_walls: num_walls as u32,
        }
    }

    fn find_water_level(&self, x: f32, z: f32) -> f32 {
        // decomp: src/engine/surface_collision.c: find_water_level.
        // Note: x/z are NOT truncated to s16 here.
        let mut water_level = FLOOR_LOWER_LIMIT;
        for r in &self.water {
            // Water is less than 50 val only, while above is gas and such.
            if r.lo_x < x && x < r.hi_x && r.lo_z < z && z < r.hi_z && r.val < 50 {
                // Only the first height is returned.
                water_level = r.level;
                break;
            }
        }
        water_level
    }

    fn wall_probe(&self, point: Vec3, radius: f32) -> bool {
        // Legacy sensing for the pre-Phase-4 ledge grab (kept for the
        // anchors test): closest-point distance to any wall triangle.
        // Not a decomp operation.
        let xi = point.x as i16;
        let zi = point.z as i16;
        if xi <= -LEVEL_BOUNDARY_MAX as i16
            || xi >= LEVEL_BOUNDARY_MAX as i16
            || zi <= -LEVEL_BOUNDARY_MAX as i16
            || zi >= LEVEL_BOUNDARY_MAX as i16
        {
            return false;
        }
        let cell_x = ((xi as i32 + LEVEL_BOUNDARY_MAX) / CELL_SIZE) & NUM_CELLS_INDEX;
        let cell_z = ((zi as i32 + LEVEL_BOUNDARY_MAX) / CELL_SIZE) & NUM_CELLS_INDEX;
        for &idx in &self.cells[cell_z as usize][cell_x as usize][SP_WALLS] {
            let s = &self.surfaces[idx];
            let a = Vec3::new(s.v[0][0] as f32, s.v[0][1] as f32, s.v[0][2] as f32);
            let b = Vec3::new(s.v[1][0] as f32, s.v[1][1] as f32, s.v[1][2] as f32);
            let c = Vec3::new(s.v[2][0] as f32, s.v[2][1] as f32, s.v[2][2] as f32);
            if (point - closest_point_on_triangle(point, a, b, c)).length() < radius {
                return true;
            }
        }
        false
    }

    fn terrain_type(&self) -> u16 {
        self.terrain
    }
}

/// Closest point on a triangle to p (Ericson's algorithm, f32).
fn closest_point_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return a + ab * v;
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return a + ac * w;
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return b + (c - b) * w;
    }
    let denom = 1.0 / (va + vb + vc).max(1e-12);
    let v = vb * denom;
    let w = vc * denom;
    a + ab * v + ac * w
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_world() -> SurfaceWorld {
        let mut w = SurfaceWorld::new();
        w.add_box(
            Vec3::new(-2000.0, -100.0, -2000.0),
            Vec3::new(2000.0, 0.0, 2000.0),
            crate::world::surface_type::DEFAULT,
            0,
        );
        w
    }

    #[test]
    fn floor_found_on_box_top() {
        let w = flat_world();
        let hit = w.find_floor(10.0, 5.0, -3.0).unwrap();
        assert!((hit.y - 0.0).abs() < 1e-3);
        assert!(hit.normal.y > 0.99);
    }

    #[test]
    fn floor_signed_zero_matches_decomp() {
        // The decomp's originOffset arithmetic yields -0.0 for floors
        // through the origin; the diff compares bitwise.
        let w = flat_world();
        let hit = w.find_floor(0.0, 0.0, 0.0).unwrap();
        assert_eq!(hit.y.to_bits(), (-0.0f32).to_bits());
    }

    #[test]
    fn no_floor_below_world() {
        let w = flat_world();
        assert!(w.find_floor(0.0, -500.0, 0.0).is_none());
    }

    #[test]
    fn wall_pushes_out() {
        let mut w = flat_world();
        w.add_box(
            Vec3::new(-200.0, 0.0, 300.0),
            Vec3::new(200.0, 200.0, 320.0),
            crate::world::surface_type::DEFAULT,
            0,
        );
        let r = w.find_wall_collisions(0.0, 0.0, 290.0, 80.0, 50.0);
        assert!(r.wall.is_some());
        // Decomp "wall overlaps": the offset is computed from the original
        // position for every wall, so both the near and far faces push
        // (the far face wins here). What matters is that the wall is
        // referenced and the position moved.
        assert!((r.pos.z - 290.0).abs() > 1.0, "pushed: {}", r.pos.z);
    }

    #[test]
    fn single_wall_pushes_back() {
        // A single wall triangle pushes away from the wall.
        let mut w = SurfaceWorld::new();
        w.add_triangle(
            [100, 0, 300],
            [-100, 0, 300],
            [-100, 200, 300],
            crate::world::surface_type::DEFAULT,
            0,
        );
        w.add_triangle(
            [100, 0, 300],
            [-100, 200, 300],
            [100, 200, 300],
            crate::world::surface_type::DEFAULT,
            0,
        );
        let r = w.find_wall_collisions(0.0, 0.0, 290.0, 80.0, 50.0);
        assert!(r.wall.is_some());
        assert!(r.pos.z < 290.0, "pushed back: {}", r.pos.z);
    }

    #[test]
    fn ramp_classifies_as_floor() {
        let mut w = SurfaceWorld::new();
        w.add_ramp(
            Vec3::new(0.0, 0.0, -100.0),
            Vec3::new(0.0, 0.0, 1.0),
            400.0,
            70.0,
            400.0,
            crate::world::surface_type::DEFAULT,
            0,
        );
        let hit = w.find_floor(0.0, 50.0, 100.0).unwrap();
        assert!(hit.normal.y > 0.9, "normal {:?}", hit.normal);
        assert!((hit.y - 35.0).abs() < 1.0, "y {}", hit.y);
    }

    #[test]
    fn steep_quad_classifies_as_wall() {
        let mut w = SurfaceWorld::new();
        // Vertical quad.
        w.add_triangle_f(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 100.0, 0.0),
            Vec3::new(0.0, 100.0, 100.0),
            crate::world::surface_type::DEFAULT,
            0,
        );
        assert!(w.find_floor(1.0, 50.0, 50.0).is_none());
    }

    #[test]
    fn ceil_above_floor_height_plus_80() {
        // find_ceil is queried at floorHeight + 80 (ST-05), not above Mario.
        let mut w = SurfaceWorld::new();
        w.add_box(
            Vec3::new(-200.0, 300.0, -200.0),
            Vec3::new(200.0, 320.0, 200.0),
            crate::world::surface_type::DEFAULT,
            0,
        );
        // Overhead box bottom (y=300) is a ceiling.
        let hit = w.find_ceil(0.0, 0.0 + 80.0, 0.0).unwrap();
        assert!((hit.y - 300.0).abs() < 1e-3, "y {}", hit.y);
    }
}
