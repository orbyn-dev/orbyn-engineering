//! Closing a lead Crew thread (Crew Preview, KC2): the live tasks it leads, read for the confirm, and the person's cancel run
//! for each of them, one by one, when the thread settles or archives. Nothing here is stored: the live set is a view of
//! `crew_runs.lead_thread_id` and the task states, and every cancel is the existing audited one, with a fixed reason.
//!
//! One rule for both routes: `live_tasks_led_by` is the only choice of what a close tries, so the confirm's count is what the
//! close tries when nothing starts or ends in between. The close chooses again when it runs: a task that started after the
//! confirm was read is tried too, and one that ended meanwhile is skipped. A try can fail, so `closed` and `failed` say which. The thread's own close/archive is the caller's and proceeds whatever this answers; a task that
//! could not be stopped keeps its state and appears in the person's "needs you" inbox as `could_not_stop`.
//!
//! The lead is not told about the tasks this close cancelled: `tell_lead` skips a cancelled result whose cancel says
//! [`REASON`], because a note would start the thread the person is closing. A cancel with another reason still tells it.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::sync::Arc;

use orbyn_contracts as wire;

use super::flow::scrub::Scrub;
use super::{lifecycle, valid_id};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::CrewTask;

/// The reason every task a lead's close cancels is stored with. The cancel audits it like any other reason.
pub const REASON: &str = "The lead thread was closed.";

/// The longest task name a confirm shows, in characters: the Flow board's card name is cut the same way.
const NAME_MAX: usize = 160;

/// How many times a close reads the live set again for a task admitted while it was cancelling the others.
const PASSES: usize = 3;

impl Core {
    /// The tasks a close of `thread_id` tries to cancel: the non-final tasks this thread leads, oldest first. The one selection
    /// both the live read and the close use.
    fn live_tasks_led_by(&self, thread_id: &str) -> Result<Vec<CrewTask>> {
        Ok(self
            .store
            .crew_tasks_led_by(thread_id)?
            .into_iter()
            .filter(|task| !lifecycle::is_final(&task.state))
            .collect())
    }

    /// The live tasks of a lead thread, as the confirm before closing or archiving it lists them. Preview, and a read only.
    pub fn crew_lead_live(&self, thread_id: &str) -> Result<wire::CrewLeadLive> {
        self.crew_preview_only()?;
        valid_id(thread_id)?;
        let tasks = self.live_tasks_led_by(thread_id)?;
        let ids: Vec<String> = tasks.iter().map(|task| task.id.clone()).collect();
        let briefs = self.store.crew_briefs_of_tasks(&ids)?;
        let person = self.display_redactor()?;
        let mut scrubs: BTreeMap<String, Scrub> = BTreeMap::new();
        let mut live = Vec::with_capacity(tasks.len());
        for task in tasks {
            let scrub = match scrubs.entry(task.crew_id.clone()) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => entry.insert(Scrub::of_crew(self, &person, &task.crew_id)?),
            };
            live.push(wire::CrewLiveTask {
                name: briefs
                    .get(&task.id)
                    .map_or_else(String::new, |brief| scrub.text(&brief.outcome, NAME_MAX)),
                id: task.id,
                state: task.state,
            });
        }
        Ok(wire::CrewLeadLive { tasks: live })
    }

    /// Cancels the live tasks a lead thread leads, one by one, when the thread is closed or archived. Preview, local and
    /// audited: each task goes through the audited cancel with a fixed reason. `Ok` is a task closed; one that ended meanwhile is
    /// skipped; any other refusal is a task that could not be stopped, and the loop goes on. The caller closes the thread
    /// whatever this answers.
    pub async fn crew_close_lead(
        self: &Arc<Self>,
        thread_id: &str,
        actor: &str,
    ) -> Result<wire::CrewLeadClosed> {
        self.crew_preview_only()?;
        valid_id(thread_id)?;
        let mut closed = Vec::new();
        let mut failed = Vec::new();
        // Each cancel awaits the worker's stop, and a task may be admitted on the thread meanwhile. A few passes read the live set
        // again and try only what was not tried yet; a task admitted after the last pass is the person's to stop (it shows on the
        // board and is cancelled by its own Stop), never silently left behind by a loop that cannot end.
        let mut tried = std::collections::BTreeSet::new();
        for _ in 0..PASSES {
            let tasks: Vec<CrewTask> = self
                .live_tasks_led_by(thread_id)?
                .into_iter()
                .filter(|task| !tried.contains(&task.id))
                .collect();
            if tasks.is_empty() {
                break;
            }
            for task in tasks {
                tried.insert(task.id.clone());
                match self.crew_cancel_task(&task.id, Some(REASON), actor).await {
                    Ok(_) => closed.push(task.id),
                    // It ended between the read and the try: already over is not a failure.
                    Err(CoreError::TaskEnded) => {}
                    Err(error) => {
                        tracing::warn!(task = %task.id, %error, "a lead thread's close could not stop a crew task");
                        failed.push(task.id);
                    }
                }
            }
        }
        Ok(wire::CrewLeadClosed { closed, failed })
    }
}
