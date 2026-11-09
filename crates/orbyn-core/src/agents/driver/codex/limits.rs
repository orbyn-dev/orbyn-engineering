use serde_json::Value;

/// The latest reset among windows at their limit.
/// Only the subscription windows and the plan leave the driver. The reply also
/// carries an account ID, upsell copy and reset-credit offers, which are dropped.
pub(super) fn limit_fields(report: Value) -> Value {
    const KEPT: [&str; 3] = ["rateLimits", "rateLimitsByLimitId", "planType"];
    match report {
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .filter(|(key, _)| KEPT.contains(&key.as_str()))
                .collect(),
        ),
        _ => Value::Null,
    }
}

pub(super) fn spent_window_reset(params: &Value) -> Option<i64> {
    ["primary", "secondary"]
        .iter()
        .filter_map(|name| params.pointer(&format!("/rateLimits/{name}")))
        .filter(|window| {
            window
                .get("usedPercent")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                >= 100.0
        })
        .filter_map(|window| window.get("resetsAt").and_then(Value::as_i64))
        .max()
}
