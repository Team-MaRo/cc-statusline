pub fn format_hours(hours: f64) -> String {
    let total_s = (hours * 3600.0).round() as i64;
    let total_s = total_s.max(0);
    let d = total_s / 86400;
    let h = (total_s % 86400) / 3600;
    let m = (total_s % 3600) / 60;
    let s = total_s % 60;
    if d > 0 && h > 0 {
        format!("{}d{:02}h", d, h)
    } else if d > 0 {
        format!("{}d", d)
    } else if h > 0 && m > 0 {
        format!("{}h{:02}m", h, m)
    } else if h > 0 {
        format!("{}h", h)
    } else if m > 0 && s > 0 {
        format!("{}m{:02}s", m, s)
    } else if m > 0 {
        format!("{}m", m)
    } else {
        format!("{}s", s)
    }
}

pub fn format_ms(ms: u64) -> String {
    format_hours(ms as f64 / 3_600_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basic() {
        assert_eq!(format_hours(0.0), "0s");
        assert_eq!(format_hours(1.0 / 60.0), "1m");
        assert_eq!(format_hours(1.5), "1h30m");
        assert_eq!(format_hours(24.0), "1d");
        assert_eq!(format_hours(25.0), "1d01h");
        assert_eq!(format_hours(12.0 / 3600.0), "12s");
    }
}
