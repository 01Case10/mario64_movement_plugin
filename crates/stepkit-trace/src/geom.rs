//! Integer-geometry contracts for the decomp oracle (plan §6 Phase 2, ST-02).
//!
//! The decomp's surface loader takes `s16` vertices, so every coordinate
//! handed to the oracle must already be integral and within ±8191. This
//! module is STRICT: nothing rounds, snaps, or truncates. Non-integral or
//! out-of-range coordinates are errors, so a scenario that converts is
//! guaranteed to mean the same geometry on both sides (plugin float world
//! and oracle integer world).

use crate::scenario::{BoxGeom, RampGeom};

/// The oracle takes s16-style integer coordinates.
pub const COORD_LIMIT: f32 = 8191.0;

/// Strict integer coordinate: must already be integral and within ±8191.
pub fn int_coord(v: f32, what: &str) -> Result<i32, String> {
    if !v.is_finite() {
        return Err(format!("{what}={v} is not finite"));
    }
    if v.fract() != 0.0 {
        return Err(format!("{what}={v} is not integral"));
    }
    if v.abs() > COORD_LIMIT {
        return Err(format!("{what}={v} is outside ±{COORD_LIMIT}"));
    }
    Ok(v as i32)
}

/// Map a scenario surface `kind` to a decomp `SURFACE_*` value. Values from
/// `oracle/sm64/include/surface_terrains.h`.
pub fn surface_type(kind: u8) -> Result<i32, String> {
    match kind {
        0 => Ok(0x0000), // SURFACE_DEFAULT
        1 => Ok(0x0014), // SURFACE_SLIPPERY
        2 => Ok(0x0015), // SURFACE_NOT_SLIPPERY
        3 => Ok(0x0013), // SURFACE_VERY_SLIPPERY
        4 => Ok(0x0021), // SURFACE_SHALLOW_QUICKSAND
        5 => Ok(0x0022), // SURFACE_DEEP_QUICKSAND
        6 => Ok(0x0027), // SURFACE_MOVING_QUICKSAND
        7 => Ok(0x0009), // SURFACE_SLOW
        _ => Err(format!("unsupported surface kind {kind}")),
    }
}

/// Integer-vertex triangle mesh for the oracle's `.oin` geometry section.
#[derive(Default)]
pub struct Mesh {
    /// Interned integer vertices.
    pub verts: Vec<(i32, i32, i32)>,
    /// (surface type, i0, i1, i2)
    pub tris: Vec<(i32, usize, usize, usize)>,
}

impl Mesh {
    /// Intern a vertex, deduplicating identical coordinates.
    pub fn vert(&mut self, v: (i32, i32, i32)) -> usize {
        match self.verts.iter().position(|&w| w == v) {
            Some(i) => i,
            None => {
                self.verts.push(v);
                self.verts.len() - 1
            }
        }
    }

    pub fn tri(&mut self, surf: i32, a: usize, b: usize, c: usize) {
        self.tris.push((surf, a, b, c));
    }
}

/// Decomp `read_surface_data` normal: (v2-v1)x(v3-v2).
pub fn decomp_normal(
    v1: (i32, i32, i32),
    v2: (i32, i32, i32),
    v3: (i32, i32, i32),
) -> (f32, f32, f32) {
    let ax = v2.0 as i64 - v1.0 as i64;
    let ay = v2.1 as i64 - v1.1 as i64;
    let az = v2.2 as i64 - v1.2 as i64;
    let bx = v3.0 as i64 - v2.0 as i64;
    let by = v3.1 as i64 - v2.1 as i64;
    let bz = v3.2 as i64 - v2.2 as i64;
    (
        (ay * bz - az * by) as f32,
        (az * bx - ax * bz) as f32,
        (ax * by - ay * bx) as f32,
    )
}

