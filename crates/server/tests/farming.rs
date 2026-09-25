//! A crop planted on the first day ripens on the day its data file predicts
//! and can then be harvested.

mod common;

use common::HostedWorld;
use messoria_shared::{content::load_content, fields::tile_at};

#[test]
fn a_crop_ripens_on_the_day_its_data_predicts() {
    let content = load_content().expect("the shipped content is valid");
    let turnip = content.crop_id("turnip").expect("turnips exist");
    let days_to_ripen = u32::from(content.crop(turnip).days_to_ripen());
    let mut world = HostedWorld::new(content.clone());

    let target = world.ground_ahead();
    let tile = tile_at(target);
    let hoe = world.slot_holding(&content, "hoe");
    world.use_item(hoe, target);
    assert!(world.field(tile).is_some(), "the hoe tills the ground");

    let seeds = world.slot_holding(&content, "turnip_seeds");
    world.use_item(seeds, target);
    let planted_on = world.clock().day();
    assert_eq!(world.crop(tile).map(|crop| crop.crop), Some(turnip));

    let watering_can = world.slot_holding(&content, "watering_can");
    let mut ripe_on = None;
    for _ in 0..=days_to_ripen + 2 {
        world.use_item(watering_can, target);
        assert!(world.is_watered(tile), "the watering can waters the field");
        world.sleep_through_the_night();

        let crop = world.crop(tile).expect("the crop keeps growing");
        if crop.is_ripe(content.crop(turnip)) {
            ripe_on = Some(world.clock().day());
            break;
        }
    }
    assert_eq!(ripe_on, Some(planted_on + days_to_ripen));

    let produce = content.crop(turnip).produce;
    let before = world.carried(produce);
    world.harvest(target);
    assert_eq!(world.carried(produce), before + 1);
    assert_eq!(world.crop(tile), None, "turnips do not regrow");
}
