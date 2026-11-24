//! The issue-reader background listeners (KC7): the issue-label listener baselines the labels already on its
//! project's tracker on the first pass for a stored label, then starts one run per label-set change through the same
//! seam as Run now; the readers expose no per-label event time, so an edit is inert and a removal and re-add is not a
//! new event (a label feed is a follow-up). The dependency-update tick reads the project tracker's feed: a crew
//! baselines its first feed (every selected update is claimed and nothing starts), then a later unclaimed update
//! starts one run, while a `Cannot` answer keeps no marker and spends no event. A fake tracker stands in for the issue
//! readers; no real tracker is contacted.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use orbyn_contracts as wire;
use orbyn_contracts::{
    BackgroundCrewUpdate, BackgroundTrigger, BackgroundTriggerKind, ConnectTracker, IssueDetail,
    IssueLabel, IssueState, IssueStateCategory, IssueSummary, TrackerKind,
};
use serde_json::json;
use zeroize::Zeroizing;

use super::background_listeners::{
    NOW, POISON, add_origin, capacity, claims, crew_with, good, hung, project, runs_of, task_of,
    threads, until_run, waiting_merges,
};
use super::harness::World;
use super::harness::world;
use crate::credentials::{Credentials, Failure, SecretBackend};
use crate::crew::background_listeners::event_tag;
use crate::crew::capacity::{PausedKind, Priority};
use crate::crew::fixtures::audit_actions;
use crate::error::{CoreError, Result};
use crate::issues::{DependencyFeed, DependencyUpdate, IssueQuery, IssueSource, ScopeList};
use crate::testing::run_git;

/// A test keychain: the tracker token stays in memory, and no OS keychain is asked.
#[derive(Default)]
struct Memory(Mutex<HashMap<(String, String), Vec<u8>>>);

impl SecretBackend for Memory {
    fn get(
        &self,
        service: &str,
        account: &str,
    ) -> std::result::Result<Option<Zeroizing<Vec<u8>>>, Failure> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(&(service.into(), account.into()))
            .cloned()
            .map(Zeroizing::new))
    }
    fn set(&self, service: &str, account: &str, bytes: &[u8]) -> std::result::Result<(), Failure> {
        self.0
            .lock()
            .unwrap()
            .insert((service.into(), account.into()), bytes.to_vec());
        Ok(())
    }
    fn delete(&self, service: &str, account: &str) -> std::result::Result<(), Failure> {
        self.0
            .lock()
            .unwrap()
            .remove(&(service.into(), account.into()));
        Ok(())
    }
}

/// One issue a fake tracker reports: labels are what the listener selects on, and the title stands in for the
/// outside text the listener must never let reach a prompt.
fn labelled(key: &str, title: &str, labels: &[&str], updated: &str) -> IssueSummary {
    IssueSummary {
        tracker: TrackerKind::Gitlab,
        scope: "team/app".into(),
        key: key.into(),
        display_key: format!("#{key}"),
        title: title.into(),
        url: format!("https://gitlab.example.test/team/app/-/issues/{key}"),
        author: None,
        state: IssueState {
            category: IssueStateCategory::Open,
            name: "Open".into(),
            close_reason: None,
        },
        created_at: "2026-10-01T00:00:00Z".into(),
        updated_at: updated.into(),
        closed_at: None,
        assignees: vec![],
        labels: labels
            .iter()
            .map(|name| IssueLabel {
                name: (*name).into(),
            })
            .collect(),
        comment_count: 0,
    }
}

/// The issue reader a listener reads. It lists the issues that carry every asked label, with a count of the reads;
/// the dependency feed stands in for the tracker's own feed (GitHub's is tested in `issues::github`), and unreadable
/// fails every read.
struct Issues {
    issues: Mutex<Vec<IssueSummary>>,
    dependency: Mutex<DependencyFeed>,
    lists: AtomicUsize,
    /// How many issues one listing returns; 0 means all of them, as a tracker with a single page does.
    page_size: AtomicUsize,
    dependency_reads: AtomicUsize,
    /// The scope every listing asked for, in call order.
    scopes: Mutex<Vec<String>>,
    unreadable: bool,
}

impl Issues {
    fn new(issues: Vec<IssueSummary>) -> Arc<Self> {
        Arc::new(Self {
            issues: Mutex::new(issues),
            dependency: Mutex::new(DependencyFeed::Cannot),
            lists: AtomicUsize::new(0),
            page_size: AtomicUsize::new(0),
            dependency_reads: AtomicUsize::new(0),
            scopes: Mutex::new(Vec::new()),
            unreadable: false,
        })
    }

    fn unreadable() -> Arc<Self> {
        Arc::new(Self {
            issues: Mutex::new(Vec::new()),
            dependency: Mutex::new(DependencyFeed::Cannot),
            lists: AtomicUsize::new(0),
            page_size: AtomicUsize::new(0),
            dependency_reads: AtomicUsize::new(0),
            scopes: Mutex::new(Vec::new()),
            unreadable: true,
        })
    }
}

