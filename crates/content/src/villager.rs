//! The villagers file: who lives in the village, how each looks, where they
//! live and keep shop, and what they do through the day.
//!
//! A villager's day is a list of slots, each saying where they are between
//! two times, on which days, in which seasons and weather. The first slot
//! that fits the moment holds; when none does, the villager is at home.

use std::collections::HashMap;

use messoria_calendar::{ClockTime, Season, Weekday, WorldTime};
use serde::{Deserialize, Serialize};

use crate::{error::Problem, people::Look, shop::ShopId, village::Village};

/// Refers to a villager within a [`Catalog`](crate::Catalog).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VillagerId(pub(crate) u16);

#[derive(Clone, Debug, PartialEq)]
pub struct VillagerDef {
    /// The id used in data files, such as `"hester"`.
    pub key: String,
    pub name: String,
    pub look: Look,
    /// The label of the house in `village.ron` they live in.
    pub home: String,
    /// The shop they keep, whose customers they greet.
    pub keeps: Option<ShopId>,
    pub day: Vec<Slot>,
}

/// Where a villager is from one time of day to another, when the slot's
/// conditions hold.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub from: ClockTime,
    pub to: ClockTime,
    pub at: Activity,
    /// Only on these weekdays, if given.
    pub on: Option<Vec<Weekday>>,
    /// Only in these seasons, if given.
    pub seasons: Option<Vec<Season>>,
    /// Only when it rains (`true`) or does not (`false`), if given.
    pub rain: Option<bool>,
}

impl Slot {
    /// Whether the slot holds at `time`, when it `rains` or not.
    pub fn holds(&self, time: WorldTime, rains: bool) -> bool {
        let clock = time.clock();
        self.from <= clock
            && clock < self.to
            && self
                .on
                .as_ref()
                .is_none_or(|days| days.contains(&time.date().weekday))
            && self
                .seasons
                .as_ref()
                .is_none_or(|seasons| seasons.contains(&time.season()))
            && self.rain.is_none_or(|rain| rain == rains)
    }
}

/// What a villager does in a slot.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub enum Activity {
    /// Indoors at home.
    Home,
    /// Standing, sitting or working at the spot with this label.
    At(String),
    /// Walking from one of these spots to the next, around and around.
    Stroll(Vec<String>),
}

impl VillagerDef {
    /// What the villager does at `time`, when it `rains` or not.
    pub fn activity(&self, time: WorldTime, rains: bool) -> &Activity {
        self.day
            .iter()
            .find(|slot| slot.holds(time, rains))
            .map_or(&Activity::Home, |slot| &slot.at)
    }
}

/// The villagers file as written.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VillagersFile {
    villagers: Vec<VillagerEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VillagerEntry {
    id: String,
    name: String,
    look: Look,
    home: String,
    #[serde(default)]
    keeps: Option<String>,
    day: Vec<SlotEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SlotEntry {
    from: String,
    to: String,
    at: Activity,
    #[serde(default)]
    on: Option<Vec<Weekday>>,
    #[serde(default)]
    seasons: Option<Vec<Season>>,
    #[serde(default)]
    rain: Option<bool>,
}

/// Villagers, checked.
pub(crate) struct Villagers {
    pub definitions: Vec<VillagerDef>,
    pub by_key: HashMap<String, VillagerId>,
}

impl VillagersFile {
    pub(crate) fn resolve(
        self,
        shops: &HashMap<String, ShopId>,
        village: &Village,
    ) -> Result<Villagers, Problem> {
        let mut villagers = Villagers {
            definitions: Vec::with_capacity(self.villagers.len()),
            by_key: HashMap::new(),
        };
        for (entry, id) in self
            .villagers
            .into_iter()
            .zip((0..=u16::MAX).map(VillagerId))
        {
            if villagers.by_key.insert(entry.id.clone(), id).is_some() {
                return Err(Problem::DuplicateVillager(entry.id));
            }
            villagers.definitions.push(entry.resolve(shops, village)?);
        }
        Ok(villagers)
    }
}

impl VillagerEntry {
    fn resolve(
        self,
        shops: &HashMap<String, ShopId>,
        village: &Village,
    ) -> Result<VillagerDef, Problem> {
        let invalid = |reason: String| Problem::InvalidVillager {
            villager: self.id.clone(),
            reason,
        };
        self.look.validate().map_err(invalid)?;
        if village.labeled(&self.home).is_none() {
            return Err(invalid(format!("no house is labeled `{}`", self.home)));
        }
        let keeps = self
            .keeps
            .as_deref()
            .map(|key| {
                shops
                    .get(key)
                    .copied()
                    .ok_or_else(|| invalid(format!("they keep unknown shop `{key}`")))
            })
            .transpose()?;
        let mut day = Vec::with_capacity(self.day.len());
        for slot in &self.day {
            let time = |text: &str| {
                text.parse::<ClockTime>()
                    .map_err(|error| invalid(format!("`{text}` is not a time: {error}")))
            };
            let (from, to) = (time(&slot.from)?, time(&slot.to)?);
            if to <= from {
                return Err(invalid(format!(
                    "the slot from {} to {} ends before it starts",
                    slot.from, slot.to
                )));
            }
            let spots: Vec<&str> = match &slot.at {
                Activity::Home => Vec::new(),
                Activity::At(label) => vec![label.as_str()],
                Activity::Stroll(labels) => labels.iter().map(String::as_str).collect(),
            };
            if let Activity::Stroll(labels) = &slot.at
                && labels.len() < 2
            {
                return Err(invalid("a stroll needs at least two spots".to_owned()));
            }
            for label in spots {
                if village.spot(label).is_none() {
                    return Err(invalid(format!("no spot is labeled `{label}`")));
                }
            }
            day.push(Slot {
                from,
                to,
                at: slot.at.clone(),
                on: slot.on.clone(),
                seasons: slot.seasons.clone(),
                rain: slot.rain,
            });
        }
        Ok(VillagerDef {
            key: self.id,
            name: self.name,
            look: self.look,
            home: self.home,
            keeps,
            day,
        })
    }
}
