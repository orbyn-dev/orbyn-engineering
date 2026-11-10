//! Reading and linking pull requests for a thread or a project: lists, details, checks, conversation and files.

use super::check_state;
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::source_control::forge::{
    self, DETAIL_FIELDS, DETAILS_ROW_LIMIT, DetailsOutcome, Forge, PullFilter, fetch_pull,
    forge_of, no_such_pull,
};
use crate::source_control::git::{git, text};
use orbyn_contracts::{
    MergePull, Project, PullCheck, PullConversation, PullDetails, PullDetailsRequest,
    PullDetailsResponse, PullFile, PullList, PullListEntry, PullListFailure, PullRequest,
    PullRequestDetail, PullRevision, PullTimelineEvent, SubmitReview,
};
use std::path::{Path, PathBuf};
use std::time::Instant;

impl Core {
    pub async fn link_pull(&self, id: &str, number: u64) -> Result<PullRequest> {
        let thread = self.scm_thread(id)?;
        let cwd = Path::new(&thread.cwd);
        let forge = forge_of(cwd).await?;
        // The forge's own "no such pull request" answer is a 404; every other failure stays as it is.
        let pull = fetch_pull(forge, cwd, number)
            .await
            .map_err(|error| match error {
                CoreError::Git(message) if no_such_pull(forge, &message) => {
                    CoreError::NotFound("pull request".into())
                }
                other => other,
            })?;
        self.store.link_pull(id, &pull)?;
        Ok(pull)
    }

    /// Reads each linked pull request again. A merge settles the thread when the
    /// rules say so; a thread still running or opted out stays as it is.
    pub async fn refresh_pulls(&self, id: &str) -> Result<Vec<PullRequest>> {
        let thread = self.scm_thread(id)?;
        let cwd = Path::new(&thread.cwd);
        let forge = forge_of(cwd).await?;
        self.link_branch_pull(id, forge, cwd).await;
        let mut merged = false;
        for pull in self.store.linked_pulls(id)? {
            if pull.state != "open" {
                continue;
            }
            let fresh = fetch_pull(forge, cwd, pull.number).await?;
            merged |= fresh.state == "merged";
            self.store.link_pull(id, &fresh)?;
        }
        let on_merge = self.preference("auto_settle_on_merge", Some(&thread.project_id))?
            == serde_json::json!(true);
        let opted_out = thread.lifecycle.as_ref().and_then(|l| l.auto_settle) == Some(false);
        if merged && on_merge && !opted_out && thread.active_run.is_none() {
            let settle = orbyn_contracts::UpdateThread {
                settled: Some(true),
                ..Default::default()
            };
            self.update_thread(id, &settle, "system:auto-settle")?;
        }
        self.store.linked_pulls(id)
    }

    /// Links the open pull request of the checkout's branch, if there is one.
    pub(super) async fn link_branch_pull(&self, id: &str, forge: Forge, cwd: &Path) {
        let Ok(branch) = git(cwd, &["branch", "--show-current"]).await else {
            return;
        };
        let branch = text(&branch);
        if branch.is_empty() {
            return;
        }
        let filter = PullFilter {
            state: "open".into(),
            head: Some(branch),
            ..PullFilter::default()
        };
        let linked = self.store.linked_pulls(id).unwrap_or_default();
        if let Ok(pulls) = forge::list_pulls(forge, cwd, &filter).await
            && let Some(pull) = pulls.first()
            && pull.state == "open"
            && !linked.iter().any(|known| known.number == pull.number)
            && let Err(error) = self.store.link_pull(id, pull)
        {
            tracing::debug!(%error, "could not link the branch's pull request");
        }
    }

    pub async fn pull_detail(&self, id: &str, number: u64) -> Result<PullRequestDetail> {
        let thread = self.scm_thread(id)?;
        let cwd = Path::new(&thread.cwd);
        forge::pull_detail(forge_of(cwd).await?, cwd, number).await
    }

    pub async fn review_pull(
        &self,
        id: &str,
        number: u64,
        review: &SubmitReview,
        actor: &str,
    ) -> Result<()> {
        let thread = self.scm_thread(id)?;
        let cwd = Path::new(&thread.cwd);
        let forge = forge_of(cwd).await?;
        self.scm_audit(actor, "git.review", &number.to_string(), Some(id))?;
        forge::submit_review(forge, cwd, number, review).await
    }

