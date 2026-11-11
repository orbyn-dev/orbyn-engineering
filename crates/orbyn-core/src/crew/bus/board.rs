//! The shared board: typed facts keyed by name, each write on the revision the writer read
//! (SPEC.md H12). The first write on a revision stands; a stale one is refused and the lead is
//! told. A fact is checked for secrets and for the answer key on the way in (H2, H1), and again
//! on the way out to a worker, since a key added later can share text with an older fact.

use std::sync::Arc;

use orbyn_contracts::{CrewError, CrewHash, CrewId, CrewObject, CrewRole, ErrorCode};
use serde_json::{Value, json};

use super::{BOARD_READ_TOOL, BOARD_WRITE_TOOL};
use crate::app::Core;
use crate::crew::guard::{refuse_leak, refuse_secret};
use crate::crew::leases::internal;
use crate::crew::stamp::Stamper;
use crate::crew::tool::{invalid, stamp_now};
use crate::error::CoreError;
use crate::store::FactWrite;
use crate::tools::mcp::McpBinding;

impl Core {
    /// `crew_board_read`: the revision that stands for `key`, or null.
    pub async fn crew_board_read_tool(
        self: &Arc<Self>,
        binding: &McpBinding,
        args: &Value,
    ) -> Result<Value, CrewError> {
        let who = self.bus_sender(binding, BOARD_READ_TOOL)?;
        let Some(fields) = args.as_object() else {
            return Err(invalid("arguments: an object"));
        };
        if let Some(extra) = fields.keys().find(|key| key.as_str() != "key") {
            return Err(invalid(format!(
                "{extra}: not an argument of crew_board_read"
            )));
        }
        let key = fields
            .get("key")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("key: required"))?;
        orbyn_contracts::CrewFactKey::new(key).map_err(|why| invalid(format!("key: {why}")))?;
        let fact = self
            .store
            .crew_fact(who.crew_id.as_str(), key)
            .map_err(internal)?;
        let Some(fact) = fact else {
            return Ok(json!({ "fact": null }));
        };
        let value = CrewObject::Fact(fact).to_value();
        if !who.is_lead() {
            refuse_leak(&value, &self.crew_key_needles(who.crew_id.as_str())?)?;
        }
        Ok(json!({ "fact": value }))
    }

    /// `crew_board_write`. The lead names the task it writes for with `task` when more than
    /// one is live; a worker writes for its own.
    pub async fn crew_board_write_tool(
        self: &Arc<Self>,
        binding: &McpBinding,
        args: &Value,
    ) -> Result<Value, CrewError> {
        let who = self.bus_sender(binding, BOARD_WRITE_TOOL)?;
        let limits = self.crew_limits();
        let Some(fields) = args.as_object() else {
            return Err(invalid("arguments: an object"));
        };
        let mut draft = fields.clone();
        let named = draft.remove("task");
        let _bus = self.crew_bus.lock.lock().await;
        let crew = who.crew_id.as_str();
        let members = self.store.crew_tasks_in_crew(crew).map_err(internal)?;
        // H5: a board write is a post, so rewriting a key is no way round the limit.
        if let Some(own) = &who.task {
            let posted = self
                .store
                .count_crew_posts(own, who.member.as_str())
                .map_err(internal)?;
            if posted >= limits.posts {
                let error = CrewError::new(
                    ErrorCode::PostLimitExceeded,
                    format!("{} reached {} posts in this run", who.member, limits.posts),
                );
                self.tell_lead_of(&who, &error);
                return Err(error);
            }
        }
        let live = |state: &str| !crate::crew::lifecycle::is_final(state);
        let task = match (&who.task, named) {
            (Some(own), None) => own.clone(),
            (Some(_), Some(_)) => return Err(invalid("task: only the lead names a task")),
            (None, Some(Value::String(id))) => members
                .iter()
                .find(|(task, _)| task.id == id)
                .filter(|(task, _)| live(&task.state))
                .map(|(task, _)| task.id.clone())
                .ok_or_else(|| CrewError::new(ErrorCode::RunEnded, "that task has no live run"))?,
            (None, Some(_)) => return Err(invalid("task: the id of a task")),
            (None, None) => {
                let mut open = members.iter().filter(|(task, _)| live(&task.state));
                match (open.next(), open.next()) {
                    (Some((task, _)), None) => task.id.clone(),
                    (None, _) => return Err(CrewError::new(ErrorCode::RunEnded, "no run is live")),
                    _ => return Err(invalid("task: name the task this fact belongs to")),
                }
            }
        };
        let (task_row, _) = members
            .iter()
            .find(|(row, _)| row.id == task)
            .ok_or_else(|| invalid("task: not in this crew"))?;
        let run = CrewId::new(task_row.run_id.clone()).map_err(invalid)?;
        let stamper = Stamper {
            crew_id: who.crew_id.clone(),
            engine: who.engine.clone(),
            member: who.member.clone(),
            now: stamp_now()?,
        };
        let fact = stamper.fact(&Value::Object(draft), &run, who.role)?;
        if fact.rev > limits.revisions {
            let error = invalid(format!(
                "{} is at its limit of {} revisions; the lead decides",
                fact.key, limits.revisions
            ));
            if who.role == CrewRole::Worker {
                self.tell_lead_of(&who, &error);
            }
            return Err(error);
        }
        let value_bytes = orbyn_contracts::canonical_json(&fact.value)?.len();
        if value_bytes > limits.fact_bytes {
            return Err(invalid(format!(
                "value: at most {} bytes, not {value_bytes}",
                limits.fact_bytes
            )));
        }
        let object = CrewObject::Fact(fact.clone()).to_value();
        // H2, then H1 (a fact is shown to every worker of the crew).
        refuse_secret(&object)?;
        refuse_leak(&object, &self.crew_key_needles(crew)?)?;
        let evidence: Vec<String> = fact
            .evidence
            .iter()
            .map(CrewHash::as_str)
            .map(str::to_owned)
            .collect();
        if !self
            .store
            .crew_has_evidence(crew, &evidence)
            .map_err(internal)?
        {
            return Err(invalid(
                "evidence: names an id the core has not recorded in this crew",
            ));
        }
        if fact.base_rev == 0
            && self
                .store
                .crew_fact(crew, fact.key.as_str())
                .map_err(internal)?
                .is_none()
            && self.store.crew_facts(crew).map_err(internal)?.len() >= limits.board_keys
        {
            return Err(invalid(format!(
                "key: the board holds {} keys already",
                limits.board_keys
            )));
        }
        let written = self
            .store
            .add_crew_fact(&task, &fact)
            .map_err(|error| match error {
                CoreError::RunClosed => CrewError::new(ErrorCode::RunEnded, "this run has ended"),
                other => internal(other),
            })?;
        match written {
            FactWrite::Stored => Ok(json!({ "id": fact.id, "rev": fact.rev })),
            FactWrite::Stale { current } => {
                let error = CrewError::new(
                    ErrorCode::BoardConflict,
                    format!(
                        "{} is at revision {current}, {} wrote on {}; the first value stands and the lead decides",
                        fact.key, who.member, fact.base_rev
                    ),
                );
                if who.role == CrewRole::Worker {
                    self.tell_lead_of(&who, &error);
                }
                Err(error)
            }
        }
    }
}
