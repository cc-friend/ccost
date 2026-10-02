use chrono::{DateTime, Utc};
use serde::Serialize;

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
    // ── Optional fields added by newer Claude Code versions (2.1.2xx+). All
    // `None` when absent or of an unexpected type; never fail the record.
    /// `effort.level` (e.g. `"high"`).
    pub effort: Option<String>,
    /// `session_name` (the `/title`-style session name), verbatim.
    pub session_name: Option<String>,
    /// `fast_mode`.
    pub fast_mode: Option<bool>,
    /// `prompt_cache.ttl` (e.g. `"5m"`, `"1h"`).
    pub prompt_cache_ttl: Option<String>,
    /// `prompt_cache.hit_ratio` (0–1).
    pub prompt_cache_hit_ratio: Option<f64>,
    /// `rate_limits.spend_limit.used_percentage` (unrounded).
    pub spend_limit_pct: Option<f64>,
    /// `rate_limits.spend_limit.resets_at` (unix seconds → UTC).
    pub spend_limit_resets_at: Option<DateTime<Utc>>,
    /// `exceeds_200k_tokens`.
    pub exceeds_200k: Option<bool>,
}

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
    pub min_five_hour_pct: Option<u8>,
    pub max_five_hour_pct: Option<u8>,
    pub min_seven_day_pct: Option<u8>,
    pub max_seven_day_pct: Option<u8>,
    /// Last non-empty `session_name` seen in the session (by timestamp).
    /// Omitted from JSON when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlRateLimitEntry {
    pub ts: DateTime<Utc>,
    pub session_id: String,
    pub cost_delta: f64,
    pub five_hour_pct: u8,
    pub five_hour_resets_at: DateTime<Utc>,
    pub seven_day_pct: u8,
    pub seven_day_resets_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlWindowSummary {
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub min_five_hour_pct: u8,
    pub max_five_hour_pct: u8,
    pub sessions: u32,
    pub total_cost: f64,
    pub est_5h_budget: Option<f64>,
    pub est_1w_budget: Option<f64>,
    pub total_duration_ms: u64,
    pub total_api_duration_ms: u64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub min_seven_day_pct: Option<u8>,
    pub max_seven_day_pct: Option<u8>,
    /// For 1h windows: the parent 5h window's reset time
    pub five_hour_resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlProjectSummary {
    pub project: String,
    pub total_cost: f64,
    pub total_duration_ms: u64,
    pub total_api_duration_ms: u64,
    pub session_count: u32,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
    pub min_five_hour_pct: Option<u8>,
    pub max_five_hour_pct: Option<u8>,
    pub min_seven_day_pct: Option<u8>,
    pub max_seven_day_pct: Option<u8>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlDaySummary {
    pub date: String,
    pub total_cost: f64,
    pub session_count: u32,
    pub min_five_hour_pct: Option<u8>,
    pub max_five_hour_pct: Option<u8>,
    pub min_seven_day_pct: Option<u8>,
    pub max_seven_day_pct: Option<u8>,
    pub total_duration_ms: u64,
    pub total_api_duration_ms: u64,
    pub total_lines_added: u64,
    pub total_lines_removed: u64,
}

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlCostDiff {
    pub session_id: String,
    pub sl_cost: f64,
    pub litellm_cost: Option<f64>,
    pub diff: Option<f64>,
    pub diff_pct: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlViewMode {
    Action,
    Session,
    Project,
    Day,
    Window1h,
    Window5h,
    Window1w,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlChartMode {
    FiveHour,
    OneWeek,
    Cost,
}
