//! In-game time, dates and seasons, and the rules for ending a day.
//!
//! Weeks have seven days and seasons thirteen weeks, near enough: winter has
//! one extra day. A day runs from 06:00 until 02:00 the next morning, one game minute per
//! real second, so a full day lasts twenty real minutes. It ends early when
//! enough players go to sleep, and at 02:00 for anyone still awake. Each day
//! has its weather, decided from the world's seed.
//!
//! This crate has no engine dependency; see
//! `docs/adr/0003-pure-domain-crates.md`.

mod date;
mod sleep;
mod time;
mod weather;

pub use crate::{
    date::{DAYS_PER_YEAR, Date, Season, Weekday},
    sleep::SleepRule,
    time::{ClockTime, GAME_MINUTE, MINUTES_PER_DAY, ParseClockTimeError, WorldTime},
    weather::Weather,
};
