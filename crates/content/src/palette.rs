//! The palette: the colors foliage and the ground are tinted, by material
//! name, and how far foliage sways in the wind.
//!
//! Models name their materials, such as `leaves` or `grass`, and the palette
//! gives a name one color, which tints the texture of every material of that
//! name. The terrain's ground materials are colored the same way. Seasons
//! override some colors for part of the year: leaves turn in autumn and grass
//! is under snow in winter.

use std::collections::HashMap;

use messoria_calendar::Season;
use messoria_voxel::Material;
use serde::Deserialize;

use crate::error::Problem;

/// A color in sRGB, each channel from 0 to 1.
pub type Rgb = [f32; 3];

#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    colors: HashMap<String, Rgb>,
    seasons: HashMap<Season, HashMap<String, Rgb>>,
    sway: HashMap<String, f32>,
}

impl Palette {
    /// The color of material `name` in `season`, if the palette has one.
    pub fn color(&self, name: &str, season: Season) -> Option<Rgb> {
        self.seasons
            .get(&season)
            .and_then(|overrides| overrides.get(name))
            .or_else(|| self.colors.get(name))
            .copied()
    }

    /// How far the tops of plants drawn with material `name` sway in the
    /// wind, in meters, if they do.
    pub fn sway(&self, name: &str) -> Option<f32> {
        self.sway.get(name).copied()
    }

    /// The color of terrain made of `material` in `season`.
    ///
    /// # Panics
    ///
    /// Never for a loaded palette, which is checked to color every ground
    /// material.
    pub fn ground(&self, material: Material, season: Season) -> Rgb {
        self.color(ground_name(material), season)
            .expect("a loaded palette colors every ground material")
    }
}

/// The name the palette gives the terrain's `material`.
fn ground_name(material: Material) -> &'static str {
    match material {
        Material::Grass => "terrain_grass",
        Material::Soil => "terrain_soil",
        Material::Stone => "terrain_stone",
        Material::Sand => "terrain_sand",
    }
}

/// The palette file as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PaletteFile {
    colors: HashMap<String, (f32, f32, f32)>,
    #[serde(default)]
    seasons: HashMap<Season, HashMap<String, (f32, f32, f32)>>,
    #[serde(default)]
    sway: HashMap<String, f32>,
}

/// A color as written in a data file, if every channel is between 0 and 1.
pub(crate) fn checked_color((red, green, blue): (f32, f32, f32)) -> Option<Rgb> {
    let color = [red, green, blue];
    color
        .iter()
        .all(|channel| (0.0..=1.0).contains(channel))
        .then_some(color)
}

impl PaletteFile {
    pub(crate) fn resolve(self) -> Result<Palette, Problem> {
        let rgb = |name: &str, color: (f32, f32, f32)| {
            checked_color(color).ok_or_else(|| Problem::InvalidColor(name.to_owned()))
        };
        let colors = self
            .colors
            .iter()
            .map(|(name, &color)| Ok((name.clone(), rgb(name, color)?)))
            .collect::<Result<HashMap<_, _>, Problem>>()?;
        if let Some(missing) = Material::ALL
            .iter()
            .map(|&material| ground_name(material))
            .find(|name| !colors.contains_key(*name))
        {
            return Err(Problem::UncoloredGround(missing.to_owned()));
        }

        let mut seasons = HashMap::new();
        for (season, overrides) in self.seasons {
            let mut resolved = HashMap::new();
            for (name, color) in overrides {
                if !colors.contains_key(&name) {
                    return Err(Problem::UnknownColor { season, name });
                }
                let color = rgb(&name, color)?;
                resolved.insert(name, color);
            }
            seasons.insert(season, resolved);
        }
        if let Some((name, _)) = self
            .sway
            .iter()
            .find(|(_, strength)| !(0.0..=1.0).contains(*strength))
        {
            return Err(Problem::InvalidSway(name.clone()));
        }
        Ok(Palette {
            colors,
            seasons,
            sway: self.sway,
        })
    }
}
