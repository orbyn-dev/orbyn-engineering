//! Crew (Preview): the engine-agnostic crew of agents, behind `CoreConfig::crew_preview`.
//! This group holds the router and its dry-run tool; the protocol's objects are in
//! `orbyn-contracts` and their tables in `store`.

mod filters;
mod gather;
mod guard;
mod persist;
mod router;
mod score;
mod stamp;
mod tiers;
mod tool;
mod view;

#[cfg(test)]
mod tests;

pub use gather::Machine;
pub use guard::{find_secret, key_needles, refuse_leak, refuse_secret};
pub use router::{RouteFailure, RouteInput, RouteRefusal, route};
pub use score::{EvalHistory, SEED_PASS_PERMILLE};
pub use stamp::Stamper;
pub use tiers::{RouterConfig, TierWords};
pub use tool::{ROUTE_TOOL, tool_specs};
pub use view::EngineView;

/// The wire spelling of an enum, such as `code_review`.
pub(crate) fn wire_name<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}
