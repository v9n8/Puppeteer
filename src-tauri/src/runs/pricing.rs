//! Estimated API-equivalent token pricing.
//!
//! Used to fill in `cost_usd` when the engine doesn't report it directly:
//! - Claude (Agent SDK) already returns `total_cost_usd`, so this is only a
//!   fallback for Claude (and calibrates the cumulative-cost repair).
//! - Codex (subscription) never reports a dollar cost, so its cost is always
//!   derived here and flagged `cost_estimated = 1`.
//!
//! Owner runs on subscriptions, so these dollar figures are a notional
//! "if this were billed per-token via API" estimate, not an actual charge.
//! Prices are USD per 1,000,000 tokens at standard (non-batch, non-fast,
//! non-long-context) list rates. Mirrored in src/lib/pricing.ts — keep the two
//! tables in sync, and bump `PRICING_VERSION` on every change so stored
//! estimates get re-priced (see `usage_cost::reprice_estimates`).
//!
//! Sources (checked 2026-10-17): Anthropic model pricing — cache writes are
//! 1.25x input with a 5-minute TTL and 2x input with a 1-hour TTL, cache reads
//! 0.1x input except Claude Fable 5.1 / Mythos 5.1 (0.025x) and Claude Opus 5.5
//! (0.05x); OpenAI API pricing — cached input as listed, cache writes billed as
//! plain input whatever their lifetime.

/// Identifies the table below; stored estimates priced with an older table are
/// recomputed once at startup. Not bumped for the 1-hour cache-write rate:
/// stored rows keep no 5-minute / 1-hour split, so only new rows take it.
pub const PRICING_VERSION: &str = "2026-10-17";

#[derive(Debug, Clone, Copy, PartialEq)]
struct ModelPrice {
    input: f64,
    output: f64,
    /// 5-minute cache writes.
    cache_write: f64,
    cache_write_1h: f64,
    cache_read: f64,
}

const fn claude(input: f64, output: f64, cache_read: f64) -> ModelPrice {
    ModelPrice { input, output, cache_write: input * 1.25, cache_write_1h: input * 2.0, cache_read }
}

const fn openai(input: f64, output: f64, cache_read: f64) -> ModelPrice {
    ModelPrice { input, output, cache_write: input, cache_write_1h: input, cache_read }
}

/// First matching substring wins, so more specific ids come first (e.g.
/// `opus-5-5` before `opus-5`, `gpt-5.5-pro` before `gpt-5.5`). Model ids may
/// carry suffixes such as `[1m]` or a date; 1M context has no surcharge.
const PRICES: &[(&str, ModelPrice)] = &[
    // ---- Anthropic Claude ----
    ("fable-5-1", claude(10.0, 50.0, 0.25)),
    ("mythos-5-1", claude(10.0, 50.0, 0.25)),
    ("fable", claude(10.0, 50.0, 1.0)),
    ("mythos", claude(10.0, 50.0, 1.0)),
    ("opus-5-5", claude(4.0, 20.0, 0.20)),
    ("opus-5", claude(5.0, 25.0, 0.50)),
    ("opus-4-8", claude(5.0, 25.0, 0.50)),
    ("opus-4-7", claude(5.0, 25.0, 0.50)),
    ("opus-4-6", claude(5.0, 25.0, 0.50)),
    ("opus-4-5", claude(5.0, 25.0, 0.50)),
    // Opus 4 / 4.1 and Claude 3 Opus.
    ("opus", claude(15.0, 75.0, 1.50)),
    ("sonnet-5", claude(2.0, 10.0, 0.20)),
    // Sonnet 4.x / 3.x.
    ("sonnet", claude(3.0, 15.0, 0.30)),
    ("haiku-4-5", claude(1.0, 5.0, 0.10)),
    // Haiku 3.5.
    ("haiku", claude(0.80, 4.0, 0.08)),
    // ---- OpenAI / Codex ----
    ("gpt-6-astra", openai(10.0, 50.0, 1.0)),
    ("gpt-5.6-terra", openai(2.0, 12.0, 0.20)),
    ("gpt-5.6-sol", openai(2.0, 10.0, 0.20)),
    ("gpt-5.6-luna", openai(0.20, 1.20, 0.02)),
    ("gpt-5.5-pro", openai(30.0, 180.0, 30.0)),
    ("gpt-5.5", openai(5.0, 30.0, 0.50)),
    ("gpt-5.3-codex", openai(1.75, 14.0, 0.175)),
    ("gpt-5", openai(1.25, 10.0, 0.125)),
    ("codex", openai(1.25, 10.0, 0.125)),
];

