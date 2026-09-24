//! Loading the content files and checking them against each other.

use std::{collections::HashMap, fs, path::Path};

use messoria_calendar::Season;
use messoria_voxel::Material;
use ron::{Options, extensions::Extensions};
use serde::de::DeserializeOwned;

use crate::{
    crop::{CropDef, CropId, Crops, CropsFile},
    error::{ContentError, Problem},
    item::{ItemDef, ItemId, ItemKind, Items, ItemsFile},
    palette::{Palette, PaletteFile},
    people::Characters,
    scenery::{CoverDef, PropDef, PropId, Scenery, SceneryFile},
    shop::{MarketRules, ShopDef, ShopId, Shops, ShopsFile},
    structure::{StructureDef, StructureId, Structures, StructuresFile},
    village::{Village, VillageFile},
};

/// Files within the data folder.
const ITEMS_FILE: &str = "items.ron";
const CROPS_FILE: &str = "crops.ron";
const SHOPS_FILE: &str = "shops.ron";
const SCENERY_FILE: &str = "scenery.ron";
const PALETTE_FILE: &str = "palette.ron";
const STRUCTURES_FILE: &str = "structures.ron";
const CHARACTERS_FILE: &str = "characters.ron";
const VILLAGE_FILE: &str = "village.ron";
/// Folder of the models, next to the data folder.
const MODELS_FOLDER: &str = "models";
const SKINS_FOLDER: &str = "skins";

/// The text of every content file.
#[derive(Clone, Copy, Debug)]
pub struct Sources<'a> {
    pub items: &'a str,
    pub crops: &'a str,
    pub shops: &'a str,
    pub scenery: &'a str,
    pub palette: &'a str,
    pub structures: &'a str,
    pub characters: &'a str,
    pub village: &'a str,
}

/// All loaded content, with every cross-reference checked.
#[derive(Clone, Debug)]
pub struct Catalog {
    items: Vec<ItemDef>,
    item_keys: HashMap<String, ItemId>,
    dug_items: HashMap<Material, ItemId>,
    starting_inventory: Vec<(ItemId, u16)>,
    starting_money: u32,
    crops: Vec<CropDef>,
    crop_keys: HashMap<String, CropId>,
    crops_by_seed: HashMap<ItemId, CropId>,
    /// Seasons in which the crops yielding each produce item grow.
    harvest_seasons: HashMap<ItemId, Vec<Season>>,
    market: MarketRules,
    shops: Vec<ShopDef>,
    shop_keys: HashMap<String, ShopId>,
    props: Vec<PropDef>,
    prop_keys: HashMap<String, PropId>,
    cover: Vec<CoverDef>,
    palette: Palette,
    structures: Vec<StructureDef>,
    structure_keys: HashMap<String, StructureId>,
    structures_by_item: HashMap<ItemId, StructureId>,
    characters: Characters,
    village: Village,
}

impl Catalog {
    /// Loads content from the data folder at `data_dir`, and checks that
    /// every model it refers to is in the models folder beside it.
    ///
    /// # Errors
    ///
    /// Fails with the file and the problem if any file is missing, malformed
    /// or inconsistent.
    pub fn load(data_dir: &Path) -> Result<Self, ContentError> {
        let read = |file: &str| {
            let path = data_dir.join(file);
            fs::read_to_string(&path).map_err(|error| ContentError {
                file: path,
                problem: error.into(),
            })
        };
        let (items, crops, shops) = (read(ITEMS_FILE)?, read(CROPS_FILE)?, read(SHOPS_FILE)?);
        let (scenery, palette) = (read(SCENERY_FILE)?, read(PALETTE_FILE)?);
        let structures = read(STRUCTURES_FILE)?;
        let (characters, village) = (read(CHARACTERS_FILE)?, read(VILLAGE_FILE)?);
        let sources = Sources {
            items: &items,
            crops: &crops,
            shops: &shops,
            scenery: &scenery,
            palette: &palette,
            structures: &structures,
            characters: &characters,
            village: &village,
        };
        let catalog = Self::build(sources, data_dir)?;

        let models = data_dir.parent().unwrap_or(data_dir).join(MODELS_FOLDER);
        let models_used = catalog
            .props
            .iter()
            .flat_map(PropDef::models_used)
            .chain(catalog.cover.iter().flat_map(|cover| &cover.models));
        let structure_models = catalog
            .structures
            .iter()
            .flat_map(|structure| structure.parts.iter().map(|part| &part.model));
        let item_models = catalog.items.iter().filter_map(|item| item.model.as_ref());
        let crop_models = catalog.crops.iter().flat_map(|crop| &crop.models);
        let shop_models = catalog.shops.iter().map(|shop| &shop.stall);
        let used = models_used
            .map(|model| (model, SCENERY_FILE))
            .chain(structure_models.map(|model| (model, STRUCTURES_FILE)))
            .chain(item_models.map(|model| (model, ITEMS_FILE)))
            .chain(crop_models.map(|model| (model, CROPS_FILE)))
            .chain(shop_models.map(|model| (model, SHOPS_FILE)));
        for (model, file) in used {
            if !models.join(model).is_file() {
                return Err(ContentError {
                    file: data_dir.join(file),
                    problem: Problem::MissingModel(model.clone()),
                });
            }
        }

        let skins = data_dir.parent().unwrap_or(data_dir).join(SKINS_FOLDER);
        let keeper_skins = catalog
            .shops
            .iter()
            .flat_map(|shop| shop.keeper.layers())
            .map(|skin| (skin, SHOPS_FILE));
        let used = catalog
            .characters
            .skins_used()
            .map(|skin| (skin.as_str(), CHARACTERS_FILE))
            .chain(keeper_skins);
        for (skin, file) in used {
            if !skins.join(skin).is_file() {
                return Err(ContentError {
                    file: data_dir.join(file),
                    problem: Problem::MissingSkin(skin.to_owned()),
                });
            }
        }
        Ok(catalog)
    }

