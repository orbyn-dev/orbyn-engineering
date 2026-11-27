//! Issue trackers beside the code (docs/issues.md): the list a tracker reports, the connections that read it,
//! a thread's links to issues, a thread started from one, and the person's own issue writes (create, comment and
//! react) through the tracker's own credential, with the edits and state moves built for GitHub and GitLab.
//! Everything a tracker returns is untrusted text and is never fed to an engine except in the framed first message;
//! issue text going in travels as a field value, never as an argument.

mod azure;
#[cfg(test)]
pub(crate) mod fake_http;
mod forgejo;
mod frame;
mod github;
mod gitlab;
mod host;
mod linear;
mod query;

#[cfg(test)]
pub(crate) use azure::Azure;
#[cfg(test)]
pub(crate) use forgejo::Forgejo;
#[cfg(test)]
pub(crate) use gitlab::GitLab;
pub use host::validate_host;
pub(crate) use query::MAX_LIMIT;
pub use query::{AssigneeFilter, IssueQuery, StateFilter};

use crate::app::Core;
use crate::credentials::{CredentialError, Credentials};
use crate::error::{CoreError, Result};
use crate::store::{AuditRecord, TrackerConnection, tracker_name, tracker_of};
use crate::util::swr;
use async_trait::async_trait;
use orbyn_contracts::{
    CommentIssue, ConnectTracker, CreateIssue, CreateThread, IssueCandidates, IssueCloseReason,
    IssueComment, IssueCounts, IssueDetail, IssueDraft, IssueKey, IssueList, IssueListFailure,
    IssueReaction, IssueRef, IssueSetStateInput, IssueStateCategory, IssueSummary, IssueTemplate,
    IssueUpdateInput, Project, ProjectIssueTracker, ReactIssue, SetProjectIssueTracker,
    StartIssueThread, Thread, TrackerKind, TrackerRow,
};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

const ALL: [TrackerKind; 5] = [
    TrackerKind::Github,
    TrackerKind::Gitlab,
    TrackerKind::AzureDevops,
    TrackerKind::Forgejo,
    TrackerKind::Linear,
];
const CACHE_FOR: Duration = Duration::from_secs(30);
const MAX_SCOPE_BYTES: usize = 200;
/// The most a title may hold, in characters (docs/issues.md, "Create, comment, react").
const MAX_TITLE_CHARS: usize = 256;
/// The most an issue body or a comment may hold, in bytes.
const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_LABELS: usize = 50;
/// The most a label name may hold, in characters. A label travels by name, so this is also the ceiling the picker keeps
/// a label name to (`web/src/features/issues/edit.ts`, `LABEL_NAME_MAX`): cutting below it would send a different label.
const MAX_LABEL_CHARS: usize = 200;
const MAX_ASSIGNEES: usize = 25;
/// The most a candidate id or a comment id may hold, in characters (a reader's own id cap).
const MAX_ID_CHARS: usize = 200;
/// The issue templates one scope answers, and the most of a template's body that is kept (docs/issues.md,
/// "Create, comment, react, templates and project binding").
const MAX_TEMPLATES: usize = 50;
const MAX_TEMPLATE_BODY_BYTES: usize = 16 * 1024;
/// The most labels and the most people one candidates read answers (docs/issues.md, "Issue writes and list
/// controls"): a scope with more sets `truncated`, so a missing name is not proof the person is ineligible.
const MAX_CANDIDATES: usize = 200;
const GITLAB_DEFAULT_HOST: &str = "https://gitlab.com";
/// The one Linear connection's teams fallback, cached for [`CACHE_FOR`] like a listing.
const LINEAR_TEAMS: &str = "linear teams";

/// One scope's page of a listing.
#[derive(Clone, Debug)]
pub(crate) struct ScopeList {
    pub issues: Vec<IssueSummary>,
    pub counts: Option<IssueCounts>,
    pub next: Option<String>,
}

/// One dependency or advisory update a tracker reports for one scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DependencyUpdate {
    /// A stable identity the tracker gives the update (an alert's number, or a package and version): the
    /// listener claims it first write wins, so the same update starts one run.
    pub key: String,
    /// The ecosystem the tracker names (`npm`, `cargo`, ...), when it names one. It is compared with the stored
    /// trigger's own `ecosystem` setting and never reaches a prompt.
    pub ecosystem: Option<String>,
}

/// What a tracker can report about dependency or advisory updates for one scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DependencyFeed {
    /// The tracker has no feed Orbyn reads for this scope. Nothing is claimed and no refusal is written: a
    /// tracker that cannot answer is not a refusal.
    Cannot,
    /// The updates the tracker reports for the scope. GitHub answers it from its Dependabot alerts; every other
    /// reader takes the trait's default.
    Updates(Vec<DependencyUpdate>),
}

/// One label event a tracker reports for one issue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LabelEvent {
    /// A stable identity the tracker gives the event: the listener claims it first write wins, so a label removed
    /// and added again starts one run even though the issue's label set looks the same.
    pub id: String,
    /// The issue key the event belongs to.
    pub issue: String,
}

/// What a tracker can report about label events for one scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LabelFeed {
    /// The tracker has no label event feed Orbyn reads for this scope. Nothing is claimed and no refusal is written:
    /// a tracker that cannot answer is not a refusal.
    Cannot,
    /// The label events the tracker reports for the scope, newest first. GitHub answers it from the repository's
    /// issue events; every other reader takes the trait's default.
    Events(Vec<LabelEvent>),
}

/// A create whose outcome nobody saw: the issue may exist now, so the answer says so and nothing retries (docs/issues.md,
/// "Issue writes"). Only the final create request goes through this; the reads before it create nothing. A refusal keeps
/// its own meaning.
pub(crate) fn possibly_created(tracker: &str, error: CoreError) -> CoreError {
    match error {
        CoreError::TrackerUnavailable(_) => CoreError::TrackerUnavailable(format!(
            "{tracker} did not answer, so the issue was possibly created. Re-read the list before writing it again."
        )),
        other => other,
    }
}

/// Whether a write's answer leaves one scope's cached listings unable to describe the tracker: the tracker accepted
/// the write, or its answer does not prove the write failed, so it may have applied it. Only an answer decided from
/// the request alone proves nothing was stored: every reader checks its fields before it sends anything, so `Invalid`
/// came back without a request, and `Unsupported` is a reader with no write path at all. A rate limit, a forbidden
/// answer, a `NotFound`, a broken connection, a timeout or an unreadable reply can each come back from the read that
/// answers a write the tracker already stored (GitHub patches the issue and then reads it back), so none of them
/// proves the edit was dropped, and each drops the scope's pages too.
fn write_may_have_changed<T>(written: &Result<T>) -> bool {
    !matches!(
        written,
        Err(CoreError::Invalid(_) | CoreError::Unsupported(_))
    )
}

/// A new issue as the owning tracker receives it: the person's validated text and the choices that name labels and
/// people. `Core::create_issue` checks every field before a source sees it.
pub(crate) struct NewIssue<'a> {
    pub title: &'a str,
    pub body: &'a str,
    pub labels: &'a [String],
    pub assignees: &'a [String],
}

/// The fields one edit writes, as the owning tracker receives them (docs/issues.md, "Issue writes and list
/// controls"). `None` means the field is not written at all, while `Some(&[])` clears a whole set: labels and
/// assignees are always the whole new set, never an addition. `Core::update_issue` checks every field before a
/// source sees it.
pub(crate) struct IssueEdit<'a> {
    pub title: Option<&'a str>,
    pub body: Option<&'a str>,
    pub labels: Option<&'a [String]>,
    pub assignees: Option<&'a [String]>,
}

