//! Issues through a real core: connections keep no token, a listing merges trackers, links and the thread
//! start leave nothing behind when they fail (docs/issues.md).

use super::harness::{Turn, world};
use crate::credentials::{Credentials, Failure, SecretBackend};
use crate::error::{CoreError, Result};
use crate::issues::{IssueQuery, IssueSource, ScopeList};
use crate::store::TrackerConnection;
use async_trait::async_trait;
use orbyn_contracts::{
    ConnectTracker, IssueCounts, IssueDetail, IssueKey, IssueState, IssueStateCategory,
    IssueSummary, IssueTemplate, StartIssueThread, TrackerKind,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

#[derive(Default)]
struct Memory(Mutex<HashMap<(String, String), Vec<u8>>>);

impl SecretBackend for Memory {
    fn get(&self, s: &str, a: &str) -> std::result::Result<Option<Zeroizing<Vec<u8>>>, Failure> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(&(s.into(), a.into()))
            .cloned()
            .map(Zeroizing::new))
    }
    fn set(&self, s: &str, a: &str, b: &[u8]) -> std::result::Result<(), Failure> {
        self.0
            .lock()
            .unwrap()
            .insert((s.into(), a.into()), b.to_vec());
        Ok(())
    }
    fn delete(&self, s: &str, a: &str) -> std::result::Result<(), Failure> {
        self.0.lock().unwrap().remove(&(s.into(), a.into()));
        Ok(())
    }
}

fn issue(tracker: TrackerKind, key: &str, updated: &str) -> IssueSummary {
    IssueSummary {
        tracker,
        scope: "team/app".into(),
        key: key.into(),
        display_key: format!("#{key}"),
        title: format!("Issue {key}"),
        url: format!("https://example.test/{key}"),
        author: None,
        state: IssueState {
            category: IssueStateCategory::Open,
            name: "Open".into(),
            close_reason: None,
        },
        created_at: "2026-11-01T00:00:00Z".into(),
        updated_at: updated.into(),
        closed_at: None,
        assignees: vec![],
        labels: vec![],
        comment_count: 0,
    }
}

struct Fake {
    tracker: TrackerKind,
    keys: Vec<(&'static str, &'static str)>,
    fail: bool,
    lists: AtomicUsize,
}

impl Fake {
    fn new(tracker: TrackerKind, keys: &[(&'static str, &'static str)], fail: bool) -> Arc<Self> {
        Arc::new(Self {
            tracker,
            keys: keys.to_vec(),
            fail,
            lists: AtomicUsize::new(0),
        })
    }
}

#[async_trait]
impl IssueSource for Fake {
    async fn list(&self, _scope: &str, _query: &IssueQuery) -> Result<ScopeList> {
        self.lists.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(CoreError::TrackerUnavailable(
                "The tracker did not answer.".into(),
            ));
        }
        Ok(ScopeList {
            issues: self
                .keys
                .iter()
                .map(|(k, u)| issue(self.tracker, k, u))
                .collect(),
            counts: Some(IssueCounts { open: 2, closed: 5 }),
            next: None,
        })
    }
    async fn get(&self, scope: &str, key: &str) -> Result<IssueDetail> {
        let (k, u) = self
            .keys
            .iter()
            .find(|(k, _)| *k == key)
            .ok_or_else(|| CoreError::NotFound("issue".into()))?;
        let mut summary = issue(self.tracker, k, u);
        summary.scope = scope.to_owned();
        Ok(IssueDetail {
            summary,
            body: "It breaks.".into(),
            comments: vec![],
            linked_pull_requests: vec![],
            linked_threads: vec![],
            reactions: None,
            states: None,
        })
    }
}

fn connect(host: Option<&str>, token: Option<&str>) -> ConnectTracker {
    ConnectTracker {
        host: host.map(str::to_owned),
        scope: Some("team/app".into()),
        token: token.map(str::to_owned),
    }
}

/// A source that has another page in every scope, to exercise the cap on emitted cursors.
struct EveryScope;

#[async_trait]
impl IssueSource for EveryScope {
    async fn list(&self, _scope: &str, _query: &IssueQuery) -> Result<ScopeList> {
        Ok(ScopeList {
            issues: vec![],
            counts: None,
            next: Some("2.cursor".into()),
        })
    }
    async fn get(&self, _scope: &str, _key: &str) -> Result<IssueDetail> {
        Err(CoreError::NotFound("issue".into()))
    }
}

fn many_scopes(scopes: Vec<String>) -> TrackerConnection {
    TrackerConnection {
        tracker: TrackerKind::Gitlab,
        host: Some("https://gitlab.example.test".into()),
        scopes,
    }
}

async fn ready() -> (super::harness::World, Arc<Fake>, Arc<Fake>) {
    let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .issues
        .set_credentials(Credentials::new(Arc::new(Memory::default()), "test"));
    let gitlab = Fake::new(
        TrackerKind::Gitlab,
        &[("1", "2026-11-03T00:00:00Z"), ("2", "2026-11-01T00:00:00Z")],
        false,
    );
    let forgejo = Fake::new(
        TrackerKind::Forgejo,
        &[("9", "2026-11-02T00:00:00Z")],
        false,
    );
    w.core
        .issues
        .set_source(TrackerKind::Gitlab, gitlab.clone());
    w.core
        .issues
        .set_source(TrackerKind::Forgejo, forgejo.clone());
    w.core
        .connect_issue_tracker(TrackerKind::Gitlab, connect(None, Some("glpat-secret123")))
        .await
        .unwrap();
    w.core
        .connect_issue_tracker(
            TrackerKind::Forgejo,
            connect(Some("https://code.example.org"), Some("fj-secret456")),
        )
        .await
        .unwrap();
    (w, gitlab, forgejo)
}

#[tokio::test]
async fn a_connection_keeps_its_host_and_never_the_token() {
    let (w, _, _) = ready().await;
    let rows = w.core.issue_trackers().await.unwrap();
    let text = serde_json::to_string(&rows).unwrap();
    assert!(!text.contains("secret"), "{text}");
    let gitlab = rows
        .iter()
        .find(|r| r.tracker == TrackerKind::Gitlab)
        .unwrap();
    assert!(gitlab.connected);
    assert_eq!(
        gitlab.host.as_deref(),
        Some("https://gitlab.com"),
        "GitLab defaults to gitlab.com"
    );
    let forgejo = rows
        .iter()
        .find(|r| r.tracker == TrackerKind::Forgejo)
        .unwrap();
    assert_eq!(forgejo.host.as_deref(), Some("https://code.example.org"));
    let stored = format!("{:?}", w.core.store.tracker_connections().unwrap());
    assert!(!stored.contains("secret"), "{stored}");
}

#[tokio::test]
async fn a_bad_host_or_a_missing_token_is_refused_and_saves_nothing() {
    let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
    w.core
        .issues
        .set_credentials(Credentials::new(Arc::new(Memory::default()), "test"));
    for (kind, input) in [
        (
            TrackerKind::Gitlab,
            connect(Some("http://gitlab.com"), Some("tok")),
        ),
        (
            TrackerKind::Gitlab,
            connect(Some("https://user:pw@gitlab.com"), Some("tok")),
        ),
        (TrackerKind::Forgejo, connect(None, Some("tok"))),
        (
            TrackerKind::Linear,
            connect(Some("https://evil.example"), Some("tok")),
        ),
        (TrackerKind::Gitlab, connect(None, None)),
        (TrackerKind::Gitlab, connect(None, Some("has space"))),
        (TrackerKind::Github, connect(None, Some("tok"))),
    ] {
        assert!(
            w.core.connect_issue_tracker(kind, input).await.is_err(),
            "{kind:?}"
        );
    }
    assert!(w.core.store.tracker_connections().unwrap().is_empty());
}