/// Generic mid-tier rate for ids matching nothing above.
const FALLBACK: ModelPrice = claude(3.0, 15.0, 0.30);

fn price_for(model: &str) -> ModelPrice {
    let m = model.to_ascii_lowercase();
    if m.starts_with("o3") || m.starts_with("o4") {
        return openai(1.25, 10.0, 0.125);
    }
    PRICES
        .iter()
        .find(|(key, _)| m.contains(key))
        .map(|(_, p)| *p)
        .unwrap_or(FALLBACK)
}

/// Cache-write tokens of a turn, split by cache lifetime where the engine says.
/// `five_min` and `one_hour` are parts of `total`; they may add up to less when
/// some writes came without a lifetime (an older CLI, a log from before the
/// split, a usage row that only kept the total).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CacheWrites {
    pub total: i64,
    pub five_min: i64,
    pub one_hour: i64,
}

impl CacheWrites {
    pub fn unsplit(total: i64) -> Self {
        Self { total, ..Self::default() }
    }

    /// From an Anthropic-style `usage` object: `cache_creation_input_tokens`,
    /// split by `cache_creation.ephemeral_{5m,1h}_input_tokens` when present.
    pub fn from_usage(usage: Option<&serde_json::Value>) -> Self {
        let tok = |v: Option<&serde_json::Value>, k: &str| {
            v.and_then(|u| u.get(k)).and_then(serde_json::Value::as_i64).unwrap_or(0)
        };
        let split = usage.and_then(|u| u.get("cache_creation"));
        Self {
            total: tok(usage, "cache_creation_input_tokens"),
            five_min: tok(split, "ephemeral_5m_input_tokens"),
            one_hour: tok(split, "ephemeral_1h_input_tokens"),
        }
    }
}

