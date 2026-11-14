//! The `route_class` row: one per routing, appended to the approval ledger after the secret and answer-key checks of
//! `ledger::safe`. A routing whose row cannot be written does not go on, because a decision with no record is not one.

use orbyn_contracts::{
    CrewRouteBlocked, CrewRouteClassHit, RejectionWhy, RouteRejection, RouteRequested,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::PathClasses;
use crate::app::Core;
use crate::crew::autopilot::ledger::safe;
use crate::crew::key_needles;
use crate::error::Result;

/// The kind of the ledger row.
pub const ROUTE_CLASS: &str = "route_class";

/// What a row holds. Every field has a default so a row that kept no detail still reads.
#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct Stored {
    pub commit: Option<String>,
    pub blob: Option<String>,
    pub problem: Option<String>,
    pub unknown: Option<String>,
    pub classes: Vec<CrewRouteClassHit>,
    pub blocked: Vec<CrewRouteBlocked>,
    pub chosen: Option<String>,
    pub rejected: Vec<RouteRejection>,
    pub message: Option<String>,
    pub requested: Option<RouteRequested>,
    pub withheld: Option<String>,
}

/// How a routing ended.
pub(in crate::crew) enum Routed<'a> {
    Chosen(&'a str),
    Refused {
        message: &'a str,
        requested: Option<&'a RouteRequested>,
    },
}

impl Core {
    /// Appends the routing's row. `rejected` is every engine the router rejected, whatever the reason.
    pub(in crate::crew) fn record_route_class(
        &self,
        task_id: &str,
        crew_id: &str,
        classes: &PathClasses,
        rejected: &[RouteRejection],
        routed: &Routed<'_>,
    ) -> Result<i64> {
        let mut stored = Stored {
            commit: classes.file.commit.clone(),
            blob: classes.file.blob.clone(),
            problem: classes.invalid().then(|| classes.problem_text()),
            unknown: classes.unknown_why().map(str::to_owned),
            classes: classes
                .matched()
                .into_iter()
                .map(|hit| CrewRouteClassHit {
                    name: hit.name,
                    pattern: hit.pattern,
                    samples: hit.samples,
                })
                .collect(),
            ..Stored::default()
        };
        stored.blocked = rejected
            .iter()
            .filter(|r| r.why == RejectionWhy::PathClass)
            .filter_map(|r| {
                classes.block_of(&r.engine).map(|block| CrewRouteBlocked {
                    engine: r.engine.as_str().to_owned(),
                    rule: block.rule,
                    reason: block.reason,
                })
            })
            .collect();
        match routed {
            Routed::Chosen(engine) => stored.chosen = Some((*engine).to_owned()),
            Routed::Refused { message, requested } => {
                stored.message = Some((*message).to_owned());
                stored.requested = requested.cloned();
                stored.rejected = rejected.to_vec();
            }
        }
        let body: Value = json!(stored);
        let needles = self
            .store
            .crew_answer_key(task_id)?
            .map_or_else(Vec::new, |key| key_needles(&key));
        self.store
            .add_crew_autopilot_event(task_id, crew_id, ROUTE_CLASS, &safe(&body, &needles))
    }
}
