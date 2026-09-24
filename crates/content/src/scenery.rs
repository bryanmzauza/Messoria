//! Scenery definitions and the scenery file: the props scattered across the
//! valley, and the ground cover growing on grass.

use std::{collections::HashMap, path::Path};

use messoria_calendar::Season;
use messoria_voxel::Material;
use serde::{Deserialize, Serialize};

use crate::{
    error::Problem,
    item::{ItemId, Items, Tool},
    palette::{Rgb, checked_color},
};

/// Refers to a prop definition within a [`Catalog`](crate::Catalog), like
/// [`ItemId`](crate::ItemId) does for items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PropId(pub(crate) u16);

/// A kind of object standing in the valley, such as a tree or a rock.
#[derive(Clone, Debug, PartialEq)]
pub struct PropDef {
    /// The id used in data files, such as `"oak"`.
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
    pub clustering: f32,
    /// Steepest ground it grows on, as rise over run.
    pub max_slope: f32,
    /// Ground it grows on.
    pub grows_on: Vec<Material>,
    /// Whether characters walk into it rather than through it.
    pub blocks: bool,
    /// What players get from it, if anything.
    pub gather: Option<Gathering>,
}

impl PropDef {
    /// Every model the prop may be drawn with, standing or gathered.
    pub fn models_used(&self) -> impl Iterator<Item = &String> {
        let remains = self
            .gather
            .as_ref()
            .and_then(|gather| match &gather.remains {
                Remains::Model { model, .. } => Some(model),
                Remains::Nothing | Remains::Itself => None,
            });
        self.models.iter().chain(remains)
    }

    /// Whether the prop still stands in the way, and keeps the ground under
    /// it, once gathered.
    pub fn stands_when_gathered(&self) -> bool {
        self.gather
            .as_ref()
            .is_none_or(|gather| gather.remains != Remains::Nothing)
    }
}

/// How a prop is gathered, and what it gives.
#[derive(Clone, Debug, PartialEq)]
pub struct Gathering {
    /// The tool it is gathered with; by hand if none.
    pub tool: Option<Tool>,
    /// Uses of the tool it takes to gather.
    pub strikes: u8,
    /// Items it gives when gathered.
    pub yields: Vec<(ItemId, u16)>,
    /// What stands where it was gathered.
    pub remains: Remains,
    /// Days after being gathered that it stands as before, if it grows back.
    pub regrows_after: Option<u16>,
    /// Color of the fruit drawn on it while it can be gathered.
    pub fruit: Option<Rgb>,
}

/// What stands where a prop was gathered.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub enum Remains {
    /// Nothing: the ground is free.
    #[default]
    Nothing,
    /// The prop itself, without its fruit.
    Itself,
    /// Another model, such as a tree's stump, drawn at `scale` times the
    /// prop's.
    Model {
        model: String,
        #[serde(default = "same_scale")]
        scale: f32,
    },
}

fn same_scale() -> f32 {
    1.0
}

/// A prop as written in the scenery file.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PropEntry {
    id: String,
    name: String,
    models: Vec<String>,
    scale: (f32, f32),
    radius: f32,
    spacing: f32,
    density: f32,
    #[serde(default)]
    clustering: f32,
    max_slope: f32,
    grows_on: Vec<Material>,
    #[serde(default = "solid")]
    blocks: bool,
    #[serde(default)]
    gather: Option<GatherEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GatherEntry {
    #[serde(default)]
    tool: Option<Tool>,
    #[serde(default = "one_strike")]
    strikes: u8,
    yields: Vec<(String, u16)>,
    #[serde(default)]
    remains: Remains,
    #[serde(default)]
    regrows_after: Option<u16>,
    #[serde(default)]
    fruit: Option<(f32, f32, f32)>,
}

fn solid() -> bool {
    true
}

fn one_strike() -> u8 {
    1
}

impl PropEntry {
    fn resolve(self, items: &Items) -> Result<PropDef, Problem> {
        let gather = match self.gather {
            None => None,
            Some(entry) => {
                let context = || format!("prop `{}`", self.id);
                let yields = entry
                    .yields
                    .iter()
                    .map(|(item, count)| Ok((items.resolve(context, item)?, *count)))
                    .collect::<Result<_, Problem>>()?;
                let fruit = entry
                    .fruit
                    .map(|color| {
                        checked_color(color).ok_or_else(|| Problem::InvalidProp {
                            prop: self.id.clone(),
                            reason: "its fruit has a color channel outside 0 to 1".to_owned(),
                        })
                    })
                    .transpose()?;
                Some(Gathering {
                    tool: entry.tool,
                    strikes: entry.strikes,
                    yields,
                    remains: entry.remains,
                    regrows_after: entry.regrows_after,
                    fruit,
                })
            }
        };
        Ok(PropDef {
            key: self.id,
            name: self.name,
            models: self.models,
            scale: self.scale,
            radius: self.radius,
            spacing: self.spacing,
            density: self.density,
            clustering: self.clustering,
            max_slope: self.max_slope,
            grows_on: self.grows_on,
            blocks: self.blocks,
            gather,
        })
    }
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
    props: Vec<PropEntry>,
    cover: Vec<CoverDef>,
}

/// Scenery, checked.
pub(crate) struct Scenery {
    pub props: Vec<PropDef>,
    pub by_key: HashMap<String, PropId>,
    pub cover: Vec<CoverDef>,
}

impl SceneryFile {
    pub(crate) fn resolve(self, items: &Items) -> Result<Scenery, Problem> {
        let mut by_key = HashMap::new();
        let mut props = Vec::with_capacity(self.props.len());
        for (entry, id) in self.props.into_iter().zip((0..=u16::MAX).map(PropId)) {
            if by_key.insert(entry.id.clone(), id).is_some() {
                return Err(Problem::DuplicateProp(entry.id));
            }
            let prop = entry.resolve(items)?;
            validate_prop(&prop)?;
            props.push(prop);
        }
        for (index, cover) in self.cover.iter().enumerate() {
            validate_cover(index, cover)?;
        }
        Ok(Scenery {
            props,
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
    match &prop.gather {
        Some(gather) => validate_gathering(prop, gather).or_else(problem),
        None => Ok(()),
    }
}

fn validate_gathering(prop: &PropDef, gather: &Gathering) -> Result<(), &'static str> {
    if gather.yields.is_empty() || gather.yields.iter().any(|&(_, count)| count == 0) {
        return Err("gathering it must yield at least one of each item it lists");
    }
    if !gather.tool.is_none_or(Tool::gathers) {
        return Err("only the axe and the pickaxe gather scenery");
    }
    if gather.strikes == 0 || (gather.tool.is_none() && gather.strikes != 1) {
        return Err("it takes one strike by hand, and at least one with a tool");
    }
    if gather.regrows_after.is_some() && gather.remains == Remains::Nothing {
        return Err("only what leaves something standing can grow back");
    }
    if gather.regrows_after == Some(0) {
        return Err("it cannot grow back in 0 days");
    }
    // Players aim at props through the obstacles they make.
    if !prop.blocks {
        return Err("it can be gathered, so it must block");
    }
    if let Remains::Model { model, scale } = &gather.remains {
        if check_models(std::slice::from_ref(model)).is_err() {
            return Err("what remains of it must be a .glb file inside the models folder");
        }
        if !(f32::MIN_POSITIVE..).contains(scale) {
            return Err("what remains of it must be drawn at a scale above 0");
        }
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
