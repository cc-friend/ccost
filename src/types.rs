use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct TokenRecord {
    pub timestamp: DateTime<Utc>,
    pub model: String,
    pub session_id: String,
    pub project: String,
    pub agent_id: String,
    pub tool_names: String,
    /// 1-based **physical** line number in the source JSONL file.
    /// Counts every line including empty lines and lines that fail to parse,
    /// so the value lines up with `vim +N`, `sed -n 'Np'`, GitHub blob `#LN`,
    /// and any external tool that operates on physical line numbers.
    pub line: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    /// Anthropic-assigned message id (`message.id` in the JSONL). `None` only
    /// for records that omit it. Together with `request_id`, this is the
    /// dedup key — stable across re-emits and safe as a cross-tool join key.
    pub message_id: Option<String>,
    /// API request id (`requestId` in the JSONL). See `message_id` for usage.
    pub request_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PricedTokenRecord {
    pub timestamp: DateTime<Utc>,
    pub model: String,
    pub session_id: String,
    pub project: String,
    pub agent_id: String,
    pub tool_names: String,
    pub line: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub message_id: Option<String>,
    pub request_id: Option<String>,
    pub input_cost: f64,
    pub cache_creation_cost: f64,
    pub cache_read_cost: f64,
    pub output_cost: f64,
    pub total_cost: f64,
}

impl PricedTokenRecord {
    pub fn from_token_record(
        record: &TokenRecord,
        input_cost: f64,
        cache_creation_cost: f64,
        cache_read_cost: f64,
        output_cost: f64,
    ) -> Self {
        Self {
            timestamp: record.timestamp,
            model: record.model.clone(),
            session_id: record.session_id.clone(),
            project: record.project.clone(),
            agent_id: record.agent_id.clone(),
            tool_names: record.tool_names.clone(),
            line: record.line,
            input_tokens: record.input_tokens,
            output_tokens: record.output_tokens,
            cache_creation_tokens: record.cache_creation_tokens,
            cache_read_tokens: record.cache_read_tokens,
            message_id: record.message_id.clone(),
            request_id: record.request_id.clone(),
            input_cost,
            cache_creation_cost,
            cache_read_cost,
            output_cost,
            total_cost: input_cost + cache_creation_cost + cache_read_cost + output_cost,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupedData {
    pub label: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub input_cost: f64,
    pub cache_creation_cost: f64,
    pub cache_read_cost: f64,
    pub output_cost: f64,
    pub total_cost: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<GroupedData>>,
}

/// Per-token USD rates for one model.
///
/// `cache_creation_cost_per_token` is the **5-minute** prompt-cache write
/// rate. The two optional fields are omitted from JSON when `None`, so
/// pricing files written before 0.6.0 round-trip unchanged.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPricing {
    pub cache_creation_cost_per_token: f64,
    pub cache_read_cost_per_token: f64,
    pub input_cost_per_token: f64,
    pub output_cost_per_token: f64,
    /// 1-hour prompt-cache write rate (`cacheCreation1hCostPerToken`).
    /// `None` → [`ModelPricing::cache_creation_1h_rate`] falls back to
    /// `2 × input_cost_per_token` (Anthropic's 1h write multiplier).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_1h_cost_per_token: Option<f64>,
    /// Multiplier applied to every cost component of a record whose
    /// `usage.speed == "fast"` (`fastModeMultiplier`). `None` → the model
    /// has no fast-mode pricing and the flag is ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fast_mode_multiplier: Option<f64>,
}

impl ModelPricing {
    /// Effective 1-hour cache write rate: the explicit field when present,
    /// otherwise `2 × input_cost_per_token`.
    pub fn cache_creation_1h_rate(&self) -> f64 {
        self.cache_creation_1h_cost_per_token
            .unwrap_or(2.0 * self.input_cost_per_token)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingData {
    #[serde(default)]
    pub fetched_at: String,
    pub models: HashMap<String, ModelPricing>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroupDimension {
    Day,
    Hour,
    Month,
    Session,
    Project,
    Model,
    Subagent,
    Tool,
    Line,
}

impl std::str::FromStr for GroupDimension {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "day" => Ok(Self::Day),
            "hour" => Ok(Self::Hour),
            "month" => Ok(Self::Month),
            "session" => Ok(Self::Session),
            "project" => Ok(Self::Project),
            "model" => Ok(Self::Model),
            "subagent" => Ok(Self::Subagent),
            "tool" => Ok(Self::Tool),
            "line" => Ok(Self::Line),
            _ => Err(format!("invalid group dimension: '{}'", s)),
        }
    }
}

impl GroupDimension {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Hour => "hour",
            Self::Month => "month",
            Self::Session => "session",
            Self::Project => "project",
            Self::Model => "model",
            Self::Subagent => "subagent",
            Self::Tool => "tool",
            Self::Line => "line",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Day => "Date",
            Self::Hour => "Hour",
            Self::Month => "Month",
            Self::Session => "Session",
            Self::Project => "Project",
            Self::Model => "Model",
            Self::Subagent => "Subagent",
            Self::Tool => "Tool",
            Self::Line => "Line",
        }
    }

    pub fn all_valid() -> &'static [&'static str] {
        &[
            "day", "hour", "month", "session", "project", "model", "subagent", "tool", "line",
        ]
    }
}

impl std::fmt::Display for GroupDimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Asc,
    Desc,
}

