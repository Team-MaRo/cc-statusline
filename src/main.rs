mod bar;
mod color;
mod context;
mod duration;
mod format;
mod git;
mod input;
mod mcp;
mod peak;
mod rate;
mod settings;
mod state;
mod tokens;

use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LINES: usize = 3;

const DEFAULT_LINES: &[&str] = &[];

fn print_help() {
    println!(
        "cc-statusline {ver}

Reads Claude Code statusline JSON from stdin and prints formatted lines.

USAGE
  cc-statusline [FLAGS] [FMT_LINE]...
  cc-statusline --passthrough
  cc-statusline mcp

  Each positional FMT_LINE = one output line (max 3 total).
  Literal `\\n` inside a FMT_LINE also splits into a new line.
  Zero args = no output. Configure format strings in settings.json.

  `cc-statusline --passthrough` writes stdin JSON unchanged to stdout
  while still updating the MCP state file. Use to feed the MCP server
  while another tool renders the statusline:
    `cc-statusline --passthrough | other-statusline-tool`

  `cc-statusline mcp` runs a stdio MCP server exposing context_usage,
  rate_limits, and session_summary tools (reads the persisted state file).

FORMAT SYNTAX
  %<name>              render a token
  %<name>:style[:...]  apply ANSI styles to that token's output
  %[style content]     apply ANSI styles to a span (literal + nested tokens)
  %%                   literal `%`
  \\n                   line break inside an arg

STYLES
  colors     black red green yellow blue magenta cyan white gray
             bright_red bright_green bright_yellow bright_blue
             bright_magenta bright_cyan bright_white
  modifiers  bold dim italic underline
  Combine:   %m:cyan:bold     %[dim:italic Label:]
  Disabled when --no-color or NO_COLOR env set.

FLAGS
  --no-color     disable ANSI colors (also: NO_COLOR env)
  --now <unix>   override current time for peak/rate math (also: NOW env)
  --prune <DUR>  drop persisted sessions older than DUR (current session always
                 kept). Off by default — state is retained forever unless this
                 is set. DUR = seconds or s/m/h/d/w suffix, e.g. 30d, 12h, 4w.
                 (also: CC_STATUSLINE_STATE_TTL_SECONDS env, in seconds)
  --debug        dump stdin JSON to stderr before rendering
  --passthrough  write stdin JSON unchanged to stdout; still updates the
                 MCP state file. Use to chain another statusline tool:
                 `cc-statusline --passthrough | other-tool`
                 Positional FMT_LINE args are ignored in this mode.
  -h, --help     this help
  -V, --version  print version

TOKENS — MODEL / SESSION
  %m, %model               model display name.  e.g. \"Opus 4.7 (1M context)\"
  %mid, %model_id          model API identifier.  e.g. \"claude-opus-4-7\"
  %e, %effort              effortLevel from ~/.claude/settings.json.  e.g. \"medium\"
  %f, %fast                \"fast\" when fast_mode is true, else empty.
  %v, %version             Claude Code version.  e.g. \"2.1.118\"
  %sid, %session_id        first 8 chars of session_id.  e.g. \"d6486277\"
  %os, %output_style       output_style.name.  e.g. \"default\"
  %vim                     $CC_VIM_MODE env if set.  e.g. \"NORMAL\"

TOKENS — CONTEXT WINDOW
  %cu, %ctx, %context      composite bar + tokens + percent vs full window.  e.g. \"[█▓░░░░░░░░] 156k/1.0M (16%)\"
  %cuu, %ctx_usable        composite bar + tokens + percent vs usable budget (pre-auto-compact).  e.g. \"[█▓░░░░░░░░] 156k/980k (16%)\"
  %cub, %ctx_bar           bar only.  e.g. \"[█▓░░░░░░░░]\"
  %cup, %ctx_pct           used % (rounded); colored by the auto-compact gradient (same as %cpu).  e.g. \"16%\"
  %ct, %ctx_tokens         used/max pair.  e.g. \"156k/1.0M\"
  %ctu, %ctx_tokens_used   used tokens only; red when exceeds_200k_tokens, else green.  e.g. \"156k\"
  %ctm, %ctx_tokens_max    max tokens only.  e.g. \"1.0M\"
  %cpu, %ctx_pct_usable    used % of pre-auto-compact budget (0.98 for ≥500k models, 0.80 otherwise, matches Claude Code's \"X% context used\"); gradients green→yellow→bright_yellow→red at 70/85/95%.  e.g. \"19.5%\"
  %cubu, %ctx_bar_usable   usable-scaled bar.  e.g. \"[█▓░░░░░░░░]\"
  %ctpu, %ctx_tokens_usable  used / usable pair.  e.g. \"156k/980k\"
  %tt, %tokens_total       cumulative session tokens (sum of per-compact-segment peaks; persisted in $XDG_CACHE_HOME/cc-statusline/sessions.json; retained forever unless --prune is set).  e.g. \"4.8M\"
  %ts, %total_speed        throughput (tokens per API-second).  e.g. \"62 tok/s\"

TOKENS — RATE LIMITS (5-hour rolling window)
  %rl5, %rate5h            composite: bar + %(Δ%) + reset(Δt).  e.g. \"[█▓░░░░░░░░] 14%(Δ-34%) 2h36m(Δ-1h41m)\"
  %rl5p, %rate5h_pct       used %.  e.g. \"14%\"
  %rl5b, %rate5h_bar       bar only.  e.g. \"[█▓░░░░░░░░]\"
  %rl5r, %rate5h_reset     time until reset.  e.g. \"2h36m\"
  %rl5d, %rate5h_delta     Δ vs expected curve.  e.g. \"Δ-34%\"
  %rl5td, %rate5h_time_delta  Δ vs expected remaining time.  e.g. \"Δ-1h41m\"

TOKENS — RATE LIMITS (7-day rolling window)
  %rl7, %rate7d            same composite for weekly.  e.g. \"[██▓░░░░░░░] 28%(Δ-56%) 1d02h(Δ-3d22h)\"
  %rl7p, %rate7d_pct       used %.
  %rl7b, %rate7d_bar       bar only.
  %rl7r, %rate7d_reset     time until reset.
  %rl7d, %rate7d_delta     Δ vs expected curve.
  %rl7td, %rate7d_time_delta  Δ vs expected remaining time.

TOKENS — TIME / PEAK
  %pk, %peak               peak/off-peak label + countdown (5-11 AM US/Pacific, Mon-Fri).
                           e.g. \"peak 4h19m\" (red) or \"off-peak 2h16m\" (green)

TOKENS — GIT
  %b, %branch              current branch with glyph.  e.g. \"⎇ main\"
  %bn, %branch_name        branch name only.  e.g. \"main\"
  %d, %diff                unstaged+staged line-change count.  e.g. \"(+12,-3)\"

TOKENS — COST / FILE-SYSTEM
  %c, %cost                this chat's cumulative cost in USD; persisted so it survives Claude resetting total_cost_usd mid-chat (never goes backwards).  e.g. \"$0.35\"
  %cd, %cost_day           today's cost, summed across all chats' per-day ledgers.  e.g. \"$48.10\"
  %cm, %cost_month         current local calendar month's cost, summed across all chats' per-day ledgers.  e.g. \"$310.20\"
  %ca, %cost_all           all-time cost, summed across all chats' per-day ledgers (a chat's history is dropped if its session is --pruned).  e.g. \"$2014.50\"
  %la, %lines_added        total_lines_added.  e.g. \"+42\"
  %lr, %lines_removed      total_lines_removed.  e.g. \"-7\"
  %dur, %duration          total_duration_ms pretty.  e.g. \"14m12s\"
  %apidur, %api_duration   total_api_duration_ms pretty.  e.g. \"1m17s\"
  %cwd                     working dir, tilde-abbreviated.  e.g. \"~/Projects/foo\"
  %cwdb, %cwd_base         basename only.  e.g. \"foo\"

EXAMPLES
  cc-statusline '%m:cyan  %e  %cu'
  cc-statusline '%[dim Model:] %m:cyan  %[dim Effort:] %e:bright_yellow' '%rl5  %peak'
  echo '<json>' | NOW=1776940000 cc-statusline '%rl5  %pk  %rl7'

See README for full examples and settings.json integration.
",
        ver = env!("CARGO_PKG_VERSION")
    );
}