    /// Builds a catalog from the text of the content files. Models are not
    /// looked for.
    ///
    /// # Errors
    ///
    /// Fails if any text is malformed or inconsistent.
    pub fn from_sources(sources: Sources<'_>) -> Result<Self, ContentError> {
        Self::build(sources, Path::new(""))
    }

    /// Builds a catalog from the files' text, naming them as found in
    /// `folder` in errors.
    fn build(sources: Sources<'_>, folder: &Path) -> Result<Self, ContentError> {
        let in_file = |file: &str| {
            let file = folder.join(file);
            move |problem| ContentError { file, problem }
        };
        let items: Items = parse::<ItemsFile>(sources.items)
            .and_then(ItemsFile::resolve)
            .map_err(in_file(ITEMS_FILE))?;
        let dug_items = dug_items(&items.definitions).map_err(in_file(ITEMS_FILE))?;
        let crops: Crops = parse::<CropsFile>(sources.crops)
            .and_then(|file| file.resolve(&items))
            .map_err(in_file(CROPS_FILE))?;
        let shops: Shops = parse::<ShopsFile>(sources.shops)
            .and_then(|file| file.resolve(&items))
            .map_err(in_file(SHOPS_FILE))?;
        let scenery: Scenery = parse::<SceneryFile>(sources.scenery)
            .and_then(|file| file.resolve(&items))
            .map_err(in_file(SCENERY_FILE))?;
        let palette = parse::<PaletteFile>(sources.palette)
            .and_then(PaletteFile::resolve)
            .map_err(in_file(PALETTE_FILE))?;
        let structures: Structures = parse::<StructuresFile>(sources.structures)
            .and_then(|file| file.resolve(&items))
            .map_err(in_file(STRUCTURES_FILE))?;
        let characters = parse::<Characters>(sources.characters)
            .and_then(|characters| characters.validate().map(|()| characters))
            .map_err(in_file(CHARACTERS_FILE))?;
        let village = parse::<VillageFile>(sources.village)
            .and_then(|file| file.resolve(&shops.by_key, &structures.by_key))
            .map_err(in_file(VILLAGE_FILE))?;

        let mut harvest_seasons: HashMap<ItemId, Vec<Season>> = HashMap::new();
        for crop in &crops.definitions {
            let seasons = harvest_seasons.entry(crop.produce).or_default();
            for &season in &crop.seasons {
                if !seasons.contains(&season) {
                    seasons.push(season);
                }
            }
        }

        Ok(Self {
            items: items.definitions,
            item_keys: items.by_key,
            dug_items,
            starting_inventory: items.starting_inventory,
            starting_money: items.starting_money,
            crops: crops.definitions,
            crop_keys: crops.by_key,
            crops_by_seed: crops.by_seed,
            harvest_seasons,
            market: shops.market,
            shops: shops.definitions,
            shop_keys: shops.by_key,
            props: scenery.props,
            prop_keys: scenery.by_key,
            cover: scenery.cover,
            palette,
            structures: structures.definitions,
            structure_keys: structures.by_key,
            structures_by_item: structures.by_item,
            characters,
            village,
        })
    }

