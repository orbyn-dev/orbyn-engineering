//! `crew_ask`: a question that waits for its answer, at most 40 seconds (SPEC.md section 4).
//! With no answer in time it returns `pending`, and the answer arrives later as a message. An
//! answer posted while the asker waits comes back to the asker itself and is not sent again.

use std::sync::Arc;
use std::time::Duration;

use orbyn_contracts::{CrewError, CrewMessage, CrewObject};
use serde_json::{Value, json};
use tokio::time::Instant;

use super::ASK_TOOL;
use super::deliver::wrap;
use super::rules::answer_to;
use crate::app::Core;
use crate::crew::leases::internal;
use crate::tools::mcp::McpBinding;

/// Stops waiting for a question when the ask ends however it ends, even if its call is dropped
/// (the tool's time budget) or fails.
struct Waiting<'a>(&'a Core, String);

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        self.0.stop_waiting(&self.1);
    }
}

/// The longest `crew_ask` waits, and how long it waits when it is not told.
const LONGEST_WAIT: u32 = 40;

impl Core {
    /// `crew_ask`.
    pub async fn crew_ask_tool(
        self: &Arc<Self>,
        binding: &McpBinding,
        args: &Value,
    ) -> Result<Value, CrewError> {
        let sent = self.send_crew_message(binding, args, ASK_TOOL).await?;
        let (question, asker) = (
            sent.message.id.as_str().to_owned(),
            self.bus_sender(binding, ASK_TOOL)?,
        );
        let address = if asker.is_lead() {
            "lead"
        } else {
            asker.member.as_str()
        }
        .to_owned();
        let wait = sent
            .message
            .wait_seconds
            .unwrap_or(LONGEST_WAIT)
            .min(LONGEST_WAIT);
        let _waiting = Waiting(self, question.clone());
        let deadline = Instant::now() + Duration::from_secs(wait.into());
        let crew = asker.crew_id.as_str().to_owned();
        let outcome = loop {
            let notified = self.crew_bus.wake.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(answer) = self.answer_for(&crew, &question, &address)? {
                break Some(answer);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break None;
            }
            // The asker's own run ending ends the wait (H7).
            self.bus_sender(binding, ASK_TOOL)?;
            let _ = tokio::time::timeout(left, notified).await;
        };
        // Stop waiting under the lock, then look once more: an answer posted just now was
        // counted as returned here, so it must not be lost.
        let _bus = self.crew_bus.lock.lock().await;
        self.stop_waiting(&question);
        let answer = match outcome {
            Some(answer) => Some(answer),
            None => self.answer_for(&crew, &question, &address)?,
        };
        Ok(match answer {
            Some(answer) => json!({
                "state": "answered",
                "question": question,
                "answer": CrewObject::Message(answer.clone()).to_value(),
                "delivered_as": wrap(&answer),
            }),
            None => json!({
                "state": "pending",
                "question": question,
                "note": "No answer yet. It arrives later as a message.",
            }),
        })
    }

    fn stop_waiting(&self, question: &str) {
        if let Ok(mut waiting) = self.crew_bus.waiting.lock() {
            waiting.remove(question);
        }
    }

    fn answer_for(
        &self,
        crew: &str,
        question: &str,
        asker: &str,
    ) -> Result<Option<CrewMessage>, CrewError> {
        let log = self.store.crew_messages_in_crew(crew).map_err(internal)?;
        Ok(answer_to(log.iter().map(|(_, m)| m), question, asker).cloned())
    }
}
