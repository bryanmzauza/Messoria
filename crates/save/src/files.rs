//! Reading and writing save files safely, and what every file shares.

use std::{
    fs::{self, File},
    io::{self, Write},
    path::Path,
};

use messoria_content::{Catalog, ItemId};
use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};

use crate::error::Problem;

/// Writes `bytes` to `path`, creating its folder if needed, so that the file
/// holds either all of its old contents or all of the new ones.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    let temporary = path.with_extension("tmp");
    let mut file = File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, path)
}

/// The contents of `path`, or `None` if there is no such file.
pub(crate) fn read_if_present(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// The start of every RON save file: the version of its format. The rest
/// of the file is ignored when reading just this.
#[derive(Deserialize)]
struct Header {
    version: u32,
}

/// The format version a RON save file was written in.
pub(crate) fn version_of(text: &str) -> Result<u32, Problem> {
    Ok(ron::from_str::<Header>(text)?.version)
}

/// Refuses files from versions this one does not know how to read.
pub(crate) fn unreadable(found: u32, supported: u32) -> Problem {
    if found > supported {
        Problem::Newer { found, supported }
    } else {
        Problem::UnknownVersion(found)
    }
}

pub(crate) fn to_ron<T: Serialize>(value: &T) -> Result<String, Problem> {
    Ok(ron::ser::to_string_pretty(value, PrettyConfig::default())?)
}

/// Resolves text ids from a save against the catalog, noting everything the
/// content no longer defines.
pub(crate) struct Resolver<'a> {
    pub catalog: &'a Catalog,
    /// Where the ids come from, for the notes.
    pub source: &'a str,
    pub lost: &'a mut Vec<String>,
}

impl Resolver<'_> {
    pub(crate) fn item(&mut self, key: &str) -> Option<ItemId> {
        let id = self.catalog.id(key);
        if id.is_none() {
            self.lost.push(format!(
                "{}: item `{key}` no longer exists and was left out",
                self.source
            ));
        }
        id
    }

    pub(crate) fn note(&mut self, what: &str) {
        self.lost.push(format!("{}: {what}", self.source));
    }
}
