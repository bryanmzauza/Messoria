//! Costs that bound how fast terrain can be edited and displayed.
//!
//! Run with `cargo bench -p messoria-voxel`.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use glam::{IVec3, Vec3};
use messoria_voxel::{Brush, BrushMode, Chunk, ChunkMap, ChunkPos, Material, Voxel, mesh_chunk};

/// Rolling hills crossing the chunk at the origin, with its 26 neighbors
/// loaded so meshing reads real borders.
fn hills() -> ChunkMap {
    let mut map = ChunkMap::default();
    for z in -1..=1 {
        for y in -1..=1 {
            for x in -1..=1 {
                let chunk = ChunkPos(IVec3::new(x, y, z));
                let origin = chunk.origin();
                map.insert(
                    chunk,
                    Chunk::from_fn(|local| {
                        let voxel = (origin + local).as_vec3();
                        let height = 16.0 + 6.0 * (voxel.x * 0.15).sin() * (voxel.z * 0.11).cos();
                        Voxel::new(voxel.y - height, Material::Grass)
                    }),
                );
            }
        }
    }
    map
}

fn meshing(c: &mut Criterion) {
    let map = hills();
    c.bench_function("mesh a surface chunk", |b| {
        b.iter(|| mesh_chunk(black_box(&map), ChunkPos(IVec3::ZERO)));
    });
}

fn editing(c: &mut Criterion) {
    let map = hills();
    let brush = Brush {
        center: Vec3::new(16.0, 16.0, 16.0),
        radius: 1.5,
        step: 0.5,
        mode: BrushMode::Lower,
    };
    c.bench_function("lower the ground with the shovel", |b| {
        b.iter_batched_ref(
            || map.clone(),
            |map| map.reshape(black_box(&brush)),
            criterion::BatchSize::LargeInput,
        );
    });
}

fn encoding(c: &mut Criterion) {
    let map = hills();
    let chunk = map.get(ChunkPos(IVec3::ZERO)).expect("chunk is loaded");
    let encoded = chunk.encode();
    c.bench_function("encode a surface chunk", |b| {
        b.iter(|| black_box(chunk).encode());
    });
    c.bench_function("decode a surface chunk", |b| {
        b.iter(|| Chunk::decode(black_box(&encoded)));
    });
}

criterion_group!(benches, meshing, editing, encoding);
criterion_main!(benches);
