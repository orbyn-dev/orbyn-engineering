//! Scripted approvers and a running task for the Autopilot tests. The worker never ends its turn,
//! so the task stays `running` and the test plays the worker's requests through the real gate.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use orbyn_contracts::{ActionKind, PermissionMode};
use serde_json::json;

use super::harness::{Turn, World, brief, key, world};
use crate::crew::{Answer, Approver, Approvers, Summary};
use crate::governance::approvals::{ActionAsk, RunScope, Verdict};
use crate::store::AutopilotEvent;

type Reply = Arc<dyn Fn(&Summary) -> Answer + Send + Sync>;

/// An approver that answers by a function and remembers what it was shown.
pub struct Scripted {
    engine: &'static str,
    family: &'static str,
    reply: Reply,
    pub calls: AtomicUsize,
    pub seen: Mutex<Vec<Summary>>,
}

impl Scripted {
    pub fn new(
        engine: &'static str,
        family: &'static str,
        reply: impl Fn(&Summary) -> Answer + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            engine,
            family,
            reply: Arc::new(reply),
            calls: AtomicUsize::new(0),
            seen: Mutex::new(Vec::new()),
        })
    }

    pub fn yes(engine: &'static str, family: &'static str) -> Arc<Self> {
        Self::new(engine, family, |_| Answer {
            approve: true,
            reason: "Fits the brief.".into(),
        })
    }

    pub fn no(engine: &'static str, family: &'static str) -> Arc<Self> {
        Self::new(engine, family, |_| Answer {
            approve: false,
            reason: "Not needed for the brief.".into(),
        })
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Approver for Scripted {
    fn engine(&self) -> &str {
        self.engine
    }

    fn family(&self) -> &str {
        self.family
    }

    async fn decide(&self, summary: &Summary) -> Answer {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().push(summary.clone());
        (self.reply)(summary)
    }
}

pub fn approvers(lead: &Arc<Scripted>, others: &[&Arc<Scripted>]) -> Approvers {
    Approvers {
        lead: Some(lead.clone()),
        others: others
            .iter()
            .map(|o| (*o).clone() as Arc<dyn Approver>)
            .collect(),
    }
}

/// A world with one running task whose worker never ends its turn. Returns the task id.
pub async fn running() -> (World, String) {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    (w, id)
}

pub fn scope(w: &World, task: &str) -> RunScope {
    RunScope {
        run_id: "run-worker".into(),
        thread_id: w.worker(task),
        engine: "alpha".into(),
        family: "alpha".into(),
        write_scope: None,
        mode: PermissionMode::Supervised,
        plan: false,
    }
}

pub fn ask(kind: ActionKind, target: &str, paths: &[&str]) -> ActionAsk {
    ActionAsk {
        kind,
        target: target.to_owned(),
        summary: "A request from the worker.".to_owned(),
        detail: json!({}),
        paths: paths.iter().map(|p| (*p).to_owned()).collect(),
    }
}

pub async fn request(w: &World, task: &str, kind: ActionKind, target: &str) -> Verdict {
    w.core
        .gate
        .request(&scope(w, task), ask(kind, target, &[]))
        .await
}

pub fn words(verdict: &Verdict) -> String {
    match verdict {
        Verdict::Deny(words) => words.clone(),
        Verdict::Allow => "allowed".to_owned(),
    }
}

pub fn events(w: &World, task: &str, kind: &str) -> Vec<AutopilotEvent> {
    w.core
        .store
        .crew_autopilot_events(task)
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == kind)
        .collect()
}

/// The approval rows of a task, in order. The routing's own `route_class` row is not an approval.
pub fn kinds(w: &World, task: &str) -> Vec<String> {
    w.core
        .store
        .crew_autopilot_events(task)
        .unwrap()
        .into_iter()
        .map(|e| e.kind)
        .filter(|kind| kind != "route_class")
        .collect()
}
