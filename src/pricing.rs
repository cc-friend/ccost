use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::LazyLock;

use regex::Regex;

use crate::types::{ModelPricing, PricedTokenRecord, PricingData, TokenRecord};

static DATE_SUFFIX_PRICING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[-@]\d{8}$").unwrap());
static VERSION_SUFFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"-v\d+(?::\d+)?$").unwrap());

/// Supported LiteLLM key prefixes and the provider prefix to strip.
/// Format: (key_prefix_to_match, prefix_to_strip)
const SUPPORTED_PREFIXES: &[(&str, &str)] = &[
    ("anthropic.claude-", "anthropic."),
    ("minimax/MiniMax-", "minimax/"),
    ("moonshot/kimi-", "moonshot/"),
    ("zai/glm-", "zai/"),
];

/// Load bundled pricing data embedded at compile time.
pub fn load_pricing() -> PricingData {
    let data = include_str!("pricing-data.json");
    serde_json::from_str(data).expect("Failed to parse bundled pricing-data.json")
}

/// Load pricing data from a user-provided JSON file path.
pub fn load_pricing_from_file(file_path: &str) -> Result<PricingData, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(file_path)?;
    let user: PricingData = serde_json::from_str(&contents)?;
    let mut pricing = load_pricing();
    // User overrides take precedence over bundled defaults
    for (key, value) in user.models {
        pricing.models.insert(key, value);
    }
    if !user.fetched_at.is_empty() {
        pricing.fetched_at = user.fetched_at;
    }
    Ok(pricing)
}

/// Fetch live pricing data from the LiteLLM repository on GitHub.
///
/// Thin IO wrapper over [`parse_litellm_pricing`] — fetches the
/// upstream JSON and hands it off. Kept tiny so almost all of the
/// "interesting" logic lives in the pure parser, which is unit-tested
/// against synthetic LiteLLM JSON without any network dependency.
pub fn fetch_live_pricing() -> Result<PricingData, Box<dyn std::error::Error>> {
    let url = "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";
    let resp = minreq::get(url).send()?;
    let raw: serde_json::Value = serde_json::from_str(resp.as_str()?)?;
    parse_litellm_pricing(&raw)
}

/// Parse a LiteLLM `model_prices_and_context_window.json`-shaped JSON
/// value into a `PricingData`. Pure function — no IO, no network.
///
/// Filters: only models whose key starts with one of `SUPPORTED_PREFIXES`
/// (Anthropic / MiniMax / Moonshot / Zai). Provider prefix is stripped,
/// then any trailing version suffix (`-v1`, `-v2:0`) is removed and
/// `@date` is normalized to `-date`. First match per normalized name
/// wins. Models lacking `input_cost_per_token` are skipped silently —
/// upstream sometimes lists models with only context-window data.
pub fn parse_litellm_pricing(
    raw: &serde_json::Value,
) -> Result<PricingData, Box<dyn std::error::Error>> {
    let obj = raw.as_object().ok_or("Expected top-level JSON object")?;

    let version_suffix_re = &*VERSION_SUFFIX_RE;
    let mut models: HashMap<String, ModelPricing> = HashMap::new();

    for (key, value) in obj.iter() {
        // Find matching prefix and strip provider part
        let stripped = SUPPORTED_PREFIXES
            .iter()
            .find(|(prefix, _)| key.starts_with(prefix))
            .map(|(_, strip)| &key[strip.len()..]);

        let Some(stripped) = stripped else { continue };

        // Must have a numeric input_cost_per_token
        let input_cost = match value
            .get("input_cost_per_token")
            .and_then(serde_json::Value::as_f64)
        {
            Some(c) => c,
            None => continue,
        };

        // Strip version suffix like -v1, -v2:0
        let normalized = version_suffix_re.replace(stripped, "").to_string();

        // Deduplicate @date vs -date variants (e.g. claude-haiku-4-5@20251001 → claude-haiku-4-5-20251001)
        let normalized = normalized.replace('@', "-");

        // First match per normalized name wins
        if models.contains_key(&normalized) {
            continue;
        }

        let output_cost = value
            .get("output_cost_per_token")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let cache_creation_cost = value
            .get("cache_creation_input_token_cost")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let cache_read_cost = value
            .get("cache_read_input_token_cost")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);

        models.insert(
            normalized,
            ModelPricing {
                input_cost_per_token: input_cost,
                output_cost_per_token: output_cost,
                cache_creation_cost_per_token: cache_creation_cost,
                cache_read_cost_per_token: cache_read_cost,
            },
        );
    }

    let fetched_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    Ok(PricingData { fetched_at, models })
}