#[async_trait]
impl IssueSource for Issues {
    async fn list(&self, scope: &str, query: &IssueQuery) -> Result<ScopeList> {
        self.lists.fetch_add(1, Ordering::SeqCst);
        self.scopes.lock().unwrap().push(scope.to_owned());
        if self.unreadable {
            return Err(CoreError::TrackerUnavailable(
                "The tracker did not answer.".into(),
            ));
        }
        let issues = self
            .issues
            .lock()
            .unwrap()
            .iter()
            .filter(|issue| {
                query.labels.iter().all(|wanted| {
                    issue
                        .labels
                        .iter()
                        .any(|label| label.name.eq_ignore_ascii_case(wanted))
                })
            })
            .cloned()
            .map(|mut issue| {
                issue.scope = scope.to_owned();
                issue
            })
            .collect::<Vec<_>>();
        // A page is a slice from the cursor the source issued (an offset); the next cursor is issued while more remain.
        let size = self.page_size.load(Ordering::SeqCst);
        let from = query
            .cursors
            .values()
            .next()
            .and_then(|cursor| cursor.parse::<usize>().ok())
            .unwrap_or(0);
        let (issues, next) = if size == 0 {
            (issues, None)
        } else {
            let end = (from + size).min(issues.len());
            let more = end < issues.len();
            (
                issues[from.min(end)..end].to_vec(),
                more.then(|| end.to_string()),
            )
        };
        Ok(ScopeList {
            issues,
            counts: None,
            next,
        })
    }

    async fn get(&self, _scope: &str, _key: &str) -> Result<IssueDetail> {
        Err(CoreError::NotFound("issue".into()))
    }

    async fn dependency_updates(&self, _scope: &str) -> Result<DependencyFeed> {
        self.dependency_reads.fetch_add(1, Ordering::SeqCst);
        Ok(self.dependency.lock().unwrap().clone())
    }
}

/// The project's origin remote names a GitLab repository, but its fetched refs still answer the start's classes
/// read: a listener never fetches, it only maps the remote to the tracker scope.
async fn add_tracker_origin(w: &World) {
    add_origin(w);
    run_git(
        &w.repo,
        &[
            "remote",
            "set-url",
            "origin",
            "https://gitlab.example.test/team/app.git",
        ],
    );
}

/// Connects the fake GitLab reader the way a person does, with the test keychain: the host and scope are stored and
/// the token never leaves memory.
async fn connect_reader(w: &World, reader: Arc<Issues>) {
    w.core
        .issues
        .set_credentials(Credentials::new(Arc::new(Memory::default()), "test"));
    w.core.issues.set_source(TrackerKind::Gitlab, reader);
    w.core
        .connect_issue_tracker(
            TrackerKind::Gitlab,
            ConnectTracker {
                host: Some("https://gitlab.example.test".into()),
                scope: Some("team/app".into()),
                token: Some("glpat-test-token".into()),
            },
        )
        .await
        .unwrap();
}

fn label_trigger(label: &str) -> BackgroundTrigger {
    BackgroundTrigger {
        kind: BackgroundTriggerKind::IssueLabel,
        branch: None,
        workflow: None,
        label: Some(label.into()),
        every_hours: None,
        ecosystem: None,
        command: None,
        on_failing: None,
    }
}

async fn issue_crew(w: &World, name: &str, label: &str, on: bool) -> String {
    crew_with(
        w,
        name,
        json!({ "kind": "issue_label", "label": label }),
        on,
    )
    .await
}

/// An issue-label crew in a named project, for the two-project isolation case.
async fn issue_crew_in(w: &World, project: &str, name: &str, label: &str, on: bool) -> String {
    let body: wire::BackgroundCrewCreate = serde_json::from_value(json!({
        "project_id": project, "name": name, "template": "fix_failing_ci",
        "trigger": { "kind": "issue_label", "label": label },
        "budget": { "turns": 10, "minutes": 30 },
        "daily": { "runs": 100, "turns": 1000 }, "utc_offset_minutes": 600
    }))
    .unwrap();
    let crew = w.core.background_create(&body).await.unwrap();
    if on {
        w.core.background_set_enabled(&crew.id, true).unwrap();
    }
    crew.id
}

async fn dependency_crew(w: &World, name: &str, ecosystem: Option<&str>, on: bool) -> String {
    let trigger = match ecosystem {
        Some(ecosystem) => json!({ "kind": "dependency_update", "ecosystem": ecosystem }),
        None => json!({ "kind": "dependency_update" }),
    };
    crew_with(w, name, trigger, on).await
}

/// The baseline claim an issue-label crew keeps on its first pass for one stored label.
fn baseline_identity(crew: &str, label: &str) -> String {
    format!("issue_label:{crew}:baseline:{}", event_tag(label))
}

/// The hash of a label set, as the listener builds the claim's last part. The production function is used, so the
/// tests cannot drift from the framing it hashes.
fn label_set(names: &[&str]) -> String {
    let labels: Vec<IssueLabel> = names
        .iter()
        .map(|name| IssueLabel {
            name: (*name).into(),
        })
        .collect();
    crate::crew::background_listeners::label_set_tag(&labels)
}

/// The claim of one issue-label event at an issue's label set, as the listener builds it.
fn issue_identity(crew: &str, key: &str, label: &str, label_set_names: &[&str]) -> String {
    format!(
        "issue_label:{crew}:gitlab:team/app:{key}:{}:{}",
        event_tag(label),
        label_set(label_set_names)
    )
}

