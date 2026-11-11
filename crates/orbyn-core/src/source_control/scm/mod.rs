//! Source control on a thread's checkout. Forge work goes through the user's own
//! `gh` or `glab`, so their login stays theirs and Orbyn holds no credential. All
//! of it needs a trusted folder, and commit hooks do run here, as they would for
//! the user. Writes wait for the thread's run to finish.

mod forges;
mod pulls;
pub mod read_cache;
mod review;
mod stack;
mod watch;
pub(crate) mod writing;

use std::path::Path;
use std::time::Duration;

use orbyn_contracts::{
    BranchInfo, CreateBranch, GitAction, GitActionResult, GitPhase, PullRequest, PullRequestDraft,
    Thread,
};
use serde_json::{Value, json};

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::source_control::forge::{create_pull, fetch_pull, forge_of, number_from_url};
use crate::source_control::git::{git, text};
use crate::store::AuditRecord;
use crate::util::tail;

const FORGE_TIMEOUT: Duration = Duration::from_secs(120);
const PUSH_TIMEOUT: Duration = Duration::from_secs(300);

/// Runs git for a real user action: hooks stay on, and nothing may prompt.
pub(crate) async fn git_user(cwd: &Path, args: &[&str], timeout: Duration) -> Result<String> {
    let mut command = crate::source_control::git::command(cwd);
    command.args(args).env("GIT_TERMINAL_PROMPT", "0");
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| CoreError::Git("Git did not finish in time.".into()))?
        .map_err(|error| CoreError::Git(format!("Could not run git: {error}")))?;
    if !output.status.success() {
        return Err(CoreError::Git(tail(
            &String::from_utf8_lossy(&output.stderr),
            1024,
        )));
    }
    Ok(text(&output.stdout))
}

fn phase(name: &str, outcome: Result<Option<String>>) -> GitPhase {
    match outcome {
        Ok(url) => GitPhase {
            name: name.to_owned(),
            ok: true,
            message: None,
            url,
        },
        Err(error) => GitPhase {
            name: name.to_owned(),
            ok: false,
            message: Some(error.public_message()),
            url: None,
        },
    }
}

async fn valid_branch(cwd: &Path, name: &str) -> Result<()> {
    git(cwd, &["check-ref-format", "--branch", name])
        .await
        .map(|_| ())
        .map_err(|_| CoreError::Invalid("That is not a valid branch name.".into()))
}

impl Core {
    fn scm_thread(&self, id: &str) -> Result<Thread> {
        let thread = self.store.thread(id)?;
        if !self.store.project(&thread.project_id)?.trusted {
            return Err(CoreError::FolderUntrusted);
        }
        Ok(thread)
    }

    /// Git writes would race an engine in the same folder.
    pub(crate) fn scm_idle(&self, id: &str) -> Result<Thread> {
        let thread = self.scm_thread(id)?;
        if thread.active_run.is_some() {
            return Err(CoreError::ThreadBusy);
        }
        Ok(thread)
    }

    fn scm_audit(&self, actor: &str, action: &str, target: &str, id: Option<&str>) -> Result<()> {
        self.store.append_audit(&AuditRecord {
            actor: actor.to_owned(),
            action: action.to_owned(),
            target: target.to_owned(),
            decision: None,
            source: "user".into(),
            thread_id: id.map(str::to_owned),
            run_id: None,
            detail: Value::Null,
        })?;
        Ok(())
    }

    /// Commits, pushes and opens a pull request, stopping at the first failure.
    /// Each phase is logged, so a client can show progress as it happens.
    pub async fn git_action(
        &self,
        id: &str,
        action: &GitAction,
        actor: &str,
    ) -> Result<GitActionResult> {
        let thread = self.scm_idle(id)?;
        let prepared = self.prepare_git_writing(&thread, action, actor).await?;
        let fresh = self.scm_idle(id)?;
        if fresh.cwd != thread.cwd
            || fresh.engine != thread.engine
            || fresh.project_id != thread.project_id
            || fresh.model != thread.model
        {
            return Err(CoreError::Invalid(
                "The thread changed. Review the action and try again.".into(),
            ));
        }
        let cwd = Path::new(&thread.cwd);
        prepared.check(cwd).await?;
        let action = &prepared.action;
        let mut phases = Vec::new();
        if let Some(message) = &action.commit_message {
            if message.trim().is_empty() {
                return Err(CoreError::Invalid("Enter a commit message.".into()));
            }
            self.scm_audit(actor, "git.commit", message, Some(id))?;
            let outcome = async {
                self.scm_idle(id)?;
                git_user(cwd, &["add", "-A"], FORGE_TIMEOUT).await?;
                prepared.check(cwd).await?;
                self.scm_idle(id)?;
                git_user(cwd, &["commit", "-q", "-m", message], FORGE_TIMEOUT).await?;
                Ok(None)
            }
            .await;
            if !self.record_phase(id, &mut phases, phase("commit", outcome))? {
                return Ok(GitActionResult {
                    phases,
                    notice: prepared.notice.clone(),
                });
            }
        }
        let branch = text(&git(cwd, &["branch", "--show-current"]).await?);
        if action.push {
            self.scm_audit(actor, "git.push", &branch, Some(id))?;
            let outcome = git_user(cwd, &["push", "-u", "origin", &branch], PUSH_TIMEOUT)
                .await
                .map(|_| None);
            if !self.record_phase(id, &mut phases, phase("push", outcome))? {
                return Ok(GitActionResult {
                    phases,
                    notice: prepared.notice.clone(),
                });
            }
        }
        if let Some(draft) = &action.pull_request {
            self.scm_audit(actor, "git.pull_request", &draft.title, Some(id))?;
            let outcome = self.open_pull(id, cwd, &branch, draft).await.map(Some);
            self.record_phase(id, &mut phases, phase("pull_request", outcome))?;
        }
        Ok(GitActionResult {
            phases,
            notice: prepared.notice.clone(),
        })
    }

