//! `StepWorld3D`: bakes Godot geometry into a stepkit [`SurfaceWorld`].
#![allow(clippy::redundant_field_names)] // derive-generated init

use godot::classes::ArrayMesh;
use godot::prelude::*;
use stepkit_core::surface::SurfaceWorld;
use stepkit_core::world::SurfaceKind;

fn kind_of(kind: i64) -> SurfaceKind {
    match kind {
        1 => SurfaceKind::Slide,
        2 => SurfaceKind::Quicksand,
        _ => SurfaceKind::Default,
    }
}

#[derive(GodotClass)]
#[class(base=Node3D, init)]
pub struct StepWorld3D {
    /// Godot meters per SM64 unit (0.01 = 1m per 100 units, Mario is 1.6m).
    #[init(val = 0.01)]
    #[export]
    #[var]
    pub unit_scale: f32,
    world: SurfaceWorld,
    base: Base<Node3D>,
}

#[godot_api]
impl StepWorld3D {
    /// Bake an axis-aligned box (in global coordinates).
    #[func]
    pub fn bake_box(&mut self, min: Vector3, max: Vector3, kind: i64) {
        let s = 1.0 / self.unit_scale.max(1e-6);
        self.world.add_box(
            glam::Vec3::new(min.x * s, min.y * s, min.z * s),
            glam::Vec3::new(max.x * s, max.y * s, max.z * s),
            kind_of(kind),
            0,
        );
    }

    /// Bake a sloped quad: from `origin` along `dir` (XZ), `length` long,
    /// rising `height`, `width` across.
    #[func]
    pub fn bake_ramp(
        &mut self,
        origin: Vector3,
        dir: Vector3,
        length: f32,
        height: f32,
        width: f32,
        kind: i64,
    ) {
        let s = 1.0 / self.unit_scale.max(1e-6);
        let d = glam::Vec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
        self.world.add_ramp(
            glam::Vec3::new(origin.x * s, origin.y * s, origin.z * s),
            d,
            length * s,
            height * s,
            width * s,
            kind_of(kind),
            0,
        );
    }

    /// Bake every triangle of an `ArrayMesh` under `node`'s global transform.
    /// Only the first surface is read; the mesh must have faces.
    #[func]
    pub fn bake_mesh(&mut self, node: Gd<Node3D>, mesh: Gd<ArrayMesh>, kind: i64) {
        let s = 1.0 / self.unit_scale.max(1e-6);
        let xform = node.get_global_transform();
        let arrays = mesh.surface_get_arrays(0);
        // 0 = Mesh.ARRAY_VERTEX, 8 = Mesh.ARRAY_INDEX (stable Godot API).
        let verts: PackedVector3Array = arrays.get(0).unwrap().to();
        let index: PackedInt32Array = arrays.get(8).unwrap().to();
        let k = kind_of(kind);
        if index.is_empty() {
            for tri in verts.as_slice().as_chunks::<3>().0 {
                let a = xform * tri[0];
                let b = xform * tri[1];
                let c = xform * tri[2];
                self.world.add_triangle(
                    glam::Vec3::new(a.x * s, a.y * s, a.z * s),
                    glam::Vec3::new(b.x * s, b.y * s, b.z * s),
                    glam::Vec3::new(c.x * s, c.y * s, c.z * s),
                    k,
                    0,
                );
            }
        } else {
            let idx = index.as_slice();
            let v = verts.as_slice();
            for t in idx.as_chunks::<3>().0 {
                let a = xform * v[t[0] as usize];
                let b = xform * v[t[1] as usize];
                let c = xform * v[t[2] as usize];
                self.world.add_triangle(
                    glam::Vec3::new(a.x * s, a.y * s, a.z * s),
                    glam::Vec3::new(b.x * s, b.y * s, b.z * s),
                    glam::Vec3::new(c.x * s, c.y * s, c.z * s),
                    k,
                    0,
                );
            }
        }
    }

    /// Remove all baked geometry.
    #[func]
    pub fn clear(&mut self) {
        self.world = SurfaceWorld::new();
    }

    /// Number of baked triangles (diagnostic).
    #[func]
    pub fn triangle_count(&self) -> i64 {
        self.world.triangle_count() as i64
    }
}

impl StepWorld3D {
    /// Clone the baked world for a character to simulate against.
    pub fn baked(&self) -> SurfaceWorld {
        self.world.clone()
    }
}
