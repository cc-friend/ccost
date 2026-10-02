# CLAUDE.md

## Overview

CLI and library: reads Claude Code JSONL from `~/.claude/projects/` and `~/.config/claude/projects/`, deduplicates, calculates costs (LiteLLM pricing), groups by configurable dimensions, outputs table/JSON/markdown/HTML/chart.

Also includes `ccost sl` subcommand: reads `~/.claude/statusline.jsonl` for rate limit tracking, session summaries, 5h budget estimation, and cost cross-comparison.

## Architecture

Main pipeline: `load_records → calculate_cost → group_records → format`

sl pipeline: `load_sl_records → filter → aggregate (segment-aware) → format`

`src/` is split into pure library (`lib.rs`, `types.rs`, `parser.rs`, `pricing.rs`, `grouper.rs`, `formatters/`) and CLI (`main.rs`).

- `src/sl/` — statusline analysis module: `types.rs`, `parser.rs`, `aggregator.rs`, `formatter.rs`
- `src/utils.rs` — shared utilities (clipboard, date parsing, terminal width, `parse_fixed_offset`)

## Key design decisions

**Dedup: `messageId:requestId` keep-max-output, global across all files.** Streaming entries have monotonically increasing `output_tokens`; keeping max is order-independent. Same composite key appears in both parent and `subagents/` files — global dedup prevents double-counting. Subagent files are found in both old (`<project>/subagents/<session>_<agent>.jsonl`) and new (`<project>/<session-uuid>/subagents/<agent>.jsonl`) directory structures; `extract_session_and_agent` maps new-structure files to the parent session UUID (via grandparent directory name) and extracts the agent file stem as `agent_id`.

**Subagent tracking:** Each `TokenRecord` and `PricedTokenRecord` carries an `agent_id` field — empty string for main session records, the subagent file stem (e.g. `agent-a0f53f284339341b2`) for subagent records. `GroupDimension::Subagent` groups by this field, labeling main session records as `(main)`.

**Tool & line tracking:** Each record carries `tool_names: String` (comma-joined sorted unique tool names from `tool_use` content blocks, empty for text-only messages) and `line: u32` (**1-based physical line number** in the source JSONL — counts every line including empty ones and lines that fail to parse, so the value matches `vim +N`, `sed -n 'Np'`, GitHub blob `#LN`, and any external join key keyed on physical lines). Content is parsed with lightweight `RawContentItem` that only deserializes `type` and `name` fields — `input`, `text`, etc. are skipped by serde for performance. `GroupDimension::Tool` labels: tool names or `(text)`. `GroupDimension::Line` labels: `#N` (physical line number).

**Dedup-key fields exposed:** `TokenRecord` and `PricedTokenRecord` carry `message_id: Option<String>` (= `message.id`) and `request_id: Option<String>` (= `requestId`) — the same composite key dedup is keyed on. Cross-tool consumers should join on `(message_id, request_id)` rather than on `line`, because dedup-key joins survive any future change to line-numbering policy or to which entry types ccost surfaces.

**Directory-level pre-filter (`LoadOptions.project_dirs`):** When set, `discover_project_dirs` only descends into the named encoded subdirectories under `<claude_dir>/projects/` — every other project is skipped before any IO. Names are the on-disk encoded form (e.g. `-home-user-my-project`, what ccfriend has as `Project.dirName`). `None` (default) preserves legacy "load every project" behavior; an explicit empty `Vec` means "load nothing". Composes with the existing `project` post-filter, which still substring-matches against `record.project` (the human-readable form) after load. Use `project_dirs` when you want the IO speedup; use `project` when you only want the records narrowed. CLI: `--project-dirs <comma_encoded_dirs>` (uses `allow_hyphen_values` because encoded dir names start with `-`).

