//! Dates and times in the user's own time zone.

use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};

/// `t` on the local clock. File times come from outside, so one out of range reads as 1970 rather than panicking.
fn local(t: SystemTime) -> Option<DateTime<Local>> {
    at_seconds(t.duration_since(UNIX_EPOCH).ok()?.as_secs())
}

fn at_seconds(secs: u64) -> Option<DateTime<Local>> {
    Local.timestamp_opt(i64::try_from(secs).ok()?, 0).single()
}

/// Year, month and day.
pub fn local_date(t: SystemTime) -> (i64, u32, u32) {
    local(t).map_or((1970, 1, 1), |l| (i64::from(l.year()), l.month(), l.day()))
}

/// Hour (0-23) and minute.
pub fn local_clock(t: SystemTime) -> (u32, u32) {
    local(t).map_or((0, 0), |l| (l.hour(), l.minute()))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn a_time_past_the_calendar_is_no_time_at_all() {
        assert!(at_seconds(u64::MAX / 4).is_none());
        assert!(at_seconds(u64::MAX).is_none());
    }

    #[test]
    fn a_time_before_1970_reads_as_the_epoch() {
        let Some(early) = UNIX_EPOCH.checked_sub(Duration::from_secs(86_400 * 400)) else { return };
        assert_eq!(local_date(early), (1970, 1, 1));
    }
}
