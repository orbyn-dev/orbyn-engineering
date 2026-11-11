//! Where a review stands, derived and never stored: which lanes ran in which rounds, what each finding's check
//! says, and whether the finding is open, fixed or regressed. A finding has no state of its own. Its check is a Gate
//! (`finding_check`) and its standing comes from comparing the rounds of its lane by fingerprint. Only a finding a
//! check confirmed, and that is still open, blocks; an unconfirmed or unchecked finding never does.

use std::collections::BTreeMap;

use orbyn_contracts::{CrewError, GateDecision, GateStage};
use serde_json::{Value, json};

use crate::app::Core;
use crate::crew::leases::internal;
use crate::crew::tool::invalid;
use crate::tools::mcp::McpBinding;

/// A finding against the later rounds of its lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindingStatus {
    /// In the lane's latest finished round, or filed in a round not yet finished.
    Open,
    /// Reported in an earlier round and absent from the lane's latest finished round.
    Fixed,
    /// In the latest finished round after an earlier round had shown it fixed.
    Regressed,
    /// Reported again in a later round; that later finding stands for it.
    Superseded,
}

impl FindingStatus {
    fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Fixed => "fixed",
            Self::Regressed => "regressed",
            Self::Superseded => "superseded",
        }
    }
}

/// The standing of a finding of `fingerprint` filed in `round`, given the fingerprints each finished round of
/// its lane reported, oldest first. A round that did not finish counts for nothing.
pub fn status_of(rounds: &[(u32, Vec<String>)], round: u32, fingerprint: &str) -> FindingStatus {
    let Some((latest, reported)) = rounds.last() else {
        return FindingStatus::Open;
    };
    if round > *latest {
        return FindingStatus::Open;
    }
    if !reported.iter().any(|f| f == fingerprint) {
        return FindingStatus::Fixed;
    }
    if round < *latest {
        return FindingStatus::Superseded;
    }
    let seen: Vec<bool> = rounds
        .iter()
        .map(|(_, found)| found.iter().any(|f| f == fingerprint))
        .collect();
    let first = seen.iter().position(|s| *s).unwrap_or(0);
    if seen[first..].iter().any(|s| !*s) {
        FindingStatus::Regressed
    } else {
        FindingStatus::Open
    }
}