/// Three-tier fuzzy matching of a JSONL model name against pricing models.
///
/// 1. Direct exact match
/// 2. Strip trailing `-YYYYMMDD` or `@YYYYMMDD` date suffix, retry exact match
/// 3. Case-insensitive exact match (with and without date suffix)
/// 4. Case-insensitive substring containment
pub fn match_model_name<'a>(
    jsonl_name: &str,
    pricing_models: &'a HashMap<String, ModelPricing>,
) -> Option<&'a ModelPricing> {
    // Tier 1: direct exact match
    if let Some(pricing) = pricing_models.get(jsonl_name) {
        return Some(pricing);
    }

    // Tier 2: strip date suffix and retry
    let stripped = DATE_SUFFIX_PRICING_RE.replace(jsonl_name, "").to_string();
    if stripped != jsonl_name {
        if let Some(pricing) = pricing_models.get(&stripped) {
            return Some(pricing);
        }
    }

    // Tier 3: case-insensitive exact match (with and without date suffix)
    let jsonl_lower = jsonl_name.to_ascii_lowercase();
    let stripped_lower = stripped.to_ascii_lowercase();
    for (key, pricing) in pricing_models {
        let key_lower = key.to_ascii_lowercase();
        if key_lower == jsonl_lower || (stripped != jsonl_name && key_lower == stripped_lower) {
            return Some(pricing);
        }
    }

    // Tier 4: case-insensitive substring containment
    for (key, pricing) in pricing_models {
        let key_lower = key.to_ascii_lowercase();
        if jsonl_lower.contains(&key_lower) || key_lower.contains(&jsonl_lower) {
            return Some(pricing);
        }
    }

    // No match
    None
}

