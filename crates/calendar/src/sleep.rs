//! When sleeping players end the day.

/// How many sleeping players it takes to skip the rest of the day.
///
/// Waiting for everyone suits a small group of friends; on large servers a
/// share of the players online is enough, or one player could keep everyone
/// up all night.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SleepRule {
    /// Every player online must be asleep.
    Everyone,
    /// At least this percentage of the players online, rounded up and never
    /// fewer than one.
    Share { percent: u8 },
}

impl SleepRule {
    /// Players that must be asleep, out of `online`, to end the day.
    pub fn required(self, online: u32) -> u32 {
        match self {
            Self::Everyone => online,
            Self::Share { percent } => (online * u32::from(percent.min(100)))
                .div_ceil(100)
                .max(1)
                .min(online),
        }
    }

    /// Whether `asleep` sleeping players out of `online` end the day. An
    /// empty world never ends its day early.
    pub fn ends_day(self, asleep: u32, online: u32) -> bool {
        online > 0 && asleep >= self.required(online)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everyone_must_sleep_by_default() {
        let rule = SleepRule::Everyone;
        assert!(rule.ends_day(1, 1));
        assert!(!rule.ends_day(2, 3));
        assert!(rule.ends_day(3, 3));
    }

    #[test]
    fn a_share_rounds_up() {
        let half = SleepRule::Share { percent: 50 };
        assert_eq!(half.required(3), 2);
        assert!(!half.ends_day(1, 3));
        assert!(half.ends_day(2, 3));
        assert!(half.ends_day(25, 50));
    }

    #[test]
    fn a_share_needs_at_least_one_sleeper() {
        let rule = SleepRule::Share { percent: 1 };
        assert_eq!(rule.required(10), 1);
        assert!(!rule.ends_day(0, 10));
    }

    #[test]
    fn an_empty_world_does_not_skip_its_day() {
        assert!(!SleepRule::Everyone.ends_day(0, 0));
        assert!(!SleepRule::Share { percent: 50 }.ends_day(0, 0));
    }
}
