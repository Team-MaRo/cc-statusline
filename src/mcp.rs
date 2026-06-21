use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
    ErrorData as McpError, ServerHandler, ServiceExt,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::context::usable_ratio;
use crate::rate::{rate_metrics, FIVE_HOUR_WINDOW_MIN, SEVEN_DAY_WINDOW_MIN};
use crate::state::{self, AllSessions, RateLimitsSnapshot, SessionEntry};

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct SessionIdArgs {
    /// Optional. If omitted, the most-recently-updated session is used.
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Clone)]
pub struct CcStatuslineServer {
    // Used by the #[tool_router]/#[tool_handler] macros + derived Clone; dead-code
    // analysis ignores those, so it'd otherwise warn "never read".
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl CcStatuslineServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Current context window usage for a Claude Code session. Reads cc-statusline's persisted state. Returns the latest segment peak, total across all compaction segments, and percent of window + usable budget.",
        annotations(
            title = "Context Usage",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false,
        )
    )]
    async fn context_usage(
        &self,
        Parameters(args): Parameters<SessionIdArgs>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text_result(payload_context_usage(
            args.session_id.as_deref(),
        )))
    }

    #[tool(
        description = "Latest known Claude Code rate-limit snapshot (5-hour and 7-day windows). Shared across all sessions; whichever session last fired the statusline wrote these numbers.",
        annotations(
            title = "Rate Limits",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false,
        )
    )]
    async fn rate_limits(&self) -> Result<CallToolResult, McpError> {
        Ok(text_result(payload_rate_limits()))
    }

    #[tool(
        description = "One-shot bundle of context_usage + rate_limits for a session.",
        annotations(
            title = "Session Summary",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false,
        )
    )]
    async fn session_summary(
        &self,
        Parameters(args): Parameters<SessionIdArgs>,
    ) -> Result<CallToolResult, McpError> {
        Ok(text_result(payload_session_summary(
            args.session_id.as_deref(),
        )))
    }
}

#[tool_handler]
impl ServerHandler for CcStatuslineServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("cc-statusline", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Read-only access to cc-statusline's persisted session state (context usage + rate-limit snapshot).".to_string(),
            )
    }
}

pub fn run() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(async {
        let server = CcStatuslineServer::new();
        match server.serve(stdio()).await {
            Ok(service) => {
                let _ = service.waiting().await;
            }
            Err(e) => {
                eprintln!("cc-statusline mcp: serve error: {}", e);
            }
        }
    });
}

fn text_result(payload: Value) -> CallToolResult {
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
    CallToolResult::success(vec![Content::text(text)])
}

fn payload_context_usage(requested: Option<&str>) -> Value {
    let all = match state::read_all() {
        Some(a) => a,
        None => return json!({ "error": "no session state on disk yet" }),
    };
    match pick_session(&all, requested) {
        Some((sid, entry)) => context_usage_payload(sid, entry),
        None => json!({ "error": "no matching session" }),
    }
}

fn payload_rate_limits() -> Value {
    let all = match state::read_all() {
        Some(a) => a,
        None => return json!({ "error": "no rate-limit snapshot on disk yet" }),
    };
    match &all.rate_limits {
        Some(rl) => rate_limits_payload(rl),
        None => json!({ "error": "no rate-limit snapshot on disk yet" }),
    }
}

fn payload_session_summary(requested: Option<&str>) -> Value {
    let all = match state::read_all() {
        Some(a) => a,
        None => return json!({ "error": "no session state on disk yet" }),
    };
    let context = match pick_session(&all, requested) {
        Some((sid, entry)) => context_usage_payload(sid, entry),
        None => Value::Null,
    };
    let rate = match &all.rate_limits {
        Some(rl) => rate_limits_payload(rl),
        None => Value::Null,
    };
    json!({ "context_usage": context, "rate_limits": rate })
}

fn pick_session<'a>(
    all: &'a AllSessions,
    requested: Option<&str>,
) -> Option<(&'a str, &'a SessionEntry)> {
    if let Some(sid) = requested {
        all.sessions
            .get_key_value(sid)
            .map(|(k, v)| (k.as_str(), v))
    } else {
        all.sessions
            .iter()
            .max_by_key(|(_, e)| e.updated_at)
            .map(|(k, v)| (k.as_str(), v))
    }
}

