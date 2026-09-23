//! Loading and validating the content catalog.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use messoria_voxel::Material;
use ron::{Options, extensions::Extensions};
use serde::Deserialize;

use crate::{
    error::{ContentError, Problem},
    item::{ItemDef, ItemId, ItemKind},
};

/// File within the data folder that defines items.
const ITEMS_FILE: &str = "items.ron";

/// All loaded content, with every cross-reference checked.
#[derive(Clone, Debug)]
pub struct Catalog {
    items: Vec<ItemDef>,
    by_key: HashMap<String, ItemId>,
    dug_items: HashMap<Material, ItemId>,
    starting_inventory: Vec<(ItemId, u16)>,
}

/// `items.ron` as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemsFile {
    items: Vec<ItemEntry>,
    starting_inventory: Vec<(String, u16)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemEntry {
    id: String,
    name: String,
    kind: ItemKind,
    #[serde(default = "single")]
    stack: u16,
    #[serde(default)]
    shelf_life: Option<u16>,
    #[serde(default)]
    spoils_into: Option<String>,
}

fn single() -> u16 {
    1
}

impl Catalog {
    /// Loads content from the data folder at `data_dir`.
    ///
    /// # Errors
    ///
    /// Fails with the file and the problem if any file is missing, malformed
    /// or inconsistent.
    pub fn load(data_dir: &Path) -> Result<Self, ContentError> {
        let path = data_dir.join(ITEMS_FILE);
        let source = fs::read_to_string(&path).map_err(|error| ContentError {
            file: path.clone(),
            problem: error.into(),
        })?;
        Self::parse_items(&source).map_err(|problem| ContentError {
            file: path,
            problem,
        })
    }

    /// Builds a catalog from the text of an items file.
    ///
    /// # Errors
    ///
    /// Fails if the text is malformed or inconsistent.
    pub fn from_items_source(source: &str) -> Result<Self, ContentError> {
        Self::parse_items(source).map_err(|problem| ContentError {
            file: PathBuf::from(ITEMS_FILE),
            problem,
        })
    }

