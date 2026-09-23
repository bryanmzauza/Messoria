//! The world clock.

use std::{fmt, str::FromStr, time::Duration};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::date::{Date, Season};

/// Real time that one game minute lasts.
pub const GAME_MINUTE: Duration = Duration::from_secs(1);

/// Hour on the wall clock when every day begins.
const DAWN_HOUR: u16 = 6;
/// Hours from dawn until the day ends at 02:00 for everyone still awake.
const DAY_HOURS: u16 = 20;
/// Hours from dawn until players may go to sleep, at 18:00.
const HOURS_UNTIL_BEDTIME: u16 = 12;

/// Game minutes in a day, from 06:00 until 02:00.
pub const MINUTES_PER_DAY: u16 = DAY_HOURS * 60;

/// A moment in the world: which day, and how far into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct WorldTime {
    /// Days since the world began; day 0 is the first of spring, year one.
    day: u32,
    /// Game minutes since 06:00 on that day, below [`MINUTES_PER_DAY`].
    minute: u16,
}

impl WorldTime {
    /// Dawn of the world's first day.
    pub const FIRST_DAWN: Self = Self { day: 0, minute: 0 };

    /// `minute` game minutes after dawn on `day`, or `None` past the end of the day.
    pub fn new(day: u32, minute: u16) -> Option<Self> {
        (minute < MINUTES_PER_DAY).then_some(Self { day, minute })
    }

    /// The moment `clock` shows on `day`, or `None` between 02:00 and 06:00,
    /// when no day is running.
    pub fn at(day: u32, clock: ClockTime) -> Option<Self> {
        let minutes = u16::from(clock.hour) * 60 + u16::from(clock.minute);
        let since_dawn = (minutes + 24 * 60 - DAWN_HOUR * 60) % (24 * 60);
        Self::new(day, since_dawn)
    }

    /// Days since the world began.
    pub fn day(self) -> u32 {
        self.day
    }

    /// Game minutes since 06:00.
    pub fn minutes_since_dawn(self) -> u16 {
        self.minute
    }

    pub fn date(self) -> Date {
        Date::from_day(self.day)
    }

    pub fn season(self) -> Season {
        self.date().season
    }

    /// The time shown on a wall clock.
    pub fn clock(self) -> ClockTime {
        let minutes = (DAWN_HOUR * 60 + self.minute) % (24 * 60);
        ClockTime {
            hour: (minutes / 60) as u8,
            minute: (minutes % 60) as u8,
        }
    }

    /// Hours since midnight at the start of this day, continuing past 24
    /// after midnight: 6.0 at dawn, 26.0 when the day ends.
    pub fn hours(self) -> f32 {
        f32::from(DAWN_HOUR) + f32::from(self.minute) / 60.0
    }

    /// One game minute later, or `None` if the day is over.
    pub fn next_minute(self) -> Option<Self> {
        Self::new(self.day, self.minute + 1)
    }

    /// Dawn of the following day.
    #[must_use]
    pub fn next_dawn(self) -> Self {
        Self {
            day: self.day + 1,
            minute: 0,
        }
    }

    /// Whether players may go to sleep, which they can from 18:00 on.
    pub fn is_bedtime(self) -> bool {
        self.minute >= HOURS_UNTIL_BEDTIME * 60
    }
}

/// A time of day as shown on a 24-hour clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockTime {
    pub hour: u8,
    pub minute: u8,
}

impl fmt::Display for ClockTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.hour, self.minute)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("expected a 24-hour time such as 18:30")]
pub struct ParseClockTimeError;

/// Parses `HH:MM` on a 24-hour clock.
impl FromStr for ClockTime {
    type Err = ParseClockTimeError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (hour, minute) = text.split_once(':').ok_or(ParseClockTimeError)?;
        let hour: u8 = hour.parse().map_err(|_| ParseClockTimeError)?;
        let minute: u8 = minute.parse().map_err(|_| ParseClockTimeError)?;
        if hour >= 24 || minute >= 60 {
            return Err(ParseClockTimeError);
        }
        Ok(Self { hour, minute })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(minute: u16) -> WorldTime {
        WorldTime::new(0, minute).unwrap()
    }

    #[test]
    fn a_day_runs_from_dawn_to_two_in_the_morning() {
        assert_eq!(at(0).clock().to_string(), "06:00");
        assert_eq!(at(18 * 60).clock().to_string(), "00:00");
        assert_eq!(at(MINUTES_PER_DAY - 1).clock().to_string(), "01:59");
        assert_eq!(at(MINUTES_PER_DAY - 1).next_minute(), None);
        assert_eq!(WorldTime::new(0, MINUTES_PER_DAY), None);
    }

    #[test]
    fn a_day_lasts_twenty_real_minutes() {
        assert_eq!(
            GAME_MINUTE * u32::from(MINUTES_PER_DAY),
            Duration::from_mins(20)
        );
    }

    #[test]
    fn bedtime_starts_at_six_in_the_evening() {
        assert!(!at(12 * 60 - 1).is_bedtime());
        assert!(at(12 * 60).is_bedtime());
        assert_eq!(at(12 * 60).clock().to_string(), "18:00");
    }

    #[test]
    fn hours_continue_past_midnight() {
        assert!((at(0).hours() - 6.0).abs() < 1e-6);
        assert!((at(MINUTES_PER_DAY - 30).hours() - 25.5).abs() < 1e-6);
    }

    #[test]
    fn wall_clock_times_map_into_the_day() {
        let clock = |text: &str| text.parse::<ClockTime>().unwrap();
        assert_eq!(WorldTime::at(0, clock("06:00")), Some(at(0)));
        assert_eq!(WorldTime::at(0, clock("21:15")), Some(at(15 * 60 + 15)));
        assert_eq!(
            WorldTime::at(0, clock("01:59")),
            Some(at(MINUTES_PER_DAY - 1))
        );
        assert_eq!(WorldTime::at(0, clock("04:00")), None);
    }

    #[test]
    fn malformed_clock_times_are_rejected() {
        for text in ["", "18", "24:00", "12:60", "ab:cd", "-1:00"] {
            assert_eq!(
                text.parse::<ClockTime>(),
                Err(ParseClockTimeError),
                "{text:?}"
            );
        }
    }

    #[test]
    fn the_next_dawn_is_the_start_of_the_next_day() {
        let late = at(1000);
        assert_eq!(late.next_dawn(), WorldTime::new(1, 0).unwrap());
    }
}