/// Box corner order used below:
/// 0:(minx,miny,minz) 1:(maxx,miny,minz) 2:(maxx,miny,maxz) 3:(minx,miny,maxz)
/// 4:(minx,maxy,minz) 5:(maxx,maxy,minz) 6:(maxx,maxy,maxz) 7:(minx,maxy,maxz)
pub fn push_box(mesh: &mut Mesh, b: &BoxGeom) -> Result<(), String> {
    let surf = surface_type(b.kind)?;
    let minx = int_coord(b.min.0, "box.min.x")?;
    let miny = int_coord(b.min.1, "box.min.y")?;
    let minz = int_coord(b.min.2, "box.min.z")?;
    let maxx = int_coord(b.max.0, "box.max.x")?;
    let maxy = int_coord(b.max.1, "box.max.y")?;
    let maxz = int_coord(b.max.2, "box.max.z")?;
    let corners = [
        (minx, miny, minz),
        (maxx, miny, minz),
        (maxx, miny, maxz),
        (minx, miny, maxz),
        (minx, maxy, minz),
        (maxx, maxy, minz),
        (maxx, maxy, maxz),
        (minx, maxy, maxz),
    ];
    let idx: Vec<usize> = corners.iter().map(|&v| mesh.vert(v)).collect();
    // Each (v1,v2,v3) triple was verified by hand so that the decomp normal
    // (v2-v1)x(v3-v2) points outward on its face. The bottom face IS emitted
    // (unlike the old oracle_cmd version): a box placed overhead must act
    // as a ceiling for the oracle's find_ceil. (6 faces x 2 triangles = 12.)
    const TRIS: [(usize, usize, usize); 12] = [
        (4, 7, 6),
        (4, 6, 5), // top (+y)
        (0, 1, 2),
        (0, 2, 3), // bottom (-y)
        (1, 0, 4),
        (1, 4, 5), // z=min wall (-z)
        (3, 2, 6),
        (3, 6, 7), // z=max wall (+z)
        (0, 3, 7),
        (0, 7, 4), // x=min wall (-x)
        (2, 1, 5),
        (2, 5, 6), // x=max wall (+x)
    ];
    for &(a, b2, c) in &TRIS {
        mesh.tri(surf, idx[a], idx[b2], idx[c]);
    }
    Ok(())
}

