//! Rules for trading at shops, shared so clients offer exactly the trades
//! the server accepts.

use bevy::math::{Vec3, Vec3Swizzles};

use crate::protocol::Shopfront;

/// How far from the middle of a stall a character can trade, in meters.
pub const SHOP_REACH: f32 = 3.0;
/// How far above or below a stall's ground a character can trade from.
const SHOP_REACH_HEIGHT: f32 = 1.5;

/// Whether a character standing at `feet` can trade at `shopfront`.
pub fn can_reach(feet: Vec3, shopfront: &Shopfront) -> bool {
    feet.xz().distance(shopfront.position.xz()) <= SHOP_REACH
        && (feet.y - shopfront.position.y).abs() <= SHOP_REACH_HEIGHT
}