#[tokio::test]
async fn a_first_pass_baselines_the_labels_already_there_and_starts_nothing() {
    let w = world(true, good()).await;
    add_tracker_origin(&w).await;
    let reader = Issues::new(vec![
        labelled("7", "Already bug", &["bug"], "2026-11-01T00:00:00Z"),
        labelled("8", "Also bug", &["bug"], "2026-11-02T00:00:00Z"),
    ]);
    connect_reader(&w, reader.clone()).await;
    let id = issue_crew(&w, "Labels", "bug", true).await;
    let before = threads(&w);

    // First sight: every labelled issue is claimed and nothing starts.
    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    assert!(runs_of(&w, &id).is_empty(), "a baseline starts nothing");
    assert_eq!(threads(&w), before);
    assert_eq!(
        claims(&w),
        [
            baseline_identity(&id, "bug"),
            issue_identity(&id, "7", "bug", &["bug"]),
            issue_identity(&id, "8", "bug", &["bug"]),
        ]
    );

    // A later pass with nothing changed starts nothing new.
    w.core
        .crew_background_listeners_tick(NOW + 60)
        .await
        .unwrap();
    assert!(runs_of(&w, &id).is_empty());
    assert_eq!(claims(&w).len(), 3);

    // An edit that leaves the labels alone moves the issue's update time but not its claim: still inert.
    *reader.issues.lock().unwrap() = vec![
        labelled("7", "Edited title", &["bug"], "2026-11-03T00:00:00Z"),
        labelled("8", "Also bug", &["bug"], "2026-11-02T00:00:00Z"),
    ];
    w.core.issues.clear_cache();
    w.core
        .crew_background_listeners_tick(NOW + 120)
        .await
        .unwrap();
    assert!(runs_of(&w, &id).is_empty(), "an edit is not a label event");
    assert_eq!(claims(&w).len(), 3);

    // A new issue carrying the label after the baseline starts one run.
    reader
        .issues
        .lock()
        .unwrap()
        .push(labelled("9", "New bug", &["bug"], "2026-11-04T00:00:00Z"));
    w.core.issues.clear_cache();
    w.core
        .crew_background_listeners_tick(NOW + 180)
        .await
        .unwrap();
    assert_eq!(runs_of(&w, &id).len(), 1);
    let run = until_run(&w, &id, wire::BackgroundRunState::Running).await;
    let task = task_of(&w, &run);
    assert_eq!(
        w.core.store.crew_task_capacity(&task).unwrap(),
        (Priority::Low, true),
        "a listener firing is a Crew-started Low start, like Run now"
    );

    // The same issue unchanged is inert on a later pass: its claim is kept.
    w.until_state(&task, "passed").await;
    until_run(&w, &id, wire::BackgroundRunState::NeedsReview).await;
    w.core
        .crew_background_listeners_tick(NOW + 240)
        .await
        .unwrap();
    assert_eq!(runs_of(&w, &id).len(), 1);
    assert_eq!(claims(&w).len(), 4);
}

#[tokio::test]
async fn a_label_added_after_the_baseline_fires_once_and_a_removal_and_re_add_does_not() {
    let w = world(true, good()).await;
    add_tracker_origin(&w).await;
    let reader = Issues::new(vec![labelled("7", "Bug", &[], "2026-11-01T00:00:00Z")]);
    connect_reader(&w, reader.clone()).await;
    let id = issue_crew(&w, "Relabels", "bug", true).await;
    // The baseline: the issue carries no label, so only the marker is claimed.
    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    assert!(runs_of(&w, &id).is_empty());
    assert_eq!(claims(&w), [baseline_identity(&id, "bug")]);

    // The label appears: one event and one run.
    *reader.issues.lock().unwrap() = vec![labelled("7", "Bug", &["bug"], "2026-11-02T00:00:00Z")];
    w.core.issues.clear_cache();
    w.core
        .crew_background_listeners_tick(NOW + 60)
        .await
        .unwrap();
    assert_eq!(runs_of(&w, &id).len(), 1);
    assert_eq!(
        claims(&w),
        [
            baseline_identity(&id, "bug"),
            issue_identity(&id, "7", "bug", &["bug"])
        ]
    );
    let run = until_run(&w, &id, wire::BackgroundRunState::Running).await;
    let task = task_of(&w, &run);
    w.until_state(&task, "passed").await;
    until_run(&w, &id, wire::BackgroundRunState::NeedsReview).await;

    // The label goes away: no matching issue, so nothing is claimed and nothing starts.
    *reader.issues.lock().unwrap() = vec![labelled("7", "Bug", &[], "2026-11-03T00:00:00Z")];
    w.core.issues.clear_cache();
    w.core
        .crew_background_listeners_tick(NOW + 120)
        .await
        .unwrap();
    assert_eq!(runs_of(&w, &id).len(), 1);
    assert_eq!(claims(&w).len(), 2);

    // Added again later: the label set is one the crew already claimed, so nothing starts. The readers expose no
    // per-label event time, so this removal and re-add cannot be told from an edit; a label feed is the follow-up.
    *reader.issues.lock().unwrap() = vec![labelled("7", "Bug", &["bug"], "2026-11-04T00:00:00Z")];
    w.core.issues.clear_cache();
    w.core
        .crew_background_listeners_tick(NOW + 180)
        .await
        .unwrap();
    assert_eq!(runs_of(&w, &id).len(), 1, "a re-add is not a new event");
    assert_eq!(claims(&w).len(), 2);

    // Any other label-set change while the wanted label is present is a new event: one run.
    *reader.issues.lock().unwrap() =
        vec![labelled("7", "Bug", &["bug", "ui"], "2026-11-05T00:00:00Z")];
    w.core.issues.clear_cache();
    w.core
        .crew_background_listeners_tick(NOW + 240)
        .await
        .unwrap();
    assert_eq!(runs_of(&w, &id).len(), 2);
    assert_eq!(
        claims(&w).len(),
        3,
        "the new label set is a new claim: {:?}",
        claims(&w)
    );

    // Back to the original set is inert: it is a set already claimed.
    *reader.issues.lock().unwrap() = vec![labelled("7", "Bug", &["bug"], "2026-11-06T00:00:00Z")];
    w.core.issues.clear_cache();
    w.core
        .crew_background_listeners_tick(NOW + 300)
        .await
        .unwrap();
    assert_eq!(runs_of(&w, &id).len(), 2);
    assert_eq!(claims(&w).len(), 3);
}