**File-level pre-filter (`LoadOptions.session_files`):** When set, only JSONL files whose owning session UUID is in the list are read — every other file is skipped before parse. The match honors all three on-disk layouts via `extract_session_and_agent`: main `<project>/<uuid>.jsonl`, new subagent `<project>/<uuid>/subagents/<agent>.jsonl`, and old subagent `<project>/subagents/<uuid>_<agent>.jsonl`. Listing one UUID transparently pulls in its subagents (new layout); old subagent layout requires listing the full file stem because `extract_session_and_agent` returns the entire stem as `session_id`. Composes with `project_dirs` and the `session` post-filter. CLI: `--session-files <comma_uuids>`.

**Pricing: per-record pricing before grouping.** Each record is priced with its model's rates before grouping; the effective per-record rate depends on the record's 5m/1h cache-write split, fast mode (`speed`) and inference geo (see "Refined cost formula"). Supports `--live-pricing` (runtime fetch via minreq) and `--pricing-data <path>` (custom file). Bundled pricing embedded via `include_str!`.

**Pricing precedence: LiteLLM snapshot < official overrides < user file.** `src/pricing-data.json` is the LiteLLM snapshot (rewritten only by `scripts/update-pricing.rs`, which calls the raw `fetch_litellm_pricing()`). `src/pricing-overrides.json` holds hand-maintained Anthropic list prices (undated ids for current Claude models plus dated aliases `claude-opus-4-5-20251101` / `claude-sonnet-4-5-20250929` / `claude-haiku-4-5-20251001` so they win tier 1 over LiteLLM's dated entries, `cacheCreation1hCostPerToken`, `fastModeMultiplier` on Opus 5.5 / 5 / 4.8, zero-rate `<synthetic>`); edit it by hand when prices change. `load_pricing()` and `fetch_live_pricing()` merge the overrides on top (whole-entry replace; `fetched_at` = newer ISO string). `load_pricing_from_file()` then puts the user file on top (whole-entry replace; a non-empty user `fetchedAt` replaces the merged one). `ModelPricing`'s two optional fields use `serde(default, skip_serializing_if = "Option::is_none")`, so pre-0.6 pricing JSON round-trips unchanged.

**Refined cost formula (mirrors ccfriend `src/lib/cost.ts`).** `TokenRecord`/`PricedTokenRecord` keep `cache_creation_tokens` as the total and add `cache_creation_5m_tokens` / `cache_creation_1h_tokens` (from `usage.cache_creation.ephemeral_{5m,1h}_input_tokens`; no split → all 5m; `c5m = split_5m + max(0, total − split_5m − c1h)`), `speed`, `inference_geo`. Cost = `in×in_rate + out×out_rate + read×read_rate + c5m×cacheCreationCostPerToken + c1h×cache_creation_1h_rate()` (field or 2×input), then every component ×`fast_mode_multiplier` when `speed == "fast"` and the model has one, then ×`US_INFERENCE_GEO_MULTIPLIER` (1.1) when `inference_geo == "us"`. `GroupedData` keeps its four cost columns (the 5m/1h split is folded into `cache_creation_cost`). `<synthetic>` rows are still skipped by the parser.

**Deterministic model matching.** `match_model_name`: raw exact first; tier 0 strips a trailing `(?i)\[1m\]` (1M context, standard rate); tiers 1–3 exact / date-stripped / case-insensitive (tier 3 picks the smallest key among case variants); tier 4 = case-insensitive substring containment either way, **only keys with an ASCII digit**, longest key wins, ties by lexicographic key order. Never depends on `HashMap` iteration order; LiteLLM's bare `claude` / `claude-instant` keys cannot catch unknown ids. `shorten_model_name` renders `…[1m]` as `<short> [1m]`.

**Project paths:** `-home-username-workspace-test` → `/home/username/workspace/test`. Filter with `--project /home/username/workspace/test`.

**Timezone handling:** Uses `chrono` + `chrono-tz` for IANA timezone support, `chrono::FixedOffset` for +HH:MM offsets, `chrono::Local` for system timezone.

**`--cost-diff` requires `--per session`.** Hard error if used with other views.

**Cost-diff totals are like-for-like:** Only matched sessions (those with both SL and LiteLLM costs) are included in totals. Prefix matching uses deterministic longest-match selection.

**sl optional fields (Claude Code 2.1.285):** `SlRecord` has `effort` (`effort.level`), `session_name`, `fast_mode`, `prompt_cache_ttl`, `prompt_cache_hit_ratio`, `spend_limit_pct` / `spend_limit_resets_at` (`rate_limits.spend_limit.*`), `exceeds_200k` (`exceeds_200k_tokens`). The raw structs keep them as `serde_json::Value` and read them leniently (wrong type or missing → `None`), so a new or changed shape never drops a line. `SlSessionSummary.session_name` = last non-empty value by timestamp, `skip_serializing_if = "Option::is_none"` so JSON is unchanged without it. Tables don't show any of them.

**sl action view EOF flushing:** `aggregate_ratelimit` skips consecutive records with same (5h%, 7d%) pair. After the main loop, remaining unaccounted cost deltas are flushed to each session's last emitted entry to avoid cost loss.

**sl segment-aware aggregation:** statusline.jsonl has cumulative counters (cost, duration, tokens, lines) that reset to 0 on session resume. Detect resets by comparing consecutive records; sum max-per-segment across segments. For continued sessions (`claude --continue`), cumulative counters carry over from the predecessor without resetting; `aggregate_sessions` subtracts the first record's values as baseline to exclude inherited costs. Window aggregation (`segment_totals_before`) returns the session baseline (not zeros) when no records exist before a time point, ensuring delta computation correctly handles continued sessions.

**sl unified table columns:** All `--per` views (session/project/day/1h/5h/1w) share the same column set: `[Label] | Cost | Duration | API Time | Lines +/- | Sess | 5h% | 1w%`. `--per 1h`/`--per 5h` add `Est 5h Budg` (= cost × 100 / Δ5h%); `--per 1w` adds `Est 1w Budg`. `--per 1h` also adds `5h Resets`. `--per action` includes `Cost` (delta). `5h%` and `1w%` show min–max ranges. Compact mode: `[Label] | Cost | Duration | Sess | 5h%`.

**sl promo adjustment (default on):** Adjusts Est 5h/1w Budget for known 2x usage promo periods (2025-12, 2026-03). Hardcoded UTC intervals in `aggregator.rs`. Adjustment: `adjusted_delta = delta_pct × (1 + promo_overlap_ratio)`. Disable with `--nopromo`.

**sl JSON field naming:** `SlWindowSummary` has separate `est_5h_budget` and `est_1w_budget` fields (serialized as `est5hBudget`/`est1wBudget`). For 1h/5h views, `est_5h_budget` is populated; for 1w views, `est_1w_budget` is populated. The other is `null`.

**`--table auto` behavior:** When writing to file (`--output`/`--filename`), auto selects full table. When printing to terminal, auto selects compact if width < 120.

**Hour key format:** Grouper emits `%Y-%m-%d %H:00` (space separator). Chart `auto_granularity_from_labels` detects hour labels by checking `len > 10 && contains(':')` (matches both space and T separators).

**Types:** `GroupDimension` and `SortOrder` implement `std::str::FromStr` (returns `Result<Self, String>`). Use `"day".parse::<GroupDimension>()` or `GroupDimension::from_str("day")`. Valid dimensions: `day`, `hour`, `month`, `session`, `project`, `model`, `subagent`, `tool`, `line`. CLI supports `-h`/`-V` short flags in addition to `--help`/`--version`.

## Commands

```bash
cargo test                  # all tests
cargo build --release       # optimized build
cargo run -- --help         # show CLI help
cargo run -- sl --help      # show sl subcommand help
```

## Maintenance rules

**Every code change must also update, if applicable:**
- **Tests** — add or update tests to cover the change
- **README.md** — update documentation, CLI reference, examples
- **CLAUDE.md** — update this file if architecture, design decisions, or conventions change
- **clippy** — `cargo clippy -- -W clippy::all` must produce zero warnings