    /// The definition of `id`.
    ///
    /// # Panics
    ///
    /// If `id` came from a different catalog with more items.
    pub fn item(&self, id: ItemId) -> &ItemDef {
        &self.items[usize::from(id.0)]
    }

    /// The item a data file calls `key`.
    pub fn id(&self, key: &str) -> Option<ItemId> {
        self.item_keys.get(key).copied()
    }

    /// Every item, with its id.
    pub fn items(&self) -> impl Iterator<Item = (ItemId, &ItemDef)> {
        (0..=u16::MAX).map(ItemId).zip(&self.items)
    }

    /// The item digging `material` yields. Every material has one.
    ///
    /// # Panics
    ///
    /// Never for a loaded catalog, which is checked to cover every material.
    pub fn dug_item(&self, material: Material) -> ItemId {
        self.dug_items[&material]
    }

    /// What every new player starts with.
    pub fn starting_inventory(&self) -> &[(ItemId, u16)] {
        &self.starting_inventory
    }

    /// The money every new player starts with.
    pub fn starting_money(&self) -> u32 {
        self.starting_money
    }

    /// The definition of `id`.
    ///
    /// # Panics
    ///
    /// If `id` came from a different catalog with more crops.
    pub fn crop(&self, id: CropId) -> &CropDef {
        &self.crops[usize::from(id.0)]
    }

    /// Every crop, with its id.
    pub fn crops(&self) -> impl Iterator<Item = (CropId, &CropDef)> {
        (0..=u16::MAX).map(CropId).zip(&self.crops)
    }

    /// The crop a data file calls `key`.
    pub fn crop_id(&self, key: &str) -> Option<CropId> {
        self.crop_keys.get(key).copied()
    }

    /// The crop that grows from the seed item `seeds`.
    pub fn crop_grown_from(&self, seeds: ItemId) -> Option<CropId> {
        self.crops_by_seed.get(&seeds).copied()
    }

    /// The seasons in which `item` is harvested, if it is a crop's produce.
    pub fn harvest_seasons(&self, item: ItemId) -> Option<&[Season]> {
        self.harvest_seasons.get(&item).map(Vec::as_slice)
    }

    /// The rules market prices follow.
    pub fn market(&self) -> &MarketRules {
        &self.market
    }

    /// The definition of `id`.
    ///
    /// # Panics
    ///
    /// If `id` came from a different catalog with more shops.
    pub fn shop(&self, id: ShopId) -> &ShopDef {
        &self.shops[usize::from(id.0)]
    }

    /// Every shop, with its id.
    pub fn shops(&self) -> impl Iterator<Item = (ShopId, &ShopDef)> {
        (0..=u16::MAX).map(ShopId).zip(&self.shops)
    }

    /// The shop a data file calls `key`.
    pub fn shop_id(&self, key: &str) -> Option<ShopId> {
        self.shop_keys.get(key).copied()
    }

    /// The definition of `id`.
    ///
    /// # Panics
    ///
    /// If `id` came from a different catalog with more props.
    pub fn prop(&self, id: PropId) -> &PropDef {
        &self.props[usize::from(id.0)]
    }

    /// Every prop, with its id.
    pub fn props(&self) -> impl Iterator<Item = (PropId, &PropDef)> {
        (0..=u16::MAX).map(PropId).zip(&self.props)
    }

    /// The prop a data file calls `key`.
    pub fn prop_id(&self, key: &str) -> Option<PropId> {
        self.prop_keys.get(key).copied()
    }

    /// The definition of structure `id`.
    ///
    /// # Panics
    ///
    /// If `id` came from a different catalog with more structures.
    pub fn structure(&self, id: StructureId) -> &StructureDef {
        &self.structures[usize::from(id.0)]
    }

    pub fn structures(&self) -> impl Iterator<Item = (StructureId, &StructureDef)> {
        (0..=u16::MAX).map(StructureId).zip(&self.structures)
    }

    /// The structure a data file calls `key`.
    pub fn structure_id(&self, key: &str) -> Option<StructureId> {
        self.structure_keys.get(key).copied()
    }

    /// The structure placing `item` builds, if it builds one.
    pub fn structure_built_from(&self, item: ItemId) -> Option<StructureId> {
        self.structures_by_item.get(&item).copied()
    }

    /// The structure that is every player's home, if there is one.
    pub fn home(&self) -> Option<StructureId> {
        self.structures()
            .find(|(_, structure)| structure.home)
            .map(|(id, _)| id)
    }

    /// How characters look and move.
    pub fn characters(&self) -> &Characters {
        &self.characters
    }

