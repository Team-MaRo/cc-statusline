use std::path::PathBuf;

fn settings_path() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        let p = PathBuf::from(dir).join("settings.json");
        if p.exists() {
            return Some(p);
        }
    }
    let home = std::env::var("HOME").ok()?;
    let p = PathBuf::from(home).join(".claude").join("settings.json");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn read_settings() -> Option<serde_json::Value> {
    let p = settings_path()?;
    let raw = std::fs::read_to_string(p).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn effort_level() -> Option<String> {
    read_settings()?
        .get("effortLevel")?
        .as_str()
        .map(|s| s.to_string())
}
