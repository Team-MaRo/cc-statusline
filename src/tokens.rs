use std::path::PathBuf;

use crate::color;
use crate::context;
use crate::duration::format_ms;
use crate::git;
use crate::input::Input;
use crate::peak::peak_part;
use crate::rate::{rate_part, RateParts};
use crate::settings;
use crate::state;

pub struct Context {
    pub input: Input,
    pub now: i64,
    pub home: Option<String>,
    rl5: Option<RateParts>,
    rl7: Option<RateParts>,
    effort: Option<String>,
    branch: Option<String>,
    diff: Option<(u64, u64)>,
}

impl Context {
    pub fn new(input: Input, now: i64) -> Self {
        let home = std::env::var("HOME").ok();
        let (rl5_raw, rl7_raw) = match &input.rate_limits {
            Some(rl) => (rl.five_hour.as_ref(), rl.seven_day.as_ref()),
            None => (None, None),
        };
        let rl5 = rate_part(
            rl5_raw.and_then(|r| r.used_percentage),
            rl5_raw.and_then(|r| r.resets_at),
            now,
            300.0,
        );
        let rl7 = rate_part(
            rl7_raw.and_then(|r| r.used_percentage),
            rl7_raw.and_then(|r| r.resets_at),
            now,
            10080.0,
        );
        let effort = settings::effort_level();
        let cwd = input.cwd.as_deref().map(PathBuf::from);
        let (branch, diff) = match &cwd {
            Some(p) if p.exists() => (git::branch(p), git::diff_counts(p)),
            _ => (None, None),
        };
        Context {
            input,
            now,
            home,
            rl5,
            rl7,
            effort,
            branch,
            diff,
        }
    }

    fn tilde(&self, p: &str) -> String {
        if let Some(h) = &self.home {
            if let Some(rest) = p.strip_prefix(h) {
                return format!("~{}", rest);
            }
        }
        p.to_string()
    }

    pub fn resolve(&self, name: &str, styles: &[String]) -> Option<String> {
        if styles.is_empty() {
            if let Some(s) = self.smart_render(name) {
                return Some(s);
            }
        }
        let raw = self.raw(name)?;
        if styles.is_empty() {
            if let Some(default) = self.default_style(name) {
                return Some(color::apply_styles(&raw, &[default.to_string()]));
            }
        }
        Some(color::apply_styles(&raw, styles))
    }

    fn session_or_live_tokens(&self) -> Option<u64> {
        let session_total = self
            .input
            .session_id
            .as_deref()
            .and_then(state::read_session_total);
        let live = self
            .input
            .context_window
            .as_ref()
            .and_then(context::current_usage_sum);
        session_total.or(live)
    }

    /// Per-chat cumulative cost. `state::record` runs before render, so the
    /// persisted segment total already includes this turn and never drops on a
    /// mid-chat cost reset. Falls back to the live value when nothing persisted.
    fn session_or_live_cost(&self) -> Option<f64> {
        let persisted = self
            .input
            .session_id
            .as_deref()
            .and_then(state::read_session_cost);
        let live = self.input.cost.as_ref().and_then(|c| c.total_cost_usd);
        persisted.or(live)
    }

    fn ctx_exceeds(&self) -> bool {
        self.input.exceeds_200k_tokens == Some(true)
    }

