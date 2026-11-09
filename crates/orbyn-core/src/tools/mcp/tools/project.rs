//! The project tools: adding, removing and moving threads and tasks, kept inside the caller's project.

use super::text;
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::tools::mcp::McpBinding;
use orbyn_contracts::Thread;
use serde_json::{Value, json};
use std::path::Path;

pub(super) const MAX_PROJECT_PATH_BYTES: usize = 4096;

pub(super) const MAX_PROJECT_ID_BYTES: usize = 128;

pub(super) fn bounded_project_text<'a>(
    args: &'a Value,
    key: &str,
    max_bytes: usize,
) -> Result<&'a str> {
    let value = text(args, key)?;
    if value.len() > max_bytes || value.contains('\0') {
        return Err(CoreError::Invalid(format!(
            "{key} is at most {max_bytes} bytes and cannot contain NUL."
        )));
    }
    Ok(value)
}

impl Core {
    /// Only project add/remove may reach another project. Resolve exactly what
    /// the approval describes; callers cannot supply the internal preview fields.
    pub(super) async fn project_tool_preview(
        &self,
        binding: &McpBinding,
        name: &str,
        args: &Value,
    ) -> Result<Option<Value>> {
        if binding.scope.plan && matches!(name, "orbyn_project_create" | "orbyn_project_delete") {
            return Err(CoreError::Invalid(
                "Plan mode does not add or remove projects.".into(),
            ));
        }
        match name {
            "orbyn_project_create" => {
                let path = bounded_project_text(args, "path", MAX_PROJECT_PATH_BYTES)?;
                if !Path::new(path).is_absolute() {
                    return Err(CoreError::Invalid(
                        "path must be an absolute existing folder.".into(),
                    ));
                }
                let root = crate::project::workspace::resolve_project(
                    &self.workspace.root()?,
                    Path::new(path),
                )
                .await?;
                let path = root.path.to_string_lossy().into_owned();
                bounded_project_text(&json!({ "path": path }), "path", MAX_PROJECT_PATH_BYTES)?;
                Ok(Some(
                    json!({ "path": path, "trusted": false, "files_created": false }),
                ))
            }
            "orbyn_project_delete" => {
                let id = bounded_project_text(args, "project_id", MAX_PROJECT_ID_BYTES)?;
                let project = self.store.project(id)?;
                bounded_project_text(
                    &json!({ "path": project.path }),
                    "path",
                    MAX_PROJECT_PATH_BYTES,
                )?;
                bounded_project_text(&json!({ "name": project.name }), "name", 4096)?;
                let own = self.store.thread(&binding.scope.thread_id)?;
                if project.id == own.project_id
                    || project.scratch
                    || Path::new(&project.path) == self.workspace.root()?
                {
                    return Err(CoreError::Invalid("The caller's own, library and scratch projects cannot be removed by a tool.".into()));
                }
                let threads = self.store.agent_project_threads(id)?;
                Ok(Some(json!({
                    "project_id": id, "path": project.path, "name": project.name,
                    "thread_ids": threads, "files_removed": false,
                })))
            }
            _ => Ok(None),
        }
    }

    /// A thread in the caller's project. Anything else reads as not found, so a
    /// tool cannot be used to learn what other projects hold.
    pub(super) fn project_thread(&self, binding: &McpBinding, id: &str) -> Result<Thread> {
        let own = self.store.thread(&binding.scope.thread_id)?;
        let thread = self.store.thread(id)?;
        if thread.project_id != own.project_id {
            return Err(CoreError::NotFound("thread".into()));
        }
        Ok(thread)
    }

    pub(super) fn project_task(&self, binding: &McpBinding, id: &str) -> Result<()> {
        let own = self.store.thread(&binding.scope.thread_id)?;
        if self.store.task(id)?.project_id != own.project_id {
            return Err(CoreError::NotFound("scheduled task".into()));
        }
        Ok(())
    }
}
