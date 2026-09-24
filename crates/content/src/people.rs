//! The characters file: the models characters are drawn with, how they hold
//! things and which of their animations play when.

use serde::Deserialize;

use crate::{error::Problem, scenery::check_models};

/// How characters look and move.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Characters {
    /// Models players are drawn with; each player gets one of them, the
    /// same on every screen.
    pub models: Vec<String>,
    /// Scale the models are drawn at.
    pub scale: f32,
    /// Name of the node of a model that held items hang from.
    pub hand: String,
    /// Where a held item sits in the hand node's frame.
    pub grip: Grip,
    pub animations: Animations,
}

/// Where a held item sits in a hand, in the hand's own frame.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grip {
    pub at: (f32, f32, f32),
    /// Turns around x, y and z, in degrees, in that order.
    pub turn: (f32, f32, f32),
    pub scale: f32,
}

/// The names of the animations every character model has, for each thing a
/// character does.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Animations {
    pub idle: String,
    pub walk: String,
    pub run: String,
    pub jump: String,
    pub fall: String,
    /// Swinging a tool.
    pub swing: String,
    /// Using something with the hands, such as a watering can or a stall.
    pub work: String,
    /// Bending down to the ground, to plant or pick.
    pub pick: String,
    /// Welcoming someone, as shopkeepers do.
    pub greet: String,
}

impl Animations {
    /// Every animation name, for checking the models have them.
    pub fn all(&self) -> [&str; 9] {
        [
            &self.idle,
            &self.walk,
            &self.run,
            &self.jump,
            &self.fall,
            &self.swing,
            &self.work,
            &self.pick,
            &self.greet,
        ]
    }
}

impl Characters {
    pub(crate) fn validate(&self) -> Result<(), Problem> {
        let problem = |reason: String| Err(Problem::InvalidCharacters(reason));
        if let Err(reason) = check_models(&self.models) {
            return problem(reason);
        }
        if !(f32::MIN_POSITIVE..).contains(&self.scale)
            || !(f32::MIN_POSITIVE..).contains(&self.grip.scale)
        {
            return problem("scales must be above 0".to_owned());
        }
        if self.hand.is_empty() || self.animations.all().iter().any(|name| name.is_empty()) {
            return problem("the hand and every animation must be named".to_owned());
        }
        Ok(())
    }
}
