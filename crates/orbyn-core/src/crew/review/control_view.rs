//! Read-only eligibility/receipts; advisory controls never replace write admission.
use crate::{
    app::Core,
    error::{CoreError, Result},
};
use orbyn_contracts::{CrewLaneRemovalReceipt, CrewRequiredReviewLane, CrewReviewControlView};
use std::collections::BTreeMap;
impl Core {
    pub async fn crew_review_control_view(&self, subject: &str) -> Result<CrewReviewControlView> {
        self.crew_preview_only()?;
        let run = self
            .store
            .crew_run(subject)?
            .ok_or_else(|| CoreError::NotFound("crew run".into()))?;
        let thread = self.store.thread(&run.lead_thread_id)?;
        let thread = self
            .correction_review_thread(&thread, subject)
            .await
            .map_err(|error| CoreError::Invalid(error.message))?;
        let (task, _) = self
            .review_subject(&thread, subject)
            .map_err(|e| CoreError::Invalid(e.message))?;
        let receipts = self.store.crew_review_controls(subject)?;
        let mut names = BTreeMap::from([
            (
                super::CODE_LANE.to_owned(),
                (false, "Default review lane".to_owned()),
            ),
            (
                super::SECURITY_LANE.to_owned(),
                (false, "Default review lane".to_owned()),
            ),
        ]);
        let changed = self
            .review_changed(&run, std::path::Path::new(&thread.cwd))
            .await;
        let rules = match run.base_commit.as_deref() {
            Some(base) => {
                super::rules::read_rules_at(std::path::Path::new(&thread.cwd), base).await
            }
            None => Err("missing pinned base".into()),
        };
        let error = match rules {
            Ok(rules) => {
                for rule in &rules {
                    if rule.applies(changed.as_deref()) {
                        names.insert(
                            rule.lane.as_str().to_owned(),
                            (rule.locked, "Repository rule · .orbyn/crew.json".into()),
                        );
                    }
                }
                // Any pinned locked rule prevents removal, matching the writer.
                for rule in rules.iter().filter(|r| r.locked) {
                    if let Some(row) = names.get_mut(rule.lane.as_str()) {
                        row.0 = true;
                    }
                }
                None
            }
            Err(_) => Some("Required lanes could not be read. Reload task.".to_owned()),
        };
        for receipt in receipts
            .iter()
            .filter(|r| r.kind == "remove_lane" && r.state == "removed")
        {
            names
                .entry(receipt.lane.clone())
                .or_insert((false, "Recorded lane removal".into()));
        }
        let lanes = names
            .into_iter()
            .map(|(lane, (locked, source))| {
                let removal = receipts
                    .iter()
                    .find(|r| r.kind == "remove_lane" && r.state == "removed" && r.lane == lane)
                    .map(|r| CrewLaneRemovalReceipt {
                        id: r.id.clone(),
                        lane: r.lane.clone(),
                        reason: r.reason.clone(),
                    });
                CrewRequiredReviewLane {
                    can_remove: task.state == "passed"
                        && !locked
                        && removal.is_none()
                        && error.is_none(),
                    lane,
                    source,
                    locked,
                    removal,
                }
            })
            .collect();
        Ok(CrewReviewControlView {
            task: subject.into(),
            lanes,
            error,
        })
    }
}
