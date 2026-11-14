//! Crew (Preview): the engine-agnostic crew of agents, behind `CoreConfig::crew_preview`.
//! This group holds the router, the task lifecycle, the verifier, the bus and the tools; the protocol's objects are in
//! `orbyn-contracts` and their tables in `store`.

mod autopilot;
mod background;
mod bus;
mod cancel;
pub mod changes;
mod checks;
mod classes;
mod filters;
#[cfg(any(test, feature = "test-support"))]
pub mod fixtures;
mod gather;
mod guard;
mod index_flags;
mod leases;
pub(crate) mod lifecycle;
mod persist;
mod protect;
mod recover;
mod report;
mod review;
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

pub(crate) use autopilot::GateReview;
pub use autopilot::{
    APPROVAL_LIFETIME_SECS, Answer, Answering, Approver, Approvers, Autopilot, COMMENT_MAX,
    DIGEST_HOURS_MAX, DiffFacts, Grants, HELD_LIFETIME_SECS, IN_A_ROW, IN_A_RUN, MergeOutcome,
    MergeTarget, Mode, ModeScope, Need, OFFSET_MAX, Outcome, PAGE_DEFAULT, PAGE_MAX, Page, RULES,
    Rate, RecentTotals, Refusal, ReviewAnswer, ReviewItem, Risk, SHOWN_MAX, Scene, Scored, Summary,
    WatchOutcome, area_of, areas_of, check_words, injection_in, lane_ran, owed_lanes, refuse,
    review_window, score, sensitive, tier_of, valid_id,
};
pub use background::{
    BackgroundEnd, EVERY_HOURS_MAX, MINUTES_MAX, NAME_MAX, SETTING_MAX, TURNS_MAX,
};
pub use bus::{ASK_TOOL, BOARD_READ_TOOL, BOARD_WRITE_TOOL, BUS_TOOLS, Bus, Limits, POST_TOOL};
pub use classes::{
    BUILT_IN as CLASS_BUILT_IN, ClassFile, Glob, PathClasses, base_commit, classify,
    classify_paths, read_class_file,
};
pub use gather::{LeftOut, Machine};
pub use guard::{find_secret, key_needles, redact, refuse_leak, refuse_secret, withhold};
pub use leases::refuse_protected;
pub use protect::protected_changes;
pub use review::{
    CHECK_FINDING_TOOL, CODE_LANE, FINDING_TOOL, FindingStatus, REVIEW_TOOL, REVIEW_TOOLS,
    RULES_FILE, RuleLane, SECURITY_LANE, STATUS_TOOL, agrees, fingerprint, read_rules, status_of,
};
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
