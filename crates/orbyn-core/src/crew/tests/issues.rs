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
    IssueSummary, StartIssueThread, TrackerKind,
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
