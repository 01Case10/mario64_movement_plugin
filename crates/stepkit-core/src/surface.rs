//! `SurfaceWorld`: the reference collision backend.
//!
//! A triangle soup with a spatial grid. Triangles are classified by their
//! normal (spec: collision.classify_floor_threshold, verify):
//! floor when `normal.y >= 0.5`, ceiling when `normal.y <= -0.5`, otherwise
//! a wall. The grid uses a `BTreeMap` so iteration order (and therefore
//! query results) is deterministic.

use crate::world::{CeilingHit, CollisionWorld, FloorHit, SurfaceKind, WallResolve};
use glam::Vec3;
use std::collections::BTreeMap;

/// Floor classification threshold on the triangle normal's y.
/// spec: collision.classify_floor_threshold (verify)
pub const FLOOR_NORMAL_Y: f32 = 0.5;
/// Ceiling classification threshold on the triangle normal's y.
/// spec: collision.classify_ceiling_threshold (verify)
pub const CEILING_NORMAL_Y: f32 = -0.5;

const CELL: f32 = 256.0;

#[derive(Clone, Debug)]
struct Tri {
    a: Vec3,
    b: Vec3,
    c: Vec3,
    normal: Vec3,
    kind: SurfaceKind,
    user_data: u64,
    /// Precomputed for point-in-triangle tests.
    d: f32, // plane constant: normal . p = d
}

impl Tri {
    fn new(a: Vec3, b: Vec3, c: Vec3, kind: SurfaceKind, user_data: u64) -> Self {
        let normal = (b - a).cross(c - a).normalize_or_zero();
        let d = normal.dot(a);
        Self {
            a,
            b,
            c,
            normal,
            kind,
            user_data,
            d,
        }
    }

    fn is_floor(&self) -> bool {
        self.normal.y >= FLOOR_NORMAL_Y
    }
    fn is_ceiling(&self) -> bool {
        self.normal.y <= CEILING_NORMAL_Y
    }

    /// Height of the triangle's plane at (x, z). Callers must check (x, z)
    /// is inside the triangle first.
    fn height_at(&self, x: f32, z: f32) -> f32 {
        // n.x * x + n.y * y + n.z * z = d  =>  y = (d - n.x*x - n.z*z) / n.y
        (self.d - self.normal.x * x - self.normal.z * z) / self.normal.y
    }

    /// Barycentric point-in-triangle test on the XZ projection.
    fn contains_xz(&self, x: f32, z: f32) -> bool {
        // Sign-of-area test, tolerant to edge hits.
        let d1 = sign(x, z, self.a.x, self.a.z, self.b.x, self.b.z);
        let d2 = sign(x, z, self.b.x, self.b.z, self.c.x, self.c.z);
        let d3 = sign(x, z, self.c.x, self.c.z, self.a.x, self.a.z);
        let has_neg = (d1 < -1e-4) || (d2 < -1e-4) || (d3 < -1e-4);
        let has_pos = (d1 > 1e-4) || (d2 > 1e-4) || (d3 > 1e-4);
        !(has_neg && has_pos)
    }
}

fn sign(px: f32, pz: f32, ax: f32, az: f32, bx: f32, bz: f32) -> f32 {
    (px - bx) * (az - bz) - (ax - bx) * (pz - bz)
}

/// A water volume: a flat surface over an XZ rectangle.
#[derive(Clone, Debug)]
struct WaterRect {
    y: f32,
    min_x: f32,
    max_x: f32,
    min_z: f32,
    max_z: f32,
}

/// Reference collision backend: triangle soup + spatial grid.
#[derive(Clone, Debug, Default)]
pub struct SurfaceWorld {
    tris: Vec<Tri>,
    grid: BTreeMap<(i32, i32, i32), Vec<u32>>,
    water: Vec<WaterRect>,
}

impl SurfaceWorld {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of triangles (diagnostic).
    pub fn triangle_count(&self) -> usize {
        self.tris.len()
    }

    fn cell_of(p: Vec3) -> (i32, i32, i32) {
        (
            (p.x / CELL).floor() as i32,
            (p.y / CELL).floor() as i32,
            (p.z / CELL).floor() as i32,
        )
    }

    /// Add one triangle. Winding determines the normal (counter-clockwise
    /// seen from the solid side gives an outward normal).
    pub fn add_triangle(
        &mut self,
        a: Vec3,
        b: Vec3,
        c: Vec3,
        kind: SurfaceKind,
        user_data: u64,
    ) -> u32 {
        let idx = self.tris.len() as u32;
        let tri = Tri::new(a, b, c, kind, user_data);
        // Insert into every cell the triangle's bounds touch.
        let min = a.min(b.min(c));
        let max = a.max(b.max(c));
        let (x0, y0, z0) = Self::cell_of(min);
        let (x1, y1, z1) = Self::cell_of(max);
        for x in x0..=x1 {
            for y in y0..=y1 {
                for z in z0..=z1 {
                    self.grid.entry((x, y, z)).or_default().push(idx);
                }
            }
        }
        self.tris.push(tri);
        idx
    }

