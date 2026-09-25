//! Hashing and smooth noise, the same on every machine.

use glam::Vec2;

/// Mixes numbers into one well-spread value, with steps of `SplitMix64`.
pub(crate) fn mix(values: &[u64]) -> u64 {
    values.iter().fold(0x6d65_7373_6f72_6961, |state, &value| {
        let mut z = (state ^ value).wrapping_add(0x9e37_79b9_7f4a_7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    })
}

/// A hash as a number from 0 up to 1, from its top bits.
pub(crate) fn unit(hash: u64) -> f32 {
    #[expect(clippy::cast_precision_loss, reason = "24 bits fit an f32 exactly")]
    let value = (hash >> 40) as f32 / (1u64 << 24) as f32;
    value
}

/// Smooth value noise from 0 to 1, with a lattice point at every integer
/// coordinate of `point`.
pub(crate) fn value(seed: u64, point: Vec2) -> f32 {
    let cell = point.floor();
    let t = point - cell;
    let t = t * t * (Vec2::splat(3.0) - 2.0 * t);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "lattice coordinates are a few thousand at most"
    )]
    let corner = |dx: f32, dz: f32| {
        let at = cell + Vec2::new(dx, dz);
        let [x, z] = [at.x as i64, at.y as i64].map(i64::cast_unsigned);
        unit(mix(&[seed, x, z]))
    };
    let near = corner(0.0, 0.0) + (corner(1.0, 0.0) - corner(0.0, 0.0)) * t.x;
    let far = corner(0.0, 1.0) + (corner(1.0, 1.0) - corner(0.0, 1.0)) * t.x;
    near + (far - near) * t.y
}

/// Fractal noise from about -1 to 1: `octaves` layers of value noise, each
/// twice as fine and half as strong as the last, over features about one
/// unit of `point` across.
pub(crate) fn fractal(seed: u64, point: Vec2, octaves: u32) -> f32 {
    let (mut total, mut strength, mut scale, mut sum) = (0.0, 1.0, 1.0, 0.0);
    for octave in 0..octaves {
        total += (value(
            seed ^ u64::from(octave).wrapping_mul(0x51_7cc1),
            point * scale,
        ) * 2.0
            - 1.0)
            * strength;
        sum += strength;
        strength *= 0.5;
        scale *= 2.0;
    }
    total / sum
}

pub(crate) fn smoothstep(from: f32, to: f32, value: f32) -> f32 {
    let t = ((value - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_stays_in_range_and_is_smooth() {
        let mut previous = value(7, Vec2::new(0.0, 0.3));
        for step in 1..400 {
            #[expect(clippy::cast_precision_loss, reason = "a small count")]
            let point = Vec2::new(step as f32 * 0.01, 0.3);
            let here = value(7, point);
            assert!((0.0..=1.0).contains(&here));
            assert!((here - previous).abs() < 0.05, "a jump at {point}");
            previous = here;
        }
        for step in -50..50 {
            #[expect(clippy::cast_precision_loss, reason = "a small count")]
            let point = Vec2::new(step as f32 * 0.37, step as f32 * -0.21);
            assert!((-1.0..=1.0).contains(&fractal(3, point, 4)));
        }
    }

    #[test]
    fn noise_depends_on_the_seed() {
        let point = Vec2::new(3.5, -2.25);
        assert_eq!(value(1, point).to_bits(), value(1, point).to_bits());
        assert!((value(1, point) - value(2, point)).abs() > 1e-6);
    }
}