#[tokio::test]
async fn changing_the_stored_label_baselines_the_issues_that_already_have_it() {
    let w = world(true, good()).await;
    add_tracker_origin(&w).await;
    // One issue carries both labels, so the second label is already on an issue when the crew is switched to it.
    let reader = Issues::new(vec![labelled(
        "7",
        "Both",
        &["bug", "ui"],
        "2026-11-01T00:00:00Z",
    )]);
    connect_reader(&w, reader.clone()).await;
    let id = issue_crew(&w, "Switch", "bug", true).await;
    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    assert!(runs_of(&w, &id).is_empty(), "the first label baselines");

    // The person changes the stored label. Its first pass baselines the issue that already carries it: no run.
    w.core
        .background_update(
            &id,
            &BackgroundCrewUpdate {
                name: None,
                trigger: Some(label_trigger("ui")),
            },
        )
        .await
        .unwrap();
    w.core
        .crew_background_listeners_tick(NOW + 60)
        .await
        .unwrap();
    assert!(
        runs_of(&w, &id).is_empty(),
        "a new stored label is baselined, not replayed: {:?}",
        claims(&w)
    );
    let claimed = claims(&w);
    assert_eq!(claimed.len(), 4, "two markers and two claims: {claimed:?}");
    assert!(claimed.contains(&baseline_identity(&id, "ui")));
    assert!(claimed.contains(&issue_identity(&id, "7", "ui", &["bug", "ui"])));

    // Only a label event after that baseline starts a run.
    reader
        .issues
        .lock()
        .unwrap()
        .push(labelled("8", "New ui", &["ui"], "2026-11-03T00:00:00Z"));
    w.core.issues.clear_cache();
    w.core
        .crew_background_listeners_tick(NOW + 120)
        .await
        .unwrap();
    assert_eq!(runs_of(&w, &id).len(), 1);
}

#[tokio::test]
async fn one_pass_attempts_at_most_one_start_per_issue_label_crew() {
    let w = world(true, hung()).await;
    add_tracker_origin(&w).await;
    let reader = Issues::new(Vec::new());
    connect_reader(&w, reader.clone()).await;
    let id = issue_crew(&w, "Many labels", "bug", true).await;
    // An empty baseline: the first pass claims the marker only and starts nothing.
    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    assert_eq!(claims(&w), [baseline_identity(&id, "bug")]);

    // Two label events arrive before the next pass: only the newest one starts.
    reader.issues.lock().unwrap().extend([
        labelled("8", "Newer", &["bug"], "2026-11-02T00:00:00Z"),
        labelled("7", "Older", &["bug"], "2026-11-01T00:00:00Z"),
    ]);
    w.core.issues.clear_cache();
    w.core
        .crew_background_listeners_tick(NOW + 60)
        .await
        .unwrap();
    assert_eq!(runs_of(&w, &id).len(), 1);
    assert_eq!(
        claims(&w),
        [
            baseline_identity(&id, "bug"),
            issue_identity(&id, "8", "bug", &["bug"])
        ],
        "the newest issue wins and only one start was attempted"
    );

    // The next pass reaches the older issue while the first run still holds the crew: claimed, refused once, and
    // the claim stays so the refusal is never retried on a timer.
    w.core
        .crew_background_listeners_tick(NOW + 120)
        .await
        .unwrap();
    assert_eq!(runs_of(&w, &id).len(), 1);
    assert_eq!(claims(&w).len(), 3);
    assert_eq!(
        audit_actions(&w.core, "crew.background.listener.refused").len(),
        1
    );
    w.core
        .crew_background_listeners_tick(NOW + 180)
        .await
        .unwrap();
    assert_eq!(claims(&w).len(), 3, "a refused event is never retried");
    assert_eq!(
        audit_actions(&w.core, "crew.background.listener.refused").len(),
        1
    );
}

#[tokio::test]
async fn a_refused_issue_label_firing_is_held_with_a_hashed_event_key() {
    let w = world(true, hung()).await;
    add_tracker_origin(&w).await;
    let reader = Issues::new(Vec::new());
    connect_reader(&w, reader.clone()).await;
    let id = issue_crew(&w, "Held labels", "bug", true).await;
    waiting_merges(&w, 3).await;
    capacity(&w, 2, true);
    // The first pass is the baseline: the marker only, and nothing is held.
    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    assert_eq!(claims(&w), [baseline_identity(&id, "bug")]);
    assert!(
        w.core
            .store
            .crew_paused_starts(&project(&w))
            .unwrap()
            .is_empty()
    );

    // A label event arrives and review capacity refuses its start. The issue's title is outside text; the held row
    // keeps hashes of the scope, key and label, and none of the title.
    reader
        .issues
        .lock()
        .unwrap()
        .push(labelled("7", POISON, &["bug"], "2026-11-01T00:00:00Z"));
    w.core.issues.clear_cache();
    w.core
        .crew_background_listeners_tick(NOW + 60)
        .await
        .unwrap();
    assert!(runs_of(&w, &id).is_empty());
    assert_eq!(claims(&w).len(), 2);
    assert_eq!(
        audit_actions(&w.core, "crew.background.listener.refused").len(),
        1
    );
    let rows = w.core.store.crew_paused_starts(&project(&w)).unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].kind, PausedKind::Background);
    assert_eq!(
        rows[0].key,
        format!(
            "{id}:issue_label:{}:{}:{}",
            event_tag("team/app"),
            event_tag("7"),
            event_tag("bug")
        )
    );
    assert!(rows[0].key.len() <= 128);
    assert!(
        !rows[0].key.contains("team/app")
            && !rows[0].key.contains(POISON)
            && !rows[0].key.contains("bug"),
        "outside text and the label stay out of the held key: {}",
        rows[0].key
    );
}

