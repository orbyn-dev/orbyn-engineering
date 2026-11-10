//! The render tool: the title and height a published page card is held to.

use serde_json::Value;

/// A published page's frame height. Nothing measures the page yet (C23), so a call
/// picks the height and these are the bounds it is held to.
pub(super) const DEFAULT_RENDER_HEIGHT: u64 = 480;

pub(super) const MIN_RENDER_HEIGHT: u64 = 120;

pub(super) const MAX_RENDER_HEIGHT: u64 = 2000;

pub(super) const MAX_RENDER_TITLE_CHARS: usize = 200;

/// The frame height a published page is shown at. A caller's number stands, held to a
/// size a card can show; without one the page gets the default. Nothing measures the
/// page yet (C23), so this is the whole of what decides it.
pub(super) fn render_height(args: &Value) -> u64 {
    args.get("height")
        .and_then(Value::as_f64)
        .map_or(DEFAULT_RENDER_HEIGHT, |height| {
            let bounded = height.clamp(MIN_RENDER_HEIGHT as f64, MAX_RENDER_HEIGHT as f64);
            // The bound is not negative, so the cast is exact after rounding.
            bounded.round() as u64
        })
}

/// The card's title: trimmed, cut to a length a row can show, and `HTML` when the
/// call named none.
pub(super) fn render_title(args: &Value) -> String {
    args.get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(|title| title.chars().take(MAX_RENDER_TITLE_CHARS).collect())
        .unwrap_or_else(|| "HTML".to_owned())
}
