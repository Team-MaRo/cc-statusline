// Peak hours follow Anthropic pattern: 5–11 AM US/Pacific, Mon–Fri.
// Source: https://isitclaudetime.com/en (5-11 AM PT weekdays).
// Pacific (America/Los_Angeles, DST-aware) resolved via chrono-tz.

use crate::color;
use crate::duration::format_hours;
use chrono::{Datelike, TimeZone, Timelike};
use chrono_tz::America::Los_Angeles;

const SECS_DAY: i64 = 86400;
const PEAK_START_HR: i64 = 5;
const PEAK_END_HR: i64 = 11;

pub fn peak_part(unix: i64) -> String {
    let dt = match Los_Angeles.timestamp_opt(unix, 0).single() {
        Some(dt) => dt,
        // Ambiguous/skipped instants at DST transitions: take the earliest.
        None => match Los_Angeles.timestamp_opt(unix, 0).earliest() {
            Some(dt) => dt,
            None => return color::green("off-peak"),
        },
    };

    let wd = dt.weekday().num_days_from_sunday(); // Sun=0 .. Sat=6
    let secs_today = dt.num_seconds_from_midnight() as i64;
    let is_weekday = (1..=5).contains(&wd);
    let peak_start = PEAK_START_HR * 3600;
    let peak_end = PEAK_END_HR * 3600;
    let is_peak = is_weekday && secs_today >= peak_start && secs_today < peak_end;

    let secs_until = if is_peak {
        peak_end - secs_today
    } else {
        let mut d_off = 0i64;
        let mut cur_wd = wd;
        let mut cur_secs = secs_today;
        loop {
            if (1..=5).contains(&cur_wd) && cur_secs < peak_start {
                break d_off * SECS_DAY + (peak_start - cur_secs);
            }
            d_off += 1;
            cur_secs = 0;
            cur_wd = (cur_wd + 1) % 7;
        }
    };

    let label = if is_peak {
        color::red("peak")
    } else {
        color::green("off-peak")
    };
    format!("{} {}", label, format_hours(secs_until as f64 / 3600.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn unix(y: i32, m: u32, d: u32, h: u32, mi: u32) -> i64 {
        Utc.with_ymd_and_hms(y, m, d, h, mi, 0).unwrap().timestamp()
    }

    #[test]
    fn user_scenario_thursday_afternoon() {
        // 2026-04-23 13:41 UTC → 06:41 PDT, Thursday → peak.
        // Peak ends 11:00 PDT = 18:00 UTC. Remaining ≈ 4h19m.
        let out = peak_part(unix(2026, 4, 23, 13, 41));
        assert!(out.starts_with("\x1b[31mpeak\x1b[0m"), "got: {}", out);
        assert!(out.contains("4h19m"), "got: {}", out);
    }

    #[test]
    fn weekend_is_off_peak_until_monday() {
        // 2026-04-25 Saturday 12:00 UTC → 05:00 PDT Sat.
        let out = peak_part(unix(2026, 4, 25, 12, 0));
        assert!(out.starts_with("\x1b[32moff-peak\x1b[0m"), "got: {}", out);
        // Next Monday 05:00 PDT = Monday 12:00 UTC = 48h away.
        assert!(out.contains("2d"), "got: {}", out);
    }

    #[test]
    fn pdt_offset_in_summer() {
        // 2026-07-15 Wed 12:00 UTC → 05:00 PDT (-7) → peak start.
        let out = peak_part(unix(2026, 7, 15, 12, 0));
        assert!(out.starts_with("\x1b[31mpeak\x1b[0m"), "got: {}", out);
    }

    #[test]
    fn pst_offset_after_dst_ends() {
        // DST ends Sun 2026-11-01; Mon 2026-11-02 is PST (-8).
        // 13:00 UTC → 05:00 PST → peak start.
        let out = peak_part(unix(2026, 11, 2, 13, 0));
        assert!(out.starts_with("\x1b[31mpeak\x1b[0m"), "got: {}", out);
        // One hour earlier (12:00 UTC → 04:00 PST) is before peak.
        let out2 = peak_part(unix(2026, 11, 2, 12, 0));
        assert!(out2.starts_with("\x1b[32moff-peak\x1b[0m"), "got: {}", out2);
    }
}
