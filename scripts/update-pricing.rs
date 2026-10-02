/// Fetch latest pricing from LiteLLM and overwrite src/pricing-data.json.
///
/// Only the LiteLLM snapshot is rewritten. Hand-maintained official
/// Anthropic entries (new models, undated aliases, 1-hour cache-write
/// rates, fast-mode multipliers, `<synthetic>`) live in
/// `src/pricing-overrides.json`, which `load_pricing()` merges on top of
/// this file — edit that one by hand when Anthropic changes prices.
///
/// Usage: cargo run --bin update-pricing
fn main() {
    match ccost::fetch_litellm_pricing() {
        Ok(pricing) => {
            let sorted: std::collections::BTreeMap<_, _> = pricing.models.into_iter().collect();
            let count = sorted.len();
            let output = serde_json::json!({
                "fetchedAt": pricing.fetched_at,
                "models": sorted
            });
            let json = serde_json::to_string_pretty(&output).unwrap();
            std::fs::write("src/pricing-data.json", format!("{json}\n")).unwrap();
            eprintln!(
                "Updated src/pricing-data.json ({} models, fetched at {})",
                count, pricing.fetched_at
            );
        }
        Err(e) => {
            eprintln!("Error fetching pricing: {e}");
            std::process::exit(1);
        }
    }
}
