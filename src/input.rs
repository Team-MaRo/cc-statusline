#![allow(dead_code)]
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct Input {
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<Model>,
    pub workspace: Option<Workspace>,
    pub version: Option<String>,
    pub output_style: Option<OutputStyle>,
    pub cost: Option<Cost>,
    pub context_window: Option<ContextWindow>,
    pub exceeds_200k_tokens: Option<bool>,
    pub fast_mode: Option<bool>,
    pub rate_limits: Option<RateLimits>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Model {
    pub id: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Workspace {
    pub current_dir: Option<String>,
    pub project_dir: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct OutputStyle {
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Cost {
    pub total_cost_usd: Option<f64>,
    pub total_duration_ms: Option<u64>,
    pub total_api_duration_ms: Option<u64>,
    pub total_lines_added: Option<u64>,
    pub total_lines_removed: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ContextWindow {
    pub context_window_size: Option<u64>,
    pub current_usage: Option<CurrentUsage>,
    pub used_percentage: Option<f64>,
    pub remaining_percentage: Option<f64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct CurrentUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct RateLimits {
    pub five_hour: Option<RateLimit>,
    pub seven_day: Option<RateLimit>,
}

#[derive(Debug, Deserialize, Default)]
pub struct RateLimit {
    pub used_percentage: Option<f64>,
    pub resets_at: Option<i64>,
}
