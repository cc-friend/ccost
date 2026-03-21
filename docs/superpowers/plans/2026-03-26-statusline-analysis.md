# cctokens sl — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `cctokens sl` subcommand that analyzes `~/.claude/statusline.jsonl` for rate limit tracking, session summaries, 5h-window budget estimation, and cost cross-comparison.

**Architecture:** New `src/sl/` module with its own types, parser (segment-aware), aggregator, and formatter. Shared utilities (clipboard, date parsing, terminal width) extracted from `main.rs` into `src/utils.rs`. Chart rendering reused via a new public `render_chart_raw` function in `chart.rs`. CLI integrated via clap subcommand.

**Tech Stack:** Rust, serde_json (parsing), chrono/chrono-tz (time), clap (CLI subcommand), existing braille chart engine.

**Spec:** `docs/superpowers/specs/2026-03-26-statusline-analysis-design.md`

---

### Task 1: Extract shared utilities from main.rs into src/utils.rs

**Files:**
- Create: `src/utils.rs`
- Modify: `src/main.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create src/utils.rs with functions extracted from main.rs**

Move these functions verbatim from `main.rs` into `src/utils.rs`, making them `pub`:

```rust
// src/utils.rs
use std::io::Write;
use std::process;

use chrono::NaiveDateTime;

/// Parse a datetime string: replace space with T, if 10 chars add T00:00:00.
pub fn parse_datetime(s: &str) -> Option<NaiveDateTime> {
    let normalized = s.replace(' ', "T");
    let full = if normalized.len() == 10 {
        format!("{}T00:00:00", normalized)
    } else if normalized.len() == 16 {
        format!("{}:00", normalized)
    } else {
        normalized
    };
    NaiveDateTime::parse_from_str(&full, "%Y-%m-%dT%H:%M:%S").ok()
}

/// Compute effective (from, to) date strings from the various date options.
pub fn compute_date_range(
    from_val: Option<String>,
    to_val: Option<String>,
    h5from_val: Option<String>,
    h5to_val: Option<String>,
    w1from_val: Option<String>,
    w1to_val: Option<String>,
) -> (Option<String>, Option<String>) {
    if let Some(ref val) = h5from_val {
        if let Some(dt) = parse_datetime(val) {
            let end = dt + chrono::Duration::hours(5);
            return (
                Some(dt.format("%Y-%m-%dT%H:%M:%S").to_string()),
                Some(end.format("%Y-%m-%dT%H:%M:%S").to_string()),
            );
        }
        return (from_val, to_val);
    }
    if let Some(ref val) = h5to_val {
        if let Some(dt) = parse_datetime(val) {
            let start = dt - chrono::Duration::hours(5);
            return (
                Some(start.format("%Y-%m-%dT%H:%M:%S").to_string()),
                Some(dt.format("%Y-%m-%dT%H:%M:%S").to_string()),
            );
        }
        return (from_val, to_val);
    }
    if let Some(ref val) = w1from_val {
        if let Some(dt) = parse_datetime(val) {
            let end = dt + chrono::Duration::days(7);
            return (
                Some(dt.format("%Y-%m-%dT%H:%M:%S").to_string()),
                Some(end.format("%Y-%m-%dT%H:%M:%S").to_string()),
            );
        }
        return (from_val, to_val);
    }
    if let Some(ref val) = w1to_val {
        if let Some(dt) = parse_datetime(val) {
            let start = dt - chrono::Duration::days(7);
            return (
                Some(start.format("%Y-%m-%dT%H:%M:%S").to_string()),
                Some(dt.format("%Y-%m-%dT%H:%M:%S").to_string()),
            );
        }
        return (from_val, to_val);
    }
    (from_val, to_val)
}

pub fn term_width() -> usize {
    use std::io::IsTerminal;
    if let Ok(cols) = std::env::var("COLUMNS") {
        if let Ok(n) = cols.parse::<usize>() {
            if n > 0 {
                return n;
            }
        }
    }
    if !std::io::stdout().is_terminal() {
        return 200;
    }
    if let Some((terminal_size::Width(w), _)) = terminal_size::terminal_size() {
        if w > 0 {
            return w as usize;
        }
    }
    80
}

/// Base64-encode bytes (standard alphabet, with padding).
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn osc52_copy(text: &str) -> Result<(), String> {
    let encoded = base64_encode(text.as_bytes());
    eprint!("\x1b]52;c;{}\x07", encoded);
    Ok(())
}

pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let result = if cfg!(target_os = "macos") {
        process::Command::new("pbcopy")
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::null())
            .stderr(process::Stdio::null())
            .spawn()
    } else if cfg!(target_os = "windows") {
        process::Command::new("clip")
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::null())
            .stderr(process::Stdio::null())
            .spawn()
    } else {
        process::Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::null())
            .stderr(process::Stdio::null())
            .spawn()
            .or_else(|_| {
                process::Command::new("xsel")
                    .arg("--clipboard")
                    .stdin(process::Stdio::piped())
                    .stdout(process::Stdio::null())
                    .stderr(process::Stdio::null())
                    .spawn()
            })
            .or_else(|_| {
                process::Command::new("wl-copy")
                    .stdin(process::Stdio::piped())
                    .stdout(process::Stdio::null())
                    .stderr(process::Stdio::null())
                    .spawn()
            })
    };

    match result {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(text.as_bytes())
                    .map_err(|e| format!("Failed to write to clipboard: {}", e))?;
                drop(stdin);
            }
            let status = child
                .wait()
                .map_err(|e| format!("Clipboard process failed: {}", e))?;
            if status.success() {
                Ok(())
            } else {
                osc52_copy(text)
            }
        }
        Err(_) => osc52_copy(text),
    }
}

/// Extension for an output format string.
pub fn ext_for_format(fmt: &str) -> &str {
    match fmt {
        "markdown" => "md",
        "json" => "json",
        "html" => "html",
        "txt" => "txt",
        "csv" => "csv",
        "tsv" => "tsv",
        _ => fmt,
    }
}
```

- [ ] **Step 2: Update src/lib.rs to export utils**

Add this line:

```rust
pub mod utils;
```

- [ ] **Step 3: Update main.rs to use utils instead of local definitions**

Replace all local definitions of `parse_datetime`, `compute_date_range`, `term_width`, `copy_to_clipboard`, `osc52_copy`, `base64_encode`, and the inline `ext_for_format` with imports:

```rust
use cctokens::utils::{parse_datetime, compute_date_range, term_width, copy_to_clipboard, ext_for_format};
```

Remove the local function definitions from `main.rs`. Remove the `use std::io::{IsTerminal, Write};` line from `main.rs` (now only needed in `utils.rs`).

- [ ] **Step 4: Run tests to verify refactor didn't break anything**

Run: `cargo test`
Expected: All 169 existing tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/utils.rs src/lib.rs src/main.rs
git commit -m "refactor: extract shared utilities from main.rs into utils.rs"
```

---

### Task 2: Expose render_chart_raw in chart.rs

**Files:**
- Modify: `src/formatters/chart.rs`

The existing `render_chart_core` is private. We need a public entry point that accepts raw `(label, value)` pairs, for sl charts.

- [ ] **Step 1: Add public render_chart_raw function**

Add after the existing `render_chart_from_records` function:

```rust
/// Render a braille chart from raw key-value pairs.
/// Used by sl module for rate-limit and cost charts.
pub fn render_chart_raw(
    keys: &[String],
    values: &[f64],
    title: &str,
    y_label_fn: fn(f64) -> String,
    width: Option<usize>,
    height: Option<usize>,
) -> String {
    if keys.is_empty() || values.is_empty() {
        return "No data to chart.".to_string();
    }

    let max_val = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min_val = 0.0_f64;

    let width = width.unwrap_or(80);
    let braille_rows = height.unwrap_or(15);

    let num_y_labels = braille_rows.min(6).max(2);
    let mut y_positions: Vec<usize> = Vec::new();
    for i in 0..num_y_labels {
        let pos = if num_y_labels == 1 {
            0
        } else {
            i * (braille_rows - 1) / (num_y_labels - 1)
        };
        y_positions.push(pos);
    }
    y_positions.sort();
    y_positions.dedup();

    let y_labels: Vec<(usize, String)> = y_positions
        .iter()
        .map(|&pos| {
            let val = if braille_rows <= 1 {
                max_val
            } else {
                max_val - (max_val - min_val) * pos as f64 / (braille_rows - 1) as f64
            };
            (pos, y_label_fn(val))
        })
        .collect();

    let y_label_width = y_labels.iter().map(|(_, l)| l.len()).max().unwrap_or(0);

    let chart_cols = if width > y_label_width + 3 {
        width - y_label_width - 3
    } else {
        10
    };
    let grid_width = chart_cols * 2;
    let grid_height = braille_rows * 4;

    let data_points: Vec<(usize, usize)> = values
        .iter()
        .enumerate()
        .map(|(i, &val)| {
            let x = if values.len() <= 1 {
                0
            } else {
                i * grid_width.saturating_sub(1) / (values.len() - 1)
            };
            let y = if max_val <= min_val {
                grid_height.saturating_sub(1)
            } else {
                let normalized = (val - min_val) / (max_val - min_val);
                let y_pos =
                    ((1.0 - normalized) * (grid_height.saturating_sub(1)) as f64).round() as usize;
                y_pos.min(grid_height.saturating_sub(1))
            };
            (x, y)
        })
        .collect();

    let mut grid = vec![vec![0u8; grid_width]; grid_height];

    for i in 0..data_points.len() {
        let (x0, y0) = data_points[i];
        if x0 < grid_width && y0 < grid_height {
            grid[y0][x0] = 1;
        }
        if i + 1 < data_points.len() {
            let (x1, y1) = data_points[i + 1];
            bresenham_line(&mut grid, x0, y0, x1, y1, grid_width, grid_height);
        }
    }

    let mut braille_chars: Vec<Vec<char>> = vec![vec![' '; chart_cols]; braille_rows];
    for br in 0..braille_rows {
        for bc in 0..chart_cols {
            let mut pattern: u8 = 0;
            for dr in 0..4 {
                for dc in 0..2 {
                    let gy = br * 4 + dr;
                    let gx = bc * 2 + dc;
                    if gy < grid_height && gx < grid_width && grid[gy][gx] != 0 {
                        pattern |= BRAILLE_DOT_MAP[dr][dc];
                    }
                }
            }
            if pattern != 0 {
                braille_chars[br][bc] =
                    char::from_u32(BRAILLE_BASE + pattern as u32).unwrap_or(' ');
            }
        }
    }

    let mut output = String::new();
    output.push_str(title);
    output.push('\n');
    output.push('\n');

    use std::collections::BTreeMap;
    let y_label_map: BTreeMap<usize, &str> = y_labels
        .iter()
        .map(|(pos, label)| (*pos, label.as_str()))
        .collect();

    for br in 0..braille_rows {
        let label = if let Some(label) = y_label_map.get(&br) {
            format!("{:>width$}", label, width = y_label_width)
        } else {
            " ".repeat(y_label_width)
        };
        output.push_str(&label);
        output.push_str(" \u{2524}");
        let row_str: String = braille_chars[br].iter().collect();
        output.push_str(&row_str);
        output.push('\n');
    }

    output.push_str(&" ".repeat(y_label_width + 1));
    output.push('\u{2514}');
    output.push_str(&"\u{2500}".repeat(chart_cols));
    output.push('\n');

    // X-axis labels - use keys directly, auto-distribute
    let mut label_line = vec![' '; chart_cols];
    if !keys.is_empty() {
        let num_labels = keys.len().min(chart_cols / 8).max(2).min(keys.len());
        let indices: Vec<usize> = if num_labels <= 1 {
            vec![0]
        } else {
            (0..num_labels)
                .map(|i| i * (keys.len() - 1) / (num_labels - 1))
                .collect()
        };
        for &idx in &indices {
            let label = &keys[idx];
            let pos = if keys.len() <= 1 {
                0
            } else {
                idx * chart_cols.saturating_sub(1) / (keys.len() - 1)
            };
            let start = pos.min(chart_cols);
            for (j, ch) in label.chars().enumerate() {
                let col = start + j;
                if col < chart_cols {
                    label_line[col] = ch;
                }
            }
        }
    }

    output.push_str(&" ".repeat(y_label_width + 2));
    let label_str: String = label_line.iter().collect();
    output.push_str(label_str.trim_end());
    output.push('\n');

    output
}

/// Format a y-axis percentage label (0-100%).
pub fn y_label_percent(val: f64) -> String {
    format!("{}%", val.round() as i64)
}
```

