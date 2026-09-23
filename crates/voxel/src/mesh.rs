//! Surface extraction with Surface Nets.
//!
//! Every grid cell (the cube between eight neighboring samples) that the
//! surface passes through gets one vertex, at the average of the points where
//! the surface crosses the cell's edges. Every sample-to-sample edge the
//! surface crosses becomes a quad joining the vertices of the four cells
//! around that edge.
//!
//! A chunk emits the quads of edges that start at its own samples. Those quads
//! use cells one step below the chunk and samples one step above it, so
//! meshing reads a one-voxel border from the neighboring chunks. Both chunks
//! sharing a border compute the same vertices there, which makes the seams
//! between chunk meshes watertight.

use glam::{IVec3, Vec3};

use crate::{
    chunk::Chunk,
    coords::{CHUNK_SIZE, ChunkPos},
    map::ChunkMap,
    voxel::{Material, Voxel},
};

/// Samples read per axis: the chunk's own, plus one on each side.
const SAMPLES: i32 = CHUNK_SIZE + 2;
/// Cells per axis that may hold a vertex the chunk needs: `-1..CHUNK_SIZE`.
const CELLS: i32 = CHUNK_SIZE + 1;

/// Corners of a cell, numbered so that bit 0 is x, bit 1 is y and bit 2 is z.
const CORNERS: [IVec3; 8] = [
    IVec3::new(0, 0, 0),
    IVec3::new(1, 0, 0),
    IVec3::new(0, 1, 0),
    IVec3::new(1, 1, 0),
    IVec3::new(0, 0, 1),
    IVec3::new(1, 0, 1),
    IVec3::new(0, 1, 1),
    IVec3::new(1, 1, 1),
];

/// The twelve edges of a cell, as pairs of corners.
const EDGES: [(usize, usize); 12] = [
    (0, 1),
    (2, 3),
    (4, 5),
    (6, 7),
    (0, 2),
    (1, 3),
    (4, 6),
    (5, 7),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

const AXES: [IVec3; 3] = [IVec3::X, IVec3::Y, IVec3::Z];

/// A triangle mesh in the chunk's local space, where `(0, 0, 0)` is the
/// chunk's origin. Front faces wind counter-clockwise and face the air.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceMesh {
    pub positions: Vec<[f32; 3]>,
    /// Unit normals pointing out of the ground.
    pub normals: Vec<[f32; 3]>,
    /// Ground material at each vertex, for coloring or texturing.
    pub materials: Vec<Material>,
    pub indices: Vec<u32>,
}

impl SurfaceMesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

/// Extracts the terrain surface of `chunk`, or `None` if it is not loaded.
///
/// Neighbors that are not loaded are treated as a continuation of the
/// chunk's own edge, so no surface appears along unloaded borders. Remesh the
/// chunk once they arrive.
pub fn mesh_chunk(map: &ChunkMap, chunk: ChunkPos) -> Option<SurfaceMesh> {
    let samples = Samples::gather(map, chunk)?;
    let mut mesh = SurfaceMesh::default();
    if samples.all_same_side() {
        return Some(mesh);
    }
    let vertices = place_vertices(&samples, &mut mesh);
    connect_quads(&samples, &vertices, &mut mesh);
    Some(mesh)
}

/// The distances and materials of a chunk and its one-voxel border.
struct Samples {
    distances: Vec<f32>,
    materials: Vec<Material>,
}

impl Samples {
    fn gather(map: &ChunkMap, chunk: ChunkPos) -> Option<Self> {
        let center = map.get(chunk)?;
        let neighbor_index = |offset: IVec3| {
            let index = (offset.x + 1) + (offset.y + 1) * 3 + (offset.z + 1) * 9;
            usize::try_from(index).expect("offsets are within -1..=1")
        };
        let mut neighbors: [Option<&Chunk>; 27] = [None; 27];
        for z in -1..=1 {
            for y in -1..=1 {
                for x in -1..=1 {
                    let offset = IVec3::new(x, y, z);
                    neighbors[neighbor_index(offset)] = map.get(ChunkPos(chunk.0 + offset));
                }
            }
        }

        let capacity = usize::try_from(SAMPLES.pow(3)).expect("small constant");
        let mut distances = Vec::with_capacity(capacity);
        let mut materials = Vec::with_capacity(capacity);
        let last = IVec3::splat(CHUNK_SIZE - 1);
        for z in -1..=CHUNK_SIZE {
            for y in -1..=CHUNK_SIZE {
                for x in -1..=CHUNK_SIZE {
                    let sample = IVec3::new(x, y, z);
                    let offset = sample.div_euclid(IVec3::splat(CHUNK_SIZE));
                    let voxel: Voxel = match neighbors[neighbor_index(offset)] {
                        Some(neighbor) => neighbor.get(sample - offset * CHUNK_SIZE),
                        None => center.get(sample.clamp(IVec3::ZERO, last)),
                    };
                    distances.push(voxel.distance());
                    materials.push(voxel.material());
                }
            }
        }
        Some(Self {
            distances,
            materials,
        })
    }