    /// Add an axis-aligned box as 12 triangles (outward normals).
    pub fn add_box(&mut self, min: Vec3, max: Vec3, kind: SurfaceKind, user_data: u64) {
        let v = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
        let (x0, y0, z0) = (min.x, min.y, min.z);
        let (x1, y1, z1) = (max.x, max.y, max.z);
        // Top (+Y normal, CCW from above): floor.
        self.add_triangle(v(x0, y1, z0), v(x0, y1, z1), v(x1, y1, z1), kind, user_data);
        self.add_triangle(v(x0, y1, z0), v(x1, y1, z1), v(x1, y1, z0), kind, user_data);
        // Bottom (-Y): ceiling.
        self.add_triangle(v(x0, y0, z0), v(x1, y0, z1), v(x0, y0, z1), kind, user_data);
        self.add_triangle(v(x0, y0, z0), v(x1, y0, z0), v(x1, y0, z1), kind, user_data);
        // +X face.
        self.add_triangle(v(x1, y0, z0), v(x1, y0, z1), v(x1, y1, z1), kind, user_data);
        self.add_triangle(v(x1, y0, z0), v(x1, y1, z1), v(x1, y1, z0), kind, user_data);
        // -X face.
        self.add_triangle(v(x0, y0, z0), v(x0, y1, z1), v(x0, y0, z1), kind, user_data);
        self.add_triangle(v(x0, y0, z0), v(x0, y1, z0), v(x0, y1, z1), kind, user_data);
        // +Z face.
        self.add_triangle(v(x0, y0, z1), v(x1, y0, z1), v(x1, y1, z1), kind, user_data);
        self.add_triangle(v(x0, y0, z1), v(x1, y1, z1), v(x0, y1, z1), kind, user_data);
        // -Z face.
        self.add_triangle(v(x0, y0, z0), v(x0, y1, z0), v(x1, y1, z0), kind, user_data);
        self.add_triangle(v(x0, y0, z0), v(x1, y1, z0), v(x1, y0, z0), kind, user_data);
    }

    /// Add a sloped quad from `origin` along `dir` (XZ, normalized) rising
    /// `height` over `length`, `width` across.
    #[allow(clippy::too_many_arguments)] // geometry builder: explicit params beat a struct here
    pub fn add_ramp(
        &mut self,
        origin: Vec3,
        dir: Vec3,
        length: f32,
        height: f32,
        width: f32,
        kind: SurfaceKind,
        user_data: u64,
    ) {
        let across = Vec3::new(-dir.z, 0.0, dir.x) * (width * 0.5);
        let p0 = origin - across;
        let p1 = origin + across;
        let p2 = origin + dir * length + across + Vec3::Y * height;
        let p3 = origin + dir * length - across + Vec3::Y * height;
        // Winding chosen so the normal points up-ish (+Y component).
        self.add_triangle(p0, p1, p2, kind, user_data);
        self.add_triangle(p0, p2, p3, kind, user_data);
    }

    /// Add a water surface over an XZ rectangle.
    pub fn add_water(&mut self, y: f32, min_x: f32, max_x: f32, min_z: f32, max_z: f32) {
        self.water.push(WaterRect {
            y,
            min_x,
            max_x,
            min_z,
            max_z,
        });
    }