Also make `y_label_cost` public (add `pub` keyword):

```rust
pub fn y_label_cost(val: f64) -> String {
```

- [ ] **Step 2: Run tests**

Run: `cargo test`
Expected: All tests pass (new function is additive).

- [ ] **Step 3: Commit**

```bash
git add src/formatters/chart.rs
git commit -m "feat: expose render_chart_raw for sl chart reuse"
```

---

### Task 3: Create sl types

**Files:**
- Create: `src/sl/mod.rs`
- Create: `src/sl/types.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create src/sl/types.rs**

```rust
use chrono::{DateTime, Utc};
use serde::Serialize;

/// A single parsed statusline snapshot.
#[derive(Debug, Clone)]
pub struct SlRecord {
    pub ts: DateTime<Utc>,
    pub session_id: String,
    pub project: String,
    pub model_id: String,
    pub model_name: String,
    pub version: String,
    pub cost_usd: f64,
    pub duration_ms: u64,
    pub api_duration_ms: u64,
    pub lines_added: u64,
    pub lines_removed: u64,
    pub context_pct: Option<u8>,
    pub context_size: u64,
    pub five_hour_pct: Option<u8>,
    pub five_hour_resets_at: Option<DateTime<Utc>>,
    pub seven_day_pct: Option<u8>,
    pub seven_day_resets_at: Option<DateTime<Utc>>,
}

/// Session summary after segment-aware aggregation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlSessionSummary {
    pub session_id: String,
    pub project: String,
    pub model_name: String,
    pub version: String,
    pub segments: u32,
    pub total_cost: f64,
    pub total_duration_ms: u64,
    pub total_api_duration_ms: u64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub max_context_pct: Option<u8>,
    pub first_ts: DateTime<Utc>,
    pub last_ts: DateTime<Utc>,
    pub last_five_hour_pct: Option<u8>,
    pub last_seven_day_pct: Option<u8>,
}

/// Rate limit timeline entry (deduplicated — only when pct changes).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlRateLimitEntry {
    pub ts: DateTime<Utc>,
    pub session_id: String,
    pub five_hour_pct: u8,
    pub five_hour_resets_at: DateTime<Utc>,
    pub seven_day_pct: u8,
    pub seven_day_resets_at: DateTime<Utc>,
}

/// 5-hour window summary.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlWindowSummary {
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub peak_five_hour_pct: u8,
    pub sessions: u32,
    pub total_cost: f64,
    pub est_budget: Option<f64>,
}

/// Project summary (aggregated from session summaries).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlProjectSummary {
    pub project: String,
    pub total_cost: f64,
    pub total_duration_ms: u64,
    pub total_api_duration_ms: u64,
    pub session_count: u32,
}

/// Day summary (aggregated from session summaries).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlDaySummary {
    pub date: String,
    pub total_cost: f64,
    pub session_count: u32,
    pub peak_five_hour_pct: Option<u8>,
    pub peak_seven_day_pct: Option<u8>,
}

/// Options for loading statusline records.
#[derive(Debug, Clone, Default)]
pub struct SlLoadOptions {
    pub file: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub tz: Option<String>,
    pub session: Option<String>,
    pub project: Option<String>,
    pub model: Option<String>,
}

/// Cost diff entry for cross-comparison.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlCostDiff {
    pub session_id: String,
    pub sl_cost: f64,
    pub litellm_cost: Option<f64>,
    pub diff: Option<f64>,
    pub diff_pct: Option<f64>,
}

/// Which sl view mode is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlViewMode {
    RateLimit,
    Session,
    Project,
    Day,
    Window,
}

/// Which sl chart mode is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlChartMode {
    FiveHour,
    SevenDay,
    Cost,
}
```

- [ ] **Step 2: Create src/sl/mod.rs**

```rust
pub mod types;
pub mod parser;
pub mod aggregator;
pub mod formatter;

pub use types::*;
pub use parser::*;
pub use aggregator::*;
```

- [ ] **Step 3: Add sl module to lib.rs**

Add this line to `src/lib.rs`:

```rust
pub mod sl;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check`
Expected: Compile error about missing `parser`, `aggregator`, `formatter` modules (will be created in subsequent tasks). This is expected — create stub files:

Create `src/sl/parser.rs`:
```rust
// Parser implementation in Task 4
```

Create `src/sl/aggregator.rs`:
```rust
// Aggregator implementation in Task 5
```

Create `src/sl/formatter.rs`:
```rust
// Formatter implementation in Task 6
```

Run: `cargo check`
Expected: Compiles with no errors (stubs have no code).

- [ ] **Step 5: Commit**

```bash
git add src/sl/ src/lib.rs
git commit -m "feat(sl): add statusline types and module structure"
```

---

### Task 4: Implement sl parser

**Files:**
- Modify: `src/sl/parser.rs`
- Create: `tests/sl_parser.rs`

- [ ] **Step 1: Write tests for parser**

Create `tests/sl_parser.rs`:

```rust
use chrono::{TimeZone, Utc};
use cctokens::sl::types::*;
use cctokens::sl::parser::*;
use std::io::Write;
use tempfile::NamedTempFile;

fn write_jsonl(lines: &[&str]) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    for line in lines {
        writeln!(f, "{}", line).unwrap();
    }
    f
}

fn make_line(ts: i64, session: &str, cost: f64, dur_ms: u64, api_ms: u64, five_h: Option<u8>, seven_d: Option<u8>, resets_5h: Option<i64>, resets_7d: Option<i64>) -> String {
    let rl = if let (Some(fh), Some(sd), Some(r5), Some(r7)) = (five_h, seven_d, resets_5h, resets_7d) {
        format!(r#","rate_limits":{{"five_hour":{{"used_percentage":{},"resets_at":{}}},"seven_day":{{"used_percentage":{},"resets_at":{}}}}}"#, fh, r5, sd, r7)
    } else {
        String::new()
    };
    format!(
        r#"{{"ts":{},"data":{{"session_id":"{}","workspace":{{"project_dir":"/home/user/proj","current_dir":"/home/user/proj","added_dirs":[]}},"model":{{"id":"claude-opus-4-6[1m]","display_name":"Opus 4.6"}},"version":"2.1.84","cost":{{"total_cost_usd":{},"total_duration_ms":{},"total_api_duration_ms":{},"total_lines_added":0,"total_lines_removed":0}},"context_window":{{"total_input_tokens":100,"total_output_tokens":50,"context_window_size":1000000,"current_usage":null,"used_percentage":2,"remaining_percentage":98}},"exceeds_200k_tokens":false{}}}}}"#,
        ts, session, cost, dur_ms, api_ms, rl
    )
}

#[test]
fn test_parse_basic_record() {
    let line = make_line(1774481258, "sess-aaa", 0.5, 1000, 500, Some(5), Some(63), Some(1774497600), Some(1774605600));
    let f = write_jsonl(&[&line]);
    let opts = SlLoadOptions::default();
    let (records, skipped) = load_sl_records(f.path().to_str().unwrap(), &opts);
    assert_eq!(records.len(), 1);
    assert_eq!(skipped, 0);
    assert_eq!(records[0].session_id, "sess-aaa");
    assert!((records[0].cost_usd - 0.5).abs() < 0.001);
    assert_eq!(records[0].five_hour_pct, Some(5));
    assert_eq!(records[0].seven_day_pct, Some(63));
}

#[test]
fn test_parse_no_rate_limits() {
    let line = make_line(1774481258, "sess-bbb", 0.0, 100, 0, None, None, None, None);
    let f = write_jsonl(&[&line]);
    let opts = SlLoadOptions::default();
    let (records, _) = load_sl_records(f.path().to_str().unwrap(), &opts);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].five_hour_pct, None);
}

#[test]
fn test_skip_malformed_lines() {
    let good = make_line(1774481258, "sess-ccc", 0.1, 100, 50, None, None, None, None);
    let f = write_jsonl(&[&good, "not json at all", "{\"ts\":123}", &good]);
    let opts = SlLoadOptions::default();
    let (records, skipped) = load_sl_records(f.path().to_str().unwrap(), &opts);
    assert_eq!(records.len(), 2);
    assert_eq!(skipped, 2);
}

#[test]
fn test_filter_by_session() {
    let a = make_line(1774481258, "sess-aaa", 0.1, 100, 50, None, None, None, None);
    let b = make_line(1774481259, "sess-bbb", 0.2, 200, 100, None, None, None, None);
    let f = write_jsonl(&[&a, &b]);
    let opts = SlLoadOptions {
        session: Some("aaa".to_string()),
        ..Default::default()
    };
    let (records, _) = load_sl_records(f.path().to_str().unwrap(), &opts);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].session_id, "sess-aaa");
}

#[test]
fn test_filter_by_project() {
    let line = make_line(1774481258, "sess-aaa", 0.1, 100, 50, None, None, None, None);
    let f = write_jsonl(&[&line]);
    let opts = SlLoadOptions {
        project: Some("nonexistent".to_string()),
        ..Default::default()
    };
    let (records, _) = load_sl_records(f.path().to_str().unwrap(), &opts);
    assert_eq!(records.len(), 0);
}