#[tokio::test]
async fn an_off_issue_crew_is_never_read_and_spends_nothing() {
    let w = world(true, hung()).await;
    add_tracker_origin(&w).await;
    let reader = Issues::new(vec![labelled("7", "Bug", &["bug"], "2026-11-01T00:00:00Z")]);
    connect_reader(&w, reader.clone()).await;
    let id = issue_crew(&w, "Off labels", "bug", false).await;
    let before = threads(&w);
    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    assert_eq!(
        reader.lists.load(Ordering::SeqCst),
        0,
        "an off crew is not read"
    );
    assert!(claims(&w).is_empty() && runs_of(&w, &id).is_empty());
    assert_eq!(threads(&w), before);

    // Turned on, the label that was already there is baselined: no run.
    w.core.background_set_enabled(&id, true).unwrap();
    w.core
        .crew_background_listeners_tick(NOW + 60)
        .await
        .unwrap();
    assert!(runs_of(&w, &id).is_empty(), "an old label starts nothing");
    assert!(reader.lists.load(Ordering::SeqCst) > 0);
    assert_eq!(claims(&w).len(), 2, "the marker and the old label's claim");

    // Only a label event after that starts one run: nothing was spent while the crew was off, and old labels are
    // never events.
    reader
        .issues
        .lock()
        .unwrap()
        .push(labelled("8", "New bug", &["bug"], "2026-11-02T00:00:00Z"));
    w.core.issues.clear_cache();
    w.core
        .crew_background_listeners_tick(NOW + 120)
        .await
        .unwrap();
    assert_eq!(runs_of(&w, &id).len(), 1);
}

#[tokio::test]
async fn a_project_with_no_tracker_reader_spends_nothing() {
    let w = world(true, hung()).await;
    // No origin remote, so no tracker can name this project: nothing is read, claimed or audited.
    let id = issue_crew(&w, "No reader", "bug", true).await;
    assert!(
        w.core
            .project_issue_reader(&w.core.store.project(&project(&w)).unwrap())
            .await
            .unwrap()
            .is_none()
    );
    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    assert!(claims(&w).is_empty() && runs_of(&w, &id).is_empty());
    assert!(audit_actions(&w.core, "crew.background.listener.refused").is_empty());
}

#[tokio::test]
async fn an_unreadable_tracker_spends_nothing_and_writes_no_refusal() {
    let w = world(true, hung()).await;
    add_tracker_origin(&w).await;
    connect_reader(&w, Issues::unreadable()).await;
    let id = issue_crew(&w, "Unreadable", "bug", true).await;
    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    assert!(claims(&w).is_empty() && runs_of(&w, &id).is_empty());
    assert!(
        audit_actions(&w.core, "crew.background.listener.refused").is_empty(),
        "a tracker that cannot answer is not a refusal"
    );
}

#[tokio::test]
async fn outside_issue_text_selects_the_run_and_never_becomes_its_prompt() {
    let w = world(true, good()).await;
    add_tracker_origin(&w).await;
    let reader = Issues::new(Vec::new());
    connect_reader(&w, reader.clone()).await;
    let id = issue_crew(&w, "Fix CI", "bug", true).await;
    // Baseline first, then the poisoned label event selects the run.
    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    reader
        .issues
        .lock()
        .unwrap()
        .push(labelled("7", POISON, &["bug"], "2026-11-01T00:00:00Z"));
    w.core.issues.clear_cache();
    w.core
        .crew_background_listeners_tick(NOW + 60)
        .await
        .unwrap();
    let run = until_run(&w, &id, wire::BackgroundRunState::Running).await;
    let task = task_of(&w, &run);
    w.until_state(&task, "passed").await;
    until_run(&w, &id, wire::BackgroundRunState::NeedsReview).await;

    // The brief is the code-owned one Run now uses: no issue title, body or tracker text in it.
    let brief = serde_json::to_string(&w.core.store.crew_brief(&task).unwrap().unwrap()).unwrap();
    assert!(
        !brief.contains(POISON),
        "the issue's title never reaches the brief"
    );
    let worker = w.worker(&task);
    let prompts = w.sent_to(&worker);
    assert!(!prompts.is_empty(), "the worker was prompted once");
    for prompt in &prompts {
        assert!(
            !prompt.contains(POISON),
            "the issue's title never reaches a prompt"
        );
        assert!(
            prompt.contains("Find why the latest CI run"),
            "the code-owned outcome is the prompt's: {prompt}"
        );
    }
    assert!(
        w.sent_to(&run.id).is_empty(),
        "the run's lead thread is never prompted"
    );
    // The claim keeps the issue's key and a hash of the label, never the title.
    let claimed = claims(&w);
    assert_eq!(
        claimed.len(),
        2,
        "the baseline marker and the issue's claim"
    );
    assert!(!claimed.iter().any(|identity| identity.contains(POISON)));
}

