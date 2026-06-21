use std::path::Path;
use std::process::{Command, Stdio};

fn run(cwd: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub fn branch(cwd: &Path) -> Option<String> {
    run(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])
}

pub fn diff_counts(cwd: &Path) -> Option<(u64, u64)> {
    let out = Command::new("git")
        .args(["diff", "--numstat", "HEAD"])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let mut add = 0u64;
    let mut del = 0u64;
    for line in s.lines() {
        let mut it = line.split_whitespace();
        let a = it.next().unwrap_or("-");
        let d = it.next().unwrap_or("-");
        add += a.parse::<u64>().unwrap_or(0);
        del += d.parse::<u64>().unwrap_or(0);
    }
    Some((add, del))
}
