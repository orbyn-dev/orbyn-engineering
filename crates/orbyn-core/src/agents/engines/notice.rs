//! What an engine that never asks before acting (Cursor on the host) needs from the service beyond its one mode
//! (docs/cursor-thread-contract.md): the one-time notice, acknowledged once per project, and the Crew role.
//!
//! The notice is a flag and a date kept as a per-project setting. No secret is ever stored with it.

use orbyn_contracts::{EngineCrewRole, EngineNotice, PermissionMode};
use serde_json::json;

use super::EngineManager;
use crate::error::{CoreError, Result};
use crate::store::AuditRecord;

/// Engines that have a one-time notice. Every other engine answers 404 for it.
const NOTICE_ENGINES: &[&str] = &["cursor"];

/// The plain reason a never-asks engine may only review, until its confinement probe passes under the host prefix.
pub const REVIEWER_ONLY_REASON: &str = "Not yet confined for writing work";

fn setting_key(project_id: &str, engine: &str) -> String {
    format!("engine_notice:{engine}:{project_id}")
}

/// The Crew role of an engine with these modes, and why when it is limited. An engine that never asks cannot be
/// gated, so it reviews only until its confinement is proven.
pub fn crew_role(modes: &[PermissionMode]) -> (EngineCrewRole, Option<String>) {
    if crate::agents::permission_modes::never_asks(modes) {
        (
            EngineCrewRole::ReviewerOnly,
            Some(REVIEWER_ONLY_REASON.to_owned()),
        )
    } else {
        (EngineCrewRole::Any, None)
    }
}

impl EngineManager {
    /// Whether `project_id` has acknowledged `engine`'s notice. `NotFound` for an engine with no notice or a project that
    /// does not exist.
    pub fn notice(&self, project_id: &str, engine: &str) -> Result<EngineNotice> {
        if !NOTICE_ENGINES.contains(&engine) {
            return Err(CoreError::NotFound(format!("notice for engine {engine}")));
        }
        self.store.project(project_id)?;
        let at = self.store.setting(&setting_key(project_id, engine))?;
        Ok(EngineNotice {
            engine: engine.to_owned(),
            project_id: project_id.to_owned(),
            acknowledged: at.is_some(),
            acknowledged_at: at,
        })
    }

    /// The person's acknowledgement. One audit row the first time; acknowledging again changes nothing and writes nothing.
    pub fn acknowledge_notice(
        &self,
        project_id: &str,
        engine: &str,
        actor: &str,
    ) -> Result<EngineNotice> {
        let current = self.notice(project_id, engine)?;
        if current.acknowledged {
            return Ok(current);
        }
        // The row comes first, so it always precedes the effect and a failed write stops it.
        self.store.append_audit(&AuditRecord {
            actor: actor.to_owned(),
            action: "engine_notice.acknowledge".into(),
            target: engine.to_owned(),
            decision: None,
            source: "user".into(),
            thread_id: None,
            run_id: None,
            detail: json!({ "project_id": project_id }),
        })?;
        self.store
            .set_setting(&setting_key(project_id, engine), &crate::util::now())?;
        self.notice(project_id, engine)
    }

    /// The refusal a run start on a never-asks engine gives an unacknowledged project, if it is one.
    pub(crate) fn notice_required(&self, project_id: &str, engine: &str) -> Result<()> {
        if !NOTICE_ENGINES.contains(&engine) || self.notice(project_id, engine)?.acknowledged {
            return Ok(());
        }
        Err(CoreError::EngineRefused {
            code: "engine_notice_required",
            message: format!(
                "Read and accept the {engine} notice for this project before the first run."
            ),
            fields: json!({ "engine": engine, "project_id": project_id }),
        })
    }
}
