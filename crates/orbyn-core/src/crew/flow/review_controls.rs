//! Pending consent exists before any reviewer task. Reads never start a lane.

use std::collections::BTreeMap;

use orbyn_contracts::{CrewReviewConsentState, FlowReviewConsent};
use serde_json::Value;

use super::{facts::Facts, scrub::Scrub};

pub(super) fn pending_controls(facts: &Facts) -> Vec<&Value> {
    let controls = facts.review["controls"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    let mut latest = BTreeMap::new();
    for control in controls.iter().filter(|c| c["kind"] == "same_engine") {
        let Some(lane) = control["lane"].as_str() else {
            continue;
        };
        if !matches!(
            control["state"].as_str(),
            Some("waiting" | "wait" | "allowed")
        ) {
            continue;
        }
        if controls
            .iter()
            .any(|c| c["kind"] == "remove_lane" && c["lane"] == lane && c["state"] == "removed")
        {
            continue;
        }
        let round = control["round"]
            .as_u64()
            .and_then(|r| u32::try_from(r).ok())
            .unwrap_or(0);
        let newest_round = super::cells::latest_rounds(&facts.review)
            .iter()
            .filter(|(name, ..)| name == lane)
            .map(|(_, round, ..)| *round)
            .max()
            .unwrap_or(0);
        if round != newest_round + 1 {
            continue;
        }
        latest.insert(lane, control);
    }
    latest.into_values().collect()
}

pub(super) fn consents(facts: &Facts, scrub: &Scrub) -> Vec<FlowReviewConsent> {
    let now = crate::util::unix_now();
    pending_controls(facts)
        .into_iter()
        .map(|control| {
            let lane = control["lane"].as_str().unwrap_or_default();
            let round = control["round"]
                .as_u64()
                .and_then(|r| u32::try_from(r).ok())
                .unwrap_or(0);
            let expires = control["expires_unix"].as_i64().unwrap_or(0);
            let state = if expires <= now {
                CrewReviewConsentState::Expired
            } else if control["state"] == "wait" {
                CrewReviewConsentState::Wait
            } else if control["state"] == "allowed" {
                CrewReviewConsentState::Allowed
            } else {
                CrewReviewConsentState::Waiting
            };
            FlowReviewConsent {
                id: control["id"].as_str().unwrap_or_default().to_owned(),
                task: facts.task.id.clone(),
                lane: scrub.text(lane, 60),
                commit: control["commit"].as_str().unwrap_or_default().to_owned(),
                round,
                engine: scrub.text(control["engine"].as_str().unwrap_or_default(), 80),
                model: scrub.text(control["model"].as_str().unwrap_or_default(), 80),
                can_answer: facts.task.state == "passed"
                    && matches!(
                        state,
                        CrewReviewConsentState::Waiting | CrewReviewConsentState::Wait
                    ),
                state,
                expires_unix: expires,
            }
        })
        .collect()
}
