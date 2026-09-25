//! A world's save folder.

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};

use messoria_content::Catalog;
use messoria_voxel::{Chunk, ChunkPos};

use crate::{
    error::{Problem, SaveError},
    files::{Resolver, read_if_present, write_atomically},
    player::PlayerState,
    terrain,
    world::WorldState,
};

const WORLD_FILE: &str = "world.ron";
const TERRAIN_FILE: &str = "terrain.bin";
const PLAYERS_FOLDER: &str = "players";
const PLAYER_EXTENSION: &str = "ron";

/// The folder a world is saved in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveDir {
    root: PathBuf,
}

/// Everything read back from a world's save.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedWorld {
    pub world: WorldState,
    /// Chunks that differ from the generated world.
    pub terrain: Vec<(ChunkPos, Chunk)>,
    /// Every player who has joined, by key.
    pub players: HashMap<String, PlayerState>,
    /// Things the save refers to that the content no longer defines, which
    /// were left out.
    pub lost: Vec<String>,
}

/// Whether `key` can name a player's file: 1 to 64 lowercase letters,
/// digits, `-` or `_`.
pub fn is_valid_player_key(key: &str) -> bool {
    (1..=64).contains(&key.len())
        && key
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_".contains(&byte))
}

impl SaveDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Reads the world saved here, or `None` if nothing has been saved yet.
    ///
    /// # Errors
    ///
    /// If any file of the save cannot be read or makes no sense.
    pub fn load(&self, catalog: &Catalog) -> Result<Option<SavedWorld>, SaveError> {
        let world_path = self.root.join(WORLD_FILE);
        let Some(world) = read(&world_path)? else {
            return Ok(None);
        };
        let mut lost = Vec::new();
        let world = WorldState::from_ron(
            &text(&world, &world_path)?,
            Resolver {
                catalog,
                source: WORLD_FILE,
                lost: &mut lost,
            },
        )
        .map_err(|problem| in_file(&world_path, problem))?;

        let terrain_path = self.root.join(TERRAIN_FILE);
        let terrain = match read(&terrain_path)? {
            Some(bytes) => {
                terrain::decode(&bytes).map_err(|problem| in_file(&terrain_path, problem))?
            }
            None => Vec::new(),
        };

        let mut players = HashMap::new();
        for (key, path) in self.player_files()? {
            let Some(bytes) = read(&path)? else {
                continue;
            };
            let source = format!("{PLAYERS_FOLDER}/{key}.{PLAYER_EXTENSION}");
            let player = PlayerState::from_ron(
                &text(&bytes, &path)?,
                Resolver {
                    catalog,
                    source: &source,
                    lost: &mut lost,
                },
            )
            .map_err(|problem| in_file(&path, problem))?;
            players.insert(key, player);
        }

        Ok(Some(SavedWorld {
            world,
            terrain,
            players,
            lost,
        }))
    }

    /// Saves the world file.
    ///
    /// # Errors
    ///
    /// If the file cannot be written.
    pub fn save_world(&self, catalog: &Catalog, world: &WorldState) -> Result<(), SaveError> {
        let path = self.root.join(WORLD_FILE);
        let text = world
            .to_ron(catalog)
            .map_err(|problem| in_file(&path, problem))?;
        write(&path, text.as_bytes())
    }

    /// Saves every chunk that differs from the generated world, replacing the
    /// ones saved before.
    ///
    /// # Errors
    ///
    /// If the file cannot be written.
    pub fn save_terrain<'a>(
        &self,
        chunks: impl IntoIterator<Item = (ChunkPos, &'a Chunk)>,
    ) -> Result<(), SaveError> {
        write(&self.root.join(TERRAIN_FILE), &terrain::encode(chunks))
    }

    /// Saves one player's file.
    ///
    /// # Errors
    ///
    /// If the file cannot be written.
    ///
    /// # Panics
    ///
    /// If `key` is not a valid player key; see [`is_valid_player_key`].
    pub fn save_player(
        &self,
        catalog: &Catalog,
        key: &str,
        player: &PlayerState,
    ) -> Result<(), SaveError> {
        assert!(
            is_valid_player_key(key),
            "`{key}` cannot name a player file"
        );
        let path = self
            .root
            .join(PLAYERS_FOLDER)
            .join(format!("{key}.{PLAYER_EXTENSION}"));
        let text = player
            .to_ron(catalog)
            .map_err(|problem| in_file(&path, problem))?;
        write(&path, text.as_bytes())
    }

    /// Every player file, with the player's key.
    fn player_files(&self) -> Result<Vec<(String, PathBuf)>, SaveError> {
        let folder = self.root.join(PLAYERS_FOLDER);
        let entries = match fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(in_file(&folder, error.into())),
        };
        let mut files = Vec::new();
        for entry in entries {
            let path = entry
                .map_err(|error| in_file(&folder, error.into()))?
                .path();
            let is_player_file = path
                .extension()
                .is_some_and(|extension| extension == PLAYER_EXTENSION);
            // Leftovers of an interrupted save, and anything else, are ignored.
            let key = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|key| is_player_file && is_valid_player_key(key));
            if let Some(key) = key {
                files.push((key.to_owned(), path.clone()));
            }
        }
        Ok(files)
    }
}

