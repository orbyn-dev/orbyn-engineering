//! Server-persistent ordinary second-engine notices, distinct from guarded
//! commands. They never block, approve or automatically retry an action.

use orbyn_contracts::{AutopilotSecondEngineNotice, AutopilotSecondEngineNotices, RouteTier};

use super::{Page, ledger};
use crate::app::Core;
use crate::crew::filters::{Want, class_screen, screen};
use crate::error::{CoreError, Result};
use crate::store::AutopilotEvent;

impl Core {
    async fn notice_satisfied(&self, event: &AutopilotEvent) -> Option<(String, String)> {
        let family = event
            .body
            .pointer("/action/lead_family")
            .and_then(serde_json::Value::as_str)?;
        let Ok(Some(run)) = self.store.crew_run(&event.task_id) else {
            return None;
        };
        let Ok(lead) = self.store.thread(&run.lead_thread_id) else {
            return None;
        };
        let Ok(Some(brief)) = self.store.crew_brief(&event.task_id) else {
            return None;
        };
        let Ok(machine) = self.crew_machine(Some(&lead.project_id)).await else {
            return None;
        };
        let Ok(statuses) = self.engines.statuses(false).await else {
            return None;
        };
        let (_, classes) = self.crew_path_classes(&lead, &brief, false).await;
        let want = Want {
            writes: false,
            needs: vec![],
            image: false,
            tier: RouteTier::Lead,
        };
        let config = self.crew_router_config();
        machine
            .views
            .iter()
            .find(|view| {
                view.family != family
                    && screen(view, &want, None, &config).is_ok_and(|admitted| {
                        self.engines.driver(view.id.as_str()).is_ok_and(|driver| {
                            driver.approval_model(&admitted.model, None).is_some()
                        })
                    })
                    && class_screen(view, &classes, &machine.views).is_ok()
                    && self.engines.driver(view.id.as_str()).is_ok_and(|driver| {
                        driver.supports_approval()
                            && statuses
                                .iter()
                                .find(|status| status.id == view.id.as_str())
                                .is_some_and(|status| {
                                    driver.supports_approval_version(status.version.as_deref())
                                })
                    })
            })
            .map(|view| (view.id.as_str().to_owned(), view.family.clone()))
    }

    pub async fn autopilot_second_engine_notices(
        &self,
        before: Option<i64>,
        limit: Option<u32>,
    ) -> Result<AutopilotSecondEngineNotices> {
        self.crew_preview_only()?;
        let page = Page::new(limit, None)?;
        if before.is_some_and(|before| before <= 0) {
            return Err(CoreError::Invalid("that notice cursor is not valid".into()));
        }
        let mut events = self.store.crew_second_notices(before, page.limit + 1)?;
        let more = events.len() > page.limit as usize;
        if more {
            events.pop();
        }
        let next_before = if more {
            events.last().map(|event| event.id)
        } else {
            None
        };
        let mut items = Vec::new();
        for event in events {
            if let Some((engine, family)) = self.notice_satisfied(&event).await {
                self.store
                    .settle_crew_second_notice(event.id, Some((&engine, &family)))?;
                continue;
            }
            let key = self.store.crew_answer_key(&event.task_id)?;
            let needles = key
                .as_ref()
                .map(crate::crew::key_needles)
                .unwrap_or_default();
            let action = event.body.get("action");
            let text = |field| {
                action
                    .and_then(|action| action.get(field))
                    .and_then(serde_json::Value::as_str)
                    .map(|text| ledger::shown(text, &needles))
            };
            items.push(AutopilotSecondEngineNotice {
                id: event.id,
                task_id: event.task_id,
                at: event.at,
                target: text("target").unwrap_or_else(|| "this action".into()),
                lead_engine: text("lead_engine"),
                lead_family: text("lead_family"),
            });
        }
        Ok(AutopilotSecondEngineNotices { items, next_before })
    }

    pub fn autopilot_dismiss_second_engine_notice(&self, id: i64) -> Result<()> {
        self.crew_preview_only()?;
        self.store.settle_crew_second_notice(id, None)
    }
}