pub fn push_ramp(mesh: &mut Mesh, r: &RampGeom) -> Result<(), String> {
    let surf = surface_type(r.kind)?;
    // Strict: every ramp parameter must already be integral. (The corner
    // check below also catches non-axis-aligned dirs that would produce
    // fractional corners.)
    let ox = int_coord(r.origin.0, "ramp.origin.x")?;
    let oy = int_coord(r.origin.1, "ramp.origin.y")?;
    let oz = int_coord(r.origin.2, "ramp.origin.z")?;
    let length = int_coord(r.length, "ramp.length")?;
    let height = int_coord(r.height, "ramp.height")?;
    let width = int_coord(r.width, "ramp.width")?;
    let len = (r.dir.0 * r.dir.0 + r.dir.1 * r.dir.1).sqrt();
    if !len.is_finite() || len <= 0.0 {
        return Err("ramp dir has zero length".into());
    }
    let dirn = (r.dir.0 / len, r.dir.1 / len);
    // Horizontal perpendicular, already unit because dirn is unit.
    let (px, pz) = (-dirn.1, dirn.0);
    let hw = width as f32 / 2.0;
    let c0f = (ox as f32 - px * hw, oy as f32, oz as f32 - pz * hw);
    let c1f = (ox as f32 + px * hw, oy as f32, oz as f32 + pz * hw);
    let c2f = (
        c0f.0 + dirn.0 * length as f32,
        c0f.1 + height as f32,
        c0f.2 + dirn.1 * length as f32,
    );
    let c3f = (
        c1f.0 + dirn.0 * length as f32,
        c1f.1 + height as f32,
        c1f.2 + dirn.1 * length as f32,
    );
    let c0 = (
        int_coord(c0f.0, "ramp.c0.x")?,
        int_coord(c0f.1, "ramp.c0.y")?,
        int_coord(c0f.2, "ramp.c0.z")?,
    );
    let c1 = (
        int_coord(c1f.0, "ramp.c1.x")?,
        int_coord(c1f.1, "ramp.c1.y")?,
        int_coord(c1f.2, "ramp.c1.z")?,
    );
    let c2 = (
        int_coord(c2f.0, "ramp.c2.x")?,
        int_coord(c2f.1, "ramp.c2.y")?,
        int_coord(c2f.2, "ramp.c2.z")?,
    );
    let c3 = (
        int_coord(c3f.0, "ramp.c3.x")?,
        int_coord(c3f.1, "ramp.c3.y")?,
        int_coord(c3f.2, "ramp.c3.z")?,
    );
    let i0 = mesh.vert(c0);
    let i1 = mesh.vert(c1);
    let i2 = mesh.vert(c2);
    let i3 = mesh.vert(c3);
    // Fix the winding so the decomp normal has positive y (the slope is a
    // floor, so its outward normal must point up).
    let n = decomp_normal(c0, c1, c3);
    if n.1 >= 0.0 {
        mesh.tri(surf, i0, i1, i3);
        mesh.tri(surf, i0, i3, i2);
    } else {
        mesh.tri(surf, i1, i0, i2);
        mesh.tri(surf, i1, i2, i3);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::Geometry;

    fn sample_box() -> BoxGeom {
        BoxGeom {
            min: (-2000.0, -100.0, -2000.0),
            max: (2000.0, 0.0, 2000.0),
            kind: 0,
        }
    }

    #[test]
    fn box_winding_normals_point_outward() {
        let mut mesh = Mesh::default();
        push_box(&mut mesh, &sample_box()).unwrap();
        // 6 faces x 2 triangles (the bottom face is emitted so overhead
        // boxes act as ceilings for the oracle's find_ceil).
        assert_eq!(mesh.tris.len(), 12);
        let outward = [
            (0.0f32, 1.0, 0.0),
            (0.0, 1.0, 0.0), // top
            (0.0, -1.0, 0.0),
            (0.0, -1.0, 0.0), // bottom
            (0.0, 0.0, -1.0),
            (0.0, 0.0, -1.0), // z=min wall
            (0.0, 0.0, 1.0),
            (0.0, 0.0, 1.0), // z=max wall
            (-1.0, 0.0, 0.0),
            (-1.0, 0.0, 0.0), // x=min wall
            (1.0, 0.0, 0.0),
            (1.0, 0.0, 0.0), // x=max wall
        ];
        for (i, &(_, a, b, c)) in mesh.tris.iter().enumerate() {
            let n = decomp_normal(mesh.verts[a], mesh.verts[b], mesh.verts[c]);
            let len = (n.0 * n.0 + n.1 * n.1 + n.2 * n.2).sqrt();
            assert!(len > 0.0, "tri {i} is degenerate");
            let o = outward[i];
            let dot = (n.0 * o.0 + n.1 * o.1 + n.2 * o.2) / len;
            assert!(
                dot > 0.99,
                "tri {i}: normal {n:?} does not point outward {o:?} (dot={dot})"
            );
        }
    }

    #[test]
    fn ramp_normal_points_up() {
        for dir in [(0.0f32, 1.0), (1.0, 0.0), (0.0, -1.0), (-1.0, 0.0)] {
            let mut mesh = Mesh::default();
            push_ramp(
                &mut mesh,
                &RampGeom {
                    origin: (0.0, 0.0, -100.0),
                    dir,
                    length: 400.0,
                    height: 187.0,
                    width: 400.0,
                    kind: 0,
                },
            )
            .unwrap();
            assert_eq!(mesh.tris.len(), 2);
            for &(_, a, b, c) in &mesh.tris {
                let n = decomp_normal(mesh.verts[a], mesh.verts[b], mesh.verts[c]);
                assert!(n.1 > 0.0, "dir {dir:?}: ramp normal {n:?} has no +y");
            }
        }
        // Downhill ramp: the surface normal still has positive y.
        let mut mesh = Mesh::default();
        push_ramp(
            &mut mesh,
            &RampGeom {
                origin: (0.0, 0.0, 0.0),
                dir: (0.0, 1.0),
                length: 400.0,
                height: -107.0,
                width: 300.0,
                kind: 0,
            },
        )
        .unwrap();
        for &(_, a, b, c) in &mesh.tris {
            let n = decomp_normal(mesh.verts[a], mesh.verts[b], mesh.verts[c]);
            assert!(n.1 > 0.0, "downhill ramp normal {n:?} has no +y");
        }
    }

    #[test]
    fn surface_type_maps_all_kinds() {
        assert_eq!(surface_type(0).unwrap(), 0x0000);
        assert_eq!(surface_type(1).unwrap(), 0x0014);
        assert_eq!(surface_type(2).unwrap(), 0x0015);
        assert_eq!(surface_type(3).unwrap(), 0x0013);
        assert_eq!(surface_type(4).unwrap(), 0x0021);
        assert_eq!(surface_type(5).unwrap(), 0x0022);
        assert_eq!(surface_type(6).unwrap(), 0x0027);
        assert_eq!(surface_type(7).unwrap(), 0x0009);
        assert!(surface_type(8).is_err());
        assert!(surface_type(255).is_err());
    }

    #[test]
    fn non_integral_box_coord_errors() {
        let mut mesh = Mesh::default();
        let mut bad = sample_box();
        bad.min.0 = -2000.5;
        let err = push_box(&mut mesh, &bad).unwrap_err();
        assert!(err.contains("-2000.5"), "{err}");
        assert!(err.contains("not integral"), "{err}");
    }

    #[test]
    fn out_of_range_box_coord_errors() {
        let mut mesh = Mesh::default();
        let mut bad = sample_box();
        bad.max.0 = 9000.0;
        let err = push_box(&mut mesh, &bad).unwrap_err();
        assert!(err.contains("9000"), "{err}");
        assert!(err.contains("outside"), "{err}");
    }

    #[test]
    fn non_integral_ramp_height_errors() {
        let mut mesh = Mesh::default();
        let err = push_ramp(
            &mut mesh,
            &RampGeom {
                origin: (0.0, 0.0, -100.0),
                dir: (0.0, 1.0),
                length: 400.0,
                height: 186.52,
                width: 400.0,
                kind: 0,
            },
        )
        .unwrap_err();
        assert!(err.contains("186.52"), "{err}");
        assert!(err.contains("not integral"), "{err}");
    }

    #[test]
    fn diagonal_ramp_dir_errors() {
        // A 45-degree dir produces fractional corners: strict mode rejects.
        let mut mesh = Mesh::default();
        let err = push_ramp(
            &mut mesh,
            &RampGeom {
                origin: (0.0, 0.0, 0.0),
                dir: (1.0, 1.0),
                length: 400.0,
                height: 100.0,
                width: 400.0,
                kind: 0,
            },
        )
        .unwrap_err();
        assert!(err.contains("not integral"), "{err}");
    }

    #[test]
    fn box_vertex_dedup() {
        let mut mesh = Mesh::default();
        push_box(&mut mesh, &sample_box()).unwrap();
        push_box(&mut mesh, &sample_box()).unwrap();
        assert_eq!(mesh.verts.len(), 8);
        assert_eq!(mesh.tris.len(), 24);
    }

    #[test]
    fn geometry_enum_dispatch() {
        // Sanity: the Geometry enum dispatches through both pushers.
        let mut mesh = Mesh::default();
        for g in [
            Geometry::Box(sample_box()),
            Geometry::Ramp(RampGeom {
                origin: (0.0, 0.0, 0.0),
                dir: (0.0, 1.0),
                length: 100.0,
                height: 50.0,
                width: 100.0,
                kind: 3,
            }),
        ] {
            match &g {
                Geometry::Box(b) => push_box(&mut mesh, b).unwrap(),
                Geometry::Ramp(r) => push_ramp(&mut mesh, r).unwrap(),
            }
        }
        assert_eq!(mesh.tris.len(), 14);
    }
}