    fn ctx_used_color(&self) -> &'static str {
        if self.ctx_exceeds() {
            "red"
        } else {
            "green"
        }
    }

    fn ctx_usable_color(&self) -> &'static str {
        let percent = self
            .input
            .context_window
            .as_ref()
            .and_then(context::used_percent_usable)
            .unwrap_or(0.0);
        if percent >= 95.0 {
            "red"
        } else if percent >= 85.0 {
            "bright_yellow"
        } else if percent >= 70.0 {
            "yellow"
        } else {
            "green"
        }
    }

    fn smart_render(&self, name: &str) -> Option<String> {
        if matches!(name, "d" | "diff") {
            let (added, removed) = self.diff?;
            if added == 0 && removed == 0 {
                return None;
            }
            return Some(format!(
                "({},{})",
                color::wrap(&format!("+{}", added), "green"),
                color::wrap(&format!("-{}", removed), "red"),
            ));
        }
        let cw = self.input.context_window.as_ref()?;
        let used_color = self.ctx_used_color();
        let exceeds = self.ctx_exceeds();
        match name {
            "cub" | "ctx_bar" => context::context_bar_smart(cw, exceeds),
            "cubu" | "ctx_bar_usable" => context::context_bar_usable_smart(cw, exceeds),
            "ct" | "ctx_tokens" => {
                let used = context::tokens_used_str(cw)?;
                let max = context::tokens_max_str(cw)?;
                Some(format!("{}/{}", color::wrap(&used, used_color), max))
            }
            "ctpu" | "ctx_tokens_usable" => {
                let used = context::tokens_used_str(cw)?;
                let max = context::tokens_usable_max(cw).map(context::format_tokens)?;
                Some(format!("{}/{}", color::wrap(&used, used_color), max))
            }
            "cu" | "ctx" | "context" => {
                cw.context_window_size?;
                let bar = context::context_bar_smart(cw, exceeds)?;
                let used = context::tokens_used_str(cw)?;
                let max = context::tokens_max_str(cw)?;
                let percent = context::used_percent_or_zero(cw).unwrap_or(0.0);
                let usable_color = self.ctx_usable_color();
                let pct = color::wrap(&format!("{:.0}%", percent), usable_color);
                Some(format!(
                    "{} {}/{} ({})",
                    bar,
                    color::wrap(&used, used_color),
                    max,
                    pct
                ))
            }
            "cuu" | "ctx_usable" => {
                cw.context_window_size?;
                let bar = context::context_bar_usable_smart(cw, exceeds)?;
                let used = context::tokens_used_str(cw)?;
                let max = context::tokens_usable_max(cw).map(context::format_tokens)?;
                let percent = context::used_percent_usable(cw).unwrap_or(0.0);
                let usable_color = self.ctx_usable_color();
                let pct = color::wrap(&format!("{:.0}%", percent), usable_color);
                Some(format!(
                    "{} {}/{} ({})",
                    bar,
                    color::wrap(&used, used_color),
                    max,
                    pct
                ))
            }
            _ => None,
        }
    }

    fn default_style(&self, name: &str) -> Option<&'static str> {
        match name {
            "cup" | "ctx_pct" | "cpu" | "ctx_pct_usable" => Some(self.ctx_usable_color()),
            "ctu" | "ctx_tokens_used" => Some(self.ctx_used_color()),
            "e" | "effort" => match self.effort.as_deref() {
                Some("low") | Some("minimal") | Some("none") => Some("green"),
                Some("medium") => Some("yellow"),
                Some("high") => Some("red"),
                Some(_) => Some("bright_red"),
                None => None,
            },
            _ => None,
        }
    }

    fn raw(&self, name: &str) -> Option<String> {
        let context_window = self.input.context_window.as_ref();
        let cost = self.input.cost.as_ref();
        match name {
            "m" | "model" => self.input.model.as_ref()?.display_name.clone(),
            "mid" | "model_id" => self.input.model.as_ref()?.id.clone(),
            "e" | "effort" => self.effort.clone(),
            "f" | "fast" => match self.input.fast_mode {
                Some(true) => Some("fast".into()),
                _ => None,
            },
            "cu" | "ctx" | "context" => context_window.and_then(context::context_composite),
            "cuu" | "ctx_usable" => context_window
                .and_then(|cw| context::context_composite_usable(cw, self.ctx_exceeds())),
            "cup" | "ctx_pct" => {
                let context_window = context_window?;
                context_window.context_window_size?;
                Some(format!(
                    "{:.0}%",
                    context_window.used_percentage.unwrap_or(0.0)
                ))
            }
            "cub" | "ctx_bar" => context_window.and_then(context::context_bar),
            "ct" | "ctx_tokens" => context_window.and_then(context::tokens_pair),
            "ctu" | "ctx_tokens_used" => context_window.and_then(context::tokens_used_str),
            "ctm" | "ctx_tokens_max" => context_window.and_then(context::tokens_max_str),
            "cpu" | "ctx_pct_usable" => context_window
                .and_then(context::used_percent_usable)
                .map(|p| format!("{:.1}%", p)),
            "cubu" | "ctx_bar_usable" => context_window.and_then(context::context_bar_usable),
            "ctpu" | "ctx_tokens_usable" => context_window.and_then(context::tokens_pair_usable),
            "tt" | "tokens_total" => self
                .session_or_live_tokens()
                .filter(|n| *n > 0)
                .map(context::format_tokens),
            "ts" | "total_speed" => {
                let tokens = self.session_or_live_tokens().unwrap_or(0);
                let ms = cost.and_then(|c| c.total_api_duration_ms).unwrap_or(0);
                let tps = if ms == 0 || tokens == 0 {
                    0.0
                } else {
                    tokens as f64 / (ms as f64 / 1000.0)
                };
                Some(format!("{:.0} tok/s", tps))
            }
            "rl5" | "rate5h" => self.rl5.as_ref().map(|r| r.composite.clone()),
            "rl5p" | "rate5h_pct" => self.rl5.as_ref().map(|r| r.used_percent_str.clone()),
            "rl5b" | "rate5h_bar" => self.rl5.as_ref().map(|r| r.bar.clone()),
            "rl5r" | "rate5h_reset" => self.rl5.as_ref().and_then(|r| {
                if r.reset_only.is_empty() {
                    None
                } else {
                    Some(r.reset_only.clone())
                }
            }),
            "rl5d" | "rate5h_delta" => self.rl5.as_ref().map(|r| r.percent_delta.clone()),
            "rl5td" | "rate5h_time_delta" => self.rl5.as_ref().and_then(|r| {
                if r.time_delta_only.is_empty() {
                    None
                } else {
                    Some(r.time_delta_only.clone())
                }
            }),
            "rl7" | "rate7d" => self.rl7.as_ref().map(|r| r.composite.clone()),
            "rl7p" | "rate7d_pct" => self.rl7.as_ref().map(|r| r.used_percent_str.clone()),
            "rl7b" | "rate7d_bar" => self.rl7.as_ref().map(|r| r.bar.clone()),
            "rl7r" | "rate7d_reset" => self.rl7.as_ref().and_then(|r| {
                if r.reset_only.is_empty() {
                    None
                } else {
                    Some(r.reset_only.clone())
                }
            }),
            "rl7d" | "rate7d_delta" => self.rl7.as_ref().map(|r| r.percent_delta.clone()),
            "rl7td" | "rate7d_time_delta" => self.rl7.as_ref().and_then(|r| {
                if r.time_delta_only.is_empty() {
                    None
                } else {
                    Some(r.time_delta_only.clone())
                }
            }),
            "pk" | "peak" => Some(peak_part(self.now)),
            "b" | "branch" => self.branch.as_ref().map(|b| format!("⎇ {}", b)),
            "bn" | "branch_name" => self.branch.clone(),
            "d" | "diff" => {
                let (a, r) = self.diff?;
                if a == 0 && r == 0 {
                    None
                } else {
                    Some(format!("(+{},-{})", a, r))
                }
            }
            "cwd" => self.input.cwd.as_ref().map(|p| self.tilde(p)),
            "cwdb" | "cwd_base" => self.input.cwd.as_ref().and_then(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
            }),
            "c" | "cost" => self.session_or_live_cost().map(|v| format!("${:.2}", v)),
            "cd" | "cost_day" => Some(format!("${:.2}", state::read_day_cost(self.now))),
            "ca" | "cost_all" => Some(format!("${:.2}", state::read_lifetime_cost())),
            "cm" | "cost_month" => Some(format!("${:.2}", state::read_month_cost(self.now))),
            "la" | "lines_added" => cost?.total_lines_added.map(|v| format!("+{}", v)),
            "lr" | "lines_removed" => cost?.total_lines_removed.map(|v| format!("-{}", v)),
            "dur" | "duration" => cost?.total_duration_ms.map(format_ms),
            "apidur" | "api_duration" => cost?.total_api_duration_ms.map(format_ms),
            "os" | "output_style" => self.input.output_style.as_ref()?.name.clone(),
            "v" | "version" => self.input.version.clone(),
            "sid" | "session_id" => self.input.session_id.as_ref().map(|s| {
                let take = s.len().min(8);
                s[..take].to_string()
            }),
            "vim" => std::env::var("CC_VIM_MODE").ok(),
            _ => {
                eprintln!("cc-statusline: unknown token %{}", name);
                Some(format!("%{}", name))
            }
        }
    }
}
