//! Crew (Preview): the engine-agnostic crew of agents, behind `CoreConfig::crew_preview`.
//! This group holds the router, the task lifecycle, the verifier and the tools; the protocol's objects are in
//! `orbyn-contracts` and their tables in `store`.

mod checks;
mod filters;
mod gather;
mod guard;
mod leases;
pub(crate) mod lifecycle;
mod persist;
mod protect;
mod recover;
mod report;
mod router;
mod score;
mod settle;
mod stamp;
mod start;
mod tiers;
mod tool;
mod verify;
mod view;
mod workers;

#[cfg(test)]
mod tests;

pub use gather::{LeftOut, Machine};
pub use guard::{find_secret, key_needles, redact, refuse_leak, refuse_secret, withhold};
pub use leases::refuse_protected;
pub use protect::protected_changes;
pub use router::{RouteFailure, RouteInput, RouteRefusal, route};
pub use score::{EvalHistory, SEED_PASS_PERMILLE};
pub use stamp::Stamper;
pub use tiers::{RouterConfig, TierWords};
pub use tool::{REPORT_TOOL, ROUTE_TOOL, START_TOOL, is_tool, tool_specs};
pub use verify::Judgement;
pub use view::EngineView;

/// The wire spelling of an enum, such as `code_review`.
pub(crate) fn wire_name<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}
