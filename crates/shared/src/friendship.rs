//! What friendship with a villager is worth at the counter, the same on
//! the server and in the shop window.

use messoria_content::{Catalog, ShopId, VillagerId};
use messoria_economy::Discount;

/// Hearts of friendship with a shop's keeper from which they take a
/// friend's price, and what share they take off.
pub const FRIEND_HEARTS: u16 = 2;
const FRIEND_DISCOUNT: Discount = Discount(10);

/// The villager keeping `shop`, if one does.
pub fn keeper_of(content: &Catalog, shop: ShopId) -> Option<VillagerId> {
    content
        .villagers()
        .find(|(_, villager)| villager.keeps == Some(shop))
        .map(|(id, _)| id)
}

/// The discount a customer with `hearts` of friendship with the keeper
/// gets.
pub fn discount(hearts: u16) -> Discount {
    if hearts >= FRIEND_HEARTS {
        FRIEND_DISCOUNT
    } else {
        Discount::NONE
    }
}
