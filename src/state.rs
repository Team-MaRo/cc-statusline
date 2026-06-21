use crate::input::Input;
use chrono::{Datelike, Local, TimeZone};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct AllSessions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) rate_limits: Option<RateLimitsSnapshot>,
    #[serde(default)]
    pub(crate) sessions: BTreeMap<String, SessionEntry>,
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub(crate) struct RateLimitsSnapshot {
    pub(crate) five_hour_used_percentage: Option<f64>,
    pub(crate) five_hour_resets_at: Option<i64>,
    pub(crate) seven_day_used_percentage: Option<f64>,
    pub(crate) seven_day_resets_at: Option<i64>,
    pub(crate) updated_at: u64,
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub(crate) struct SessionEntry {
    #[serde(default)]
    pub(crate) segments: Vec<u64>,
    #[serde(default)]
    pub(crate) updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) context_window_size: Option<u64>,
    /// Per-chat cost peaks. Claude Code can reset `total_cost_usd` to ~0
    /// mid-chat; each reset starts a new segment, so the chat total
    /// (sum of peaks) never goes backwards. Mirrors `segments` for tokens.
    #[serde(default)]
    pub(crate) cost_segments: Vec<f64>,
    /// This chat's cost broken down by local day ("YYYY-MM-DD"). Stored per
    /// session (beside `cost_segments`) — the day/month/all-time totals are
    /// inferred by aggregating this map across every session at read time.
    /// No global sum is kept.
    #[serde(default)]
    pub(crate) daily_cost: BTreeMap<String, f64>,
}

impl SessionEntry {
    pub(crate) fn cost_total(&self) -> f64 {
        // fold from +0.0 (not `.sum()`, which yields -0.0 on empty segments and
        // renders as "$-0.00").
        self.cost_segments.iter().fold(0.0, |a, b| a + b)
    }
}

/// `now` is the resolved unix time (NOW override or wall clock) — drives month
/// bucketing and prune staleness. `prune` is `Some(ttl_secs)` to drop sessions
/// older than the TTL (current session always kept), or `None` to keep all.
pub fn record(input: &Input, now: i64, prune: Option<u64>) {
    let Some(path) = sessions_path() else {
        return;
    };
    let mut all = load(&path).unwrap_or_default();
    let sid = input.session_id.as_deref().and_then(sanitize);

    let now = if now >= 0 { now as u64 } else { 0 };
    let mut changed = false;
    if let Some(ttl) = prune {
        changed |= prune_stale(&mut all, now, ttl, sid.as_deref());
    }

    if let Some(rl) = &input.rate_limits {
        let snap = RateLimitsSnapshot {
            five_hour_used_percentage: rl.five_hour.as_ref().and_then(|r| r.used_percentage),
            five_hour_resets_at: rl.five_hour.as_ref().and_then(|r| r.resets_at),
            seven_day_used_percentage: rl.seven_day.as_ref().and_then(|r| r.used_percentage),
            seven_day_resets_at: rl.seven_day.as_ref().and_then(|r| r.resets_at),
            updated_at: now,
        };
        if !rate_limits_equal_ignoring_time(all.rate_limits.as_ref(), &snap) {
            all.rate_limits = Some(snap);
            changed = true;
        }
    }

    if let Some(sid) = &sid {
        if let Some(cw) = &input.context_window {
            let entry = all.sessions.entry(sid.clone()).or_default();
            if let Some(size) = cw.context_window_size {
                if entry.context_window_size != Some(size) {
                    entry.context_window_size = Some(size);
                    changed = true;
                }
            }
            let cur = crate::context::current_usage_sum(cw).unwrap_or(0);
            if cur > 0 {
                let seg_changed = match entry.segments.last_mut() {
                    None => {
                        entry.segments.push(cur);
                        true
                    }
                    Some(last_peak) => {
                        if cur < *last_peak / 2 {
                            entry.segments.push(cur);
                            true
                        } else if cur > *last_peak {
                            *last_peak = cur;
                            true
                        } else {
                            false
                        }
                    }
                };
                if seg_changed {
                    entry.updated_at = now;
                    changed = true;
                }
            }
        }
        if let Some(c) = input.cost.as_ref().and_then(|c| c.total_cost_usd) {
            changed |= apply_cost(&mut all, sid, c, now);
        }
    }

    if changed {
        let _ = save(&path, &all);
    }
}