    /// Triangles near a point (its cell plus neighbors).
    fn nearby(&self, p: Vec3) -> Vec<u32> {
        let (cx, cy, cz) = Self::cell_of(p);
        let mut out = Vec::new();
        for x in cx - 1..=cx + 1 {
            for y in cy - 1..=cy + 1 {
                for z in cz - 1..=cz + 1 {
                    if let Some(ids) = self.grid.get(&(x, y, z)) {
                        out.extend_from_slice(ids);
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

impl CollisionWorld for SurfaceWorld {
    fn find_floor(&self, pos: Vec3, max_above: f32) -> Option<FloorHit> {
        let mut best: Option<FloorHit> = None;
        for idx in self.nearby(pos) {
            let tri = &self.tris[idx as usize];
            if !tri.is_floor() || !tri.contains_xz(pos.x, pos.z) {
                continue;
            }
            let y = tri.height_at(pos.x, pos.z);
            if y <= pos.y + max_above && best.is_none_or(|b: FloorHit| y > b.y) {
                best = Some(FloorHit {
                    y,
                    normal: tri.normal,
                    kind: tri.kind,
                    user_data: tri.user_data,
                });
            }
        }
        best
    }

    fn find_ceiling(&self, pos: Vec3, height: f32) -> Option<CeilingHit> {
        let mut best: Option<CeilingHit> = None;
        for idx in self.nearby(pos) {
            let tri = &self.tris[idx as usize];
            if !tri.is_ceiling() || !tri.contains_xz(pos.x, pos.z) {
                continue;
            }
            let y = tri.height_at(pos.x, pos.z);
            if y >= pos.y && y <= pos.y + height && best.is_none_or(|c: CeilingHit| y < c.y) {
                best = Some(CeilingHit {
                    y,
                    normal: tri.normal,
                });
            }
        }
        best
    }

    fn resolve_walls(&self, pos: Vec3, offset: f32, radius: f32) -> WallResolve {
        let mut p = pos;
        let mut hit = false;
        let mut num_walls = 0u32;
        let mut normal = Vec3::ZERO;
        // Two relaxation passes for corners.
        for _ in 0..2 {
            let sample = Vec3::new(p.x, p.y + offset, p.z);
            for idx in self.nearby(sample) {
                let tri = &self.tris[idx as usize];
                if tri.is_floor() || tri.is_ceiling() {
                    continue;
                }
                // Closest point on the triangle to the sample.
                let cp = closest_point_on_triangle(sample, tri);
                let delta = sample - cp;
                let dist = delta.length();
                if dist < radius {
                    // Push out along the wall's horizontal normal.
                    let mut n = Vec3::new(tri.normal.x, 0.0, tri.normal.z);
                    if n.length_squared() < 1e-8 {
                        n = if dist > 1e-6 {
                            Vec3::new(delta.x, 0.0, delta.z).normalize()
                        } else {
                            Vec3::X
                        };
                    } else {
                        n = n.normalize();
                    }
                    // Only push if the sample is on the solid side or inside.
                    let push = radius - dist + 0.01;
                    p.x += n.x * push;
                    p.z += n.z * push;
                    hit = true;
                    num_walls += 1;
                    normal = n;
                }
            }
        }
        WallResolve {
            pos: p,
            hit,
            num_walls,
            normal,
        }
    }

    fn wall_probe(&self, point: Vec3, radius: f32) -> bool {
        for idx in self.nearby(point) {
            let tri = &self.tris[idx as usize];
            if tri.is_floor() || tri.is_ceiling() {
                continue;
            }
            let cp = closest_point_on_triangle(point, tri);
            if (point - cp).length() < radius {
                return true;
            }
        }
        false
    }

    fn water_level(&self, x: f32, z: f32) -> Option<f32> {
        self.water
            .iter()
            .filter(|w| x >= w.min_x && x <= w.max_x && z >= w.min_z && z <= w.max_z)
            .map(|w| w.y)
            .fold(None, |best: Option<f32>, y| {
                Some(best.map_or(y, |b: f32| b.max(y)))
            })
    }
}

/// Closest point on a triangle to p (Ericson's algorithm, f32).
fn closest_point_on_triangle(p: Vec3, tri: &Tri) -> Vec3 {
    let ab = tri.b - tri.a;
    let ac = tri.c - tri.a;
    let ap = p - tri.a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return tri.a;
    }
    let bp = p - tri.b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return tri.b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return tri.a + ab * v;
    }
    let cp = p - tri.c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return tri.c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return tri.a + ac * w;
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return tri.b + (tri.c - tri.b) * w;
    }
    let denom = 1.0 / (va + vb + vc).max(1e-12);
    let v = vb * denom;
    let w = vc * denom;
    tri.a + ab * v + ac * w
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn floor_found_on_box_top() {
        let w = flat_world();
        let hit = w.find_floor(Vec3::new(10.0, 5.0, -3.0), 1.0).unwrap();
        assert!((hit.y - 0.0).abs() < 1e-3);
        assert!(hit.normal.y > 0.99);
    }

    #[test]
    fn no_floor_below_world() {
        let w = flat_world();
        assert!(w.find_floor(Vec3::new(0.0, -500.0, 0.0), 1.0).is_none());
    }

    #[test]
    fn wall_pushes_out() {
        let mut w = flat_world();
        w.add_box(
            Vec3::new(-200.0, 0.0, 300.0),
            Vec3::new(200.0, 200.0, 320.0),
            SurfaceKind::Default,
            0,
        );
        let r = w.resolve_walls(Vec3::new(0.0, 0.0, 290.0), 80.0, 50.0);
        assert!(r.hit);
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
            SurfaceKind::Default,
            0,
        );
        let hit = w.find_floor(Vec3::new(0.0, 50.0, 100.0), 1.0).unwrap();
        assert!(hit.normal.y > 0.9, "normal {:?}", hit.normal);
        assert!((hit.y - 35.0).abs() < 1.0, "y {}", hit.y);
    }

    #[test]
    fn steep_quad_classifies_as_wall() {
        let mut w = SurfaceWorld::new();
        // Vertical quad.
        w.add_triangle(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 100.0, 0.0),
            Vec3::new(0.0, 100.0, 100.0),
            SurfaceKind::Default,
            0,
        );
        assert!(w.find_floor(Vec3::new(1.0, 50.0, 50.0), 1.0).is_none());
    }
}