#[tokio::test]
async fn an_untrusted_project_spends_nothing_on_the_issue_path() {
    let w = world(true, hung()).await;
    add_tracker_origin(&w).await;
    let reader = Issues::new(vec![labelled("7", "Bug", &["bug"], "2026-11-01T00:00:00Z")]);
    connect_reader(&w, reader.clone()).await;
    let id = issue_crew(&w, "Untrusted", "bug", true).await;
    w.core.store.untrust_project_for_test(&project(&w));
    let before = threads(&w);
    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    assert_eq!(
        reader.lists.load(Ordering::SeqCst),
        0,
        "an untrusted project is not read"
    );
    assert!(claims(&w).is_empty() && runs_of(&w, &id).is_empty());
    assert!(audit_actions(&w.core, "crew.background.listener.refused").is_empty());
    assert_eq!(threads(&w), before);
}

#[tokio::test]
async fn a_reader_answers_only_for_its_own_project_scope() {
    let w = world(true, hung()).await;
    add_tracker_origin(&w).await;
    // A second project on the same tracker host, with its own scope.
    let other = w.core.new_project("other", false, None).await.unwrap();
    w.core.store.trust_project(&other.id, "test").unwrap();
    // The returned row is from before the trust; read it back so the trusted flag is the stored one.
    let other = w.core.store.project(&other.id).unwrap();
    let other_repo = std::path::PathBuf::from(&other.path);
    run_git(
        &other_repo,
        &[
            "remote",
            "add",
            "origin",
            "https://gitlab.example.test/team/other.git",
        ],
    );
    let reader = Issues::new(vec![labelled("7", "Bug", &["bug"], "2026-11-01T00:00:00Z")]);
    connect_reader(&w, reader.clone()).await;
    let mine = issue_crew(&w, "Mine", "bug", true).await;
    let theirs = issue_crew_in(&w, &other.id, "Theirs", "bug", true).await;

    // Each reader is the project's own: the lead's checkout is team/app, the other project team/other.
    let mine_project = w.core.store.project(&project(&w)).unwrap();
    assert_eq!(
        w.core
            .project_issue_reader(&mine_project)
            .await
            .unwrap()
            .unwrap()
            .scope,
        "team/app"
    );
    assert_eq!(
        w.core
            .project_issue_reader(&other)
            .await
            .unwrap()
            .unwrap()
            .scope,
        "team/other"
    );

    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    let mut scopes = reader.scopes.lock().unwrap().clone();
    scopes.sort();
    assert_eq!(
        scopes,
        ["team/app", "team/other"],
        "each crew reads only its project's scope"
    );
    assert!(runs_of(&w, &mine).is_empty() && runs_of(&w, &theirs).is_empty());
    assert_eq!(claims(&w).len(), 4, "two markers and one issue claim each");
}

#[tokio::test]
async fn labels_added_while_a_crew_was_off_fire_once_per_pass_when_it_is_back_on() {
    let w = world(true, good()).await;
    add_tracker_origin(&w).await;
    let reader = Issues::new(Vec::new());
    connect_reader(&w, reader.clone()).await;
    let id = issue_crew(&w, "Toggled", "bug", true).await;
    // The first pass baselines an empty page: the marker only.
    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    assert_eq!(claims(&w), [baseline_identity(&id, "bug")]);

    // Off, then two labels appear. Turning it back on does not baseline again: they fire, one per pass.
    w.core.background_set_enabled(&id, false).unwrap();
    reader.issues.lock().unwrap().extend([
        labelled("8", "Newer", &["bug"], "2026-11-02T00:00:00Z"),
        labelled("7", "Older", &["bug"], "2026-11-01T00:00:00Z"),
    ]);
    w.core.background_set_enabled(&id, true).unwrap();
    w.core.issues.clear_cache();
    w.core
        .crew_background_listeners_tick(NOW + 60)
        .await
        .unwrap();
    assert_eq!(runs_of(&w, &id).len(), 1, "one start per pass");
    assert_eq!(
        claims(&w),
        [
            baseline_identity(&id, "bug"),
            issue_identity(&id, "8", "bug", &["bug"])
        ]
    );
    let run = until_run(&w, &id, wire::BackgroundRunState::Running).await;
    let task = task_of(&w, &run);
    w.until_state(&task, "passed").await;
    until_run(&w, &id, wire::BackgroundRunState::NeedsReview).await;

    // The next pass reaches the second label event.
    w.core
        .crew_background_listeners_tick(NOW + 120)
        .await
        .unwrap();
    assert_eq!(runs_of(&w, &id).len(), 2);
    assert_eq!(claims(&w).len(), 3);
}

#[tokio::test]
async fn a_dependency_update_crew_without_a_reader_feed_spends_nothing() {
    let w = world(true, hung()).await;
    add_tracker_origin(&w).await;
    let reader = Issues::new(Vec::new());
    connect_reader(&w, reader.clone()).await;
    let id = dependency_crew(&w, "Deps", None, true).await;
    let before = threads(&w);
    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    assert_eq!(
        reader.dependency_reads.load(Ordering::SeqCst),
        1,
        "the tick asks the project's reader"
    );
    assert_eq!(reader.lists.load(Ordering::SeqCst), 0, "no issue is listed");
    assert!(
        claims(&w).is_empty() && runs_of(&w, &id).is_empty(),
        "a tracker that cannot answer spends no event"
    );
    assert!(audit_actions(&w.core, "crew.background.listener.refused").is_empty());
    assert!(
        w.core
            .store
            .crew_paused_starts(&project(&w))
            .unwrap()
            .is_empty()
    );
    assert_eq!(threads(&w), before);
}

