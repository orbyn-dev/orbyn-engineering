//! Held delivery follows the real worker launch, including a person's resume.

use super::bus_support::{Duo, duo};
use crate::agents::driver::{
    EngineDescriptor, EngineDriver, EngineEvent, EngineProbe, ModelCatalog, RunHandle, RunSpec,
};
use crate::app::Core;
use crate::error::{CoreError, Result};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, Weak};
use tokio::sync::{Notify, mpsc};

pub(super) async fn pause_worker(d: &Duo) -> String {
    let worker = d.w.worker(&d.b);
    d.w.core
        .store
        .move_crew_task(&d.b, "needs_person", None)
        .unwrap();
    let active = d.w.core.store.thread(&worker).unwrap().active_run.unwrap();
    d.w.core.runs.cancel(&active.id).unwrap();
    d.w.until("the paused worker's turn ends", |w| {
        w.core.store.thread(&worker).unwrap().active_run.is_none()
    })
    .await;
    worker
}

struct Observation {
    answer: Value,
    user_prompt: String,
    context: String,
}
type Observed = Arc<Mutex<Option<Observation>>>;

/// An engine can use its MCP token before its asynchronous start returns a handle.
struct InboxBeforeAccept {
    inner: Arc<dyn EngineDriver>,
    core: Weak<Core>,
    observed: Observed,
    reject: bool,
    release: Option<Arc<Notify>>,
}

#[async_trait]
impl EngineDriver for InboxBeforeAccept {
    fn descriptor(&self) -> EngineDescriptor {
        let mut descriptor = self.inner.descriptor();
        descriptor.id = "beta-resume-probe".into();
        descriptor
    }
    fn family(&self) -> String {
        self.inner.family()
    }
    fn supports_approval(&self) -> bool {
        self.inner.supports_approval()
    }
    async fn probe(&self) -> EngineProbe {
        self.inner.probe().await
    }
    async fn models(&self) -> Result<Option<ModelCatalog>> {
        self.inner.models().await
    }
    async fn start_run(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<EngineEvent>,
    ) -> Result<RunHandle> {
        let core = self.core.upgrade().unwrap();
        let token = spec.mcp_url.as_deref().unwrap().rsplit('/').next().unwrap();
        let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "crew_board_read", "arguments": { "key": "none" } } });
        let reply = core.mcp.handle(token, &request).await.unwrap().unwrap();
        assert_ne!(reply["result"]["isError"], true, "{reply}");
        let answer =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        *self.observed.lock().unwrap() = Some(Observation {
            answer,
            user_prompt: spec.user_prompt.clone(),
            context: spec.context.clone(),
        });
        if let Some(release) = &self.release {
            release.notified().await;
        }
        if self.reject {
            return Err(CoreError::EngineUnavailable(
                "The fake engine refused the start.".into(),
            ));
        }
        self.inner.start_run(spec, events).await
    }
}

fn probe_start(d: &Duo, worker: &str, reject: bool, release: Option<Arc<Notify>>) -> Observed {
    let observed = Arc::new(Mutex::new(None));
    d.w.core.engines.register(Arc::new(InboxBeforeAccept {
        inner: d.w.core.engines.driver("beta").unwrap(),
        core: Arc::downgrade(&d.w.core),
        observed: observed.clone(),
        reject,
        release,
    }));
    // Registration adds drivers; it does not replace the original fake. Point
    // this paused test thread at the uniquely named wrapper before resuming it.
    let conn = rusqlite::Connection::open(d.w.core.config.data_dir.join("orbyn.sqlite3")).unwrap();
    assert_eq!(
        conn.execute(
            "UPDATE threads SET engine = 'beta-resume-probe' WHERE id = ?1",
            [worker],
        )
        .unwrap(),
        1
    );
    observed
}

async fn waiting_message(d: &Duo, body: &str) -> String {
    let worker = pause_worker(d).await;
    let (sent, error) = d.call(&d.ta, "crew_post", Duo::fact(&d.mb, body)).await;
    assert!(!error, "{sent}");
    worker
}

fn resume(d: &Duo, worker: &str) {
    d.w.core
        .store
        .move_crew_task(&d.b, "running", None)
        .unwrap();
    d.w.core
        .runs
        .enqueue(worker, "Continue with the person's answer.", &[])
        .unwrap();
}

#[tokio::test]
async fn an_early_engine_inbox_cannot_duplicate_its_launch_context_or_change_the_person_prompt() {
    let d = duo().await;
    let body = "Carry this fact in context once.";
    let worker = waiting_message(&d, body).await;
    let observed = probe_start(&d, &worker, false, None);
    resume(&d, &worker);
    d.w.until("the engine calls its inbox before acceptance", |_| {
        observed.lock().unwrap().is_some()
    })
    .await;
    let crew = d.w.task(&d.b).crew_id;
    d.w.until("the accepted start commits its receipt", |w| {
        w.core
            .store
            .held_crew_messages(&crew, &d.mb)
            .unwrap()
            .is_empty()
    })
    .await;
    let seen = observed.lock().unwrap();
    let seen = seen.as_ref().unwrap();
    assert!(seen.answer.get("inbox").is_none(), "{}", seen.answer);
    assert_eq!(seen.context.matches(body).count(), 1);
    assert_eq!(seen.user_prompt, "Continue with the person's answer.");
}

