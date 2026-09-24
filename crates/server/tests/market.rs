//! Trading at the village grocer: buying seeds, selling produce at the
//! market's price up to the daily limit, the market recovering overnight,
//! and the village's ground staying as it is.

mod common;

use bevy::prelude::*;
use common::HostedWorld;
use messoria_calendar::Season;
use messoria_content::{Catalog, Quality};
use messoria_economy::{Market, PriceBasis};
use messoria_shared::{
    content::load_content,
    protocol::{Deal, MarketState, SoldToday},
    terrain::Terrain,
    village,
};

#[test]
fn produce_sells_at_the_market_price_up_to_the_daily_limit() {
    let content = load_content().expect("the shipped content is valid");
    let grocer = content.shop_id("grocer").expect("the grocer exists");
    let limit = content.shop(grocer).daily_limit;
    let turnip = content.id("turnip").expect("turnips exist");
    let mut world = HostedWorld::new(content.clone());
    world.set_time("10:00");
    world.go_to_stall(&content, "grocer");

    world.give(&content, "turnip", limit + 10);
    let slot = world.slot_of(turnip).expect("the turnips are carried");
    let before = world.coins();
    world.trade(
        &content,
        "grocer",
        Deal::Sell {
            slot,
            count: limit + 10,
        },
    );

    let offer = content
        .shop(grocer)
        .offer(turnip)
        .expect("the grocer buys turnips");
    let basis = PriceBasis::new(&content, offer, Quality::Normal, Season::Spring);
    let expected = Market::default().quote(basis, limit, content.market());
    assert_eq!(world.coins(), before + expected);
    assert_eq!(
        world.carried(turnip),
        10,
        "the grocer buys only its daily limit"
    );

    let after_first_sale = world.coins();
    world.trade(&content, "grocer", Deal::Sell { slot, count: 1 });
    assert_eq!(
        world.coins(),
        after_first_sale,
        "the limit holds for the day"
    );

    world.sleep_through_the_night();
    let saturation = market(&mut world).saturation(turnip);
    let recovered = f32::from(limit) * (1.0 - content.market().daily_recovery);
    assert!(
        (saturation - recovered).abs() < 0.01,
        "saturation {saturation}"
    );
    assert!(sales_today(&mut world).is_empty());

    world.set_time("10:00");
    world.go_to_stall(&content, "grocer");
    world.trade(&content, "grocer", Deal::Sell { slot, count: 1 });
    assert!(
        world.coins() > after_first_sale,
        "a new day brings a new limit"
    );
}

#[test]
fn seeds_are_bought_only_while_the_shop_is_open() {
    let content = load_content().expect("the shipped content is valid");
    let seeds = content.id("turnip_seeds").expect("turnip seeds exist");
    let mut world = HostedWorld::new(content.clone());
    world.go_to_stall(&content, "grocer");
    let (coins, carried) = (world.coins(), world.carried(seeds));

    // The world starts at dawn, before the grocer opens.
    world.trade(
        &content,
        "grocer",
        Deal::Buy {
            item: seeds,
            count: 5,
        },
    );
    assert_eq!((world.coins(), world.carried(seeds)), (coins, carried));

    world.set_time("09:00");
    world.trade(
        &content,
        "grocer",
        Deal::Buy {
            item: seeds,
            count: 5,
        },
    );
    let price = price_of(&content, "turnip_seeds");
    assert_eq!(world.coins(), coins - 5 * price);
    assert_eq!(world.carried(seeds), carried + 5);
}

#[test]
fn trading_needs_the_character_at_the_stall() {
    let content = load_content().expect("the shipped content is valid");
    let seeds = content.id("turnip_seeds").expect("turnip seeds exist");
    let mut world = HostedWorld::new(content.clone());
    world.set_time("10:00");
    let coins = world.coins();

    world.trade(
        &content,
        "grocer",
        Deal::Buy {
            item: seeds,
            count: 1,
        },
    );
    assert_eq!(world.coins(), coins, "players start far from the village");
}

#[test]
fn the_village_ground_cannot_be_dug() {
    let content = load_content().expect("the shipped content is valid");
    let mut world = HostedWorld::new(content.clone());
    let stall = world.go_to_stall(&content, "grocer");
    let target = stall.position + Vec3::new(2.0, 0.0, 2.0);
    let before = world.world().resource::<Terrain>().0.clone();

    let shovel = world.slot_holding(&content, "shovel");
    world.use_item(shovel, target);

    let terrain = world.world().resource::<Terrain>();
    assert!(
        before
            .positions()
            .all(|chunk| before.get(chunk) == terrain.get(chunk)),
        "the terrain changed"
    );
    assert!(village::reaches(target, 0.0));
}

fn market(world: &mut HostedWorld) -> Market {
    world
        .world()
        .query::<&MarketState>()
        .single(world.world())
        .expect("the world has a market")
        .0
        .clone()
}

fn sales_today(world: &mut HostedWorld) -> messoria_economy::SalesLedger {
    world
        .world()
        .query::<&SoldToday>()
        .single(world.world())
        .expect("one character")
        .0
        .clone()
}

fn price_of(content: &Catalog, key: &str) -> u32 {
    let grocer = content.shop(content.shop_id("grocer").expect("the grocer exists"));
    grocer
        .listing(content.id(key).expect("the item exists"))
        .expect("the grocer sells it")
        .price
}
