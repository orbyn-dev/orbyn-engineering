//! Lead-only merge requests. A tool cannot impersonate the person's Review answers or CI policy.

use std::sync::Arc;

use orbyn_contracts::{CrewError, ErrorCode};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{MergeTarget, valid_id};
use crate::app::Core;
use crate::crew::leases::internal;
use crate::crew::tool::invalid;
use crate::tools::mcp::McpBinding;

pub const MERGE_TOOLS: [&str; 4] = [
    "crew_queue_merge",
    "crew_process_merges",
    "crew_answer_review",
    "crew_watch_main",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    task: String,
    #[serde(default = "main_branch")]
    branch: String,
}

fn main_branch() -> String {
    "main".into()
}

pub fn specs() -> Vec<Value> {
    MERGE_TOOLS.iter().map(|name| json!({
        "name": name,
        "description": match *name {
            "crew_queue_merge" => "Preview, lead only: queue a passed task for Core-owned forge publication and merging. Person-owned required CI policy, holds and review lanes still apply. No direct main push or merge authority is granted.",
            "crew_process_merges" => "Preview, lead only: process this crew's queue for the task's target. Uncertain publication/merge effects are reconciled before any retry.",
            "crew_answer_review" => "Preview, lead only: admit a scoped fix for the person's already stored Comment. This cannot answer Looks good, Comment or Revert on the person's behalf.",
            _ => "Preview, lead only: check the actual forge tip after confirmed Crew merges, deliver durable digest/overdue notices, and ask a person when checks break.",
        },
        "inputSchema": {"type": "object", "additionalProperties": false, "properties": {"task": {"type": "string"}, "branch": {"type": "string"}}, "required": ["task"]}
    })).collect()
}

impl Core {
    /// Local-person HTTP operation. The request contains no filesystem path, CI policy or owed-lane override.
    pub async fn crew_merge_request(
        self: &Arc<Self>,
        task: &str,
        request: &MergeRequest,
    ) -> crate::error::Result<Value> {
        valid_id(task)?;
        let target = self.crew_merge_target(task, &request.branch).await?;
        let now = crate::util::unix_now();
        match request.operation {
            MergeOperation::Queue => {
                self.crew_queue_forge_merge(task, &target, now).await?;
                Ok(json!({"task": task, "queued": true}))
            }
            MergeOperation::Process => {
                Ok(json!(self.crew_process_forge_merges(&target, now).await?))
            }
            MergeOperation::Watch => {
                self.crew_watch_forge_main(&target, now).await?;
                Ok(json!({"checked": true}))
            }
            MergeOperation::Release => {
                self.crew_preview_only()?;
                self.crew_release_merge(task)?;
                Ok(json!({"released": true}))
            }
        }
    }

    pub async fn crew_merge_tool(
        self: &Arc<Self>,
        binding: &McpBinding,
        tool: &str,
        args: &Value,
    ) -> Result<Value, CrewError> {
        if !self.crew_enabled() {
            return Err(CrewError::new(ErrorCode::UnknownTool, tool));
        }
        let request: Request =
            serde_json::from_value(args.clone()).map_err(|e| invalid(e.to_string()))?;
        valid_id(&request.task).map_err(internal)?;
        let (thread, _) = self.review_lead(binding, tool)?;
        self.review_subject(&thread, &request.task)?;
        let target = self
            .crew_merge_target(&request.task, &request.branch)
            .await
            .map_err(internal)?;
        let now = crate::util::unix_now();
        match tool {
            "crew_queue_merge" => {
                self.crew_queue_forge_merge(&request.task, &target, now)
                    .await
                    .map_err(internal)?;
                Ok(json!({"task": request.task, "queued": true}))
            }
            "crew_process_merges" => Ok(json!(
                self.process_forge_merges_for(&target, Some(&thread.id), now)
                    .await
                    .map_err(internal)?
            )),
            "crew_answer_review" => self.crew_merge_comment_fix(&thread, &request.task).await,
            "crew_watch_main" => {
                self.crew_watch_forge_main(&target, now)
                    .await
                    .map_err(internal)?;
                Ok(json!({"checked": true}))
            }
            _ => Err(CrewError::new(ErrorCode::UnknownTool, tool)),
        }
    }

    /// No agent-supplied filesystem path. A task's saved target wins once it is queued.
    pub async fn crew_merge_target(
        &self,
        task_id: &str,
        branch: &str,
    ) -> crate::error::Result<MergeTarget> {
        self.crew_preview_only()?;
        self.refuse_unproven_escape_check(task_id)?;
        if let Some(intent) = self.store.crew_forge_merge(task_id)? {
            if branch != intent.branch {
                return Err(crate::error::CoreError::Invalid(
                    "the stored target branch differs".into(),
                ));
            }
            return Ok(MergeTarget {
                repo: intent.repo.into(),
                branch: intent.branch,
                owed: None,
            });
        }
        let run = self
            .store
            .crew_run(task_id)?
            .ok_or_else(|| crate::error::CoreError::NotFound("crew run".into()))?;
        let lead = self.store.thread(&run.lead_thread_id)?;
        let project = self.store.project(&lead.project_id)?;
        let repo = std::path::PathBuf::from(project.path);
        crate::source_control::git::git(
            &repo,
            &["check-ref-format", &format!("refs/heads/{branch}")],
        )
        .await?;
        Ok(MergeTarget {
            repo,
            branch: branch.into(),
            owed: None,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeRequest {
    pub operation: MergeOperation,
    #[serde(default = "main_branch")]
    pub branch: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeOperation {
    Queue,
    Process,
    Watch,
    Release,
}
