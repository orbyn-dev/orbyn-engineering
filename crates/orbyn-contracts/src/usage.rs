//! Token usage, prices and engine limit windows.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
wire! {
/// Estimated dollars by token type.
pub struct TokenCosts {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}
}

wire! {
/// One service tier of a model, as its history records it.
pub struct TierUsage {
    /// `standard`, `fast`, `priority`, `batch`, `flex` or `other`.
    pub tier: String,
    pub tokens: u64,
    /// `None` when this tier has no configured price.
    pub estimated_cost: Option<f64>,
}
}
wire! {
pub struct ModelUsage {
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    /// `None` when no tier of this model has a configured price.
    pub estimated_cost: Option<f64>,
    /// The priced part of `estimated_cost`, by token type.
    #[serde(default)]
    pub cost_split: Option<TokenCosts>,
    #[serde(default)]
    pub tiers: Vec<TierUsage>,
    /// Tokens in tiers with no configured price, left out of the estimate.
    #[serde(default)]
    pub unpriced_tokens: u64,
    /// This model's daily tokens in the period, oldest first.
    #[serde(default)]
    pub days: Vec<DailyUsage>,
}
}

wire! {
pub struct DailyUsage {
    /// `YYYY-MM-DD`.
    pub date: String,
    pub tokens: u64,
    /// The priced part of this day. `None` when nothing that day has a price.
    #[serde(default)]
    pub estimated_cost: Option<f64>,
}
}

wire! {
pub struct EngineUsage {
    pub engine: String,
    /// `scanned`, or `no_history` when this machine has none.
    pub status: String,
    pub files_scanned: u32,
    pub files_skipped: u32,
    pub models: Vec<ModelUsage>,
    pub days: Vec<DailyUsage>,
}
}

wire! {
pub struct UsageReport {
    /// Stable for this machine.
    pub host_id: String,
    pub scanned_at: String,
    pub engines: Vec<EngineUsage>,
}
}

wire! {
pub struct EngineLimits {
    pub engine: String,
    pub checked_at: String,
    /// The engine's own report, which it gives without Orbyn reading a credential.
    pub limits: Value,
    /// The same report in one shape. Empty when the shape is unknown.
    #[serde(default)]
    pub windows: Vec<LimitWindow>,
    /// The subscription plan, when the engine names it.
    #[serde(default)]
    pub plan: Option<String>,
}
}

wire! {
pub struct LimitWindow {
    /// The engine's window ID; Codex uses `primary` and `secondary`.
    pub id: String,
    pub label: String,
    pub used_percent: f64,
    pub window_minutes: Option<u64>,
    pub resets_at: Option<String>,
}
}

wire! {
/// One number per window: the most used engine, since that one stops first.
pub struct PooledWindow {
    pub label: String,
    pub window_minutes: Option<u64>,
    pub used_percent: f64,
    /// The soonest reset among the engines counted.
    pub resets_at: Option<String>,
    pub engines: Vec<String>,
}
}

wire! {
pub struct LimitsOverview {
    pub pooled: Vec<PooledWindow>,
    pub engines: Vec<EngineLimits>,
    /// Engines with no report, with why.
    pub unavailable: Vec<LimitsUnavailable>,
}
}

wire! {
pub struct LimitsUnavailable {
    pub engine: String,
    pub reason: String,
}
}