/// What a tracker offers. A scope and a key are checked by the source before anything is sent.
#[async_trait]
pub(crate) trait IssueSource: Send + Sync {
    async fn list(&self, scope: &str, query: &IssueQuery) -> Result<ScopeList>;
    async fn get(&self, scope: &str, key: &str) -> Result<IssueDetail>;
    /// The scopes the tracker itself names when a connection stores none. Only Linear needs this: a team key
    /// cannot be read from a git remote (docs/issues.md, "Linear as built").
    async fn scopes(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
    /// What this tracker can report about dependency or advisory updates for one scope. The default is
    /// [`DependencyFeed::Cannot`] and every reader but GitHub takes it: GitHub answers its Dependabot alerts feed,
    /// and a tracker that cannot answer spends no event.
    async fn dependency_updates(&self, _scope: &str) -> Result<DependencyFeed> {
        Ok(DependencyFeed::Cannot)
    }
    /// What this tracker can report about label events for one scope. The default is [`LabelFeed::Cannot`] and
    /// every reader but GitHub takes it: GitHub answers the repository's issue events feed, and a tracker that
    /// cannot answer spends no event.
    async fn label_events(&self, _scope: &str, _label: &str) -> Result<LabelFeed> {
        Ok(LabelFeed::Cannot)
    }
    /// Whether this reader also writes issues (docs/issues.md, "Create, comment, react"). Only the trackers whose
    /// own write is built say true; `Core` refuses a write for the others before anything is sent, and R15 PR1b
    /// adds them. The default is false.
    fn can_write(&self) -> bool {
        false
    }
    /// Creates one issue and answers it as the tracker has it. Only called when [`Self::can_write`] is true. A call
    /// whose outcome is unknown (a timeout) is `TrackerUnavailable` and says the issue was possibly created: the
    /// caller creates one issue per call and never retries.
    async fn create(&self, _scope: &str, _new: &NewIssue<'_>) -> Result<IssueDetail> {
        Err(CoreError::Unsupported(
            "This tracker cannot create issues from Orbyn yet.".into(),
        ))
    }
    /// Writes one comment on one issue and answers it as the tracker stored it. Only called when
    /// [`Self::can_write`] is true.
    async fn comment(&self, _scope: &str, _key: &str, _body: &str) -> Result<IssueComment> {
        Err(CoreError::Unsupported(
            "This tracker cannot comment on issues from Orbyn yet.".into(),
        ))
    }
    /// Adds (`on`) or takes back (`!on`) one reaction, on the issue when `comment` is none, and answers the issue
    /// with its reaction counts. Both directions are idempotent. Only called when [`Self::can_write`] is true.
    async fn react(
        &self,
        _scope: &str,
        _key: &str,
        _comment: Option<&str>,
        _reaction: IssueReaction,
        _on: bool,
    ) -> Result<IssueDetail> {
        Err(CoreError::Unsupported(
            "This tracker cannot react from Orbyn yet.".into(),
        ))
    }
    /// The tracker's own issue templates for one scope (docs/issues.md, "Create, comment, react, templates and
    /// project binding"). The default answers none: a tracker whose API has no template source answers `[]`, never
    /// 404. `Core::issue_templates` keeps at most 50 and cuts a body to 16 KiB.
    async fn templates(&self, _scope: &str) -> Result<Vec<IssueTemplate>> {
        Ok(Vec::new())
    }
    /// Whether this reader also edits an issue's own fields (docs/issues.md, "Issue writes and list controls"):
    /// its title, body, labels and assignees, its state, and the labels and people a picker offers. Only the
    /// trackers whose edit is built say true; `Core` refuses the edits of the others before anything is sent.
    fn can_edit(&self) -> bool {
        false
    }
    /// Writes the fields an edit carries and answers the issue as the tracker stored it. Only called when
    /// [`Self::can_edit`] is true, and only with fields `Core::update_issue` has checked.
    async fn update(&self, _scope: &str, _key: &str, _edit: &IssueEdit<'_>) -> Result<IssueDetail> {
        Err(CoreError::Unsupported(
            "This tracker cannot edit issues from Orbyn yet.".into(),
        ))
    }
    /// Moves one issue to one of the states its own detail declares, and answers it as the tracker stored it.
    /// Only called with a name the issue offered; a tracker that cannot say `not_planned` closes as completed and
    /// its answer says what it stored. Only called when [`Self::can_edit`] is true.
    async fn set_state(
        &self,
        _scope: &str,
        _key: &str,
        _state: &str,
        _close_reason: Option<IssueCloseReason>,
    ) -> Result<IssueDetail> {
        Err(CoreError::Unsupported(
            "This tracker cannot change an issue's state from Orbyn yet.".into(),
        ))
    }
    /// The labels and people a picker may offer for one scope, in the tracker's own words. `Core` keeps at most
    /// 200 of each and says whether there were more.
    async fn candidates(&self, _scope: &str) -> Result<IssueCandidates> {
        Err(CoreError::Unsupported(
            "This tracker cannot list its labels and people from Orbyn yet.".into(),
        ))
    }
}

pub(crate) struct IssueDesk {
    credentials: Mutex<Credentials>,
    sources: Mutex<HashMap<TrackerKind, Arc<dyn IssueSource>>>,
    cache: Mutex<Listings>,
    /// Linear's teams fallback, which every no-scope read would otherwise ask for again.
    scope_cache: Mutex<HashMap<String, (Instant, Vec<String>)>>,
    /// Tests send the reader GitLab builds to a local fake server instead of the stored host.
    #[cfg(test)]
    gitlab_test_base: Mutex<Option<String>>,
    /// Tests send the reader Forgejo builds to a local fake server instead of the stored host.
    #[cfg(test)]
    forgejo_test_base: Mutex<Option<String>>,
    /// Tests send the reader Azure DevOps builds to a local fake server instead of the stored host.
    #[cfg(test)]
    azure_test_base: Mutex<Option<String>>,
    /// Tests send the reader Linear builds to a local fake server instead of the fixed host.
    #[cfg(test)]
    linear_test_base: Mutex<Option<String>>,
}

/// The 30-second listing cache, and how many times the cache has been dropped.
///
/// A listing asks the tracker without holding the lock, so a write can land while it is out. The count is bumped under
/// the same lock that drops the entries: a listing whose count changed while it was out leaves its answer out of the
/// cache, so a write is never undone by a request it overlapped.
#[derive(Default)]
struct Listings {
    /// How many times the cache has been dropped. A listing compares the value it started with to the value it finds.
    dropped: u64,
    entries: HashMap<String, (Instant, ScopeList)>,
}

impl Listings {
    /// One listing, as it was cached, while it is younger than [`CACHE_FOR`].
    fn get(&self, key: &str) -> Option<ScopeList> {
        let (at, list) = self.entries.get(key)?;
        (at.elapsed() < CACHE_FOR).then(|| list.clone())
    }

    /// Caches one listing, unless the cache was dropped while that listing was out. A request that overlapped a write
    /// may be answered from the tracker as it was before that write, so its answer must not become the next page.
    fn put_since(&mut self, dropped: u64, key: String, list: ScopeList) {
        if self.dropped == dropped {
            self.entries.insert(key, (Instant::now(), list));
        }
    }

    /// Drops every entry and bumps [`Self::dropped`], so a listing already out cannot cache its answer.
    fn drop_all(&mut self) {
        self.entries.clear();
        self.dropped += 1;
    }

    /// Drops every entry whose key starts with `prefix`, bumping [`Self::dropped`] like [`Self::drop_all`]. A prefix
    /// ends at a [`swr::key`] separator, so it always names a whole scope and never the start of a longer name
    /// (see [`IssueDesk::scope_prefix`]).
    fn drop_prefixed(&mut self, prefix: &str) {
        self.entries.retain(|key, _| !key.starts_with(prefix));
        self.dropped += 1;
    }
}

impl IssueDesk {
    pub(crate) fn new(credentials: Credentials) -> Self {
        Self {
            credentials: Mutex::new(credentials),
            sources: Mutex::new(HashMap::new()),
            cache: Mutex::new(Listings::default()),
            scope_cache: Mutex::new(HashMap::new()),
            #[cfg(test)]
            gitlab_test_base: Mutex::new(None),
            #[cfg(test)]
            forgejo_test_base: Mutex::new(None),
            #[cfg(test)]
            azure_test_base: Mutex::new(None),
            #[cfg(test)]
            linear_test_base: Mutex::new(None),
        }
    }

