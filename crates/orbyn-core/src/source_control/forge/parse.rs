//! Reading what a forge answered into the shared pull request shapes, and bucketing checks.

use orbyn_contracts::{PullChecks, PullRequest};
use serde_json::Value;

use crate::util::json::{field, optional};

/// Failing check names kept per pull request, so a list stays small.
pub(super) const MAX_FAILING_NAMES: usize = 20;

pub(super) fn id_of(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::String(text)) => text.clone(),
        _ => String::new(),
    }
}

pub(super) fn person(value: &Value) -> Option<String> {
    value
        .get("login")
        .or_else(|| value.get("username"))
        .or_else(|| value.get("name"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

pub(super) fn check_state(status: &str, conclusion: &str) -> &'static str {
    match (
        status.to_uppercase().as_str(),
        conclusion.to_uppercase().as_str(),
    ) {
        (_, "SUCCESS" | "PASS" | "PASSED") | ("SUCCESS" | "PASS" | "PASSED", _) => "pass",
        (_, "FAILURE" | "FAIL" | "FAILED" | "TIMED_OUT" | "CANCELLED" | "CANCELED" | "ERROR")
        | ("FAILURE" | "FAIL" | "FAILED" | "ERROR", _) => "fail",
        (_, "SKIPPED" | "SKIPPING") | ("SKIPPED" | "SKIPPING", _) => "skipped",
        (_, "NEUTRAL") => "neutral",
        _ => "pending",
    }
}

/// What one entry of GitHub's check rollup counts as.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum CheckBucket {
    Pass,
    Fail,
    Running,
    /// Skipped, neutral or stale: only in the total.
    Other,
}

/// A check run is running until it is `COMPLETED`; then its conclusion decides. A
/// commit status has only a `state`, where `EXPECTED` and `PENDING` are still to come.
pub(super) fn check_bucket(entry: &Value) -> CheckBucket {
    let status = field(entry, "status").to_uppercase();
    let conclusion = field(entry, "conclusion").to_uppercase();
    if status.is_empty() && conclusion.is_empty() {
        return match field(entry, "state").to_uppercase().as_str() {
            "SUCCESS" => CheckBucket::Pass,
            "FAILURE" | "ERROR" => CheckBucket::Fail,
            _ => CheckBucket::Running,
        };
    }
    if !status.is_empty() && status != "COMPLETED" {
        return CheckBucket::Running;
    }
    match conclusion.as_str() {
        "SUCCESS" => CheckBucket::Pass,
        "FAILURE" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED" | "STARTUP_FAILURE" => {
            CheckBucket::Fail
        }
        _ => CheckBucket::Other,
    }
}

/// One word for the counts: a failure wins, then anything unfinished.
pub(super) fn github_rollup(value: &Value) -> Option<String> {
    let checks = github_checks(value)?;
    Some(
        if checks.total == 0 {
            "none"
        } else if checks.failing > 0 {
            "fail"
        } else if checks.running > 0 {
            "pending"
        } else {
            "pass"
        }
        .to_owned(),
    )
}

/// Per-state counts over GitHub's check rollup, which holds check runs and commit
/// statuses. Skipped, neutral and stale checks count only in the total.
pub(super) fn github_checks(value: &Value) -> Option<PullChecks> {
    let entries = value.get("statusCheckRollup")?.as_array()?;
    let mut checks = PullChecks {
        total: u32::try_from(entries.len()).unwrap_or(u32::MAX),
        passing: 0,
        failing: 0,
        running: 0,
        failing_names: Vec::new(),
    };
    for entry in entries {
        match check_bucket(entry) {
            CheckBucket::Pass => checks.passing += 1,
            CheckBucket::Fail => {
                checks.failing += 1;
                let name = [field(entry, "name"), field(entry, "context")].concat();
                if !name.is_empty() && checks.failing_names.len() < MAX_FAILING_NAMES {
                    checks.failing_names.push(name);
                }
            }
            CheckBucket::Running => checks.running += 1,
            CheckBucket::Other => {}
        }
    }
    Some(checks)
}

pub(super) fn review_word(decision: &str) -> String {
    match decision.to_uppercase().as_str() {
        "APPROVED" => "approved",
        "CHANGES_REQUESTED" => "changes_requested",
        "REVIEW_REQUIRED" => "review_required",
        _ => "none",
    }
    .to_owned()
}

/// Reads any forge's JSON into one shape. States are lowercase and `opened` is
/// GitLab's word for open.
pub fn parse_pull(value: &Value) -> Option<PullRequest> {
    let number = value
        .get("number")
        .or_else(|| value.get("iid"))
        .or_else(|| value.get("index"))
        .or_else(|| value.get("pullRequestId"))?
        .as_u64()?;
    let state = [field(value, "state"), field(value, "status")]
        .concat()
        .to_lowercase();
    let strip = |name: &str| name.trim_start_matches("refs/heads/").to_owned();
    let head = [
        field(value, "headRefName"),
        field(value, "source_branch"),
        field(value, "head"),
        field(value, "sourceRefName"),
    ]
    .concat();
    let base = [
        field(value, "baseRefName"),
        field(value, "target_branch"),
        field(value, "base"),
        field(value, "targetRefName"),
    ]
    .concat();
    let url = [field(value, "url"), field(value, "web_url")].concat();
    let author = value
        .get("author")
        .and_then(person)
        .or_else(|| {
            value
                .pointer("/createdBy/displayName")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .or_else(|| optional(value, "poster"));
    let labels = value
        .get("labels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|label| {
            label
                .as_str()
                .map(str::to_owned)
                .or_else(|| optional(label, "name"))
        })
        .collect();
    let draft = ["isDraft", "draft"]
        .iter()
        .any(|key| value.get(*key).and_then(Value::as_bool) == Some(true));
    Some(PullRequest {
        number,
        title: field(value, "title").to_owned(),
        url,
        state: match state.as_str() {
            "opened" | "active" => "open".to_owned(),
            "completed" => "merged".to_owned(),
            "abandoned" => "closed".to_owned(),
            _ => state,
        },
        head: strip(&head),
        base: strip(&base),
        author,
        draft,
        updated_at: optional(value, "updatedAt").or_else(|| optional(value, "updated_at")),
        labels,
        review_state: value
            .get("reviewDecision")
            .map(|decision| review_word(decision.as_str().unwrap_or_default())),
        checks_state: github_rollup(value),
        checks: github_checks(value),
    })
}

pub(super) fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or_default().to_owned()
}