fn in_file(path: &Path, problem: Problem) -> SaveError {
    SaveError {
        file: path.to_path_buf(),
        problem,
    }
}

fn read(path: &Path) -> Result<Option<Vec<u8>>, SaveError> {
    read_if_present(path).map_err(|error| in_file(path, error.into()))
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), SaveError> {
    write_atomically(path, bytes).map_err(|error| in_file(path, error.into()))
}

fn text(bytes: &[u8], path: &Path) -> Result<String, SaveError> {
    String::from_utf8(bytes.to_vec()).map_err(|error| {
        in_file(
            path,
            io::Error::new(io::ErrorKind::InvalidData, error).into(),
        )
    })
}

#[cfg(test)]
mod tests {
    use glam::{IVec2, IVec3, Vec3};
    use messoria_calendar::WorldTime;
    use messoria_content::Quality;
    use messoria_economy::{Market, SalesLedger, Wallet};
    use messoria_farming::Planting;
    use messoria_inventory::Inventory;
    use messoria_voxel::{Material, Voxel};

    use super::*;
    use crate::{
        profile::Profile,
        world::{FieldState, GatheredProp, StructureState},
    };

    fn catalog() -> Catalog {
        let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/data");
        Catalog::load(&data_dir).expect("the shipped content is valid")
    }

    /// An empty folder of its own for each test.
    fn scratch(name: &str) -> SaveDir {
        let root = std::env::temp_dir()
            .join(format!("messoria-save-tests-{}", std::process::id()))
            .join(name);
        let _ = fs::remove_dir_all(&root);
        SaveDir::new(root)
    }

    fn world(catalog: &Catalog) -> WorldState {
        let turnip = catalog.id("turnip").unwrap();
        WorldState {
            seed: 0xfeed,
            clock: WorldTime::at(12, "14:37".parse().unwrap()).unwrap(),
            market: [(turnip, 45.5)].into_iter().collect::<Market>(),
            fields: vec![
                FieldState {
                    tile: IVec2::new(-3, 7),
                    height: 8.2,
                    watered: true,
                    fertilized: false,
                    crop: Some(Planting {
                        crop: catalog.crop_id("potato").unwrap(),
                        days_grown: 3,
                    }),
                },
                FieldState {
                    tile: IVec2::new(-2, 7),
                    height: 8.25,
                    watered: false,
                    fertilized: true,
                    crop: None,
                },
            ],
            gathered: vec![GatheredProp {
                kind: catalog.prop_id("oak").unwrap(),
                cell: [12, 40],
                day: 11,
            }],
            structures: vec![
                StructureState {
                    kind: catalog.structure_id("cabin").unwrap(),
                    position: Vec3::new(30.0, 8.5, -12.0),
                    facing: std::f32::consts::PI,
                    home_of: Some("host".to_owned()),
                    stored: None,
                },
                StructureState {
                    kind: catalog.structure_id("chest").unwrap(),
                    position: Vec3::new(31.6, 8.5, -13.95),
                    facing: 0.0,
                    home_of: None,
                    stored: Some({
                        let mut stored = Inventory::default();
                        stored.add(catalog, catalog.id("wood").unwrap(), Quality::Normal, 40, 0);
                        stored
                    }),
                },
            ],
        }
    }

    fn player(catalog: &Catalog) -> PlayerState {
        let mut inventory = Inventory::default();
        inventory.add(
            catalog,
            catalog.id("shovel").unwrap(),
            Quality::Normal,
            1,
            0,
        );
        inventory.add(catalog, catalog.id("turnip").unwrap(), Quality::Gold, 7, 10);
        let grocer = catalog.shop_id("grocer").unwrap();
        PlayerState {
            position: Vec3::new(1.5, 8.125, -4.75),
            heading: 2.5,
            energy: 64,
            inventory,
            money: Wallet::with(1234),
            sold_today: [(grocer, catalog.id("turnip").unwrap(), 12)]
                .into_iter()
                .collect::<SalesLedger>(),
            saved_on: 12,
        }
    }

    #[test]
    fn a_folder_without_a_save_loads_as_nothing() {
        let save = scratch("empty");
        assert_eq!(save.load(&catalog()).unwrap(), None);
    }

    #[test]
    fn everything_saved_loads_back_the_same() {
        let (catalog, save) = (catalog(), scratch("round-trip"));
        let (world, player) = (world(&catalog), player(&catalog));
        let chunk = Chunk::uniform(Voxel::new(-2.0, Material::Stone));
        let position = ChunkPos(IVec3::new(-1, 0, 3));

        save.save_world(&catalog, &world).unwrap();
        save.save_terrain([(position, &chunk)]).unwrap();
        save.save_player(&catalog, "0123abcd", &player).unwrap();

        let loaded = save.load(&catalog).unwrap().expect("a world was saved");
        assert_eq!(loaded.world, world);
        assert_eq!(loaded.terrain, vec![(position, chunk)]);
        assert_eq!(loaded.players.get("0123abcd"), Some(&player));
        assert!(loaded.lost.is_empty(), "{:?}", loaded.lost);
    }