    fn creds(&self) -> Credentials {
        self.credentials
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn set_credentials(&self, credentials: Credentials) {
        *self
            .credentials
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = credentials;
    }

    #[cfg(test)]
    pub(crate) fn set_source(&self, kind: TrackerKind, source: Arc<dyn IssueSource>) {
        self.sources
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(kind, source);
    }

    /// A local plain-http server the GitLab source built by `gitlab_source` talks to, in tests only.
    #[cfg(test)]
    pub(crate) fn set_gitlab_test_base(&self, base: &str) {
        *self
            .gitlab_test_base
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(base.trim_end_matches('/').to_owned());
    }

    #[cfg(test)]
    fn gitlab_test_base(&self) -> Option<String> {
        self.gitlab_test_base
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// A local plain-http server the Forgejo source built by `forgejo_source` talks to, in tests only.
    #[cfg(test)]
    pub(crate) fn set_forgejo_test_base(&self, base: &str) {
        *self
            .forgejo_test_base
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(base.trim_end_matches('/').to_owned());
    }

    #[cfg(test)]
    fn forgejo_test_base(&self) -> Option<String> {
        self.forgejo_test_base
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// A local plain-http server the Azure DevOps source built by `azure_source` talks to, in tests only.
    #[cfg(test)]
    pub(crate) fn set_azure_test_base(&self, base: &str) {
        *self
            .azure_test_base
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(base.trim_end_matches('/').to_owned());
    }

    #[cfg(test)]
    fn azure_test_base(&self) -> Option<String> {
        self.azure_test_base
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// A local plain-http server the Linear source built by `linear_source` talks to, in tests only.
    #[cfg(test)]
    pub(crate) fn set_linear_test_base(&self, base: &str) {
        *self
            .linear_test_base
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(base.trim_end_matches('/').to_owned());
    }

    #[cfg(test)]
    fn linear_test_base(&self) -> Option<String> {
        self.linear_test_base
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// A tracker-named scope list cached for [`CACHE_FOR`]. Only the Linear teams fallback uses this.
    fn cached_scopes(&self, key: &str) -> Option<Vec<String>> {
        let cache = self
            .scope_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let (at, scopes) = cache.get(key)?;
        (at.elapsed() < CACHE_FOR).then(|| scopes.clone())
    }

    fn cache_scopes(&self, key: &str, scopes: Vec<String>) {
        self.scope_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.to_owned(), (Instant::now(), scopes));
    }

    fn clear_scopes(&self) {
        self.scope_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    /// The key one listing is cached under: its connection's tracker and scope, then every filter, sort, limit and
    /// page the query carries. [`Self::scope_prefix`] names the part a write drops.
    fn list_key(kind: TrackerKind, scope: &str, query: &IssueQuery) -> String {
        format!("{}{query:?}", Self::scope_prefix(kind, scope))
    }

    /// What every cached listing of one connection's scope starts with: the tracker and the scope, each ending at a
    /// [`swr::key`] separator, then the query's `Debug`. The separator is a control character and a scope with a
    /// control character is refused, so a scope's prefix ends exactly where its name ends: the prefix of
    /// `org/Project` matches no page of `org/Project Two`, whose name only starts with the written one.
    fn scope_prefix(kind: TrackerKind, scope: &str) -> String {
        swr::key(&[tracker_name(kind), scope])
    }

    /// Drops every cached listing of one connection's scope, whatever its filter, sort or page, after a write whose
    /// effect reaches it (docs/issues.md, "Cache"). The scope is as narrow as the write: the tracker itself applies
    /// the filters, so which cached pages the changed issue belongs to cannot be known here, while the connection
    /// and scope of the write name every page it can reach. Pages of other scopes and connections stay.
    pub(crate) fn scope_changed(&self, kind: TrackerKind, scope: &str) {
        let prefix = Self::scope_prefix(kind, scope);
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drop_prefixed(prefix.as_str());
    }

    /// Drops every cached listing, because the connection one was cached from is gone: a connect and a disconnect both
    /// change which tracker the next listing asks. A test whose fake tracker changed its answer inside the cache window
    /// without a write has no scope to drop, so it drops all of it too.
    fn drop_listings(&self) {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drop_all();
    }

    /// Drops the whole 30-second listing cache. Tests call it between passes when a fake tracker changes its answer
    /// inside the cache window without a write; a real write drops its own scope at once (`scope_changed`), and a
    /// connect or disconnect drops everything, because the connection it was cached from is gone.
    #[cfg(test)]
    pub(crate) fn clear_cache(&self) {
        self.drop_listings();
    }
}

fn credential_error(error: CredentialError) -> CoreError {
    match error {
        CredentialError::InvalidKey => {
            CoreError::Invalid("The token is empty, too long or has a space or line break.".into())
        }
        CredentialError::Unavailable => CoreError::Unsupported(
            "There is no usable keychain here, so a token cannot be saved.".into(),
        ),
        CredentialError::Blocked => {
            CoreError::Unsupported("The keychain refused for now. Unlock it and try again.".into())
        }
        CredentialError::UnknownEngine | CredentialError::Off | CredentialError::Failed => {
            CoreError::Internal("The keychain failed.".into())
        }
    }
}

/// `owner/repo` of a github.com remote, in either form `git` writes.
pub(crate) fn github_repo_of_remote(url: &str) -> Option<String> {
    let url = url.trim();
    let path = if let Some(rest) = url.strip_prefix("https://") {
        let rest = rest.rsplit_once('@').map_or(rest, |(_, r)| r);
        rest.strip_prefix("github.com/")?
    } else if let Some(rest) = url.strip_prefix("ssh://") {
        let rest = rest.rsplit_once('@').map_or(rest, |(_, r)| r);
        rest.strip_prefix("github.com/")?
    } else {
        url.strip_prefix("git@github.com:")?
    };
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = path.split('/');
    let (owner, repo) = (parts.next()?, parts.next()?);
    (parts.next().is_none() && !owner.is_empty() && !repo.is_empty())
        .then(|| format!("{owner}/{repo}"))
}

/// The scope one tracker names for a project's origin remote, or none when the remote is on another host or names no
/// container. Shared by `issue_scopes` (a listing over every project the connection may read) and a listener's
/// `project_issue_reader` (the one project behind a crew), so the two mappings cannot drift. Linear names a team key
/// that a git remote cannot give, so it answers none.
fn scope_of_remote(connection: &TrackerConnection, remote: &str) -> Option<String> {
    let host = match connection.tracker {
        TrackerKind::Github | TrackerKind::Gitlab => {
            connection.host.as_deref().unwrap_or(GITLAB_DEFAULT_HOST)
        }
        TrackerKind::AzureDevops | TrackerKind::Forgejo => {
            connection.host.as_deref().unwrap_or_default()
        }
        TrackerKind::Linear => return None,
    };
    match connection.tracker {
        TrackerKind::Github => github_repo_of_remote(remote),
        TrackerKind::Gitlab => gitlab::gitlab_path_of_remote(remote, host),
        TrackerKind::AzureDevops => azure::azure_scope_of_remote(remote, host),
        TrackerKind::Forgejo => forgejo::forgejo_path_of_remote(remote, host),
        TrackerKind::Linear => None,
    }
}

/// The issue reader that can answer for one project: the tracker its checkout's origin remote names, and the
/// scope that remote maps to. Only a connected tracker whose host matches the remote can answer, and Linear reads
/// team keys that no git remote names, so it never answers for a project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectReader {
    pub tracker: TrackerKind,
    pub scope: String,
}

impl Core {
    /// The issue reader for one project, or none. A folder Orbyn is not trusted with, a folder that is not a git
    /// checkout, a checkout with no `origin` remote and a project whose tracker is not connected or is on another
    /// host all answer none: a listener reads nothing for them and spends no event. The scope is derived from the
    /// project's own remote exactly as `issue_scopes` derives it, so nothing outside the connection's scopes is read.
    pub(crate) async fn project_issue_reader(
        &self,
        project: &Project,
    ) -> Result<Option<ProjectReader>> {
        if project.scratch || !project.is_git || !project.trusted {
            return Ok(None);
        }
        let root = std::path::Path::new(&project.path);
        let Ok(url) = crate::source_control::git::git(root, &["remote", "get-url", "origin"]).await
        else {
            return Ok(None);
        };
        let remote = crate::source_control::git::text(&url);
        let connections = self.store.tracker_connections()?;
        for row in self
            .issue_trackers()
            .await?
            .into_iter()
            .filter(|row| row.connected)
        {
            let connection = connections
                .iter()
                .find(|connection| connection.tracker == row.tracker)
                .cloned()
                .unwrap_or(TrackerConnection {
                    tracker: row.tracker,
                    host: None,
                    scopes: Vec::new(),
                });
            if let Some(scope) =
                scope_of_remote(&connection, &remote).filter(|scope| !scope.is_empty())
            {
                return Ok(Some(ProjectReader {
                    tracker: row.tracker,
                    scope,
                }));
            }
        }
        Ok(None)
    }

    /// One scope's page of issues for a background listener: only the tracker and scope a project's reader named,
    /// so a listener can read no other project's issues. Reads only; the listing's 30-second cache applies.
    pub(crate) async fn scope_issues(
        &self,
        tracker: TrackerKind,
        scope: &str,
        query: &IssueQuery,
    ) -> Result<ScopeList> {
        self.scope_list(tracker, scope, query).await
    }

    /// What the project's tracker reports about dependency or advisory updates. GitHub answers from its
    /// Dependabot alerts; every other tracker answers [`DependencyFeed::Cannot`], and the listener spends no event.
    pub(crate) async fn tracker_dependency_updates(
        &self,
        tracker: TrackerKind,
        scope: &str,
    ) -> Result<DependencyFeed> {
        self.issue_source(tracker)?.dependency_updates(scope).await
    }

    /// What the project's tracker reports about label events for one stored label. GitHub answers from the
    /// repository's issue events; every other tracker answers [`LabelFeed::Cannot`], and the listener spends no event.
    pub(crate) async fn tracker_label_events(
        &self,
        tracker: TrackerKind,
        scope: &str,
        label: &str,
    ) -> Result<LabelFeed> {
        self.issue_source(tracker)?.label_events(scope, label).await
    }

    fn issue_source(&self, kind: TrackerKind) -> Result<Arc<dyn IssueSource>> {
        if let Some(source) = self
            .issues
            .sources
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&kind)
        {
            return Ok(source.clone());
        }
        match kind {
            TrackerKind::Github => Ok(Arc::new(github::GitHub {
                cwd: self.config.data_dir.clone(),
            })),
            TrackerKind::Gitlab => self.gitlab_source(),
            TrackerKind::AzureDevops => self.azure_source(),
            TrackerKind::Forgejo => self.forgejo_source(),
            TrackerKind::Linear => self.linear_source(),
        }
    }

    /// The GitLab reader of the stored connection: its https host and the token from the keychain.
    pub(crate) fn gitlab_source(&self) -> Result<Arc<dyn IssueSource>> {
        let not_connected = |why: &str| CoreError::TrackerNotConnected(why.to_owned());
        let connection = self
            .store
            .tracker_connections()?
            .into_iter()
            .find(|c| c.tracker == TrackerKind::Gitlab)
            .ok_or_else(|| not_connected("GitLab is not connected."))?;
        let token = self
            .issues
            .creds()
            .tracker_token(tracker_name(TrackerKind::Gitlab))
            .map_err(|error| match error {
                CredentialError::Unavailable => not_connected("There is no usable keychain here"),
                CredentialError::Blocked => {
                    not_connected("The keychain refused for now. Unlock it and try again.")
                }
                _ => not_connected("The saved GitLab token could not be read."),
            })?
            .ok_or_else(|| not_connected("No token is saved"))?;
        let host = connection.host.as_deref().unwrap_or(GITLAB_DEFAULT_HOST);
        #[cfg(test)]
        if let Some(base) = self.issues.gitlab_test_base() {
            validate_host(host)?;
            return Ok(Arc::new(gitlab::GitLab::for_test(
                &base,
                token.as_str(),
                Duration::from_secs(10),
            )));
        }
        Ok(Arc::new(gitlab::GitLab::new(host, token)?))
    }

    /// The Forgejo reader of the stored connection: its https host and the token from the keychain.
    pub(crate) fn forgejo_source(&self) -> Result<Arc<dyn IssueSource>> {
        let not_connected = |why: &str| CoreError::TrackerNotConnected(why.to_owned());
        let connection = self
            .store
            .tracker_connections()?
            .into_iter()
            .find(|c| c.tracker == TrackerKind::Forgejo)
            .ok_or_else(|| not_connected("Forgejo is not connected."))?;
        let token = self
            .issues
            .creds()
            .tracker_token(tracker_name(TrackerKind::Forgejo))
            .map_err(|error| match error {
                CredentialError::Unavailable => not_connected("There is no usable keychain here"),
                CredentialError::Blocked => {
                    not_connected("The keychain refused for now. Unlock it and try again.")
                }
                _ => not_connected("The saved Forgejo token could not be read."),
            })?
            .ok_or_else(|| not_connected("No token is saved"))?;
        let host = connection
            .host
            .as_deref()
            .ok_or_else(|| not_connected("Forgejo has no host saved."))?;
        #[cfg(test)]
        if let Some(base) = self.issues.forgejo_test_base() {
            validate_host(host)?;
            return Ok(Arc::new(forgejo::Forgejo::for_test(
                &base,
                token.as_str(),
                Duration::from_secs(10),
            )));
        }
        Ok(Arc::new(forgejo::Forgejo::new(host, token)?))
    }

    /// The Azure DevOps reader of the stored connection: its https host and the token from the keychain.
    pub(crate) fn azure_source(&self) -> Result<Arc<dyn IssueSource>> {
        let not_connected = |why: &str| CoreError::TrackerNotConnected(why.to_owned());
        let connection = self
            .store
            .tracker_connections()?
            .into_iter()
            .find(|c| c.tracker == TrackerKind::AzureDevops)
            .ok_or_else(|| not_connected("Azure DevOps is not connected."))?;
        let token = self
            .issues
            .creds()
            .tracker_token(tracker_name(TrackerKind::AzureDevops))
            .map_err(|error| match error {
                CredentialError::Unavailable => not_connected("There is no usable keychain here"),
                CredentialError::Blocked => {
                    not_connected("The keychain refused for now. Unlock it and try again.")
                }
                _ => not_connected("The saved Azure DevOps token could not be read."),
            })?
            .ok_or_else(|| not_connected("No token is saved"))?;
        let host = connection
            .host
            .as_deref()
            .ok_or_else(|| not_connected("Azure DevOps has no host saved."))?;
        #[cfg(test)]
        if let Some(base) = self.issues.azure_test_base() {
            validate_host(host)?;
            return Ok(Arc::new(azure::Azure::for_test(
                &base,
                token.as_str(),
                Duration::from_secs(10),
            )));
        }
        Ok(Arc::new(azure::Azure::new(host, token)?))
    }

    /// The Linear reader of the stored connection: the one endpoint and the token from the keychain.
    pub(crate) fn linear_source(&self) -> Result<Arc<dyn IssueSource>> {
        let not_connected = |why: &str| CoreError::TrackerNotConnected(why.to_owned());
        let connection = self
            .store
            .tracker_connections()?
            .into_iter()
            .find(|c| c.tracker == TrackerKind::Linear)
            .ok_or_else(|| not_connected("Linear is not connected."))?;
        let token = self
            .issues
            .creds()
            .tracker_token(tracker_name(TrackerKind::Linear))
            .map_err(|error| match error {
                CredentialError::Unavailable => not_connected("There is no usable keychain here"),
                CredentialError::Blocked => {
                    not_connected("The keychain refused for now. Unlock it and try again.")
                }
                _ => not_connected("The saved Linear token could not be read."),
            })?
            .ok_or_else(|| not_connected("No token is saved"))?;
        let host = connection
            .host
            .as_deref()
            .ok_or_else(|| not_connected("Linear has no host saved."))?;
        #[cfg(test)]
        if let Some(base) = self.issues.linear_test_base() {
            linear::validate_linear_host(host)?;
            return Ok(Arc::new(linear::Linear::for_test(
                &base,
                token.as_str(),
                Duration::from_secs(10),
            )));
        }
        Ok(Arc::new(linear::Linear::new(host, token)?))
    }

    /// The scopes a tracker is read in: the scopes the connection names, then for GitHub, GitLab, Azure DevOps
    /// and Forgejo the projects of this machine's git remotes (for the last three only those on the connection's
    /// host); for Linear the teams its key can see when it names none; for the others only the connection's own.
    async fn issue_scopes(&self, connection: &TrackerConnection) -> Result<Vec<String>> {
        let mut scopes = connection.scopes.clone();
        if connection.tracker == TrackerKind::Linear && scopes.is_empty() {
            if let Some(cached) = self.issues.cached_scopes(LINEAR_TEAMS) {
                return Ok(cached);
            }
            let teams = self.issue_source(TrackerKind::Linear)?.scopes().await?;
            self.issues.cache_scopes(LINEAR_TEAMS, teams.clone());
            return Ok(teams);
        }
        if matches!(
            connection.tracker,
            TrackerKind::Github
                | TrackerKind::Gitlab
                | TrackerKind::AzureDevops
                | TrackerKind::Forgejo
        ) {
            for project in self.store.list_projects().unwrap_or_default() {
                if project.scratch || !project.is_git {
                    continue;
                }
                let cwd = std::path::Path::new(&project.path);
                let Ok(url) =
                    crate::source_control::git::git(cwd, &["remote", "get-url", "origin"]).await
                else {
                    continue;
                };
                let remote = crate::source_control::git::text(&url);
                if let Some(path) = scope_of_remote(connection, &remote)
                    && !scopes.contains(&path)
                {
                    scopes.push(path);
                }
            }
        }
        Ok(scopes)
    }

    /// Every tracker, whether it is connected, and why not.
    pub async fn issue_trackers(&self) -> Result<Vec<TrackerRow>> {
        let stored = self.store.tracker_connections()?;
        let mut rows = Vec::new();
        for kind in ALL {
            let connection = stored.iter().find(|c| c.tracker == kind);
            let (connected, detail) = match kind {
                TrackerKind::Github => {
                    if crate::host::process::resolve("gh").is_some() {
                        (true, None)
                    } else {
                        (false, Some("gh is not installed".to_owned()))
                    }
                }
                other => match (
                    connection,
                    self.issues.creds().tracker_saved(tracker_name(other)),
                ) {
                    (Some(_), Ok(true)) => (true, None),
                    (Some(_), Ok(false)) => (false, Some("No token is saved".to_owned())),
                    (_, Err(CredentialError::Unavailable)) => {
                        (false, Some("There is no usable keychain here".to_owned()))
                    }
                    _ => (false, None),
                },
            };
            rows.push(TrackerRow {
                tracker: kind,
                connected,
                scope: connection.and_then(|c| c.scopes.first().cloned()),
                host: connection.and_then(|c| c.host.clone()),
                detail,
            });
        }
        Ok(rows)
    }

    /// Connects a tracker. The token goes to the keychain and nowhere else; the host and scope are stored.
    pub async fn connect_issue_tracker(
        &self,
        kind: TrackerKind,
        input: ConnectTracker,
    ) -> Result<TrackerRow> {
        let scope = input
            .scope
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if let Some(scope) = scope
            && (scope.len() > MAX_SCOPE_BYTES || scope.chars().any(char::is_control))
        {
            return Err(CoreError::Invalid(
                "The scope is too long or has control characters.".into(),
            ));
        }
        let host = match kind {
            TrackerKind::Github => {
                if input.host.is_some() || input.token.is_some() {
                    return Err(CoreError::Invalid(
                        "GitHub reads through your own gh: it takes no host or token here.".into(),
                    ));
                }
                if crate::host::process::resolve("gh").is_none() {
                    return Err(CoreError::TrackerNotConnected(
                        "Install gh and sign in to it to read GitHub issues.".into(),
                    ));
                }
                None
            }
            TrackerKind::Linear => {
                if input.host.is_some() {
                    return Err(CoreError::Invalid("Linear has one host.".into()));
                }
                Some("https://api.linear.app".to_owned())
            }
            TrackerKind::Gitlab => Some(validate_host(
                input.host.as_deref().unwrap_or(GITLAB_DEFAULT_HOST),
            )?),
            TrackerKind::AzureDevops | TrackerKind::Forgejo => {
                Some(validate_host(input.host.as_deref().ok_or_else(|| {
                    CoreError::Invalid("This tracker needs a host.".into())
                })?)?)
            }
        };
        if kind == TrackerKind::AzureDevops {
            let host = host
                .as_deref()
                .ok_or_else(|| CoreError::Invalid("This tracker needs a host.".into()))?;
            azure::check_connect_host(host)?;
            if let Some(scope) = scope {
                azure::check_connect_scope(scope)?;
            }
        }
        if kind == TrackerKind::Linear
            && let Some(scope) = scope
        {
            linear::team_key(scope)?;
        }
        if kind != TrackerKind::Github {
            let token = input
                .token
                .as_deref()
                .ok_or_else(|| CoreError::Invalid("This tracker needs a token.".into()))?;
            self.issues
                .creds()
                .tracker_save(tracker_name(kind), token)
                .map_err(credential_error)?;
        }
        let connection = TrackerConnection {
            tracker: kind,
            host,
            scopes: scope.map(str::to_owned).into_iter().collect(),
        };
        self.store.put_tracker(&connection, &crate::util::now())?;
        self.issues.drop_listings();
        self.issues.clear_scopes();
        Ok(self
            .issue_trackers()
            .await?
            .into_iter()
            .find(|row| row.tracker == kind)
            .expect("every tracker has a row"))
    }

    /// Removes the connection and the token.
    pub async fn disconnect_issue_tracker(&self, kind: TrackerKind) -> Result<TrackerRow> {
        if kind != TrackerKind::Github {
            self.issues
                .creds()
                .tracker_remove(tracker_name(kind))
                .map_err(credential_error)?;
        }
        self.store.delete_tracker(kind)?;
        self.issues.drop_listings();
        self.issues.clear_scopes();
        Ok(self
            .issue_trackers()
            .await?
            .into_iter()
            .find(|row| row.tracker == kind)
            .expect("every tracker has a row"))
    }

    async fn scope_list(
        &self,
        kind: TrackerKind,
        scope: &str,
        query: &IssueQuery,
    ) -> Result<ScopeList> {
        let id = IssueDesk::list_key(kind, scope, query);
        // The answer is asked for without the lock, so a write can drop this scope while the request is out. The count
        // read here says whether that happened: if it did, the answer is the tracker as it was before the write and it
        // stays out of the cache, so the next listing reads the tracker again instead of serving a page a write undid.
        let dropped = {
            let cache = self
                .issues
                .cache
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            match cache.get(&id) {
                Some(hit) => return Ok(hit),
                None => cache.dropped,
            }
        };
        let fresh = self.issue_source(kind)?.list(scope, query).await?;
        self.issues
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .put_since(dropped, id, fresh.clone());
        Ok(fresh)
    }

    /// The listing over every connected tracker (or the one asked for), newest update first.
    pub async fn list_issues(&self, query: &IssueQuery) -> Result<IssueList> {
        let rows = self.issue_trackers().await?;
        let stored = self.store.tracker_connections()?;
        let mut jobs = Vec::new();
        for row in rows
            .iter()
            .filter(|r| r.connected && query.tracker.is_none_or(|t| t == r.tracker))
        {
            let connection = stored
                .iter()
                .find(|c| c.tracker == row.tracker)
                .cloned()
                .unwrap_or(TrackerConnection {
                    tracker: row.tracker,
                    host: None,
                    scopes: Vec::new(),
                });
            jobs.push((row.tracker, connection));
        }
        let mut list = IssueList {
            issues: Vec::new(),
            counts: None,
            failed: Vec::new(),
            next_cursors: BTreeMap::new(),
        };
        let mut totals = (0u32, 0u32);
        let mut counted = true;
        let mut any = false;
        for (kind, connection) in jobs {
            let mut scopes = match self.issue_scopes(&connection).await {
                Ok(scopes) => scopes,
                Err(error) => {
                    if matches!(&error, CoreError::Invalid(_)) {
                        return Err(error);
                    }
                    counted = false;
                    list.failed.push(IssueListFailure {
                        tracker: kind,
                        message: error.public_message(),
                    });
                    continue;
                }
            };
            if !query.cursors.is_empty() {
                // Carrying on: only the scopes named in `cursors` are read, one page each.
                scopes.retain(|s| {
                    query
                        .cursors
                        .contains_key(&format!("{} {s}", tracker_name(kind)))
                });
            }
            let answers = futures_util::future::join_all(
                scopes.iter().map(|s| self.scope_list(kind, s, query)),
            )
            .await;
            let mut failed: Option<String> = None;
            for (scope, answer) in scopes.iter().zip(answers) {
                match answer {
                    Ok(page) => {
                        any = true;
                        match page.counts {
                            Some(c) => {
                                totals = (
                                    totals.0.saturating_add(c.open),
                                    totals.1.saturating_add(c.closed),
                                )
                            }
                            None => counted = false,
                        }
                        if let Some(next) = page.next {
                            list.next_cursors
                                .insert(format!("{} {scope}", tracker_name(kind)), next);
                        }
                        list.issues.extend(page.issues);
                    }
                    Err(error) => {
                        if matches!(&error, CoreError::Invalid(_)) {
                            return Err(error);
                        }
                        counted = false;
                        failed = Some(error.public_message());
                    }
                }
            }
            if let Some(message) = failed {
                list.failed.push(IssueListFailure {
                    tracker: kind,
                    message,
                });
            }
        }
        list.issues.sort_by(|a, b| match query.sort {
            query::IssueSort::Updated => b.updated_at.cmp(&a.updated_at),
            query::IssueSort::Created => b.created_at.cmp(&a.created_at),
        });
        list.counts = (any && counted).then_some(IssueCounts {
            open: totals.0,
            closed: totals.1,
        });
        list.next_cursors = query::capped_cursors(std::mem::take(&mut list.next_cursors));
        Ok(list)
    }

    /// Creates one issue (docs/issues.md, "Create, comment, react") and answers it as the tracker stored it. One
    /// call creates one issue: a call whose outcome is unknown answers `tracker_unavailable` saying the issue was
    /// possibly created, and nothing here retries. A template id is looked up in the tracker's templates: a missing
    /// body is filled from the named template and an unknown id is `400 invalid`.
    pub async fn create_issue(&self, input: CreateIssue) -> Result<IssueDetail> {
        self.require_connected(input.tracker).await?;
        self.check_issue_scope(input.tracker, &input.scope).await?;
        let source = self.writing_source(input.tracker)?;
        let title = checked_title(&input.title)?;
        let provided_body = input.body.is_some();
        let mut body = input.body.unwrap_or_default();
        // A template fills a body the person did not send; a body they did send wins. The template's text is the
        // tracker's and as untrusted as any issue text (docs/issues.md, "Issue text is untrusted").
        if let Some(id) = &input.template {
            let template = self
                .issue_templates(input.tracker, &input.scope)
                .await?
                .into_iter()
                .find(|template| template.id == *id)
                .ok_or_else(|| CoreError::Invalid("Unknown issue template.".into()))?;
            if !provided_body {
                body = template.body;
            }
        }
        checked_body(&body, "The issue body")?;
        checked_labels(&input.labels)?;
        checked_assignees(&input.assignees)?;
        let created = source
            .create(
                &input.scope,
                &NewIssue {
                    title: &title,
                    body: &body,
                    labels: &input.labels,
                    assignees: &input.assignees,
                },
            )
            .await;
        if write_may_have_changed(&created) {
            self.issues.scope_changed(input.tracker, &input.scope);
        }
        created
    }

    /// Writes one comment on one issue and answers it as the tracker stored it. Editing or deleting a comment is
    /// not part of this route.
    pub async fn comment_issue(&self, input: CommentIssue) -> Result<IssueComment> {
        self.require_connected(input.tracker).await?;
        self.check_issue_scope(input.tracker, &input.scope).await?;
        let body = checked_comment(&input.body)?;
        let written = self
            .writing_source(input.tracker)?
            .comment(&input.scope, &input.key, body)
            .await;
        if write_may_have_changed(&written) {
            self.issues.scope_changed(input.tracker, &input.scope);
        }
        written
    }

    /// Adds or takes back one reaction and answers the issue with the tracker's reaction counts. Both directions are
    /// idempotent on the tracker; the same call twice answers the same issue. A reaction changes no field a listing
    /// carries and no order it sorts by, so the cached pages stay.
    pub async fn react_issue(&self, input: ReactIssue) -> Result<IssueDetail> {
        self.require_connected(input.tracker).await?;
        self.check_issue_scope(input.tracker, &input.scope).await?;
        if let Some(comment) = &input.comment_id {
            checked_id(comment, "comment")?;
        }
        let source = self.writing_source(input.tracker)?;
        let mut detail = source
            .react(
                &input.scope,
                &input.key,
                input.comment_id.as_deref(),
                input.reaction,
                input.on,
            )
            .await?;
        detail.linked_threads = self.store.threads_linking(&IssueKey {
            tracker: input.tracker,
            scope: input.scope,
            key: input.key,
        })?;
        Ok(detail)
    }

    /// Writes the present fields of one issue (docs/issues.md, "Issue writes and list controls") and answers the
    /// issue as the tracker stored it. A field that is absent is not written; `labels` and `assignees` are the
    /// whole new set, so an empty list clears them. A body with none of the four present is `400 invalid`, and
    /// every field is checked before the tracker is sent anything.
    pub async fn update_issue(&self, input: IssueUpdateInput) -> Result<IssueDetail> {
        self.require_connected(input.tracker).await?;
        self.check_issue_scope(input.tracker, &input.scope).await?;
        let title = input.title.as_deref().map(checked_title).transpose()?;
        if let Some(body) = &input.body {
            checked_body(body, "The issue body")?;
        }
        if let Some(labels) = &input.labels {
            checked_labels(labels)?;
        }
        if let Some(assignees) = &input.assignees {
            checked_assignees(assignees)?;
        }
        if title.is_none()
            && input.body.is_none()
            && input.labels.is_none()
            && input.assignees.is_none()
        {
            return Err(CoreError::Invalid(
                "Nothing to change: send a title, a body, labels or assignees.".into(),
            ));
        }
        let source = self.editing_source(input.tracker)?;
        let written = source
            .update(
                &input.scope,
                &input.key,
                &IssueEdit {
                    title: title.as_deref(),
                    body: input.body.as_deref(),
                    labels: input.labels.as_deref(),
                    assignees: input.assignees.as_deref(),
                },
            )
            .await;
        if write_may_have_changed(&written) {
            self.issues.scope_changed(input.tracker, &input.scope);
        }
        let mut detail = written?;
        detail.linked_threads = self.store.threads_linking(&IssueKey {
            tracker: input.tracker,
            scope: input.scope,
            key: input.key,
        })?;
        Ok(detail)
    }

    /// Moves one issue to one of the states its own detail declares and answers it as the tracker stored it
    /// (docs/issues.md, "Issue writes and list controls"). The detail is read first, so a name the issue does not
    /// offer and a `close_reason` on an open state are refused before anything is written.
    pub async fn set_state_issue(&self, input: IssueSetStateInput) -> Result<IssueDetail> {
        self.require_connected(input.tracker).await?;
        self.check_issue_scope(input.tracker, &input.scope).await?;
        checked_state(&input.state)?;
        let source = self.editing_source(input.tracker)?;
        let key = IssueKey {
            tracker: input.tracker,
            scope: input.scope.clone(),
            key: input.key.clone(),
        };
        let states = self.issue(&key).await?.states.unwrap_or_default();
        let wanted = states
            .iter()
            .find(|state| state.name == input.state)
            .ok_or_else(|| CoreError::Invalid("That state is not one this issue offers.".into()))?;
        let closed = wanted.category == IssueStateCategory::Closed;
        if !closed && input.close_reason.is_some() {
            return Err(CoreError::Invalid(
                "A close reason is only for a closed state.".into(),
            ));
        }
        // A closed state closes as `completed` unless the person said `not_planned`; a tracker that cannot say
        // `not_planned` answers what it stored.
        let close_reason =
            closed.then_some(input.close_reason.unwrap_or(IssueCloseReason::Completed));
        let written = source
            .set_state(&input.scope, &input.key, &wanted.name, close_reason)
            .await;
        if write_may_have_changed(&written) {
            self.issues.scope_changed(input.tracker, &input.scope);
        }
        let mut detail = written?;
        detail.linked_threads = self.store.threads_linking(&key)?;
        Ok(detail)
    }

    /// The labels and people a picker may offer for one scope (docs/issues.md, "Issue writes and list controls"):
    /// at most 200 of each, `truncated` when the tracker had more, so a missing name is not proof the person is
    /// ineligible. A scope outside the connection is refused as everywhere else.
    pub async fn issue_candidates(
        &self,
        tracker: TrackerKind,
        scope: &str,
    ) -> Result<IssueCandidates> {
        self.require_connected(tracker).await?;
        self.check_issue_scope(tracker, scope).await?;
        let mut candidates = self.editing_source(tracker)?.candidates(scope).await?;
        candidates.truncated |=
            candidates.labels.len() > MAX_CANDIDATES || candidates.assignees.len() > MAX_CANDIDATES;
        candidates.labels.truncate(MAX_CANDIDATES);
        candidates.assignees.truncate(MAX_CANDIDATES);
        Ok(candidates)
    }

    /// The tracker's own issue templates for one scope: a read, at most 50, each body cut to 16 KiB. A tracker
    /// whose API has no template source answers an empty list, never `404`.
    pub async fn issue_templates(
        &self,
        tracker: TrackerKind,
        scope: &str,
    ) -> Result<Vec<IssueTemplate>> {
        self.require_connected(tracker).await?;
        self.check_issue_scope(tracker, scope).await?;
        let mut templates = self.issue_source(tracker)?.templates(scope).await?;
        templates.truncate(MAX_TEMPLATES);
        for template in &mut templates {
            template.body = frame::cut(&template.body, MAX_TEMPLATE_BODY_BYTES).to_owned();
        }
        Ok(templates)
    }

    /// Binds a project to the connected tracker and scope its issues come from, or unbinds it (docs/issues.md,
    /// "Create, comment, react, templates and project binding"). `Ok(None)` is the unbind. A tracker that is not
    /// connected and a scope outside the connection are `400 invalid`; nothing about the connection, the project or
    /// any issue changes. A bind writes one audit row with ids and names only, never a token; unbinding writes none
    /// and answers the same way when there was nothing bound (it is idempotent).
    pub async fn bind_project_issue_tracker(
        &self,
        project_id: &str,
        input: SetProjectIssueTracker,
    ) -> Result<Option<ProjectIssueTracker>> {
        self.store.project(project_id)?;
        let Some(tracker) = input.tracker else {
            if input.scope.is_some() {
                return Err(CoreError::Invalid("An unbind names no scope.".into()));
            }
            self.store.clear_project_issue_tracker(project_id)?;
            return Ok(None);
        };
        let scope = input
            .scope
            .as_deref()
            .map(str::trim)
            .filter(|scope| !scope.is_empty())
            .ok_or_else(|| CoreError::Invalid("A binding needs a scope.".into()))?;
        if scope.len() > MAX_SCOPE_BYTES || scope.chars().any(char::is_control) {
            return Err(CoreError::Invalid(
                "The scope is too long or has control characters.".into(),
            ));
        }
        // A tracker that is not connected is the write route's 400, not the read routes' 409: the binding is refused
        // before anything is stored.
        let connected = self
            .issue_trackers()
            .await?
            .into_iter()
            .find(|row| row.tracker == tracker)
            .is_some_and(|row| row.connected);
        if !connected {
            return Err(CoreError::Invalid(format!(
                "{} is not connected.",
                display_name(tracker)
            )));
        }
        let connection = self
            .store
            .tracker_connections()?
            .into_iter()
            .find(|connection| connection.tracker == tracker)
            .unwrap_or(TrackerConnection {
                tracker,
                host: None,
                scopes: Vec::new(),
            });
        if !self
            .issue_scopes(&connection)
            .await?
            .iter()
            .any(|known| known == scope)
        {
            return Err(CoreError::Invalid(
                "That scope is not part of this connection.".into(),
            ));
        }
        // The audit row is written before the effect it describes (docs/architecture.md rule 6): an audit failure
        // leaves no binding behind.
        self.store.append_audit(&AuditRecord {
            actor: "user".into(),
            action: "issues.project.bind".into(),
            target: project_id.to_owned(),
            decision: None,
            source: "user".into(),
            thread_id: None,
            run_id: None,
            detail: serde_json::json!({
                "project": project_id,
                "tracker": tracker_name(tracker),
                "scope": scope,
            }),
        })?;
        self.store.set_project_issue_tracker(
            project_id,
            tracker_name(tracker),
            scope,
            &crate::util::now(),
        )?;
        // Answer from the row that was stored: the write is what the page reads back.
        let stored = self
            .store
            .project_issue_tracker(project_id)?
            .ok_or_else(|| {
                CoreError::Internal("the issue tracker binding was not stored".into())
            })?;
        let stored_tracker = tracker_of(&stored.tracker)
            .ok_or_else(|| CoreError::Internal("the stored issue tracker is unknown".into()))?;
        Ok(Some(ProjectIssueTracker {
            tracker: stored_tracker,
            scope: stored.scope,
        }))
    }

    /// The source that may write for this tracker. A tracker whose own write is not built yet answers the API's
    /// `422 unsupported` with one fixed sentence that names it (R15 PR1: GitHub only), and nothing is sent.
    fn writing_source(&self, kind: TrackerKind) -> Result<Arc<dyn IssueSource>> {
        let source = self.issue_source(kind)?;
        if source.can_write() {
            return Ok(source);
        }
        Err(CoreError::Unsupported(format!(
            "{} issue writes arrive in a later update.",
            display_name(kind)
        )))
    }

    /// The source that may edit an issue and offer candidates for this tracker. A tracker whose edit is not built
    /// answers the API's `422 unsupported` with one fixed sentence that names it, and nothing is sent.
    fn editing_source(&self, kind: TrackerKind) -> Result<Arc<dyn IssueSource>> {
        let source = self.issue_source(kind)?;
        if source.can_edit() {
            return Ok(source);
        }
        Err(CoreError::Unsupported(format!(
            "{} issue edits arrive in a later update.",
            display_name(kind)
        )))
    }

    /// One issue with its comments, the pull requests the tracker links and the threads that link it.
    pub async fn issue(&self, key: &IssueKey) -> Result<IssueDetail> {
        self.require_connected(key.tracker).await?;
        self.check_issue_scope(key.tracker, &key.scope).await?;
        let mut detail = self
            .issue_source(key.tracker)?
            .get(&key.scope, &key.key)
            .await?;
        detail.linked_threads = self.store.threads_linking(key)?;
        Ok(detail)
    }

    async fn require_connected(&self, kind: TrackerKind) -> Result<()> {
        let rows = self.issue_trackers().await?;
        match rows.iter().find(|r| r.tracker == kind) {
            Some(row) if row.connected => Ok(()),
            Some(row) => Err(CoreError::TrackerNotConnected(
                row.detail
                    .clone()
                    .unwrap_or_else(|| format!("{} is not connected.", display_name(kind))),
            )),
            None => Err(CoreError::TrackerNotConnected(
                "That tracker is not connected.".into(),
            )),
        }
    }

    /// A scope outside the connection is refused: nothing outside it is read or written.
    async fn check_issue_scope(&self, tracker: TrackerKind, scope: &str) -> Result<()> {
        let connection = self
            .store
            .tracker_connections()?
            .into_iter()
            .find(|c| c.tracker == tracker)
            .unwrap_or(TrackerConnection {
                tracker,
                host: None,
                scopes: Vec::new(),
            });
        if self
            .issue_scopes(&connection)
            .await?
            .iter()
            .any(|s| s == scope)
        {
            Ok(())
        } else {
            Err(CoreError::Invalid(
                "That scope is not part of this connection.".into(),
            ))
        }
    }

    fn issue_ref(summary: &IssueSummary) -> IssueRef {
        IssueRef {
            tracker: summary.tracker,
            scope: summary.scope.clone(),
            key: summary.key.clone(),
            display_key: summary.display_key.clone(),
            title: summary.title.clone(),
            state: summary.state.clone(),
            labels: summary.labels.clone(),
        }
    }

    /// Every thread's linked issues, oldest link first.
    pub fn issue_links(&self) -> Result<BTreeMap<String, Vec<IssueRef>>> {
        self.store.issue_links()
    }

    /// A number, words or a pasted issue link, searched over the connected trackers.
    pub async fn find_issues(&self, input: &str) -> Result<Vec<IssueRef>> {
        let input = input.trim();
        if input.is_empty() || input.len() > 300 || input.chars().any(char::is_control) {
            return Ok(Vec::new());
        }
        let number = input.trim_start_matches('#');
        if let Some(repo_and_key) = input.strip_prefix("https://github.com/").and_then(|rest| {
            let mut parts = rest.split('/');
            let (owner, repo, kind, key) =
                (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
            let key = key.split(['?', '#']).next()?.to_owned();
            (kind == "issues").then(|| (format!("{owner}/{repo}"), key))
        }) {
            let key = IssueKey {
                tracker: TrackerKind::Github,
                scope: repo_and_key.0,
                key: repo_and_key.1,
            };
            return Ok(self
                .issue(&key)
                .await
                .map(|d| vec![Self::issue_ref(&d.summary)])
                .unwrap_or_default());
        }
        if !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) {
            let mut found = Vec::new();
            let stored = self.store.tracker_connections()?;
            for row in self
                .issue_trackers()
                .await?
                .into_iter()
                .filter(|r| r.connected)
            {
                let connection = stored
                    .iter()
                    .find(|c| c.tracker == row.tracker)
                    .cloned()
                    .unwrap_or(TrackerConnection {
                        tracker: row.tracker,
                        host: None,
                        scopes: Vec::new(),
                    });
                for scope in self.issue_scopes(&connection).await.unwrap_or_default() {
                    let key = IssueKey {
                        tracker: row.tracker,
                        scope,
                        key: number.to_owned(),
                    };
                    if let Ok(detail) = self.issue(&key).await {
                        found.push(Self::issue_ref(&detail.summary));
                    }
                }
            }
            return Ok(found);
        }
        let query = IssueQuery {
            state: StateFilter::All,
            text: input.to_owned(),
            limit: 10,
            ..IssueQuery::default()
        };
        Ok(self
            .list_issues(&query)
            .await?
            .issues
            .iter()
            .map(Self::issue_ref)
            .collect())
    }

    /// Adds a link between a thread and an issue the tracker has.
    pub async fn link_issue(&self, thread_id: &str, key: &IssueKey) -> Result<()> {
        self.store.thread(thread_id)?;
        let detail = self.issue(key).await?;
        self.store.put_issue_link(
            thread_id,
            &Self::issue_ref(&detail.summary),
            &crate::util::now(),
        )
    }

    /// Removes the link from this thread only. The issue is untouched.
    pub fn unlink_issue(&self, thread_id: &str, key: &IssueKey) -> Result<()> {
        self.store.thread(thread_id)?;
        self.store.delete_issue_link(thread_id, key)
    }

    /// The framed first message for a thread started from this issue.
    pub async fn issue_draft(&self, key: &IssueKey) -> Result<IssueDraft> {
        let detail = self.issue(key).await?;
        Ok(IssueDraft {
            message: frame::draft(&detail.summary, &detail.body),
        })
    }

    /// Creates the thread, links it to the issue and queues the person's message, in one write.
    pub async fn start_issue_thread(self: &Arc<Self>, input: StartIssueThread) -> Result<Thread> {
        let message = input.message.trim();
        if message.is_empty() {
            return Err(CoreError::Invalid("The first message is empty.".into()));
        }
        let key = IssueKey {
            tracker: input.tracker,
            scope: input.scope.clone(),
            key: input.key.clone(),
        };
        let detail = self.issue(&key).await?;
        self.store.project(&input.project_id)?;
        let engine = self
            .preference("default_engine", Some(&input.project_id))?
            .as_str()
            .filter(|e| !e.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| CoreError::Invalid("Choose an engine in Settings first.".into()))?;
        let thread = self
            .create_thread(CreateThread {
                project_id: Some(input.project_id.clone()),
                engine,
                model: None,
                title: Some(
                    detail
                        .summary
                        .title
                        .chars()
                        .take(120)
                        .collect::<String>()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" "),
                ),
                worktree: None,
                skill_profile: None,
                base_branch: None,
                effort: None,
                options: BTreeMap::new(),
                branch_name_seed: None,
            })
            .await?;
        if let Err(error) = self.store.put_issue_link(
            &thread.id,
            &Self::issue_ref(&detail.summary),
            &crate::util::now(),
        ) {
            let _ = self.delete_thread(&thread.id, "issues");
            return Err(error);
        }
        if let Err(error) = self.runs.enqueue(&thread.id, message, &[]) {
            let _ = self.delete_thread(&thread.id, "issues");
            return Err(error);
        }
        Ok(thread)
    }
}

/// An issue title as the contract takes it: one line, 1 to 256 characters, trimmed.
fn checked_title(input: &str) -> Result<String> {
    let title = input.trim();
    if title.is_empty() {
        return Err(CoreError::Invalid("The issue needs a title.".into()));
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(CoreError::Invalid(
            "The title is over 256 characters.".into(),
        ));
    }
    if title.chars().any(char::is_control) {
        return Err(CoreError::Invalid("A title is one line.".into()));
    }
    Ok(title.to_owned())
}

/// An issue body or a comment as the contract takes it: at most 64 KiB of bytes.
fn checked_body(input: &str, what: &str) -> Result<()> {
    if input.len() > MAX_BODY_BYTES {
        return Err(CoreError::Invalid(format!("{what} is over 64 KiB.")));
    }
    Ok(())
}

/// A comment is 1 to 64 KiB and not blank: the tracker's own text, sent as a field value.
fn checked_comment(input: &str) -> Result<&str> {
    checked_body(input, "The comment")?;
    if input.trim().is_empty() {
        return Err(CoreError::Invalid("The comment is empty.".into()));
    }
    Ok(input)
}

/// Label names, at most 50: each 1 to 200 characters and no control characters.
fn checked_labels(labels: &[String]) -> Result<()> {
    if labels.len() > MAX_LABELS {
        return Err(CoreError::Invalid("You can set at most 50 labels.".into()));
    }
    if labels.iter().any(|label| {
        label.is_empty()
            || label.chars().count() > MAX_LABEL_CHARS
            || label.chars().any(char::is_control)
    }) {
        return Err(CoreError::Invalid(
            "A label name is empty, too long or has control characters.".into(),
        ));
    }
    Ok(())
}

/// Candidate ids, at most 25: each 1 to 200 characters and no control characters. An id is how the tracker
/// addresses a person on a write, and the tracker refuses one it does not know.
fn checked_assignees(assignees: &[String]) -> Result<()> {
    if assignees.len() > MAX_ASSIGNEES {
        return Err(CoreError::Invalid(
            "You can assign at most 25 people.".into(),
        ));
    }
    for assignee in assignees {
        checked_id(assignee, "assignee")?;
    }
    Ok(())
}

/// A tracker id the person sent: non-empty, at most 200 characters, no control characters. It is a value the tracker
/// refuses when it does not know it, never part of a path or an argument.
fn checked_id(id: &str, what: &str) -> Result<()> {
    if id.is_empty() || id.chars().count() > MAX_ID_CHARS || id.chars().any(char::is_control) {
        return Err(CoreError::Invalid(format!("That {what} id is not valid.")));
    }
    Ok(())
}

/// A state name the person picked: 1 to 200 characters, no control characters. Whether the issue offers it is
/// checked against the detail's own `states` before the tracker is sent anything.
fn checked_state(state: &str) -> Result<()> {
    let state = state.trim();
    if state.is_empty() {
        return Err(CoreError::Invalid("A state needs a name.".into()));
    }
    if state.chars().count() > MAX_LABEL_CHARS || state.chars().any(char::is_control) {
        return Err(CoreError::Invalid(
            "That state name is too long or has control characters.".into(),
        ));
    }
    Ok(())
}

pub(crate) fn display_name(kind: TrackerKind) -> &'static str {
    match kind {
        TrackerKind::Github => "GitHub",
        TrackerKind::Gitlab => "GitLab",
        TrackerKind::AzureDevops => "Azure DevOps",
        TrackerKind::Forgejo => "Forgejo",
        TrackerKind::Linear => "Linear",
    }
}

#[cfg(test)]
mod tests;
