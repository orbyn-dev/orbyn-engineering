//! Telling the lead when a guard refuses a member (SPEC.md H4, H5, H12: "the lead is told").
//! The notice is a message from the core to `lead`, stored under the member's task and queued
//! on the lead's thread, so the lead reads it as data like any other message. The same refusal
//! is told once: a worker that keeps knocking does not fill the lead's queue.

use std::sync::Arc;

use orbyn_contracts::{
    CrewAddressee, CrewError, CrewHash, CrewId, CrewLongText, CrewMessage, CrewObject, CrewRole,
    MessageKind,
};

use super::super::leases::{core_author, internal, timestamp};
use super::Sender;
use super::deliver::Target;
use crate::app::Core;

impl Core {
    /// Tells the lead of `error`, which refused `who`. A refusal of the lead itself is not told.
    pub(super) fn tell_lead_of(self: &Arc<Self>, who: &Sender, error: &CrewError) {
        let Some(task_id) = who.task.as_deref() else {
            return;
        };
        if let Err(failure) = self.try_tell_lead(who, task_id, error) {
            tracing::warn!(%failure, "could not tell the lead of a refused crew message");
        }
    }

    fn try_tell_lead(
        self: &Arc<Self>,
        who: &Sender,
        task_id: &str,
        error: &CrewError,
    ) -> Result<(), CrewError> {
        let text = format!(
            "{} was refused ({:?}): {}",
            who.member, error.code, error.message
        );
        let body: String = text.chars().take(1000).collect();
        let task = self
            .store
            .crew_task(task_id)
            .map_err(internal)?
            .ok_or_else(|| {
                CrewError::new(orbyn_contracts::ErrorCode::InvalidObject, "no such task")
            })?;
        let told = self.store.crew_messages(task_id).map_err(internal)?;
        if told
            .iter()
            .any(|m| m.author.role == CrewRole::Core && m.body.as_str() == body)
        {
            return Ok(());
        }
        let invalid = |why: String| CrewError::new(orbyn_contracts::ErrorCode::InvalidObject, why);
        let mut notice = CrewMessage {
            v: 0,
            id: CrewHash::new(format!("sha256:{}", "0".repeat(64))).map_err(invalid)?,
            crew_id: who.crew_id.clone(),
            author: core_author(),
            created_at: timestamp(crate::util::unix_now())?,
            run_id: CrewId::new(task.run_id.clone()).map_err(invalid)?,
            to: CrewAddressee::new("lead").map_err(invalid)?,
            kind: MessageKind::Blocker,
            body: CrewLongText::new(body).map_err(invalid)?,
            refs: None,
            in_reply_to: None,
            expects_reply: None,
            wait_seconds: None,
        };
        notice.validate()?;
        notice.id = CrewObject::Message(notice.clone()).computed_id()?;
        self.store
            .add_crew_object(task_id, &CrewObject::Message(notice.clone()))
            .map_err(internal)?;
        let target = Target {
            recipient: "lead".into(),
            thread: Some(who.lead_thread.clone()),
            steer: false,
            hold: false,
            guard: false,
        };
        self.deliver_crew_message(&notice, &target, &[]);
        Ok(())
    }
}