/// Estimate cost in USD from per-category token counts.
///
/// Cache writes without a lifetime are priced as 1-hour writes: Puppeteer
/// drives the Claude CLI on a subscription, where the CLI's `promptCacheTtl`
/// defaults the main conversation to 1 hour, so that is what those writes most
/// likely were. Pricing them at the 5-minute rate under-counts the estimate.
pub fn estimate_cost(
    model: &str,
    input_tokens: i64,
    output_tokens: i64,
    cache_writes: CacheWrites,
    cache_read_tokens: i64,
) -> f64 {
    let p = price_for(model);
    let per = 1_000_000.0;
    let total = cache_writes.total.max(cache_writes.five_min + cache_writes.one_hour);
    let one_hour = total - cache_writes.five_min;
    (input_tokens as f64) / per * p.input
        + (output_tokens as f64) / per * p.output
        + (cache_writes.five_min as f64) / per * p.cache_write
        + (one_hour as f64) / per * p.cache_write_1h
        + (cache_read_tokens as f64) / per * p.cache_read
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specific_ids_win_over_family() {
        assert_eq!(price_for("claude-opus-5-5[1m]"), claude(4.0, 20.0, 0.20));
        assert_eq!(price_for("claude-opus-5[1m]"), claude(5.0, 25.0, 0.50));
        assert_eq!(price_for("claude-opus-4-8"), claude(5.0, 25.0, 0.50));
        assert_eq!(price_for("claude-opus-4-1-20250805"), claude(15.0, 75.0, 1.50));
        assert_eq!(price_for("claude-fable-5-1"), claude(10.0, 50.0, 0.25));
        assert_eq!(price_for("claude-fable-5"), claude(10.0, 50.0, 1.0));
        assert_eq!(price_for("claude-sonnet-5-5"), claude(2.0, 10.0, 0.20));
        assert_eq!(price_for("claude-sonnet-4-6[1m]"), claude(3.0, 15.0, 0.30));
        assert_eq!(price_for("claude-haiku-4-5"), claude(1.0, 5.0, 0.10));
        assert_eq!(price_for("gpt-5.5-pro"), openai(30.0, 180.0, 30.0));
        assert_eq!(price_for("gpt-5.5"), openai(5.0, 30.0, 0.50));
        assert_eq!(price_for("gpt-5.6-terra"), openai(2.0, 12.0, 0.20));
        assert_eq!(price_for("gpt-5-codex"), openai(1.25, 10.0, 0.125));
        assert_eq!(price_for("o4-mini"), openai(1.25, 10.0, 0.125));
        assert_eq!(price_for("something-else"), FALLBACK);
    }

    #[test]
    fn claude_cache_write_is_1_25x_input_for_5m_and_2x_for_1h() {
        assert_eq!(price_for("claude-opus-5-5").cache_write, 5.0);
        assert_eq!(price_for("claude-opus-5-5").cache_write_1h, 8.0);
        assert_eq!(price_for("claude-fable-5-1").cache_write, 12.5);
        assert_eq!(price_for("claude-fable-5-1").cache_write_1h, 20.0);
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn estimate_sums_categories() {
        // 1M of each category on Opus 5.5, the write unsplit (so 1-hour): 4 + 20 + 8 + 0.2.
        let c = estimate_cost("claude-opus-5-5", 1_000_000, 1_000_000, CacheWrites::unsplit(1_000_000), 1_000_000);
        close(c, 32.2);
    }

    #[test]
    fn split_cache_writes_are_priced_by_lifetime() {
        // Opus 5.5: 1M 5-minute writes at $5, 2M 1-hour writes at $8.
        let w = CacheWrites { total: 3_000_000, five_min: 1_000_000, one_hour: 2_000_000 };
        close(estimate_cost("claude-opus-5-5", 0, 0, w, 0), 5.0 + 16.0);
        let all_5m = CacheWrites { total: 1_000_000, five_min: 1_000_000, one_hour: 0 };
        close(estimate_cost("claude-opus-5-5", 0, 0, all_5m, 0), 5.0);
    }

    #[test]
    fn writes_without_a_lifetime_count_as_1h() {
        close(estimate_cost("claude-opus-5-5", 0, 0, CacheWrites::unsplit(1_000_000), 0), 8.0);
        // Split covers only part of the total: the rest is priced as 1-hour.
        let partial = CacheWrites { total: 2_000_000, five_min: 1_000_000, one_hour: 0 };
        close(estimate_cost("claude-opus-5-5", 0, 0, partial, 0), 5.0 + 8.0);
    }

    #[test]
    fn codex_price_does_not_depend_on_cache_lifetime() {
        let split = CacheWrites { total: 1_000_000, five_min: 400_000, one_hour: 600_000 };
        for w in [CacheWrites::unsplit(1_000_000), split] {
            close(estimate_cost("gpt-5.5", 1_000_000, 1_000_000, w, 1_000_000), 5.0 + 30.0 + 5.0 + 0.5);
        }
    }

    #[test]
    fn cache_writes_read_from_usage() {
        let usage = serde_json::json!({
            "cache_creation_input_tokens": 300,
            "cache_creation": { "ephemeral_5m_input_tokens": 100, "ephemeral_1h_input_tokens": 200 }
        });
        assert_eq!(
            CacheWrites::from_usage(Some(&usage)),
            CacheWrites { total: 300, five_min: 100, one_hour: 200 }
        );
        let old = serde_json::json!({ "cache_creation_input_tokens": 300 });
        assert_eq!(CacheWrites::from_usage(Some(&old)), CacheWrites::unsplit(300));
        assert_eq!(CacheWrites::from_usage(None), CacheWrites::default());
    }
}