#[tokio::test]
async fn a_dependency_feed_baselines_on_its_first_pass_then_starts_one_run_for_a_new_update() {
    let w = world(true, good()).await;
    add_tracker_origin(&w).await;
    // The fake feed proves the claim, baseline and start path the listener uses for any reader that answers Updates.
    let reader = Issues::new(Vec::new());
    *reader.dependency.lock().unwrap() = DependencyFeed::Updates(vec![
        DependencyUpdate {
            key: "alert-1".into(),
            ecosystem: Some("cargo".into()),
        },
        DependencyUpdate {
            key: "alert-2".into(),
            ecosystem: Some("npm".into()),
        },
    ]);
    connect_reader(&w, reader.clone()).await;
    let id = dependency_crew(&w, "Deps", Some("cargo"), true).await;

    // The first feed is a baseline: the crew's own ecosystem is claimed, the marker is kept, and nothing starts.
    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    assert!(runs_of(&w, &id).is_empty(), "a baseline starts nothing");
    let mut expected = vec![
        format!(
            "dependency_update:{id}:gitlab:team/app:baseline:{}",
            event_tag("cargo")
        ),
        format!(
            "dependency_update:{id}:gitlab:team/app:{}",
            event_tag("alert-1")
        ),
    ];
    expected.sort();
    assert_eq!(
        claims(&w),
        expected,
        "the crew's ecosystem selects its own update only"
    );

    // A repeat is inert: the update was claimed by the baseline.
    w.core
        .crew_background_listeners_tick(NOW + 60)
        .await
        .unwrap();
    assert!(runs_of(&w, &id).is_empty());
    assert_eq!(claims(&w).len(), 2);

    // A new update on the crew's ecosystem starts exactly one run.
    *reader.dependency.lock().unwrap() = DependencyFeed::Updates(vec![
        DependencyUpdate {
            key: "alert-1".into(),
            ecosystem: Some("cargo".into()),
        },
        DependencyUpdate {
            key: "alert-2".into(),
            ecosystem: Some("npm".into()),
        },
        DependencyUpdate {
            key: "alert-3".into(),
            ecosystem: Some("cargo".into()),
        },
    ]);
    w.core
        .crew_background_listeners_tick(NOW + 120)
        .await
        .unwrap();
    assert_eq!(runs_of(&w, &id).len(), 1);
    let mut expected = vec![
        format!(
            "dependency_update:{id}:gitlab:team/app:baseline:{}",
            event_tag("cargo")
        ),
        format!(
            "dependency_update:{id}:gitlab:team/app:{}",
            event_tag("alert-1")
        ),
        format!(
            "dependency_update:{id}:gitlab:team/app:{}",
            event_tag("alert-3")
        ),
    ];
    expected.sort();
    assert_eq!(claims(&w), expected);
}

#[tokio::test]
async fn a_cannot_feed_keeps_no_marker_and_spends_nothing() {
    let w = world(true, hung()).await;
    add_tracker_origin(&w).await;
    let reader = Issues::new(Vec::new());
    *reader.dependency.lock().unwrap() = DependencyFeed::Cannot;
    connect_reader(&w, reader.clone()).await;
    let id = dependency_crew(&w, "Deps", None, true).await;

    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    assert_eq!(reader.dependency_reads.load(Ordering::SeqCst), 1);
    assert!(
        claims(&w).is_empty(),
        "a tracker that cannot answer keeps no marker and spends no event"
    );
    assert!(runs_of(&w, &id).is_empty());

    // No marker was kept: the first feed this crew gets is still its baseline, so the open update starts nothing.
    *reader.dependency.lock().unwrap() = DependencyFeed::Updates(vec![DependencyUpdate {
        key: "alert-1".into(),
        ecosystem: None,
    }]);
    w.core
        .crew_background_listeners_tick(NOW + 60)
        .await
        .unwrap();
    assert!(runs_of(&w, &id).is_empty(), "the first feed is a baseline");
    let mut expected = vec![
        format!(
            "dependency_update:{id}:gitlab:team/app:baseline:{}",
            event_tag("")
        ),
        format!(
            "dependency_update:{id}:gitlab:team/app:{}",
            event_tag("alert-1")
        ),
    ];
    expected.sort();
    assert_eq!(claims(&w), expected);
}

#[tokio::test]
async fn an_ecosystem_filtered_crew_baselines_only_its_own_ecosystem() {
    let w = world(true, hung()).await;
    add_tracker_origin(&w).await;
    let reader = Issues::new(Vec::new());
    *reader.dependency.lock().unwrap() = DependencyFeed::Updates(vec![
        DependencyUpdate {
            key: "alert-1".into(),
            ecosystem: Some("cargo".into()),
        },
        DependencyUpdate {
            key: "alert-2".into(),
            ecosystem: Some("npm".into()),
        },
        DependencyUpdate {
            key: "alert-3".into(),
            ecosystem: None,
        },
    ]);
    connect_reader(&w, reader.clone()).await;
    let id = dependency_crew(&w, "Deps", Some("cargo"), true).await;

    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    assert!(runs_of(&w, &id).is_empty(), "a baseline starts nothing");
    let mut expected = vec![
        format!(
            "dependency_update:{id}:gitlab:team/app:baseline:{}",
            event_tag("cargo")
        ),
        format!(
            "dependency_update:{id}:gitlab:team/app:{}",
            event_tag("alert-1")
        ),
    ];
    expected.sort();
    assert_eq!(
        claims(&w),
        expected,
        "only the crew's own ecosystem is claimed"
    );
}