#[tokio::test]
async fn a_refused_engine_start_releases_claims_without_a_worker_delivery_receipt() {
    let d = duo().await;
    let worker = waiting_message(&d, "A refused start must leave this pending.").await;
    let observed = probe_start(&d, &worker, true, None);
    resume(&d, &worker);
    d.w.until("the refused start settles", |w| {
        observed.lock().unwrap().is_some()
            && w.core.store.thread(&worker).unwrap().active_run.is_none()
    })
    .await;
    let crew = d.w.task(&d.b).crew_id;
    assert_eq!(
        d.w.core
            .store
            .held_crew_messages(&crew, &d.mb)
            .unwrap()
            .len(),
        1
    );
    let recovered = d.w.core.collect_held(
        &crew,
        &d.mb,
        &worker,
        true,
        "tool_result",
        "recovery control",
    );
    assert!(!recovered.is_empty(), "the refused start leaked its claim");
    assert_eq!(
        d.w.core
            .store
            .crew_deliveries(&d.a)
            .unwrap()
            .iter()
            .filter(|row| row.recipient == d.mb && row.mode != "held")
            .count(),
        0
    );
}

#[tokio::test]
async fn cancellation_during_engine_acceptance_keeps_held_receipts_unacknowledged() {
    let d = duo().await;
    let worker = waiting_message(&d, "Cancellation must not acknowledge this.").await;
    let release = Arc::new(Notify::new());
    let observed = probe_start(&d, &worker, false, Some(release.clone()));
    resume(&d, &worker);
    d.w.until("the start is awaiting acceptance", |_| {
        observed.lock().unwrap().is_some()
    })
    .await;
    let active = d.w.core.store.thread(&worker).unwrap().active_run.unwrap();
    d.w.core.runs.cancel(&active.id).unwrap();
    release.notify_one();
    d.w.until("the cancelled start ends", |w| {
        w.core.store.thread(&worker).unwrap().active_run.is_none()
    })
    .await;
    let crew = d.w.task(&d.b).crew_id;
    assert_eq!(
        d.w.core
            .store
            .held_crew_messages(&crew, &d.mb)
            .unwrap()
            .len(),
        1
    );
    let recovered = d.w.core.collect_held(
        &crew,
        &d.mb,
        &worker,
        true,
        "tool_result",
        "recovery control",
    );
    assert!(!recovered.is_empty());
}

