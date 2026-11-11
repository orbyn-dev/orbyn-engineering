//! What follows a task that passed, for the review group. A second run that passed decides its finding. A fix that
//! passed sends every lane of the reviewed task round again, on the fix's commit. A lane that passed tells the lead how
//! many findings it filed, and whether any is one a fix already answered (which stops the loop). All of it is code: the
//! lead is told, and nothing here asks a model to decide.

use std::sync::Arc;

use orbyn_contracts::CrewToken;

use crate::app::Core;
use crate::crew::guard::refuse_secret;
use crate::store::CrewReview;

impl Core {
    /// Queues a short notice for the lead of the crew `task_id` belongs to, as data.
    pub(super) fn tell_lead_text(&self, task_id: &str, text: &str) {
        let Ok(Some(run)) = self.store.crew_run(task_id) else {
            return;
        };
        let body = text.replace('&', "&amp;").replace('<', "&lt;");
        let notice =
            format!("<crew-review task=\"{task_id}\" trust=\"data\">\n{body}\n</crew-review>");
        if refuse_secret(&serde_json::json!({ "notice": notice })).is_err() {
            return;
        }
        if let Err(error) = self.runs.enqueue(&run.lead_thread_id, &notice, &[]) {
            tracing::warn!(%error, "could not tell the lead about a review");
        }
    }

    /// Called when a task has passed. Cheap for a task that is neither a review run nor a fix.
    pub(crate) async fn review_task_passed(self: &Arc<Self>, task_id: &str) {
        if let Ok(Some(review)) = self.store.crew_review(task_id) {
            if review.role == "finding_check" {
                self.decide_second_run(&review).await;
            } else {
                self.review_lane_passed(&review);
            }
        }
        if let Ok(Some(fix)) = self.store.crew_fix_for_task(task_id) {
            self.rerun_lanes(task_id, &fix.subject_id).await;
        }
    }

    /// Starts `review_task_unpassed` for a task that ended or waits for a person without passing.
    pub(crate) fn spawn_review_unpassed(self: &Arc<Self>, task_id: &str) {
        if !matches!(self.store.crew_fix_for_task(task_id), Ok(Some(_))) {
            return;
        }
        let (core, id) = (self.clone(), task_id.to_owned());
        tokio::spawn(async move { core.review_task_unpassed(&id).await });
    }

    /// A fix did not pass. If an earlier fix passed while this one was still running, the lanes did not run for it then, so
    /// they run now, once no fix is left running.
    async fn review_task_unpassed(self: &Arc<Self>, fix_task: &str) {
        let Ok(Some(fix)) = self.store.crew_fix_for_task(fix_task) else {
            return;
        };
        let Ok(fixes) = self.store.crew_fixes_of(&fix.subject_id) else {
            return;
        };
        let Ok(reviews) = self.store.crew_reviews_of(&fix.subject_id) else {
            return;
        };
        let task = |id: &str| self.store.crew_task(id).ok().flatten();
        let last_round = reviews
            .iter()
            .filter(|r| r.role == "review")
            .filter_map(|r| task(&r.task_id))
            .map(|t| t.created_at)
            .max()
            .unwrap_or_default();
        // The newest fix that passed after the last round of lanes started.
        let owed = fixes
            .iter()
            .filter_map(|f| task(&f.fix_task_id).filter(|t| t.state == "passed"))
            .filter(|t| t.updated_at > last_round)
            .max_by(|a, b| a.updated_at.cmp(&b.updated_at));
        if let Some(passed) = owed {
            self.rerun_lanes(&passed.id, &fix.subject_id).await;
        }
    }

    fn review_lane_passed(&self, review: &CrewReview) {
        let Ok(filed) = self.store.crew_findings_of_task(&review.task_id) else {
            return;
        };
        let Ok(fixes) = self.store.crew_fixes_of(&review.subject_id) else {
            return;
        };
        let fixed_places: Vec<(String, String)> = fixes
            .iter()
            .filter_map(|x| self.store.crew_finding(&x.finding_id).ok().flatten())
            .map(|f| (f.lane, f.finding.path.as_str().to_owned()))
            .collect();
        let again = filed
            .iter()
            .filter(|f| {
                fixed_places.contains(&(f.lane.clone(), f.finding.path.as_str().to_owned()))
            })
            .count();
        let mut text = format!(
            "Review lane {} (round {}) finished with {} findings. Call crew_review_status for where they stand.",
            review.lane,
            review.round,
            filed.len()
        );
        if again > 0 {
            text.push_str(&format!(
                " {again} of them were already sent to a builder once; the same finding after a fix stops the loop, so a person decides."
            ));
        }
        self.tell_lead_text(&review.subject_id, &text);
    }

    /// A fix passed: every lane that has reviewed the work runs its next round on the fix's commit.
    pub(super) async fn rerun_lanes(self: &Arc<Self>, fix_task: &str, subject_id: &str) {
        let Ok(Some(subject)) = self.store.crew_task(subject_id) else {
            return;
        };
        let Ok(Some(run)) = self.store.crew_run(subject_id) else {
            return;
        };
        let Ok(thread) = self.store.thread(&run.lead_thread_id) else {
            return;
        };
        let Ok(stamper) = self.review_stamper(&thread, &thread.engine) else {
            return;
        };
        let Ok(Some(fix)) = self.store.crew_task(fix_task) else {
            return;
        };
        // Lanes read the shared checkout: while another fix of this work is still running its writes would fail their
        // read-only checks. The last fix to pass runs them.
        if let Ok(fixes) = self.store.crew_fixes_of(subject_id) {
            for other in fixes.iter().filter(|f| f.fix_task_id != fix_task) {
                let running = self
                    .store
                    .crew_task(&other.fix_task_id)
                    .ok()
                    .flatten()
                    .is_some_and(|t| {
                        !matches!(
                            t.state.as_str(),
                            "passed" | "stopped" | "cancelled" | "needs_person"
                        )
                    });
                if running {
                    return;
                }
            }
        }
        let Ok(commit) = self
            .review_commit(&fix, std::path::Path::new(&thread.cwd))
            .await
        else {
            return;
        };
        let Ok(reviews) = self.store.crew_reviews_of(subject_id) else {
            return;
        };
        let mut lanes: Vec<String> = Vec::new();
        for review in reviews.iter().filter(|r| r.role == "review") {
            if !lanes.contains(&review.lane) {
                lanes.push(review.lane.clone());
            }
        }
        for lane in lanes {
            let Ok(token) = CrewToken::new(lane) else {
                continue;
            };
            if let Err(error) = self
                .start_review_run(
                    &thread,
                    &stamper,
                    &subject,
                    &token,
                    &commit,
                    super::lanes::Run::Lane,
                )
                .await
            {
                tracing::warn!(%error, "could not run a lane again after a fix");
            }
        }
    }
}