    pub async fn merge_pull(
        &self,
        id: &str,
        number: u64,
        options: &MergePull,
        actor: &str,
    ) -> Result<()> {
        let thread = self.scm_thread(id)?;
        let cwd = Path::new(&thread.cwd);
        let forge = forge_of(cwd).await?;
        self.scm_audit(actor, "git.merge", &number.to_string(), Some(id))?;
        forge::merge(forge, cwd, number, options).await?;
        self.refresh_pulls(id).await.map(|_| ())
    }

    /// A trusted project's folder and forge.
    pub(super) async fn pull_site(&self, project_id: &str) -> Result<(Project, Forge)> {
        let project = self.store.project(project_id)?;
        if !project.trusted {
            return Err(CoreError::FolderUntrusted);
        }
        let forge = forge_of(Path::new(&project.path)).await?;
        Ok((project, forge))
    }

    pub async fn project_pulls(
        &self,
        project_id: &str,
        filter: &PullFilter,
    ) -> Result<Vec<PullRequest>> {
        check_state(&filter.state)?;
        let (project, forge) = self.pull_site(project_id).await?;
        forge::list_pulls(forge, Path::new(&project.path), filter).await
    }

    /// Pull requests across every trusted project with a remote. A project that
    /// cannot be read is named, so a partial list is never mistaken for a whole one.
    pub async fn all_pulls(&self, filter: &PullFilter) -> Result<PullList> {
        check_state(&filter.state)?;
        let mut list = PullList {
            pulls: Vec::new(),
            failed: Vec::new(),
        };
        for project in self.store.list_projects()? {
            if !project.trusted || !project.is_git {
                continue;
            }
            let cwd = PathBuf::from(&project.path);
            if git(&cwd, &["remote", "get-url", "origin"]).await.is_err() {
                continue;
            }
            match self.project_pulls(&project.id, filter).await {
                Ok(pulls) => list
                    .pulls
                    .extend(pulls.into_iter().map(|pull| PullListEntry {
                        project_id: project.id.clone(),
                        pull,
                    })),
                Err(error) => list.failed.push(PullListFailure {
                    project_id: project.id,
                    message: error.public_message(),
                }),
            }
        }
        Ok(list)
    }

    /// What the base list leaves out, for pull requests the caller already shows. GitHub
    /// answers one batched request per project, and only for rows whose details are not
    /// already known (an unchanged `updated_at`, for at most ten minutes) unless the
    /// request is `fresh`. Repeated rows count once, and rows past the first 100 are
    /// ignored and so have no details. A project that cannot be read, a failed batch or a forge
    /// without details leaves those rows out whole. A spent rate limit pauses details for
    /// two minutes and leaves every row without them.
    pub async fn pull_details(&self, request: &PullDetailsRequest) -> Result<PullDetailsResponse> {
        let mut projects: Vec<(&str, Vec<&orbyn_contracts::PullDetailsRef>)> = Vec::new();
        let mut rows = 0;
        for pull in &request.pulls {
            let repeated = projects.iter().any(|(id, refs)| {
                *id == pull.project_id && refs.iter().any(|r| r.number == pull.number)
            });
            if repeated {
                continue;
            }
            if rows == DETAILS_ROW_LIMIT {
                break;
            }
            rows += 1;
            match projects.iter_mut().find(|(id, _)| *id == pull.project_id) {
                Some((_, refs)) => refs.push(pull),
                None => projects.push((pull.project_id.as_str(), vec![pull])),
            }
        }
        // `left` is read by the caller, which holds the cache's lock: taking it here would wait on itself.
        let paused = |left: Option<std::time::Duration>| PullDetailsResponse {
            details: Vec::new(),
            paused: DETAIL_FIELDS
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
            resume_at: left.and_then(|left| {
                let at = time::OffsetDateTime::now_utc() + left;
                at.format(&time::format_description::well_known::Rfc3339)
                    .ok()
            }),
        };
        let mut found: Vec<PullDetails> = Vec::new();
        for (project_id, refs) in projects {
            let Ok((project, forge)) = self.pull_site(project_id).await else {
                continue;
            };
            if forge != Forge::GitHub {
                continue;
            }
            let now = Instant::now();
            let mut known = Vec::new();
            let mut missing = Vec::new();
            {
                let cache = self.pull_details.lock().unwrap();
                if cache.paused(now) {
                    return Ok(paused(cache.resumes_in(now)));
                }
                for pull in refs {
                    let cached = if request.fresh {
                        None
                    } else {
                        cache.lookup(project_id, pull.number, pull.updated_at.as_deref(), now)
                    };
                    match cached {
                        Some(details) => known.push(details),
                        None => missing.push(pull),
                    }
                }
            }
            if !missing.is_empty() {
                let numbers: Vec<u64> = missing.iter().map(|pull| pull.number).collect();
                let cwd = Path::new(&project.path);
                match forge::github_details(forge, cwd, project_id, &numbers).await {
                    DetailsOutcome::Details(mut asked) => {
                        let now = Instant::now();
                        let mut cache = self.pull_details.lock().unwrap();
                        for pull in missing {
                            if let Some(details) = asked.remove(&pull.number) {
                                cache.store(details.clone(), pull.updated_at.as_deref(), now);
                                known.push(details);
                            }
                        }
                    }
                    DetailsOutcome::RateLimited => {
                        // GitHub's own reset time, when it will say; else the fixed pause.
                        let reset = forge::github_graphql_reset(cwd).await.map(|at| {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map_or(0, |since| since.as_secs());
                            std::time::Duration::from_secs(at.saturating_sub(now))
                        });
                        let left = {
                            let mut cache = self.pull_details.lock().unwrap();
                            match reset {
                                Some(span) => cache.pause_for(Instant::now(), span),
                                None => cache.pause(Instant::now()),
                            }
                            cache.resumes_in(Instant::now())
                        };
                        return Ok(paused(left));
                    }
                    // Nothing new; the cached rows below still stand.
                    DetailsOutcome::Unavailable => {}
                }
            }
            found.extend(known);
        }
        Ok(PullDetailsResponse {
            details: found,
            paused: Vec::new(),
            resume_at: None,
        })
    }

