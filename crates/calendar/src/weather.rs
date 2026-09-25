//! Weather, decided for each day from the world's seed, so a world's weather
//! is the same however often it is replayed.

use serde::{Deserialize, Serialize};

use crate::date::{Date, Season};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Weather {
    #[default]
    Clear,
    /// Waters every field.
    Rain,
    Snow,
}

impl Weather {
    /// The weather on `day` of the world grown from `seed`. The first day is
    /// always clear, so new players start dry.
    pub fn on(seed: u64, day: u32) -> Self {
        if day == 0 {
            return Self::Clear;
        }
        let season = Date::from_day(day).season;
        let percent_chance = match season {
            Season::Spring => 25,
            Season::Summer => 15,
            Season::Autumn | Season::Winter => 30,
        };
        if scramble(seed ^ u64::from(day)) % 100 >= percent_chance {
            Self::Clear
        } else if season == Season::Winter {
            Self::Snow
        } else {
            Self::Rain
        }
    }

    /// Whether this weather waters every field for the day.
    pub fn waters_fields(self) -> bool {
        self == Self::Rain
    }
}

/// Spreads the bits of `value` so that neighboring days get unrelated
/// weather (the `SplitMix64` finalizer).
fn scramble(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_is_the_same_every_time() {
        for day in 0..400 {
            assert_eq!(Weather::on(7, day), Weather::on(7, day));
        }
    }

    #[test]
    fn the_first_day_is_clear() {
        for seed in 0..50 {
            assert_eq!(Weather::on(seed, 0), Weather::Clear);
        }
    }

    #[test]
    fn it_rains_now_and_then_in_spring_and_snows_in_winter() {
        let spring: Vec<_> = (1..91).map(|day| Weather::on(3, day)).collect();
        let rainy = spring
            .iter()
            .filter(|&&weather| weather == Weather::Rain)
            .count();
        assert!((10..40).contains(&rainy), "{rainy} rainy days in spring");
        assert!(!spring.contains(&Weather::Snow));

        let winter: Vec<_> = (273..365).map(|day| Weather::on(3, day)).collect();
        assert!(winter.contains(&Weather::Snow));
        assert!(!winter.contains(&Weather::Rain));
    }
}
