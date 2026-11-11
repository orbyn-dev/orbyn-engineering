//! The bus ledger: what the bus decided on its own, recorded by the core and never by a member.
//! Today that is one event, a hop reset. A lead message to one member restarts the exchange count
//! of every pair that member is in (SPEC.md H4), so a routine message can re-arm a looping pair the lead
//! never looked at. The row says which pairs were re-armed and how many exchanges each had, so real
//! runs show whether that happens. It holds ids and counts and no message text.

use orbyn_contracts::CrewMessage;
use serde_json::{Value, json};

use super::rules::exchanges;
use crate::app::Core;
use crate::crew::guard::{refuse_leak, refuse_secret};
use crate::store::CrewTask;

/// The pairs `member` is in with each of `others`, with the exchanges each had before the lead's message.
pub(super) fn rearmed<'a>(
    log: impl Iterator<Item = &'a CrewMessage> + Clone,
    member: &str,
    others: &[String],
) -> Vec<(String, u32)> {
    others
        .iter()
        .filter(|other| other.as_str() != member)
        .map(|other| (other.clone(), exchanges(log.clone(), member, other)))
        .collect()
}

impl Core {
    /// Appends a `hop_reset` row. The row passes the secret scan and the answer-key check like every other
    /// record; one that fails either is not written. A ledger that cannot be written never undoes the message.
    pub(super) fn record_hop_reset(
        &self,
        task: &CrewTask,
        message: &CrewMessage,
        member: &str,
        pairs: &[(String, u32)],
        needles: &[String],
    ) {
        let body: Value = json!({
            "event": "hop_reset",
            "message": message.id,
            "to": member,
            "pairs": pairs
                .iter()
                .map(|(with, before)| json!({ "with": with, "before": before }))
                .collect::<Vec<_>>(),
        });
        if refuse_secret(&body).is_err() || refuse_leak(&body, needles).is_err() {
            tracing::warn!("a hop reset was not recorded: the row failed the secret or key check");
            return;
        }
        if let Err(error) =
            self.store
                .add_crew_bus_event(&task.id, &task.crew_id, "hop_reset", &body)
        {
            tracing::warn!(%error, "could not record a hop reset");
        }
    }
}
