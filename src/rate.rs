#![allow(dead_code)]
use crate::bar::make_bar;
use crate::color;
use crate::duration::format_hours;

pub const FIVE_HOUR_WINDOW_MIN: f64 = 300.0;
pub const SEVEN_DAY_WINDOW_MIN: f64 = 10_080.0;

pub struct RateMetrics {
    pub used_percent: f64,
    pub expected_percent: f64,
    pub delta_percent: f64,
    pub actual_remaining_min: f64,
    pub expected_remaining_min: f64,
    pub delta_remaining_min: f64,
    pub over: bool,
}

pub fn rate_metrics(
    used_percent: Option<f64>,
    resets_at: Option<i64>,
    now: i64,
    window_min: f64,
) -> RateMetrics {
    let used = used_percent.unwrap_or(0.0);
    match resets_at {
        None => RateMetrics {
            used_percent: used,
            expected_percent: 0.0,
            delta_percent: used,
            actual_remaining_min: 0.0,
            expected_remaining_min: 0.0,
            delta_remaining_min: 0.0,
            over: used > 0.0,
        },
        Some(r) => {
            let actual_rem = ((r - now) as f64 / 60.0).max(0.0);
            let elapsed = (window_min - actual_rem).clamp(0.0, window_min);
            let expected_percent = elapsed / window_min * 100.0;
            let expected_rem = (1.0 - used / 100.0).max(0.0) * window_min;
            RateMetrics {
                used_percent: used,
                expected_percent,
                delta_percent: used - expected_percent,
                actual_remaining_min: actual_rem,
                expected_remaining_min: expected_rem,
                delta_remaining_min: actual_rem - expected_rem,
                over: used > expected_percent,
            }
        }
    }
}

fn colored_percent_delta(m: &RateMetrics) -> String {
    let text = format!("Δ{:+.2}%", m.delta_percent);
    if m.over {
        color::red(&text)
    } else {
        color::green(&text)
    }
}

fn colored_time_delta(m: &RateMetrics) -> String {
    let delta = m.delta_remaining_min;
    let sign = if delta >= 0.0 { "+" } else { "-" };
    let text = format!("Δ{}{}", sign, format_hours(delta.abs() / 60.0));
    if m.over {
        color::red(&text)
    } else {
        color::green(&text)
    }
}

pub struct RateParts {
    pub bar: String,
    pub used_percent_str: String,
    pub percent_delta: String,
    pub reset_str: String,
    pub reset_only: String,
    pub time_delta_only: String,
    pub composite: String,
}

pub fn rate_part(
    used_percent: Option<f64>,
    resets_at: Option<i64>,
    now: i64,
    window_min: f64,
) -> Option<RateParts> {
    let m = rate_metrics(used_percent, resets_at, now, window_min);
    let bar = make_bar(m.used_percent, m.expected_percent);
    let used_percent_str = format!("{:.0}%", m.used_percent);
    let percent_delta_str = colored_percent_delta(&m);

    let (reset_str, reset_only, time_delta_only) = if resets_at.is_some() {
        let dur = format_hours(m.actual_remaining_min / 60.0);
        let td = colored_time_delta(&m);
        (format!(" {}({})", dur, td), dur, td)
    } else {
        (String::new(), String::new(), String::new())
    };

    let composite = format!(
        "[{}] {}({}){}",
        bar, used_percent_str, percent_delta_str, reset_str
    );
    Some(RateParts {
        bar,
        used_percent_str,
        percent_delta: percent_delta_str,
        reset_str,
        reset_only,
        time_delta_only,
        composite,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_metrics_under_curve() {
        // 20% used, halfway through 300-min window → expected 50%, delta -30, under.
        let m = rate_metrics(Some(20.0), Some(150 * 60), 0, 300.0);
        assert!((m.expected_percent - 50.0).abs() < 0.01);
        assert!((m.delta_percent - (-30.0)).abs() < 0.01);
        assert!(!m.over);
    }

    #[test]
    fn rate_metrics_over_curve() {
        // 80% used, quarter through (75 min remaining of 300) → expected 75%, delta +5, over.
        let m = rate_metrics(Some(80.0), Some(75 * 60), 0, 300.0);
        assert!((m.expected_percent - 75.0).abs() < 0.01);
        assert!((m.delta_percent - 5.0).abs() < 0.01);
        assert!(m.over);
        // Expected to have 20% (60 min) left if on curve; actually has 75 min → ahead in time.
        assert!((m.expected_remaining_min - 60.0).abs() < 0.01);
        assert!((m.delta_remaining_min - 15.0).abs() < 0.01);
    }

    #[test]
    fn rate_metrics_no_reset_returns_zero_expected() {
        let m = rate_metrics(Some(40.0), None, 0, 300.0);
        assert_eq!(m.expected_percent, 0.0);
        assert_eq!(m.actual_remaining_min, 0.0);
        assert_eq!(m.expected_remaining_min, 0.0);
        assert_eq!(m.delta_remaining_min, 0.0);
        assert!((m.delta_percent - 40.0).abs() < 0.01);
        assert!(m.over);
    }

    #[test]
    fn rate_metrics_no_used_treats_as_zero() {
        let m = rate_metrics(None, Some(150 * 60), 0, 300.0);
        assert_eq!(m.used_percent, 0.0);
        assert!((m.expected_percent - 50.0).abs() < 0.01);
        assert!((m.delta_percent - (-50.0)).abs() < 0.01);
        assert!(!m.over);
    }
}
