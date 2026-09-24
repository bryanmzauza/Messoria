//! Voxel terrain: storage, editing, queries and meshing.
//!
//! The world is a grid of samples one meter apart, sitting at integer world
//! coordinates. Each sample stores the signed distance to the terrain surface
//! (negative inside solid ground) and a material. Surfaces are extracted with
//! Surface Nets, so terrain renders as smooth shapes rather than cubes.
//!
//! This crate has no engine dependency; see
//! `docs/adr/0003-pure-domain-crates.md`.

mod chunk;
mod coords;
mod edit;
mod map;
mod mesh;
mod query;
mod voxel;

pub use crate::{
    chunk::{Chunk, DecodeError},
    coords::{CHUNK_SIZE, ChunkPos},
    edit::{Brush, BrushMode, ChunkChanges, Levelling, Reshape, SURFACE_SEARCH},
    map::ChunkMap,
    mesh::{SurfaceMesh, mesh_chunk},
    query::RayHit,
    voxel::{Material, Voxel},
};
