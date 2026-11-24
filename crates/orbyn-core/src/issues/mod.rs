//! Issue trackers beside the code (docs/issues.md): the list a tracker reports, the connections that read it,
//! a thread's links to issues and a thread started from one. Beta, read-only against the tracker. Everything a
//! tracker returns is untrusted text and is never fed to an engine except in the framed first message.

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
use crate::store::{TrackerConnection, tracker_name};
use async_trait::async_trait;
use orbyn_contracts::{
    ConnectTracker, CreateThread, IssueCounts, IssueDetail, IssueDraft, IssueKey, IssueList,
    IssueListFailure, IssueRef, IssueSummary, Project, StartIssueThread, Thread, TrackerKind,
    TrackerRow,
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
}

pub(crate) struct IssueDesk {
    credentials: Mutex<Credentials>,
    sources: Mutex<HashMap<TrackerKind, Arc<dyn IssueSource>>>,
    cache: Mutex<HashMap<String, (Instant, ScopeList)>>,
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

impl IssueDesk {
    pub(crate) fn new(credentials: Credentials) -> Self {
        Self {
            credentials: Mutex::new(credentials),
            sources: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::new()),
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

    /// Drops the 30-second listing cache. Tests call it between passes when a fake tracker changes its answer inside
    /// the cache window; no production path needs it, because a listener's interval is 60 seconds at least.
    #[cfg(test)]
    pub(crate) fn clear_cache(&self) {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
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
        self.issues
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
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
        self.issues
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
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
        let id = format!("{} {scope} {query:?}", tracker_name(kind));
        if let Some((at, hit)) = self
            .issues
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&id)
            && at.elapsed() < CACHE_FOR
        {
            return Ok(hit.clone());
        }
        let fresh = self.issue_source(kind)?.list(scope, query).await?;
        self.issues
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, (Instant::now(), fresh.clone()));
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
        list.issues.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        list.counts = (any && counted).then_some(IssueCounts {
            open: totals.0,
            closed: totals.1,
        });
        list.next_cursors = query::capped_cursors(std::mem::take(&mut list.next_cursors));
        Ok(list)
    }

    /// One issue with its comments, the pull requests the tracker links and the threads that link it.
    pub async fn issue(&self, key: &IssueKey) -> Result<IssueDetail> {
        self.require_connected(key.tracker).await?;
        self.check_issue_scope(key).await?;
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

    /// A scope outside the connection is refused: nothing outside it is read.
    async fn check_issue_scope(&self, key: &IssueKey) -> Result<()> {
        let connection = self
            .store
            .tracker_connections()?
            .into_iter()
            .find(|c| c.tracker == key.tracker)
            .unwrap_or(TrackerConnection {
                tracker: key.tracker,
                host: None,
                scopes: Vec::new(),
            });
        if self.issue_scopes(&connection).await?.contains(&key.scope) {
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