    fn index(sample: IVec3) -> usize {
        let index = (sample.x + 1) + (sample.y + 1) * SAMPLES + (sample.z + 1) * SAMPLES * SAMPLES;
        #[expect(clippy::cast_sign_loss, reason = "samples start at -1")]
        let index = index as usize;
        index
    }

    fn distance(&self, sample: IVec3) -> f32 {
        self.distances[Self::index(sample)]
    }

    fn material(&self, sample: IVec3) -> Material {
        self.materials[Self::index(sample)]
    }

    fn all_same_side(&self) -> bool {
        let solid = self.distances[0] < 0.0;
        self.distances
            .iter()
            .all(|&distance| (distance < 0.0) == solid)
    }
}

fn cell_index(cell: IVec3) -> usize {
    let index = (cell.x + 1) + (cell.y + 1) * CELLS + (cell.z + 1) * CELLS * CELLS;
    #[expect(clippy::cast_sign_loss, reason = "cells start at -1")]
    let index = index as usize;
    index
}

/// Places one vertex in every cell the surface crosses, returning each
/// cell's vertex index (`u32::MAX` where there is none).
fn place_vertices(samples: &Samples, mesh: &mut SurfaceMesh) -> Vec<u32> {
    let cell_count = usize::try_from(CELLS.pow(3)).expect("small constant");
    let mut vertices = vec![u32::MAX; cell_count];

    for z in -1..CHUNK_SIZE {
        for y in -1..CHUNK_SIZE {
            for x in -1..CHUNK_SIZE {
                let cell = IVec3::new(x, y, z);
                let distances = CORNERS.map(|corner| samples.distance(cell + corner));
                let solid = distances.map(|distance| distance < 0.0);
                if solid.iter().all(|&s| s) || solid.iter().all(|&s| !s) {
                    continue;
                }

                let mut sum = Vec3::ZERO;
                let mut crossings = 0.0;
                for (a, b) in EDGES {
                    if solid[a] != solid[b] {
                        let t = distances[a] / (distances[a] - distances[b]);
                        sum += CORNERS[a].as_vec3().lerp(CORNERS[b].as_vec3(), t);
                        crossings += 1.0;
                    }
                }
                let offset = sum / crossings;

                // The ground material closest to the surface, so the top
                // layer of the terrain is the one that shows.
                let material = (0..8)
                    .filter(|&corner| solid[corner])
                    .max_by(|&a, &b| distances[a].total_cmp(&distances[b]))
                    .map(|corner| samples.material(cell + CORNERS[corner]))
                    .expect("a crossed cell has a solid corner");

                let index = u32::try_from(mesh.positions.len()).expect("fewer than 2³² vertices");
                vertices[cell_index(cell)] = index;
                mesh.positions.push((cell.as_vec3() + offset).to_array());
                mesh.normals.push(gradient(&distances, offset).to_array());
                mesh.materials.push(material);
            }
        }
    }
    vertices
}

/// Direction of steepest distance increase within a cell at `offset`, by
/// differentiating the trilinear interpolation of its corners.
fn gradient(distances: &[f32; 8], offset: Vec3) -> Vec3 {
    let mut gradient = Vec3::ZERO;
    for (corner, &distance) in distances.iter().enumerate() {
        let position = CORNERS[corner].as_vec3();
        // Weight of this corner along each axis, and the sign of its slope.
        let weight = Vec3::ONE - (position - offset).abs();
        let slope = position * 2.0 - Vec3::ONE;
        gradient += distance
            * Vec3::new(
                slope.x * weight.y * weight.z,
                slope.y * weight.x * weight.z,
                slope.z * weight.x * weight.y,
            );
    }
    gradient.normalize_or(Vec3::Y)
}

/// Emits a quad for every edge that starts at one of the chunk's own samples
/// and crosses the surface.
fn connect_quads(samples: &Samples, vertices: &[u32], mesh: &mut SurfaceMesh) {
    for z in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let start = IVec3::new(x, y, z);
                let start_solid = samples.distance(start) < 0.0;
                for (axis, &direction) in AXES.iter().enumerate() {
                    if start_solid == (samples.distance(start + direction) < 0.0) {
                        continue;
                    }
                    // The four cells around the edge, counter-clockwise when
                    // seen from the positive end of the axis.
                    let b = AXES[(axis + 1) % 3];
                    let c = AXES[(axis + 2) % 3];
                    let quad = [start - b - c, start - c, start, start - b]
                        .map(|cell| vertices[cell_index(cell)]);
                    debug_assert!(quad.iter().all(|&vertex| vertex != u32::MAX));
                    // Face whichever side of the edge is air.
                    let quad = if start_solid {
                        quad
                    } else {
                        [quad[0], quad[3], quad[2], quad[1]]
                    };
                    push_quad(mesh, quad);
                }
            }
        }
    }
}

