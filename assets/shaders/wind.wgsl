// The wind that sways foliage (see crates/client/src/wind.rs).
//
// A vertex is pushed along the wind by a slow wave that rolls across the
// valley, a faster flutter on top, and gusts that come and go, in proportion
// to how high it stands over the base of its plant: roots and trunks stay
// put, the tops of leaves and blades move most.

#define_import_path messoria::wind

struct Sway {
    // How far the top of a plant moves, in meters.
    strength: f32,
    // The time now and a frame ago, in seconds, so that motion vectors see
    // how far the wind moved each vertex.
    time: f32,
    previous_time: f32,
    // Keeps the uniform 16 bytes wide.
    padding: f32,
}

// Height over a plant's base at which it sways fully, in meters.
const FULL_SWAY_HEIGHT: f32 = 1.2;
// The wind blows mostly this way, across the valley.
const WIND: vec2<f32> = vec2<f32>(0.8, 0.6);

// Where the wind has pushed `world`, a vertex `height` meters over the base
// of its plant, at `time`.
fn displace(world: vec3<f32>, height: f32, strength: f32, time: f32) -> vec3<f32> {
    let weight = clamp(height / FULL_SWAY_HEIGHT, 0.0, 1.0);
    let along = dot(world.xz, WIND);
    let wave = sin(along * 0.35 - time * 1.3);
    let flutter = sin(along * 2.1 + world.y * 1.7 - time * 4.3) * 0.35;
    let gust = 0.65 + 0.35 * sin(time * 0.37 + along * 0.02);
    let push = (wave + flutter) * gust * strength * weight;
    // Blades bend as they move, dipping a little.
    return world + vec3<f32>(WIND.x * push, -abs(push) * 0.25, WIND.y * push);
}
