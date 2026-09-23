//! In-game time, dates and seasons, and the rules for ending a day.
//!
//! A day runs from 06:00 until 02:00 the next morning, one game minute per
//! real second, so a full day lasts twenty real minutes. It ends early when
//! enough players go to sleep, and at 02:00 for anyone still awake.
//!
//! This crate has no engine dependency; see
//! `docs/adr/0003-pure-domain-crates.md`.

mod date;
mod sleep;
mod time;

pub use crate::{
    date::{DAYS_PER_YEAR, Date, Season},
    sleep::SleepRule,
    time::{ClockTime, GAME_MINUTE, MINUTES_PER_DAY, ParseClockTimeError, WorldTime},
};