/// Fold one observed `total_cost_usd` into the per-chat segments and the
/// all-time / monthly aggregates. Returns whether anything changed. Pure over
/// `all` so it can be unit-tested directly. Mirrors the token-segment logic:
/// a drop to below half the last peak (e.g. Claude resetting cost to ~0 mid
/// chat) starts a new segment, so the chat total only ever grows.
pub(crate) fn apply_cost(all: &mut AllSessions, sid: &str, c: f64, now: u64) -> bool {
    if !c.is_finite() || c < 0.0 {
        return false;
    }
    let entry = all.sessions.entry(sid.to_string()).or_default();
    update_cost_segments(&mut entry.cost_segments, c);
    // Keep the day ledger reconciled to the chat total: today's bucket absorbs
    // whatever the ledger is short by. In steady state that's just the new
    // delta; the first time a pre-existing / previously-untracked chat is seen
    // (cost in `cost_segments` but an empty/partial `daily_cost`), the whole
    // backlog lands on today. Guarantees the day/month/all-time sums always
    // cover the chat's own cost.
    let total = entry.cost_total();
    let ledger: f64 = entry.daily_cost.values().fold(0.0, |a, b| a + b);
    let missing = total - ledger;
    if missing <= 0.0 {
        return false;
    }
    entry.updated_at = now;
    *entry.daily_cost.entry(day_key(now as i64)).or_default() += missing;
    true
}

fn update_cost_segments(segments: &mut Vec<f64>, c: f64) {
    match segments.last_mut() {
        None => {
            if c > 0.0 {
                segments.push(c);
            }
        }
        Some(last) => {
            if c < *last / 2.0 {
                if c > 0.0 {
                    segments.push(c);
                }
            } else if c > *last {
                *last = c;
            }
        }
    }
}

/// Local-calendar "YYYY-MM-DD" for a unix timestamp. `chrono::Local` reads the
/// OS tz database (package-manager managed), so it tracks DST/zone changes
/// without a bundled copy. Honors the `TZ` env var (tests pin `TZ=UTC`). The
/// month prefix is the first 7 chars ("YYYY-MM"), since the width is fixed.
pub(crate) fn day_key(now: i64) -> String {
    match Local.timestamp_opt(now, 0).single() {
        Some(dt) => format!("{:04}-{:02}-{:02}", dt.year(), dt.month(), dt.day()),
        None => "0000-00-00".to_string(),
    }
}

pub(crate) fn read_all() -> Option<AllSessions> {
    let path = sessions_path()?;
    load(&path)
}

pub(crate) fn read_session_total(session_id: &str) -> Option<u64> {
    let sid = sanitize(session_id)?;
    let path = sessions_path()?;
    let all = load(&path)?;
    let entry = all.sessions.get(&sid)?;
    Some(entry.segments.iter().sum())
}

pub(crate) fn read_session_cost(session_id: &str) -> Option<f64> {
    let sid = sanitize(session_id)?;
    let path = sessions_path()?;
    let all = load(&path)?;
    let entry = all.sessions.get(&sid)?;
    Some(entry.cost_total())
}

// The day/month/all-time cost totals are inferred by aggregating each session's
// per-day `daily_cost` map across all sessions — no global sum is stored. Folds
// start from +0.0 (not `.sum()`, which yields -0.0 on an empty range, rendering
// as "$-0.00").

pub(crate) fn read_lifetime_cost() -> f64 {
    sessions_path()
        .and_then(|p| load(&p))
        .map(|all| {
            all.sessions
                .values()
                .flat_map(|s| s.daily_cost.values())
                .fold(0.0, |a, b| a + b)
        })
        .unwrap_or(0.0)
}

