//! Scenery definitions and the scenery file: the props scattered across the
//! valley, and the ground cover growing on grass.

use std::{collections::HashMap, path::Path};

use messoria_calendar::Season;
use messoria_voxel::Material;
use serde::{Deserialize, Serialize};

use crate::error::Problem;

/// Refers to a prop definition within a [`Catalog`](crate::Catalog), like
/// [`ItemId`](crate::ItemId) does for items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PropId(pub(crate) u16);

/// A kind of object standing in the valley, such as a tree or a rock.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropDef {
    /// The id used in data files, such as `"oak"`.
    #[serde(rename = "id")]
    pub key: String,
    pub name: String,
    /// Models, relative to the models folder; each prop uses one of them.
    pub models: Vec<String>,
    /// Smallest and largest scale the models are drawn at.
    pub scale: (f32, f32),
    /// Radius of the ground the prop stands on, in meters at scale 1. Nobody
    /// can dig, raise or till it.
    pub radius: f32,
    /// Size of the grid cells props are scattered in, in meters: at most one
    /// prop of this kind per cell.
    pub spacing: f32,
    /// Share of cells that have one, where it grows best.
    pub density: f32,
    /// How much props gather into groves and leave clearings, from 0 (evenly
    /// spread) to 1 (only in groves).
    #[serde(default)]
    pub clustering: f32,
    /// Steepest ground it grows on, as rise over run.
    pub max_slope: f32,
    /// Ground it grows on.
    pub grows_on: Vec<Material>,
}

/// Small plants covering grassy ground, drawn by clients only.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverDef {
    /// Models, relative to the models folder; each plant uses one of them.
    pub models: Vec<String>,
    /// Plants per square meter of grass.
    pub per_square_meter: f32,
    /// Smallest and largest scale the models are drawn at.
    pub scale: (f32, f32),
    /// Seasons it shows in; all year if empty.
    #[serde(default)]
    pub seasons: Vec<Season>,
}

impl CoverDef {
    pub fn shows_in(&self, season: Season) -> bool {
        self.seasons.is_empty() || self.seasons.contains(&season)
    }
}

/// The scenery file as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SceneryFile {
    props: Vec<PropDef>,
    cover: Vec<CoverDef>,
}

/// Scenery, checked.
pub(crate) struct Scenery {
    pub props: Vec<PropDef>,
    pub by_key: HashMap<String, PropId>,
    pub cover: Vec<CoverDef>,
}

impl SceneryFile {
    pub(crate) fn resolve(self) -> Result<Scenery, Problem> {
        let mut by_key = HashMap::new();
        for (prop, id) in self.props.iter().zip((0..=u16::MAX).map(PropId)) {
            if by_key.insert(prop.key.clone(), id).is_some() {
                return Err(Problem::DuplicateProp(prop.key.clone()));
            }
            validate_prop(prop)?;
        }
        for (index, cover) in self.cover.iter().enumerate() {
            validate_cover(index, cover)?;
        }
        Ok(Scenery {
            props: self.props,
            by_key,
            cover: self.cover,
        })
    }
}

fn validate_prop(prop: &PropDef) -> Result<(), Problem> {
    let problem = |reason: &str| {
        Err(Problem::InvalidProp {
            prop: prop.key.clone(),
            reason: reason.to_owned(),
        })
    };
    if let Err(reason) = check_models(&prop.models) {
        return problem(&reason);
    }
    if !valid_scale(prop.scale) {
        return problem("its scale must be a positive range, smallest first");
    }
    // Written as ranges so that a missing number (NaN) fails every check.
    if !(0.0..).contains(&prop.radius) {
        return problem("its radius cannot be negative");
    }
    if !(f32::MIN_POSITIVE..).contains(&prop.spacing) {
        return problem("its spacing must be above 0");
    }
    if !(f32::MIN_POSITIVE..=1.0).contains(&prop.density) {
        return problem("its density must be above 0 and at most 1");
    }
    if !(0.0..=1.0).contains(&prop.clustering) {
        return problem("its clustering must be between 0 and 1");
    }
    if !(0.0..).contains(&prop.max_slope) {
        return problem("its steepest slope cannot be negative");
    }
    if prop.grows_on.is_empty() {
        return problem("it grows on no ground");
    }
    Ok(())
}

fn validate_cover(index: usize, cover: &CoverDef) -> Result<(), Problem> {
    let problem = |reason: String| {
        Err(Problem::InvalidCover {
            index: index + 1,
            reason,
        })
    };
    if let Err(reason) = check_models(&cover.models) {
        return problem(reason);
    }
    if !valid_scale(cover.scale) {
        return problem("its scale must be a positive range, smallest first".to_owned());
    }
    if !(f32::MIN_POSITIVE..).contains(&cover.per_square_meter) {
        return problem(
            "it must grow somewhere: plants per square meter must be above 0".to_owned(),
        );
    }
    Ok(())
}

fn valid_scale((smallest, largest): (f32, f32)) -> bool {
    (f32::MIN_POSITIVE..).contains(&smallest) && (smallest..).contains(&largest)
}

/// Checks that models are listed, and that each is a glTF file inside the
/// models folder.
fn check_models(models: &[String]) -> Result<(), String> {
    if models.is_empty() {
        return Err("it lists no model".to_owned());
    }
    let inside_models = |model: &str| {
        let glb = Path::new(model)
            .extension()
            .is_some_and(|extension| extension == "glb");
        glb && !model.contains('\\')
            && model
                .split('/')
                .all(|part| !part.is_empty() && part != "..")
    };
    match models.iter().find(|model| !inside_models(model)) {
        Some(model) => Err(format!(
            "`{model}` is not a .glb file inside the models folder"
        )),
        None => Ok(()),
    }
}