    pub async fn project_pull(&self, project_id: &str, number: u64) -> Result<PullRequestDetail> {
        let (project, forge) = self.pull_site(project_id).await?;
        forge::pull_detail(forge, Path::new(&project.path), number).await
    }

    pub async fn pull_checks(&self, project_id: &str, number: u64) -> Result<Vec<PullCheck>> {
        let (project, forge) = self.pull_site(project_id).await?;
        forge::pull_checks(forge, Path::new(&project.path), number).await
    }

    pub async fn approve_workflow(&self, project_id: &str, run: u64, actor: &str) -> Result<()> {
        let (project, forge) = self.pull_site(project_id).await?;
        self.scm_audit(actor, "git.workflow_approve", &run.to_string(), None)?;
        forge::approve_workflow(forge, Path::new(&project.path), run).await
    }

    pub async fn pull_conversation(
        &self,
        project_id: &str,
        number: u64,
    ) -> Result<PullConversation> {
        let (project, forge) = self.pull_site(project_id).await?;
        forge::conversation(forge, Path::new(&project.path), number).await
    }

    pub async fn pull_timeline(
        &self,
        project_id: &str,
        number: u64,
    ) -> Result<Vec<PullTimelineEvent>> {
        let (project, forge) = self.pull_site(project_id).await?;
        forge::timeline(forge, Path::new(&project.path), number).await
    }

    /// Changed files, with this service's viewed marks.
    pub async fn pull_files(&self, project_id: &str, number: u64) -> Result<Vec<PullFile>> {
        let (project, forge) = self.pull_site(project_id).await?;
        let viewed = self.store.viewed_files(project_id, number)?;
        let mut files = forge::pull_files(forge, Path::new(&project.path), number).await?;
        for file in &mut files {
            file.viewed = viewed.contains(&file.path);
        }
        Ok(files)
    }

    pub async fn pull_revisions(&self, project_id: &str, number: u64) -> Result<Vec<PullRevision>> {
        let (project, forge) = self.pull_site(project_id).await?;
        forge::pull_revisions(forge, Path::new(&project.path), number).await
    }

    /// One commit's files. Viewed marks belong to the whole pull request, not a commit.
    pub async fn revision_files(&self, project_id: &str, sha: &str) -> Result<Vec<PullFile>> {
        let (project, forge) = self.pull_site(project_id).await?;
        forge::revision_files(forge, Path::new(&project.path), sha).await
    }
}
