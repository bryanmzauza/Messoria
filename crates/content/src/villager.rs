//! The villagers file: who lives in the village, how each looks, where they
//! live and keep shop, what they do through the day, what they say and what
//! they make of a gift.
//!
//! A villager's day is a list of slots, each saying where they are between
//! two times, on which days, in which seasons and weather. The first slot
//! that fits the moment holds; when none does, the villager is at home.
//! What they say is picked among the lines whose conditions hold, the most
//! particular ones first, so a close friend on a rainy winter evening hears
//! something nobody else does.

use std::collections::HashMap;

use messoria_calendar::{ClockTime, Season, Weekday, WorldTime};
use serde::{Deserialize, Serialize};

use crate::{
    error::Problem,
    item::{ItemId, Items},
    people::Look,
    shop::ShopId,
    village::Village,
};

/// Friendship points in one heart.
pub const HEART: u16 = 100;
/// Hearts a friendship can reach.
pub const MOST_HEARTS: u16 = 10;

/// Refers to a villager within a [`Catalog`](crate::Catalog).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VillagerId(pub(crate) u16);

impl VillagerId {
    /// The villager's place in the content.
    pub fn index(self) -> u16 {
        self.0
    }
}

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
    pub lines: Vec<Line>,
    pub gift_lines: GiftLines,
    pub loves: Vec<ItemId>,
    pub likes: Vec<ItemId>,
    pub dislikes: Vec<ItemId>,
    /// What the villager gives a player who first reaches these hearts.
    pub gifts: Vec<Gift>,
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

/// Something a villager says, when its conditions hold.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub says: String,
    /// Only to players with at least this many hearts.
    pub hearts: u16,
    pub from: Option<ClockTime>,
    pub to: Option<ClockTime>,
    pub seasons: Option<Vec<Season>>,
    pub rain: Option<bool>,
}

impl Line {
    /// Whether the line can be said at `time`, when it `rains` or not, to
    /// a player with `hearts`.
    fn fits(&self, time: WorldTime, rains: bool, hearts: u16) -> bool {
        let clock = time.clock();
        hearts >= self.hearts
            && self.from.is_none_or(|from| from <= clock)
            && self.to.is_none_or(|to| clock < to)
            && self
                .seasons
                .as_ref()
                .is_none_or(|seasons| seasons.contains(&time.season()))
            && self.rain.is_none_or(|rain| rain == rains)
    }

    /// How particular the line is: lines that fit and are more particular
    /// are said over plainer ones.
    fn particularity(&self) -> u16 {
        u16::from(self.from.is_some() || self.to.is_some())
            + u16::from(self.seasons.is_some())
            + u16::from(self.rain.is_some())
            + 2 * self.hearts
    }
}

/// What a villager says when given something.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GiftLines {
    pub loved: String,
    pub liked: String,
    pub disliked: String,
}

/// What a villager makes of a gift.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reaction {
    Loved,
    Liked,
    Neutral,
    Disliked,
}

/// Something a villager gives a player whose friendship reaches `at`
/// hearts.
#[derive(Clone, Debug, PartialEq)]
pub struct Gift {
    pub at: u16,
    pub item: ItemId,
    pub count: u16,
}

impl VillagerDef {
    /// What the villager does at `time`, when it `rains` or not.
    pub fn activity(&self, time: WorldTime, rains: bool) -> &Activity {
        self.day
            .iter()
            .find(|slot| slot.holds(time, rains))
            .map_or(&Activity::Home, |slot| &slot.at)
    }

    /// What the villager says at `time`, when it `rains` or not, to a
    /// player with `hearts`: the `pick`th of the most particular lines that
    /// fit, so the same moment can bring different lines.
    pub fn line(&self, time: WorldTime, rains: bool, hearts: u16, pick: usize) -> &str {
        let fitting: Vec<&Line> = self
            .lines
            .iter()
            .filter(|line| line.fits(time, rains, hearts))
            .collect();
        let best = fitting
            .iter()
            .map(|line| line.particularity())
            .max()
            .unwrap_or(0);
        let top: Vec<&Line> = fitting
            .into_iter()
            .filter(|line| line.particularity() == best)
            .collect();
        top.get(pick % top.len().max(1))
            .map_or("...", |line| line.says.as_str())
    }

    /// What the villager makes of being given `item`.
    pub fn reaction(&self, item: ItemId) -> Reaction {
        if self.loves.contains(&item) {
            Reaction::Loved
        } else if self.likes.contains(&item) {
            Reaction::Liked
        } else if self.dislikes.contains(&item) {
            Reaction::Disliked
        } else {
            Reaction::Neutral
        }
    }

    /// What the villager says about a gift met with `reaction`.
    pub fn gift_line(&self, reaction: Reaction) -> &str {
        match reaction {
            Reaction::Loved => &self.gift_lines.loved,
            Reaction::Liked | Reaction::Neutral => &self.gift_lines.liked,
            Reaction::Disliked => &self.gift_lines.disliked,
        }
    }
}

