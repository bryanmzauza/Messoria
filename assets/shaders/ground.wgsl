// Paints the terrain with pixel-art textures in world space (see
// crates/client/src/ground.rs).
//
// The mesh's vertex colors are not colors: each vertex carries a weight of 1
// for its ground material (grass, soil, stone, sand in r, g, b, a), and the
// weights blend across each triangle. Every texel picks the heaviest material
// after a little noise, so borders are drawn in pixels. The texture is laid
// from above on flat ground and from the side on slopes, blending between
// the two where the ground turns.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}

struct GroundLook {
    tints: array<vec4<f32>, 4>,
    snow: u32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> look: GroundLook;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var ground_textures: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var ground_sampler: sampler;

// Texels per meter, as the textures are painted.
const TEXELS: f32 = 32.0;
// Variants of each texture kind, laid side by side in the array.
const VARIANTS: u32 = 4u;
// Texture sets, in the order of the strip.
const GRASS: u32 = 0u;
const SOIL: u32 = 1u;
const PAVING: u32 = 2u;
const ROCK: u32 = 3u;
const SAND: u32 = 4u;
const SNOW: u32 = 5u;
// Least upward normal for grass to hold, and for stone to lie as paving
// rather than stand as rock.
const GRASS_FLATNESS: f32 = 0.62;
const PAVING_FLATNESS: f32 = 0.8;
// How much noise decides between materials where they meet.
const BORDER_NOISE: f32 = 0.35;

fn mix_bits(value: u32) -> u32 {
    var h = value;
    h ^= h >> 16u;
    h *= 0x7feb352du;
    h ^= h >> 15u;
    h *= 0x846ca68bu;
    h ^= h >> 16u;
    return h;
}

fn hash3(p: vec3<i32>) -> u32 {
    let x = bitcast<u32>(p.x) * 0x8da6b343u;
    let y = bitcast<u32>(p.y) * 0xd8163841u;
    let z = bitcast<u32>(p.z) * 0xcb1ab31fu;
    return mix_bits((x ^ y) ^ z);
}

fn unit(h: u32) -> f32 {
    return f32(h & 0xffffu) / 65535.0;
}

// Samples texture set `kind` at `uv`, in meters across a projection plane. Each
// meter shows one of the kind's variants, flipped one way or another, so the
// tiles do not visibly repeat. `ddx` and `ddy` are the derivatives of `uv`
// across the screen, taken where control flow is uniform.
fn sample_set(kind: u32, uv: vec2<f32>, plane: u32, ddx: vec2<f32>, ddy: vec2<f32>) -> vec3<f32> {
    let cell = vec2<i32>(floor(uv));
    let h = hash3(vec3<i32>(cell, i32(plane * 8u + kind)));
    let layer = kind * VARIANTS + h % VARIANTS;
    var local = fract(uv);
    // Rock faces keep their strata level; everything else may also flip
    // upside down.
    if ((h >> 8u) & 1u) == 1u {
        local.x = 1.0 - local.x;
    }
    if kind != ROCK && ((h >> 9u) & 1u) == 1u {
        local.y = 1.0 - local.y;
    }
    return textureSampleGrad(ground_textures, ground_sampler, local, layer, ddx, ddy).rgb;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let world = in.world_position.xyz;
    let normal = normalize(in.world_normal);

    // The three projections, and their derivatives while control flow is
    // still uniform. Side projections run down the texture as the world
    // goes down.
    let top_uv = world.xz;
    let x_uv = vec2<f32>(world.z, -world.y);
    let z_uv = vec2<f32>(world.x, -world.y);
    let top_ddx = dpdx(top_uv);
    let top_ddy = dpdy(top_uv);
    let x_ddx = dpdx(x_uv);
    let x_ddy = dpdy(x_uv);
    let z_ddx = dpdx(z_uv);
    let z_ddy = dpdy(z_uv);

#ifdef VERTEX_COLORS
    let weights = in.color;
#else
    let weights = vec4<f32>(1.0, 0.0, 0.0, 0.0);
#endif

    // The heaviest material at this texel, after noise.
    let texel = vec3<i32>(floor(world * TEXELS));
    let seed = hash3(texel);
    var material = 0u;
    var heaviest = -1.0;
    for (var i = 0u; i < 4u; i++) {
        let weight = weights[i] + BORDER_NOISE * (unit(mix_bits(seed + i * 0x9e3779b9u)) - 0.5);
        if weight > heaviest {
            heaviest = weight;
            material = i;
        }
    }
    let wobble = 0.08 * (unit(mix_bits(seed ^ 0x51ed270bu)) - 0.5);
    var kind = SOIL;
    var tint = look.tints[material].rgb;
    switch material {
        case 0u: {
            if normal.y + wobble < GRASS_FLATNESS {
                kind = SOIL;
                tint = look.tints[1].rgb;
            } else if look.snow == 1u {
                kind = SNOW;
                tint = vec3<f32>(1.0);
            } else {
                kind = GRASS;
            }
        }
        case 2u: {
            kind = select(ROCK, PAVING, normal.y + wobble > PAVING_FLATNESS);
        }
        case 3u: {
            kind = SAND;
        }
        default: {
            kind = SOIL;
        }
    }

    // Laid from above on flat ground and from the side on slopes.
    var blend = pow(abs(normal), vec3<f32>(4.0));
    blend /= blend.x + blend.y + blend.z;
    var color = vec3<f32>(0.0);
    if blend.y > 0.01 {
        color += blend.y * sample_set(kind, top_uv, 1u, top_ddx, top_ddy);
    }
    if blend.x > 0.01 {
        color += blend.x * sample_set(kind, x_uv, 2u, x_ddx, x_ddy);
    }
    if blend.z > 0.01 {
        color += blend.z * sample_set(kind, z_uv, 3u, z_ddx, z_ddy);
    }

    pbr_input.material.base_color = vec4<f32>(color * tint, 1.0);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
