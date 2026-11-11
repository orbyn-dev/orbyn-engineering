//! A crew of two fake engines for the review tests: `alpha` builds and `beta` reviews, or the reverse, both mapped to
//! the tiers the router needs for a review role. The script is told the number of the turn, counted across every worker
//! and reviewer of the crew in the order they start.

use std::sync::Arc;

use serde_json::{Value, json};

use super::harness::{Turn, World, brief, report, world_tiered};

/// A finding draft in `path` at `line`.
pub fn finding(path: &str, line: u32, claim: &str) -> Value {
    json!({
        "severity": "high", "path": path, "line": line, "claim": claim,
        "evidence": "Reading the code shows it.", "repro": "call it with a large value",
    })
}

/// A reviewer's turn: it files these findings and reports.
pub fn reviewed(findings: Vec<Value>) -> Turn {
    let mut steps: Vec<(&'static str, Value)> =
        findings.into_iter().map(|f| ("crew_finding", f)).collect();
    steps.push(("crew_report", report("Reviewed.")));
    Turn::Steps(steps)
}

/// The builder's turn: it writes a file under `src/` and reports.
pub fn built(root: &std::path::Path) -> Turn {
    std::fs::write(root.join("src/lib.txt"), "fn parse_duration() {}\n").unwrap();
    Turn::Report(report("done"))
}

/// A fix's turn: the file the repro looks for, added to the index so `git ls-files` sees it.
pub fn fixed(root: &std::path::Path) -> Turn {
    std::fs::write(root.join("src/fixed.txt"), "fixed\n").unwrap();
    crate::testing::run_git(root, &["add", "src/fixed.txt"]);
    Turn::Report(report("fixed"))
}

/// A repro that fails until `src/fixed.txt` is in the index, so it confirms a finding the fix answers.
pub fn repro(id: &str) -> Value {
    json!({
        "id": id, "kind": "command",
        "params": { "run": "git ls-files --error-unmatch src/fixed.txt" },
        "proves": "The fix is in place.",
    })
}

pub async fn crew(script: super::harness::Script) -> World {
    world_tiered(true, script, &[("alpha", false), ("beta", false)], true).await
}

impl World {
    /// Starts the builder on `claude` and waits until it has passed.
    pub async fn built_work(&self) -> String {
        let (answer, error) = self
            .start(
                brief(json!({ "engine": { "id": "alpha", "assigned_by": "person" } })),
                super::harness::key(),
            )
            .await;
        assert!(!error, "{answer}");
        let id = answer["task"].as_str().unwrap().to_owned();
        self.until("the work ends", |w| {
            matches!(
                w.task(&id).state.as_str(),
                "passed" | "stopped" | "needs_person"
            )
        })
        .await;
        assert_eq!(self.task(&id).state, "passed");
        id
    }

    pub async fn lead(&self, tool: &str, args: Value) -> (Value, bool) {
        self.call(&self.lead_token, tool, args).await
    }

    /// Asks for these lanes and returns the review tasks, in order.
    pub async fn review(&self, task: &str, lanes: &[&str]) -> Vec<String> {
        let (answer, error) = self
            .lead("crew_review", json!({ "task": task, "lanes": lanes }))
            .await;
        assert!(!error, "{answer}");
        answer["lanes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                assert!(l["task"].is_string(), "a lane did not start: {l}");
                l["task"].as_str().unwrap().to_owned()
            })
            .collect()
    }

    pub fn status(&self, task: &str) -> Value {
        self.core.review_status(task).unwrap()
    }

    /// The first finding the review of `task` holds.
    pub fn first_finding(&self, task: &str) -> String {
        self.status(task)["findings"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }
}

pub fn arc(
    f: impl Fn(usize, &std::path::Path) -> Turn + Send + Sync + 'static,
) -> super::harness::Script {
    Arc::new(f)
}