#[tokio::test]
async fn unreadable_answer_key_state_never_becomes_permission_to_send_a_held_body() {
    let d = duo().await;
    let (sent, error) = d
        .call(
            &d.ta,
            "crew_post",
            Duo::fact(&d.mb, "Pending before a guard read fails."),
        )
        .await;
    assert!(!error, "{sent}");
    let conn = rusqlite::Connection::open(d.w.core.config.data_dir.join("orbyn.sqlite3")).unwrap();
    conn.execute(
        "ALTER TABLE crew_finding_repros RENAME TO unavailable_crew_finding_repros",
        [],
    )
    .unwrap();
    let task = d.w.task(&d.b);
    let run = d.w.core.store.crew_run(&d.b).unwrap().unwrap();
    let held = d.w.core.held_for_worker(&task, &run);
    assert!(
        held.is_empty(),
        "a missing guard sent a body: {}",
        held.text()
    );
    assert_eq!(
        d.w.core
            .store
            .held_crew_messages(&task.crew_id, &d.mb)
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn a_collector_rechecks_pending_after_another_delivery_finishes() {
    let d = duo().await;
    let body = "One completed delivery is enough.";
    let (sent, error) = d.call(&d.ta, "crew_post", Duo::fact(&d.mb, body)).await;
    assert!(!error, "{sent}");
    let crew = d.w.task(&d.b).crew_id;
    let thread = d.w.worker(&d.b);
    let first =
        d.w.core
            .collect_held_with(&crew, &d.mb, &thread, true, "next_turn", "launch", || {
                let other =
                    d.w.core
                        .collect_held(&crew, &d.mb, &thread, true, "tool_result", "inbox");
                assert_eq!(other.text().matches(body).count(), 1);
                d.w.core.commit_held(&other);
            });
    assert!(
        first.is_empty(),
        "a stale snapshot repeated an accepted delivery: {}",
        first.text()
    );
    let deliveries = d.w.core.store.crew_deliveries(&d.a).unwrap();
    assert_eq!(
        deliveries
            .iter()
            .filter(|row| row.recipient == d.mb && row.mode != "held")
            .count(),
        1
    );
}

#[tokio::test]
async fn a_needs_person_worker_receives_held_messages_when_its_turn_resumes() {
    let d = duo().await;
    let worker = d.w.worker(&d.b);
    d.w.core
        .store
        .move_crew_task(&d.b, "needs_person", None)
        .unwrap();
    let active = d.w.core.store.thread(&worker).unwrap().active_run.unwrap();
    d.w.core.runs.cancel(&active.id).unwrap();
    d.w.until("the paused worker's turn ends", |w| {
        w.core.store.thread(&worker).unwrap().active_run.is_none()
    })
    .await;

    let body = "Keep the parser's newline behavior on resume.";
    let (sent, error) = d.call(&d.ta, "crew_post", Duo::fact(&d.mb, body)).await;
    assert!(!error, "{sent}");
    assert_eq!(sent["delivered"][0]["mode"], "held");
    d.w.core
        .store
        .move_crew_task(&d.b, "running", None)
        .unwrap();
    d.w.core
        .runs
        .enqueue(&worker, "Continue after the person's answer.", &[])
        .unwrap();
    d.w.until("the resumed turn starts", |w| w.sent_to(&worker).len() == 2)
        .await;
    let prompts = d.w.sent_to(&worker);
    let resumed = prompts.last().unwrap();
    assert_eq!(resumed.matches(body).count(), 1, "{resumed}");
    assert!(resumed.contains("trust=\"data\""));
    let crew = d.w.task(&d.b).crew_id;
    d.w.until("the accepted launch records delivery", |w| {
        w.core
            .store
            .held_crew_messages(&crew, &d.mb)
            .unwrap()
            .is_empty()
    })
    .await;
}

/// A `/compact` is not an ordinary turn: not every driver sends the extra context with it, so the held messages must stay held and
/// reach the next ordinary turn, exactly once.
#[tokio::test]
async fn a_compact_command_leaves_held_messages_for_the_next_ordinary_turn() {
    let d = duo().await;
    let worker = d.w.worker(&d.b);
    d.w.core
        .store
        .move_crew_task(&d.b, "needs_person", None)
        .unwrap();
    let active = d.w.core.store.thread(&worker).unwrap().active_run.unwrap();
    d.w.core.runs.cancel(&active.id).unwrap();
    d.w.until("the paused worker's turn ends", |w| {
        w.core.store.thread(&worker).unwrap().active_run.is_none()
    })
    .await;

    let body = "Keep the parser's newline behavior after a compact.";
    let (sent, error) = d.call(&d.ta, "crew_post", Duo::fact(&d.mb, body)).await;
    assert!(!error, "{sent}");
    d.w.core
        .store
        .move_crew_task(&d.b, "running", None)
        .unwrap();
    d.w.core.runs.enqueue(&worker, "/compact", &[]).unwrap();
    d.w.until("the compact turn starts", |w| w.sent_to(&worker).len() == 2)
        .await;
    let crew = d.w.task(&d.b).crew_id;
    assert!(
        !d.w.sent_to(&worker).last().unwrap().contains(body),
        "a compact turn carried the held message"
    );
    assert_eq!(
        d.w.core
            .store
            .held_crew_messages(&crew, &d.mb)
            .unwrap()
            .len(),
        1,
        "a compact turn marked the held message delivered"
    );
    d.w.until("the compact turn ends", |w| {
        w.core.store.thread(&worker).unwrap().active_run.is_none()
    })
    .await;

    d.w.core
        .runs
        .enqueue(&worker, "Continue after the compact.", &[])
        .unwrap();
    d.w.until("the next ordinary turn starts", |w| {
        w.sent_to(&worker).len() == 3
    })
    .await;
    let resumed = d.w.sent_to(&worker).last().unwrap().clone();
    assert_eq!(resumed.matches(body).count(), 1, "{resumed}");
}

#[tokio::test]
async fn a_terminal_worker_lists_undelivered_ids_to_the_lead_without_forwarding_bodies() {
    let d = duo().await;
    let body = "This worker-only note must not become a lead instruction.";
    let (sent, error) = d.call(&d.ta, "crew_post", Duo::fact(&d.mb, body)).await;
    assert!(!error, "{sent}");
    let crew = d.w.task(&d.b).crew_id;
    let held = d.w.core.store.held_crew_messages(&crew, &d.mb).unwrap();
    assert_eq!(held.len(), 1);
    let message_id = held[0].id.as_str().to_owned();
    d.w.core
        .crew_cancel_task(&d.b, Some("The person ended this task."), "test")
        .await
        .unwrap();
    d.w.until("the lead receives the final result", |w| {
        w.sent_to(&w.lead)
            .iter()
            .any(|prompt| prompt.contains("<crew-result"))
    })
    .await;
    let notice =
        d.w.sent_to(&d.w.lead)
            .into_iter()
            .find(|p| p.contains("<crew-result"))
            .unwrap();
    assert!(notice.contains("<crew-held-summary"), "{notice}");
    assert!(
        notice.contains(&message_id) && notice.contains(&d.mb),
        "{notice}"
    );
    assert!(!notice.contains(body));
    assert_eq!(
        d.w.core
            .store
            .held_crew_messages(&crew, &d.mb)
            .unwrap()
            .len(),
        1
    );
}
