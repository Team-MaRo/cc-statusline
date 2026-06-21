# AGENTS.md

Guidance for AI agents (Claude Code and others) working in this repository.

## What this is

`cc-statusline` is a Rust CLI that Claude Code invokes as its `statusLine.command`. It reads Claude Code's statusline JSON from stdin, interpolates user-supplied format strings (passed as positional args), and prints up to 3 ANSI-styled lines to stdout. Zero args = no output by design.

## Common commands

```sh
cargo test                      # run tests in tests/render.rs
cargo build --release           # build optimized binary (target/release/cc-statusline)
cargo test <name>               # single test by substring match
cargo run --release --example gen_svg   # regenerate docs/output.svg preview

# Manual smoke test (binary reads JSON from stdin):
echo '{}' | ./target/release/cc-statusline '%m %cu'

# Deterministic time for peak/rate tests:
NOW=1776940000 echo '<json>' | cc-statusline '%rl5 %pk'

# Nix (Linux): build the OCI image, load + run it
nix build .#dockerImage && docker load < result
echo '{"model":{"display_name":"Opus"}}' | docker run -i --rm cc-statusline:latest '%m'

# Dev shell with the Rust toolchain (replaces the old compose `dev` service):
nix develop
```

### Windows from-source builds

Use the **MSVC** toolchain (the rustup default, `stable-x86_64-pc-windows-msvc`). The GNU toolchain is *not* viable here: a transitive dep (`chrono`/`windows-link`) makes it run `dlltool`, which needs an assembler (`as.exe`) that rustup's bundled MinGW doesn't ship.

1. Install the build tools + linker + Windows SDK (one-time):
   ```sh
   winget install --id Microsoft.VisualStudio.2022.BuildTools -e \
     --override "--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
   ```
