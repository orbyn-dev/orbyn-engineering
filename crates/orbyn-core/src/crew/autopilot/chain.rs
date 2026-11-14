//! The approval chain after the hard rules: the lead decides, and a second key on another vendor
//! must agree where the action needs two. It is a function of the mode, the tier and the
//! approvers, so each branch is tested without a `Core`. The lead sees only a [`Summary`]: kind, target, paths, the brief it serves and why
//! the action needs what it needs. Never the raw page, tool text or detail.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;

use super::Mode;
use super::tier::Need;

/// What an approver is shown.
#[derive(Clone, Debug, Serialize)]
pub struct Summary {
    pub kind: String,
    pub target: String,
    pub paths: Vec<String>,
    /// The outcome of the brief the action serves.
    pub brief: String,
    pub effects: Vec<String>,
    pub tier: String,
    pub reasons: Vec<String>,
}

/// An approver's answer. A reason is required, or the answer is a denial.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    pub approve: bool,
    pub reason: String,
}

/// Anyone who may say yes or no to a summary: the lead, or a second key. The engine-backed ones
/// are not built yet (see the README); tests use scripted ones.
#[async_trait]
pub trait Approver: Send + Sync {
    /// The engine that answers.
    fn engine(&self) -> &str;
    /// Its vendor: two engines of one family are one vendor.
    fn family(&self) -> &str;
    async fn decide(&self, summary: &Summary) -> Answer;
}

/// The lead and the others who may be a second key.
#[derive(Clone, Default)]
pub struct Approvers {
    pub lead: Option<Arc<dyn Approver>>,
    pub others: Vec<Arc<dyn Approver>>,
}

/// How the chain ended.
#[derive(Clone, Debug)]
pub enum Outcome {
    Allow {
        decided_by: &'static str,
        lead: Option<Answer>,
        second: Option<(String, Answer)>,
    },
    Deny {
        decided_by: &'static str,
        reason: String,
        lead: Option<Answer>,
        second: Option<(String, Answer)>,
        /// The two keys disagreed.
        disagreement: bool,
        /// The second key was missing: the person is told to sign in to a second engine.
        needs_second_engine: bool,
    },
    /// A person answers, as before Autopilot.
    Person { why: &'static str },
}

fn deny(decided_by: &'static str, reason: impl Into<String>) -> Outcome {
    Outcome::Deny {
        decided_by,
        reason: reason.into(),
        lead: None,
        second: None,
        disagreement: false,
        needs_second_engine: false,
    }
}

/// An answer with no reason is a denial: the lead must say why.
fn checked(answer: Answer) -> Answer {
    if answer.approve && answer.reason.trim().is_empty() {
        Answer {
            approve: false,
            reason: "An approval needs a reason, and none was given.".to_owned(),
        }
    } else {
        answer
    }
}

/// Decides an action that needs `need`.
pub async fn decide(mode: Mode, need: Need, summary: &Summary, approvers: &Approvers) -> Outcome {
    // The guard comes before the Off short-circuit: a person is never asked in a way that skips it, and no approver is asked at all
    // unless the mode is Full. The command is held for the person, who may approve it once.
    if need == Need::Guarded {
        return guarded(mode, summary, approvers).await;
    }
    // Off: no one is approved by code, so a person answers as before. The always-denied list has already run.
    if mode == Mode::Off {
        return Outcome::Person {
            why: "autopilot is off",
        };
    }
    if need == Need::Nothing {
        return Outcome::Allow {
            decided_by: "always_allowed",
            lead: None,
            second: None,
        };
    }
    let Some(lead) = &approvers.lead else {
        return match mode {
            Mode::Full => deny(
                "no_approver",
                "No lead is available to approve this, so it was denied.",
            ),
            _ => Outcome::Person {
                why: "no lead is available to approve",
            },
        };
    };
    let first = checked(lead.decide(summary).await);
    if !first.approve {
        return Outcome::Deny {
            decided_by: "lead",
            reason: first.reason.clone(),
            lead: Some(first),
            second: None,
            disagreement: false,
            needs_second_engine: false,
        };
    }
    if need == Need::Lead {
        return Outcome::Allow {
            decided_by: "lead",
            lead: Some(first),
            second: None,
        };
    }
    let Some(key) = approvers
        .others
        .iter()
        .find(|o| o.family() != lead.family())
    else {
        return match mode {
            Mode::Full => Outcome::Deny {
                decided_by: "two_keys",
                reason: "This needs a second approver on another vendor, and none is signed in."
                    .to_owned(),
                lead: Some(first),
                second: None,
                disagreement: false,
                needs_second_engine: true,
            },
            _ => Outcome::Person {
                why: "no second approver on another vendor",
            },
        };
    };
    let second = checked(key.decide(summary).await);
    let name = key.engine().to_owned();
    if second.approve {
        Outcome::Allow {
            decided_by: "two_keys",
            lead: Some(first),
            second: Some((name, second)),
        }
    } else {
        Outcome::Deny {
            decided_by: "two_keys",
            reason: format!("The second approver disagreed: {}", second.reason),
            lead: Some(first),
            second: Some((name, second)),
            disagreement: true,
            needs_second_engine: false,
        }
    }
}

/// A destructive command against a production-linked target. Under On and Off nobody is asked: it is denied, and the caller holds it
/// for the person (no two keys under On or Off). Under Full the lead decides, then a second key on another vendor; with no second
/// vendor signed in it is denied for now, and when the two keys disagree it is denied.
async fn guarded(mode: Mode, summary: &Summary, approvers: &Approvers) -> Outcome {
    if mode != Mode::Full {
        return deny(
            "guard",
            "This command can destroy data on a production-linked target. Autopilot does not approve it in this mode; it is held for you.",
        );
    }
    let Some(lead) = &approvers.lead else {
        return deny(
            "no_approver",
            "No lead is available to approve this, so it was denied.",
        );
    };
    let first = checked(lead.decide(summary).await);
    if !first.approve {
        return Outcome::Deny {
            decided_by: "lead",
            reason: first.reason.clone(),
            lead: Some(first),
            second: None,
            disagreement: false,
            needs_second_engine: false,
        };
    }
    let Some(key) = approvers
        .others
        .iter()
        .find(|o| o.family() != lead.family())
    else {
        return Outcome::Deny {
            decided_by: "two_keys",
            reason: "This can destroy data on a production-linked target and needs a second approver on another vendor. None is signed in, so it was denied for now."
                .to_owned(),
            lead: Some(first),
            second: None,
            disagreement: false,
            needs_second_engine: true,
        };
    };
    let second = checked(key.decide(summary).await);
    let name = key.engine().to_owned();
    if second.approve {
        Outcome::Allow {
            decided_by: "two_keys",
            lead: Some(first),
            second: Some((name, second)),
        }
    } else {
        Outcome::Deny {
            decided_by: "two_keys",
            reason: format!("The second approver disagreed: {}", second.reason),
            lead: Some(first),
            second: Some((name, second)),
            disagreement: true,
            needs_second_engine: false,
        }
    }
}
