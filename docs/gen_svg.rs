// Generates docs/output.svg by running several fixtures through the binary
// and converting ANSI-colored output to SVG. No external deps.
//
// Usage: cargo run --release --example gen_svg
// Output: docs/output.svg

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const FONT_SIZE: u32 = 15;
const LINE_H: u32 = 22;
const CHAR_W: f32 = 9.0; // Fira Code ~0.6em at 15px
const PAD_X: u32 = 20;
const PAD_Y: u32 = 20;
const CHROME_H: u32 = 32;

struct Scenario {
    title: &'static str,
    now: i64,
    json: &'static str,
}

const FMT_LINES: &[&str] = &[
    "%m:cyan  %e:yellow  %cu",
    "%b:green %d:yellow  %rl5  %pk",
    "%cwd:dim  %c  %rl7",
];

fn fixtures() -> Vec<Scenario> {
    // Anchored 2024-06-15 12:00 UTC = 1718452800 (off-peak) / 15:00 = 1718463600 (peak)
    vec![
        Scenario {
            title: "1. under-curve on both (fresh window)",
            now: 1718452800,
            json: r#"{"session_id":"11111111-2222-3333-4444-555555555555","cwd":"/home/user/projects/myapp","model":{"display_name":"Claude Sonnet 4.6","id":"claude-sonnet-4-6"},"version":"2.1.118","output_style":{"name":"default"},"cost":{"total_cost_usd":0.12},"context_window":{"context_window_size":200000,"used_percentage":28},"rate_limits":{"five_hour":{"used_percentage":18,"resets_at":1718467200},"seven_day":{"used_percentage":8,"resets_at":1718971200}}}"#,
        },
        Scenario {
            title: "2. over-curve on 5h (40% used, ~0% elapsed)",
            now: 1718452800,
            json: r#"{"cwd":"/home/user/myapp","model":{"display_name":"Claude Sonnet 4.6"},"cost":{"total_cost_usd":1.84},"context_window":{"context_window_size":200000,"used_percentage":55},"rate_limits":{"five_hour":{"used_percentage":40,"resets_at":1718467200},"seven_day":{"used_percentage":5,"resets_at":1718971200}}}"#,
        },
        Scenario {
            title: "3. over-curve on both (heavy use)",
            now: 1718452800,
            json: r#"{"cwd":"/home/user/prod-service","model":{"display_name":"Claude Opus 4.7 (1M context)"},"cost":{"total_cost_usd":14.22},"context_window":{"context_window_size":1000000,"used_percentage":85},"rate_limits":{"five_hour":{"used_percentage":80,"resets_at":1718453700},"seven_day":{"used_percentage":60,"resets_at":1718712000}}}"#,
        },
        Scenario {
            title: "4. no resets_at available",
            now: 1718452800,
            json: r#"{"cwd":"/home/user/tool","model":{"display_name":"Claude Haiku 4.5"},"cost":{"total_cost_usd":0.02},"context_window":{"context_window_size":200000,"used_percentage":10},"rate_limits":{"five_hour":{"used_percentage":5},"seven_day":{"used_percentage":2}}}"#,
        },
        Scenario {
            title: "5. peak hours (15:00 UTC, inside 13–19 window)",
            now: 1718463600,
            json: r#"{"cwd":"/home/user/dashboards","model":{"display_name":"Claude Sonnet 4.6"},"cost":{"total_cost_usd":0.71},"context_window":{"context_window_size":200000,"used_percentage":72},"rate_limits":{"five_hour":{"used_percentage":18,"resets_at":1718478000},"seven_day":{"used_percentage":8,"resets_at":1718982000}}}"#,
        },
        Scenario {
            title: "6. off-peak hours (08:00 UTC)",
            now: 1718438400,
            json: r#"{"cwd":"/home/user/scratch","model":{"display_name":"Claude Sonnet 4.6"},"cost":{"total_cost_usd":0.04},"context_window":{"context_window_size":200000,"used_percentage":40},"rate_limits":{"five_hour":{"used_percentage":18,"resets_at":1718452800},"seven_day":{"used_percentage":8,"resets_at":1718956800}}}"#,
        },
        Scenario {
            title: "7. fresh session, null fields (no ctx/rl yet)",
            now: 1718452800,
            json: r#"{"cwd":"/home/user/projects","model":{"display_name":"Claude Opus 4.7 (1M context)"},"cost":{"total_cost_usd":0},"context_window":{"context_window_size":1000000,"used_percentage":null}}"#,
        },
    ]
}

fn binary_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("target");
    p.push("release");
    p.push("cc-statusline");
    p
}

fn run_binary(scenario: &Scenario) -> String {
    let exe = binary_path();
    let mut cmd = Command::new(exe);
    cmd.env("NOW", scenario.now.to_string())
        .env_remove("NO_COLOR")
        .env_remove("CC_VIM_MODE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for l in FMT_LINES {
        cmd.arg(l);
    }
    let mut child = cmd.spawn().expect("spawn binary");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(scenario.json.as_bytes())
        .unwrap();
    let out = child.wait_with_output().expect("binary output");
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[derive(Clone, Default, Debug)]
struct Style {
    fg: Option<&'static str>,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
}

fn color_for(code: u32) -> Option<&'static str> {
    Some(match code {
        30 => "#2e3436",
        31 => "#cc0000",
        32 => "#4e9a06",
        33 => "#c4a000",
        34 => "#3465a4",
        35 => "#75507b",
        36 => "#06989a",
        37 => "#d3d7cf",
        90 => "#555753",
        91 => "#ef2929",
        92 => "#8ae234",
        93 => "#fce94f",
        94 => "#729fcf",
        95 => "#ad7fa8",
        96 => "#34e2e2",
        97 => "#eeeeec",
        _ => return None,
    })
}