2. Build from a shell where MSVC `cl`/`link` are on PATH — easiest is to source `vcvars64.bat` first (a plain PowerShell/cmd prompt won't have them):
   ```powershell
   cmd /c '"C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" && cargo build --release'
   ```
   Don't build from Git Bash: its `/usr/bin/link.exe` shadows the MSVC linker.
3. The statusline runs `cc-statusline` from PATH (`~/.cargo/bin`), which is a *separate* file from `target/release/`. After building, copy the new binary over it (or `cargo install --path .`) or the statusline keeps running the old version.

Some locked-down machines (Application Control policy) block running build scripts from inside the project's own `target/` dir — set `CARGO_TARGET_DIR` to a path under `%LOCALAPPDATA%` to work around it.

## Architecture

Binary entry: `src/main.rs`. Flow per invocation:

1. `parse_args` — each positional = one format line (max 3, `MAX_LINES` in main.rs). Flags: `--no-color`, `--now <unix>`, `--prune <dur>`, `--debug`, `-V`, `-h`. `NOW` and `NO_COLOR` env vars mirror flags.
2. Read stdin → `input::Input` (serde_json, `from_str(..).unwrap_or_default()` — missing/invalid JSON is non-fatal, tokens render empty).
3. `state::record(&input, now, prune)` persists per-session token + cost state to `$XDG_CACHE_HOME/cc-statusline/sessions.json` (see State below).
4. Build `tokens::Context` — pre-computes rate-limit parts (`rate::rate_part` for both 5h/300min and 7d/10080min windows), `settings::effort_level` (reads `~/.claude/settings.json`), git branch/diff via `git::branch` / `git::diff_counts` shelling out in `cwd`.
5. For each format line: `format::parse` → `Vec<Seg>` (Lit / Tok / Group / LineBreak), then `format::render_segs` with closure resolving token names via `context.resolve(name, styles)`.
6. Print joined output; trailing newline only if non-empty.

Module map:

- `format.rs` — format parser + renderer. Grammar: `%name`, `%name:style[:style…]`, `%[style content]` span, `%%` literal, `\n` line split. Unknown tokens render `%name` verbatim + stderr warning. Missing data collapses one surrounding space so separators don't double up.
- `tokens.rs` — `Context::resolve(name, styles)` is the single dispatch point from format names (short + long aliases, e.g. `%m`/`%model`) to rendered strings. Add new tokens here AND document them in `main.rs::print_help` AND `README.md`.
- `color.rs` — ANSI codes, global disable flag (`set_disabled`). Honors NO_COLOR.
- `bar.rs` — bar glyph rendering. 10 cells × 3 sub-levels (`░` → `▒` → `▓` → `█`); used drives the glyph, expected shown as green ghost at matching sub-level, over-curve red.
- `rate.rs` — rate-limit math: computes used%, bar, Δ% vs linear expected curve, Δt vs expected remaining time, reset countdown. Window length passed in minutes.
- `peak.rs` — peak-hours logic (5–11 AM US/Pacific, Mon–Fri). DST-aware via `chrono-tz` (`America::Los_Angeles`). The bundled IANA db is filtered to that one zone by `CHRONO_TZ_TIMEZONE_FILTER`, set with `force = true` in `.cargo/config.toml` (keeps the binary ~1.1 MB smaller than the full db; requires chrono-tz's `filter-by-regex` feature). Colors label red in-peak, green off-peak.
- `context.rs` — context-window bar/percent, including `_usable` variants scaled against the pre-auto-compact budget (0.98 for ≥500k models, 0.80 otherwise).
- `state.rs` — persistence. Per-session `cost_segments` + token `segments` (each survives Claude resetting the value mid-chat by starting a new segment); per-session `daily_cost` ledger keyed `YYYY-MM-DD` (local tz). `%cd`/`%cm`/`%ca` are computed by aggregating every session's `daily_cost`. Pruning is opt-in (`--prune <dur>` / `CC_STATUSLINE_STATE_TTL_SECONDS`); off by default.
- `git.rs` — `git rev-parse --abbrev-ref HEAD` + `git diff --numstat HEAD` in the input `cwd`.
- `settings.rs` — reads `effortLevel` from `~/.claude/settings.json`.
- `input.rs` — serde structs mirroring Claude Code's statusline JSON schema (model, workspace, cost, rate_limits, context, etc.). All fields `Option`; tolerate shape changes.
- `mcp.rs` — `cc-statusline mcp` stdio MCP server (read-only `context_usage`, `rate_limits`, `session_summary` tools over the persisted state file).
- `duration.rs` — ms → `14m12s` style.

Tests (`tests/render.rs`) drive the binary end-to-end by piping JSON + format args and asserting stdout — that's the contract. Keep JSON fixtures there when adding tokens. State/persistence has unit tests in `src/state.rs`; tests pin `TZ=UTC` and use temp `XDG_CACHE_HOME`.

## When adding a token

Four places must agree, or tests/docs drift:

1. `src/tokens.rs` — add a match arm in `Ctx::resolve` (both short + long alias).
2. `src/main.rs` — add a line under the relevant TOKENS section in `print_help`.
3. `README.md` — add a row to the tokens table.
4. `tests/render.rs` — add a case if it's non-trivial.

## Release

`.github/release-please-config.json` (+ `.github/release-please-manifest.json`) drives release-please. `release-artifacts.yml` builds versionless raw binaries on each published release (`cc-statusline-<os>-<arch>[-libc][.exe]`, no `.sha256` sidecars — GitHub exposes asset digests) and dispatches a bump to the Homebrew tap (separate repo `Team-MaRo/homebrew-tap`; needs `GH_PAT` secret, no-op without it). The tap's prebuilt-binary formula + bump workflow live in that repo. `flake.nix` packages it for Nix (CHRONO_TZ filter set explicitly in the derivation) and builds the OCI image (`packages.dockerImage`, Linux only) via `pkgs.dockerTools.streamLayeredImage` + `Team-MaRo/nix-utils`'s `fixOciImageHistory`. `docker.yml` builds + pushes multi-arch images (amd64/arm64, riscv64 opt-in) to Docker Hub on `*.*.*` release tags — no-op until `vars.IMAGE_NAME`/`vars.DOCKERHUB_USERNAME` + `secrets.DOCKERHUB_TOKEN` are set. No Dockerfile (the flake is the single source of truth). Workflows trigger on the `master` branch.
