//! Dates and seasons.

use std::fmt;

/// Days in a year.
pub const DAYS_PER_YEAR: u32 = 365;

/// Length of each season in days, in calendar order. They add up to a year.
const SEASON_DAYS: [u16; 4] = [91, 91, 91, 92];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Season {
    const ALL: [Self; 4] = [Self::Spring, Self::Summer, Self::Autumn, Self::Winter];
}

impl fmt::Display for Season {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Spring => "Spring",
            Self::Summer => "Summer",
            Self::Autumn => "Autumn",
            Self::Winter => "Winter",
        })
    }
}

/// A calendar date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Date {
    /// Starting at 1.
    pub year: u32,
    pub season: Season,
    /// Day within the season, starting at 1.
    pub day: u16,
}

impl Date {
    /// The date `day` days after the world began.
    pub(crate) fn from_day(day: u32) -> Self {
        let year = day / DAYS_PER_YEAR + 1;
        let mut remaining =
            u16::try_from(day % DAYS_PER_YEAR).expect("a day of the year is below 365");
        for (season, length) in Season::ALL.into_iter().zip(SEASON_DAYS) {
            if remaining < length {
                return Self {
                    year,
                    season,
                    day: remaining + 1,
                };
            }
            remaining -= length;
        }
        unreachable!("season lengths add up to a year")
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}, year {}", self.season, self.day, self.year)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seasons_fill_the_year() {
        let total: u32 = SEASON_DAYS.iter().map(|&days| u32::from(days)).sum();
        assert_eq!(total, DAYS_PER_YEAR);
    }

    #[test]
    fn days_map_to_seasons() {
        let date = |day| Date::from_day(day).to_string();
        assert_eq!(date(0), "Spring 1, year 1");
        assert_eq!(date(90), "Spring 91, year 1");
        assert_eq!(date(91), "Summer 1, year 1");
        assert_eq!(date(273), "Winter 1, year 1");
        assert_eq!(date(364), "Winter 92, year 1");
        assert_eq!(date(365), "Spring 1, year 2");
    }
}