/// How many hearts `points` of friendship are.
pub fn hearts(points: u16) -> u16 {
    (points / HEART).min(MOST_HEARTS)
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
    lines: Vec<LineEntry>,
    gift_lines: GiftLines,
    #[serde(default)]
    loves: Vec<String>,
    #[serde(default)]
    likes: Vec<String>,
    #[serde(default)]
    dislikes: Vec<String>,
    #[serde(default)]
    gifts: Vec<GiftEntry>,
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LineEntry {
    says: String,
    #[serde(default)]
    hearts: u16,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    seasons: Option<Vec<Season>>,
    #[serde(default)]
    rain: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GiftEntry {
    at: u16,
    item: String,
    count: u16,
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
        items: &Items,
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
            villagers
                .definitions
                .push(entry.resolve(shops, items, village)?);
        }
        Ok(villagers)
    }
}

impl VillagerEntry {
    fn resolve(
        self,
        shops: &HashMap<String, ShopId>,
        items: &Items,
        village: &Village,
    ) -> Result<VillagerDef, Problem> {
        let invalid = |reason: String| Problem::InvalidVillager {
            villager: self.id.clone(),
            reason,
        };
        let context = || format!("villager `{}`", self.id);
        let time = |text: &str| {
            text.parse::<ClockTime>()
                .map_err(|error| invalid(format!("`{text}` is not a time: {error}")))
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

        let day = self.resolve_day(&invalid, &time, village)?;
        let spoken = self.resolve_lines(&invalid, &time)?;
        let resolve_items = |keys: &[String]| {
            keys.iter()
                .map(|key| items.resolve(context, key))
                .collect::<Result<Vec<ItemId>, Problem>>()
        };
        let (loves, likes, dislikes) = (
            resolve_items(&self.loves)?,
            resolve_items(&self.likes)?,
            resolve_items(&self.dislikes)?,
        );
        let overlap = loves
            .iter()
            .any(|item| likes.contains(item) || dislikes.contains(item))
            || likes.iter().any(|item| dislikes.contains(item));
        if overlap {
            return Err(invalid(
                "an item cannot be loved, liked and disliked at once".to_owned(),
            ));
        }
        let mut presents = Vec::with_capacity(self.gifts.len());
        for gift in &self.gifts {
            if gift.count == 0 || gift.at == 0 || gift.at > MOST_HEARTS {
                return Err(invalid(format!(
                    "a gift of {} `{}` at {} hearts makes no sense",
                    gift.count, gift.item, gift.at
                )));
            }
            presents.push(Gift {
                at: gift.at,
                item: items.resolve(context, &gift.item)?,
                count: gift.count,
            });
        }
        Ok(VillagerDef {
            key: self.id,
            name: self.name,
            look: self.look,
            home: self.home,
            keeps,
            day,
            lines: spoken,
            gift_lines: self.gift_lines,
            loves,
            likes,
            dislikes,
            gifts: presents,
        })
    }

    /// The villager's day: slots in order, each from a time to a later
    /// one, at spots the village has.
    fn resolve_day(
        &self,
        invalid: &impl Fn(String) -> Problem,
        time: &impl Fn(&str) -> Result<ClockTime, Problem>,
        village: &Village,
    ) -> Result<Vec<Slot>, Problem> {
        let mut day = Vec::with_capacity(self.day.len());
        for slot in &self.day {
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
        Ok(day)
    }

    /// The villager's lines, each saying something, within the hearts a
    /// friendship can have, and at least one for any moment.
    fn resolve_lines(
        &self,
        invalid: &impl Fn(String) -> Problem,
        time: &impl Fn(&str) -> Result<ClockTime, Problem>,
    ) -> Result<Vec<Line>, Problem> {
        let mut lines = Vec::with_capacity(self.lines.len());
        for line in &self.lines {
            if line.says.trim().is_empty() {
                return Err(invalid("a line says nothing".to_owned()));
            }
            if line.hearts > MOST_HEARTS {
                return Err(invalid(format!(
                    "a line asks for {} hearts, more than a friendship can have",
                    line.hearts
                )));
            }
            lines.push(Line {
                says: line.says.clone(),
                hearts: line.hearts,
                from: line.from.as_deref().map(time).transpose()?,
                to: line.to.as_deref().map(time).transpose()?,
                seasons: line.seasons.clone(),
                rain: line.rain,
            });
        }
        let plain = |line: &Line| {
            line.hearts == 0
                && line.from.is_none()
                && line.to.is_none()
                && line.seasons.is_none()
                && line.rain.is_none()
        };
        if !lines.iter().any(plain) {
            return Err(invalid(
                "they need at least one line for any moment".to_owned(),
            ));
        }
        Ok(lines)
    }
}
