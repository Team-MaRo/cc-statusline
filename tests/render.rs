use std::io::Write;
use std::process::{Command, Stdio};

fn run(format_lines: &[&str], now: i64, stdin_json: &str) -> String {
    let exe = env!("CARGO_BIN_EXE_cc-statusline");
    let mut cmd = Command::new(exe);
    cmd.env("NO_COLOR", "1")
        .env("NOW", now.to_string())
        .env_remove("CC_VIM_MODE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for f in format_lines {
        cmd.arg(f);
    }
    let mut child = cmd.spawn().expect("spawn");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin_json.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn model_only() {
    let out = run(&["%m"], 0, r#"{"model":{"display_name":"Opus 4.7"}}"#);
    assert_eq!(out, "Opus 4.7\n");
}

#[test]
fn empty_token_collapses_space() {
    let out = run(&["a %vim b"], 0, r#"{}"#);
    assert_eq!(out, "a b\n");
}

#[test]
fn literal_percent_escape() {
    let out = run(&["100%% done"], 0, r#"{}"#);
    assert_eq!(out, "100% done\n");
}

#[test]
fn multiline_backslash_n() {
    let out = run(&["one\\ntwo"], 0, r#"{}"#);
    assert_eq!(out, "one\ntwo\n");
}

#[test]
fn multi_arg_lines() {
    let out = run(&["%m", "literal"], 0, r#"{"model":{"display_name":"X"}}"#);
    assert_eq!(out, "X\nliteral\n");
}

#[test]
fn max_3_lines() {
    let out = run(&["1", "2", "3", "4"], 0, r#"{}"#);
    assert_eq!(out, "1\n2\n3\n");
}

#[test]
fn passthrough_returns_stdin_verbatim() {
    let json = r#"{"model":{"display_name":"Opus 4.7"},"session_id":"abc123"}"#;
    let out = run(&["--passthrough"], 0, json);
    assert_eq!(out, json);
}

#[test]
fn passthrough_ignores_format_args() {
    let json = r#"{"model":{"display_name":"X"}}"#;
    let out = run(&["--passthrough", "%m"], 0, json);
    assert_eq!(out, json);
}

#[test]
fn peak_hours() {
    // 2024-01-02 15:00 UTC = 1704207600 (inside 13-19 peak window)
    let out = run(&["%pk"], 1704207600, r#"{}"#);
    assert!(out.starts_with("peak "), "got: {}", out);
}

#[test]
fn off_peak_hours() {
    // 2024-01-02 08:00 UTC = 1704182400
    let out = run(&["%pk"], 1704182400, r#"{}"#);
    assert!(out.starts_with("off-peak "), "got: {}", out);
}

#[test]
fn rate5h_renders_when_populated() {
    let now = 1704207600_i64;
    let json = format!(
        r#"{{"rate_limits":{{"five_hour":{{"used_percentage":50,"resets_at":{}}}}}}}"#,
        now + 7200
    );
    let out = run(&["%rl5"], now, &json);
    assert!(out.contains('[') && out.contains(']'), "got: {}", out);
    assert!(out.contains("50%"), "got: {}", out);
}

#[test]
fn rate5h_missing_renders_empty_bar() {
    let out = run(&["%rl5"], 0, r#"{}"#);
    assert!(out.contains("[░░░░░░░░░░]"), "got: {}", out);
    assert!(out.contains("0%"), "got: {}", out);
}

#[test]
fn ctx_bar_gradient_partial_cell() {
    let out = run(
        &["%cub"],
        0,
        r#"{"context_window":{"context_window_size":1000000,"used_percentage":17}}"#,
    );
    assert!(out.contains("[█▓░░░░░░░░]"), "got: {}", out);
}

#[test]
fn ctx_bar_full_at_100() {
    let out = run(
        &["%cub"],
        0,
        r#"{"context_window":{"context_window_size":1000000,"used_percentage":100}}"#,
    );
    assert!(out.contains("[██████████]"), "got: {}", out);
}

#[test]
fn ctx_bar_renders() {
    let out = run(
        &["%cu"],
        0,
        r#"{"context_window":{"context_window_size":200000,"used_percentage":25}}"#,
    );
    assert!(out.contains("50k/200k"), "got: {}", out);
    assert!(out.contains("(25%)"), "got: {}", out);
}

#[test]
fn style_suffix_no_color_passthrough() {
    let out = run(&["%m:red:bold"], 0, r#"{"model":{"display_name":"Opus"}}"#);
    assert_eq!(out, "Opus\n");
}

#[test]
fn cost_formats_dollars() {
    let out = run(&["%c"], 0, r#"{"cost":{"total_cost_usd":1.2345}}"#);
    assert_eq!(out, "$1.23\n");
}

#[test]
fn session_id_short() {
    let out = run(
        &["%sid"],
        0,
        r#"{"session_id":"d6486277-1d9a-4634-8f1c-b410dc89e3c1"}"#,
    );
    assert_eq!(out, "d6486277\n");
}

#[test]
fn unknown_token_verbatim() {
    let out = run(&["%nope"], 0, r#"{}"#);
    assert_eq!(out, "%nope\n");
}

#[test]
fn group_span_renders_literal_and_nested_token() {
    let out = run(
        &["%[dim Model:] %m"],
        0,
        r#"{"model":{"display_name":"Opus"}}"#,
    );
    assert_eq!(out, "Model: Opus\n");
}

#[test]
fn cwd_base_only_basename() {
    let out = run(&["%cwdb"], 0, r#"{"cwd":"/tmp/some/project"}"#);
    assert_eq!(out, "project\n");
}

#[test]
fn ctx_percent_usable_scales_against_usable_budget() {
    // 200k reserves ~20k → usable 180k (ratio 0.90). 45% raw → 45/0.90 = 50%.
    let out = run(
        &["%cpu"],
        0,
        r#"{"context_window":{"context_window_size":200000,"used_percentage":45}}"#,
    );
    assert_eq!(out, "50.0%\n");
}

fn run_with_cache(format_lines: &[&str], stdin_json: &str, cache_dir: &std::path::Path) -> String {
    let exe = env!("CARGO_BIN_EXE_cc-statusline");
    let mut cmd = Command::new(exe);
    cmd.env("NO_COLOR", "1")
        .env("NOW", "0")
        .env("XDG_CACHE_HOME", cache_dir)
        .env_remove("CC_VIM_MODE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for f in format_lines {
        cmd.arg(f);
    }
    let mut child = cmd.spawn().expect("spawn");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin_json.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn tokens_total_accumulates_across_compaction() {
    let tmpdir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("cc-statusline-compact-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmpdir);
    std::fs::create_dir_all(&tmpdir).unwrap();

    // Pre-compact turn: 200k current_usage. Persists segments=[200000].
    let _ = run_with_cache(
        &["%tt"],
        r#"{"session_id":"compact-test","context_window":{"context_window_size":1000000,"current_usage":{"input_tokens":200000,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
        &tmpdir,
    );

    // Post-compact turn: 50k current_usage. < 50% of last_peak → new segment.
    // segments=[200000, 50000], sum=250000.
    let out = run_with_cache(
        &["%tt"],
        r#"{"session_id":"compact-test","context_window":{"context_window_size":1000000,"current_usage":{"input_tokens":50000,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
        &tmpdir,
    );
    assert_eq!(out, "250k\n");
}

fn run_with_cache_at(
    format_lines: &[&str],
    stdin_json: &str,
    cache_dir: &std::path::Path,
    now: i64,
    tz: &str,
) -> String {
    let exe = env!("CARGO_BIN_EXE_cc-statusline");
    let mut cmd = Command::new(exe);
    cmd.env("NO_COLOR", "1")
        .env("NOW", now.to_string())
        .env("TZ", tz)
        .env("XDG_CACHE_HOME", cache_dir)
        .env_remove("CC_VIM_MODE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for f in format_lines {
        cmd.arg(f);
    }
    let mut child = cmd.spawn().expect("spawn");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin_json.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    String::from_utf8(out.stdout).unwrap()
}

fn fresh_cache(tag: &str) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "cc-statusline-{}-{}",
        tag,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn cost_persists_across_in_chat_reset() {
    let dir = fresh_cache("cost-reset");
    let json = |c: &str| {
        format!(
            r#"{{"session_id":"cost-test","cost":{{"total_cost_usd":{}}}}}"#,
            c
        )
    };
    let _ = run_with_cache(&["%c"], &json("50"), &dir);
    let _ = run_with_cache(&["%c"], &json("0"), &dir); // Claude resets cost mid-chat
    let out = run_with_cache(&["%c"], &json("5"), &dir); // regrows
    assert_eq!(out, "$55.00\n");
}

#[test]
fn cost_all_sums_across_chats() {
    let dir = fresh_cache("cost-all");
    let _ = run_with_cache(
        &["%c"],
        r#"{"session_id":"chat-a","cost":{"total_cost_usd":50}}"#,
        &dir,
    );
    let out = run_with_cache(
        &["%c %ca"],
        r#"{"session_id":"chat-b","cost":{"total_cost_usd":10}}"#,
        &dir,
    );
    assert_eq!(out, "$10.00 $60.00\n");
}

#[test]
fn cost_never_renders_negative_zero() {
    // A session with token/context activity but no cost field persists an entry
    // with empty cost_segments. %cost must render "$0.00", never "$-0.00".
    let dir = fresh_cache("cost-negzero");
    let out = run_with_cache(
        &["%c"],
        r#"{"session_id":"nz","context_window":{"context_window_size":1000000,"current_usage":{"input_tokens":100,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
        &dir,
    );
    assert_eq!(out, "$0.00\n", "got: {:?}", out);
    assert!(!out.contains("-0.00"));
}

#[test]
fn untracked_chat_backfills_so_sums_cover_chat_cost() {
    // Reproduces: a chat with pre-existing cost in cost_segments but no per-day
    // ledger (state written before per-day tracking). The sums must not be
    // smaller than the chat cost — the backlog is back-filled into today.
    let dir = fresh_cache("cost-backfill");
    let cc = dir.join("cc-statusline");
    std::fs::create_dir_all(&cc).unwrap();
    std::fs::write(
        cc.join("sessions.json"),
        r#"{"sessions":{"old":{"segments":[1000],"updated_at":1,"cost_segments":[186.07]}}}"#,
    )
    .unwrap();
    let out = run_with_cache_at(
        &["%c %cd %cm %ca"],
        r#"{"session_id":"old","cost":{"total_cost_usd":186.07}}"#,
        &dir,
        0,
        "UTC",
    );
    assert_eq!(out, "$186.07 $186.07 $186.07 $186.07\n", "got: {:?}", out);
}

#[test]
fn cost_day_month_all_derive_from_ledger() {
    let dir = fresh_cache("cost-ledger");
    // Day 1970-01-01 (UTC): two chats → today/month/all all = $60.
    let _ = run_with_cache_at(
        &["%c"],
        r#"{"session_id":"a","cost":{"total_cost_usd":50}}"#,
        &dir,
        0,
        "UTC",
    );
    let out = run_with_cache_at(
        &["%c %cd %cm %ca"],
        r#"{"session_id":"b","cost":{"total_cost_usd":10}}"#,
        &dir,
        0,
        "UTC",
    );
    assert_eq!(out, "$10.00 $60.00 $60.00 $60.00\n");

    // A later day in the SAME month (~1970-01-03): %cd resets, %cm/%ca grow.
    let out2 = run_with_cache_at(
        &["%cd %cm %ca"],
        r#"{"session_id":"c","cost":{"total_cost_usd":7}}"#,
        &dir,
        200_000,
        "UTC",
    );
    assert_eq!(out2, "$7.00 $67.00 $67.00\n");

    // A later MONTH (~1970-02-01) with no spend: %cd and %cm are $0, %ca holds.
    let out3 = run_with_cache_at(
        &["%cd %cm %ca"],
        r#"{"session_id":"d","cost":{"total_cost_usd":0}}"#,
        &dir,
        2_700_000,
        "UTC",
    );
    assert_eq!(out3, "$0.00 $0.00 $67.00\n");
}

#[test]
fn prune_off_by_default_on_but_with_flag() {
    let read_sessions = |dir: &std::path::Path| {
        std::fs::read_to_string(dir.join("cc-statusline").join("sessions.json")).unwrap_or_default()
    };

    // Default (no --prune): an aged session is kept.
    let keep = fresh_cache("prune-keep");
    let _ = run_with_cache_at(
        &["%c"],
        r#"{"session_id":"old","cost":{"total_cost_usd":5}}"#,
        &keep,
        1_000,
        "UTC",
    );
    let _ = run_with_cache_at(
        &["%c"],
        r#"{"session_id":"cur","cost":{"total_cost_usd":5}}"#,
        &keep,
        1_000_000,
        "UTC",
    );
    let kept = read_sessions(&keep);
    assert!(kept.contains("\"old\""), "old should be kept: {}", kept);
    assert!(kept.contains("\"cur\""));

    // With --prune 1s: the aged session is dropped, current session retained.
    let drop = fresh_cache("prune-drop");
    let _ = run_with_cache_at(
        &["%c"],
        r#"{"session_id":"old","cost":{"total_cost_usd":5}}"#,
        &drop,
        1_000,
        "UTC",
    );
    let _ = run_with_cache_at(
        &["--prune", "1s", "%c"],
        r#"{"session_id":"cur","cost":{"total_cost_usd":5}}"#,
        &drop,
        1_000_000,
        "UTC",
    );
    let pruned = read_sessions(&drop);
    assert!(
        !pruned.contains("\"old\""),
        "old should be pruned: {}",
        pruned
    );
    assert!(pruned.contains("\"cur\""), "cur should remain: {}", pruned);
}

#[test]
fn tokens_total_sums_current_usage() {
    // %tt = live current_usage sum (input + output + cache_creation + cache_read).
    let out = run(
        &["%tt"],
        0,
        r#"{"context_window":{"current_usage":{"input_tokens":100,"output_tokens":100,"cache_creation_input_tokens":300,"cache_read_input_tokens":1500}}}"#,
    );
    assert_eq!(out, "2k\n");
}

#[test]
fn total_speed_tokens_per_api_second() {
    let out = run(
        &["%ts"],
        0,
        r#"{"context_window":{"current_usage":{"input_tokens":500,"output_tokens":0,"cache_creation_input_tokens":100,"cache_read_input_tokens":400}},"cost":{"total_api_duration_ms":10000}}"#,
    );
    assert_eq!(out, "100 tok/s\n");
}

#[test]
fn total_speed_zero_when_no_api_time() {
    let out = run(
        &["%ts"],
        0,
        r#"{"context_window":{"current_usage":{"input_tokens":1000,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
    );
    assert_eq!(out, "0 tok/s\n");
}

#[test]
fn lines_added_and_removed() {
    let out = run(
        &["%la %lr"],
        0,
        r#"{"cost":{"total_lines_added":42,"total_lines_removed":7}}"#,
    );
    assert_eq!(out, "+42 -7\n");
}

#[test]
fn duration_formats_minutes_and_seconds() {
    // 14m12s = (14*60 + 12)*1000 = 852000 ms
    let out = run(&["%dur"], 0, r#"{"cost":{"total_duration_ms":852000}}"#);
    assert_eq!(out, "14m12s\n");
}

#[test]
fn vim_env_token_renders_when_set() {
    let exe = env!("CARGO_BIN_EXE_cc-statusline");
    let mut cmd = Command::new(exe);
    cmd.env("NO_COLOR", "1")
        .env("NOW", "0")
        .env("CC_VIM_MODE", "NORMAL")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .arg("%vim");
    let mut child = cmd.spawn().expect("spawn");
    child.stdin.as_mut().unwrap().write_all(b"{}").unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "NORMAL\n");
}

#[test]
fn rate5h_delta_ahead_of_curve_shows_positive_sign() {
    // 80% used, reset in 4h → 1h elapsed of 5h window → expected 20%. Δ+60.
    let now = 1_704_182_400_i64;
    let json = format!(
        r#"{{"rate_limits":{{"five_hour":{{"used_percentage":80,"resets_at":{}}}}}}}"#,
        now + 4 * 3600
    );
    let out = run(&["%rl5d"], now, &json);
    assert_eq!(out, "Δ+60.00%\n");
}

#[test]
fn rate5h_delta_behind_curve_shows_negative_sign() {
    // 50% used, reset in 2h → 3h elapsed of 5h window → expected 60%. Δ-10.
    let now = 1_704_182_400_i64;
    let json = format!(
        r#"{{"rate_limits":{{"five_hour":{{"used_percentage":50,"resets_at":{}}}}}}}"#,
        now + 2 * 3600
    );
    let out = run(&["%rl5d"], now, &json);
    assert_eq!(out, "Δ-10.00%\n");
}

#[test]
fn rate5h_bar_tip_red_when_over_at_rounding_boundary() {
    // Regression: bar tip color must agree with the Δ delta color.
    // used 7.0%, expected 6.0% (18min elapsed of 5h) — both round to 2/30
    // units (glyph ▓), but raw used > expected → over → red delta.
    // Pre-fix the bar quantized to equal units and painted a GREEN tip.
    // NOW=0 (run_color), resets_at = (300-18)*60 = 16920.
    let json = r#"{"rate_limits":{"five_hour":{"used_percentage":7.0,"resets_at":16920}}}"#;
    let out = run_color(&["%rl5"], json);
    assert!(
        out.contains("\x1b[31m▓"),
        "expected red bar tip, got: {:?}",
        out
    );
    assert!(
        !out.contains("\x1b[32m"),
        "bar must not be green while over-curve, got: {:?}",
        out
    );
}

#[test]
fn version_flag_prints_and_exits() {
    let exe = env!("CARGO_BIN_EXE_cc-statusline");
    let out = Command::new(exe).arg("-V").output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.starts_with("cc-statusline "), "got: {}", s);
}

#[test]
fn zero_args_no_output() {
    let out = run(&[], 0, r#"{"model":{"display_name":"Opus"}}"#);
    assert_eq!(out, "");
}

fn run_color(format_lines: &[&str], stdin_json: &str) -> String {
    let exe = env!("CARGO_BIN_EXE_cc-statusline");
    let mut cmd = Command::new(exe);
    cmd.env_remove("NO_COLOR")
        .env("NOW", "0")
        .env_remove("CC_VIM_MODE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for f in format_lines {
        cmd.arg(f);
    }
    let mut child = cmd.spawn().expect("spawn");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin_json.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn ctx_default_green_when_not_exceeding() {
    let out = run_color(
        &["%cup"],
        r#"{"context_window":{"context_window_size":1000000,"used_percentage":10.0},"exceeds_200k_tokens":false}"#,
    );
    assert!(
        out.contains("\x1b[32m") && out.contains("10%"),
        "got: {:?}",
        out
    );
}

#[test]
fn ctu_default_red_when_exceeding() {
    // %ctu retains the binary 200k coloring; %cup moved to the auto-compact gradient.
    let out = run_color(
        &["%ctu"],
        r#"{"context_window":{"context_window_size":1000000,"used_percentage":25.0},"exceeds_200k_tokens":true}"#,
    );
    assert!(
        out.contains("\x1b[31m") && out.contains("250k"),
        "got: {:?}",
        out
    );
}

#[test]
fn ctx_default_green_when_flag_missing() {
    let out = run_color(
        &["%cup"],
        r#"{"context_window":{"context_window_size":1000000,"used_percentage":25.0}}"#,
    );
    assert!(
        out.contains("\x1b[32m") && !out.contains("\x1b[31m"),
        "got: {:?}",
        out
    );
}

#[test]
fn ctx_user_style_overrides_default() {
    let out = run_color(
        &["%cup:cyan"],
        r#"{"context_window":{"context_window_size":1000000,"used_percentage":10.0},"exceeds_200k_tokens":true}"#,
    );
    assert!(
        out.contains("\x1b[36m") && !out.contains("\x1b[31m"),
        "got: {:?}",
        out
    );
}

#[test]
fn ctx_pct_usable_green_below_70() {
    // 45% raw of 200k (usable 180k) → usable = 50% → green
    let out = run_color(
        &["%cpu"],
        r#"{"context_window":{"context_window_size":200000,"used_percentage":45}}"#,
    );
    assert!(
        out.contains("\x1b[32m") && out.contains("50.0%"),
        "got: {:?}",
        out
    );
}

#[test]
fn ctx_pct_usable_yellow_at_75() {
    // 67.5% raw of 200k (usable 180k) → usable = 75% → yellow
    let out = run_color(
        &["%cpu"],
        r#"{"context_window":{"context_window_size":200000,"used_percentage":67.5}}"#,
    );
    assert!(
        out.contains("\x1b[33m") && out.contains("75.0%"),
        "got: {:?}",
        out
    );
}

#[test]
fn ctx_pct_usable_bright_yellow_at_90() {
    // 81% raw of 200k (usable 180k) → usable = 90% → bright_yellow
    let out = run_color(
        &["%cpu"],
        r#"{"context_window":{"context_window_size":200000,"used_percentage":81}}"#,
    );
    assert!(
        out.contains("\x1b[93m") && out.contains("90.0%"),
        "got: {:?}",
        out
    );
}

#[test]
fn ctx_pct_usable_red_at_100() {
    // 90% raw of 200k (usable 180k) → usable = 100% → red
    let out = run_color(
        &["%cpu"],
        r#"{"context_window":{"context_window_size":200000,"used_percentage":90}}"#,
    );
    assert!(
        out.contains("\x1b[31m") && out.contains("100.0%"),
        "got: {:?}",
        out
    );
}

#[test]
fn ctx_pct_usable_ignores_exceeds_200k_flag() {
    // exceeds_200k_tokens=true but usable% low → still green, never red.
    let out = run_color(
        &["%cpu"],
        r#"{"context_window":{"context_window_size":1000000,"used_percentage":25.0},"exceeds_200k_tokens":true}"#,
    );
    assert!(
        out.contains("\x1b[32m") && !out.contains("\x1b[31m"),
        "got: {:?}",
        out
    );
}

#[test]
fn empty_token_between_space_separators_collapses_once() {
    // Per README: only one surrounding space collapses. `A %vim %c` → `A $0.10`, not `A  $0.10`.
    let out = run(&["A %vim %c"], 0, r#"{"cost":{"total_cost_usd":0.1}}"#);
    assert_eq!(out, "A $0.10\n");
}

#[test]
fn cuu_composite_renders_bar_tokens_pct_on_1m() {
    // 1M, 10% used → usable budget = 980k, pct_usable ≈ 10.2%.
    let out = run(
        &["%cuu"],
        0,
        r#"{"context_window":{"context_window_size":1000000,"used_percentage":10}}"#,
    );
    assert!(out.contains("100k/980k"), "got: {:?}", out);
    assert!(out.contains("(10%)"), "got: {:?}", out);
    assert!(out.starts_with('['), "expected bar prefix, got: {:?}", out);
}

#[test]
fn cpu_uses_98pct_ratio_on_1m_context() {
    // 95% raw of 1M → 95 / 0.98 ≈ 96.9%, no longer capped at 100%.
    let out = run(
        &["%cpu"],
        0,
        r#"{"context_window":{"context_window_size":1000000,"used_percentage":95}}"#,
    );
    assert_eq!(out, "96.9%\n");
}

#[test]
fn ctx_pct_default_gradient_yellow_at_75pct_usable() {
    // 200k size (usable 180k), 72% raw → 72/0.90 = 80% usable → yellow.
    let out = run_color(
        &["%cup"],
        r#"{"context_window":{"context_window_size":200000,"used_percentage":72}}"#,
    );
    assert!(
        out.contains("\x1b[33m") && out.contains("72%"),
        "got: {:?}",
        out
    );
}

#[test]
fn ctx_pct_default_gradient_red_near_compact() {
    // 1M size, 95% raw → 95/0.98 ≈ 96.9% usable → red.
    let out = run_color(
        &["%cup"],
        r#"{"context_window":{"context_window_size":1000000,"used_percentage":95}}"#,
    );
    assert!(
        out.contains("\x1b[31m") && out.contains("95%"),
        "got: {:?}",
        out
    );
}

#[test]
fn ctx_composite_pct_uses_usable_gradient() {
    // 1M size, current_usage sum ≈ 950k → pct portion red (gradient); used portion red (exceeds_200k).
    let out = run_color(
        &["%cu"],
        r#"{"context_window":{"context_window_size":1000000,"current_usage":{"input_tokens":50000,"output_tokens":0,"cache_creation_input_tokens":100000,"cache_read_input_tokens":800000},"used_percentage":95},"exceeds_200k_tokens":true}"#,
    );
    // pct shown is the raw 95% (`(95%)`), wrapped in red.
    assert!(
        out.contains("\x1b[31m") && out.contains("(\x1b[31m95%"),
        "got: {:?}",
        out
    );
}

#[test]
fn ctx_bar_no_red_without_exceeds_flag() {
    // 964k of 1M but exceeds_200k_tokens absent → the 200k flag is the source of
    // truth, so the bar must NOT redden (matches the green flag-driven number).
    let out = run_color(
        &["%cubu"],
        r#"{"context_window":{"context_window_size":1000000,"current_usage":{"input_tokens":964000,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
    );
    assert!(
        !out.contains("\x1b[31m"),
        "bar must not be red without exceeds_200k_tokens, got: {:?}",
        out
    );
}

#[test]
fn ctx_bar_red_with_exceeds_flag() {
    // Same usage with the flag set → bar reddens past the 200k mark.
    let out = run_color(
        &["%cubu"],
        r#"{"context_window":{"context_window_size":1000000,"current_usage":{"input_tokens":964000,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}},"exceeds_200k_tokens":true}"#,
    );
    assert!(
        out.contains("\x1b[31m"),
        "bar should redden past 200k when flag is set, got: {:?}",
        out
    );
}

#[test]
fn ctx_used_number_green_without_exceeds_flag() {
    // The used-token number stays driven by the backend flag (no local 200k math).
    let out = run_color(
        &["%ctu"],
        r#"{"context_window":{"context_window_size":1000000,"current_usage":{"input_tokens":964000,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
    );
    assert!(
        out.contains("\x1b[32m964k") && !out.contains("\x1b[31m"),
        "used number should stay green when flag absent, got: {:?}",
        out
    );
}

#[test]
fn cpu_uses_90pct_ratio_at_200k_size() {
    // 200k reserves ~20k → usable 180k (ratio 0.90). 45% raw → 45 / 0.90 = 50%.
    let out = run(
        &["%cpu"],
        0,
        r#"{"context_window":{"context_window_size":200000,"used_percentage":45}}"#,
    );
    assert_eq!(out, "50.0%\n");
}