impl Core {
    pub(super) fn crew_status_tool(
        &self,
        binding: &McpBinding,
        args: &Value,
    ) -> Result<Value, CrewError> {
        let (thread, _) = self.review_lead(binding, "crew_review_status")?;
        let id = args
            .get("task")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("task: required"))?;
        let (subject, _) = self.review_subject(&thread, id)?;
        self.review_status(&subject.id)
    }

    /// The review of finished work, as the lead reads it.
    pub fn review_status(&self, subject: &str) -> Result<Value, CrewError> {
        let reviews = self.store.crew_reviews_of(subject).map_err(internal)?;
        // What an independent second review filed is evidence for a check, not a finding of the lane.
        let second_runs: Vec<&str> = reviews
            .iter()
            .filter(|r| r.role == "finding_check")
            .map(|r| r.task_id.as_str())
            .collect();
        let findings: Vec<_> = self
            .store
            .crew_findings_of(subject)
            .map_err(internal)?
            .into_iter()
            .filter(|f| !second_runs.contains(&f.task_id.as_str()))
            .collect();
        let fixes = self.store.crew_fixes_of(subject).map_err(internal)?;
        let gates = self.store.crew_gates(subject).map_err(internal)?;

        // Lane -> its rounds, with the fingerprints a finished round reported.
        let mut lanes: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        let mut finished: BTreeMap<String, Vec<(u32, Vec<String>)>> = BTreeMap::new();
        let mut seconds = Vec::new();
        for review in &reviews {
            let state = self
                .store
                .crew_task(&review.task_id)
                .map_err(internal)?
                .map_or_else(|| "gone".to_owned(), |t| t.state);
            if review.role == "finding_check" {
                seconds.push(json!({
                    "finding": review.check_of, "lane": review.lane, "task": review.task_id, "state": state,
                }));
                continue;
            }
            lanes.entry(review.lane.clone()).or_default().push(json!({
                "round": review.round, "task": review.task_id, "state": state, "commit": review.commit,
            }));
            if state == "passed" {
                let prints = findings
                    .iter()
                    .filter(|f| f.task_id == review.task_id)
                    .map(|f| f.fingerprint.clone())
                    .collect();
                finished
                    .entry(review.lane.clone())
                    .or_default()
                    .push((review.round, prints));
            }
        }
        for rounds in finished.values_mut() {
            rounds.sort_by_key(|(round, _)| *round);
        }

        let decision = |finding: &str| {
            gates
                .iter()
                .find(|g| g.stage == GateStage::FindingCheck && g.subject.as_str() == finding)
                .map(|g| g.decision)
        };
        let waiting = |finding: &str| {
            seconds.iter().any(|s| {
                s["finding"] == finding && matches!(s["state"].as_str(), Some(state) if !crate::crew::lifecycle::is_final(state))
            })
        };
        let state_of = |task: &str| {
            self.store
                .crew_task(task)
                .ok()
                .flatten()
                .map_or_else(String::new, |t| t.state)
        };
        // A place is a lane and a file: a reworded claim is the same place. It blocks once a finding there is confirmed,
        // and stops blocking only when a fix that names it, and whose key proves the file changed, has passed and no later
        // finished round reports the place again. A model's silence clears nothing.
        let place =
            |f: &crate::store::StoredFinding| (f.lane.clone(), f.finding.path.as_str().to_owned());
        let mut shown = Vec::new();
        let mut blocked = false;
        for stored in &findings {
            let here = place(stored);
            let cluster: Vec<&crate::store::StoredFinding> =
                findings.iter().filter(|o| place(o) == here).collect();
            let confirmed = cluster
                .iter()
                .any(|o| decision(o.finding.id.as_str()) == Some(GateDecision::Confirmed));
            let place_fixes: Vec<&crate::store::CrewFix> = fixes
                .iter()
                .filter(|f| {
                    cluster
                        .iter()
                        .any(|o| o.finding.id.as_str() == f.finding_id)
                })
                .collect();
            let fix_round = place_fixes
                .iter()
                .filter(|f| state_of(&f.fix_task_id) == "passed")
                .filter_map(|f| {
                    cluster
                        .iter()
                        .find(|o| o.finding.id.as_str() == f.finding_id)
                })
                .map(|o| o.round)
                .max();
            let reappeared = fix_round.is_some_and(|round| {
                cluster
                    .iter()
                    .any(|o| o.round > round && state_of(&o.task_id) == "passed")
            });
            let blocks_place = confirmed && (fix_round.is_none() || reappeared);
            let own = decision(stored.finding.id.as_str());
            let check = match (own, confirmed) {
                (Some(GateDecision::Confirmed), _) | (None, true) => "confirmed",
                (Some(_), _) => "unconfirmed",
                (None, false) if waiting(stored.finding.id.as_str()) => "pending",
                (None, false) => "unchecked",
            };
            let status = status_of(
                finished.get(&stored.lane).map_or(&[][..], Vec::as_slice),
                stored.round,
                &stored.fingerprint,
            );
            let latest = cluster
                .last()
                .is_some_and(|o| o.finding.id == stored.finding.id);
            let cleared = fix_round.is_some_and(|r| stored.round <= r) && !reappeared;
            let status_name = match status {
                _ if cleared => "fixed",
                // Absent from a later round with no fix that proves it: unreported, still what it was.
                FindingStatus::Fixed => "unreported",
                other => other.name(),
            };
            let blocks = blocks_place && latest && check == "confirmed";
            blocked |= blocks;
            let fix = place_fixes.first();
            let loop_stopped =
                blocks && fix.is_some_and(|f| f.finding_id != stored.finding.id.as_str());
            shown.push(json!({
                "id": stored.finding.id,
                "lane": stored.lane,
                "round": stored.round,
                "severity": stored.finding.severity,
                "path": stored.finding.path,
                "line": stored.finding.line,
                "claim": stored.finding.claim,
                "check": check,
                "status": status_name,
                "blocks": blocks,
                "fix_task": fix.map(|f| &f.fix_task_id),
                "loop_stopped": loop_stopped,
            }));
        }
        let lanes: Vec<Value> = lanes
            .into_iter()
            .map(|(lane, rounds)| json!({ "lane": lane, "rounds": rounds }))
            .collect();
        Ok(json!({
            "task": subject,
            "lanes": lanes,
            "second_reviews": seconds,
            "findings": shown,
            "blocked": blocked,
        }))
    }
}