    /// Where the village's stalls and buildings stand.
    pub fn village(&self) -> &Village {
        &self.village
    }

    /// The plants covering grassy ground.
    pub fn cover(&self) -> &[CoverDef] {
        &self.cover
    }

    /// The colors everything is drawn in.
    pub fn palette(&self) -> &Palette {
        &self.palette
    }
}

/// Parses a content file, allowing optional fields to be written as plain
/// values rather than wrapped in `Some`.
fn parse<T: DeserializeOwned>(source: &str) -> Result<T, Problem> {
    Ok(Options::default()
        .with_default_extension(Extensions::IMPLICIT_SOME)
        .from_str(source)?)
}

/// Maps every terrain material to the one item digging it yields.
fn dug_items(items: &[ItemDef]) -> Result<HashMap<Material, ItemId>, Problem> {
    let mut dug_items: HashMap<Material, ItemId> = HashMap::new();
    for (item, id) in items.iter().zip((0..=u16::MAX).map(ItemId)) {
        let ItemKind::Terrain { materials } = &item.kind else {
            continue;
        };
        for &material in materials {
            if let Some(first) = dug_items.insert(material, id) {
                return Err(Problem::AmbiguousMaterial {
                    material,
                    first: items[usize::from(first.0)].key.clone(),
                    second: item.key.clone(),
                });
            }
        }
    }
    match Material::ALL
        .into_iter()
        .find(|material| !dug_items.contains_key(material))
    {
        Some(material) => Err(Problem::UndiggableMaterial(material)),
        None => Ok(dug_items),
    }
}

#[cfg(test)]
mod tests {
    use messoria_calendar::WorldTime;

    use super::*;
    use crate::{Rgb, Sources};