struct Args {
    lines: Vec<String>,
    now: Option<i64>,
    no_color: bool,
    debug: bool,
    passthrough: bool,
    prune: Option<u64>,
}

/// Parse a retention duration: bare seconds or an `s`/`m`/`h`/`d`/`w` suffix
/// (e.g. `30d`, `12h`, `4w`, `604800`). Returns seconds.
fn parse_duration(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (num, mult): (&str, u64) = match s.as_bytes()[s.len() - 1] {
        b's' => (&s[..s.len() - 1], 1),
        b'm' => (&s[..s.len() - 1], 60),
        b'h' => (&s[..s.len() - 1], 3600),
        b'd' => (&s[..s.len() - 1], 86400),
        b'w' => (&s[..s.len() - 1], 604800),
        _ => (s, 1),
    };
    num.trim().parse::<u64>().ok()?.checked_mul(mult)
}

fn parse_args() -> Result<Args, i32> {
    let mut out = Args {
        lines: Vec::new(),
        now: None,
        no_color: false,
        debug: false,
        passthrough: false,
        prune: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print_help();
                return Err(0);
            }
            "-V" | "--version" => {
                println!("cc-statusline {}", env!("CARGO_PKG_VERSION"));
                return Err(0);
            }
            "--no-color" => out.no_color = true,
            "--debug" => out.debug = true,
            "--passthrough" => out.passthrough = true,
            "--now" => {
                if let Some(v) = it.next() {
                    out.now = v.parse().ok();
                }
            }
            s if s.starts_with("--now=") => {
                out.now = s[6..].parse().ok();
            }
            "--prune" => {
                if let Some(v) = it.next() {
                    out.prune = parse_duration(&v);
                }
            }
            s if s.starts_with("--prune=") => {
                out.prune = parse_duration(&s[8..]);
            }
            _ => out.lines.push(a),
        }
    }
    Ok(out)
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("mcp") {
        mcp::run();
        return;
    }

    let args = match parse_args() {
        Ok(a) => a,
        Err(code) => std::process::exit(code),
    };

    if args.no_color || std::env::var_os("NO_COLOR").is_some() {
        color::set_disabled(true);
    }

    let mut buf = String::new();
    let _ = std::io::stdin().read_to_string(&mut buf);

    if args.debug {
        eprintln!("cc-statusline stdin: {}", buf);
    }

    let input: input::Input = serde_json::from_str(&buf).unwrap_or_default();

    let now = args
        .now
        .or_else(|| std::env::var("NOW").ok().and_then(|s| s.parse().ok()))
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        });

    // Pruning is opt-in: `--prune <DURATION>`, else the legacy
    // CC_STATUSLINE_STATE_TTL_SECONDS env (seconds), else keep everything.
    let prune = args.prune.or_else(|| {
        std::env::var("CC_STATUSLINE_STATE_TTL_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
    });
    state::record(&input, now, prune);

    if args.passthrough {
        use std::io::Write;
        let _ = std::io::stdout().write_all(buf.as_bytes());
        return;
    }

    let context = tokens::Context::new(input, now);

    let raw_lines: Vec<String> = if args.lines.is_empty() {
        DEFAULT_LINES.iter().map(|s| s.to_string()).collect()
    } else {
        args.lines
    };

    let mut all: Vec<String> = Vec::new();
    for format_line in &raw_lines {
        let segs = format::parse(format_line);
        let resolve = |n: &str, s: &[String]| context.resolve(n, s);
        let rendered = format::render_segs(&segs, &resolve);
        for r in rendered {
            all.push(r);
        }
    }

    if all.len() > MAX_LINES {
        eprintln!(
            "cc-statusline: {} lines requested, truncated to {}",
            all.len(),
            MAX_LINES
        );
        all.truncate(MAX_LINES);
    }

    for (i, line) in all.iter().enumerate() {
        if i > 0 {
            println!();
        }
        print!("{}", line);
    }
    if !all.is_empty() {
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::parse_duration;

    #[test]
    fn parse_duration_suffixes_and_bare_seconds() {
        assert_eq!(parse_duration("30d"), Some(2_592_000));
        assert_eq!(parse_duration("12h"), Some(43_200));
        assert_eq!(parse_duration("4w"), Some(2_419_200));
        assert_eq!(parse_duration("90m"), Some(5_400));
        assert_eq!(parse_duration("45s"), Some(45));
        assert_eq!(parse_duration("604800"), Some(604_800));
    }

    #[test]
    fn parse_duration_rejects_junk() {
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("abc"), None);
        assert_eq!(parse_duration("d"), None);
        assert_eq!(parse_duration("1.5h"), None);
    }
}
