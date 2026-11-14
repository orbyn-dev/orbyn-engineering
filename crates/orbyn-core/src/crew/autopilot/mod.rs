//! Autopilot (slice 4f), backend only: the lead approves what an agent asks for, code limits what
//! the lead may approve, and the person reviews what reached main. See the [README](README.md).

mod blast;
mod chain;
mod exit;
mod hard;
mod hard_tools;
mod held;
mod hosts;
mod ledger;
mod merge;
mod presence;
mod review;
mod shell;
mod targets;
mod tier;
mod tripwire;

pub use chain::{Answer, Approver, Approvers, Outcome, Summary};
pub use hard::{RULES, Refusal, Scene, refuse};
pub use held::{APPROVAL_LIFETIME_SECS, HELD_LIFETIME_SECS, HeldBook, SHOWN_MAX};
pub use hosts::read_allowed_hosts;
pub use merge::{
    Answering, COMMENT_MAX, DIGEST_HOURS_MAX, DiffFacts, MergeOutcome, MergeTarget, OFFSET_MAX,
    PAGE_DEFAULT, PAGE_MAX, Page, Rate, RecentTotals, ReviewAnswer, ReviewItem, Risk, Scored,
    WatchOutcome, area_of, areas_of, check_words, lane_ran, owed_lanes, review_window, score,
    sensitive, valid_id,
};
pub(crate) use review::GateReview;
pub use review::{Autopilot, ModeScope};
pub use tier::{Grants, Need, tier_of};
pub use tripwire::{IN_A_ROW, IN_A_RUN, injection_in};

/// How much Autopilot does for a repository or crew. The person chooses; the default is `Full`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Nothing is approved by the lead. A person answers every request, and nothing merges by itself.
    Off,
    /// The lead approves. A high-risk diff waits for a person before it merges.
    On,
    /// The lead approves, and a high-risk diff merges and goes to the top of the person's queue.
    #[default]
    Full,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
            Self::Full => "full",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "off" => Some(Self::Off),
            "on" => Some(Self::On),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}