#[derive(Debug)]
struct Span {
    text: String,
    style: Style,
}

fn parse_ansi(line: &str) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    let mut cur = Style::default();
    let mut buf = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            if !buf.is_empty() {
                out.push(Span {
                    text: std::mem::take(&mut buf),
                    style: cur.clone(),
                });
            }
            chars.next();
            let mut seq = String::new();
            for nc in chars.by_ref() {
                if nc == 'm' {
                    break;
                }
                seq.push(nc);
            }
            let parts: Vec<u32> = if seq.is_empty() {
                vec![0]
            } else {
                seq.split(';').filter_map(|p| p.parse().ok()).collect()
            };
            for p in parts {
                match p {
                    0 => cur = Style::default(),
                    1 => cur.bold = true,
                    2 => cur.dim = true,
                    3 => cur.italic = true,
                    4 => cur.underline = true,
                    22 => {
                        cur.bold = false;
                        cur.dim = false;
                    }
                    23 => cur.italic = false,
                    24 => cur.underline = false,
                    39 => cur.fg = None,
                    n if (30..=37).contains(&n) || (90..=97).contains(&n) => {
                        cur.fg = color_for(n);
                    }
                    _ => {}
                }
            }
        } else {
            buf.push(c);
        }
    }
    if !buf.is_empty() {
        out.push(Span {
            text: buf,
            style: cur,
        });
    }
    out
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn span_to_text(span: &Span, x: f32, y: u32) -> String {
    let st = &span.style;
    let mut style = String::new();
    if let Some(fg) = st.fg {
        style.push_str(&format!("fill:{};", fg));
    } else {
        style.push_str("fill:#eeeeec;");
    }
    if st.bold {
        style.push_str("font-weight:bold;");
    }
    if st.dim {
        style.push_str("opacity:0.55;");
    }
    if st.italic {
        style.push_str("font-style:italic;");
    }
    if st.underline {
        style.push_str("text-decoration:underline;");
    }
    format!(
        "<text x=\"{:.1}\" y=\"{}\" xml:space=\"preserve\" style=\"{}\">{}</text>",
        x,
        y,
        style,
        xml_escape(&span.text)
    )
}

struct Line {
    kind: LineKind,
    spans: Vec<Span>,
}

enum LineKind {
    Header,
    Body,
    Blank,
}

fn build_lines(scenarios: &[Scenario]) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    for (i, s) in scenarios.iter().enumerate() {
        if i > 0 {
            lines.push(Line {
                kind: LineKind::Blank,
                spans: vec![],
            });
        }
        lines.push(Line {
            kind: LineKind::Header,
            spans: vec![Span {
                text: format!("=== {} ===", s.title),
                style: Style {
                    fg: Some("#8ae234"),
                    bold: true,
                    ..Default::default()
                },
            }],
        });
        let raw = run_binary(s);
        for body in raw.trim_end_matches('\n').split('\n') {
            lines.push(Line {
                kind: LineKind::Body,
                spans: parse_ansi(body),
            });
        }
    }
    lines
}

fn visible_len(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.text.chars().count()).sum()
}

fn main() {
    let scenarios = fixtures();
    let lines = build_lines(&scenarios);

    let max_cols = lines
        .iter()
        .map(|l| visible_len(&l.spans))
        .max()
        .unwrap_or(80);
    let width = (PAD_X * 2) + ((max_cols as f32 * CHAR_W).ceil() as u32) + 20;
    let height = CHROME_H + PAD_Y * 2 + LINE_H * lines.len() as u32;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\">\n",
        w = width,
        h = height
    ));
    svg.push_str(
        "<style>\
        .term{fill:#eeeeec;font-family:'Fira Code','JetBrains Mono','Menlo','Consolas',monospace;font-size:15px;}\
        </style>\n",
    );
    // Window chrome
    svg.push_str(&format!(
        "<rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" rx=\"8\" fill=\"#1d1f21\"/>\n",
        width, height
    ));
    svg.push_str(&format!(
        "<rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" rx=\"8\" fill=\"#2d2f31\"/>\n",
        width, CHROME_H
    ));
    svg.push_str(&format!(
        "<rect x=\"0\" y=\"{}\" width=\"{}\" height=\"1\" fill=\"#000\"/>\n",
        CHROME_H, width
    ));
    svg.push_str("<circle cx=\"16\" cy=\"16\" r=\"6\" fill=\"#ff5f56\"/>\n");
    svg.push_str("<circle cx=\"36\" cy=\"16\" r=\"6\" fill=\"#ffbd2e\"/>\n");
    svg.push_str("<circle cx=\"56\" cy=\"16\" r=\"6\" fill=\"#27c93f\"/>\n");
    svg.push_str(&format!(
        "<text x=\"{}\" y=\"20\" text-anchor=\"middle\" style=\"fill:#888;font-family:system-ui,sans-serif;font-size:12px;\">cc-statusline — preview</text>\n",
        width / 2
    ));

    // Body text
    svg.push_str("<g class=\"term\">\n");
    let mut y = CHROME_H + PAD_Y + FONT_SIZE;
    for line in &lines {
        match line.kind {
            LineKind::Blank => {}
            LineKind::Header | LineKind::Body => {
                let mut col = 0f32;
                for span in &line.spans {
                    let x = PAD_X as f32 + col * CHAR_W;
                    svg.push_str(&span_to_text(span, x, y));
                    svg.push('\n');
                    col += span.text.chars().count() as f32;
                }
            }
        }
        y += LINE_H;
    }
    svg.push_str("</g>\n</svg>\n");

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("docs");
    fs::create_dir_all(&path).unwrap();
    path.push("output.svg");
    fs::write(&path, svg).unwrap();
    println!("wrote {}", path.display());
}