impl std::str::FromStr for SortOrder {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "asc" => Ok(Self::Asc),
            "desc" => Ok(Self::Desc),
            _ => Err(format!("invalid sort order: '{}'", s)),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    pub claude_dir: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub tz: Option<String>,
    pub project: Option<String>,
    pub model: Option<String>,
    pub session: Option<String>,
    /// Directory-level pre-filter. When `Some(list)`, only the named
    /// project subdirectories under `<claude_dir>/projects/` are
    /// scanned — every other project is skipped before any IO. Names
    /// are the **encoded** on-disk form (e.g. `-home-user-my-project`,
    /// matching `Project.dirName` in ccfriend), not the human-readable
    /// project path. An explicit empty `Vec` means "load nothing"; the
    /// default `None` preserves the legacy behavior of scanning every
    /// project.
    ///
    /// This is the load-time analog of the `project` field, which is a
    /// post-load filter on `record.project`. The two compose: a query
    /// can name its directories explicitly **and** still substring-
    /// filter the resulting records.
    pub project_dirs: Option<Vec<String>>,
    /// File-level pre-filter by session UUID. When `Some(list)`, only
    /// JSONL files whose owning session id is in the list are read —
    /// every other file is skipped before parse. The match honors all
    /// three on-disk layouts via `extract_session_and_agent`:
    ///
    ///   - main session: `<project>/<uuid>.jsonl`
    ///   - new subagent: `<project>/<uuid>/subagents/<agent>.jsonl`
    ///   - old subagent: `<project>/subagents/<uuid>_<agent>.jsonl`
    ///
    /// All three resolve to the same `session_id`, so listing one
    /// UUID transparently pulls in its subagents too.
    ///
    /// Composes with `project_dirs` (which restricts which directories
    /// are even discovered) and with the `session` post-filter (which
    /// substring-matches `record.session_id` after load).
    pub session_files: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DedupStats {
    pub before: usize,
    pub after: usize,
}

#[derive(Debug, Clone)]
pub struct RecordsMeta {
    pub earliest: Option<DateTime<Utc>>,
    pub latest: Option<DateTime<Utc>>,
    pub projects: Vec<String>,
    pub models: Vec<String>,
    pub sessions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceMode {
    Off,
    Integer,
    Decimal,
}

pub struct LoadResult {
    pub records: Vec<TokenRecord>,
    pub dedup: DedupStats,
    pub meta: RecordsMeta,
}

pub struct GroupResult {
    pub data: Vec<GroupedData>,
    pub totals: GroupedData,
}

pub struct GroupOptions {
    pub order: SortOrder,
    pub tz: Option<String>,
}

impl Default for GroupOptions {
    fn default() -> Self {
        Self {
            order: SortOrder::Asc,
            tz: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- GroupDimension::from_str ---

    #[test]
    fn test_group_dimension_from_str_all_valid() {
        assert_eq!(
            "day".parse::<GroupDimension>().unwrap(),
            GroupDimension::Day
        );
        assert_eq!(
            "hour".parse::<GroupDimension>().unwrap(),
            GroupDimension::Hour
        );
        assert_eq!(
            "month".parse::<GroupDimension>().unwrap(),
            GroupDimension::Month
        );
        assert_eq!(
            "session".parse::<GroupDimension>().unwrap(),
            GroupDimension::Session
        );
        assert_eq!(
            "project".parse::<GroupDimension>().unwrap(),
            GroupDimension::Project
        );
        assert_eq!(
            "model".parse::<GroupDimension>().unwrap(),
            GroupDimension::Model
        );
        assert_eq!(
            "subagent".parse::<GroupDimension>().unwrap(),
            GroupDimension::Subagent
        );
        assert_eq!(
            "tool".parse::<GroupDimension>().unwrap(),
            GroupDimension::Tool
        );
        assert_eq!(
            "line".parse::<GroupDimension>().unwrap(),
            GroupDimension::Line
        );
    }

    #[test]
    fn test_group_dimension_from_str_invalid() {
        let err = "week".parse::<GroupDimension>().unwrap_err();
        assert!(err.contains("invalid group dimension"));
        assert!(err.contains("week"));
    }

    // --- GroupDimension::as_str roundtrip ---

    #[test]
    fn test_group_dimension_as_str_roundtrip() {
        let variants = [
            GroupDimension::Day,
            GroupDimension::Hour,
            GroupDimension::Month,
            GroupDimension::Session,
            GroupDimension::Project,
            GroupDimension::Model,
            GroupDimension::Subagent,
            GroupDimension::Tool,
            GroupDimension::Line,
        ];
        for v in variants {
            let s = v.as_str();
            let parsed: GroupDimension = s.parse().unwrap();
            assert_eq!(parsed, v, "roundtrip failed for {:?}", v);
        }
    }

    // --- GroupDimension::label ---

    #[test]
    fn test_group_dimension_label_all() {
        assert_eq!(GroupDimension::Day.label(), "Date");
        assert_eq!(GroupDimension::Hour.label(), "Hour");
        assert_eq!(GroupDimension::Month.label(), "Month");
        assert_eq!(GroupDimension::Session.label(), "Session");
        assert_eq!(GroupDimension::Project.label(), "Project");
        assert_eq!(GroupDimension::Model.label(), "Model");
        assert_eq!(GroupDimension::Subagent.label(), "Subagent");
        assert_eq!(GroupDimension::Tool.label(), "Tool");
        assert_eq!(GroupDimension::Line.label(), "Line");
    }

    // --- GroupDimension::all_valid ---

    #[test]
    fn test_group_dimension_all_valid() {
        let valid = GroupDimension::all_valid();
        assert_eq!(valid.len(), 9);
        // Every entry must be parseable as a GroupDimension
        for s in valid {
            assert!(
                s.parse::<GroupDimension>().is_ok(),
                "'{}' should be a valid GroupDimension",
                s
            );
        }
        // Spot-check expected members
        assert!(valid.contains(&"day"));
        assert!(valid.contains(&"subagent"));
    }

    // --- GroupDimension Display ---

    #[test]
    fn test_group_dimension_display() {
        let variants = [
            GroupDimension::Day,
            GroupDimension::Hour,
            GroupDimension::Month,
            GroupDimension::Session,
            GroupDimension::Project,
            GroupDimension::Model,
            GroupDimension::Subagent,
            GroupDimension::Tool,
            GroupDimension::Line,
        ];
        for v in variants {
            assert_eq!(
                format!("{}", v),
                v.as_str(),
                "Display output mismatch for {:?}",
                v
            );
        }
    }

    // --- SortOrder::from_str ---

    #[test]
    fn test_sort_order_from_str_valid() {
        assert_eq!("asc".parse::<SortOrder>().unwrap(), SortOrder::Asc);
        assert_eq!("desc".parse::<SortOrder>().unwrap(), SortOrder::Desc);
    }

    #[test]
    fn test_sort_order_from_str_invalid() {
        let err = "ascending".parse::<SortOrder>().unwrap_err();
        assert!(err.contains("invalid sort order"));
        assert!(err.contains("ascending"));
    }

    // --- PricedTokenRecord::from_token_record ---

    #[test]
    fn test_priced_token_record_from_token_record() {
        use chrono::Utc;
        let now = Utc::now();
        let record = TokenRecord {
            timestamp: now,
            model: "claude-3-5-sonnet".to_string(),
            session_id: "sess-abc".to_string(),
            project: "my-project".to_string(),
            agent_id: "agent-1".to_string(),
            tool_names: "Edit, Read".to_string(),
            line: 3,
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_tokens: 10,
            cache_read_tokens: 5,
            message_id: Some("msg-id".to_string()),
            request_id: Some("req-id".to_string()),
        };

        let priced = PricedTokenRecord::from_token_record(&record, 0.01, 0.02, 0.003, 0.04);

        assert_eq!(priced.timestamp, now);
        assert_eq!(priced.model, "claude-3-5-sonnet");
        assert_eq!(priced.session_id, "sess-abc");
        assert_eq!(priced.project, "my-project");
        assert_eq!(priced.agent_id, "agent-1");
        assert_eq!(priced.tool_names, "Edit, Read");
        assert_eq!(priced.line, 3);
        assert_eq!(priced.input_tokens, 100);
        assert_eq!(priced.output_tokens, 50);
        assert_eq!(priced.cache_creation_tokens, 10);
        assert_eq!(priced.cache_read_tokens, 5);
        assert_eq!(priced.message_id.as_deref(), Some("msg-id"));
        assert_eq!(priced.request_id.as_deref(), Some("req-id"));
        assert!((priced.input_cost - 0.01).abs() < f64::EPSILON);
        assert!((priced.cache_creation_cost - 0.02).abs() < f64::EPSILON);
        assert!((priced.cache_read_cost - 0.003).abs() < f64::EPSILON);
        assert!((priced.output_cost - 0.04).abs() < f64::EPSILON);
        // total_cost = sum of all four components
        let expected_total = 0.01 + 0.02 + 0.003 + 0.04;
        assert!((priced.total_cost - expected_total).abs() < 1e-10);
    }

    // --- ModelPricing optional fields ---

    #[test]
    fn test_model_pricing_deserializes_without_optional_fields() {
        let p: ModelPricing = serde_json::from_str(
            r#"{"inputCostPerToken":3e-6,"outputCostPerToken":1.5e-5,
                "cacheCreationCostPerToken":3.75e-6,"cacheReadCostPerToken":3e-7}"#,
        )
        .unwrap();
        assert_eq!(p.cache_creation_1h_cost_per_token, None);
        assert_eq!(p.fast_mode_multiplier, None);
        // Fallback 1h rate = 2 × input.
        assert!((p.cache_creation_1h_rate() - 6e-6).abs() < 1e-18);
    }

    #[test]
    fn test_model_pricing_deserializes_optional_fields() {
        let p: ModelPricing = serde_json::from_str(
            r#"{"inputCostPerToken":4e-6,"outputCostPerToken":2e-5,
                "cacheCreationCostPerToken":5e-6,"cacheReadCostPerToken":2e-7,
                "cacheCreation1hCostPerToken":9e-6,"fastModeMultiplier":2}"#,
        )
        .unwrap();
        assert_eq!(p.cache_creation_1h_cost_per_token, Some(9e-6));
        assert_eq!(p.fast_mode_multiplier, Some(2.0));
        // Explicit field wins over the 2 × input fallback.
        assert_eq!(p.cache_creation_1h_rate(), 9e-6);
    }

    #[test]
    fn test_model_pricing_serializes_without_none_fields() {
        let p = ModelPricing {
            input_cost_per_token: 1e-6,
            ..Default::default()
        };
        let v = serde_json::to_value(&p).unwrap();
        let obj = v.as_object().unwrap();
        assert_eq!(obj.len(), 4, "None fields must be omitted: {v}");
        assert!(!obj.contains_key("cacheCreation1hCostPerToken"));
        assert!(!obj.contains_key("fastModeMultiplier"));

        let p = ModelPricing {
            cache_creation_1h_cost_per_token: Some(2e-6),
            fast_mode_multiplier: Some(2.0),
            ..p
        };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["cacheCreation1hCostPerToken"], serde_json::json!(2e-6));
        assert_eq!(v["fastModeMultiplier"], serde_json::json!(2.0));
    }

    // --- GroupOptions::default ---

    #[test]
    fn test_group_options_default() {
        let opts = GroupOptions::default();
        assert_eq!(opts.order, SortOrder::Asc);
        assert!(opts.tz.is_none());
    }
}
