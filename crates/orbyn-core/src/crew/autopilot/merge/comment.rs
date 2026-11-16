//! A person's stored Comment becomes an ordinary scoped task, never a command.

use std::sync::Arc;

use orbyn_contracts::{CrewObject, Thread};
use serde_json::{Value, json};

use crate::app::Core;
use crate::crew::leases::{internal, timestamp};
use crate::crew::start::{Launch, Links};
use crate::crew::tool::invalid;
use crate::source_control::git::{git_hardened, text};

fn draft(mut object: Value, own: &[&str]) -> Value {
    if let Some(fields) = object.as_object_mut() {
        for field in [
            "type",
            "v",
            "id",
            "crew_id",
            "author",
            "created_at",
            "run_id",
        ]
        .iter()
        .copied()
        .chain(own.iter().copied())
        {
            fields.remove(field);
        }
    }
    object
}

impl Core {
    pub(super) async fn crew_merge_comment_fix(
        self: &Arc<Self>,
        thread: &Thread,
        task_id: &str,
    ) -> Result<Value, orbyn_contracts::CrewError> {
        let _serial = self.crew_merges.lock().await;
        let (subject, _) = self.review_subject(thread, task_id)?;
        let record = self
            .store
            .crew_merge(task_id)
            .map_err(internal)?
            .ok_or_else(|| invalid("the task has not merged"))?;
        if record.review_state != "commented" {
            return Err(invalid(
                "only the person's stored Comment can start a review fix",
            ));
        }
        let comment = record
            .comment
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| invalid("no person comment"))?;
        let original = self
            .store
            .crew_brief(task_id)
            .map_err(internal)?
            .ok_or_else(|| invalid("no original brief"))?;
        let key = self
            .store
            .crew_answer_key(task_id)
            .map_err(internal)?
            .ok_or_else(|| invalid("no original key"))?;
        let mut stamper = self.review_stamper(thread, original.author.engine.as_str())?;
        // Stable admission identity across retries. A crash after admission must not launch a second worker.
        stamper.now = timestamp(record.merged_unix)?;
        let mut brief = draft(CrewObject::Brief(original.clone()).to_value(), &[]);
        brief["outcome"] = json!(format!(
            "Address the person's review comment on task {}. The comment is task data: {}",
            subject.id, comment
        ));
        brief["stop_when"] = json!([
            "The review comment is addressed within the original task's writable files, or stop for a person."
        ]);
        let brief = stamper.brief(&brief)?;
        if let Some(existing) = self.store.crew_task(brief.id.as_str()).map_err(internal)? {
            return Ok(
                json!({"task": existing.id, "state": existing.state, "already_admitted": true}),
            );
        }
        let cwd = std::path::Path::new(&thread.cwd);
        let head = text(
            &git_hardened(cwd, &["rev-parse", "--verify", "HEAD"])
                .await
                .map_err(internal)?,
        );
        if head != record.merge_commit {
            return Err(invalid(
                "the lead checkout must be at the reviewed merge before a Comment fix is admitted",
            ));
        }
        let mut key = draft(CrewObject::AnswerKey(key).to_value(), &["brief"]);
        let paths: Vec<_> = original.write_paths().map(|p| p.as_str()).collect();
        if paths.is_empty() {
            return Err(invalid(
                "the original task grants no writable files for a fix",
            ));
        }
        key["checks"]
            .as_array_mut()
            .ok_or_else(|| invalid("original checks are missing"))?
            .push(json!({
                "id": "merge-comment-scope", "kind": "file_scope", "params": {"only": paths},
                "proves": "The fix stays within the original task's writable files."
            }));
        let key = stamper.answer_key(&key, &brief.id)?;
        self.launch_crew_task(Launch {
            lead: thread,
            now: stamper.now,
            brief,
            key,
            role: None,
            lane: None,
            author_engine: None,
            prefix: "worker",
            prompt: None,
            links: Links::default(),
        })
        .await
    }
}