fn context_usage_payload(sid: &str, entry: &SessionEntry) -> Value {
    let current_peak = entry.segments.last().copied().unwrap_or(0);
    let total: u64 = entry.segments.iter().sum();
    let size = entry.context_window_size;

    let used_percent = match size {
        Some(s) if s > 0 => Some((current_peak as f64) / (s as f64) * 100.0),
        _ => None,
    };
    let (usable_percent, remaining_usable_tokens) = match size {
        Some(s) if s > 0 => {
            let ratio = usable_ratio(s);
            let usable_max = ((s as f64) * ratio).floor() as u64;
            let usable_pct = (current_peak as f64) / (usable_max as f64) * 100.0;
            let remaining = usable_max.saturating_sub(current_peak);
            (Some(usable_pct), Some(remaining))
        }
        _ => (None, None),
    };

    json!({
        "session_id": sid,
        "segments": entry.segments,
        "current_segment_peak": current_peak,
        "total_across_segments": total,
        "context_window_size": size,
        "used_percent": used_percent,
        "usable_percent": usable_percent,
        "remaining_usable_tokens": remaining_usable_tokens,
        "updated_at": entry.updated_at,
    })
}

fn rate_window_payload(
    used_pct: Option<f64>,
    resets_at: Option<i64>,
    now: i64,
    window_min: f64,
) -> Value {
    if resets_at.is_none() {
        return json!({
            "used_percentage": used_pct,
            "resets_at": Value::Null,
        });
    }
    let m = rate_metrics(used_pct, resets_at, now, window_min);
    json!({
        "used_percentage": m.used_percent,
        "expected_percentage": m.expected_percent,
        "delta_percentage": m.delta_percent,
        "over_curve": m.over,
        "resets_at": resets_at,
        "remaining_seconds": (m.actual_remaining_min * 60.0).round() as i64,
        "expected_remaining_seconds": (m.expected_remaining_min * 60.0).round() as i64,
        "delta_remaining_seconds": (m.delta_remaining_min * 60.0).round() as i64,
    })
}

fn rate_limits_payload(rl: &RateLimitsSnapshot) -> Value {
    let now = state::unix_now() as i64;
    json!({
        "five_hour": rate_window_payload(
            rl.five_hour_used_percentage,
            rl.five_hour_resets_at,
            now,
            FIVE_HOUR_WINDOW_MIN,
        ),
        "seven_day": rate_window_payload(
            rl.seven_day_used_percentage,
            rl.seven_day_resets_at,
            now,
            SEVEN_DAY_WINDOW_MIN,
        ),
        "snapshot_updated_at": rl.updated_at,
        "snapshot_age_seconds": (now as u64).saturating_sub(rl.updated_at),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_usage_payload_computes_percentages() {
        let entry = SessionEntry {
            segments: vec![200_000, 30_000],
            updated_at: 100,
            context_window_size: Some(1_000_000),
            ..Default::default()
        };
        let payload = context_usage_payload("sid", &entry);
        assert_eq!(payload["current_segment_peak"], 30_000);
        assert_eq!(payload["total_across_segments"], 230_000);
        let used_pct = payload["used_percent"].as_f64().unwrap();
        assert!((used_pct - 3.0).abs() < 0.01);
        let usable_pct = payload["usable_percent"].as_f64().unwrap();
        assert!((usable_pct - 3.061).abs() < 0.01);
        assert_eq!(payload["remaining_usable_tokens"], 950_000);
    }

    #[test]
    fn pick_session_defaults_to_most_recent() {
        let mut all = AllSessions::default();
        all.sessions.insert(
            "old".into(),
            SessionEntry {
                segments: vec![1],
                updated_at: 100,
                context_window_size: None,
                ..Default::default()
            },
        );
        all.sessions.insert(
            "new".into(),
            SessionEntry {
                segments: vec![1],
                updated_at: 200,
                context_window_size: None,
                ..Default::default()
            },
        );
        let (sid, _) = pick_session(&all, None).unwrap();
        assert_eq!(sid, "new");
    }
}