    #[test]
    fn saves_from_newer_versions_are_refused() {
        let (catalog, save) = (catalog(), scratch("newer"));
        save.save_world(&catalog, &world(&catalog)).unwrap();
        let path = save.root().join(WORLD_FILE);
        let text = fs::read_to_string(&path)
            .unwrap()
            .replacen("version: 4", "version: 9", 1);
        fs::write(&path, text).unwrap();

        let error = save.load(&catalog).unwrap_err();
        assert_eq!(error.file, path);
        assert!(matches!(
            error.problem,
            Problem::Newer {
                found: 9,
                supported: 4
            }
        ));
    }

    #[test]
    fn worlds_of_the_smaller_valley_are_refused_with_a_reason() {
        let (catalog, save) = (catalog(), scratch("smaller-valley"));
        save.save_world(&catalog, &world(&catalog)).unwrap();
        let path = save.root().join(WORLD_FILE);
        let text = fs::read_to_string(&path)
            .unwrap()
            .replacen("version: 4", "version: 3", 1);
        fs::write(&path, text).unwrap();

        let error = save.load(&catalog).unwrap_err();
        assert!(matches!(error.problem, Problem::OlderValley(3)));
        assert!(error.to_string().contains("start a new world"));
    }

    #[test]
    fn gathered_props_of_kinds_that_no_longer_exist_are_forgotten() {
        let (catalog, save) = (catalog(), scratch("lost-prop"));
        save.save_world(&catalog, &world(&catalog)).unwrap();
        let path = save.root().join(WORLD_FILE);
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("\"oak\"", "\"baobab\"");
        fs::write(&path, text).unwrap();

        let loaded = save.load(&catalog).unwrap().expect("a world was saved");
        assert!(loaded.world.gathered.is_empty());
        assert!(loaded.lost[0].contains("`baobab`"), "{:?}", loaded.lost);
    }

    #[test]
    fn content_that_no_longer_exists_is_left_out_and_reported() {
        let (catalog, save) = (catalog(), scratch("lost"));
        save.save_world(&catalog, &world(&catalog)).unwrap();
        save.save_player(&catalog, "host", &player(&catalog))
            .unwrap();
        let path = save.root().join(PLAYERS_FOLDER).join("host.ron");
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("\"turnip\"", "\"golden_turnip\"");
        fs::write(&path, text).unwrap();

        let loaded = save.load(&catalog).unwrap().expect("a world was saved");
        let player = &loaded.players["host"];
        assert_eq!(player.inventory.count(catalog.id("shovel").unwrap()), 1);
        assert_eq!(player.inventory.count(catalog.id("turnip").unwrap()), 0);
        assert!(player.sold_today.is_empty());
        assert_eq!(loaded.lost.len(), 2, "{:?}", loaded.lost);
        assert!(
            loaded.lost[0].contains("`golden_turnip`"),
            "{:?}",
            loaded.lost
        );
    }

    #[test]
    fn saving_leaves_no_temporary_files_and_ignores_stray_ones() {
        let (catalog, save) = (catalog(), scratch("temporary"));
        save.save_world(&catalog, &world(&catalog)).unwrap();
        save.save_player(&catalog, "host", &player(&catalog))
            .unwrap();
        fs::write(
            save.root().join(PLAYERS_FOLDER).join("other.tmp"),
            "half written",
        )
        .unwrap();

        let files: Vec<_> = fs::read_dir(save.root())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert!(
            files
                .iter()
                .all(|name| !name.to_string_lossy().ends_with(".tmp"))
        );
        let loaded = save.load(&catalog).unwrap().expect("a world was saved");
        assert_eq!(loaded.players.keys().collect::<Vec<_>>(), ["host"]);
    }

    #[test]
    fn truncated_terrain_is_reported() {
        let (catalog, save) = (catalog(), scratch("truncated"));
        save.save_world(&catalog, &world(&catalog)).unwrap();
        let chunk = Chunk::uniform(Voxel::AIR);
        save.save_terrain([(ChunkPos(IVec3::ZERO), &chunk)])
            .unwrap();
        let path = save.root().join(TERRAIN_FILE);
        let bytes = fs::read(&path).unwrap();
        fs::write(&path, &bytes[..bytes.len() - 1]).unwrap();

        let error = save.load(&catalog).unwrap_err();
        assert!(matches!(error.problem, Problem::Truncated), "{error}");
    }

    #[test]
    fn a_profile_keeps_its_id() {
        let save = scratch("profile");
        let path = save.root().join("profile.ron");
        let created = Profile::load_or_create(&path).unwrap();
        assert_eq!(Profile::load_or_create(&path).unwrap(), created);
    }

    #[test]
    fn player_keys_are_safe_file_names() {
        assert!(is_valid_player_key("host"));
        assert!(is_valid_player_key("00ff12ab34cd56ef"));
        assert!(!is_valid_player_key(""));
        assert!(!is_valid_player_key("../world"));
        assert!(!is_valid_player_key("Host"));
    }
}