#[test]
fn test_filter_by_time_range() {
    let a = make_line(1774481000, "sess-aaa", 0.1, 100, 50, None, None, None, None);
    let b = make_line(1774482000, "sess-aaa", 0.2, 200, 100, None, None, None, None);
    let c = make_line(1774483000, "sess-aaa", 0.3, 300, 150, None, None, None, None);
    let f = write_jsonl(&[&a, &b, &c]);
    // ts 1774482000 = 2026-03-25T23:40:00Z
    let opts = SlLoadOptions {
        from: Some("2026-03-25T23:35:00".to_string()),
        to: Some("2026-03-25T23:45:00".to_string()),
        tz: Some("UTC".to_string()),
        ..Default::default()
    };
    let (records, _) = load_sl_records(f.path().to_str().unwrap(), &opts);
    assert_eq!(records.len(), 1);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test sl_parser`
Expected: Compile error — `load_sl_records` not defined yet.

- [ ] **Step 3: Implement the parser**

Replace `src/sl/parser.rs` contents:

```rust
use std::fs;
use std::path::Path;

use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use serde_json::Value;

use super::types::{SlLoadOptions, SlRecord};

/// Parse a unix timestamp (seconds) into DateTime<Utc>.
fn ts_to_datetime(ts: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_opt(ts, 0).single()
}

/// Parse the user's tz string into a helper that converts NaiveDateTime to DateTime<Utc>.
fn parse_filter_datetime(s: &str, tz: Option<&str>) -> Option<DateTime<Utc>> {
    let normalized = s.replace(' ', "T");
    let full = if normalized.len() == 10 {
        format!("{}T00:00:00", normalized)
    } else if normalized.len() == 16 {
        format!("{}:00", normalized)
    } else {
        normalized
    };
    let naive = NaiveDateTime::parse_from_str(&full, "%Y-%m-%dT%H:%M:%S").ok()?;

    match tz {
        Some("UTC") => Some(naive.and_utc()),
        Some(tz_str) if tz_str.starts_with('+') || tz_str.starts_with('-') => {
            let sign: i32 = if tz_str.starts_with('+') { 1 } else { -1 };
            let rest = &tz_str[1..];
            let parts: Vec<&str> = rest.split(':').collect();
            if parts.len() == 2 {
                let hours: i32 = parts[0].parse().ok()?;
                let minutes: i32 = parts[1].parse().ok()?;
                let offset = chrono::FixedOffset::east_opt(sign * (hours * 3600 + minutes * 60))?;
                Some(naive.and_local_timezone(offset).single()?.with_timezone(&Utc))
            } else {
                None
            }
        }
        Some(tz_str) => {
            if let Ok(tz_parsed) = tz_str.parse::<chrono_tz::Tz>() {
                tz_parsed
                    .from_local_datetime(&naive)
                    .single()
                    .map(|dt| dt.with_timezone(&Utc))
            } else {
                Some(naive.and_local_timezone(Local).single()?.with_timezone(&Utc))
            }
        }
        None => Some(naive.and_local_timezone(Local).single()?.with_timezone(&Utc)),
    }
}

/// Parse a single JSONL line into an SlRecord.
fn parse_line(line: &str) -> Option<SlRecord> {
    let v: Value = serde_json::from_str(line).ok()?;
    let ts_val = v.get("ts")?.as_i64()?;
    let ts = ts_to_datetime(ts_val)?;
    let data = v.get("data")?;

    let session_id = data.get("session_id")?.as_str()?.to_string();
    let project = data
        .pointer("/workspace/project_dir")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let model_id = data
        .pointer("/model/id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let model_name = data
        .pointer("/model/display_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let version = data
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let cost = data.pointer("/cost");
    let cost_usd = cost
        .and_then(|c| c.get("total_cost_usd"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let duration_ms = cost
        .and_then(|c| c.get("total_duration_ms"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let api_duration_ms = cost
        .and_then(|c| c.get("total_api_duration_ms"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let lines_added = cost
        .and_then(|c| c.get("total_lines_added"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let lines_removed = cost
        .and_then(|c| c.get("total_lines_removed"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let cw = data.pointer("/context_window");
    let context_pct = cw
        .and_then(|c| c.get("used_percentage"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u8);
    let context_size = cw
        .and_then(|c| c.get("context_window_size"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let rl = data.pointer("/rate_limits");
    let five_hour_pct = rl
        .and_then(|r| r.pointer("/five_hour/used_percentage"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u8);
    let five_hour_resets_at = rl
        .and_then(|r| r.pointer("/five_hour/resets_at"))
        .and_then(|v| v.as_i64())
        .and_then(ts_to_datetime);
    let seven_day_pct = rl
        .and_then(|r| r.pointer("/seven_day/used_percentage"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u8);
    let seven_day_resets_at = rl
        .and_then(|r| r.pointer("/seven_day/resets_at"))
        .and_then(|v| v.as_i64())
        .and_then(ts_to_datetime);

    Some(SlRecord {
        ts,
        session_id,
        project,
        model_id,
        model_name,
        version,
        cost_usd,
        duration_ms,
        api_duration_ms,
        lines_added,
        lines_removed,
        context_pct,
        context_size,
        five_hour_pct,
        five_hour_resets_at,
        seven_day_pct,
        seven_day_resets_at,
    })
}

/// Load and filter statusline records from a JSONL file.
/// Returns (records, skipped_count).
pub fn load_sl_records(file_path: &str, opts: &SlLoadOptions) -> (Vec<SlRecord>, usize) {
    let path = Path::new(file_path);
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (Vec::new(), 0),
    };

    let tz_ref = opts.tz.as_deref();
    let from_dt = opts
        .from
        .as_deref()
        .and_then(|s| parse_filter_datetime(s, tz_ref));
    let to_dt = opts
        .to
        .as_deref()
        .and_then(|s| parse_filter_datetime(s, tz_ref));

    let mut records = Vec::new();
    let mut skipped = 0;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match parse_line(line) {
            Some(record) => {
                // Time filter
                if let Some(ref from) = from_dt {
                    if record.ts < *from {
                        continue;
                    }
                }
                if let Some(ref to) = to_dt {
                    if record.ts > *to {
                        continue;
                    }
                }
                // Session filter (case-insensitive substring)
                if let Some(ref filter) = opts.session {
                    if !record
                        .session_id
                        .to_lowercase()
                        .contains(&filter.to_lowercase())
                    {
                        continue;
                    }
                }
                // Project filter
                if let Some(ref filter) = opts.project {
                    if !record
                        .project
                        .to_lowercase()
                        .contains(&filter.to_lowercase())
                    {
                        continue;
                    }
                }
                // Model filter
                if let Some(ref filter) = opts.model {
                    let combined = format!("{} {}", record.model_id, record.model_name);
                    if !combined.to_lowercase().contains(&filter.to_lowercase()) {
                        continue;
                    }
                }
                records.push(record);
            }
            None => {
                skipped += 1;
            }
        }
    }

    (records, skipped)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --test sl_parser`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/sl/parser.rs tests/sl_parser.rs
git commit -m "feat(sl): implement statusline JSONL parser with filtering"
```

---

### Task 5: Implement sl aggregator

**Files:**
- Modify: `src/sl/aggregator.rs`
- Create: `tests/sl_aggregator.rs`

- [ ] **Step 1: Write tests for aggregator**

Create `tests/sl_aggregator.rs`:

```rust
use chrono::{TimeZone, Utc};
use cctokens::sl::types::*;
use cctokens::sl::aggregator::*;

fn make_record(ts: i64, session: &str, cost: f64, dur_ms: u64, api_ms: u64, five_h: Option<u8>, seven_d: Option<u8>, resets_5h: Option<i64>, resets_7d: Option<i64>) -> SlRecord {
    SlRecord {
        ts: Utc.timestamp_opt(ts, 0).unwrap(),
        session_id: session.to_string(),
        project: "/home/user/proj".to_string(),
        model_id: "claude-opus-4-6".to_string(),
        model_name: "Opus 4.6".to_string(),
        version: "2.1.84".to_string(),
        cost_usd: cost,
        duration_ms: dur_ms,
        api_duration_ms: api_ms,
        lines_added: 0,
        lines_removed: 0,
        context_pct: Some(2),
        context_size: 1000000,
        five_hour_pct: five_h,
        five_hour_resets_at: resets_5h.and_then(|t| Utc.timestamp_opt(t, 0).single()),
        seven_day_pct: seven_d,
        seven_day_resets_at: resets_7d.and_then(|t| Utc.timestamp_opt(t, 0).single()),
    }
}

#[test]
fn test_aggregate_sessions_single_segment() {
    let records = vec![
        make_record(1000, "s1", 0.0, 100, 0, Some(1), Some(60), Some(2000), Some(9000)),
        make_record(1001, "s1", 0.5, 1000, 500, Some(2), Some(60), Some(2000), Some(9000)),
        make_record(1002, "s1", 1.0, 2000, 1000, Some(3), Some(60), Some(2000), Some(9000)),
    ];
    let summaries = aggregate_sessions(&records);
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].segments, 1);
    assert!((summaries[0].total_cost - 1.0).abs() < 0.001);
    assert_eq!(summaries[0].total_duration_ms, 2000);
    assert_eq!(summaries[0].last_five_hour_pct, Some(3));
}

#[test]
fn test_aggregate_sessions_with_reset() {
    let records = vec![
        make_record(1000, "s1", 0.0, 100, 0, Some(1), Some(60), Some(2000), Some(9000)),
        make_record(1001, "s1", 0.5, 1000, 500, Some(2), Some(60), Some(2000), Some(9000)),
        // RESET: cost drops, duration drops
        make_record(2000, "s1", 0.0, 50, 0, Some(5), Some(61), Some(4000), Some(9000)),
        make_record(2001, "s1", 0.3, 800, 400, Some(6), Some(61), Some(4000), Some(9000)),
    ];
    let summaries = aggregate_sessions(&records);
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].segments, 2);
    assert!((summaries[0].total_cost - 0.8).abs() < 0.001); // 0.5 + 0.3
    assert_eq!(summaries[0].total_duration_ms, 1800); // 1000 + 800
    assert_eq!(summaries[0].last_five_hour_pct, Some(6));
}

#[test]
fn test_aggregate_sessions_zero_not_reset() {
    // Two records both with cost=0 and dur=0 — NOT a reset
    let records = vec![
        make_record(1000, "s1", 0.0, 0, 0, None, None, None, None),
        make_record(1001, "s1", 0.0, 100, 0, Some(1), Some(60), Some(2000), Some(9000)),
        make_record(1002, "s1", 0.5, 500, 200, Some(2), Some(60), Some(2000), Some(9000)),
    ];
    let summaries = aggregate_sessions(&records);
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].segments, 1);
    assert!((summaries[0].total_cost - 0.5).abs() < 0.001);
}

#[test]
fn test_aggregate_ratelimit() {
    let records = vec![
        make_record(1000, "s1", 0.0, 100, 0, None, None, None, None),
        make_record(1001, "s1", 0.1, 200, 100, Some(1), Some(60), Some(2000), Some(9000)),
        make_record(1002, "s1", 0.2, 300, 200, Some(1), Some(60), Some(2000), Some(9000)), // dup, same pcts
        make_record(1003, "s1", 0.3, 400, 300, Some(2), Some(60), Some(2000), Some(9000)), // changed
    ];
    let entries = aggregate_ratelimit(&records);
    // Should have 2 entries: (1, 60) and (2, 60). The None and dup are filtered.
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].five_hour_pct, 1);
    assert_eq!(entries[1].five_hour_pct, 2);
}

#[test]
fn test_aggregate_windows() {
    let records = vec![
        make_record(1000, "s1", 0.0, 100, 0, Some(1), Some(60), Some(2000), Some(9000)),
        make_record(1001, "s1", 0.5, 500, 200, Some(3), Some(60), Some(2000), Some(9000)),
        // Different window
        make_record(3000, "s2", 0.0, 100, 0, Some(1), Some(61), Some(5000), Some(9000)),
        make_record(3001, "s2", 1.0, 1000, 500, Some(5), Some(61), Some(5000), Some(9000)),
    ];
    let sessions = aggregate_sessions(&records);
    let windows = aggregate_windows(&records, &sessions);
    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0].peak_five_hour_pct, 3);
    assert!((windows[0].total_cost - 0.5).abs() < 0.001);
    assert_eq!(windows[1].peak_five_hour_pct, 5);
}

#[test]
fn test_aggregate_by_project() {
    let mut records = vec![
        make_record(1000, "s1", 0.0, 100, 0, None, None, None, None),
        make_record(1001, "s1", 0.5, 500, 200, None, None, None, None),
    ];
    records[0].project = "/home/user/projA".to_string();
    records[1].project = "/home/user/projA".to_string();
    let mut records2 = vec![
        make_record(2000, "s2", 0.0, 100, 0, None, None, None, None),
        make_record(2001, "s2", 1.0, 1000, 500, None, None, None, None),
    ];
    records2[0].project = "/home/user/projB".to_string();
    records2[1].project = "/home/user/projB".to_string();
    records.extend(records2);

    let sessions = aggregate_sessions(&records);
    let projects = aggregate_by_project(&sessions);
    assert_eq!(projects.len(), 2);
}

#[test]
fn test_aggregate_by_day() {
    let records = vec![
        make_record(1774481258, "s1", 0.0, 100, 0, Some(2), Some(63), Some(1774497600), Some(1774605600)),
        make_record(1774481300, "s1", 0.5, 500, 200, Some(3), Some(63), Some(1774497600), Some(1774605600)),
    ];
    let sessions = aggregate_sessions(&records);
    let days = aggregate_by_day(&sessions, Some("UTC"));
    assert_eq!(days.len(), 1);
    assert_eq!(days[0].session_count, 1);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test sl_aggregator`
Expected: Compile error — aggregator functions not defined.

- [ ] **Step 3: Implement the aggregator**

Replace `src/sl/aggregator.rs`:

```rust
use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Local, Utc};

use super::types::*;

/// Detect segment boundaries and aggregate per-session.
pub fn aggregate_sessions(records: &[SlRecord]) -> Vec<SlSessionSummary> {
    // Group by session_id, preserving order within each group
    let mut session_map: BTreeMap<&str, Vec<&SlRecord>> = BTreeMap::new();
    for r in records {
        session_map.entry(&r.session_id).or_default().push(r);
    }

    let mut summaries = Vec::new();

    for (session_id, recs) in &session_map {
        let mut segments: u32 = 1;
        let mut seg_max_cost: f64 = 0.0;
        let mut seg_max_dur: u64 = 0;
        let mut seg_max_api: u64 = 0;
        let mut seg_max_lines_add: u64 = 0;
        let mut seg_max_lines_rem: u64 = 0;

        let mut total_cost: f64 = 0.0;
        let mut total_dur: u64 = 0;
        let mut total_api: u64 = 0;
        let mut total_lines_add: u64 = 0;
        let mut total_lines_rem: u64 = 0;
        let mut max_ctx: Option<u8> = None;

        let mut prev_cost: f64 = 0.0;
        let mut prev_dur: u64 = 0;
        let mut is_first = true;

        for r in recs {
            if !is_first {
                // Detect reset: cost drops significantly OR duration drops significantly
                // But NOT if previous values were both 0 (session just started)
                let cost_dropped = r.cost_usd < prev_cost - 0.0001;
                let dur_dropped = r.duration_ms + 100 < prev_dur;
                let prev_was_nonzero = prev_cost > 0.0001 || prev_dur > 100;

                if (cost_dropped || dur_dropped) && prev_was_nonzero {
                    // Flush current segment
                    total_cost += seg_max_cost;
                    total_dur += seg_max_dur;
                    total_api += seg_max_api;
                    total_lines_add += seg_max_lines_add;
                    total_lines_rem += seg_max_lines_rem;
                    segments += 1;

                    // Start new segment
                    seg_max_cost = 0.0;
                    seg_max_dur = 0;
                    seg_max_api = 0;
                    seg_max_lines_add = 0;
                    seg_max_lines_rem = 0;
                }
            }

            seg_max_cost = seg_max_cost.max(r.cost_usd);
            seg_max_dur = seg_max_dur.max(r.duration_ms);
            seg_max_api = seg_max_api.max(r.api_duration_ms);
            seg_max_lines_add = seg_max_lines_add.max(r.lines_added);
            seg_max_lines_rem = seg_max_lines_rem.max(r.lines_removed);

            if let Some(pct) = r.context_pct {
                max_ctx = Some(max_ctx.map_or(pct, |prev: u8| prev.max(pct)));
            }

            prev_cost = r.cost_usd;
            prev_dur = r.duration_ms;
            is_first = false;
        }

        // Flush last segment
        total_cost += seg_max_cost;
        total_dur += seg_max_dur;
        total_api += seg_max_api;
        total_lines_add += seg_max_lines_add;
        total_lines_rem += seg_max_lines_rem;

        let last = recs.last().unwrap();
        let first = recs.first().unwrap();

        summaries.push(SlSessionSummary {
            session_id: session_id.to_string(),
            project: first.project.clone(),
            model_name: first.model_name.clone(),
            version: first.version.clone(),
            segments,
            total_cost,
            total_duration_ms: total_dur,
            total_api_duration_ms: total_api,
            total_lines_added: total_lines_add,
            total_lines_removed: total_lines_rem,
            max_context_pct: max_ctx,
            first_ts: first.ts,
            last_ts: last.ts,
            last_five_hour_pct: last.five_hour_pct,
            last_seven_day_pct: last.seven_day_pct,
        });
    }

    summaries
}

/// Build rate limit timeline: filter records with rate_limits, deduplicate consecutive identical (5h%, 7d%).
pub fn aggregate_ratelimit(records: &[SlRecord]) -> Vec<SlRateLimitEntry> {
    let mut entries = Vec::new();
    let mut prev: Option<(u8, u8)> = None;

    for r in records {
        if let (Some(fh), Some(sd), Some(fh_reset), Some(sd_reset)) = (
            r.five_hour_pct,
            r.seven_day_pct,
            r.five_hour_resets_at,
            r.seven_day_resets_at,
        ) {
            let current = (fh, sd);
            if prev.map_or(true, |p| p != current) {
                entries.push(SlRateLimitEntry {
                    ts: r.ts,
                    session_id: r.session_id.clone(),
                    five_hour_pct: fh,
                    five_hour_resets_at: fh_reset,
                    seven_day_pct: sd,
                    seven_day_resets_at: sd_reset,
                });
                prev = Some(current);
            }
        }
    }

    entries
}

/// Aggregate by 5h window (grouped by resets_at).
pub fn aggregate_windows(records: &[SlRecord], sessions: &[SlSessionSummary]) -> Vec<SlWindowSummary> {
    // Group records by five_hour_resets_at
    let mut window_map: BTreeMap<DateTime<Utc>, Vec<&SlRecord>> = BTreeMap::new();
    for r in records {
        if let Some(resets_at) = r.five_hour_resets_at {
            window_map.entry(resets_at).or_default().push(r);
        }
    }

    // For cost calculation within a window, we need segment-aware per-session costs.
    // We'll do a mini segment-detection within each window's records, grouped by session.
    let mut summaries = Vec::new();

    for (resets_at, recs) in &window_map {
        let window_start = *resets_at - chrono::Duration::hours(5);
        let peak_pct = recs.iter().filter_map(|r| r.five_hour_pct).max().unwrap_or(0);

        // Count unique sessions
        let unique_sessions: HashSet<&str> = recs.iter().map(|r| r.session_id.as_str()).collect();

        // Calculate cost in this window using segment-aware logic per session
        let mut window_cost = 0.0;
        let mut session_groups: BTreeMap<&str, Vec<&SlRecord>> = BTreeMap::new();
        for r in recs {
            session_groups.entry(&r.session_id).or_default().push(r);
        }
        for (_sid, srecs) in &session_groups {
            let mut seg_max_cost: f64 = 0.0;
            let mut prev_cost: f64 = 0.0;
            let mut prev_dur: u64 = 0;
            let mut is_first = true;

            for r in srecs {
                if !is_first {
                    let cost_dropped = r.cost_usd < prev_cost - 0.0001;
                    let dur_dropped = r.duration_ms + 100 < prev_dur;
                    let prev_was_nonzero = prev_cost > 0.0001 || prev_dur > 100;
                    if (cost_dropped || dur_dropped) && prev_was_nonzero {
                        window_cost += seg_max_cost;
                        seg_max_cost = 0.0;
                    }
                }
                seg_max_cost = seg_max_cost.max(r.cost_usd);
                prev_cost = r.cost_usd;
                prev_dur = r.duration_ms;
                is_first = false;
            }
            window_cost += seg_max_cost;
        }

        let est_budget = if peak_pct > 0 {
            Some(window_cost * 100.0 / peak_pct as f64)
        } else {
            None
        };

        summaries.push(SlWindowSummary {
            window_start,
            window_end: *resets_at,
            peak_five_hour_pct: peak_pct,
            sessions: unique_sessions.len() as u32,
            total_cost: window_cost,
            est_budget,
        });
    }

    summaries
}

/// Aggregate session summaries by project.
pub fn aggregate_by_project(sessions: &[SlSessionSummary]) -> Vec<SlProjectSummary> {
    let mut map: BTreeMap<&str, SlProjectSummary> = BTreeMap::new();

    for s in sessions {
        let entry = map.entry(&s.project).or_insert_with(|| SlProjectSummary {
            project: s.project.clone(),
            total_cost: 0.0,
            total_duration_ms: 0,
            total_api_duration_ms: 0,
            session_count: 0,
        });
        entry.total_cost += s.total_cost;
        entry.total_duration_ms += s.total_duration_ms;
        entry.total_api_duration_ms += s.total_api_duration_ms;
        entry.session_count += 1;
    }

    map.into_values().collect()
}

/// Format a DateTime in the specified timezone as "YYYY-MM-DD".
fn format_date(dt: &DateTime<Utc>, tz: Option<&str>) -> String {
    let fmt = "%Y-%m-%d";
    match tz {
        Some("UTC") => dt.format(fmt).to_string(),
        Some(tz_str) if tz_str.starts_with('+') || tz_str.starts_with('-') => {
            let sign: i32 = if tz_str.starts_with('+') { 1 } else { -1 };
            let rest = &tz_str[1..];
            let parts: Vec<&str> = rest.split(':').collect();
            if parts.len() == 2 {
                if let (Ok(h), Ok(m)) = (parts[0].parse::<i32>(), parts[1].parse::<i32>()) {
                    if let Some(offset) = chrono::FixedOffset::east_opt(sign * (h * 3600 + m * 60)) {
                        return dt.with_timezone(&offset).format(fmt).to_string();
                    }
                }
            }
            dt.with_timezone(&Local).format(fmt).to_string()
        }
        Some(tz_str) => {
            if let Ok(tz_parsed) = tz_str.parse::<chrono_tz::Tz>() {
                dt.with_timezone(&tz_parsed).format(fmt).to_string()
            } else {
                dt.with_timezone(&Local).format(fmt).to_string()
            }
        }
        None => dt.with_timezone(&Local).format(fmt).to_string(),
    }
}

/// Aggregate session summaries by day.
pub fn aggregate_by_day(sessions: &[SlSessionSummary], tz: Option<&str>) -> Vec<SlDaySummary> {
    let mut map: BTreeMap<String, SlDaySummary> = BTreeMap::new();

    for s in sessions {
        let date = format_date(&s.first_ts, tz);
        let entry = map.entry(date.clone()).or_insert_with(|| SlDaySummary {
            date,
            total_cost: 0.0,
            session_count: 0,
            peak_five_hour_pct: None,
            peak_seven_day_pct: None,
        });
        entry.total_cost += s.total_cost;
        entry.session_count += 1;
        if let Some(pct) = s.last_five_hour_pct {
            entry.peak_five_hour_pct =
                Some(entry.peak_five_hour_pct.map_or(pct, |prev: u8| prev.max(pct)));
        }
        if let Some(pct) = s.last_seven_day_pct {
            entry.peak_seven_day_pct =
                Some(entry.peak_seven_day_pct.map_or(pct, |prev: u8| prev.max(pct)));
        }
    }

    map.into_values().collect()
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --test sl_aggregator`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/sl/aggregator.rs tests/sl_aggregator.rs
git commit -m "feat(sl): implement segment-aware aggregator"
```

---

### Task 6: Implement sl formatter

**Files:**
- Modify: `src/sl/formatter.rs`
- Create: `tests/sl_formatter.rs`

- [ ] **Step 1: Write tests for formatter**

Create `tests/sl_formatter.rs`:

```rust
use chrono::{TimeZone, Utc};
use cctokens::sl::types::*;
use cctokens::sl::formatter::*;
use cctokens::types::PriceMode;

fn make_rl_entry(ts: i64, fh: u8, sd: u8) -> SlRateLimitEntry {
    SlRateLimitEntry {
        ts: Utc.timestamp_opt(ts, 0).unwrap(),
        session_id: "sess-aaa".to_string(),
        five_hour_pct: fh,
        five_hour_resets_at: Utc.timestamp_opt(ts + 3600, 0).unwrap(),
        seven_day_pct: sd,
        seven_day_resets_at: Utc.timestamp_opt(ts + 86400, 0).unwrap(),
    }
}

fn make_session_summary() -> SlSessionSummary {
    SlSessionSummary {
        session_id: "abcd1234-5678".to_string(),
        project: "/home/user/project".to_string(),
        model_name: "Opus 4.6".to_string(),
        version: "2.1.84".to_string(),
        segments: 2,
        total_cost: 1.5,
        total_duration_ms: 3600000,
        total_api_duration_ms: 1200000,
        total_lines_added: 100,
        total_lines_removed: 20,
        max_context_pct: Some(8),
        first_ts: Utc.timestamp_opt(1774481258, 0).unwrap(),
        last_ts: Utc.timestamp_opt(1774484858, 0).unwrap(),
        last_five_hour_pct: Some(5),
        last_seven_day_pct: Some(63),
    }
}

#[test]
fn test_format_ratelimit_table() {
    let entries = vec![
        make_rl_entry(1774481258, 2, 63),
        make_rl_entry(1774481300, 3, 63),
    ];
    let opts = SlFormatOptions {
        tz: Some("UTC".to_string()),
        price_mode: PriceMode::Decimal,
        compact: false,
        color: false,
    };
    let result = format_sl_ratelimit_table(&entries, &opts);
    assert!(result.contains("5h%"));
    assert!(result.contains("7d%"));
    assert!(result.contains("2%"));
    assert!(result.contains("3%"));
}

#[test]
fn test_format_session_table() {
    let sessions = vec![make_session_summary()];
    let opts = SlFormatOptions {
        tz: Some("UTC".to_string()),
        price_mode: PriceMode::Decimal,
        compact: false,
        color: false,
    };
    let result = format_sl_session_table(&sessions, &opts);
    assert!(result.contains("abcd1234"));
    assert!(result.contains("$1.50"));
    assert!(result.contains("33%")); // API% = 1200000/3600000 = 33%
}

#[test]
fn test_format_sl_json_ratelimit() {
    let entries = vec![make_rl_entry(1774481258, 2, 63)];
    let meta = SlJsonMeta {
        source: "statusline".to_string(),
        file: "test.jsonl".to_string(),
        view: "ratelimit".to_string(),
        from: None,
        to: None,
        tz: Some("UTC".to_string()),
        generated_at: "2026-03-26T00:00:00Z".to_string(),
    };
    let result = format_sl_json_ratelimit(&entries, &meta);
    assert!(result.contains("\"fiveHourPct\""));
    assert!(result.contains("\"ratelimit\""));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test sl_formatter`
Expected: Compile error.

- [ ] **Step 3: Implement the formatter**

Replace `src/sl/formatter.rs`:

```rust
use chrono::{DateTime, Local, Utc};
use serde_json;

use crate::types::PriceMode;
use crate::formatters::table::format_cost;
use super::types::*;

/// Options for sl formatters.
pub struct SlFormatOptions {
    pub tz: Option<String>,
    pub price_mode: PriceMode,
    pub compact: bool,
    pub color: bool,
}

/// Metadata for sl JSON output.
pub struct SlJsonMeta {
    pub source: String,
    pub file: String,
    pub view: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub tz: Option<String>,
    pub generated_at: String,
}

/// Format a DateTime for display in the given tz.
fn fmt_dt(dt: &DateTime<Utc>, tz: Option<&str>, fmt_str: &str) -> String {
    match tz {
        Some("UTC") => dt.format(fmt_str).to_string(),
        Some(tz_str) if tz_str.starts_with('+') || tz_str.starts_with('-') => {
            let sign: i32 = if tz_str.starts_with('+') { 1 } else { -1 };
            let rest = &tz_str[1..];
            let parts: Vec<&str> = rest.split(':').collect();
            if parts.len() == 2 {
                if let (Ok(h), Ok(m)) = (parts[0].parse::<i32>(), parts[1].parse::<i32>()) {
                    if let Some(offset) = chrono::FixedOffset::east_opt(sign * (h * 3600 + m * 60)) {
                        return dt.with_timezone(&offset).format(fmt_str).to_string();
                    }
                }
            }
            dt.with_timezone(&Local).format(fmt_str).to_string()
        }
        Some(tz_str) => {
            if let Ok(tz_parsed) = tz_str.parse::<chrono_tz::Tz>() {
                dt.with_timezone(&tz_parsed).format(fmt_str).to_string()
            } else {
                dt.with_timezone(&Local).format(fmt_str).to_string()
            }
        }
        None => dt.with_timezone(&Local).format(fmt_str).to_string(),
    }
}

fn fmt_time(dt: &DateTime<Utc>, tz: Option<&str>) -> String {
    fmt_dt(dt, tz, "%Y-%m-%d %H:%M")
}

fn fmt_time_short(dt: &DateTime<Utc>, tz: Option<&str>) -> String {
    fmt_dt(dt, tz, "%m-%d %H:%M")
}

fn fmt_duration(ms: u64) -> String {
    let secs = ms / 1000;
    if secs >= 3600 {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        format!("{}h{:02}m", h, m)
    } else if secs >= 60 {
        let m = secs / 60;
        let s = secs % 60;
        format!("{}m{:02}s", m, s)
    } else {
        format!("{}s", secs)
    }
}

fn fmt_pct_opt(pct: Option<u8>) -> String {
    pct.map_or("\u{2014}".to_string(), |p| format!("{}%", p)) // — for None
}

/// Generic table renderer from header + rows (each row is a Vec<String>).
fn render_table(headers: &[&str], rows: &[Vec<String>], color: bool) -> String {
    // Calculate column widths
    let num_cols = headers.len();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < num_cols {
                widths[i] = widths[i].max(cell.len());
            }
        }
    }

    let mut output = String::new();

    // Top border
    output.push('\u{250C}'); // ┌
    for (i, w) in widths.iter().enumerate() {
        output.push_str(&"\u{2500}".repeat(w + 2)); // ─
        if i < num_cols - 1 {
            output.push('\u{252C}'); // ┬
        }
    }
    output.push('\u{2510}'); // ┐
    output.push('\n');

    // Header row
    output.push('\u{2502}'); // │
    for (i, header) in headers.iter().enumerate() {
        let padded = format!(" {:width$} ", header, width = widths[i]);
        if color {
            output.push_str(&format!("\x1b[1m{}\x1b[0m", padded));
        } else {
            output.push_str(&padded);
        }
        output.push('\u{2502}');
    }
    output.push('\n');

    // Header separator
    output.push('\u{251C}'); // ├
    for (i, w) in widths.iter().enumerate() {
        output.push_str(&"\u{2500}".repeat(w + 2));
        if i < num_cols - 1 {
            output.push('\u{253C}'); // ┼
        }
    }
    output.push('\u{2524}'); // ┤
    output.push('\n');

    // Data rows
    for row in rows {
        output.push('\u{2502}');
        for (i, cell) in row.iter().enumerate() {
            if i < num_cols {
                // Right-align numeric columns (all except first)
                if i == 0 {
                    output.push_str(&format!(" {:width$} ", cell, width = widths[i]));
                } else {
                    output.push_str(&format!(" {:>width$} ", cell, width = widths[i]));
                }
                output.push('\u{2502}');
            }
        }
        output.push('\n');
    }

    // Bottom border
    output.push('\u{2514}'); // └
    for (i, w) in widths.iter().enumerate() {
        output.push_str(&"\u{2500}".repeat(w + 2));
        if i < num_cols - 1 {
            output.push('\u{2534}'); // ┴
        }
    }
    output.push('\u{2518}'); // ┘
    output.push('\n');

    output
}

// ── Public formatters ──

pub fn format_sl_ratelimit_table(entries: &[SlRateLimitEntry], opts: &SlFormatOptions) -> String {
    let tz = opts.tz.as_deref();
    let headers: Vec<&str> = if opts.compact {
        vec!["Time", "5h%", "7d%", "5h Resets"]
    } else {
        vec!["Time", "5h%", "7d%", "5h Resets", "Session"]
    };

    let rows: Vec<Vec<String>> = entries
        .iter()
        .map(|e| {
            let mut row = vec![
                fmt_time(&e.ts, tz),
                format!("{}%", e.five_hour_pct),
                format!("{}%", e.seven_day_pct),
                fmt_time_short(&e.five_hour_resets_at, tz),
            ];
            if !opts.compact {
                row.push(e.session_id[..8.min(e.session_id.len())].to_string());
            }
            row
        })
        .collect();

    render_table(&headers, &rows, opts.color)
}

pub fn format_sl_session_table(sessions: &[SlSessionSummary], opts: &SlFormatOptions) -> String {
    let headers: Vec<&str> = if opts.compact {
        vec!["Session", "Project", "Cost", "Duration", "Lines +/-"]
    } else {
        vec!["Session", "Project", "Cost", "Duration", "API Time", "API%", "Lines +/-", "Ctx%", "Segs"]
    };

    let rows: Vec<Vec<String>> = sessions
        .iter()
        .map(|s| {
            let sid = s.session_id[..8.min(s.session_id.len())].to_string();
            let proj = shorten_project(&s.project);
            let cost = format_cost(s.total_cost, opts.price_mode);
            let dur = fmt_duration(s.total_duration_ms);

            if opts.compact {
                vec![
                    sid,
                    proj,
                    cost,
                    dur,
                    format!("+{}/-{}", s.total_lines_added, s.total_lines_removed),
                ]
            } else {
                let api_dur = fmt_duration(s.total_api_duration_ms);
                let api_pct = if s.total_duration_ms > 0 {
                    format!("{}%", (s.total_api_duration_ms as f64 / s.total_duration_ms as f64 * 100.0).round() as u64)
                } else {
                    "\u{2014}".to_string()
                };
                vec![
                    sid,
                    proj,
                    cost,
                    dur,
                    api_dur,
                    api_pct,
                    format!("+{}/-{}", s.total_lines_added, s.total_lines_removed),
                    fmt_pct_opt(s.max_context_pct),
                    s.segments.to_string(),
                ]
            }
        })
        .collect();

    render_table(&headers, &rows, opts.color)
}

pub fn format_sl_project_table(projects: &[SlProjectSummary], opts: &SlFormatOptions) -> String {
    let headers = vec!["Project", "Cost", "Duration", "API Time", "Sessions"];
    let rows: Vec<Vec<String>> = projects
        .iter()
        .map(|p| {
            vec![
                shorten_project(&p.project),
                format_cost(p.total_cost, opts.price_mode),
                fmt_duration(p.total_duration_ms),
                fmt_duration(p.total_api_duration_ms),
                p.session_count.to_string(),
            ]
        })
        .collect();
    render_table(&headers, &rows, opts.color)
}

pub fn format_sl_day_table(days: &[SlDaySummary], opts: &SlFormatOptions) -> String {
    let headers = vec!["Date", "Cost", "Sessions", "Peak 5h%", "Peak 7d%"];
    let rows: Vec<Vec<String>> = days
        .iter()
        .map(|d| {
            vec![
                d.date.clone(),
                format_cost(d.total_cost, opts.price_mode),
                d.session_count.to_string(),
                fmt_pct_opt(d.peak_five_hour_pct),
                fmt_pct_opt(d.peak_seven_day_pct),
            ]
        })
        .collect();
    render_table(&headers, &rows, opts.color)
}

pub fn format_sl_window_table(windows: &[SlWindowSummary], opts: &SlFormatOptions) -> String {
    let tz = opts.tz.as_deref();
    let headers = vec!["5h Window", "Peak 5h%", "Sessions", "Cost", "Est Budget"];
    let rows: Vec<Vec<String>> = windows
        .iter()
        .map(|w| {
            let window_label = format!(
                "{}\u{2192}{}",
                fmt_time_short(&w.window_start, tz),
                fmt_time_short(&w.window_end, tz),
            );
            vec![
                window_label,
                format!("{}%", w.peak_five_hour_pct),
                w.sessions.to_string(),
                format_cost(w.total_cost, opts.price_mode),
                w.est_budget
                    .map(|b| format_cost(b, opts.price_mode))
                    .unwrap_or_else(|| "\u{2014}".to_string()),
            ]
        })
        .collect();
    render_table(&headers, &rows, opts.color)
}

pub fn format_sl_cost_diff_table(sessions: &[SlSessionSummary], diffs: &[SlCostDiff], opts: &SlFormatOptions) -> String {
    let headers = vec!["Session", "Project", "Cost(SL)", "Cost(LiteLLM)", "Diff", "Diff%"];
    let rows: Vec<Vec<String>> = diffs
        .iter()
        .map(|d| {
            let proj = sessions
                .iter()
                .find(|s| s.session_id == d.session_id)
                .map(|s| shorten_project(&s.project))
                .unwrap_or_default();
            vec![
                d.session_id[..8.min(d.session_id.len())].to_string(),
                proj,
                format_cost(d.sl_cost, opts.price_mode),
                d.litellm_cost
                    .map(|c| format_cost(c, opts.price_mode))
                    .unwrap_or_else(|| "\u{2014}".to_string()),
                d.diff
                    .map(|v| format!("{:+.4}", v))
                    .unwrap_or_else(|| "\u{2014}".to_string()),
                d.diff_pct
                    .map(|v| format!("{:+.2}%", v))
                    .unwrap_or_else(|| "\u{2014}".to_string()),
            ]
        })
        .collect();
    render_table(&headers, &rows, opts.color)
}

// ── JSON formatters ──

pub fn format_sl_json_ratelimit(entries: &[SlRateLimitEntry], meta: &SlJsonMeta) -> String {
    let output = serde_json::json!({
        "meta": {
            "source": meta.source,
            "file": meta.file,
            "view": meta.view,
            "from": meta.from,
            "to": meta.to,
            "tz": meta.tz,
            "generatedAt": meta.generated_at,
        },
        "data": entries,
    });
    serde_json::to_string_pretty(&output).unwrap_or_default()
}

pub fn format_sl_json_sessions(sessions: &[SlSessionSummary], meta: &SlJsonMeta) -> String {
    let total_cost: f64 = sessions.iter().map(|s| s.total_cost).sum();
    let output = serde_json::json!({
        "meta": {
            "source": meta.source,
            "file": meta.file,
            "view": meta.view,
            "from": meta.from,
            "to": meta.to,
            "tz": meta.tz,
            "generatedAt": meta.generated_at,
        },
        "data": sessions,
        "totals": { "totalCost": total_cost, "sessions": sessions.len() },
    });
    serde_json::to_string_pretty(&output).unwrap_or_default()
}

pub fn format_sl_json_windows(windows: &[SlWindowSummary], meta: &SlJsonMeta) -> String {
    let output = serde_json::json!({
        "meta": {
            "source": meta.source,
            "file": meta.file,
            "view": meta.view,
            "from": meta.from,
            "to": meta.to,
            "tz": meta.tz,
            "generatedAt": meta.generated_at,
        },
        "data": windows,
    });
    serde_json::to_string_pretty(&output).unwrap_or_default()
}

pub fn format_sl_json_projects(projects: &[SlProjectSummary], meta: &SlJsonMeta) -> String {
    let output = serde_json::json!({
        "meta": {
            "source": meta.source,
            "file": meta.file,
            "view": meta.view,
            "from": meta.from,
            "to": meta.to,
            "tz": meta.tz,
            "generatedAt": meta.generated_at,
        },
        "data": projects,
    });
    serde_json::to_string_pretty(&output).unwrap_or_default()
}

pub fn format_sl_json_days(days: &[SlDaySummary], meta: &SlJsonMeta) -> String {
    let output = serde_json::json!({
        "meta": {
            "source": meta.source,
            "file": meta.file,
            "view": meta.view,
            "from": meta.from,
            "to": meta.to,
            "tz": meta.tz,
            "generatedAt": meta.generated_at,
        },
        "data": days,
    });
    serde_json::to_string_pretty(&output).unwrap_or_default()
}

// ── CSV formatters ──

pub fn format_sl_csv_ratelimit(entries: &[SlRateLimitEntry], tz: Option<&str>) -> String {
    let mut out = String::from("Time,5h%,7d%,5h Resets,Session\n");
    for e in entries {
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            fmt_time(&e.ts, tz),
            e.five_hour_pct,
            e.seven_day_pct,
            fmt_time_short(&e.five_hour_resets_at, tz),
            &e.session_id[..8.min(e.session_id.len())],
        ));
    }
    out
}

pub fn format_sl_csv_sessions(sessions: &[SlSessionSummary], opts: &SlFormatOptions) -> String {
    let mut out = String::from("Session,Project,Cost,Duration_ms,API_ms,API%,Lines+,Lines-,Ctx%,Segments\n");
    for s in sessions {
        let api_pct = if s.total_duration_ms > 0 {
            (s.total_api_duration_ms as f64 / s.total_duration_ms as f64 * 100.0).round() as u64
        } else {
            0
        };
        out.push_str(&format!(
            "{},{},{},{},{},{}%,{},{},{},{}\n",
            &s.session_id[..8.min(s.session_id.len())],
            s.project,
            format_cost(s.total_cost, opts.price_mode),
            s.total_duration_ms,
            s.total_api_duration_ms,
            api_pct,
            s.total_lines_added,
            s.total_lines_removed,
            s.max_context_pct.map_or("-".to_string(), |p| format!("{}%", p)),
            s.segments,
        ));
    }
    out
}

// ── Helpers ──

fn shorten_project(path: &str) -> String {
    // Show last 2 components: ".../parent/project"
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() <= 2 {
        path.to_string()
    } else {
        format!(".../{}/{}", parts[parts.len() - 2], parts[parts.len() - 1])
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --test sl_formatter`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/sl/formatter.rs tests/sl_formatter.rs
git commit -m "feat(sl): implement sl-specific formatters (table, json, csv)"
```

---

### Task 7: Integrate sl subcommand into main.rs

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Add clap subcommand and run_sl function**

Add to `build_command()`, after the existing `.arg(...)` chain, before the final build:

```rust
.subcommand(
    Command::new("sl")
        .about("Analyze statusline.jsonl (rate limits, sessions, budgets)")
        .arg(Arg::new("file").long("file").value_name("path"))
        .arg(
            Arg::new("per")
                .long("per")
                .value_name("dim"),
        )
        .arg(Arg::new("chart").long("chart").value_name("mode"))
        .arg(
            Arg::new("cost-diff")
                .long("cost-diff")
                .action(ArgAction::SetTrue),
        )
        .arg(Arg::new("from").long("from").value_name("date"))
        .arg(Arg::new("to").long("to").value_name("date"))
        .arg(Arg::new("tz").long("tz").value_name("timezone"))
        .arg(Arg::new("session").long("session").value_name("id"))
        .arg(Arg::new("project").long("project").value_name("name"))
        .arg(Arg::new("model").long("model").value_name("name"))
        .arg(Arg::new("cost").long("cost").value_name("mode"))
        .arg(Arg::new("output").long("output").value_name("format"))
        .arg(Arg::new("filename").long("filename").value_name("path"))
        .arg(Arg::new("copy").long("copy").value_name("format"))
        .arg(Arg::new("order").long("order").value_name("order"))
        .arg(Arg::new("table").long("table").value_name("mode"))
        .arg(Arg::new("5hfrom").long("5hfrom").value_name("datetime"))
        .arg(Arg::new("5hto").long("5hto").value_name("datetime"))
        .arg(Arg::new("1wfrom").long("1wfrom").value_name("datetime"))
        .arg(Arg::new("1wto").long("1wto").value_name("datetime"))
        .arg(
            Arg::new("claude-dir")
                .long("claude-dir")
                .value_name("dir"),
        )
        .arg(
            Arg::new("live-pricing")
                .long("live-pricing")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("pricing-data")
                .long("pricing-data")
                .value_name("path"),
        )
        .arg(
            Arg::new("help")
                .long("help")
                .action(ArgAction::SetTrue),
        ),
)
```

- [ ] **Step 2: Add sl dispatch in main()**

At the start of `main()`, after parsing matches, add before the `--help` check:

```rust
if let Some(sl_matches) = matches.subcommand_matches("sl") {
    run_sl(sl_matches);
    return;
}
```

- [ ] **Step 3: Implement run_sl function**

Add the `run_sl` function to `main.rs`. This is the core dispatch logic for the `sl` subcommand. It handles validation, loading, aggregation, and formatting:

```rust
fn run_sl(matches: &clap::ArgMatches) {
    use cctokens::sl::*;
    use cctokens::sl::formatter::*;
    use cctokens::formatters::chart::{render_chart_raw, y_label_cost, y_label_percent};
    use cctokens::utils::{compute_date_range, copy_to_clipboard, ext_for_format, term_width};

    // Handle --help
    if matches.get_flag("help") {
        let help = r#"Usage: cctokens sl [options]

Analyze statusline.jsonl for rate limits, session stats, and budget estimation.

Options:
  --file <path>         Statusline file path (default: ~/.claude/statusline.jsonl)
  --per <dim>           View mode: session, project, day, window
                        Default (no --per): rate limit timeline
  --chart <mode>        Chart: 5h, 7d, cost
  --cost-diff           Compare cost with LiteLLM calculation (only with --per session)
  --from <date>         Start date (YYYY-MM-DD or YYYY-MM-DDTHH:MM:SS)
  --to <date>           End date
  --5hfrom <datetime>   5-hour window from datetime
  --5hto <datetime>     5-hour window to datetime
  --1wfrom <datetime>   1-week window from datetime
  --1wto <datetime>     1-week window to datetime
  --tz <timezone>       Timezone: UTC, +HH:MM, or IANA name
  --session <id>        Filter by session (substring)
  --project <name>      Filter by project (substring)
  --model <name>        Filter by model (substring)
  --cost <mode>         Cost display: true (default), decimal, false
  --output <format>     Output: json, csv, txt
  --filename <path>     Write output to file
  --copy <format>       Copy to clipboard: json, csv, txt
  --order <order>       Sort: asc (default), desc
  --table <mode>        Table: auto (default), full, compact
  --help                Show this help"#;
        eprintln!("{}", help);
        return;
    }

    let mut errors: Vec<String> = Vec::new();

    // Validate --per
    let per_val = matches.get_one::<String>("per").cloned();
    let view_mode = if let Some(ref p) = per_val {
        match p.as_str() {
            "session" => SlViewMode::Session,
            "project" => SlViewMode::Project,
            "day" => SlViewMode::Day,
            "window" => SlViewMode::Window,
            _ => {
                errors.push(format!(
                    "--per: invalid dimension '{}'. Valid: session, project, day, window",
                    p
                ));
                SlViewMode::RateLimit
            }
        }
    } else {
        SlViewMode::RateLimit
    };

    // Validate --chart
    let chart_val = matches.get_one::<String>("chart").cloned();
    let chart_mode = if let Some(ref c) = chart_val {
        match c.as_str() {
            "5h" => Some(SlChartMode::FiveHour),
            "7d" => Some(SlChartMode::SevenDay),
            "cost" => Some(SlChartMode::Cost),
            _ => {
                errors.push(format!(
                    "--chart: invalid mode '{}'. Valid: 5h, 7d, cost",
                    c
                ));
                None
            }
        }
    } else {
        None
    };

    // Validate --cost
    let cost_str = matches.get_one::<String>("cost").cloned();
    let price_mode = if let Some(ref c) = cost_str {
        match c.as_str() {
            "true" | "" => PriceMode::Integer,
            "false" => PriceMode::Off,
            "decimal" => PriceMode::Decimal,
            _ => {
                errors.push(format!("--cost: invalid value '{}'. Valid: true, false, decimal", c));
                PriceMode::Integer
            }
        }
    } else {
        PriceMode::Integer
    };

    // Validate --output
    let output_format = matches.get_one::<String>("output").cloned();
    if let Some(ref fmt) = output_format {
        match fmt.as_str() {
            "json" | "csv" | "txt" => {}
            _ => errors.push(format!(
                "--output: invalid format '{}'. Valid for sl: json, csv, txt",
                fmt
            )),
        }
    }

    // Validate --copy
    let copy_format = matches.get_one::<String>("copy").cloned();
    if let Some(ref fmt) = copy_format {
        match fmt.as_str() {
            "json" | "csv" | "txt" => {}
            _ => errors.push(format!(
                "--copy: invalid format '{}'. Valid for sl: json, csv, txt",
                fmt
            )),
        }
    }

    // Validate --order
    let order_str = matches.get_one::<String>("order").cloned();
    let order = if let Some(ref o) = order_str {
        match SortOrder::from_str(o) {
            Some(ord) => ord,
            None => {
                errors.push(format!("--order: invalid value '{}'. Valid: asc, desc", o));
                SortOrder::Asc
            }
        }
    } else {
        SortOrder::Asc
    };

    // Date validation
    let date_re = regex::Regex::new(r"^\d{4}-\d{2}-\d{2}([ T]\d{2}:\d{2}(:\d{2})?)?$").unwrap();
    let from_val = matches.get_one::<String>("from").cloned();
    let to_val = matches.get_one::<String>("to").cloned();
    let h5from_val = matches.get_one::<String>("5hfrom").cloned();
    let h5to_val = matches.get_one::<String>("5hto").cloned();
    let w1from_val = matches.get_one::<String>("1wfrom").cloned();
    let w1to_val = matches.get_one::<String>("1wto").cloned();

    for (name, val) in [
        ("--from", &from_val), ("--to", &to_val),
        ("--5hfrom", &h5from_val), ("--5hto", &h5to_val),
        ("--1wfrom", &w1from_val), ("--1wto", &w1to_val),
    ] {
        if let Some(ref v) = val {
            if !date_re.is_match(v) {
                errors.push(format!("{}: invalid date format '{}'", name, v));
            }
        }
    }
    if h5from_val.is_some() && h5to_val.is_some() {
        errors.push("--5hfrom and --5hto cannot be used together".to_string());
    }
    if w1from_val.is_some() && w1to_val.is_some() {
        errors.push("--1wfrom and --1wto cannot be used together".to_string());
    }
    let has_5h = h5from_val.is_some() || h5to_val.is_some();
    let has_1w = w1from_val.is_some() || w1to_val.is_some();
    if has_5h && has_1w {
        errors.push("--5h* and --1w* options cannot be used together".to_string());
    }
    if (has_5h || has_1w) && (from_val.is_some() || to_val.is_some()) {
        errors.push("--5hfrom/--5hto/--1wfrom/--1wto cannot be used with --from/--to".to_string());
    }

    // --cost-diff validation
    let cost_diff = matches.get_flag("cost-diff");
    if cost_diff && view_mode != SlViewMode::Session {
        eprintln!("Warning: --cost-diff only works with --per session, ignoring");
    }

    if !errors.is_empty() {
        for err in &errors {
            eprintln!("Error: {}", err);
        }
        process::exit(1);
    }

    // Compute date range
    let (effective_from, effective_to) = compute_date_range(
        from_val, to_val, h5from_val, h5to_val, w1from_val, w1to_val,
    );

    // Determine file path
    let file_path = matches.get_one::<String>("file").cloned().unwrap_or_else(|| {
        let home = dirs::home_dir().unwrap_or_default();
        home.join(".claude").join("statusline.jsonl").to_string_lossy().to_string()
    });

    // Check file exists
    if !std::path::Path::new(&file_path).exists() {
        eprintln!("Error: statusline file not found: {}", file_path);
        eprintln!("Hint: set up statusline.sh hook to generate this file");
        process::exit(1);
    }

    let tz_opt = matches.get_one::<String>("tz").cloned();

    // Load records
    let load_opts = SlLoadOptions {
        file: Some(file_path.clone()),
        from: effective_from.clone(),
        to: effective_to.clone(),
        tz: tz_opt.clone(),
        session: matches.get_one::<String>("session").cloned(),
        project: matches.get_one::<String>("project").cloned(),
        model: matches.get_one::<String>("model").cloned(),
    };

    let (records, skipped) = load_sl_records(&file_path, &load_opts);
    if skipped > 0 {
        eprintln!("Skipped {} malformed lines", skipped);
    }
    if records.is_empty() {
        eprintln!("No statusline records found");
        return;
    }

    eprintln!("Loaded {} statusline records", records.len());

    // Table mode
    let table_mode = matches.get_one::<String>("table").map(|s| s.as_str()).unwrap_or("auto");
    let compact = match table_mode {
        "compact" => true,
        "full" => false,
        _ => term_width() < 120,
    };

    let fmt_opts = SlFormatOptions {
        tz: tz_opt.clone(),
        price_mode,
        compact,
        color: true,
    };

    let fmt_opts_nocolor = SlFormatOptions {
        tz: tz_opt.clone(),
        price_mode,
        compact,
        color: false,
    };

    let json_meta = SlJsonMeta {
        source: "statusline".to_string(),
        file: file_path.clone(),
        view: match chart_mode {
            Some(_) => "chart".to_string(),
            None => match view_mode {
                SlViewMode::RateLimit => "ratelimit".to_string(),
                SlViewMode::Session => "sessions".to_string(),
                SlViewMode::Project => "project".to_string(),
                SlViewMode::Day => "day".to_string(),
                SlViewMode::Window => "window".to_string(),
            },
        },
        from: effective_from,
        to: effective_to,
        tz: tz_opt.clone(),
        generated_at: chrono::Utc::now().to_rfc3339(),
    };

    let filename_opt = matches.get_one::<String>("filename").cloned();

    // ── Chart mode ──
    if let Some(chart) = chart_mode {
        let rl_entries = aggregate_ratelimit(&records);

        let (keys, values, title, y_fn): (Vec<String>, Vec<f64>, &str, fn(f64) -> String) = match chart {
            SlChartMode::FiveHour => {
                let tz = tz_opt.as_deref();
                let k: Vec<String> = rl_entries.iter().map(|e| {
                    let dt = &e.ts;
                    match tz {
                        Some("UTC") => dt.format("%m-%dT%H:%M").to_string(),
                        _ => dt.with_timezone(&chrono::Local).format("%m-%dT%H:%M").to_string(),
                    }
                }).collect();
                let v: Vec<f64> = rl_entries.iter().map(|e| e.five_hour_pct as f64).collect();
                (k, v, "5-Hour Rate Limit (%)", y_label_percent)
            }
            SlChartMode::SevenDay => {
                let tz = tz_opt.as_deref();
                let k: Vec<String> = rl_entries.iter().map(|e| {
                    let dt = &e.ts;
                    match tz {
                        Some("UTC") => dt.format("%m-%dT%H:%M").to_string(),
                        _ => dt.with_timezone(&chrono::Local).format("%m-%dT%H:%M").to_string(),
                    }
                }).collect();
                let v: Vec<f64> = rl_entries.iter().map(|e| e.seven_day_pct as f64).collect();
                (k, v, "7-Day Rate Limit (%)", y_label_percent)
            }
            SlChartMode::Cost => {
                // Cumulative cost over time — use session summaries sorted by last_ts
                let mut sessions = aggregate_sessions(&records);
                sessions.sort_by_key(|s| s.last_ts);
                let tz = tz_opt.as_deref();
                let mut cumulative = 0.0;
                let mut k = Vec::new();
                let mut v = Vec::new();
                for s in &sessions {
                    cumulative += s.total_cost;
                    let label = match tz {
                        Some("UTC") => s.last_ts.format("%m-%dT%H:%M").to_string(),
                        _ => s.last_ts.with_timezone(&chrono::Local).format("%m-%dT%H:%M").to_string(),
                    };
                    k.push(label);
                    v.push(cumulative);
                }
                (k, v, "Cumulative Cost ($)", y_label_cost)
            }
        };

        if keys.is_empty() {
            eprintln!("No data to chart.");
            return;
        }

        let chart_output = render_chart_raw(&keys, &values, title, y_fn, None, None);

        if output_format.is_some() || filename_opt.is_some() {
            let ext = output_format.as_deref().unwrap_or("txt");
            let target = filename_opt.unwrap_or_else(|| format!("cctokens-sl.{}", ext));
            if let Err(e) = fs::write(&target, &chart_output) {
                eprintln!("Error writing to '{}': {}", target, e);
                process::exit(1);
            }
            eprintln!("Wrote chart to {}", target);
        } else {
            print!("{}", chart_output);
        }

        if let Some(ref copy_fmt) = copy_format {
            if copy_fmt == "txt" {
                match copy_to_clipboard(&chart_output) {
                    Ok(()) => eprintln!("Copied chart to clipboard"),
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
        }

        return;
    }

    // ── Non-chart mode ──
    // Generate table content for current view
    let generate_table = |color: bool| -> String {
        let opts = if color { &fmt_opts } else { &fmt_opts_nocolor };
        match view_mode {
            SlViewMode::RateLimit => {
                let entries = aggregate_ratelimit(&records);
                if entries.is_empty() {
                    return "No rate limit data found.\n".to_string();
                }
                format_sl_ratelimit_table(&entries, opts)
            }
            SlViewMode::Session => {
                let mut sessions = aggregate_sessions(&records);
                if order == SortOrder::Desc {
                    sessions.sort_by(|a, b| b.total_cost.partial_cmp(&a.total_cost).unwrap_or(std::cmp::Ordering::Equal));
                }
                if cost_diff {
                    let diffs = compute_cost_diffs(&sessions, matches);
                    format_sl_cost_diff_table(&sessions, &diffs, opts)
                } else {
                    format_sl_session_table(&sessions, opts)
                }
            }
            SlViewMode::Project => {
                let sessions = aggregate_sessions(&records);
                let projects = aggregate_by_project(&sessions);
                format_sl_project_table(&projects, opts)
            }
            SlViewMode::Day => {
                let sessions = aggregate_sessions(&records);
                let days = aggregate_by_day(&sessions, tz_opt.as_deref());
                format_sl_day_table(&days, opts)
            }
            SlViewMode::Window => {
                let sessions = aggregate_sessions(&records);
                let windows = aggregate_windows(&records, &sessions);
                format_sl_window_table(&windows, opts)
            }
        }
    };

    let generate_format_content = |fmt: &str| -> String {
        match fmt {
            "json" => {
                match view_mode {
                    SlViewMode::RateLimit => {
                        let entries = aggregate_ratelimit(&records);
                        format_sl_json_ratelimit(&entries, &json_meta)
                    }
                    SlViewMode::Session => {
                        let sessions = aggregate_sessions(&records);
                        format_sl_json_sessions(&sessions, &json_meta)
                    }
                    SlViewMode::Project => {
                        let sessions = aggregate_sessions(&records);
                        let projects = aggregate_by_project(&sessions);
                        format_sl_json_projects(&projects, &json_meta)
                    }
                    SlViewMode::Day => {
                        let sessions = aggregate_sessions(&records);
                        let days = aggregate_by_day(&sessions, tz_opt.as_deref());
                        format_sl_json_days(&days, &json_meta)
                    }
                    SlViewMode::Window => {
                        let sessions = aggregate_sessions(&records);
                        let windows = aggregate_windows(&records, &sessions);
                        format_sl_json_windows(&windows, &json_meta)
                    }
                }
            }
            "csv" => {
                match view_mode {
                    SlViewMode::RateLimit => {
                        let entries = aggregate_ratelimit(&records);
                        format_sl_csv_ratelimit(&entries, tz_opt.as_deref())
                    }
                    SlViewMode::Session => {
                        let sessions = aggregate_sessions(&records);
                        format_sl_csv_sessions(&sessions, &fmt_opts_nocolor)
                    }
                    _ => generate_table(false), // fallback to text table
                }
            }
            "txt" => generate_table(false),
            _ => generate_table(false),
        }
    };

    // Handle --copy
    if let Some(ref copy_fmt) = copy_format {
        let content = generate_format_content(copy_fmt);
        match copy_to_clipboard(&content) {
            Ok(()) => eprintln!("Copied {} to clipboard", copy_fmt),
            Err(e) => eprintln!("Error: {}", e),
        }
    }

    // Handle --output / --filename
    if output_format.is_some() || filename_opt.is_some() {
        let fmt = output_format.as_deref().unwrap_or("txt");
        let content = generate_format_content(fmt);
        let ext = ext_for_format(fmt);
        let target = filename_opt.unwrap_or_else(|| format!("cctokens-sl.{}", ext));
        if let Err(e) = fs::write(&target, &content) {
            eprintln!("Error writing to '{}': {}", target, e);
            process::exit(1);
        }
        eprintln!("Wrote report to {}", target);
        return;
    }

    // Default: print table to stdout
    print!("{}", generate_table(true));
}

/// Compute cost diffs by loading conversation JSONL and comparing.
fn compute_cost_diffs(sessions: &[SlSessionSummary], matches: &clap::ArgMatches) -> Vec<SlCostDiff> {
    use cctokens::*;

    let load_opts = LoadOptions {
        claude_dir: matches.get_one::<String>("claude-dir").cloned(),
        from: matches.get_one::<String>("from").cloned(),
        to: matches.get_one::<String>("to").cloned(),
        tz: matches.get_one::<String>("tz").cloned(),
        project: matches.get_one::<String>("project").cloned(),
        model: matches.get_one::<String>("model").cloned(),
        session: matches.get_one::<String>("session").cloned(),
    };

    let pricing = if matches.get_flag("live-pricing") {
        match fetch_live_pricing() {
            Ok(p) => p,
            Err(_) => load_pricing(),
        }
    } else if let Some(path) = matches.get_one::<String>("pricing-data") {
        load_pricing_from_file(path).unwrap_or_else(|_| load_pricing())
    } else {
        load_pricing()
    };

    let result = load_records(&load_opts);
    let priced = calculate_cost(&result.records, Some(&pricing));

    // Sum cost per session
    let mut session_costs: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for r in &priced {
        *session_costs.entry(r.session_id.clone()).or_insert(0.0) += r.total_cost;
    }

    sessions
        .iter()
        .map(|s| {
            // Match by full session_id or prefix
            let litellm_cost = session_costs.get(&s.session_id).copied()
                .or_else(|| {
                    // Try matching by prefix
                    let prefix = &s.session_id[..8.min(s.session_id.len())];
                    session_costs.iter()
                        .find(|(k, _)| k.starts_with(prefix))
                        .map(|(_, v)| *v)
                });

            let (diff, diff_pct) = if let Some(lc) = litellm_cost {
                let d = s.total_cost - lc;
                let pct = if lc.abs() > 0.0001 { Some(d / lc * 100.0) } else { None };
                (Some(d), pct)
            } else {
                (None, None)
            };

            SlCostDiff {
                session_id: s.session_id.clone(),
                sl_cost: s.total_cost,
                litellm_cost,
                diff,
                diff_pct,
            }
        })
        .collect()
}
```

- [ ] **Step 2: Update help text to mention sl subcommand**

In the `print_help()` function, add at the end before the closing `"#;`:

```
  sl                    Analyze statusline.jsonl (run 'cctokens sl --help' for details)
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`
Expected: Compiles successfully.

- [ ] **Step 4: Run all tests**

Run: `cargo test`
Expected: All tests pass (existing + new).

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat(sl): integrate sl subcommand into CLI"
```

---

### Task 8: Integration tests for cctokens sl

**Files:**
- Create: `tests/cli_sl.rs`

- [ ] **Step 1: Create test fixture**

Create `tests/cli_sl.rs`:

```rust
use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;
use tempfile::NamedTempFile;

fn make_sl_line(ts: i64, session: &str, cost: f64, dur_ms: u64, api_ms: u64, lines_add: u64, lines_rem: u64, five_h: Option<u8>, seven_d: Option<u8>, resets_5h: Option<i64>, resets_7d: Option<i64>) -> String {
    let rl = if let (Some(fh), Some(sd), Some(r5), Some(r7)) = (five_h, seven_d, resets_5h, resets_7d) {
        format!(r#","rate_limits":{{"five_hour":{{"used_percentage":{},"resets_at":{}}},"seven_day":{{"used_percentage":{},"resets_at":{}}}}}"#, fh, r5, sd, r7)
    } else {
        String::new()
    };
    format!(
        r#"{{"ts":{},"data":{{"session_id":"{}","workspace":{{"project_dir":"/home/user/proj","current_dir":"/home/user/proj","added_dirs":[]}},"model":{{"id":"claude-opus-4-6[1m]","display_name":"Opus 4.6"}},"version":"2.1.84","cost":{{"total_cost_usd":{},"total_duration_ms":{},"total_api_duration_ms":{},"total_lines_added":{},"total_lines_removed":{}}},"context_window":{{"total_input_tokens":100,"total_output_tokens":50,"context_window_size":1000000,"current_usage":null,"used_percentage":2,"remaining_percentage":98}},"exceeds_200k_tokens":false{}}}}}"#,
        ts, session, cost, dur_ms, api_ms, lines_add, lines_rem, rl
    )
}

fn create_test_file() -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    // Session 1: single segment, with rate limits
    writeln!(f, "{}", make_sl_line(1774481258, "sess-aaaa-1111", 0.0, 100, 0, 0, 0, Some(2), Some(63), Some(1774497600), Some(1774605600))).unwrap();
    writeln!(f, "{}", make_sl_line(1774481270, "sess-aaaa-1111", 0.5, 1000, 500, 10, 2, Some(3), Some(63), Some(1774497600), Some(1774605600))).unwrap();
    writeln!(f, "{}", make_sl_line(1774481280, "sess-aaaa-1111", 1.0, 2000, 1000, 20, 5, Some(4), Some(63), Some(1774497600), Some(1774605600))).unwrap();
    // Session 2: with reset (segment boundary)
    writeln!(f, "{}", make_sl_line(1774481300, "sess-bbbb-2222", 0.0, 100, 0, 0, 0, Some(4), Some(63), Some(1774497600), Some(1774605600))).unwrap();
    writeln!(f, "{}", make_sl_line(1774481400, "sess-bbbb-2222", 2.0, 5000, 2000, 50, 10, Some(5), Some(64), Some(1774497600), Some(1774605600))).unwrap();
    // Reset
    writeln!(f, "{}", make_sl_line(1774490000, "sess-bbbb-2222", 0.0, 50, 0, 0, 0, Some(1), Some(64), Some(1774515600), Some(1774605600))).unwrap();
    writeln!(f, "{}", make_sl_line(1774490100, "sess-bbbb-2222", 0.5, 1000, 500, 5, 1, Some(2), Some(64), Some(1774515600), Some(1774605600))).unwrap();
    f
}

#[test]
fn test_sl_default_ratelimit() {
    let f = create_test_file();
    Command::cargo_bin("cctokens").unwrap()
        .args(["sl", "--file", f.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("5h%"))
        .stdout(predicate::str::contains("7d%"));
}

#[test]
fn test_sl_per_session() {
    let f = create_test_file();
    Command::cargo_bin("cctokens").unwrap()
        .args(["sl", "--file", f.path().to_str().unwrap(), "--per", "session", "--cost", "decimal"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sess-aaa"))
        .stdout(predicate::str::contains("$1.00"))
        .stdout(predicate::str::contains("$2.50")); // 2.0 + 0.5 from reset
}

#[test]
fn test_sl_per_window() {
    let f = create_test_file();
    Command::cargo_bin("cctokens").unwrap()
        .args(["sl", "--file", f.path().to_str().unwrap(), "--per", "window", "--cost", "decimal"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Peak 5h%"))
        .stdout(predicate::str::contains("Est Budget"));
}

#[test]
fn test_sl_chart_5h() {
    let f = create_test_file();
    Command::cargo_bin("cctokens").unwrap()
        .args(["sl", "--file", f.path().to_str().unwrap(), "--chart", "5h"])
        .assert()
        .success()
        .stdout(predicate::str::contains("5-Hour Rate Limit"));
}

#[test]
fn test_sl_json_output() {
    let f = create_test_file();
    let output = Command::cargo_bin("cctokens").unwrap()
        .args(["sl", "--file", f.path().to_str().unwrap(), "--per", "session", "--output", "json", "--filename", "/dev/stdout"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["meta"]["source"], "statusline");
    assert_eq!(json["meta"]["view"], "sessions");
    assert!(json["data"].as_array().unwrap().len() >= 2);
}

#[test]
fn test_sl_file_not_found() {
    Command::cargo_bin("cctokens").unwrap()
        .args(["sl", "--file", "/nonexistent/statusline.jsonl"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_sl_help() {
    Command::cargo_bin("cctokens").unwrap()
        .args(["sl", "--help"])
        .assert()
        .success()
        .stderr(predicate::str::contains("statusline"));
}

#[test]
fn test_sl_invalid_per() {
    let f = create_test_file();
    Command::cargo_bin("cctokens").unwrap()
        .args(["sl", "--file", f.path().to_str().unwrap(), "--per", "invalid"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid dimension"));
}

#[test]
fn test_sl_session_filter() {
    let f = create_test_file();
    Command::cargo_bin("cctokens").unwrap()
        .args(["sl", "--file", f.path().to_str().unwrap(), "--per", "session", "--session", "aaaa", "--cost", "decimal"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sess-aaa"))
        .stdout(predicate::str::contains("$1.00").count(1));
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test --test cli_sl`
Expected: All tests pass.

- [ ] **Step 3: Run full test suite**

Run: `cargo test`
Expected: All tests pass (existing 169 + new sl tests).

- [ ] **Step 4: Commit**

```bash
git add tests/cli_sl.rs
git commit -m "test(sl): add integration tests for cctokens sl subcommand"
```

---

### Task 9: Manual smoke test with real data

**Files:** None (verification only)

- [ ] **Step 1: Test default ratelimit view**

Run: `cargo run --release -- sl`
Expected: Rate limit timeline table printed to stdout.

- [ ] **Step 2: Test session view**

Run: `cargo run --release -- sl --per session --cost decimal`
Expected: Session summaries with segment-aware costs.

- [ ] **Step 3: Test window view**

Run: `cargo run --release -- sl --per window --cost decimal`
Expected: 5h window summaries with estimated budgets.

- [ ] **Step 4: Test chart**

Run: `cargo run --release -- sl --chart 5h`
Expected: Braille chart of 5h% over time.

- [ ] **Step 5: Test cost-diff**

Run: `cargo run --release -- sl --per session --cost-diff --cost decimal`
Expected: Two cost columns with diff percentage.

- [ ] **Step 6: Test JSON output**

Run: `cargo run --release -- sl --per session --output json --filename /dev/stdout`
Expected: Valid JSON with meta.source = "statusline".

- [ ] **Step 7: Verify existing cctokens still works**

Run: `cargo run --release -- --per day --cost decimal`
Expected: Same output as before (no regression).
