//! Fixed pseudo-random numbers, for visual variety that must stay the same
//! from frame to frame without being stored.

/// A fixed pseudo-random number in `0..1` for `index`.
pub(crate) fn unit_noise(index: u32) -> f32 {
    let mut value = index.wrapping_mul(0x9e37_79b9);
    value ^= value >> 16;
    value = value.wrapping_mul(0x85eb_ca6b);
    value ^= value >> 13;
    #[expect(clippy::cast_precision_loss, reason = "only the top bits matter")]
    let unit = (value >> 8) as f32 / (1 << 24) as f32;
    unit
}