    fn parse_items(source: &str) -> Result<Self, Problem> {
        // Implicit `Some` lets optional fields be written as plain values.
        let file: ItemsFile = Options::default()
            .with_default_extension(Extensions::IMPLICIT_SOME)
            .from_str(source)?;

        let mut by_key = HashMap::new();
        for (index, entry) in file.items.iter().enumerate() {
            let id = ItemId(u16::try_from(index).expect("fewer than 65536 items"));
            if by_key.insert(entry.id.clone(), id).is_some() {
                return Err(Problem::DuplicateItem(entry.id.clone()));
            }
        }
        let resolve = |context: String, key: &str| {
            by_key
                .get(key)
                .copied()
                .ok_or_else(|| Problem::UnknownItem {
                    context,
                    item: key.to_owned(),
                })
        };

        let mut items = Vec::with_capacity(file.items.len());
        for entry in file.items {
            let spoils_into = entry
                .spoils_into
                .as_deref()
                .map(|key| resolve(format!("item `{}`", entry.id), key))
                .transpose()?;
            let item = ItemDef {
                key: entry.id,
                name: entry.name,
                kind: entry.kind,
                max_stack: entry.stack,
                shelf_life: entry.shelf_life,
                spoils_into,
            };
            validate_item(&item, &by_key)?;
            items.push(item);
        }

        let starting_inventory = file
            .starting_inventory
            .iter()
            .map(|(key, count)| {
                if *count == 0 {
                    return Err(Problem::EmptyStartingStack(key.clone()));
                }
                Ok((resolve("the starting inventory".to_owned(), key)?, *count))
            })
            .collect::<Result<_, _>>()?;

        Ok(Self {
            dug_items: dug_items(&items)?,
            items,
            by_key,
            starting_inventory,
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
        self.by_key.get(key).copied()
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
}

fn validate_item(item: &ItemDef, by_key: &HashMap<String, ItemId>) -> Result<(), Problem> {
    if item.max_stack == 0 {
        return Err(Problem::EmptyStack(item.key.clone()));
    }
    if matches!(item.kind, ItemKind::Tool(_)) && item.max_stack > 1 {
        return Err(Problem::StackedTool(item.key.clone()));
    }
    if item.shelf_life.is_some() && item.spoils_into.is_none() {
        return Err(Problem::SpoilsIntoNothing(item.key.clone()));
    }
    if item.spoils_into.is_some() && item.spoils_into == by_key.get(&item.key).copied() {
        return Err(Problem::SpoilsIntoItself(item.key.clone()));
    }
    Ok(())
}

/// Maps every terrain material to the one item digging it yields.
fn dug_items(items: &[ItemDef]) -> Result<HashMap<Material, ItemId>, Problem> {
    let mut dug_items: HashMap<Material, ItemId> = HashMap::new();
    for (index, item) in items.iter().enumerate() {
        let ItemKind::Terrain { materials } = &item.kind else {
            continue;
        };
        let id = ItemId(u16::try_from(index).expect("fewer than 65536 items"));
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
    use super::*;

    const VALID: &str = r#"(
        items: [
            (id: "shovel", name: "Shovel", kind: Tool(Shovel)),
            (id: "soil", name: "Soil", kind: Terrain(materials: [Grass, Soil]), stack: 99),
            (id: "stone", name: "Stone", kind: Terrain(materials: [Stone, Sand]), stack: 99),
            (id: "berries", name: "Berries", kind: Food(energy: 5), stack: 20, shelf_life: 3, spoils_into: "compost"),
            (id: "compost", name: "Compost", kind: Goods, stack: 99),
        ],
        starting_inventory: [("shovel", 1), ("berries", 4)],
    )"#;

    fn problem(source: &str) -> String {
        Catalog::from_items_source(source)
            .expect_err("content should be rejected")
            .to_string()
    }

    #[test]
    fn valid_content_loads_with_every_reference_resolved() {
        let catalog = Catalog::from_items_source(VALID).unwrap();
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
    fn syntax_errors_name_the_file_and_position() {
        let message =
            problem("(items: [(id: \"a\", name: \"A\", kind: Toll)], starting_inventory: [])");
        assert!(message.starts_with("items.ron: 1:"), "{message}");
        assert!(message.contains("Toll"), "{message}");
    }

    #[test]
    fn unknown_references_are_named() {
        let message =
            problem(&VALID.replace("spoils_into: \"compost\"", "spoils_into: \"compot\""));
        assert_eq!(
            message,
            "items.ron: item `berries` refers to unknown item `compot`"
        );

        let message = problem(&VALID.replace("(\"berries\", 4)", "(\"beries\", 4)"));
        assert_eq!(
            message,
            "items.ron: the starting inventory refers to unknown item `beries`"
        );
    }

    #[test]
    fn inconsistent_items_are_rejected() {
        let cases = [
            (
                VALID.replace("(id: \"compost\"", "(id: \"soil\""),
                "item `soil` is defined more than once",
            ),
            (
                VALID.replace("kind: Tool(Shovel))", "kind: Tool(Shovel), stack: 5)"),
                "item `shovel` is a tool, and tools do not stack",
            ),
            (
                VALID.replace("kind: Goods, stack: 99", "kind: Goods, stack: 0"),
                "item `compost` has a stack size of 0",
            ),
            (
                VALID.replace(", spoils_into: \"compost\"", ""),
                "item `berries` has a shelf life but does not say what it spoils into",
            ),
            (
                VALID.replace("spoils_into: \"compost\"", "spoils_into: \"berries\""),
                "item `berries` spoils into itself",
            ),
            (
                VALID.replace("[Stone, Sand]", "[Stone]"),
                "no item is dug from Sand",
            ),
            (
                VALID.replace("[Stone, Sand]", "[Stone, Sand, Soil]"),
                "Soil is dug as both `soil` and `stone`",
            ),
            (
                VALID.replace("(\"berries\", 4)", "(\"berries\", 0)"),
                "the starting inventory lists `berries` with a count of 0",
            ),
        ];
        for (source, expected) in cases {
            assert_eq!(problem(&source), format!("items.ron: {expected}"));
        }
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let message = problem(&VALID.replace("stack: 20,", "stack: 20, colour: \"red\","));
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