/// Splits a quad along its shorter diagonal, which avoids long slivers.
fn push_quad(mesh: &mut SurfaceMesh, [a, b, c, d]: [u32; 4]) {
    let position = |vertex: u32| Vec3::from(mesh.positions[vertex as usize]);
    if position(a).distance_squared(position(c)) <= position(b).distance_squared(position(d)) {
        mesh.indices.extend([a, b, c, a, c, d]);
    } else {
        mesh.indices.extend([a, b, d, b, c, d]);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::map::tests::flat_world;

    fn triangles(mesh: &SurfaceMesh) -> impl Iterator<Item = [Vec3; 3]> + '_ {
        mesh.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|triangle| [0, 1, 2].map(|i| Vec3::from(mesh.positions[triangle[i] as usize])))
    }

    fn facing(triangle: [Vec3; 3]) -> Vec3 {
        (triangle[1] - triangle[0]).cross(triangle[2] - triangle[0])
    }

    #[test]
    fn flat_ground_meshes_to_an_upward_plane() {
        let map = flat_world(10.3, [IVec3::ZERO]);
        let mesh = mesh_chunk(&map, ChunkPos(IVec3::ZERO)).unwrap();

        assert!(!mesh.is_empty());
        for (position, normal) in mesh.positions.iter().zip(&mesh.normals) {
            assert!((position[1] - 10.3).abs() < 0.02, "vertex at {position:?}");
            assert!(Vec3::from(*normal).abs_diff_eq(Vec3::Y, 1e-4));
        }
        assert!(triangles(&mesh).all(|triangle| facing(triangle).y > 0.0));
    }

    #[test]
    fn chunks_without_a_surface_have_empty_meshes() {
        let map = flat_world(100.0, [IVec3::ZERO]);
        assert!(mesh_chunk(&map, ChunkPos(IVec3::ZERO)).unwrap().is_empty());
    }

    #[test]
    fn unloaded_chunks_cannot_be_meshed() {
        let map = flat_world(10.0, [IVec3::ZERO]);
        assert_eq!(mesh_chunk(&map, ChunkPos(IVec3::X)), None);
    }

    /// A sphere straddling eight chunks must come out closed and consistently
    /// oriented: every edge shared by exactly two triangles, in opposite
    /// directions, with no gaps at chunk borders.
    #[test]
    fn meshes_are_watertight_across_chunks() {
        let center = Vec3::new(0.3, 0.2, -0.4);
        let mut map = ChunkMap::default();
        for z in -1..=0 {
            for y in -1..=0 {
                for x in -1..=0 {
                    let chunk = ChunkPos(IVec3::new(x, y, z));
                    let origin = chunk.origin();
                    map.insert(
                        chunk,
                        Chunk::from_fn(|local| {
                            let distance = (origin + local).as_vec3().distance(center) - 10.0;
                            Voxel::new(distance, Material::Stone)
                        }),
                    );
                }
            }
        }

        let weld = |point: Vec3| (point * 1000.0).round().as_ivec3();
        let mut directed_edges: HashMap<(IVec3, IVec3), u32> = HashMap::new();
        let mut triangle_count = 0;
        for chunk in map.positions().collect::<Vec<_>>() {
            let mesh = mesh_chunk(&map, chunk).unwrap();
            let origin = chunk.origin().as_vec3();
            for triangle in triangles(&mesh) {
                let triangle = triangle.map(|vertex| vertex + origin);
                let centroid = (triangle[0] + triangle[1] + triangle[2]) / 3.0;
                assert!(
                    facing(triangle).dot(centroid - center) > 0.0,
                    "triangle faces inward"
                );

                let welded = triangle.map(weld);
                for (from, to) in [(0, 1), (1, 2), (2, 0)] {
                    *directed_edges
                        .entry((welded[from], welded[to]))
                        .or_default() += 1;
                }
                triangle_count += 1;
            }
        }

        assert!(triangle_count > 1000);
        for (&(from, to), &count) in &directed_edges {
            assert_eq!(count, 1, "edge {from}→{to} is used {count} times");
            assert_eq!(
                directed_edges.get(&(to, from)),
                Some(&1),
                "edge {from}→{to} is open"
            );
        }
    }
}
