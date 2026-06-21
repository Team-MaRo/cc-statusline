use std::sync::atomic::{AtomicBool, Ordering};

static DISABLED: AtomicBool = AtomicBool::new(false);

pub fn set_disabled(v: bool) {
    DISABLED.store(v, Ordering::Relaxed);
}

pub fn disabled() -> bool {
    DISABLED.load(Ordering::Relaxed)
}

pub const RESET: &str = "\x1b[0m";

pub fn code_for(name: &str) -> Option<&'static str> {
    Some(match name {
        "black" => "\x1b[30m",
        "red" => "\x1b[31m",
        "green" => "\x1b[32m",
        "yellow" => "\x1b[33m",
        "blue" => "\x1b[34m",
        "magenta" => "\x1b[35m",
        "cyan" => "\x1b[36m",
        "white" => "\x1b[37m",
        "gray" | "grey" | "bright_black" => "\x1b[90m",
        "bright_red" => "\x1b[91m",
        "bright_green" => "\x1b[92m",
        "bright_yellow" => "\x1b[93m",
        "bright_blue" => "\x1b[94m",
        "bright_magenta" => "\x1b[95m",
        "bright_cyan" => "\x1b[96m",
        "bright_white" => "\x1b[97m",
        "bold" => "\x1b[1m",
        "dim" => "\x1b[2m",
        "italic" => "\x1b[3m",
        "underline" => "\x1b[4m",
        _ => return None,
    })
}

pub fn wrap(s: &str, style: &str) -> String {
    if disabled() {
        return s.to_string();
    }
    match code_for(style) {
        Some(code) => format!("{}{}{}", code, s, RESET),
        None => s.to_string(),
    }
}

pub fn apply_styles(s: &str, styles: &[String]) -> String {
    if disabled() || styles.is_empty() {
        return s.to_string();
    }
    let mut prefix = String::new();
    for st in styles {
        if let Some(c) = code_for(st) {
            prefix.push_str(c);
        }
    }
    if prefix.is_empty() {
        return s.to_string();
    }
    // Re-apply prefix after any inner full-reset so inner colored spans
    // (e.g. rate/peak bars) don't clobber outer style.
    let reapply = format!("{}{}", RESET, prefix);
    let replaced = s.replace(RESET, &reapply);
    format!("{}{}{}", prefix, replaced, RESET)
}

pub fn red(s: &str) -> String {
    wrap(s, "red")
}
pub fn green(s: &str) -> String {
    wrap(s, "green")
}
