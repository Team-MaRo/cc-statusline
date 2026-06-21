use crate::bar::{plain_bar, threshold_bar};
use crate::input::ContextWindow;

const EXCEEDS_THRESHOLD_TOKENS: f64 = 200_000.0;

fn threshold_percent(context_window: &ContextWindow) -> Option<f64> {
    let size = context_window.context_window_size? as f64;
    if size <= 0.0 {
        return None;
    }
    Some(EXCEEDS_THRESHOLD_TOKENS / size * 100.0)
}

// The 200k threshold red follows Claude Code's `exceeds_200k_tokens` flag, not a
// locally computed boundary — if the backend stops sending/setting it, the bar
// stops reddening (consistent with the flag-driven used-token number). Without
// the flag, threshold = 100% so no cell is ever over-curve → neutral fill.
pub fn context_bar_smart(context_window: &ContextWindow, exceeds: bool) -> Option<String> {
    let percent = used_percent_or_zero(context_window)?;
    let threshold = if exceeds {
        threshold_percent(context_window).unwrap_or(100.0)
    } else {
        100.0
    };
    Some(format!("[{}]", threshold_bar(percent, threshold)))
}

pub fn context_bar_usable_smart(context_window: &ContextWindow, exceeds: bool) -> Option<String> {
    let percent = used_percent_usable(context_window)?;
    let threshold = if exceeds {
        let size = context_window.context_window_size?;
        let ratio = usable_ratio(size);
        threshold_percent(context_window)
            .map(|t| (t / ratio).min(100.0))
            .unwrap_or(100.0)
    } else {
        100.0
    };
    Some(format!("[{}]", threshold_bar(percent, threshold)))
}

pub fn current_usage_sum(context_window: &ContextWindow) -> Option<u64> {
    let cu = context_window.current_usage.as_ref()?;
    Some(
        cu.input_tokens.unwrap_or(0)
            + cu.output_tokens.unwrap_or(0)
            + cu.cache_creation_input_tokens.unwrap_or(0)
            + cu.cache_read_input_tokens.unwrap_or(0),
    )
}

pub fn tokens_used(context_window: &ContextWindow) -> Option<u64> {
    let size = context_window.context_window_size?;
    if let Some(used) = current_usage_sum(context_window) {
        return Some(used);
    }
    let percent = context_window.used_percentage.unwrap_or(0.0);
    Some(((size as f64) * percent / 100.0).round() as u64)
}

pub fn used_percent_or_zero(context_window: &ContextWindow) -> Option<f64> {
    let size = context_window.context_window_size?;
    if size == 0 {
        return Some(0.0);
    }
    if let Some(used) = current_usage_sum(context_window) {
        return Some((used as f64) / (size as f64) * 100.0);
    }
    Some(context_window.used_percentage.unwrap_or(0.0))
}

pub fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        let m = n as f64 / 1_000_000.0;
        if m >= 10.0 {
            format!("{:.0}M", m)
        } else {
            format!("{:.1}M", m)
        }
    } else if n >= 1_000 {
        format!("{}k", (n as f64 / 1_000.0).round() as u64)
    } else {
        n.to_string()
    }
}

pub fn tokens_pair(context_window: &ContextWindow) -> Option<String> {
    let used = tokens_used(context_window)?;
    let size = context_window.context_window_size?;
    Some(format!("{}/{}", format_tokens(used), format_tokens(size)))
}

pub fn tokens_used_str(context_window: &ContextWindow) -> Option<String> {
    tokens_used(context_window).map(format_tokens)
}

pub fn tokens_max_str(context_window: &ContextWindow) -> Option<String> {
    context_window.context_window_size.map(format_tokens)
}

pub fn context_bar(context_window: &ContextWindow) -> Option<String> {
    let percent = used_percent_or_zero(context_window)?;
    Some(format!("[{}]", plain_bar(percent)))
}

// TODO: 0.98 is confirmed empirically for 1M Opus (compact ~980k). 0.80 is a legacy
// assumption for 200k models — needs confirmation. If CC reserves a constant ~20k regardless
// of size, switch to `(size - 20_000) / size` and drop the threshold.
pub fn usable_ratio(size: u64) -> f64 {
    if size >= 500_000 {
        0.98
    } else {
        0.80
    }
}

pub fn used_percent_usable(context_window: &ContextWindow) -> Option<f64> {
    let percent = used_percent_or_zero(context_window)?;
    let size = context_window.context_window_size?;
    Some(percent / usable_ratio(size))
}

pub fn context_bar_usable(context_window: &ContextWindow) -> Option<String> {
    let percent = used_percent_usable(context_window)?;
    Some(format!("[{}]", plain_bar(percent)))
}

pub fn tokens_usable_max(context_window: &ContextWindow) -> Option<u64> {
    let size = context_window.context_window_size?;
    Some(((size as f64) * usable_ratio(size)).floor() as u64)
}

pub fn tokens_pair_usable(context_window: &ContextWindow) -> Option<String> {
    let used = tokens_used(context_window)?;
    let max = tokens_usable_max(context_window)?;
    Some(format!("{}/{}", format_tokens(used), format_tokens(max)))
}

pub fn context_composite(context_window: &ContextWindow) -> Option<String> {
    let percent = used_percent_or_zero(context_window)?;
    let bar = context_bar(context_window)?;
    let pair = tokens_pair(context_window).unwrap_or_default();
    Some(format!("{} {} ({:.0}%)", bar, pair, percent))
}

pub fn context_composite_usable(context_window: &ContextWindow, exceeds: bool) -> Option<String> {
    let percent = used_percent_usable(context_window)?;
    let bar = context_bar_usable_smart(context_window, exceeds)?;
    let pair = tokens_pair_usable(context_window).unwrap_or_default();
    Some(format!("{} {} ({:.0}%)", bar, pair, percent))
}
