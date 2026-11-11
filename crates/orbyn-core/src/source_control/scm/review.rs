//! Acting on a pull request: comments, reviews, people, labels, updates, merges and bulk actions.

use super::check_opaque;
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::source_control::forge::{self, fetch_pull};
use orbyn_contracts::{
    AddPullComment, EditPullComment, MergePull, PullBulkResult, PullCandidate, PullRequestDetail,
    ReactPullComment, SetPullPeople, SubmitReview, UpdatePull,
};
use serde_json::json;
use std::path::Path;

impl Core {
    pub async fn comment_pull(
        &self,
        project_id: &str,
        number: u64,
        request: &AddPullComment,
        actor: &str,
    ) -> Result<()> {
        if request.body.trim().is_empty() {
            return Err(CoreError::Invalid("Write a comment first.".into()));
        }
        let (project, forge) = self.pull_site(project_id).await?;
        self.scm_audit(actor, "git.pull_comment", &number.to_string(), None)?;
        let outcome = forge::add_comment(forge, Path::new(&project.path), number, request).await;
        self.scm_reads.pull_changed(project_id, number);
        outcome
    }

    pub async fn edit_pull_comment(
        &self,
        project_id: &str,
        number: u64,
        comment: &str,
        request: &EditPullComment,
        actor: &str,
    ) -> Result<()> {
        check_opaque(comment)?;
        let (project, forge) = self.pull_site(project_id).await?;
        self.scm_audit(actor, "git.pull_comment_edit", comment, None)?;
        let cwd = Path::new(&project.path);
        let outcome =
            forge::edit_comment(forge, cwd, number, comment, &request.kind, &request.body).await;
        self.scm_reads.pull_changed(project_id, number);
        outcome
    }

    pub async fn react_pull_comment(
        &self,
        project_id: &str,
        number: u64,
        comment: &str,
        request: &ReactPullComment,
    ) -> Result<()> {
        check_opaque(comment)?;
        let (project, forge) = self.pull_site(project_id).await?;
        let cwd = Path::new(&project.path);
        let outcome =
            forge::react(forge, cwd, number, comment, &request.kind, &request.content).await;
        self.scm_reads.pull_changed(project_id, number);
        outcome
    }

    pub async fn resolve_pull_thread(
        &self,
        project_id: &str,
        number: u64,
        thread: &str,
        resolved: bool,
        actor: &str,
    ) -> Result<()> {
        check_opaque(thread)?;
        let (project, forge) = self.pull_site(project_id).await?;
        self.scm_audit(actor, "git.pull_thread_resolve", thread, None)?;
        let cwd = Path::new(&project.path);
        let outcome = forge::set_thread_resolved(forge, cwd, number, thread, resolved).await;
        self.scm_reads.pull_changed(project_id, number);
        outcome
    }

    pub async fn submit_pull_review(
        &self,
        project_id: &str,
        number: u64,
        review: &SubmitReview,
        actor: &str,
    ) -> Result<()> {
        let (project, forge) = self.pull_site(project_id).await?;
        self.scm_audit(actor, "git.review", &number.to_string(), None)?;
        let outcome = forge::submit_review(forge, Path::new(&project.path), number, review).await;
        self.scm_reads.pull_changed(project_id, number);
        outcome
    }

    pub async fn pull_reviewers(
        &self,
        project_id: &str,
        refresh: bool,
    ) -> Result<Vec<PullCandidate>> {
        self.cached_candidates(project_id, false, refresh).await
    }

    pub async fn pull_labels(&self, project_id: &str, refresh: bool) -> Result<Vec<PullCandidate>> {
        self.cached_candidates(project_id, true, refresh).await
    }

    pub async fn set_pull_reviewers(
        &self,
        project_id: &str,
        number: u64,
        change: &SetPullPeople,
        actor: &str,
    ) -> Result<()> {
        let (project, forge) = self.pull_site(project_id).await?;
        self.scm_audit(actor, "git.pull_reviewers", &number.to_string(), None)?;
        let outcome = forge::set_reviewers(forge, Path::new(&project.path), number, change).await;
        self.scm_reads.pull_changed(project_id, number);
        outcome
    }

    pub async fn set_pull_labels(
        &self,
        project_id: &str,
        number: u64,
        change: &SetPullPeople,
        actor: &str,
    ) -> Result<()> {
        let (project, forge) = self.pull_site(project_id).await?;
        self.scm_audit(actor, "git.pull_labels", &number.to_string(), None)?;
        let outcome = forge::set_labels(forge, Path::new(&project.path), number, change).await;
        self.scm_reads.pull_changed(project_id, number);
        outcome
    }

    pub async fn update_pull(
        &self,
        project_id: &str,
        number: u64,
        update: &UpdatePull,
        actor: &str,
    ) -> Result<PullRequestDetail> {
        let (project, forge) = self.pull_site(project_id).await?;
        let cwd = Path::new(&project.path);
        self.scm_audit(actor, "git.pull_update", &number.to_string(), None)?;
        let outcome = match forge::update_pull(forge, cwd, number, update).await {
            Ok(()) => forge::pull_detail(forge, cwd, number).await,
            Err(error) => Err(error),
        };
        self.scm_reads.pull_changed(project_id, number);
        outcome
    }

    pub async fn merge_project_pull(
        &self,
        project_id: &str,
        number: u64,
        options: &MergePull,
        actor: &str,
    ) -> Result<()> {
        let (project, forge) = self.pull_site(project_id).await?;
        self.scm_audit(actor, "git.merge", &number.to_string(), None)?;
        let mut options = options.clone();
        options.strip_credits |=
            self.preference("remove_agent_credits_on_merge", Some(project_id))? == json!(true);
        let outcome = forge::merge(forge, Path::new(&project.path), number, &options).await;
        self.scm_reads.pull_changed(project_id, number);
        outcome
    }

    pub async fn pull_action(
        &self,
        project_id: &str,
        number: u64,
        action: &str,
        actor: &str,
    ) -> Result<()> {
        let (project, forge) = self.pull_site(project_id).await?;
        let cwd = Path::new(&project.path);
        // Deleting a branch needs its name, and only then is it worth a read.
        let head = if action == "delete_branch" {
            fetch_pull(forge, cwd, number).await?.head
        } else {
            String::new()
        };
        self.scm_audit(
            actor,
            &format!("git.pull_{action}"),
            &number.to_string(),
            None,
        )?;
        let outcome = forge::pull_action(forge, cwd, number, action, &head).await;
        self.scm_reads.pull_changed(project_id, number);
        outcome
    }

    /// Closes each in turn. One refusal does not stop the rest.
    pub async fn close_pulls(
        &self,
        project_id: &str,
        numbers: &[u64],
        actor: &str,
    ) -> Result<Vec<PullBulkResult>> {
        let mut results = Vec::with_capacity(numbers.len());
        for number in numbers {
            let outcome = self.pull_action(project_id, *number, "close", actor).await;
            results.push(PullBulkResult {
                number: *number,
                ok: outcome.is_ok(),
                message: outcome.err().map(|error| error.public_message()),
            });
        }
        Ok(results)
    }

    pub fn set_files_viewed(
        &self,
        project_id: &str,
        number: u64,
        paths: &[String],
        viewed: bool,
    ) -> Result<()> {
        self.store.project(project_id)?;
        self.store
            .set_files_viewed(project_id, number, paths, viewed)
    }
}