#[tokio::test]
async fn a_listing_merges_trackers_newest_first_and_reads_each_scope_once_in_thirty_seconds() {
    let (w, gitlab, _) = ready().await;
    let list = w.core.list_issues(&IssueQuery::default()).await.unwrap();
    let keys: Vec<&str> = list.issues.iter().map(|i| i.key.as_str()).collect();
    assert_eq!(keys, ["1", "9", "2"], "newest update first across trackers");
    assert_eq!(
        list.counts,
        Some(IssueCounts {
            open: 4,
            closed: 10
        })
    );
    w.core.list_issues(&IssueQuery::default()).await.unwrap();
    assert_eq!(
        gitlab.lists.load(Ordering::SeqCst),
        1,
        "the second read came from the cache"
    );
    let only = w
        .core
        .list_issues(&IssueQuery {
            tracker: Some(TrackerKind::Forgejo),
            ..IssueQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(only.issues.len(), 1);
}

#[tokio::test]
async fn next_cursors_are_capped_to_fifty_and_four_kibibytes() {
    let (w, _, _) = ready().await;
    w.core
        .issues
        .set_source(TrackerKind::Gitlab, Arc::new(EveryScope));
    let short: Vec<String> = (0..60).map(|i| format!("grp/p{i:02}")).collect();
    w.core
        .store
        .put_tracker(&many_scopes(short.clone()), "2026-11-01T00:00:00Z")
        .unwrap();
    let list = w
        .core
        .list_issues(&IssueQuery {
            tracker: Some(TrackerKind::Gitlab),
            ..IssueQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(list.next_cursors.len(), 50, "at most fifty cursors");
    let expected: Vec<String> = short
        .iter()
        .take(50)
        .map(|scope| format!("gitlab {scope}"))
        .collect();
    assert_eq!(
        list.next_cursors.keys().cloned().collect::<Vec<_>>(),
        expected,
        "the kept cursors are the first in key order"
    );

    let long: Vec<String> = (0..60)
        .map(|i| format!("grp/{i:02}{}", "x".repeat(150)))
        .collect();
    w.core
        .store
        .put_tracker(&many_scopes(long.clone()), "2026-11-01T00:00:00Z")
        .unwrap();
    let list = w
        .core
        .list_issues(&IssueQuery {
            tracker: Some(TrackerKind::Gitlab),
            ..IssueQuery::default()
        })
        .await
        .unwrap();
    let json = serde_json::to_string(&list.next_cursors).unwrap();
    let kept = list.next_cursors.len();
    assert!(json.len() <= 4096, "{} bytes of cursors", json.len());
    assert!(kept < 50, "the bytes cap bites before the count cap");
    let expected: Vec<String> = long
        .iter()
        .take(kept)
        .map(|scope| format!("gitlab {scope}"))
        .collect();
    assert_eq!(
        list.next_cursors.keys().cloned().collect::<Vec<_>>(),
        expected,
        "the kept cursors are the first in key order"
    );
}

#[tokio::test]
async fn a_tracker_that_fails_is_reported_while_another_answers() {
    let (w, _, _) = ready().await;
    w.core.issues.set_source(
        TrackerKind::Forgejo,
        Fake::new(TrackerKind::Forgejo, &[], true),
    );
    let list = w
        .core
        .list_issues(&IssueQuery {
            text: "x".into(),
            ..IssueQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(list.issues.len(), 2, "GitLab still answered");
    assert_eq!(list.failed.len(), 1);
    assert_eq!(list.failed[0].tracker, TrackerKind::Forgejo);
    assert_eq!(list.failed[0].message, "The tracker did not answer.");
    assert_eq!(
        list.counts, None,
        "totals are not claimed when one tracker could not say"
    );
}

#[tokio::test]
async fn a_link_is_added_read_back_and_removed_from_that_thread_only() {
    let (w, _, _) = ready().await;
    let key = IssueKey {
        tracker: TrackerKind::Gitlab,
        scope: "team/app".into(),
        key: "1".into(),
    };
    w.core.link_issue(&w.lead, &key).await.unwrap();
    w.core.link_issue(&w.lead, &key).await.unwrap();
    let links = w.core.issue_links().unwrap();
    assert_eq!(links[&w.lead].len(), 1, "linking twice is one link");
    assert_eq!(links[&w.lead][0].display_key, "#1");
    assert_eq!(
        w.core.issue(&key).await.unwrap().linked_threads,
        std::slice::from_ref(&w.lead)
    );
    w.core.unlink_issue(&w.lead, &key).unwrap();
    assert!(w.core.issue_links().unwrap().is_empty());
    assert!(w.core.link_issue("no-such-thread", &key).await.is_err());
    let missing = IssueKey {
        key: "404".into(),
        ..key.clone()
    };
    assert!(
        matches!(
            w.core.link_issue(&w.lead, &missing).await,
            Err(CoreError::NotFound(_))
        ),
        "a number the tracker lacks"
    );
}

#[tokio::test]
async fn a_scope_outside_the_connection_is_refused_and_so_is_a_tracker_not_connected() {
    let (w, _, _) = ready().await;
    let outside = IssueKey {
        tracker: TrackerKind::Gitlab,
        scope: "someone/else".into(),
        key: "1".into(),
    };
    assert!(matches!(
        w.core.issue(&outside).await,
        Err(CoreError::Invalid(_))
    ));
    let linear = IssueKey {
        tracker: TrackerKind::Linear,
        scope: "ENG".into(),
        key: "ENG-1".into(),
    };
    assert!(matches!(
        w.core.issue(&linear).await,
        Err(CoreError::TrackerNotConnected(_))
    ));
}

#[tokio::test]
async fn the_draft_frames_the_issue_text_as_data() {
    let (w, _, _) = ready().await;
    let key = IssueKey {
        tracker: TrackerKind::Gitlab,
        scope: "team/app".into(),
        key: "1".into(),
    };
    let draft = w.core.issue_draft(&key).await.unwrap().message;
    assert!(
        draft.starts_with("Work on #1 \"Issue 1\" (gitlab team/app,"),
        "{draft}"
    );
    assert!(
        draft.contains("<<<ISSUE TEXT\nIt breaks.\nISSUE TEXT>>>"),
        "{draft}"
    );
}

#[tokio::test]
async fn a_thread_start_that_cannot_finish_creates_no_thread() {
    let (w, _, _) = ready().await;
    let before = w
        .core
        .store
        .list_threads(None, false)
        .map(|t| t.len())
        .unwrap_or(0);
    let start = |message: &str, project: &str, key: &str| StartIssueThread {
        tracker: TrackerKind::Gitlab,
        scope: "team/app".into(),
        key: key.into(),
        message: message.into(),
        project_id: project.into(),
    };
    let project = w.core.store.thread(&w.lead).unwrap().project_id;
    assert!(
        w.core
            .start_issue_thread(start("  ", &project, "1"))
            .await
            .is_err(),
        "an empty message"
    );
    assert!(
        w.core
            .start_issue_thread(start("go", "no-project", "1"))
            .await
            .is_err(),
        "an unknown project"
    );
    assert!(
        w.core
            .start_issue_thread(start("go", &project, "404"))
            .await
            .is_err(),
        "an issue the tracker lacks"
    );
    assert!(
        w.core
            .start_issue_thread(start("go", &project, "1"))
            .await
            .is_err(),
        "no default engine is set"
    );
    assert_eq!(
        w.core
            .store
            .list_threads(None, false)
            .map(|t| t.len())
            .unwrap_or(0),
        before
    );
}

mod gitlab {
    use super::*;
    use crate::issues::GitLab;
    use crate::issues::fake_http::{Reply, Seen, Server};
    use serde_json::{Value, json};
    use std::time::Duration;

    fn row(req: &Seen, iid: u64) -> Value {
        json!({ "iid": iid, "title": format!("Issue {iid}"), "state": "opened",
            "web_url": format!("http://{}/team/app/-/issues/{iid}", req.header("host").unwrap_or_default()),
            "created_at": "2026-11-01T00:00:00Z", "updated_at": format!("2026-11-0{iid}T00:00:00Z"),
            "labels": [], "assignees": [], "user_notes_count": 0, "description": "Body" })
    }

    /// A GitLab connection whose reader talks to the fake server. The stored host is https, as it must be.
    async fn connected(server: &Server) -> super::super::harness::World {
        let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
        w.core
            .issues
            .set_credentials(Credentials::new(Arc::new(Memory::default()), "test"));
        w.core.issues.set_source(
            TrackerKind::Gitlab,
            Arc::new(GitLab::for_test(
                &server.base,
                "glpat-secret123",
                Duration::from_secs(5),
            )),
        );
        w.core
            .connect_issue_tracker(
                TrackerKind::Gitlab,
                connect(Some("https://gitlab.example.test"), Some("glpat-secret123")),
            )
            .await
            .unwrap();
        w
    }

    fn key(scope: &str) -> IssueKey {
        IssueKey {
            tracker: TrackerKind::Gitlab,
            scope: scope.into(),
            key: "1".into(),
        }
    }

    #[tokio::test]
    async fn a_scope_outside_the_connection_is_refused_with_no_request_sent() {
        let server = Server::start(|req| {
            if req.path().ends_with("/notes") {
                Reply::json(&json!([]))
            } else {
                Reply::json(&row(req, 1))
            }
        })
        .await;
        let w = connected(&server).await;
        let outside = w.core.issue(&key("other/proj")).await;
        assert!(matches!(outside, Err(CoreError::Invalid(_))), "{outside:?}");
        let link = w.core.issue_draft(&key("team/other")).await;
        assert!(matches!(link, Err(CoreError::Invalid(_))));
        assert!(server.seen().is_empty(), "nothing was sent");
        let detail = w.core.issue(&key("team/app")).await.unwrap();
        assert_eq!(detail.summary.display_key, "#1");
        assert!(
            server
                .seen()
                .iter()
                .any(|r| r.path() == "/api/v4/projects/team%2Fapp/issues/1")
        );
    }

    #[tokio::test]
    async fn a_failing_gitlab_is_reported_with_fixed_copy_and_a_wait_when_rate_limited() {
        let server = Server::start(|_| Reply::status(500)).await;
        let w = connected(&server).await;
        let list = w.core.list_issues(&IssueQuery::default()).await.unwrap();
        assert!(list.issues.is_empty());
        assert_eq!(list.failed.len(), 1);
        assert_eq!(list.failed[0].tracker, TrackerKind::Gitlab);
        assert_eq!(list.failed[0].message, "GitLab did not answer.");
        let limited = Server::start(|_| Reply::status(429).header("retry-after", "42")).await;
        w.core.issues.set_source(
            TrackerKind::Gitlab,
            Arc::new(GitLab::for_test(
                &limited.base,
                "glpat-secret123",
                Duration::from_secs(5),
            )),
        );
        let list = w.core.list_issues(&IssueQuery::default()).await.unwrap();
        assert!(list.failed[0].message.contains("42 seconds"), "{list:?}");
        let text = format!("{list:?}");
        assert!(!text.contains("secret"), "{text}");
        let one = w.core.issue(&key("team/app")).await;
        assert!(matches!(one, Err(CoreError::TrackerRateLimited(_))));
    }

    #[tokio::test]
    async fn carrying_on_reads_only_the_scopes_named_in_the_cursors() {
        let server = Server::start(|req| {
            if req.path().ends_with("/issues_statistics") {
                return Reply::status(500);
            }
            if req.param("page").as_deref() == Some("1") {
                Reply::json(&json!([row(req, 1)])).header("x-next-page", "2")
            } else {
                Reply::json(&json!([row(req, 2)])).header("x-next-page", "")
            }
        })
        .await;
        let w = connected(&server).await;
        let first = w.core.list_issues(&IssueQuery::default()).await.unwrap();
        assert_eq!(first.issues[0].key, "1");
        assert!(first.next_cursors.contains_key("gitlab team/app"));
        let second = w
            .core
            .list_issues(&IssueQuery {
                cursors: first.next_cursors.clone(),
                ..IssueQuery::default()
            })
            .await
            .unwrap();
        assert_eq!(second.issues.len(), 1);
        assert_eq!(second.issues[0].key, "2");
        assert!(second.next_cursors.is_empty());
        let before = server.seen().len();
        let other = w
            .core
            .list_issues(&IssueQuery {
                cursors: [("gitlab team/elsewhere".to_owned(), "2.x".to_owned())].into(),
                ..IssueQuery::default()
            })
            .await
            .unwrap();
        assert!(other.issues.is_empty() && other.failed.is_empty());
        assert_eq!(server.seen().len(), before, "an unnamed scope is not read");
        let forged = w
            .core
            .list_issues(&IssueQuery {
                cursors: [(
                    "gitlab team/app".to_owned(),
                    "2.0000000000000000".to_owned(),
                )]
                .into(),
                ..IssueQuery::default()
            })
            .await;
        assert!(
            matches!(&forged, Err(CoreError::Invalid(_))),
            "a cursor we did not issue is a 400, not a failed tracker: {forged:?}"
        );
        assert_eq!(server.seen().len(), before);
    }

    #[tokio::test]
    async fn a_comma_in_a_gitlab_label_is_a_listing_error_and_not_a_failed_tracker() {
        let server = Server::start(|_| Reply::json(&json!([]))).await;
        let w = connected(&server).await;
        let before = server.seen().len();
        let list = w
            .core
            .list_issues(&IssueQuery {
                tracker: Some(TrackerKind::Gitlab),
                labels: vec!["a,b".into()],
                ..IssueQuery::default()
            })
            .await;
        assert!(
            matches!(&list, Err(CoreError::Invalid(_))),
            "GitLab's comma-label refusal is a 400: {list:?}"
        );
        assert_eq!(server.seen().len(), before, "refused before any request");
    }

    #[tokio::test]
    async fn gitlab_source_needs_a_connection_and_refuses_a_stored_plain_http_host() {
        let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
        w.core
            .issues
            .set_credentials(Credentials::new(Arc::new(Memory::default()), "test"));
        let missing = w.core.gitlab_source().err().expect("no connection");
        assert!(
            matches!(missing, CoreError::TrackerNotConnected(_)),
            "{missing:?}"
        );
        w.core
            .connect_issue_tracker(
                TrackerKind::Gitlab,
                connect(Some("https://gitlab.example.test"), Some("glpat-secret123")),
            )
            .await
            .unwrap();
        w.core
            .store
            .put_tracker(
                &TrackerConnection {
                    tracker: TrackerKind::Gitlab,
                    host: Some("http://gitlab.example.test".into()),
                    scopes: vec!["team/app".into()],
                },
                "2026-11-01T00:00:00Z",
            )
            .unwrap();
        let plain = w.core.gitlab_source().err().expect("a plain http host");
        assert!(matches!(plain, CoreError::Invalid(_)), "{plain:?}");
    }

    #[tokio::test]
    async fn gitlab_source_sends_the_keychain_token_to_the_stored_connection_host() {
        let server = Server::start(|_| Reply::json(&json!([]))).await;
        let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
        w.core
            .issues
            .set_credentials(Credentials::new(Arc::new(Memory::default()), "test"));
        w.core
            .connect_issue_tracker(
                TrackerKind::Gitlab,
                connect(Some("https://gitlab.example.test"), Some("glpat-secret123")),
            )
            .await
            .unwrap();
        w.core.issues.set_gitlab_test_base(&server.base);
        let source = w.core.gitlab_source().expect("a GitLab source");
        source
            .list("team/app", &IssueQuery::default())
            .await
            .unwrap();
        let seen = server.seen();
        assert!(!seen.is_empty(), "the source sent a request");
        assert!(
            seen.iter()
                .all(|r| r.header("private-token") == Some("glpat-secret123")),
            "{seen:?}"
        );
    }
}

mod forgejo {
    use super::*;
    use crate::issues::Forgejo;
    use crate::issues::fake_http::{Reply, Seen, Server};
    use serde_json::{Value, json};
    use std::time::Duration;

    fn row(req: &Seen, number: u64) -> Value {
        json!({ "number": number, "title": format!("Issue {number}"), "state": "open",
            "html_url": format!("http://{}/team/app/issues/{number}", req.header("host").unwrap_or_default()),
            "created_at": "2026-11-01T00:00:00Z", "updated_at": format!("2026-11-0{number}T00:00:00Z"),
            "user": { "login": "ann" }, "labels": [], "assignees": [], "comments": 0, "body": "Body" })
    }

    /// A Forgejo connection whose reader talks to the fake server. The stored host is https, as it must be.
    async fn connected(server: &Server) -> super::super::harness::World {
        let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
        w.core
            .issues
            .set_credentials(Credentials::new(Arc::new(Memory::default()), "test"));
        w.core.issues.set_source(
            TrackerKind::Forgejo,
            Arc::new(Forgejo::for_test(
                &server.base,
                "fj-secret456",
                Duration::from_secs(5),
            )),
        );
        w.core
            .connect_issue_tracker(
                TrackerKind::Forgejo,
                connect(Some("https://forgejo.example.test"), Some("fj-secret456")),
            )
            .await
            .unwrap();
        w
    }

    fn key(scope: &str) -> IssueKey {
        IssueKey {
            tracker: TrackerKind::Forgejo,
            scope: scope.into(),
            key: "1".into(),
        }
    }

    #[tokio::test]
    async fn a_scope_outside_the_connection_is_refused_with_no_request_sent() {
        let server = Server::start(|req| {
            if req.path().ends_with("/comments") {
                Reply::json(&json!([]))
            } else if req.path().ends_with("/issues") {
                Reply::json(&json!([row(req, 1)]))
            } else {
                Reply::json(&row(req, 1))
            }
        })
        .await;
        let w = connected(&server).await;
        let outside = w.core.issue(&key("other/proj")).await;
        assert!(matches!(outside, Err(CoreError::Invalid(_))), "{outside:?}");
        let link = w.core.issue_draft(&key("team/other")).await;
        assert!(matches!(link, Err(CoreError::Invalid(_))));
        assert!(server.seen().is_empty(), "nothing was sent");
        let detail = w.core.issue(&key("team/app")).await.unwrap();
        assert_eq!(detail.summary.display_key, "#1");
        assert!(
            server
                .seen()
                .iter()
                .any(|r| r.path() == "/api/v1/repos/team/app/issues/1")
        );
    }

    #[tokio::test]
    async fn a_failing_forgejo_is_reported_with_fixed_copy_and_a_wait_when_rate_limited() {
        let server = Server::start(|_| Reply::status(500)).await;
        let w = connected(&server).await;
        let list = w.core.list_issues(&IssueQuery::default()).await.unwrap();
        assert!(list.issues.is_empty());
        assert_eq!(list.failed.len(), 1);
        assert_eq!(list.failed[0].tracker, TrackerKind::Forgejo);
        assert_eq!(list.failed[0].message, "Forgejo did not answer.");
        let limited = Server::start(|_| Reply::status(429).header("retry-after", "42")).await;
        w.core.issues.set_source(
            TrackerKind::Forgejo,
            Arc::new(Forgejo::for_test(
                &limited.base,
                "fj-secret456",
                Duration::from_secs(5),
            )),
        );
        let list = w.core.list_issues(&IssueQuery::default()).await.unwrap();
        assert!(list.failed[0].message.contains("42 seconds"), "{list:?}");
        let text = format!("{list:?}");
        assert!(!text.contains("secret"), "{text}");
        let one = w.core.issue(&key("team/app")).await;
        assert!(matches!(one, Err(CoreError::TrackerRateLimited(_))));
    }

    #[tokio::test]
    async fn a_forged_forgejo_cursor_is_a_listing_error_not_a_failed_tracker() {
        let server = Server::start(|req| Reply::json(&json!([row(req, 1)]))).await;
        let w = connected(&server).await;
        let before = server.seen().len();
        let forged = w
            .core
            .list_issues(&IssueQuery {
                tracker: Some(TrackerKind::Forgejo),
                cursors: [(
                    "forgejo team/app".to_owned(),
                    "2.0000000000000000".to_owned(),
                )]
                .into(),
                ..IssueQuery::default()
            })
            .await;
        assert!(
            matches!(&forged, Err(CoreError::Invalid(_))),
            "a cursor we did not issue is a 400, not a failed tracker: {forged:?}"
        );
        assert_eq!(server.seen().len(), before, "nothing was sent");
        let unnamed = w
            .core
            .list_issues(&IssueQuery {
                tracker: Some(TrackerKind::Forgejo),
                cursors: [("forgejo team/elsewhere".to_owned(), "2.x".to_owned())].into(),
                ..IssueQuery::default()
            })
            .await
            .unwrap();
        assert!(unnamed.issues.is_empty() && unnamed.failed.is_empty());
        assert_eq!(server.seen().len(), before, "an unnamed scope is not read");
    }

    #[tokio::test]
    async fn forgejo_source_needs_a_connection_and_refuses_a_stored_plain_http_host() {
        let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
        w.core
            .issues
            .set_credentials(Credentials::new(Arc::new(Memory::default()), "test"));
        let missing = w.core.forgejo_source().err().expect("no connection");
        assert!(
            matches!(missing, CoreError::TrackerNotConnected(_)),
            "{missing:?}"
        );
        w.core
            .connect_issue_tracker(
                TrackerKind::Forgejo,
                connect(Some("https://forgejo.example.test"), Some("fj-secret456")),
            )
            .await
            .unwrap();
        w.core
            .store
            .put_tracker(
                &TrackerConnection {
                    tracker: TrackerKind::Forgejo,
                    host: Some("http://forgejo.example.test".into()),
                    scopes: vec!["team/app".into()],
                },
                "2026-11-01T00:00:00Z",
            )
            .unwrap();
        let plain = w.core.forgejo_source().err().expect("a plain http host");
        assert!(matches!(plain, CoreError::Invalid(_)), "{plain:?}");
    }

    #[tokio::test]
    async fn forgejo_source_sends_the_keychain_token_to_the_stored_connection_host() {
        let server = Server::start(|_| Reply::json(&json!([]))).await;
        let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
        w.core
            .issues
            .set_credentials(Credentials::new(Arc::new(Memory::default()), "test"));
        w.core
            .connect_issue_tracker(
                TrackerKind::Forgejo,
                connect(Some("https://forgejo.example.test"), Some("fj-secret456")),
            )
            .await
            .unwrap();
        w.core.issues.set_forgejo_test_base(&server.base);
        let source = w.core.forgejo_source().expect("a Forgejo source");
        source
            .list("team/app", &IssueQuery::default())
            .await
            .unwrap();
        let seen = server.seen();
        assert!(!seen.is_empty(), "the source sent a request");
        assert!(
            seen.iter()
                .all(|r| r.header("authorization") == Some("token fj-secret456")),
            "{seen:?}"
        );
    }
}

mod azure {
    use super::*;
    use crate::issues::Azure;
    use crate::issues::fake_http::{Reply, Seen, Server};
    use base64::Engine as _;
    use serde_json::{Value, json};
    use std::time::Duration;

    fn row(_req: &Seen, id: u64) -> Value {
        json!({ "id": id, "fields": {
            "System.Title": format!("Work item {id}"),
            "System.State": "New",
            "System.Description": "Body",
            "System.CreatedBy": { "uniqueName": "ann@example.test" },
            "System.CreatedDate": "2026-11-01T00:00:00Z",
            "System.ChangedDate": "2026-11-02T00:00:00Z"
        } })
    }

    fn basic(token: &str) -> String {
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!(":{token}"))
        )
    }

    /// An Azure DevOps connection whose reader talks to the fake server. The stored host is https.
    async fn connected(server: &Server) -> super::super::harness::World {
        let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
        w.core
            .issues
            .set_credentials(Credentials::new(Arc::new(Memory::default()), "test"));
        w.core.issues.set_source(
            TrackerKind::AzureDevops,
            Arc::new(Azure::for_test(
                &server.base,
                "azure-secret789",
                Duration::from_secs(5),
            )),
        );
        w.core
            .connect_issue_tracker(
                TrackerKind::AzureDevops,
                ConnectTracker {
                    host: Some("https://dev.azure.com/org".into()),
                    scope: Some("org/proj".into()),
                    token: Some("azure-secret789".into()),
                },
            )
            .await
            .unwrap();
        w
    }

    fn key(scope: &str) -> IssueKey {
        IssueKey {
            tracker: TrackerKind::AzureDevops,
            scope: scope.into(),
            key: "1".into(),
        }
    }

    #[tokio::test]
    async fn a_scope_outside_the_connection_is_refused_with_no_request_sent() {
        let server = Server::start(|req| {
            if req.path().ends_with("/comments") {
                Reply::json(&json!({ "count": 0, "comments": [] }))
            } else if req.path().ends_with("/wiql") {
                Reply::json(&json!({ "workItems": [{ "id": 1 }] }))
            } else if req.path().ends_with("/workitems") {
                Reply::json(&json!({ "count": 1, "value": [row(req, 1)] }))
            } else {
                Reply::json(&row(req, 1))
            }
        })
        .await;
        let w = connected(&server).await;
        let outside = w.core.issue(&key("other/proj")).await;
        assert!(matches!(outside, Err(CoreError::Invalid(_))), "{outside:?}");
        let link = w.core.issue_draft(&key("org/other")).await;
        assert!(matches!(link, Err(CoreError::Invalid(_))));
        assert!(server.seen().is_empty(), "nothing was sent");
        let detail = w.core.issue(&key("org/proj")).await.unwrap();
        assert_eq!(detail.summary.display_key, "#1");
        assert!(
            server
                .seen()
                .iter()
                .any(|r| r.path() == "/org/proj/_apis/wit/workitems/1")
        );
    }

    #[tokio::test]
    async fn a_forged_azure_cursor_is_a_listing_error_not_a_failed_tracker() {
        let server = Server::start(|req| {
            if req.path().ends_with("/wiql") {
                Reply::json(&json!({ "workItems": [{ "id": 1 }] }))
            } else {
                Reply::json(&json!({ "count": 1, "value": [row(req, 1)] }))
            }
        })
        .await;
        let w = connected(&server).await;
        let before = server.seen().len();
        let forged = w
            .core
            .list_issues(&IssueQuery {
                tracker: Some(TrackerKind::AzureDevops),
                cursors: [(
                    "azure_devops org/proj".to_owned(),
                    "50.0000000000000000".to_owned(),
                )]
                .into(),
                ..IssueQuery::default()
            })
            .await;
        assert!(
            matches!(&forged, Err(CoreError::Invalid(_))),
            "a cursor we did not issue is a 400, not a failed tracker: {forged:?}"
        );
        assert_eq!(server.seen().len(), before, "nothing was sent");
        let unnamed = w
            .core
            .list_issues(&IssueQuery {
                tracker: Some(TrackerKind::AzureDevops),
                cursors: [("azure_devops org/elsewhere".to_owned(), "50.x".to_owned())].into(),
                ..IssueQuery::default()
            })
            .await
            .unwrap();
        assert!(unnamed.issues.is_empty() && unnamed.failed.is_empty());
        assert_eq!(server.seen().len(), before, "an unnamed scope is not read");
    }

    #[tokio::test]
    async fn azure_source_needs_a_connection_and_refuses_a_stored_plain_http_host() {
        let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
        w.core
            .issues
            .set_credentials(Credentials::new(Arc::new(Memory::default()), "test"));
        let missing = w.core.azure_source().err().expect("no connection");
        assert!(
            matches!(missing, CoreError::TrackerNotConnected(_)),
            "{missing:?}"
        );
        w.core
            .connect_issue_tracker(
                TrackerKind::AzureDevops,
                ConnectTracker {
                    host: Some("https://dev.azure.com/org".into()),
                    scope: Some("org/proj".into()),
                    token: Some("azure-secret789".into()),
                },
            )
            .await
            .unwrap();
        w.core
            .store
            .put_tracker(
                &TrackerConnection {
                    tracker: TrackerKind::AzureDevops,
                    host: Some("http://dev.azure.com/org".into()),
                    scopes: vec!["org/proj".into()],
                },
                "2026-11-01T00:00:00Z",
            )
            .unwrap();
        let plain = w.core.azure_source().err().expect("a plain http host");
        assert!(matches!(plain, CoreError::Invalid(_)), "{plain:?}");
    }

    #[tokio::test]
    async fn azure_source_sends_the_keychain_token_to_the_stored_connection_host() {
        let server = Server::start(|req| {
            if req.path().ends_with("/wiql") {
                Reply::json(&json!({ "workItems": [{ "id": 1 }] }))
            } else {
                Reply::json(&json!({ "count": 1, "value": [row(req, 1)] }))
            }
        })
        .await;
        let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
        w.core
            .issues
            .set_credentials(Credentials::new(Arc::new(Memory::default()), "test"));
        w.core
            .connect_issue_tracker(
                TrackerKind::AzureDevops,
                ConnectTracker {
                    host: Some("https://dev.azure.com/org".into()),
                    scope: Some("org/proj".into()),
                    token: Some("azure-secret789".into()),
                },
            )
            .await
            .unwrap();
        w.core.issues.set_azure_test_base(&server.base);
        let source = w.core.azure_source().expect("an Azure DevOps source");
        source
            .list("org/proj", &IssueQuery::default())
            .await
            .unwrap();
        let seen = server.seen();
        assert!(!seen.is_empty(), "the source sent a request");
        assert!(
            seen.iter()
                .all(|r| r.header("authorization") == Some(basic("azure-secret789").as_str())),
            "{seen:?}"
        );
    }

    #[tokio::test]
    async fn a_failing_azure_is_reported_with_fixed_copy_and_a_wait_when_rate_limited() {
        let server = Server::start(|_| Reply::status(500)).await;
        let w = connected(&server).await;
        let list = w.core.list_issues(&IssueQuery::default()).await.unwrap();
        assert!(list.issues.is_empty());
        assert_eq!(list.failed.len(), 1);
        assert_eq!(list.failed[0].tracker, TrackerKind::AzureDevops);
        assert_eq!(list.failed[0].message, "Azure DevOps did not answer.");
        let limited = Server::start(|_| Reply::status(429).header("retry-after", "42")).await;
        w.core.issues.set_source(
            TrackerKind::AzureDevops,
            Arc::new(Azure::for_test(
                &limited.base,
                "azure-secret789",
                Duration::from_secs(5),
            )),
        );
        let list = w.core.list_issues(&IssueQuery::default()).await.unwrap();
        assert!(list.failed[0].message.contains("42 seconds"), "{list:?}");
        let text = format!("{list:?}");
        assert!(!text.contains("secret"), "{text}");
    }

    #[tokio::test]
    async fn a_bad_azure_host_or_scope_is_refused_at_connect_and_saves_nothing() {
        let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
        w.core
            .issues
            .set_credentials(Credentials::new(Arc::new(Memory::default()), "test"));
        for (host, scope) in [
            ("https://dev.azure.com.evil.com", "org/proj"),
            ("https://dev.azure.com/org/proj", "org/proj"),
            ("https://dev.azure.com/org", "org/.."),
            ("https://dev.azure.com/org", "org/proj/extra"),
        ] {
            let error = w
                .core
                .connect_issue_tracker(
                    TrackerKind::AzureDevops,
                    ConnectTracker {
                        host: Some(host.into()),
                        scope: Some(scope.into()),
                        token: Some("azure-secret789".into()),
                    },
                )
                .await;
            assert!(
                matches!(error, Err(CoreError::Invalid(_))),
                "{host} {scope}: {error:?}"
            );
        }
        assert!(w.core.store.tracker_connections().unwrap().is_empty());
        w.core
            .connect_issue_tracker(
                TrackerKind::AzureDevops,
                ConnectTracker {
                    host: Some("https://dev.azure.com/org".into()),
                    scope: Some("org/proj".into()),
                    token: Some("azure-secret789".into()),
                },
            )
            .await
            .unwrap();
        assert_eq!(w.core.store.tracker_connections().unwrap().len(), 1);
    }
}

mod linear {
    use super::*;
    use crate::issues::fake_http::{Reply, Seen, Server};
    use serde_json::{Value, json};

    const TOKEN: &str = "lin-secret789";

    fn row(identifier: &str, updated: &str) -> Value {
        json!({
            "identifier": identifier,
            "title": format!("Issue {identifier}"),
            "description": "Body",
            "url": format!("https://linear.app/orbyn/issue/{identifier}"),
            "createdAt": "2026-11-01T00:00:00Z",
            "updatedAt": updated,
            "completedAt": null,
            "canceledAt": null,
            "state": { "name": "In Progress", "type": "started" },
            "creator": { "name": "ann", "displayName": "Ann" },
            "assignee": { "name": "bo", "displayName": "Bo" },
            "labels": { "nodes": [{ "name": "bug" }] }
        })
    }

    fn query_of(req: &Seen) -> String {
        req.json()
            .and_then(|body| body.get("query").and_then(Value::as_str).map(str::to_owned))
            .unwrap_or_default()
    }

    fn team_of(req: &Seen) -> String {
        req.json()
            .and_then(|body| {
                body.get("variables")?
                    .get("team")?
                    .as_str()
                    .map(str::to_owned)
            })
            .unwrap_or_default()
    }

    /// Team keys for a connection with no scope, and one issue in each team.
    fn answer(req: &Seen) -> Reply {
        let query = query_of(req);
        if query.contains("OrbynTeams") {
            return Reply::json(&json!({
                "data": { "teams": { "nodes": [{ "key": "ENG" }, { "key": "OPS" }] } }
            }));
        }
        let team = team_of(req);
        let (identifier, updated) = if team == "OPS" {
            ("OPS-2", "2026-11-03T00:00:00Z")
        } else {
            ("ENG-1", "2026-11-02T00:00:00Z")
        };
        if query.contains("OrbynIssue(") {
            let mut issue = row(identifier, updated);
            issue["description"] = json!("It breaks.");
            issue["comments"] = json!({ "nodes": [] });
            return Reply::json(&json!({ "data": { "issue": issue } }));
        }
        Reply::json(&json!({
            "data": {
                "issues": {
                    "nodes": [row(identifier, updated)],
                    "pageInfo": { "hasNextPage": false, "endCursor": null }
                }
            }
        }))
    }

    fn connect_linear(scope: Option<&str>) -> ConnectTracker {
        ConnectTracker {
            host: None,
            scope: scope.map(str::to_owned),
            token: Some(TOKEN.to_owned()),
        }
    }

    async fn connected(server: &Server, scope: Option<&str>) -> super::super::harness::World {
        let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
        w.core
            .issues
            .set_credentials(Credentials::new(Arc::new(Memory::default()), "test"));
        w.core
            .connect_issue_tracker(TrackerKind::Linear, connect_linear(scope))
            .await
            .unwrap();
        w.core.issues.set_linear_test_base(&server.base);
        w
    }

    fn key(scope: &str, value: &str) -> IssueKey {
        IssueKey {
            tracker: TrackerKind::Linear,
            scope: scope.into(),
            key: value.into(),
        }
    }

    #[tokio::test]
    async fn connect_refuses_a_scope_that_is_not_a_team_key_before_saving_anything() {
        let memory = Arc::new(Memory::default());
        let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
        w.core
            .issues
            .set_credentials(Credentials::new(memory.clone(), "test"));
        for scope in ["eng", "ENG-1", "ENG 1", "ABCDEFGHIJK", "ÉNG"] {
            let refused = w
                .core
                .connect_issue_tracker(TrackerKind::Linear, connect_linear(Some(scope)))
                .await;
            assert!(
                matches!(refused, Err(CoreError::Invalid(_))),
                "{scope}: {refused:?}"
            );
        }
        assert!(w.core.store.tracker_connections().unwrap().is_empty());
        assert!(memory.0.lock().unwrap().is_empty(), "no token was saved");
        let saved = w
            .core
            .connect_issue_tracker(TrackerKind::Linear, connect_linear(Some("ENG")))
            .await
            .unwrap();
        assert_eq!(saved.scope.as_deref(), Some("ENG"));
    }

    #[tokio::test]
    async fn a_connection_with_no_scope_reads_teams_once_in_the_cache_window() {
        let server = Server::start(answer).await;
        let w = connected(&server, None).await;
        let teams = || {
            server
                .seen()
                .iter()
                .filter(|request| query_of(request).contains("OrbynTeams"))
                .count()
        };
        w.core.issue(&key("ENG", "ENG-1")).await.unwrap();
        assert_eq!(teams(), 1);
        w.core.issue(&key("ENG", "ENG-1")).await.unwrap();
        assert_eq!(teams(), 1, "one detail makes at most one teams read in 30s");
        let found = w.core.find_issues("1").await.unwrap();
        assert!(found.is_empty());
        assert_eq!(teams(), 1, "a bare-number find shares the cached teams");
        w.core
            .connect_issue_tracker(TrackerKind::Linear, connect_linear(None))
            .await
            .unwrap();
        w.core.issue(&key("ENG", "ENG-1")).await.unwrap();
        assert_eq!(teams(), 2, "connect clears the cached teams");
    }

    #[tokio::test]
    async fn a_scope_outside_the_connection_is_refused_with_no_request_sent() {
        let server = Server::start(answer).await;
        let w = connected(&server, Some("ENG")).await;
        for scope in ["OPS", "eng", "ENG-1"] {
            let outside = w.core.issue(&key(scope, "ENG-1")).await;
            assert!(matches!(outside, Err(CoreError::Invalid(_))), "{scope}");
            let draft = w.core.issue_draft(&key(scope, "ENG-1")).await;
            assert!(
                matches!(draft, Err(CoreError::Invalid(_))),
                "a draft is refused the same way: {scope}"
            );
        }
        assert!(server.seen().is_empty(), "nothing was sent");
        let detail = w.core.issue(&key("ENG", "ENG-1")).await.unwrap();
        assert_eq!(detail.summary.display_key, "ENG-1");
        assert_eq!(detail.body, "It breaks.");
        assert!(
            server
                .seen()
                .iter()
                .any(|r| query_of(r).contains("OrbynIssue("))
        );
    }

    #[tokio::test]
    async fn a_forged_linear_cursor_is_a_listing_error_not_a_failed_tracker() {
        let server = Server::start(answer).await;
        let w = connected(&server, Some("ENG")).await;
        let before = server.seen().len();
        let forged = w
            .core
            .list_issues(&IssueQuery {
                tracker: Some(TrackerKind::Linear),
                cursors: [("linear ENG".to_owned(), "0000000000000000.end-1".to_owned())].into(),
                ..IssueQuery::default()
            })
            .await;
        assert!(
            matches!(&forged, Err(CoreError::Invalid(_))),
            "a cursor we did not issue is a 400, not a failed tracker: {forged:?}"
        );
        assert_eq!(server.seen().len(), before, "nothing was sent");
        let unnamed = w
            .core
            .list_issues(&IssueQuery {
                tracker: Some(TrackerKind::Linear),
                cursors: [("linear OTHER".to_owned(), "0000000000000000.x".to_owned())].into(),
                ..IssueQuery::default()
            })
            .await
            .unwrap();
        assert!(unnamed.issues.is_empty() && unnamed.failed.is_empty());
        assert_eq!(server.seen().len(), before, "an unnamed scope is not read");
    }

    #[tokio::test]
    async fn a_failing_linear_is_reported_with_fixed_copy_and_a_wait_when_rate_limited() {
        let server = Server::start(|_| Reply::status(500)).await;
        let w = connected(&server, Some("ENG")).await;
        let list = w.core.list_issues(&IssueQuery::default()).await.unwrap();
        assert!(list.issues.is_empty());
        assert_eq!(list.failed.len(), 1);
        assert_eq!(list.failed[0].tracker, TrackerKind::Linear);
        assert_eq!(list.failed[0].message, "Linear did not answer.");
        let text = format!("{list:?}");
        assert!(!text.contains("secret"), "{text}");

        let limited = Server::start(|_| Reply::status(429).header("retry-after", "42")).await;
        w.core.issues.set_linear_test_base(&limited.base);
        let list = w.core.list_issues(&IssueQuery::default()).await.unwrap();
        assert_eq!(list.failed.len(), 1);
        assert!(list.failed[0].message.contains("42 seconds"), "{list:?}");
        let one = w.core.issue(&key("ENG", "ENG-1")).await;
        assert!(matches!(one, Err(CoreError::TrackerRateLimited(_))));
    }

    #[tokio::test]
    async fn linear_source_needs_a_connection_and_refuses_a_stored_plain_http_host() {
        let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
        w.core
            .issues
            .set_credentials(Credentials::new(Arc::new(Memory::default()), "test"));
        let missing = w.core.linear_source().err().expect("no connection");
        assert!(
            matches!(missing, CoreError::TrackerNotConnected(_)),
            "{missing:?}"
        );
        w.core
            .connect_issue_tracker(TrackerKind::Linear, connect_linear(Some("ENG")))
            .await
            .unwrap();
        w.core
            .store
            .put_tracker(
                &TrackerConnection {
                    tracker: TrackerKind::Linear,
                    host: Some("http://api.linear.app".into()),
                    scopes: vec!["ENG".into()],
                },
                "2026-11-01T00:00:00Z",
            )
            .unwrap();
        let plain = w.core.linear_source().err().expect("a plain http host");
        assert!(matches!(plain, CoreError::Invalid(_)), "{plain:?}");
    }

    #[tokio::test]
    async fn linear_source_sends_the_keychain_token_to_the_request() {
        let server = Server::start(|req| {
            assert!(query_of(req).contains("OrbynIssues("), "{:?}", req.body);
            Reply::json(&json!({
                "data": { "issues": {
                    "nodes": [row("ENG-1", "2026-11-02T00:00:00Z")],
                    "pageInfo": { "hasNextPage": false, "endCursor": null }
                } }
            }))
        })
        .await;
        let w = connected(&server, Some("ENG")).await;
        let source = w.core.linear_source().expect("a Linear source");
        source.list("ENG", &IssueQuery::default()).await.unwrap();
        let seen = server.seen();
        assert!(!seen.is_empty(), "the source sent a request");
        assert!(
            seen.iter()
                .all(|r| r.header("authorization") == Some(TOKEN)),
            "{seen:?}"
        );
        assert!(
            seen.iter().all(|r| !r
                .header("authorization")
                .unwrap_or_default()
                .starts_with("Bearer ")),
            "Linear personal keys take no Bearer prefix"
        );
    }

    #[tokio::test]
    async fn a_connection_with_no_scope_lists_each_team_its_key_can_see() {
        let server = Server::start(answer).await;
        let w = connected(&server, None).await;
        let list = w.core.list_issues(&IssueQuery::default()).await.unwrap();
        let keys: Vec<&str> = list.issues.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, ["OPS-2", "ENG-1"], "newest update first across teams");
        assert_eq!(list.counts, None, "Linear's totals are Planned");
        assert!(list.failed.is_empty());
        assert!(
            server
                .seen()
                .iter()
                .any(|r| query_of(r).contains("OrbynTeams")),
            "the teams fallback ran"
        );
        let mut teams: Vec<String> = server
            .seen()
            .iter()
            .filter(|r| query_of(r).contains("OrbynIssues("))
            .map(team_of)
            .collect();
        teams.sort();
        assert_eq!(teams, ["ENG", "OPS"]);

        let server = Server::start(answer).await;
        let w = connected(&server, Some("ENG")).await;
        w.core.list_issues(&IssueQuery::default()).await.unwrap();
        assert!(
            !server
                .seen()
                .iter()
                .any(|r| query_of(r).contains("OrbynTeams")),
            "a stored team key needs no teams read"
        );
    }
}

mod writes {
    //! The issue write routes' owning rules through a real core: which tracker's own write is built, what a write
    //! validates before anything is sent, and what it refuses (docs/issues.md, "Create, comment, react"). The
    //! trackers' wire calls are tested against each tracker's own fake; here the source counts the writes it receives,
    //! so a refusal can be shown to have sent nothing.

    use super::*;
    use crate::issues::{IssueEdit, NewIssue, display_name};
    use orbyn_contracts::{
        CommentIssue, CreateIssue, IssueAssigneeCandidate, IssueCandidateLabel, IssueCandidates,
        IssueCloseReason, IssueComment, IssueReaction, IssueSetStateInput, IssueState,
        IssueStateCategory, IssueUpdateInput, ReactIssue,
    };

    /// A source whose write may or may not be built. Every write it receives is counted; a read answers nothing.
    struct Writes {
        can_write: bool,
        creates: AtomicUsize,
        comments: AtomicUsize,
        reactions: AtomicUsize,
        edits: AtomicUsize,
        /// The states `get` declares; absent means the issue is not found.
        states: Option<Vec<IssueState>>,
        templates: Vec<IssueTemplate>,
        /// The labels each create sent, in order: a template's labels must never appear here unrequested.
        sent_labels: Mutex<Vec<Vec<String>>>,
    }

    impl Writes {
        fn new(can_write: bool) -> Arc<Self> {
            Self::with_templates(can_write, Vec::new())
        }

        fn with_templates(can_write: bool, templates: Vec<IssueTemplate>) -> Arc<Self> {
            Arc::new(Self {
                can_write,
                creates: AtomicUsize::new(0),
                comments: AtomicUsize::new(0),
                reactions: AtomicUsize::new(0),
                edits: AtomicUsize::new(0),
                states: None,
                templates,
                sent_labels: Mutex::new(Vec::new()),
            })
        }

        /// A source whose edits are built, whose issue `7` declares an open and a closed state.
        fn editing() -> Arc<Self> {
            let mut source = Self {
                can_write: true,
                creates: AtomicUsize::new(0),
                comments: AtomicUsize::new(0),
                reactions: AtomicUsize::new(0),
                edits: AtomicUsize::new(0),
                states: None,
                templates: Vec::new(),
                sent_labels: Mutex::new(Vec::new()),
            };
            source.states = Some(vec![
                IssueState {
                    category: IssueStateCategory::Open,
                    name: "Open".into(),
                    close_reason: None,
                },
                IssueState {
                    category: IssueStateCategory::Closed,
                    name: "Closed".into(),
                    close_reason: Some(IssueCloseReason::Completed),
                },
            ]);
            Arc::new(source)
        }

        fn writes(&self) -> usize {
            self.creates.load(Ordering::SeqCst)
                + self.comments.load(Ordering::SeqCst)
                + self.reactions.load(Ordering::SeqCst)
                + self.edits.load(Ordering::SeqCst)
        }

        fn detail(&self, scope: &str, key: &str) -> IssueDetail {
            let mut summary = issue(TrackerKind::Github, key, "2026-11-24T00:00:00Z");
            summary.scope = scope.to_owned();
            IssueDetail {
                summary,
                body: String::new(),
                comments: vec![],
                linked_pull_requests: vec![],
                linked_threads: vec![],
                reactions: None,
                states: self.states.clone(),
            }
        }
    }

    #[async_trait]
    impl IssueSource for Writes {
        async fn list(&self, _scope: &str, _query: &IssueQuery) -> Result<ScopeList> {
            Ok(ScopeList {
                issues: vec![],
                counts: None,
                next: None,
            })
        }

        async fn get(&self, scope: &str, key: &str) -> Result<IssueDetail> {
            if self.states.is_some() {
                return Ok(self.detail(scope, key));
            }
            Err(CoreError::NotFound(format!("issue {key}")))
        }

        fn can_write(&self) -> bool {
            self.can_write
        }

        fn can_edit(&self) -> bool {
            self.states.is_some()
        }

        async fn update(
            &self,
            scope: &str,
            key: &str,
            _edit: &IssueEdit<'_>,
        ) -> Result<IssueDetail> {
            self.edits.fetch_add(1, Ordering::SeqCst);
            Ok(self.detail(scope, key))
        }

        async fn set_state(
            &self,
            scope: &str,
            key: &str,
            _state: &str,
            _close_reason: Option<IssueCloseReason>,
        ) -> Result<IssueDetail> {
            self.edits.fetch_add(1, Ordering::SeqCst);
            Ok(self.detail(scope, key))
        }

        /// More labels and people than the 200 the route keeps, with no `truncated` of its own.
        async fn candidates(&self, _scope: &str) -> Result<IssueCandidates> {
            Ok(IssueCandidates {
                labels: (0..201)
                    .map(|n| IssueCandidateLabel {
                        name: format!("label-{n}"),
                        color: None,
                    })
                    .collect(),
                assignees: (0..201)
                    .map(|n| IssueAssigneeCandidate {
                        login: format!("user-{n}"),
                        name: None,
                        id: format!("user-{n}"),
                    })
                    .collect(),
                truncated: false,
            })
        }

        async fn create(&self, scope: &str, new: &NewIssue<'_>) -> Result<IssueDetail> {
            self.creates.fetch_add(1, Ordering::SeqCst);
            self.sent_labels
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(new.labels.to_vec());
            let mut summary = issue(TrackerKind::Github, "7", "2026-11-24T00:00:00Z");
            summary.scope = scope.to_owned();
            summary.title = new.title.to_owned();
            Ok(IssueDetail {
                summary,
                body: new.body.to_owned(),
                comments: vec![],
                linked_pull_requests: vec![],
                linked_threads: vec![],
                reactions: None,
                states: None,
            })
        }

        async fn comment(&self, scope: &str, key: &str, body: &str) -> Result<IssueComment> {
            self.comments.fetch_add(1, Ordering::SeqCst);
            Ok(IssueComment {
                id: format!("{scope} {key}"),
                author: None,
                body: body.to_owned(),
                created_at: "2026-11-24T00:00:00Z".into(),
            })
        }

        async fn templates(&self, _scope: &str) -> Result<Vec<IssueTemplate>> {
            Ok(self.templates.clone())
        }

        async fn react(
            &self,
            scope: &str,
            key: &str,
            _comment: Option<&str>,
            reaction: IssueReaction,
            on: bool,
        ) -> Result<IssueDetail> {
            self.reactions.fetch_add(1, Ordering::SeqCst);
            let mut summary = issue(TrackerKind::Github, key, "2026-11-24T00:00:00Z");
            summary.scope = scope.to_owned();
            Ok(IssueDetail {
                summary,
                body: String::new(),
                comments: vec![],
                linked_pull_requests: vec![],
                linked_threads: vec![],
                reactions: Some(vec![orbyn_contracts::IssueReactionCount {
                    content: format!("{reaction:?}"),
                    count: u32::from(on),
                }]),
                states: None,
            })
        }
    }

    /// A world with one connected tracker and a source that counts writes. The host, scope and token are the ones
    /// each tracker's `connect` accepts (docs/issues.md, "Trackers and credentials").
    async fn connected(
        kind: TrackerKind,
        scope: &str,
        can_write: bool,
    ) -> (super::super::harness::World, Arc<Writes>) {
        let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
        w.core
            .issues
            .set_credentials(Credentials::new(Arc::new(Memory::default()), "test"));
        let (host, token) = match kind {
            TrackerKind::Gitlab => (Some("https://gitlab.example.test"), Some("glpat-test")),
            TrackerKind::Forgejo => (Some("https://forgejo.example.test"), Some("fj-test")),
            TrackerKind::AzureDevops => (Some("https://dev.azure.com/acme"), Some("az-test")),
            TrackerKind::Linear => (None, Some("lin-test")),
            TrackerKind::Github => (None, None),
        };
        w.core
            .connect_issue_tracker(
                kind,
                ConnectTracker {
                    host: host.map(str::to_owned),
                    scope: Some(scope.to_owned()),
                    token: token.map(str::to_owned),
                },
            )
            .await
            .unwrap();
        let source = Writes::new(can_write);
        w.core.issues.set_source(kind, source.clone());
        (w, source)
    }

    fn create(kind: TrackerKind, scope: &str) -> CreateIssue {
        CreateIssue {
            tracker: kind,
            scope: scope.to_owned(),
            title: "Crash on start".into(),
            body: Some("It breaks.".into()),
            labels: vec!["bug".into()],
            assignees: vec!["12".into()],
            template: None,
        }
    }

    fn comment(kind: TrackerKind, scope: &str) -> CommentIssue {
        CommentIssue {
            tracker: kind,
            scope: scope.to_owned(),
            key: "7".into(),
            body: "Looks good.".into(),
        }
    }

    fn react(kind: TrackerKind, scope: &str) -> ReactIssue {
        ReactIssue {
            tracker: kind,
            scope: scope.to_owned(),
            key: "7".into(),
            comment_id: None,
            reaction: IssueReaction::Heart,
            on: true,
        }
    }

    #[tokio::test]
    async fn a_tracker_without_its_own_write_answers_422_unsupported_and_sends_nothing() {
        for (kind, scope) in [
            (TrackerKind::Gitlab, "team/app"),
            (TrackerKind::Forgejo, "team/app"),
            (TrackerKind::AzureDevops, "acme/app"),
            (TrackerKind::Linear, "ENG"),
        ] {
            let (w, source) = connected(kind, scope, false).await;
            let wanted = format!(
                "{} issue writes arrive in a later update.",
                display_name(kind)
            );
            let create = w.core.create_issue(create(kind, scope)).await.map(|_| ());
            let comment = w.core.comment_issue(comment(kind, scope)).await.map(|_| ());
            let react = w.core.react_issue(react(kind, scope)).await.map(|_| ());
            for (what, refused) in [("create", create), ("comment", comment), ("react", react)] {
                let Err(error) = refused else {
                    panic!("{kind:?} {what} answered a success");
                };
                assert_eq!(error.status(), 422, "{kind:?} {what}: {error:?}");
                assert_eq!(error.code(), "unsupported", "{kind:?} {what}");
                assert_eq!(error.public_message(), wanted, "{kind:?} {what}");
            }
            assert_eq!(source.writes(), 0, "{kind:?}: nothing reached the tracker");
        }
    }

    #[tokio::test]
    async fn a_create_checks_every_field_and_writes_one_issue() {
        let (w, source) = connected(TrackerKind::Gitlab, "team/app", true).await;
        let valid = create(TrackerKind::Gitlab, "team/app");
        let detail = w.core.create_issue(valid.clone()).await.unwrap();
        assert_eq!(
            (detail.summary.title.as_str(), detail.summary.scope.as_str()),
            ("Crash on start", "team/app")
        );
        assert_eq!(source.writes(), 1);
        for (what, input) in [
            (
                "an empty title",
                CreateIssue {
                    title: "   ".into(),
                    ..valid.clone()
                },
            ),
            (
                "a title with a line break",
                CreateIssue {
                    title: "one\ntwo".into(),
                    ..valid.clone()
                },
            ),
            (
                "a title over 256 characters",
                CreateIssue {
                    title: "x".repeat(257),
                    ..valid.clone()
                },
            ),
            (
                "a body over 64 KiB",
                CreateIssue {
                    body: Some("x".repeat(64 * 1024 + 1)),
                    ..valid.clone()
                },
            ),
            (
                "more than 50 labels",
                CreateIssue {
                    labels: (0..51).map(|n| format!("label-{n}")).collect(),
                    ..valid.clone()
                },
            ),
            (
                "an empty label name",
                CreateIssue {
                    labels: vec![String::new()],
                    ..valid.clone()
                },
            ),
            (
                "more than 25 assignees",
                CreateIssue {
                    assignees: (0..26).map(|n| n.to_string()).collect(),
                    ..valid.clone()
                },
            ),
            (
                "a template id",
                CreateIssue {
                    template: Some("bug".into()),
                    ..valid.clone()
                },
            ),
        ] {
            let refused = w.core.create_issue(input).await.map(|_| ());
            assert!(
                matches!(refused, Err(CoreError::Invalid(_))),
                "{what}: {refused:?}"
            );
            assert_eq!(source.writes(), 1, "{what}: nothing was written");
        }
        let template = CreateIssue {
            template: Some("bug".into()),
            ..valid.clone()
        };
        let error = w.core.create_issue(template).await.unwrap_err();
        assert_eq!(error.public_message(), "Unknown issue template.");
    }

    #[tokio::test]
    async fn a_comment_is_one_to_64_kib_and_not_blank() {
        let (w, source) = connected(TrackerKind::Gitlab, "team/app", true).await;
        let stored = w
            .core
            .comment_issue(comment(TrackerKind::Gitlab, "team/app"))
            .await
            .unwrap();
        assert_eq!(stored.body, "Looks good.");
        assert_eq!(source.writes(), 1);
        for (what, body) in [
            ("a blank comment", " \n\t ".to_owned()),
            ("a comment over 64 KiB", "x".repeat(64 * 1024 + 1)),
        ] {
            let refused = w
                .core
                .comment_issue(CommentIssue {
                    body,
                    ..comment(TrackerKind::Gitlab, "team/app")
                })
                .await
                .map(|_| ());
            assert!(
                matches!(refused, Err(CoreError::Invalid(_))),
                "{what}: {refused:?}"
            );
            assert_eq!(source.writes(), 1, "{what}: nothing was written");
        }
    }

    #[tokio::test]
    async fn a_reaction_takes_a_plain_comment_id_and_answers_the_issue_with_its_counts() {
        let (w, source) = connected(TrackerKind::Gitlab, "team/app", true).await;
        let detail = w
            .core
            .react_issue(react(TrackerKind::Gitlab, "team/app"))
            .await
            .unwrap();
        assert_eq!(detail.reactions.unwrap()[0].count, 1);
        assert_eq!(source.writes(), 1);
        for comment_id in ["bad\nid".to_owned(), String::new()] {
            let refused = w
                .core
                .react_issue(ReactIssue {
                    comment_id: Some(comment_id),
                    ..react(TrackerKind::Gitlab, "team/app")
                })
                .await
                .map(|_| ());
            assert!(matches!(refused, Err(CoreError::Invalid(_))), "{refused:?}");
            assert_eq!(source.writes(), 1, "a refused id wrote nothing");
        }
    }

    #[tokio::test]
    async fn a_scope_outside_the_connection_is_refused_and_nothing_is_written() {
        let (w, source) = connected(TrackerKind::Gitlab, "team/app", true).await;
        for (what, refused) in [
            (
                "create",
                w.core
                    .create_issue(create(TrackerKind::Gitlab, "other/repo"))
                    .await
                    .map(|_| ()),
            ),
            (
                "comment",
                w.core
                    .comment_issue(comment(TrackerKind::Gitlab, "other/repo"))
                    .await
                    .map(|_| ()),
            ),
            (
                "react",
                w.core
                    .react_issue(react(TrackerKind::Gitlab, "other/repo"))
                    .await
                    .map(|_| ()),
            ),
        ] {
            assert!(
                matches!(refused, Err(CoreError::Invalid(_))),
                "{what}: {refused:?}"
            );
        }
        assert_eq!(
            source.writes(),
            0,
            "nothing was written outside the connection"
        );
    }

    fn template(id: &str, body: &str) -> IssueTemplate {
        IssueTemplate {
            id: id.into(),
            name: format!("Template {id}"),
            description: None,
            body: body.into(),
            labels: vec![],
        }
    }

    fn labeled_template(id: &str, body: &str, labels: &[&str]) -> IssueTemplate {
        IssueTemplate {
            id: id.into(),
            name: format!("Template {id}"),
            description: None,
            body: body.into(),
            labels: labels.iter().map(|label| (*label).to_owned()).collect(),
        }
    }

    #[tokio::test]
    async fn a_template_fills_a_missing_body_and_a_given_body_wins() {
        let (w, _source) = connected(TrackerKind::Gitlab, "team/app", true).await;
        let source = Writes::with_templates(
            true,
            vec![labeled_template("bug", "Steps: one.", &["template-label"])],
        );
        w.core
            .issues
            .set_source(TrackerKind::Gitlab, source.clone());
        let filled = w
            .core
            .create_issue(CreateIssue {
                template: Some("bug".into()),
                body: None,
                ..create(TrackerKind::Gitlab, "team/app")
            })
            .await
            .unwrap();
        assert_eq!(
            filled.body, "Steps: one.",
            "a missing body is the template's"
        );
        let given = w
            .core
            .create_issue(CreateIssue {
                template: Some("bug".into()),
                body: Some("Mine.".into()),
                ..create(TrackerKind::Gitlab, "team/app")
            })
            .await
            .unwrap();
        assert_eq!(
            given.body, "Mine.",
            "a body the person sent wins over the template"
        );
        assert_eq!(source.writes(), 2);
        assert_eq!(
            source.sent_labels.lock().unwrap().clone(),
            vec![vec!["bug".to_owned()], vec!["bug".to_owned()]],
            "a template fills only a missing body: its labels are never merged into the create's"
        );
    }

    #[tokio::test]
    async fn an_unknown_template_id_is_invalid_and_creates_nothing() {
        let (w, _source) = connected(TrackerKind::Gitlab, "team/app", true).await;
        let source = Writes::with_templates(true, vec![template("bug", "Body")]);
        w.core
            .issues
            .set_source(TrackerKind::Gitlab, source.clone());
        let refused = w
            .core
            .create_issue(CreateIssue {
                template: Some("missing".into()),
                ..create(TrackerKind::Gitlab, "team/app")
            })
            .await
            .map(|_| ());
        assert!(
            matches!(&refused, Err(CoreError::Invalid(message)) if message == "Unknown issue template."),
            "{refused:?}"
        );
        assert_eq!(source.writes(), 0, "an unknown template creates nothing");
        // A tracker with no templates is the same: every id is unknown.
        let source = Writes::new(true);
        w.core
            .issues
            .set_source(TrackerKind::Gitlab, source.clone());
        let refused = w
            .core
            .create_issue(CreateIssue {
                template: Some("bug".into()),
                ..create(TrackerKind::Gitlab, "team/app")
            })
            .await
            .map(|_| ());
        assert!(matches!(refused, Err(CoreError::Invalid(_))), "{refused:?}");
        assert_eq!(source.writes(), 0);
    }

    /// A world whose connected tracker's source builds its edits and declares an open and a closed state. GitLab stands in for
    /// the one built tracker: connecting GitHub needs a real `gh`, and the core's rules do not depend on which tracker edits.
    async fn editing() -> (super::super::harness::World, Arc<Writes>) {
        let (w, _) = connected(TrackerKind::Gitlab, "team/app", true).await;
        let source = Writes::editing();
        w.core
            .issues
            .set_source(TrackerKind::Gitlab, source.clone());
        (w, source)
    }

    fn update(kind: TrackerKind, scope: &str) -> IssueUpdateInput {
        IssueUpdateInput {
            tracker: kind,
            scope: scope.to_owned(),
            key: "7".into(),
            title: Some("New title".into()),
            body: None,
            labels: None,
            assignees: None,
        }
    }

    #[tokio::test]
    async fn an_update_with_nothing_to_change_is_refused_and_sends_nothing() {
        let (w, source) = editing().await;
        let mut empty = update(TrackerKind::Gitlab, "team/app");
        empty.title = None;
        let error = w.core.update_issue(empty).await.map(|_| ()).unwrap_err();
        assert_eq!((error.status(), error.code()), (400, "invalid_input"));
        assert_eq!(source.writes(), 0);
        // One present field is written.
        w.core
            .update_issue(update(TrackerKind::Gitlab, "team/app"))
            .await
            .unwrap();
        assert_eq!(source.writes(), 1);
    }

    #[tokio::test]
    async fn a_tracker_without_edits_answers_422_unsupported_for_all_three_edit_calls() {
        for (kind, scope) in [
            (TrackerKind::Gitlab, "team/app"),
            (TrackerKind::Forgejo, "team/app"),
            (TrackerKind::AzureDevops, "acme/app"),
            (TrackerKind::Linear, "ENG"),
        ] {
            let (w, source) = connected(kind, scope, true).await;
            let wanted = format!(
                "{} issue edits arrive in a later update.",
                display_name(kind)
            );
            let update = w.core.update_issue(update(kind, scope)).await.map(|_| ());
            let state = w
                .core
                .set_state_issue(IssueSetStateInput {
                    tracker: kind,
                    scope: scope.to_owned(),
                    key: "7".into(),
                    state: "Open".into(),
                    close_reason: None,
                })
                .await
                .map(|_| ());
            let candidates = w.core.issue_candidates(kind, scope).await.map(|_| ());
            for (what, refused) in [
                ("update", update),
                ("state", state),
                ("candidates", candidates),
            ] {
                let Err(error) = refused else {
                    panic!("{kind:?} {what} answered a success");
                };
                assert_eq!(error.status(), 422, "{kind:?} {what}: {error:?}");
                assert_eq!(error.code(), "unsupported", "{kind:?} {what}");
                assert_eq!(error.public_message(), wanted, "{kind:?} {what}");
            }
            assert_eq!(source.writes(), 0, "{kind:?}: nothing reached the tracker");
        }
    }

    #[tokio::test]
    async fn a_close_reason_on_an_open_state_or_an_unknown_state_is_refused_before_a_write() {
        let (w, source) = editing().await;
        let state = |name: &str, reason: Option<IssueCloseReason>| IssueSetStateInput {
            tracker: TrackerKind::Gitlab,
            scope: "team/app".into(),
            key: "7".into(),
            state: name.to_owned(),
            close_reason: reason,
        };
        for (what, input) in [
            (
                "a reason on an open state",
                state("Open", Some(IssueCloseReason::NotPlanned)),
            ),
            ("a state the issue does not declare", state("Wontfix", None)),
        ] {
            let error = w.core.set_state_issue(input).await.map(|_| ()).unwrap_err();
            assert_eq!(
                (error.status(), error.code()),
                (400, "invalid_input"),
                "{what}"
            );
        }
        assert_eq!(source.writes(), 0);
        // A closed state takes a reason, and an open one none.
        w.core
            .set_state_issue(state("Closed", Some(IssueCloseReason::NotPlanned)))
            .await
            .unwrap();
        w.core.set_state_issue(state("Open", None)).await.unwrap();
        assert_eq!(source.writes(), 2);
    }

    #[tokio::test]
    async fn candidates_are_cut_to_200_of_each_and_say_so() {
        let (w, _source) = editing().await;
        let candidates = w
            .core
            .issue_candidates(TrackerKind::Gitlab, "team/app")
            .await
            .unwrap();
        assert_eq!(candidates.labels.len(), 200);
        assert_eq!(candidates.assignees.len(), 200);
        assert!(
            candidates.truncated,
            "a source that sent more than 200 is marked"
        );
    }
}

mod binding {
    //! The project-to-tracker binding through a real core: two names stored, one audit row with ids and names only,
    //! an idempotent unbind, and the connection and scope checks (docs/issues.md, "… bindProject").

    use super::*;
    use orbyn_contracts::{ProjectIssueTracker, SetProjectIssueTracker};

    async fn project_world() -> (super::super::harness::World, String) {
        let w = world(false, Arc::new(|_, _| Turn::Hang)).await;
        let project = w
            .core
            .store
            .list_projects()
            .unwrap()
            .into_iter()
            .next()
            .expect("the harness registers one project");
        (w, project.id)
    }

    fn request(tracker: TrackerKind, scope: &str) -> SetProjectIssueTracker {
        SetProjectIssueTracker {
            tracker: Some(tracker),
            scope: Some(scope.into()),
        }
    }

    async fn connect(w: &super::super::harness::World) {
        w.core
            .issues
            .set_credentials(Credentials::new(Arc::new(Memory::default()), "test"));
        w.core
            .connect_issue_tracker(
                TrackerKind::Gitlab,
                ConnectTracker {
                    host: Some("https://gitlab.example.test".into()),
                    scope: Some("team/app".into()),
                    token: Some("glpat-test".into()),
                },
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_connected_scope_is_stored_answered_and_unbound_idempotently() {
        let (w, project) = project_world().await;
        connect(&w).await;
        let bound = w
            .core
            .bind_project_issue_tracker(&project, request(TrackerKind::Gitlab, "team/app"))
            .await
            .unwrap();
        assert_eq!(
            bound,
            Some(ProjectIssueTracker {
                tracker: TrackerKind::Gitlab,
                scope: "team/app".into(),
            })
        );
        let stored = w.core.store.project_issue_tracker(&project).unwrap();
        assert_eq!(
            stored
                .as_ref()
                .map(|row| (row.tracker.as_str(), row.scope.as_str())),
            Some(("gitlab", "team/app")),
            "the two names are stored, nothing else"
        );
        let audit = w.core.audit_events(None, 50).unwrap();
        let rows: Vec<_> = audit
            .iter()
            .filter(|event| event.action == "issues.project.bind")
            .collect();
        assert_eq!(rows.len(), 1, "one audit row");
        assert_eq!(rows[0].target, project);
        assert_eq!(
            rows[0].detail,
            serde_json::json!({ "project": project, "tracker": "gitlab", "scope": "team/app" })
        );
        // Unbinding is idempotent: the desired end state is "no binding".
        let unbound = w
            .core
            .bind_project_issue_tracker(
                &project,
                SetProjectIssueTracker {
                    tracker: None,
                    scope: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(unbound, None);
        assert!(
            w.core
                .store
                .project_issue_tracker(&project)
                .unwrap()
                .is_none()
        );
        let again = w
            .core
            .bind_project_issue_tracker(
                &project,
                SetProjectIssueTracker {
                    tracker: None,
                    scope: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(again, None, "a second unbind is the same end state");
    }

    #[tokio::test]
    async fn an_unconnected_tracker_or_a_foreign_scope_is_invalid_and_stores_nothing() {
        let (w, project) = project_world().await;
        let unconnected = w
            .core
            .bind_project_issue_tracker(&project, request(TrackerKind::Forgejo, "grp/proj"))
            .await;
        assert!(
            matches!(unconnected, Err(CoreError::Invalid(_))),
            "{unconnected:?}"
        );
        connect(&w).await;
        let foreign = w
            .core
            .bind_project_issue_tracker(&project, request(TrackerKind::Gitlab, "other/repo"))
            .await;
        assert!(matches!(foreign, Err(CoreError::Invalid(_))), "{foreign:?}");
        assert!(
            w.core
                .store
                .project_issue_tracker(&project)
                .unwrap()
                .is_none()
        );
        assert!(
            !w.core
                .audit_events(None, 50)
                .unwrap()
                .iter()
                .any(|event| event.action == "issues.project.bind"),
            "a refused binding writes no audit row"
        );
    }

    #[tokio::test]
    async fn a_scope_outside_the_size_and_control_character_rules_is_invalid_and_stores_nothing() {
        let (w, project) = project_world().await;
        connect(&w).await;
        for scope in ["x".repeat(201), "team/app\u{7}".to_owned()] {
            let refused = w
                .core
                .bind_project_issue_tracker(
                    &project,
                    SetProjectIssueTracker {
                        tracker: Some(TrackerKind::Gitlab),
                        scope: Some(scope),
                    },
                )
                .await;
            assert!(matches!(refused, Err(CoreError::Invalid(_))), "{refused:?}");
        }
        assert!(
            w.core
                .store
                .project_issue_tracker(&project)
                .unwrap()
                .is_none()
        );
        assert!(
            !w.core
                .audit_events(None, 50)
                .unwrap()
                .iter()
                .any(|event| event.action == "issues.project.bind"),
            "nothing was audited"
        );
    }

    #[tokio::test]
    async fn a_tracker_whose_stored_scope_matches_is_still_refused_when_not_connected() {
        let (w, project) = project_world().await;
        // A connection with the right scope but no usable credential: the scope check would pass, so only the
        // connected-tracker check can refuse this.
        w.core
            .store
            .put_tracker(
                &TrackerConnection {
                    tracker: TrackerKind::Gitlab,
                    host: Some("https://gitlab.example.test".into()),
                    scopes: vec!["team/app".into()],
                },
                "2026-11-25T00:00:00Z",
            )
            .unwrap();
        let refused = w
            .core
            .bind_project_issue_tracker(&project, request(TrackerKind::Gitlab, "team/app"))
            .await
            .map(|_| ());
        match refused {
            Err(CoreError::Invalid(message)) => {
                assert!(message.contains("not connected"), "{message}")
            }
            other => {
                panic!("an unconnected tracker is refused before its scope is read: {other:?}")
            }
        }
        // A forge whose scope no remote names is the same refusal, not a scope refusal.
        let azure = w
            .core
            .bind_project_issue_tracker(&project, request(TrackerKind::AzureDevops, "org/proj"))
            .await
            .map(|_| ());
        match azure {
            Err(CoreError::Invalid(message)) => {
                assert!(
                    message.contains("Azure DevOps is not connected"),
                    "{message}"
                )
            }
            other => panic!("an unconnected forge is refused: {other:?}"),
        }
        assert!(
            w.core
                .store
                .project_issue_tracker(&project)
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn an_audit_failure_leaves_no_binding() {
        let (w, project) = project_world().await;
        connect(&w).await;
        rusqlite::Connection::open(w._dir.path().join("data/orbyn.sqlite3"))
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER refuse_binding_audit BEFORE INSERT ON audit_events
                 WHEN NEW.action = 'issues.project.bind'
                 BEGIN SELECT RAISE(ABORT, 'binding audit unavailable'); END;",
            )
            .unwrap();
        let refused = w
            .core
            .bind_project_issue_tracker(&project, request(TrackerKind::Gitlab, "team/app"))
            .await;
        assert!(
            refused.is_err(),
            "the audit refusal fails the bind: {refused:?}"
        );
        assert!(
            w.core
                .store
                .project_issue_tracker(&project)
                .unwrap()
                .is_none(),
            "the audit row is written before the binding, so a failed audit leaves none"
        );
    }

    #[tokio::test]
    async fn an_unknown_project_and_an_unbind_with_a_scope_are_refused() {
        let (w, project) = project_world().await;
        let missing = w
            .core
            .bind_project_issue_tracker("not-a-project", request(TrackerKind::Gitlab, "team/app"))
            .await;
        assert!(
            matches!(missing, Err(CoreError::NotFound(_))),
            "{missing:?}"
        );
        connect(&w).await;
        let scoped_unbind = w
            .core
            .bind_project_issue_tracker(
                &project,
                SetProjectIssueTracker {
                    tracker: None,
                    scope: Some("team/app".into()),
                },
            )
            .await;
        assert!(
            matches!(scoped_unbind, Err(CoreError::Invalid(_))),
            "{scoped_unbind:?}"
        );
    }
}
