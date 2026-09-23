//! Loading the content files and checking them against each other.

use std::{collections::HashMap, fs, path::Path};

use messoria_voxel::Material;
use ron::{Options, extensions::Extensions};
use serde::de::DeserializeOwned;

use crate::{
    crop::{CropDef, CropId, Crops, CropsFile},
    error::{ContentError, Problem},
    item::{ItemDef, ItemId, ItemKind, Items, ItemsFile},
};

/// Files within the data folder.
const ITEMS_FILE: &str = "items.ron";
const CROPS_FILE: &str = "crops.ron";

/// All loaded content, with every cross-reference checked.
#[derive(Clone, Debug)]
pub struct Catalog {
    items: Vec<ItemDef>,
    item_keys: HashMap<String, ItemId>,
    dug_items: HashMap<Material, ItemId>,
    starting_inventory: Vec<(ItemId, u16)>,
    crops: Vec<CropDef>,
    crop_keys: HashMap<String, CropId>,
    crops_by_seed: HashMap<ItemId, CropId>,
}

impl Catalog {
    /// Loads content from the data folder at `data_dir`.
    ///
    /// # Errors
    ///
    /// Fails with the file and the problem if any file is missing, malformed
    /// or inconsistent.
    pub fn load(data_dir: &Path) -> Result<Self, ContentError> {
        let read = |file: &str| {
            let path = data_dir.join(file);
            fs::read_to_string(&path)
                .map(|source| (source, path.clone()))
                .map_err(|error| ContentError {
                    file: path,
                    problem: error.into(),
                })
        };
        let (items, items_path) = read(ITEMS_FILE)?;
        let (crops, crops_path) = read(CROPS_FILE)?;
        Self::build(&items, &items_path, &crops, &crops_path)
    }

    /// Builds a catalog from the text of the content files.
    ///
    /// # Errors
    ///
    /// Fails if any text is malformed or inconsistent.
    pub fn from_sources(items: &str, crops: &str) -> Result<Self, ContentError> {
        Self::build(items, Path::new(ITEMS_FILE), crops, Path::new(CROPS_FILE))
    }

    fn build(
        items: &str,
        items_path: &Path,
        crops: &str,
        crops_path: &Path,
    ) -> Result<Self, ContentError> {
        let in_file = |file: &Path| {
            let file = file.to_path_buf();
            move |problem| ContentError { file, problem }
        };
        let items: Items = parse::<ItemsFile>(items)
            .and_then(ItemsFile::resolve)
            .map_err(in_file(items_path))?;
        let dug_items = dug_items(&items.definitions).map_err(in_file(items_path))?;
        let crops: Crops = parse::<CropsFile>(crops)
            .and_then(|file| file.resolve(&items))
            .map_err(in_file(crops_path))?;

        Ok(Self {
            items: items.definitions,
            item_keys: items.by_key,
            dug_items,
            starting_inventory: items.starting_inventory,
            crops: crops.definitions,
            crop_keys: crops.by_key,
            crops_by_seed: crops.by_seed,
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
    use messoria_calendar::Season;

    use super::*;

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
    )"#;

    const CROPS: &str = r#"(
        crops: [
            (
                id: "turnip", name: "Turnip", seeds: "turnip_seeds", produce: "turnip",
                seasons: [Spring], stages: [1, 1, 2], color: (0.9, 0.8, 0.9),
            ),
        ],
    )"#;

    fn problem_with_items(items: &str) -> String {
        Catalog::from_sources(items, CROPS)
            .expect_err("content should be rejected")
            .to_string()
    }

    fn problem_with_crops(crops: &str) -> String {
        Catalog::from_sources(ITEMS, crops)
            .expect_err("content should be rejected")
            .to_string()
    }

    #[test]
    fn valid_content_loads_with_every_reference_resolved() {
        let catalog = Catalog::from_sources(ITEMS, CROPS).unwrap();
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
    }

    #[test]
    fn crops_link_their_seeds_and_produce() {
        let catalog = Catalog::from_sources(ITEMS, CROPS).unwrap();
        let turnip = catalog.crop_id("turnip").unwrap();
        let seeds = catalog.id("turnip_seeds").unwrap();

        assert_eq!(catalog.crop_grown_from(seeds), Some(turnip));
        assert_eq!(catalog.crop(turnip).produce, catalog.id("turnip").unwrap());
        assert_eq!(catalog.crop(turnip).seasons, [Season::Spring]);
        assert_eq!(catalog.crop(turnip).days_to_ripen(), 4);
        assert_eq!(catalog.crop(turnip).regrows_after, None);
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
            "crops: [(id: \"turnip\", name: \"Again\", seeds: \"turnip_seeds\", produce: \"turnip\", seasons: [Spring], stages: [1], color: (0, 0, 0)),",
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