pub(crate) fn read_month_cost(now: i64) -> f64 {
    let prefix = day_key(now)[..7].to_string(); // "YYYY-MM"
    sessions_path()
        .and_then(|p| load(&p))
        .map(|all| {
            all.sessions
                .values()
                .flat_map(|s| s.daily_cost.iter())
                .filter(|(k, _)| k.starts_with(&prefix))
                .map(|(_, v)| *v)
                .fold(0.0, |a, b| a + b)
        })
        .unwrap_or(0.0)
}

pub(crate) fn read_day_cost(now: i64) -> f64 {
    let key = day_key(now);
    sessions_path()
        .and_then(|p| load(&p))
        .map(|all| {
            all.sessions
                .values()
                .filter_map(|s| s.daily_cost.get(&key))
                .fold(0.0, |a, b| a + b)
        })
        .unwrap_or(0.0)
}

fn rate_limits_equal_ignoring_time(a: Option<&RateLimitsSnapshot>, b: &RateLimitsSnapshot) -> bool {
    match a {
        None => false,
        Some(a) => {
            a.five_hour_used_percentage == b.five_hour_used_percentage
                && a.five_hour_resets_at == b.five_hour_resets_at
                && a.seven_day_used_percentage == b.seven_day_used_percentage
                && a.seven_day_resets_at == b.seven_day_resets_at
        }
    }
}

fn prune_stale(all: &mut AllSessions, now: u64, ttl: u64, keep: Option<&str>) -> bool {
    let before = all.sessions.len();
    all.sessions.retain(|sid, e| {
        if Some(sid.as_str()) == keep {
            return true;
        }
        now.saturating_sub(e.updated_at) <= ttl
    });
    before != all.sessions.len()
}

fn sanitize(session_id: &str) -> Option<String> {
    let safe: String = session_id
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if safe.is_empty() {
        None
    } else {
        Some(safe)
    }
}

fn sessions_path() -> Option<PathBuf> {
    let base = if let Some(x) = std::env::var_os("XDG_CACHE_HOME") {
        PathBuf::from(x)
    } else {
        let home = std::env::var_os("HOME")?;
        PathBuf::from(home).join(".cache")
    };
    let dir = base.join("cc-statusline");
    fs::create_dir_all(&dir).ok()?;
    Some(dir.join("sessions.json"))
}

fn load(path: &PathBuf) -> Option<AllSessions> {
    let s = fs::read_to_string(path).ok()?;
    serde_json::from_str(&s).ok()
}

fn save(path: &PathBuf, all: &AllSessions) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        let json = serde_json::to_string_pretty(all).map_err(std::io::Error::other)?;
        f.write_all(json.as_bytes())?;
        f.write_all(b"\n")?;
    }
    fs::rename(tmp, path)?;
    Ok(())
}

pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(updated_at: u64) -> SessionEntry {
        SessionEntry {
            segments: vec![100],
            updated_at,
            context_window_size: None,
            cost_segments: Vec::new(),
            daily_cost: BTreeMap::new(),
        }
    }

    #[test]
    fn prune_drops_entries_older_than_ttl() {
        let mut all = AllSessions::default();
        all.sessions.insert("fresh".into(), mk(900));
        all.sessions.insert("stale".into(), mk(100));
        let pruned = prune_stale(&mut all, 1000, 500, None);
        assert!(pruned);
        assert!(all.sessions.contains_key("fresh"));
        assert!(!all.sessions.contains_key("stale"));
    }

    #[test]
    fn prune_never_removes_current_session() {
        let mut all = AllSessions::default();
        all.sessions.insert("current".into(), mk(0));
        all.sessions.insert("stale".into(), mk(0));
        let pruned = prune_stale(&mut all, 1_000_000, 100, Some("current"));
        assert!(pruned);
        assert!(all.sessions.contains_key("current"));
        assert!(!all.sessions.contains_key("stale"));
    }

    #[test]
    fn prune_returns_false_when_nothing_stale() {
        let mut all = AllSessions::default();
        all.sessions.insert("a".into(), mk(900));
        all.sessions.insert("b".into(), mk(950));
        let pruned = prune_stale(&mut all, 1000, 500, None);
        assert!(!pruned);
        assert_eq!(all.sessions.len(), 2);
    }

    #[test]
    fn rate_limits_equal_ignores_updated_at() {
        let a = RateLimitsSnapshot {
            five_hour_used_percentage: Some(10.0),
            five_hour_resets_at: Some(100),
            seven_day_used_percentage: Some(20.0),
            seven_day_resets_at: Some(200),
            updated_at: 1,
        };
        let b = RateLimitsSnapshot {
            updated_at: 99999,
            ..a.clone()
        };
        assert!(rate_limits_equal_ignoring_time(Some(&a), &b));
    }

    #[test]
    fn rate_limits_equal_false_when_pct_differs() {
        let a = RateLimitsSnapshot {
            five_hour_used_percentage: Some(10.0),
            ..Default::default()
        };
        let b = RateLimitsSnapshot {
            five_hour_used_percentage: Some(20.0),
            ..Default::default()
        };
        assert!(!rate_limits_equal_ignoring_time(Some(&a), &b));
    }

    #[test]
    fn schema_round_trip_with_new_fields() {
        let all = AllSessions {
            rate_limits: Some(RateLimitsSnapshot {
                five_hour_used_percentage: Some(28.5),
                five_hour_resets_at: Some(1779105000),
                seven_day_used_percentage: Some(21.0),
                seven_day_resets_at: Some(1779138000),
                updated_at: 1779100000,
            }),
            sessions: BTreeMap::from([(
                "s1".to_string(),
                SessionEntry {
                    segments: vec![200_000, 30_000],
                    updated_at: 1779100000,
                    context_window_size: Some(1_000_000),
                    cost_segments: vec![12.5],
                    daily_cost: BTreeMap::new(),
                },
            )]),
        };
        let json = serde_json::to_string(&all).unwrap();
        let back: AllSessions = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.sessions.get("s1").unwrap().segments,
            vec![200_000, 30_000]
        );
        assert_eq!(
            back.sessions.get("s1").unwrap().context_window_size,
            Some(1_000_000)
        );
        assert_eq!(
            back.rate_limits.unwrap().five_hour_used_percentage,
            Some(28.5)
        );
    }

    #[test]
    fn schema_backward_compat_no_new_fields() {
        // Old format: no rate_limits, no context_window_size, no cost fields.
        let json = r#"{"sessions":{"s":{"segments":[100],"updated_at":1}}}"#;
        let back: AllSessions = serde_json::from_str(json).unwrap();
        assert!(back.rate_limits.is_none());
        assert_eq!(back.sessions.get("s").unwrap().segments, vec![100]);
        assert!(back
            .sessions
            .get("s")
            .unwrap()
            .context_window_size
            .is_none());
        assert!(back.sessions.get("s").unwrap().cost_segments.is_empty());
        assert!(back.sessions.get("s").unwrap().daily_cost.is_empty());
    }

    // Per-session day ledgers summed across all sessions (no global store).
    fn ledger_sum(all: &AllSessions) -> f64 {
        all.sessions
            .values()
            .flat_map(|s| s.daily_cost.values())
            .sum()
    }

    fn day_total(all: &AllSessions, key: &str) -> f64 {
        all.sessions
            .values()
            .filter_map(|s| s.daily_cost.get(key))
            .sum()
    }

    #[test]
    fn apply_cost_accumulates_across_in_chat_reset() {
        // 0 is in 1970-01-01 (UTC); the chat total survives a mid-chat reset.
        let mut all = AllSessions::default();
        assert!(apply_cost(&mut all, "a", 50.0, 0));
        assert!(!apply_cost(&mut all, "a", 50.0, 0)); // idempotent: no delta
        assert!(!apply_cost(&mut all, "a", 0.0, 0)); // reset to 0 alone: no positive delta
        assert!(apply_cost(&mut all, "a", 5.0, 0)); // regrows in a fresh segment
        let total = all.sessions.get("a").unwrap().cost_total();
        assert!((total - 55.0).abs() < 1e-9, "chat total = {}", total);
        assert!((ledger_sum(&all) - 55.0).abs() < 1e-9);
    }

    #[test]
    fn apply_cost_reset_to_zero_is_noop_until_regrow() {
        let mut all = AllSessions::default();
        assert!(apply_cost(&mut all, "a", 30.0, 0));
        assert!(!apply_cost(&mut all, "a", 0.0, 0)); // drop to 0 alone adds nothing
        assert!((ledger_sum(&all) - 30.0).abs() < 1e-9);
        assert_eq!(all.sessions.get("a").unwrap().cost_segments, vec![30.0]);
    }

    #[test]
    fn apply_cost_ledger_sums_across_chats_same_day() {
        std::env::set_var("TZ", "UTC");
        let mut all = AllSessions::default();
        apply_cost(&mut all, "a", 50.0, 0);
        apply_cost(&mut all, "b", 10.0, 0);
        // Two separate sessions, each with its own day ledger; total for the
        // day is the sum across sessions.
        assert!((day_total(&all, "1970-01-01") - 60.0).abs() < 1e-9);
        assert!((all.sessions.get("a").unwrap().daily_cost["1970-01-01"] - 50.0).abs() < 1e-9);
        assert!((all.sessions.get("b").unwrap().daily_cost["1970-01-01"] - 10.0).abs() < 1e-9);
        assert!((ledger_sum(&all) - 60.0).abs() < 1e-9);
    }

    #[test]
    fn apply_cost_buckets_by_day() {
        // TZ is process-wide; pin to UTC so the keys are deterministic.
        std::env::set_var("TZ", "UTC");
        let mut all = AllSessions::default();
        apply_cost(&mut all, "a", 40.0, 0); // 1970-01-01
        apply_cost(&mut all, "b", 5.0, 2_700_000); // 1970-02-01 (~31.25 days)
        assert!((day_total(&all, "1970-01-01") - 40.0).abs() < 1e-9);
        assert!((day_total(&all, "1970-02-01") - 5.0).abs() < 1e-9);
    }

    #[test]
    fn day_key_and_month_prefix_under_utc() {
        std::env::set_var("TZ", "UTC");
        let k = day_key(0);
        assert_eq!(k, "1970-01-01");
        assert_eq!(&k[..7], "1970-01");
    }

    #[test]
    fn apply_cost_backfills_untracked_chat_into_today() {
        std::env::set_var("TZ", "UTC");
        let mut all = AllSessions::default();
        // Pre-existing chat: cost already in cost_segments (e.g. from before the
        // per-day ledger existed), but daily_cost is empty.
        all.sessions.insert(
            "old".into(),
            SessionEntry {
                cost_segments: vec![186.07],
                ..Default::default()
            },
        );
        // First observation at the same total → backlog lands on today, so the
        // ledger sum now matches the chat total (sums >= chat cost).
        assert!(apply_cost(&mut all, "old", 186.07, 0));
        assert!((day_total(&all, "1970-01-01") - 186.07).abs() < 1e-9);
        assert!((ledger_sum(&all) - 186.07).abs() < 1e-9);
        // A later charge tracks normally as a delta.
        assert!(apply_cost(&mut all, "old", 198.04, 0));
        assert!((day_total(&all, "1970-01-01") - 198.04).abs() < 1e-9);
    }

    #[test]
    fn cost_total_of_empty_session_is_positive_zero() {
        // A session created for token/context tracking but with no cost yet:
        // cost_total() must be +0.0, never -0.0 (which renders as "$-0.00").
        let entry = SessionEntry::default();
        assert!(entry.cost_segments.is_empty());
        assert!(entry.cost_total().is_sign_positive(), "got negative zero");
        assert_eq!(format!("${:.2}", entry.cost_total()), "$0.00");
    }

    #[test]
    fn apply_cost_rejects_negative_and_nan() {
        let mut all = AllSessions::default();
        assert!(!apply_cost(&mut all, "a", -1.0, 0));
        assert!(!apply_cost(&mut all, "a", f64::NAN, 0));
        assert!(ledger_sum(&all) == 0.0);
    }
}