    async fn open_pull(
        &self,
        id: &str,
        cwd: &Path,
        branch: &str,
        draft: &PullRequestDraft,
    ) -> Result<String> {
        let forge = forge_of(cwd).await?;
        let url = create_pull(
            forge,
            cwd,
            branch,
            &draft.title,
            &draft.body,
            draft.base.as_deref(),
        )
        .await?;
        self.scm_reads.lists_changed();
        // The pull request exists now; a failed lookup must not report otherwise.
        match number_from_url(&url) {
            Some(number) => {
                let pull = fetch_pull(forge, cwd, number).await.unwrap_or(PullRequest {
                    number,
                    title: draft.title.clone(),
                    url: url.clone(),
                    state: "open".into(),
                    head: branch.to_owned(),
                    base: draft.base.clone().unwrap_or_default(),
                    author: None,
                    draft: false,
                    updated_at: None,
                    labels: Vec::new(),
                    review_state: None,
                    checks_state: None,
                    checks: None,
                });
                self.store.link_pull(id, &pull)?;
            }
            None => tracing::warn!("the forge returned no pull request number"),
        }
        Ok(url)
    }

    fn record_phase(&self, id: &str, phases: &mut Vec<GitPhase>, phase: GitPhase) -> Result<bool> {
        let ok = phase.ok;
        self.store
            .note_thread_event(id, "git.phase", json!(phase))?;
        phases.push(phase);
        Ok(ok)
    }

    pub async fn branches(&self, id: &str) -> Result<Vec<BranchInfo>> {
        let thread = self.scm_thread(id)?;
        let cwd = Path::new(&thread.cwd);
        let current = text(&git(cwd, &["branch", "--show-current"]).await?);
        let refs = git(
            cwd,
            &[
                "for-each-ref",
                "--format=%(refname)",
                "refs/heads",
                "refs/remotes",
            ],
        )
        .await?;
        Ok(String::from_utf8_lossy(&refs)
            .lines()
            .filter(|name| !name.ends_with("/HEAD"))
            .filter_map(|name| {
                let (name, remote) = match name.strip_prefix("refs/heads/") {
                    Some(local) => (local, false),
                    None => (name.strip_prefix("refs/remotes/")?, true),
                };
                Some(BranchInfo {
                    current: !remote && name == current,
                    name: name.to_owned(),
                    remote,
                })
            })
            .collect())
    }

    pub async fn create_branch(&self, id: &str, request: &CreateBranch, actor: &str) -> Result<()> {
        let thread = self.scm_idle(id)?;
        let cwd = Path::new(&thread.cwd);
        valid_branch(cwd, &request.name).await?;
        self.scm_audit(actor, "git.branch", &request.name, Some(id))?;
        let mut args = vec!["switch", "-q", "-c", request.name.as_str()];
        if let Some(start) = &request.start {
            if start.starts_with('-') {
                return Err(CoreError::Invalid(
                    "That is not a valid start point.".into(),
                ));
            }
            args.push(start);
        }
        git_user(cwd, &args, FORGE_TIMEOUT).await.map(|_| ())
    }

    /// Git itself refuses a switch that would overwrite local changes.
    pub async fn switch_branch(&self, id: &str, name: &str, actor: &str) -> Result<()> {
        let thread = self.scm_idle(id)?;
        let cwd = Path::new(&thread.cwd);
        valid_branch(cwd, name).await?;
        self.scm_audit(actor, "git.switch", name, Some(id))?;
        git_user(cwd, &["switch", "-q", name], FORGE_TIMEOUT)
            .await
            .map(|_| ())
    }
}

fn check_state(state: &str) -> Result<()> {
    if matches!(state, "open" | "closed" | "merged" | "all") {
        Ok(())
    } else {
        Err(CoreError::Invalid("Unknown pull request state.".into()))
    }
}

/// Comment and thread ids go into API paths, so only plain characters pass.
fn check_opaque(id: &str) -> Result<()> {
    let plain = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '='));
    if plain {
        Ok(())
    } else {
        Err(CoreError::Invalid("That id is not valid.".into()))
    }
}

#[cfg(test)]
mod tests;
