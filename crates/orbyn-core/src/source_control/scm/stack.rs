//! Stacked pull requests: finding a stack, acting on it, and checking out or working on a pull request.

use super::{FORGE_TIMEOUT, PUSH_TIMEOUT, git_user};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::source_control::forge::{self, Forge, PullFilter, fetch_pull};
use crate::source_control::git::{git, text};
use orbyn_contracts::{
    CheckoutPull, MergePull, PullBulkResult, PullCheckout, PullRequest, PullStack, StackAction,
    Thread, WorkOnPull,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub(super) fn pull_ref(forge: Forge) -> &'static str {
    match forge {
        Forge::GitLab => "merge-requests",
        _ => "pull",
    }
}

pub(super) fn stack_of(pulls: &[PullRequest], number: u64) -> Option<PullStack> {
    let start = pulls.iter().find(|pull| pull.number == number)?;
    let by_head: HashMap<&str, &PullRequest> = pulls
        .iter()
        .map(|pull| (pull.head.as_str(), pull))
        .collect();
    let mut layers = vec![start.clone()];
    let mut below = by_head.get(start.base.as_str());
    while let Some(pull) = below {
        if layers.iter().any(|layer| layer.number == pull.number) {
            break;
        }
        layers.insert(0, (*pull).clone());
        below = by_head.get(pull.base.as_str());
    }
    let mut top = start.clone();
    while let Some(next) = pulls.iter().find(|pull| {
        pull.base == top.head && !layers.iter().any(|layer| layer.number == pull.number)
    }) {
        layers.push(next.clone());
        top = next.clone();
    }
    let position = layers.iter().position(|pull| pull.number == number)?;
    Some(PullStack { layers, position })
}

impl Core {
    /// The chain of open pull requests through `number`: each layer's base is the
    /// head of the one below it.
    pub async fn pull_stack(&self, project_id: &str, number: u64) -> Result<PullStack> {
        let filter = PullFilter {
            state: "open".into(),
            ..PullFilter::default()
        };
        let pulls = self.project_pulls(project_id, &filter).await?;
        stack_of(&pulls, number).ok_or_else(|| CoreError::NotFound("pull request".into()))
    }

    /// Merges from the bottom, or rebases each layer on the one below, stopping at
    /// the first failure. Earlier layers stand.
    pub async fn stack_action(
        &self,
        project_id: &str,
        number: u64,
        action: &StackAction,
        actor: &str,
    ) -> Result<Vec<PullBulkResult>> {
        if !matches!(action.action.as_str(), "merge" | "rebase") {
            return Err(CoreError::Invalid("Choose merge or rebase.".into()));
        }
        let stack = self.pull_stack(project_id, number).await?;
        let mut results = Vec::new();
        for layer in &stack.layers {
            let outcome = if action.action == "merge" {
                let options = MergePull {
                    method: action.method.clone().unwrap_or_else(|| "merge".to_owned()),
                    auto: false,
                    delete_branch: false,
                    subject: None,
                    body: None,
                    strip_credits: false,
                    expected_head: None,
                };
                self.merge_project_pull(project_id, layer.number, &options, actor)
                    .await
            } else {
                self.pull_action(project_id, layer.number, "rebase", actor)
                    .await
            };
            let failed = outcome.is_err();
            results.push(PullBulkResult {
                number: layer.number,
                ok: !failed,
                message: outcome.err().map(|error| error.public_message()),
            });
            if failed {
                break;
            }
        }
        Ok(results)
    }

    /// Checks a pull request out, in place or in a worktree of its own, and warns
    /// when the local head is not the forge's latest.
    pub async fn checkout_pull(
        &self,
        project_id: &str,
        number: u64,
        request: &CheckoutPull,
        actor: &str,
    ) -> Result<PullCheckout> {
        let (project, forge) = self.pull_site(project_id).await?;
        let cwd = PathBuf::from(&project.path);
        let detail = forge::pull_detail(forge, &cwd, number).await?;
        let head = detail.pull.head.clone();
        git(&cwd, &["check-ref-format", "--branch", &head])
            .await
            .map_err(|_| CoreError::Git("The pull request's branch name is not valid.".into()))?;
        self.scm_audit(actor, "git.pull_checkout", &number.to_string(), None)?;
        // Git won't move a branch a checkout is on; the warning below says if it's behind.
        let worktrees = git(&cwd, &["worktree", "list", "--porcelain"]).await?;
        let held = format!("branch refs/heads/{head}");
        if !String::from_utf8_lossy(&worktrees)
            .lines()
            .any(|line| line == held)
        {
            let spec = format!("+refs/{}/{number}/head:refs/heads/{head}", pull_ref(forge));
            let fetched = git_user(&cwd, &["fetch", "origin", &spec], PUSH_TIMEOUT).await;
            if fetched.is_err() {
                let spec = format!("+refs/heads/{head}:refs/heads/{head}");
                git_user(&cwd, &["fetch", "origin", &spec], PUSH_TIMEOUT).await?;
            }
        }
        let path = if request.worktree {
            let purpose = format!("pr-{number}");
            let path = crate::source_control::git::worktree_path(
                &crate::source_control::git::primary_checkout(&cwd).await?,
                &purpose,
            )?;
            if !path.exists() {
                let target = path.to_string_lossy().into_owned();
                git_user(
                    &cwd,
                    &["worktree", "add", "-q", &target, &head],
                    PUSH_TIMEOUT,
                )
                .await?;
            }
            path
        } else {
            git_user(&cwd, &["switch", "-q", &head], FORGE_TIMEOUT).await?;
            cwd
        };
        let local = text(&git(&path, &["rev-parse", "HEAD"]).await?);
        let warning = detail
            .head_sha
            .filter(|sha| *sha != local)
            .map(|_| "This checkout is not on the pull request's latest commit.".to_owned());
        Ok(PullCheckout {
            path: path.to_string_lossy().into_owned(),
            branch: head,
            warning,
        })
    }

    /// A new thread on the pull request's own worktree, linked to it.
    pub async fn work_on_pull(
        &self,
        project_id: &str,
        number: u64,
        request: &WorkOnPull,
        actor: &str,
    ) -> Result<Thread> {
        let checkout = self
            .checkout_pull(project_id, number, &CheckoutPull { worktree: true }, actor)
            .await?;
        let (project, forge) = self.pull_site(project_id).await?;
        let pull = fetch_pull(forge, Path::new(&project.path), number).await?;
        let thread = self.store.create_thread(crate::store::NewThread {
            project_id: project_id.to_owned(),
            title: pull.title.clone(),
            engine: request.engine.clone(),
            model: request.model.clone(),
            cwd: checkout.path.clone(),
            worktree: Some(orbyn_contracts::Worktree {
                path: checkout.path,
                branch: checkout.branch,
                base: pull.base.clone(),
            }),
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })?;
        self.store.link_pull(&thread.id, &pull)?;
        Ok(thread)
    }
}
