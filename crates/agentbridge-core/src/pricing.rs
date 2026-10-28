use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{capability::ProviderId, providers::Usage};

/// Charged against the budget when a model has no known price, so unknown models cannot spend for free.
pub const UNKNOWN_MODEL_ESTIMATE_USD: f64 = 0.10;
const CHARS_PER_TOKEN: f64 = 4.0;
const ESTIMATED_TEXT_OUTPUT_TOKENS: f64 = 1_000.0;
const TOKENS_PER_MILLION: f64 = 1_000_000.0;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PriceEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_call: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_per_mtok: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_per_mtok: Option<f64>,
}

/// Approximate list prices in USD. They drift, so config overrides win and costs are flagged as
/// estimates unless computed from provider-reported token usage.
fn builtin(key: &str) -> Option<PriceEntry> {
    let entry = |per_call, input, output| PriceEntry {
        per_call,
        input_per_mtok: input,
        output_per_mtok: output,
    };
    match key {
        "openai/gpt-image-1" => Some(entry(Some(0.042), Some(5.0), Some(40.0))),
        "openai/gpt-image-2.5-flare" => Some(entry(Some(0.04), Some(5.0), Some(30.0))),
        "openai/gpt-4.1-mini" => Some(entry(None, Some(0.40), Some(1.60))),
        "openai/gpt-4o-mini-tts" => Some(entry(Some(0.015), None, None)),
        "gemini/gemini-2.5-flash-image" => Some(entry(Some(0.039), Some(0.30), Some(30.0))),
        "gemini/gemini-2.5-flash" => Some(entry(None, Some(0.30), Some(2.50))),
        "gemini/gemini-3.8-flash-tts" => Some(entry(None, Some(0.50), Some(9.0))),
        "elevenlabs/sound-generation" => Some(entry(Some(0.03), None, None)),
        "elevenlabs/eleven_multilingual_v2" => Some(entry(Some(0.03), None, None)),
        "openrouter/google/gemini-2.5-flash-image" => {
            Some(entry(Some(0.039), Some(0.30), Some(30.0)))
        }
        "openrouter/google/gemini-2.5-flash" => Some(entry(None, Some(0.30), Some(2.50))),
        "openrouter/google/gemini-3.8-flash-tts" => Some(entry(None, Some(0.50), Some(9.0))),
        _ => None,
    }
}

pub fn lookup(
    overrides: &BTreeMap<String, PriceEntry>,
    provider: ProviderId,
    model: &str,
) -> Option<PriceEntry> {
    let key = format!("{provider}/{model}");
    overrides.get(&key).cloned().or_else(|| builtin(&key))
}

pub fn estimate(entry: Option<&PriceEntry>, prompt: &str) -> f64 {
    let Some(entry) = entry else {
        return UNKNOWN_MODEL_ESTIMATE_USD;
    };
    if let Some(per_call) = entry.per_call {
        return per_call;
    }
    match (entry.input_per_mtok, entry.output_per_mtok) {
        (Some(input), Some(output)) => {
            let input_tokens = prompt.chars().count() as f64 / CHARS_PER_TOKEN;
            (input_tokens * input + ESTIMATED_TEXT_OUTPUT_TOKENS * output) / TOKENS_PER_MILLION
        }
        _ => UNKNOWN_MODEL_ESTIMATE_USD,
    }
}

/// Returns `(cost, is_estimate)`.
pub fn actual(entry: Option<&PriceEntry>, usage: Option<&Usage>, estimate: f64) -> (f64, bool) {
    // A billed amount reported by the provider beats any calculation from list prices.
    if let Some(billed) = usage.and_then(|u| u.cost_usd) {
        return (billed, false);
    }
    if let (Some(entry), Some(usage)) = (entry, usage) {
        if let (Some(input), Some(output)) = (entry.input_per_mtok, entry.output_per_mtok) {
            let cost = (usage.input_tokens as f64 * input + usage.output_tokens as f64 * output)
                / TOKENS_PER_MILLION;
            return (cost, false);
        }
    }
    (estimate, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_beats_builtin() {
        let mut overrides = BTreeMap::new();
        overrides.insert(
            "openai/gpt-image-1".to_string(),
            PriceEntry {
                per_call: Some(0.5),
                ..Default::default()
            },
        );
        let entry = lookup(&overrides, ProviderId::Openai, "gpt-image-1").unwrap();
        assert_eq!(estimate(Some(&entry), "x"), 0.5);
    }

    #[test]
    fn unknown_model_uses_fallback_estimate() {
        let entry = lookup(&BTreeMap::new(), ProviderId::Openai, "brand-new-model");
        assert_eq!(estimate(entry.as_ref(), "x"), UNKNOWN_MODEL_ESTIMATE_USD);
    }

    #[test]
    fn token_usage_gives_exact_cost() {
        let entry = lookup(&BTreeMap::new(), ProviderId::Openai, "gpt-image-1").unwrap();
        let usage = Usage {
            input_tokens: 100,
            output_tokens: 1056,
            cost_usd: None,
        };
        let (cost, is_estimate) = actual(Some(&entry), Some(&usage), 0.042);
        assert!(!is_estimate);
        assert!((cost - 0.04274).abs() < 1e-9);
    }

    #[test]
    fn provider_billed_cost_wins() {
        let usage = Usage {
            input_tokens: 10,
            output_tokens: 10,
            cost_usd: Some(0.0123),
        };
        assert_eq!(actual(None, Some(&usage), 0.5), (0.0123, false));
    }
}