    const ITEMS: &str = r#"(
        items: [
            (id: "shovel", name: "Shovel", kind: Tool(Shovel)),
            (id: "soil", name: "Soil", kind: Terrain(materials: [Grass, Soil]), stack: 99),
            (id: "stone", name: "Stone", kind: Terrain(materials: [Stone, Sand]), stack: 99),
            (id: "berries", name: "Berries", kind: Food(energy: 5), stack: 20, shelf_life: 3, spoils_into: "compost"),
            (id: "compost", name: "Compost", kind: Fertilizer, stack: 99),
            (id: "turnip_seeds", name: "Turnip seeds", kind: Seed, stack: 99),
            (id: "turnip", name: "Turnip", kind: Food(energy: 10), stack: 99),
        ],
        starting_inventory: [("shovel", 1), ("berries", 4)],
        starting_money: 150,
    )"#;

    const CROPS: &str = r#"(
        crops: [
            (
                id: "turnip", name: "Turnip", seeds: "turnip_seeds", produce: "turnip",
                seasons: [Spring], stages: [1, 1, 2], color: (0.9, 0.8, 0.9),
                models: ["crops/a.glb", "crops/b.glb", "crops/c.glb", "crops/d.glb"],
            ),
        ],
    )"#;

    const SHOPS: &str = r#"(
        market: (
            off_season_markup: 1.5,
            halves_after: 100.0,
            daily_recovery: 0.25,
            silver_bonus: 1.25,
            gold_bonus: 1.5,
        ),
        shops: [
            (
                id: "grocer", name: "Grocer", opens: "09:00", closes: "17:00",
                closed_on: [Sunday], daily_limit: 20,
                buys: [("turnip", 30), ("berries", 8)],
                sells: [(item: "turnip_seeds", price: 10, seasons: [Spring]), (item: "compost", price: 5)],
                stall: "town/stall.glb",
                keeper: (body: "b.png", outfit: "o.png", hair: "h.png", hair_color: (0.2, 0.1, 0.1)),
            ),
        ],
    )"#;

    const SCENERY: &str = r#"(
        props: [
            (
                id: "oak", name: "Oak", models: ["nature/oak.glb"], scale: (3.0, 4.0),
                radius: 0.2, spacing: 10.0, density: 0.5, clustering: 0.5, max_slope: 0.6,
                grows_on: [Grass],
            ),
        ],
        cover: [(models: ["nature/grass.glb"], per_square_meter: 0.2, scale: (1.0, 1.5), seasons: [Spring])],
    )"#;

    const PALETTE: &str = r#"(
        colors: {
            "terrain_grass": (0.3, 0.5, 0.2),
            "terrain_soil": (0.4, 0.3, 0.2),
            "terrain_stone": (0.5, 0.5, 0.5),
            "terrain_sand": (0.8, 0.7, 0.5),
            "leaves": (0.2, 0.5, 0.2),
        },
        seasons: {Autumn: {"leaves": (0.8, 0.4, 0.1)}},
    )"#;

    fn catalog(
        items: &str,
        crops: &str,
        shops: &str,
        scenery: &str,
        palette: &str,
    ) -> Result<Catalog, ContentError> {
        Catalog::from_sources(Sources {
            items,
            crops,
            shops,
            scenery,
            palette,
            structures: "(structures: [])",
            characters: CHARACTERS,
            village: "(stalls: [(id: \"grocer\", at: (0.0, -3.0), turn: 180.0)])",
        })
    }

    const CHARACTERS: &str = r#"(
        pixel: 0.1, texels: 1, skin_size: (8, 8),
        limbs: (
            body: (joint: (0.0, 1.0, 0.0), boxes: [(from: (0.0, 0.0, 0.0), size: (1.0, 1.0, 1.0), uv: (0, 0))]),
            head: (joint: (0.0, 1.0, 0.0), boxes: [(from: (0.0, 0.0, 0.0), size: (1.0, 1.0, 1.0), uv: (0, 0))]),
            right_arm: (joint: (1.0, 1.0, 0.0), boxes: [(from: (0.0, 0.0, 0.0), size: (1.0, 1.0, 1.0), uv: (0, 0))]),
            left_arm: (joint: (-1.0, 1.0, 0.0), boxes: [(from: (0.0, 0.0, 0.0), size: (1.0, 1.0, 1.0), uv: (0, 0))]),
            right_leg: (joint: (0.5, 1.0, 0.0), boxes: [(from: (0.0, 0.0, 0.0), size: (1.0, 1.0, 1.0), uv: (0, 0))]),
            left_leg: (joint: (-0.5, 1.0, 0.0), boxes: [(from: (0.0, 0.0, 0.0), size: (1.0, 1.0, 1.0), uv: (0, 0))]),
            right_forearm: (joint: (0.0, -1.0, 0.0), boxes: [(from: (0.0, 0.0, 0.0), size: (1.0, 1.0, 1.0), uv: (0, 0))]),
            left_forearm: (joint: (0.0, -1.0, 0.0), boxes: [(from: (0.0, 0.0, 0.0), size: (1.0, 1.0, 1.0), uv: (0, 0))]),
            right_shin: (joint: (0.0, -1.0, 0.0), boxes: [(from: (0.0, 0.0, 0.0), size: (1.0, 1.0, 1.0), uv: (0, 0))]),
            left_shin: (joint: (0.0, -1.0, 0.0), boxes: [(from: (0.0, 0.0, 0.0), size: (1.0, 1.0, 1.0), uv: (0, 0))]),
        ),
        expressions: (blink: (0, 0), smile: (0, 0), surprise: (0, 0), effort: (0, 0)),
        grip: (at: (0.0, 0.0, 0.0), turn: (0.0, 0.0, 0.0), tool_size: 1.0, item_size: 0.5),
        wardrobe: (bodies: ["b.png"], outfits: ["o.png"], hair: ["h.png"], hair_colors: [(0.2, 0.1, 0.1)]),
    )"#;

    fn valid() -> Catalog {
        catalog(ITEMS, CROPS, SHOPS, SCENERY, PALETTE).expect("the test content is valid")
    }

    fn rejection(result: Result<Catalog, ContentError>) -> String {
        result.expect_err("content should be rejected").to_string()
    }

    fn problem_with_items(items: &str) -> String {
        rejection(catalog(items, CROPS, SHOPS, SCENERY, PALETTE))
    }

    fn problem_with_crops(crops: &str) -> String {
        rejection(catalog(ITEMS, crops, SHOPS, SCENERY, PALETTE))
    }

    fn problem_with_shops(shops: &str) -> String {
        rejection(catalog(ITEMS, CROPS, shops, SCENERY, PALETTE))
    }

    fn problem_with_scenery(scenery: &str) -> String {
        rejection(catalog(ITEMS, CROPS, SHOPS, scenery, PALETTE))
    }

    fn problem_with_palette(palette: &str) -> String {
        rejection(catalog(ITEMS, CROPS, SHOPS, SCENERY, palette))
    }

    #[test]
    fn scenery_and_palette_load() {
        let catalog = valid();
        let oak = catalog.prop(catalog.prop_id("oak").unwrap());
        assert_eq!(oak.models, ["nature/oak.glb"]);
        assert_eq!(oak.grows_on, [Material::Grass]);
        assert!(catalog.cover()[0].shows_in(Season::Spring));
        assert!(!catalog.cover()[0].shows_in(Season::Winter));

        let palette = catalog.palette();
        let near = |color: Option<Rgb>, expected: Rgb| {
            color.is_some_and(|color| {
                color
                    .iter()
                    .zip(expected)
                    .all(|(channel, expected)| (channel - expected).abs() < 1e-6)
            })
        };
        assert!(near(
            palette.color("leaves", Season::Spring),
            [0.2, 0.5, 0.2]
        ));
        assert!(near(
            palette.color("leaves", Season::Autumn),
            [0.8, 0.4, 0.1]
        ));
        assert_eq!(palette.color("bark", Season::Spring), None);
        assert!(near(
            Some(palette.ground(Material::Soil, Season::Winter)),
            [0.4, 0.3, 0.2]
        ));
    }

    #[test]
    fn inconsistent_scenery_is_rejected() {
        let cases = [
            (
                SCENERY.replace("props: [", "props: [(id: \"oak\", name: \"Oak\", models: [\"a.glb\"], scale: (1.0, 1.0), radius: 0.1, spacing: 1.0, density: 1.0, max_slope: 1.0, grows_on: [Grass]),"),
                "prop `oak` is defined more than once",
            ),
            (
                SCENERY.replace("[\"nature/oak.glb\"]", "[\"../oak.glb\"]"),
                "prop `oak` is invalid: `../oak.glb` is not a .glb file inside the models folder",
            ),
            (
                SCENERY.replace("[\"nature/oak.glb\"]", "[]"),
                "prop `oak` is invalid: it lists no model",
            ),
            (
                SCENERY.replace("scale: (3.0, 4.0)", "scale: (4.0, 3.0)"),
                "prop `oak` is invalid: its scale must be a positive range, smallest first",
            ),
            (
                SCENERY.replace("density: 0.5", "density: 1.5"),
                "prop `oak` is invalid: its density must be above 0 and at most 1",
            ),
            (
                SCENERY.replace("grows_on: [Grass]", "grows_on: []"),
                "prop `oak` is invalid: it grows on no ground",
            ),
            (
                SCENERY.replace("per_square_meter: 0.2", "per_square_meter: 0.0"),
                "ground cover 1 is invalid: it must grow somewhere: plants per square meter must be above 0",
            ),
        ];
        for (source, expected) in cases {
            assert_eq!(
                problem_with_scenery(&source),
                format!("scenery.ron: {expected}")
            );
        }
    }

    #[test]
    fn inconsistent_palettes_are_rejected() {
        let cases = [
            (
                PALETTE.replace("\"terrain_sand\": (0.8, 0.7, 0.5),", ""),
                "the palette has no color for `terrain_sand`",
            ),
            (
                PALETTE.replace("(0.2, 0.5, 0.2)", "(0.2, 1.5, 0.2)"),
                "color `leaves` has a channel outside 0 to 1",
            ),
            (
                PALETTE.replace("{\"leaves\": (0.8", "{\"leafs\": (0.8"),
                "Autumn changes color `leafs`, which the palette does not define",
            ),
        ];
        for (source, expected) in cases {
            assert_eq!(
                problem_with_palette(&source),
                format!("palette.ron: {expected}")
            );
        }
    }

    #[test]
    fn valid_content_loads_with_every_reference_resolved() {
        let catalog = valid();
        let berries = catalog.id("berries").unwrap();

        assert_eq!(catalog.item(berries).name, "Berries");
        assert_eq!(catalog.item(berries).shelf_life, Some(3));
        assert_eq!(catalog.item(berries).spoils_into, catalog.id("compost"));
        assert_eq!(catalog.item(catalog.id("shovel").unwrap()).max_stack, 1);
        assert_eq!(
            catalog.dug_item(Material::Grass),
            catalog.id("soil").unwrap()
        );
        assert_eq!(
            catalog.starting_inventory(),
            [(catalog.id("shovel").unwrap(), 1), (berries, 4)]
        );
        assert_eq!(catalog.starting_money(), 150);
    }

    #[test]
    fn crops_link_their_seeds_and_produce() {
        let catalog = valid();
        let turnip = catalog.crop_id("turnip").unwrap();
        let seeds = catalog.id("turnip_seeds").unwrap();

        assert_eq!(catalog.crop_grown_from(seeds), Some(turnip));
        assert_eq!(catalog.crop(turnip).produce, catalog.id("turnip").unwrap());
        assert_eq!(catalog.crop(turnip).seasons, [Season::Spring]);
        assert_eq!(catalog.crop(turnip).days_to_ripen(), 4);
        assert_eq!(catalog.crop(turnip).regrows_after, None);
    }

    #[test]
    fn shops_trade_on_their_days_and_hours() {
        let catalog = valid();
        let grocer = catalog.shop(catalog.shop_id("grocer").unwrap());
        let at = |day, clock: &str| WorldTime::at(day, clock.parse().unwrap()).unwrap();

        // Day 0 is a Monday and day 6 a Sunday.
        assert!(!grocer.is_open(at(0, "08:59")));
        assert!(grocer.is_open(at(0, "09:00")));
        assert!(grocer.is_open(at(0, "16:59")));
        assert!(!grocer.is_open(at(0, "17:00")));
        assert!(!grocer.is_open(at(6, "12:00")));
    }

    #[test]
    fn shops_link_what_they_buy_and_sell() {
        let catalog = valid();
        let grocer = catalog.shop(catalog.shop_id("grocer").unwrap());
        let (turnip, seeds, compost) = (
            catalog.id("turnip").unwrap(),
            catalog.id("turnip_seeds").unwrap(),
            catalog.id("compost").unwrap(),
        );

        assert_eq!(grocer.offer(turnip).map(|offer| offer.base_price), Some(30));
        assert_eq!(grocer.offer(seeds), None);
        assert!(grocer.listing(seeds).unwrap().on_sale_in(Season::Spring));
        assert!(!grocer.listing(seeds).unwrap().on_sale_in(Season::Summer));
        assert!(grocer.listing(compost).unwrap().on_sale_in(Season::Winter));
        assert_eq!(catalog.harvest_seasons(turnip), Some(&[Season::Spring][..]));
        assert_eq!(catalog.harvest_seasons(compost), None);
    }

    #[test]
    fn inconsistent_shops_are_rejected() {
        let cases = [
            (
                SHOPS.replace("shops: [", "shops: [(id: \"grocer\", name: \"Again\", opens: \"09:00\", closes: \"10:00\", daily_limit: 1, buys: [], sells: [], stall: \"s.glb\", keeper: (body: \"b.png\", outfit: \"o.png\", hair: \"h.png\", hair_color: (0.0, 0.0, 0.0))),"),
                "shop `grocer` is defined more than once",
            ),
            (
                SHOPS.replace("(\"turnip\", 30)", "(\"turnips\", 30)"),
                "shop `grocer` refers to unknown item `turnips`",
            ),
            (
                SHOPS.replace("opens: \"09:00\"", "opens: \"9 am\""),
                "shop `grocer` is invalid: `9 am` is not a time such as 09:30",
            ),
            (
                SHOPS.replace("opens: \"09:00\"", "opens: \"05:00\""),
                "shop `grocer` is invalid: it must open after 06:00 and close later the same day",
            ),
            (
                SHOPS.replace("closes: \"17:00\"", "closes: \"08:00\""),
                "shop `grocer` is invalid: it must open after 06:00 and close later the same day",
            ),
            (
                SHOPS.replace("daily_limit: 20", "daily_limit: 0"),
                "shop `grocer` is invalid: its daily limit must allow selling something",
            ),
            (
                SHOPS.replace("(\"turnip\", 30)", "(\"turnip\", 0)"),
                "shop `grocer` is invalid: every price must be above 0",
            ),
            (
                SHOPS.replace("(\"berries\", 8)", "(\"turnip\", 8)"),
                "shop `grocer` is invalid: it lists an item twice",
            ),
            (
                SHOPS.replace("seasons: [Spring]", "seasons: []"),
                "shop `grocer` is invalid: an item on sale in no season should not be listed",
            ),
            (
                SHOPS.replace("daily_recovery: 0.25", "daily_recovery: 1.5"),
                "the market rules are invalid: the daily recovery must be between 0 and 1",
            ),
            (
                SHOPS.replace("gold_bonus: 1.5", "gold_bonus: 1.1"),
                "the market rules are invalid: quality bonuses must be at least 1 and grow with quality",
            ),
            (
                SHOPS.replace("halves_after: 100.0", "halves_after: 0.0"),
                "the market rules are invalid: prices must take a positive number of sales to halve",
            ),
        ];
        for (source, expected) in cases {
            assert_eq!(
                problem_with_shops(&source),
                format!("shops.ron: {expected}")
            );
        }
    }

    #[test]
    fn syntax_errors_name_the_file_and_position() {
        let message = problem_with_items(
            "(items: [(id: \"a\", name: \"A\", kind: Toll)], starting_inventory: [])",
        );
        assert!(message.starts_with("items.ron: 1:"), "{message}");
        assert!(message.contains("Toll"), "{message}");
    }

    #[test]
    fn unknown_references_are_named() {
        let message = problem_with_items(
            &ITEMS.replace("spoils_into: \"compost\"", "spoils_into: \"compot\""),
        );
        assert_eq!(
            message,
            "items.ron: item `berries` refers to unknown item `compot`"
        );

        let message = problem_with_items(&ITEMS.replace("(\"berries\", 4)", "(\"beries\", 4)"));
        assert_eq!(
            message,
            "items.ron: the starting inventory refers to unknown item `beries`"
        );

        let message =
            problem_with_crops(&CROPS.replace("produce: \"turnip\"", "produce: \"tunrip\""));
        assert_eq!(
            message,
            "crops.ron: crop `turnip` refers to unknown item `tunrip`"
        );
    }

    #[test]
    fn inconsistent_items_are_rejected() {
        let cases = [
            (
                ITEMS.replace("(id: \"compost\"", "(id: \"soil\""),
                "item `soil` is defined more than once",
            ),
            (
                ITEMS.replace("kind: Tool(Shovel))", "kind: Tool(Shovel), stack: 5)"),
                "item `shovel` is a tool, and tools do not stack",
            ),
            (
                ITEMS.replace("kind: Fertilizer, stack: 99", "kind: Fertilizer, stack: 0"),
                "item `compost` has a stack size of 0",
            ),
            (
                ITEMS.replace(", spoils_into: \"compost\"", ""),
                "item `berries` has a shelf life but does not say what it spoils into",
            ),
            (
                ITEMS.replace("spoils_into: \"compost\"", "spoils_into: \"berries\""),
                "item `berries` spoils into itself",
            ),
            (
                ITEMS.replace("[Stone, Sand]", "[Stone]"),
                "no item is dug from Sand",
            ),
            (
                ITEMS.replace("[Stone, Sand]", "[Stone, Sand, Soil]"),
                "Soil is dug as both `soil` and `stone`",
            ),
            (
                ITEMS.replace("(\"berries\", 4)", "(\"berries\", 0)"),
                "the starting inventory lists `berries` with a count of 0",
            ),
        ];
        for (source, expected) in cases {
            assert_eq!(
                problem_with_items(&source),
                format!("items.ron: {expected}")
            );
        }
    }

    #[test]
    fn inconsistent_crops_are_rejected() {
        let duplicate = CROPS.replace(
            "crops: [",
            "crops: [(id: \"turnip\", name: \"Again\", seeds: \"turnip_seeds\", produce: \"turnip\", seasons: [Spring], stages: [1], color: (0, 0, 0), models: [\"a.glb\", \"b.glb\"]),",
        );
        let cases = [
            (duplicate, "crop `turnip` is defined more than once"),
            (
                CROPS.replace("seeds: \"turnip_seeds\"", "seeds: \"berries\""),
                "crop `turnip` grows from `berries`, which is not a seed",
            ),
            (
                CROPS.replace("seasons: [Spring]", "seasons: []"),
                "crop `turnip` is invalid: it grows in no season",
            ),
            (
                CROPS.replace("stages: [1, 1, 2]", "stages: [1, 0]"),
                "crop `turnip` is invalid: every crop needs at least one stage, and every stage at least one day",
            ),
            (
                CROPS.replace("stages: [1, 1, 2],", "stages: [1, 1, 2], regrows_after: 9,"),
                "crop `turnip` is invalid: it must take between one day and its full growth to regrow",
            ),
            (
                CROPS.replace("stages: [1, 1, 2],", "stages: [1, 1, 2], harvest: 0,"),
                "crop `turnip` is invalid: a harvest must yield something",
            ),
            (
                "(crops: [])".to_owned(),
                "seed `turnip_seeds` does not grow any crop",
            ),
        ];
        for (source, expected) in cases {
            assert_eq!(
                problem_with_crops(&source),
                format!("crops.ron: {expected}")
            );
        }
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let message =
            problem_with_items(&ITEMS.replace("stack: 20,", "stack: 20, colour: \"red\","));
        assert!(message.contains("colour"), "{message}");
    }

    #[test]
    fn missing_files_name_the_path() {
        let folder = Path::new("no/such/folder");
        let message = Catalog::load(folder).unwrap_err().to_string();
        let expected = format!("{}: cannot be read", folder.join(ITEMS_FILE).display());
        assert!(message.starts_with(&expected), "{message}");
    }

    #[test]
    fn the_shipped_content_is_valid() {
        let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data");
        if let Err(error) = Catalog::load(&data_dir) {
            panic!("{error}");
        }
    }
}
