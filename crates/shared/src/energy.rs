//! Stamina: spent by working, restored by sleeping.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Energy a fully rested character has.
pub const MAX_ENERGY: u16 = 100;

/// A character's remaining energy. Tools cost energy, and a character without
/// enough left cannot use them.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Energy(u16);

/// How a character's night went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rest {
    /// Went to sleep before the day ended.
    Slept,
    /// Was still awake at 02:00 and collapsed.
    PassedOut,
}

impl Energy {
    pub const FULL: Self = Self(MAX_ENERGY);

    pub fn current(self) -> u16 {
        self.0
    }

    /// Share of the maximum left, from 0 to 1.
    pub fn fraction(self) -> f32 {
        f32::from(self.0) / f32::from(MAX_ENERGY)
    }

    /// Spends `cost` if enough energy is left, returning whether it did.
    pub fn try_spend(&mut self, cost: u16) -> bool {
        match self.0.checked_sub(cost) {
            Some(left) => {
                self.0 = left;
                true
            }
            None => false,
        }
    }

    /// Energy on waking up after `rest`. Passing out only restores half, the
    /// price of staying up too late.
    #[must_use]
    pub fn after(self, rest: Rest) -> Self {
        match rest {
            Rest::Slept => Self::FULL,
            Rest::PassedOut => self.max(Self(MAX_ENERGY / 2)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_spends_energy_until_it_runs_out() {
        let mut energy = Energy(3);
        assert!(energy.try_spend(2));
        assert_eq!(energy.current(), 1);
        assert!(!energy.try_spend(2));
        assert_eq!(energy.current(), 1);
    }

    #[test]
    fn sleeping_restores_everything() {
        assert_eq!(Energy(10).after(Rest::Slept), Energy::FULL);
    }

    #[test]
    fn passing_out_restores_only_half() {
        assert_eq!(Energy(10).after(Rest::PassedOut).current(), MAX_ENERGY / 2);
        assert_eq!(Energy(80).after(Rest::PassedOut).current(), 80);
    }
}