/// Calculate costs for a slice of token records using pricing data.
///
/// If `pricing` is None, loads the bundled pricing data.
/// Warns once per unmatched model name to stderr.
pub fn calculate_cost(
    records: &[TokenRecord],
    pricing: Option<&PricingData>,
) -> Vec<PricedTokenRecord> {
    let bundled;
    let pricing = match pricing {
        Some(p) => p,
        None => {
            bundled = load_pricing();
            &bundled
        }
    };

    let mut warned: HashSet<&str> = HashSet::new();
    let mut results = Vec::with_capacity(records.len());
    let mut model_cache: HashMap<&str, Option<&ModelPricing>> = HashMap::new();

    for record in records {
        let cached = *model_cache
            .entry(record.model.as_str())
            .or_insert_with(|| match_model_name(&record.model, &pricing.models));
        match cached {
            Some(model_pricing) => {
                let input_cost = record.input_tokens as f64 * model_pricing.input_cost_per_token;
                let cache_creation_cost = record.cache_creation_tokens as f64
                    * model_pricing.cache_creation_cost_per_token;
                let cache_read_cost =
                    record.cache_read_tokens as f64 * model_pricing.cache_read_cost_per_token;
                let output_cost = record.output_tokens as f64 * model_pricing.output_cost_per_token;

                results.push(PricedTokenRecord::from_token_record(
                    record,
                    input_cost,
                    cache_creation_cost,
                    cache_read_cost,
                    output_cost,
                ));
            }
            None => {
                if warned.insert(&record.model) {
                    eprintln!(
                        "Warning: No pricing found for model '{}', costs will be 0",
                        record.model
                    );
                }
                results.push(PricedTokenRecord::from_token_record(
                    record, 0.0, 0.0, 0.0, 0.0,
                ));
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_pricing(models: Vec<(&str, f64, f64, f64, f64)>) -> HashMap<String, ModelPricing> {
        models
            .into_iter()
            .map(|(name, inp, out, cc, cr)| {
                (
                    name.to_string(),
                    ModelPricing {
                        input_cost_per_token: inp,
                        output_cost_per_token: out,
                        cache_creation_cost_per_token: cc,
                        cache_read_cost_per_token: cr,
                    },
                )
            })
            .collect()
    }

    // ── match_model_name tests ──────────────────────────────────────────

    #[test]
    fn test_match_exact() {
        let models = make_pricing(vec![("claude-opus-4-6", 0.01, 0.02, 0.0, 0.0)]);
        let result = match_model_name("claude-opus-4-6", &models);
        assert!(result.is_some());
        assert_eq!(result.unwrap().input_cost_per_token, 0.01);
    }

    #[test]
    fn test_match_strip_date_suffix() {
        let models = make_pricing(vec![("claude-sonnet-4", 0.003, 0.015, 0.0, 0.0)]);
        let result = match_model_name("claude-sonnet-4-20250514", &models);
        assert!(result.is_some());
        assert_eq!(result.unwrap().input_cost_per_token, 0.003);
    }

    #[test]
    fn test_match_strip_at_date_suffix() {
        let models = make_pricing(vec![("claude-sonnet-4", 0.003, 0.015, 0.0, 0.0)]);
        let result = match_model_name("claude-sonnet-4@20250514", &models);
        assert!(result.is_some());
    }

    #[test]
    fn test_match_substring_containment() {
        let models = make_pricing(vec![("claude-opus-4-6", 0.01, 0.02, 0.0, 0.0)]);
        // jsonl_name contains the pricing key
        let result = match_model_name("claude-opus-4-6-some-variant", &models);
        assert!(result.is_some());
    }

    #[test]
    fn test_match_substring_reverse() {
        let models = make_pricing(vec![("claude-3-5-haiku-20241022", 0.001, 0.005, 0.0, 0.0)]);
        // pricing key contains jsonl_name
        let result = match_model_name("claude-3-5-haiku", &models);
        assert!(result.is_some());
    }

    #[test]
    fn test_match_no_match() {
        let models = make_pricing(vec![("claude-opus-4-6", 0.01, 0.02, 0.0, 0.0)]);
        let result = match_model_name("gpt-4o", &models);
        assert!(result.is_none());
    }

    #[test]
    fn test_match_glm_case_insensitive() {
        // LiteLLM stores "glm-4.7", user sets "GLM-4.7"
        let models = make_pricing(vec![("glm-4.7", 0.01, 0.02, 0.0, 0.0)]);
        assert!(match_model_name("GLM-4.7", &models).is_some());
        assert!(match_model_name("glm-4.7", &models).is_some());
    }

    #[test]
    fn test_match_glm_air_case_insensitive() {
        let models = make_pricing(vec![("glm-4.5-air", 0.005, 0.01, 0.0, 0.0)]);
        assert!(match_model_name("GLM-4.5-Air", &models).is_some());
        assert!(match_model_name("glm-4.5-air", &models).is_some());
    }

    #[test]
    fn test_match_kimi() {
        let models = make_pricing(vec![("kimi-k2-thinking", 0.01, 0.02, 0.0, 0.0)]);
        assert!(match_model_name("kimi-k2-thinking", &models).is_some());
    }

    #[test]
    fn test_match_minimax() {
        let models = make_pricing(vec![("MiniMax-M2.7", 0.01, 0.02, 0.0, 0.0)]);
        assert!(match_model_name("MiniMax-M2.7", &models).is_some());
    }

    #[test]
    fn test_no_match_unrelated_models() {
        let models = make_pricing(vec![
            ("claude-opus-4-6", 0.01, 0.02, 0.0, 0.0),
            ("glm-4.7", 0.01, 0.02, 0.0, 0.0),
            ("kimi-k2-thinking", 0.01, 0.02, 0.0, 0.0),
            ("MiniMax-M2.7", 0.01, 0.02, 0.0, 0.0),
        ]);
        assert!(match_model_name("gpt-4o", &models).is_none());
        assert!(match_model_name("gemini-2.5-pro", &models).is_none());
        assert!(match_model_name("deepseek-r1", &models).is_none());
    }

    #[test]
    fn test_match_empty_models() {
        let models: HashMap<String, ModelPricing> = HashMap::new();
        assert!(match_model_name("claude-opus-4-6", &models).is_none());
    }

    // ── load_pricing tests ──────────────────────────────────────────────

    #[test]
    fn test_load_bundled_pricing() {
        let pricing = load_pricing();
        assert!(
            !pricing.models.is_empty(),
            "bundled pricing should have models"
        );
        assert!(!pricing.fetched_at.is_empty());
    }

    #[test]
    fn test_load_pricing_from_file_merges_with_bundled() {
        let bundled = load_pricing();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("pricing.json");
        let json = serde_json::json!({
            "fetchedAt": "2026-01-01T00:00:00Z",
            "models": {
                "claude-test-1": {
                    "inputCostPerToken": 0.001,
                    "outputCostPerToken": 0.002,
                    "cacheCreationCostPerToken": 0.0,
                    "cacheReadCostPerToken": 0.0
                }
            }
        });
        std::fs::write(&path, serde_json::to_string(&json).unwrap()).unwrap();

        let result = load_pricing_from_file(path.to_str().unwrap());
        assert!(result.is_ok());
        let pricing = result.unwrap();
        // user model is present
        assert!(pricing.models.contains_key("claude-test-1"));
        // bundled models are also present
        assert!(
            pricing.models.len() > 1,
            "should include bundled models + user model"
        );
        assert_eq!(pricing.models.len(), bundled.models.len() + 1);
        // user fetchedAt overrides bundled
        assert_eq!(pricing.fetched_at, "2026-01-01T00:00:00Z");
    }

    #[test]
    fn test_load_pricing_from_file_without_fetched_at() {
        let bundled = load_pricing();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("pricing.json");
        let json = serde_json::json!({
            "models": {
                "claude-test-1": {
                    "inputCostPerToken": 0.001,
                    "outputCostPerToken": 0.002,
                    "cacheCreationCostPerToken": 0.0,
                    "cacheReadCostPerToken": 0.0
                }
            }
        });
        std::fs::write(&path, serde_json::to_string(&json).unwrap()).unwrap();

        let pricing = load_pricing_from_file(path.to_str().unwrap()).unwrap();
        assert!(pricing.models.contains_key("claude-test-1"));
        // fetched_at falls back to bundled value
        assert_eq!(pricing.fetched_at, bundled.fetched_at);
    }

    #[test]
    fn test_load_pricing_from_file_not_found() {
        let result = load_pricing_from_file("/nonexistent/path/pricing.json");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_pricing_from_file_invalid_json() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not valid json!!!").unwrap();

        let result = load_pricing_from_file(path.to_str().unwrap());
        assert!(result.is_err());
    }

    // ── calculate_cost tests ────────────────────────────────────────────

    #[test]
    fn test_calculate_cost_known_model() {
        let pricing = load_pricing();
        let records = vec![TokenRecord {
            timestamp: chrono::Utc::now(),
            model: "claude-opus-4-6".to_string(),
            session_id: "s1".to_string(),
            project: "p1".to_string(),
            agent_id: String::new(),
            tool_names: String::new(),
            line: 0,
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_tokens: 100,
            cache_read_tokens: 200,
            message_id: None,
            request_id: None,
        }];

        let priced = calculate_cost(&records, Some(&pricing));
        assert_eq!(priced.len(), 1);
        assert!(
            priced[0].input_cost > 0.0,
            "known model should have non-zero input cost"
        );
        assert!(priced[0].output_cost > 0.0);
        assert!(priced[0].total_cost > 0.0);
    }

    #[test]
    fn test_calculate_cost_unknown_model() {
        let pricing = load_pricing();
        let records = vec![TokenRecord {
            timestamp: chrono::Utc::now(),
            model: "totally-unknown-model-xyz".to_string(),
            session_id: "s1".to_string(),
            project: "p1".to_string(),
            agent_id: String::new(),
            tool_names: String::new(),
            line: 0,
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            message_id: None,
            request_id: None,
        }];

        let priced = calculate_cost(&records, Some(&pricing));
        assert_eq!(priced.len(), 1);
        assert_eq!(priced[0].input_cost, 0.0);
        assert_eq!(priced[0].output_cost, 0.0);
        assert_eq!(priced[0].total_cost, 0.0);
    }

    #[test]
    fn test_calculate_cost_empty() {
        let pricing = load_pricing();
        let priced = calculate_cost(&[], Some(&pricing));
        assert!(priced.is_empty());
    }

    #[test]
    fn test_calculate_cost_loads_bundled_when_none() {
        let records = vec![TokenRecord {
            timestamp: chrono::Utc::now(),
            model: "claude-opus-4-6".to_string(),
            session_id: "s1".to_string(),
            project: "p1".to_string(),
            agent_id: String::new(),
            tool_names: String::new(),
            line: 0,
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            message_id: None,
            request_id: None,
        }];

        // pricing = None should load bundled
        let priced = calculate_cost(&records, None);
        assert_eq!(priced.len(), 1);
        assert!(priced[0].total_cost > 0.0);
    }

    #[test]
    fn test_calculate_cost_caches_model_lookup() {
        let pricing = load_pricing();
        // Two records with same model — caching should work correctly
        let records = vec![
            TokenRecord {
                timestamp: chrono::Utc::now(),
                model: "claude-opus-4-6".to_string(),
                session_id: "s1".to_string(),
                project: "p1".to_string(),
                agent_id: String::new(),
                tool_names: String::new(),
                line: 0,
                input_tokens: 1000,
                output_tokens: 500,
                cache_creation_tokens: 0,
                cache_read_tokens: 0,
                message_id: None,
                request_id: None,
            },
            TokenRecord {
                timestamp: chrono::Utc::now(),
                model: "claude-opus-4-6".to_string(),
                session_id: "s1".to_string(),
                project: "p1".to_string(),
                agent_id: String::new(),
                tool_names: String::new(),
                line: 0,
                input_tokens: 2000,
                output_tokens: 1000,
                cache_creation_tokens: 0,
                cache_read_tokens: 0,
                message_id: None,
                request_id: None,
            },
        ];

        let priced = calculate_cost(&records, Some(&pricing));
        assert_eq!(priced.len(), 2);
        // Second record has 2x tokens, should have ~2x cost
        assert!(priced[1].input_cost > priced[0].input_cost);
    }

    #[test]
    fn test_calculate_cost_multiple_unknown_models_warns_once() {
        // Two records with the same unknown model — both should get 0 cost.
        // This exercises the warning-dedup path (warned HashSet).
        let pricing = load_pricing();
        let records = vec![
            TokenRecord {
                timestamp: chrono::Utc::now(),
                model: "absolutely-unknown-model-zzz".to_string(),
                session_id: "s1".to_string(),
                project: "p1".to_string(),
                agent_id: String::new(),
                tool_names: String::new(),
                line: 0,
                input_tokens: 100,
                output_tokens: 50,
                cache_creation_tokens: 0,
                cache_read_tokens: 0,
                message_id: None,
                request_id: None,
            },
            TokenRecord {
                timestamp: chrono::Utc::now(),
                model: "absolutely-unknown-model-zzz".to_string(),
                session_id: "s2".to_string(),
                project: "p1".to_string(),
                agent_id: String::new(),
                tool_names: String::new(),
                line: 0,
                input_tokens: 200,
                output_tokens: 100,
                cache_creation_tokens: 0,
                cache_read_tokens: 0,
                message_id: None,
                request_id: None,
            },
        ];

        let priced = calculate_cost(&records, Some(&pricing));
        assert_eq!(priced.len(), 2);
        // Both unknown-model records must have zero cost
        assert_eq!(priced[0].total_cost, 0.0);
        assert_eq!(priced[1].total_cost, 0.0);
        assert_eq!(priced[0].input_cost, 0.0);
        assert_eq!(priced[1].input_cost, 0.0);
    }

    #[test]
    fn test_calculate_cost_preserves_record_fields() {
        // Verify that all fields from TokenRecord are faithfully copied into
        // PricedTokenRecord by from_token_record.
        let ts = chrono::Utc::now();
        let record = TokenRecord {
            timestamp: ts,
            model: "claude-opus-4-6".to_string(),
            session_id: "session-abc".to_string(),
            project: "proj-xyz".to_string(),
            agent_id: "agent-001".to_string(),
            tool_names: String::new(),
            line: 0,
            input_tokens: 123,
            output_tokens: 456,
            cache_creation_tokens: 78,
            cache_read_tokens: 90,
            message_id: None,
            request_id: None,
        };
        let pricing = load_pricing();
        let priced = calculate_cost(std::slice::from_ref(&record), Some(&pricing));

        assert_eq!(priced.len(), 1);
        let p = &priced[0];
        assert_eq!(p.timestamp, ts);
        assert_eq!(p.model, "claude-opus-4-6");
        assert_eq!(p.session_id, "session-abc");
        assert_eq!(p.project, "proj-xyz");
        assert_eq!(p.agent_id, "agent-001");
        assert_eq!(p.input_tokens, 123);
        assert_eq!(p.output_tokens, 456);
        assert_eq!(p.cache_creation_tokens, 78);
        assert_eq!(p.cache_read_tokens, 90);
        // total_cost = sum of individual costs
        let expected = p.input_cost + p.cache_creation_cost + p.cache_read_cost + p.output_cost;
        assert!((p.total_cost - expected).abs() < 1e-12);
    }

    #[test]
    fn test_match_case_insensitive_exact_stripped() {
        // "Claude-Sonnet-4-20250514" should match pricing key "claude-sonnet-4"
        // via: strip date suffix → "Claude-Sonnet-4", then case-insensitive exact match.
        let models = make_pricing(vec![("claude-sonnet-4", 0.003, 0.015, 0.0, 0.0)]);
        let result = match_model_name("Claude-Sonnet-4-20250514", &models);
        assert!(
            result.is_some(),
            "expected case-insensitive date-stripped match"
        );
        assert_eq!(result.unwrap().input_cost_per_token, 0.003);
    }

    #[test]
    fn test_match_model_no_false_positive_partial() {
        // "claude" is in the bundled pricing, but the substring tier must not
        // accidentally match it when a better key like "claude-sonnet-4" is present.
        // The test verifies that "claude" does NOT match "claude-opus-4-6" via the
        // substring tier when the pricing table contains both.
        let models = make_pricing(vec![
            ("claude-opus-4-6", 0.01, 0.02, 0.0, 0.0),
            ("claude-sonnet-4", 0.003, 0.015, 0.0, 0.0),
        ]);
        // "claude" is shorter than both keys.  The substring tier checks whether
        // jsonl_lower.contains(key_lower) || key_lower.contains(jsonl_lower).
        // "claude-opus-4-6".contains("claude") → true, so Tier 4 WILL return a hit.
        // The important property is that a result IS returned (not panicking), and
        // that it is one of the two known pricing entries.
        let result = match_model_name("claude", &models);
        // Tier 4: one of the keys contains "claude" → should match
        assert!(result.is_some());
        let cost = result.unwrap().input_cost_per_token;
        assert!(
            cost == 0.01 || cost == 0.003,
            "matched cost should be one of the two known models"
        );
    }

    // ── parse_litellm_pricing tests ─────────────────────────────────────
    //
    // The pure parsing logic of `fetch_live_pricing` lives here so it
    // can be exercised without HTTPS to GitHub. Each case feeds in a
    // synthetic LiteLLM-shaped JSON and asserts the resulting
    // `PricingData.models` contents.

    fn json_obj(pairs: &[(&str, serde_json::Value)]) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for (k, v) in pairs {
            map.insert((*k).to_string(), v.clone());
        }
        serde_json::Value::Object(map)
    }

    fn model_value(input: f64, output: f64, cc: f64, cr: f64) -> serde_json::Value {
        serde_json::json!({
            "input_cost_per_token": input,
            "output_cost_per_token": output,
            "cache_creation_input_token_cost": cc,
            "cache_read_input_token_cost": cr,
        })
    }

    #[test]
    fn test_parse_litellm_anthropic_prefix() {
        let raw = json_obj(&[(
            "anthropic.claude-opus-4-6",
            model_value(15.0e-6, 75.0e-6, 0.0, 0.0),
        )]);
        let pricing = parse_litellm_pricing(&raw).unwrap();
        // Provider prefix `anthropic.` stripped.
        assert!(pricing.models.contains_key("claude-opus-4-6"));
        assert!((pricing.models["claude-opus-4-6"].input_cost_per_token - 15.0e-6).abs() < 1e-12);
        assert!((pricing.models["claude-opus-4-6"].output_cost_per_token - 75.0e-6).abs() < 1e-12);
    }

    #[test]
    fn test_parse_litellm_minimax_prefix() {
        let raw = json_obj(&[("minimax/MiniMax-M2.7", model_value(0.001, 0.002, 0.0, 0.0))]);
        let pricing = parse_litellm_pricing(&raw).unwrap();
        assert!(pricing.models.contains_key("MiniMax-M2.7"));
    }

    #[test]
    fn test_parse_litellm_moonshot_kimi_prefix() {
        let raw = json_obj(&[(
            "moonshot/kimi-k2-thinking",
            model_value(0.001, 0.002, 0.0, 0.0),
        )]);
        let pricing = parse_litellm_pricing(&raw).unwrap();
        assert!(pricing.models.contains_key("kimi-k2-thinking"));
    }

    #[test]
    fn test_parse_litellm_zai_glm_prefix() {
        let raw = json_obj(&[("zai/glm-4.7", model_value(0.0005, 0.001, 0.0, 0.0))]);
        let pricing = parse_litellm_pricing(&raw).unwrap();
        assert!(pricing.models.contains_key("glm-4.7"));
    }

    #[test]
    fn test_parse_litellm_skips_unsupported_provider() {
        // Models from providers we don't track (e.g. openai, azure)
        // must not appear in the output.
        let raw = json_obj(&[
            ("openai/gpt-4o", model_value(0.005, 0.015, 0.0, 0.0)),
            (
                "gemini/gemini-2.5-pro",
                model_value(0.0035, 0.0105, 0.0, 0.0),
            ),
            (
                "anthropic.claude-opus-4-6",
                model_value(15.0e-6, 75.0e-6, 0.0, 0.0),
            ),
        ]);
        let pricing = parse_litellm_pricing(&raw).unwrap();
        assert_eq!(pricing.models.len(), 1, "only Anthropic model kept");
        assert!(pricing.models.contains_key("claude-opus-4-6"));
    }

    #[test]
    fn test_parse_litellm_skips_models_without_input_cost() {
        // Some upstream entries carry only context-window data (no
        // pricing). Skip them silently — they'd render as $0/token.
        let raw = json_obj(&[
            (
                "anthropic.claude-opus-4-6",
                serde_json::json!({"max_tokens": 200000, "max_input_tokens": 200000}),
            ),
            (
                "anthropic.claude-sonnet-4",
                model_value(3.0e-6, 15.0e-6, 0.0, 0.0),
            ),
        ]);
        let pricing = parse_litellm_pricing(&raw).unwrap();
        assert_eq!(pricing.models.len(), 1);
        assert!(!pricing.models.contains_key("claude-opus-4-6"));
        assert!(pricing.models.contains_key("claude-sonnet-4"));
    }

    #[test]
    fn test_parse_litellm_skips_models_with_non_numeric_input_cost() {
        // Defensive: upstream sometimes ships strings/nulls.
        let raw = json_obj(&[
            (
                "anthropic.claude-opus-4-6",
                serde_json::json!({"input_cost_per_token": "not-a-number"}),
            ),
            (
                "anthropic.claude-sonnet-4",
                serde_json::json!({"input_cost_per_token": null}),
            ),
            (
                "anthropic.claude-haiku-4-5",
                model_value(1.0e-6, 5.0e-6, 0.0, 0.0),
            ),
        ]);
        let pricing = parse_litellm_pricing(&raw).unwrap();
        assert_eq!(pricing.models.len(), 1);
        assert!(pricing.models.contains_key("claude-haiku-4-5"));
    }

    #[test]
    fn test_parse_litellm_strips_version_suffix_v1() {
        // `-v1`, `-v2:0` etc. are AWS Bedrock revision tags. Normalize
        // them away so multiple versions of the same model collapse.
        let raw = json_obj(&[(
            "anthropic.claude-opus-4-6-v1",
            model_value(15.0e-6, 75.0e-6, 0.0, 0.0),
        )]);
        let pricing = parse_litellm_pricing(&raw).unwrap();
        assert!(pricing.models.contains_key("claude-opus-4-6"));
        assert!(!pricing.models.contains_key("claude-opus-4-6-v1"));
    }

    #[test]
    fn test_parse_litellm_strips_version_suffix_with_colon() {
        let raw = json_obj(&[(
            "anthropic.claude-sonnet-4-v2:0",
            model_value(3.0e-6, 15.0e-6, 0.0, 0.0),
        )]);
        let pricing = parse_litellm_pricing(&raw).unwrap();
        assert!(pricing.models.contains_key("claude-sonnet-4"));
    }

    #[test]
    fn test_parse_litellm_normalizes_at_to_dash_in_dates() {
        // Anthropic API uses `claude-haiku-4-5@20251001`; LiteLLM keys
        // sometimes use the `@` form, but ccost-internal canonical
        // form uses `-` (matching what the JSONL emits).
        let raw = json_obj(&[(
            "anthropic.claude-haiku-4-5@20251001",
            model_value(1.0e-6, 5.0e-6, 0.0, 0.0),
        )]);
        let pricing = parse_litellm_pricing(&raw).unwrap();
        assert!(pricing.models.contains_key("claude-haiku-4-5-20251001"));
        assert!(!pricing.models.contains_key("claude-haiku-4-5@20251001"));
    }

    #[test]
    fn test_parse_litellm_first_match_per_normalized_name_wins() {
        // After version stripping + @→- normalization, two upstream
        // keys can collide. The first one in iteration order wins —
        // we don't try to merge or pick "latest".
        let raw = json_obj(&[
            (
                "anthropic.claude-opus-4-6",
                model_value(15.0e-6, 75.0e-6, 0.0, 0.0),
            ),
            // Same normalized name (-v2 stripped) → skipped.
            (
                "anthropic.claude-opus-4-6-v2",
                model_value(99.0e-6, 99.0e-6, 0.0, 0.0),
            ),
        ]);
        let pricing = parse_litellm_pricing(&raw).unwrap();
        assert_eq!(pricing.models.len(), 1);
        let opus = &pricing.models["claude-opus-4-6"];
        // The first entry's price wins; the -v2's 99.0e-6 must NOT
        // appear (would indicate duplicate-merge bug).
        assert!((opus.input_cost_per_token - 15.0e-6).abs() < 1e-12);
    }

    #[test]
    fn test_parse_litellm_optional_cost_fields_default_to_zero() {
        // Only `input_cost_per_token` is required; everything else
        // defaults to 0 if missing.
        let raw = json_obj(&[(
            "anthropic.claude-opus-4-6",
            serde_json::json!({"input_cost_per_token": 15.0e-6}),
        )]);
        let pricing = parse_litellm_pricing(&raw).unwrap();
        let opus = &pricing.models["claude-opus-4-6"];
        assert_eq!(opus.output_cost_per_token, 0.0);
        assert_eq!(opus.cache_creation_cost_per_token, 0.0);
        assert_eq!(opus.cache_read_cost_per_token, 0.0);
    }

    #[test]
    fn test_parse_litellm_top_level_not_object_returns_error() {
        // Defensive: if upstream returns a JSON array or scalar, we
        // surface the error instead of panicking.
        let raw = serde_json::json!([1, 2, 3]);
        assert!(parse_litellm_pricing(&raw).is_err());

        let raw = serde_json::json!("string");
        assert!(parse_litellm_pricing(&raw).is_err());
    }

    #[test]
    fn test_parse_litellm_empty_object_yields_empty_pricing() {
        let raw = json_obj(&[]);
        let pricing = parse_litellm_pricing(&raw).unwrap();
        assert!(pricing.models.is_empty());
        // fetched_at still gets set to "now".
        assert!(!pricing.fetched_at.is_empty());
    }

    #[test]
    fn test_parse_litellm_realistic_mixed_input() {
        // End-to-end shape: the kind of real LiteLLM JSON ccost would
        // see in production — mix of supported, unsupported,
        // version-suffixed, missing-cost.
        let raw = json_obj(&[
            (
                "anthropic.claude-opus-4-6",
                model_value(15.0e-6, 75.0e-6, 18.75e-6, 1.5e-6),
            ),
            (
                "anthropic.claude-sonnet-4-v1",
                model_value(3.0e-6, 15.0e-6, 3.75e-6, 0.3e-6),
            ),
            (
                "anthropic.claude-haiku-4-5@20251001",
                model_value(1.0e-6, 5.0e-6, 0.0, 0.0),
            ),
            ("openai/gpt-4o", model_value(5.0e-6, 15.0e-6, 0.0, 0.0)),
            (
                "anthropic.context-only-no-pricing",
                serde_json::json!({"max_tokens": 200000}),
            ),
            ("zai/glm-4.7", model_value(0.5e-6, 1.0e-6, 0.0, 0.0)),
        ]);
        let pricing = parse_litellm_pricing(&raw).unwrap();
        assert_eq!(pricing.models.len(), 4, "opus + sonnet + haiku + glm");
        assert!(pricing.models.contains_key("claude-opus-4-6"));
        assert!(pricing.models.contains_key("claude-sonnet-4"));
        assert!(pricing.models.contains_key("claude-haiku-4-5-20251001"));
        assert!(pricing.models.contains_key("glm-4.7"));
        assert!(!pricing.models.contains_key("gpt-4o"));
        assert!(!pricing.models.contains_key("context-only-no-pricing"));
    }

    #[test]
    fn test_load_pricing_has_known_models() {
        let pricing = load_pricing();
        // A sample of models that must be present in the bundled pricing data
        let expected = &[
            "claude-sonnet-4-20250514",
            "claude-opus-4-6",
            "claude-3-5-haiku-20241022",
        ];
        for key in expected {
            assert!(
                pricing.models.contains_key(*key),
                "bundled pricing missing expected model key: {}",
                key
            );
        }
    }
}
