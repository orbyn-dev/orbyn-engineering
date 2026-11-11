//! The fix loop: a confirmed finding sends a fix brief to a builder. The brief is written by the core and goes through
//! the same checks as `crew_start` (shape, secrets, protected paths, the key's text, a reviewer never writes), by the
//! same function. Its key is the reviewed task's own checks, plus the lead's repro check when that is what confirmed the
//! finding, so the fix must make the repro pass; the key is never sent. The builder may write the finding's file and
//! nothing else. When the fix passes, the lanes run again (see `ended`). The same finding after a fix stops the loop: one
//! fix brief per finding, and a later round that reports it again is still open and is not fixed a second time.

use std::sync::Arc;

use orbyn_contracts::{CrewCheck, CrewError, CrewObject, Thread};
use serde_json::{Value, json};

use crate::app::Core;
use crate::crew::guard::{key_needles, refuse_leak};
use crate::crew::leases::internal;
use crate::crew::review::RULES_FILE;
use crate::crew::stamp::Stamper;
use crate::crew::start::{Launch, Links};
use crate::crew::tool::invalid;
use crate::store::{CrewFix, CrewTask, StoredFinding};

impl Core {
    /// Sends a fix brief for a confirmed finding, or stops the loop when this finding already had one.
    pub(super) async fn start_fix(
        self: &Arc<Self>,
        thread: &Thread,
        stamper: &Stamper,
        found: &StoredFinding,
        subject: &CrewTask,
        repro: Option<CrewCheck>,
    ) -> Result<Value, CrewError> {
        let earlier = self.store.crew_fixes_of(&subject.id).map_err(internal)?;
        if earlier.iter().any(|f| f.fingerprint == found.fingerprint) {
            self.tell_lead_text(
                &subject.id,
                &format!(
                    "The same finding came back after a fix ({} at {}:{}), so the loop stops. A person decides.",
                    crate::crew::wire_name(&found.finding.severity),
                    found.finding.path,
                    found.finding.line
                ),
            );
            return Ok(json!({ "started": false, "stopped": "repeated_finding" }));
        }
        let subject_brief = self
            .store
            .crew_brief(&subject.id)
            .map_err(internal)?
            .ok_or_else(|| invalid("the task has no brief"))?;
        let subject_key = self
            .store
            .crew_answer_key(&subject.id)
            .map_err(internal)?
            .ok_or_else(|| invalid("the task has no key"))?;
        let finding = &found.finding;
        let claim: String = finding.claim.as_str().chars().take(1000).collect();
        let outcome = format!(
            "Fix a {} finding from the {} review at {}:{}: {claim} Change only {} and keep all other behaviour.",
            crate::crew::wire_name(&finding.severity),
            found.lane,
            finding.path,
            finding.line,
            finding.path
        );
        let mut checks: Vec<Value> = subject_key
            .checks
            .iter()
            .map(|c| serde_json::to_value(c).map_err(|e| invalid(e.to_string())))
            .collect::<Result<_, _>>()?;
        if let Some(mut repro) = repro {
            if subject_key.checks.iter().any(|c| c.id == repro.id) {
                repro.id =
                    orbyn_contracts::CrewId::new(format!("{}-fix", repro.id)).map_err(invalid)?;
            }
            checks.push(serde_json::to_value(&repro).map_err(|e| invalid(e.to_string()))?);
        }
        let mut protected: Vec<Value> = subject_key
            .protected_paths
            .iter()
            .map(|p| json!(p))
            .collect();
        protected.push(json!(RULES_FILE));
        let draft = |with_inputs: bool| {
            json!({
                "kind": "implement",
                "outcome": outcome,
                "inputs": if with_inputs { json!([{ "kind": "text", "text": finding.evidence }]) } else { json!([]) },
                "files": [{ "path": finding.path, "mode": "write" }],
                "allowed_effects": ["read_repo", "write_leased", "run_checks"],
                "stop_when": ["The finding no longer holds and the checks pass."],
                "budget": { "turns": subject_brief.budget.turns, "minutes": subject_brief.budget.minutes },
                "refs": [finding.id],
            })
        };
        let crew_needles = self.crew_key_needles(&subject.crew_id)?;
        // What the reviewer quoted may hold a check's text. Such evidence is left out of the brief, not the brief refused.
        let mut brief = stamper.brief(&draft(true))?;
        let mut key = stamper.answer_key(
            &json!({ "checks": checks, "protected_paths": protected }),
            &brief.id,
        )?;
        let mut needles = crew_needles.clone();
        needles.extend(key_needles(&key));
        if refuse_leak(&CrewObject::Brief(brief.clone()).to_value(), &needles).is_err() {
            brief = stamper.brief(&draft(false))?;
            key = stamper.answer_key(
                &json!({ "checks": checks, "protected_paths": protected }),
                &brief.id,
            )?;
            needles = crew_needles;
            needles.extend(key_needles(&key));
        }
        refuse_leak(&CrewObject::Brief(brief.clone()).to_value(), &needles)?;
        let fix = CrewFix {
            finding_id: finding.id.as_str().to_owned(),
            subject_id: subject.id.clone(),
            fix_task_id: brief.id.as_str().to_owned(),
            fingerprint: found.fingerprint.clone(),
        };
        let started = self
            .launch_crew_task(Launch {
                lead: thread,
                now: stamper.now.clone(),
                brief,
                key,
                role: None,
                lane: None,
                author_engine: None,
                prefix: "worker",
                prompt: None,
                links: Links {
                    review: None,
                    fix: Some(fix),
                },
            })
            .await;
        Ok(match started {
            Ok(mut answer) => {
                answer["started"] = json!(true);
                answer
            }
            Err(error) => json!({ "started": false, "error": error }),
        })
    }
}