#[tokio::test]
async fn a_crew_pointed_at_another_repository_baselines_that_repositorys_alerts() {
    let w = world(true, good()).await;
    add_tracker_origin(&w).await;
    let reader = Issues::new(Vec::new());
    *reader.dependency.lock().unwrap() = DependencyFeed::Updates(vec![DependencyUpdate {
        key: "1".into(),
        ecosystem: Some("cargo".into()),
    }]);
    connect_reader(&w, reader.clone()).await;
    let id = dependency_crew(&w, "Deps", Some("cargo"), true).await;
    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    assert!(runs_of(&w, &id).is_empty(), "the first feed is a baseline");

    // The project's remote moves to another repository whose open alerts reuse number 1 and add number 2.
    run_git(
        &w.repo,
        &[
            "remote",
            "set-url",
            "origin",
            "https://gitlab.example.test/team/other.git",
        ],
    );
    *reader.dependency.lock().unwrap() = DependencyFeed::Updates(vec![
        DependencyUpdate {
            key: "1".into(),
            ecosystem: Some("cargo".into()),
        },
        DependencyUpdate {
            key: "2".into(),
            ecosystem: Some("cargo".into()),
        },
    ]);
    w.core.issues.clear_cache();
    w.core
        .crew_background_listeners_tick(NOW + 60)
        .await
        .unwrap();
    assert!(
        runs_of(&w, &id).is_empty(),
        "the new repository is baselined, not replayed one alert per pass"
    );
    let claimed = claims(&w);
    for key in ["1", "2"] {
        assert!(claimed.contains(&format!(
            "dependency_update:{id}:gitlab:team/other:{}",
            event_tag(key)
        )));
    }

    // An alert opened there after the baseline starts one run.
    *reader.dependency.lock().unwrap() = DependencyFeed::Updates(vec![DependencyUpdate {
        key: "3".into(),
        ecosystem: Some("cargo".into()),
    }]);
    w.core
        .crew_background_listeners_tick(NOW + 120)
        .await
        .unwrap();
    assert_eq!(runs_of(&w, &id).len(), 1);
}

#[tokio::test]
async fn the_baseline_reads_every_page_before_it_is_kept() {
    let w = world(true, good()).await;
    add_tracker_origin(&w).await;
    let reader = Issues::new(vec![
        labelled("7", "One", &["bug"], "2026-11-03T00:00:00Z"),
        labelled("8", "Two", &["bug"], "2026-11-02T00:00:00Z"),
        labelled("9", "Three", &["bug"], "2026-11-01T00:00:00Z"),
    ]);
    reader.page_size.store(2, Ordering::SeqCst);
    connect_reader(&w, reader.clone()).await;
    let id = issue_crew(&w, "Labels", "bug", true).await;

    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    assert!(runs_of(&w, &id).is_empty(), "a baseline starts nothing");
    assert_eq!(
        claims(&w),
        [
            baseline_identity(&id, "bug"),
            issue_identity(&id, "7", "bug", &["bug"]),
            issue_identity(&id, "8", "bug", &["bug"]),
            issue_identity(&id, "9", "bug", &["bug"]),
        ],
        "the issue on the second page is part of the baseline"
    );

    // A title edit that moves the old issue onto the first page is not a label event.
    *reader.issues.lock().unwrap() = vec![
        labelled("9", "Three, edited", &["bug"], "2026-11-04T00:00:00Z"),
        labelled("7", "One", &["bug"], "2026-11-03T00:00:00Z"),
        labelled("8", "Two", &["bug"], "2026-11-02T00:00:00Z"),
    ];
    w.core.issues.clear_cache();
    w.core
        .crew_background_listeners_tick(NOW + 60)
        .await
        .unwrap();
    assert!(runs_of(&w, &id).is_empty(), "an edit is inert");
}

#[tokio::test]
async fn changing_a_crews_ecosystem_baselines_the_alerts_already_open_there() {
    let w = world(true, hung()).await;
    add_tracker_origin(&w).await;
    let reader = Issues::new(Vec::new());
    *reader.dependency.lock().unwrap() = DependencyFeed::Updates(vec![
        DependencyUpdate {
            key: "alert-1".into(),
            ecosystem: Some("cargo".into()),
        },
        DependencyUpdate {
            key: "alert-2".into(),
            ecosystem: Some("npm".into()),
        },
    ]);
    connect_reader(&w, reader.clone()).await;
    // The stored ecosystem is written in another case than the feed's: it still selects.
    let id = dependency_crew(&w, "Deps", Some("Cargo"), true).await;
    w.core.crew_background_listeners_tick(NOW).await.unwrap();
    assert!(runs_of(&w, &id).is_empty());
    assert!(
        claims(&w).contains(&format!(
            "dependency_update:{id}:gitlab:team/app:{}",
            event_tag("alert-1")
        )),
        "{:?}",
        claims(&w)
    );

    // Pointed at npm, the crew baselines the npm alert that was already open instead of firing it.
    let update: BackgroundCrewUpdate = serde_json::from_value(
        json!({ "trigger": { "kind": "dependency_update", "ecosystem": "npm" } }),
    )
    .unwrap();
    w.core.background_update(&id, &update).await.unwrap();
    w.core
        .crew_background_listeners_tick(NOW + 60)
        .await
        .unwrap();
    assert!(
        runs_of(&w, &id).is_empty(),
        "an alert already open never fires"
    );
    assert!(claims(&w).contains(&format!(
        "dependency_update:{id}:gitlab:team/app:{}",
        event_tag("alert-2")
    )));
}
