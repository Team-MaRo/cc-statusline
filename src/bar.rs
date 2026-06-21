use crate::color;

const CELLS: i32 = 10;
const LEVELS: i32 = 3;
const TOTAL: i32 = CELLS * LEVELS;

fn units(percent: f64) -> i32 {
    ((percent * TOTAL as f64 / 100.0).round() as i32).clamp(0, TOTAL)
}

fn glyph(fill: i32) -> &'static str {
    match fill {
        0 => "░",
        1 => "▒",
        2 => "▓",
        _ => "█",
    }
}

pub fn plain_bar(used_percent: f64) -> String {
    let used_units = units(used_percent);
    let style = if used_percent >= 90.0 {
        Some("red")
    } else if used_percent >= 75.0 {
        Some("yellow")
    } else {
        None
    };
    let mut bar = String::with_capacity(80);
    for i in 0..CELLS {
        let fill = (used_units - i * LEVELS).clamp(0, LEVELS);
        if fill > 0 {
            let g = glyph(fill);
            match style {
                Some(s) => bar.push_str(&color::wrap(g, s)),
                None => bar.push_str(g),
            }
        } else {
            bar.push('░');
        }
    }
    bar
}

pub fn threshold_bar(used_percent: f64, threshold_percent: f64) -> String {
    let used_units = units(used_percent);
    let thresh_units = units(threshold_percent);
    let mut bar = String::with_capacity(80);
    for i in 0..CELLS {
        let fill = (used_units - i * LEVELS).clamp(0, LEVELS);
        if fill > 0 {
            let g = glyph(fill);
            let cell_end = i * LEVELS + fill;
            if cell_end > thresh_units {
                bar.push_str(&color::red(g));
            } else {
                bar.push_str(g);
            }
        } else {
            bar.push('░');
        }
    }
    bar
}

pub fn make_bar(used_percent: f64, expected_percent: f64) -> String {
    let used_units = units(used_percent);
    let exp_units = units(expected_percent);
    // Raw-float over/under, matching rate::RateMetrics.over so the bar tip
    // color always agrees with the Δ% / Δt delta colors. Quantizing here
    // (used_units > exp_units) would disagree at the rounding boundary.
    let over = used_percent > expected_percent;

    let mut bar = String::with_capacity(120);
    for i in 0..CELLS {
        let cell_used = (used_units - i * LEVELS).clamp(0, LEVELS);
        let cell_exp = (exp_units - i * LEVELS).clamp(0, LEVELS);
        if cell_used > 0 {
            let g = glyph(cell_used);
            let cell_end = i * LEVELS + cell_used;
            if cell_end > exp_units || (over && cell_used < LEVELS) {
                bar.push_str(&color::red(g));
            } else if !over && cell_used < LEVELS {
                bar.push_str(&color::green(g));
            } else {
                bar.push_str(g);
            }
        } else if cell_exp > 0 {
            bar.push_str(&color::green("░"));
        } else {
            bar.push('░');
        }
    }
    bar
}
