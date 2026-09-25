//! How long it takes to generate a column of terrain and to grow its
//! scenery, which the server does as players travel and clients do for
//! everything they draw.

use std::{hint::black_box, path::Path};

use criterion::{Criterion, criterion_group, criterion_main};
use glam::IVec2;
use messoria_content::Catalog;
use messoria_worldgen::{Landscape, props_in_column};

fn generation(c: &mut Criterion) {
    let landscape = Landscape::new(7);
    let data = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/data");
    let catalog = Catalog::load(Path::new(data)).expect("the shipped content is valid");
    // A column of farmland crossed by a road, and one in the hills.
    for (name, column) in [
        ("farmland", IVec2::new(0, 2)),
        ("hills", IVec2::new(-20, 17)),
    ] {
        c.bench_function(&format!("generate a {name} column"), |b| {
            b.iter(|| landscape.column(black_box(column)));
        });
        c.bench_function(&format!("grow a {name} column's scenery"), |b| {
            b.iter(|| props_in_column(&landscape, &catalog, black_box(column)));
        });
    }
    c.bench_function("lay out a valley", |b| {
        b.iter(|| Landscape::new(black_box(7)));
    });
}

criterion_group!(benches, generation);
criterion_main!(benches);
