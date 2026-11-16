//! Lead-requested advisory export, independent of reviewer completion and merge authority.
use crate::crew::{
    guard::{refuse_leak, refuse_secret},
    leases::internal,
    tool::invalid,
};
use crate::source_control::{
    forge::{
        crew::CrewRepository,
        crew_review::{self, Reconciled},
    },
    git::{git_hardened, text},
};
use crate::{
    app::Core,
    error::{CoreError, Result},
    governance::approvals::{ActionAsk, RunScope},
    store::{Publication, PublicationState, Store},
    tools::mcp::McpBinding,
    util::{new_id, sha256_hex, unix_now},
};
use orbyn_contracts::{ActionKind, CheckKind, CheckOutcome, CrewError};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    task: String,
    review: Option<String>,
    pull: Option<u64>,
    publication: Option<String>,
    renew: Option<String>,
}

/// Constructed only after an internal durable claim. No wire deserializer can mint it.
pub(in crate::crew) struct PendingReviewComment {
    row: Publication,
}
impl PendingReviewComment {
    fn claimed(row: Publication) -> Self {
        Self { row }
    }
    fn ask(&self) -> ActionAsk {
        ActionAsk {
            kind: ActionKind::Network,
            target: self.row.target.resource(),
            paths: vec![],
            summary: format!(
                "Advisory {} review round {} for PR {}, reviewed head {}",
                self.row.lane, self.row.round, self.row.target.number, self.row.target.head
            ),
            detail: json!({"crew_review_comment":self.row.id,"claim":self.row.claim,"scope_digest":self.row.admission_digest(),"revision":self.row.revision,"body_sha256":self.row.body_sha256,"head":self.row.target.head}),
        }
    }
}
struct AdmissionGuard {
    store: Arc<Store>,
    id: String,
    claim: String,
}
impl Drop for AdmissionGuard {
    fn drop(&mut self) {
        let _ = self.store.release_review_publication(&self.id, &self.claim);
    }
}
fn unavailable() -> CoreError {
    CoreError::Invalid("review publication context is stale or unavailable".into())
}
impl Core {
    pub(in crate::crew) fn publication_live(&self, scope: &RunScope) -> Result<()> {
        let run = self.store.run(&scope.run_id)?;
        let thread = self.store.thread(&scope.thread_id)?;
        if run.thread_id != scope.thread_id
            || run.state.is_terminal()
            || self.store.publication_stop_requested(&scope.run_id)?
            || thread.engine != scope.engine
        {
            return Err(unavailable());
        }
        Ok(())
    }
    fn publication_scan(&self, row: &Publication) -> Result<()> {
        let value = json!({"body":row.body,"task_uri":row.task_uri,"target":row.target});
        refuse_secret(&value).map_err(|e| CoreError::Invalid(e.message))?;
        refuse_leak(
            &value,
            &self
                .crew_key_needles(&row.crew)
                .map_err(|e| CoreError::Invalid(e.message))?,
        )
        .map_err(|e| CoreError::Invalid(e.message))?;
        if sha256_hex(row.body.as_bytes()) != row.body_sha256 {
            return Err(unavailable());
        }
        Ok(())
    }
    async fn publication_snapshot(&self, row: &Publication) -> Result<(PathBuf, bool)> {
        let thread = self.store.thread(&row.crew)?;
        if thread.project_id != row.project {
            return Err(unavailable());
        }
        let cwd = PathBuf::from(self.store.project(&row.project)?.path);
        if text(&git_hardened(&cwd, &["remote", "get-url", "origin"]).await?) != row.origin {
            return Err(unavailable());
        }
        let repo = CrewRepository::from_remote(&row.origin)?;
        let (target, author, open) =
            crew_review::receipt_snapshot(&repo, &cwd, row.target.number).await?;
        if target.github != row.target.github
            || target.host != row.target.host
            || target.repository != row.target.repository
            || target.repository_id != row.target.repository_id
            || author != row.expected_author
        {
            return Err(unavailable());
        }
        Ok((cwd, open && target == row.target))
    }
    pub(super) async fn publish_review_tool(
        self: &Arc<Self>,
        binding: &McpBinding,
        args: &Value,
    ) -> std::result::Result<Value, CrewError> {
        let request: Request =
            serde_json::from_value(args.clone()).map_err(|e| invalid(e.to_string()))?;
        let (thread, _) = self.review_lead(binding, "crew_publish_review")?;
        self.publication_live(&binding.scope).map_err(internal)?;
        let (subject, _) = self.review_subject(&thread, &request.task)?;
        let mut row = if let Some(id) = request.publication {
            if request.review.is_some() || request.pull.is_some() || request.renew.is_some() {
                return Err(invalid("publication excludes review and pull"));
            }
            self.store
                .review_publication(&id)
                .map_err(internal)?
                .filter(|r| {
                    r.subject == subject.id && r.crew == thread.id && r.project == thread.project_id
                })
                .ok_or_else(|| invalid("no such publication in this crew"))?
        } else {
            self.prepare_review_publication(
                &thread,
                &subject,
                &request
                    .review
                    .ok_or_else(|| invalid("review is required"))?,
                request.pull.ok_or_else(|| invalid("pull is required"))?,
                request.renew.as_deref(),
            )
            .await
            .map_err(internal)?
        };
        self.execute_review_publication(binding, &mut row)
            .await
            .map_err(internal)?;
        let renewable = self
            .store
            .review_publication_renewable(&row)
            .map_err(internal)?;
        Ok(
            json!({"publication":row.id,"task":row.subject,"review":row.review,"state":row.state,"body_sha256":row.body_sha256,"decided_by":row.decision,"receipt":row.receipt,"task_uri":row.task_uri,"renewable":renewable,"replaces":row.replaces}),
        )
    }
    async fn prepare_review_publication(
        &self,
        thread: &orbyn_contracts::Thread,
        subject: &crate::store::CrewTask,
        review: &str,
        pull: u64,
        renew: Option<&str>,
    ) -> Result<Publication> {
        if subject.state != "passed" {
            return Err(unavailable());
        }
        let relation = self
            .store
            .crew_review(review)?
            .filter(|r| r.subject_id == subject.id && r.role == "review")
            .ok_or_else(unavailable)?;
        let task = self
            .store
            .crew_task(review)?
            .filter(|t| t.state == "passed")
            .ok_or_else(unavailable)?;
        let latest = self
            .store
            .crew_reviews_of(&subject.id)?
            .into_iter()
            .filter(|r| r.role == "review" && r.lane == relation.lane)
            .map(|r| r.round)
            .max();
        if latest != Some(relation.round) {
            return Err(unavailable());
        }
        let evidence = self
            .verification_evidence(&task)
            .map_err(|e| CoreError::Invalid(e.message))?;
        let key = self
            .store
            .crew_answer_key(review)?
            .ok_or_else(unavailable)?;
        // Only a lane begun on a clean commit can describe the PR snapshot. A
        // dirty-baseline file_scope review is valid locally, but cannot be exported.
        if !key.checks.iter().any(|c| {
            c.id.as_str() == "chk-read-only"
                && c.kind == CheckKind::DiffSize
                && c.params["max_files"] == 0
        }) || !evidence.iter().any(|e| {
            e.check.as_str() == "chk-read-only"
                && e.outcome == CheckOutcome::Pass
                && e.commit.as_str() == relation.commit
        }) {
            return Err(unavailable());
        }
        let cwd = PathBuf::from(self.store.project(&thread.project_id)?.path);
        let origin = text(&git_hardened(&cwd, &["remote", "get-url", "origin"]).await?);
        let repo = CrewRepository::from_remote(&origin)?;
        let (target, author) = crew_review::snapshot(&repo, &cwd, pull).await?;
        let subject_run = self.store.crew_run(&subject.id)?.ok_or_else(unavailable)?;
        if target.head != relation.commit
            || target.head
                != self
                    .review_commit(subject, &cwd)
                    .await
                    .map_err(|e| CoreError::Invalid(e.message))?
            || subject_run.base_commit.as_deref() != Some(target.base.as_str())
        {
            return Err(unavailable());
        }
        let operation = new_id();
        let task_uri = format!("orbyn://crew/task/{}", subject.id.replace(':', "%3A"));
        if crate::host::shell::link_route(&task_uri).is_none() {
            return Err(unavailable());
        }
        let status = self
            .review_status(&subject.id)
            .map_err(|e| CoreError::Invalid(e.message))?;
        let route = self
            .store
            .crew_route_decisions(review)?
            .pop()
            .ok_or_else(unavailable)?;
        let same = route
            .reasons
            .iter()
            .any(|r| r.code.as_str() == "same_engine_as_author");
        let mut body = format!(
            "Advisory Crew review — {} round {}\n\nReviewed head: `{}`\nBase: `{}`\nTask: `{}`\nReview run: `{}`\nEngine: {} / {}{}\n\nTask locator (copy into the owning Orbyn installation; requires its local task data):\n`{task_uri}`\n\nFindings below are advisory. This comment is not a forge approval or a proof of correctness.\n",
            relation.lane,
            relation.round,
            target.head,
            target.base,
            subject.id,
            review,
            route.engine.as_str(),
            route.model.as_str(),
            if same {
                " — Same engine as author"
            } else {
                ""
            }
        );
        let findings: Vec<_> = status["findings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|f| f["lane"] == relation.lane && f["round"] == relation.round)
            .collect();
        let mut included = 0;
        for finding in &findings {
            let line = format!(
                "\n- {}:{} [{}; {}]: {}\n",
                finding["path"].as_str().unwrap_or(""),
                finding["line"],
                finding["check"].as_str().unwrap_or("unchecked"),
                finding["status"].as_str().unwrap_or("unknown"),
                finding["claim"].as_str().unwrap_or("")
            );
            if body.len() + line.len() > 15_000 {
                break;
            }
            body.push_str(&line);
            included += 1;
        }
        body.push_str(&format!("\n{} findings; {} omitted from this bounded summary.\n<!-- orbyn-review:{operation} -->\n",findings.len(),findings.len()-included));
        if body.len() > 16 * 1024 {
            return Err(unavailable());
        }
        let body_sha256 = sha256_hex(body.as_bytes());
        let scope_digest = sha256_hex(
            serde_json::to_string(&json!([
                thread.project_id,
                target.host,
                target.repository_id,
                pull,
                target.head,
                review,
                relation.lane,
                relation.round
            ]))?
            .as_bytes(),
        );
        let row = Publication {
            id: operation,
            subject: subject.id.clone(),
            review: review.into(),
            project: thread.project_id.clone(),
            crew: thread.id.clone(),
            lane: relation.lane,
            round: relation.round,
            target,
            origin,
            expected_author: author,
            task_uri,
            body,
            body_sha256,
            scope_digest,
            replaces: renew.map(str::to_owned),
            ever_sending: false,
            expires: unix_now() + 600,
            revision: 0,
            state: PublicationState::Draft,
            caller_run: None,
            caller_thread: None,
            claim: None,
            mode: None,
            decision: None,
            receipt: None,
        };
        self.publication_scan(&row)?;
        self.store.add_review_publication(&row)
    }
    async fn execute_review_publication(
        self: &Arc<Self>,
        binding: &McpBinding,
        row: &mut Publication,
    ) -> Result<()> {
        use PublicationState::*;
        self.publication_scan(row)?;
        if matches!(row.state, Sending | Unknown) {
            return self.reconcile_review_publication(row).await;
        }
        if row.state == Admitting {
            self.store.recover_review_publication_admission(row)?;
        }
        if row.state != Draft {
            return Ok(());
        }
        if row.expires <= unix_now() {
            return self.store.update_review_publication(row, Stale, None, None);
        }
        let (_, valid) = self.publication_snapshot(row).await?;
        if !valid {
            return self.store.update_review_publication(row, Stale, None, None);
        }
        let mode = self.crew_autopilot_mode(&row.project, &row.crew).name();
        self.store.claim_review_publication(
            row,
            &binding.scope.run_id,
            &binding.scope.thread_id,
            mode,
        )?;
        let admission_revision = row.revision;
        let admission_claim = row.claim.clone();
        let _guard = AdmissionGuard {
            store: self.store.clone(),
            id: row.id.clone(),
            claim: row.claim.clone().ok_or_else(unavailable)?,
        };
        let ask = PendingReviewComment::claimed(row.clone()).ask();
        let verdict = self.gate.request(&binding.scope, ask).await;
        *row = self
            .store
            .review_publication(&row.id)?
            .ok_or_else(unavailable)?;
        if !verdict.is_allowed() {
            return self.store.update_review_publication(
                row,
                Draft,
                Some("effect_gate_denied".into()),
                None,
            );
        }
        // An unbound gate/account-only Pass is never publication authority. The
        // narrow Crew branch must have visited and recorded this exact claim.
        if row.state != Admitting
            || row.claim != admission_claim
            || row.revision <= admission_revision
            || row.decision.is_none()
        {
            return Err(unavailable());
        }
        self.publication_live(&binding.scope)?;
        let latest = self
            .store
            .crew_reviews_of(&row.subject)?
            .into_iter()
            .filter(|r| r.role == "review" && r.lane == row.lane)
            .map(|r| r.round)
            .max();
        let (cwd, valid) = self.publication_snapshot(row).await?;
        if !valid
            || latest != Some(row.round)
            || row.expires <= unix_now()
            || self.crew_autopilot_mode(&row.project, &row.crew).name() != mode
        {
            return self.store.update_review_publication(row, Stale, None, None);
        }
        self.publication_scan(row)?;
        let decision = format!(
            "effect_gate_allow_after_{}",
            row.decision.as_deref().ok_or_else(unavailable)?
        );
        self.store
            .update_review_publication(row, Sending, Some(decision), None)?;
        match crew_review::send(&row.target, &cwd, &row.body).await {
            Ok(comment) => match crew_review::verify(
                &row.target,
                &cwd,
                comment,
                row.expected_author,
                &row.body,
            )
            .await
            {
                Ok(receipt) => {
                    let current = self
                        .publication_snapshot(row)
                        .await
                        .is_ok_and(|(_, valid)| valid);
                    self.store.update_review_publication(
                        row,
                        if current { Published } else { NeedsPerson },
                        None,
                        Some(receipt),
                    )
                }
                Err(_) => self
                    .store
                    .update_review_publication(row, Unknown, None, None),
            },
            Err(_) => self
                .store
                .update_review_publication(row, Unknown, None, None),
        }
    }
    async fn reconcile_review_publication(&self, row: &mut Publication) -> Result<()> {
        // Identity/auth/head loss never triggers a replacement POST.
        let (cwd, valid) = match self.publication_snapshot(row).await {
            Ok(value) => value,
            Err(_) => {
                return self.store.update_review_publication(
                    row,
                    PublicationState::Unknown,
                    None,
                    None,
                );
            }
        };
        match crew_review::reconcile(
            &row.target,
            &cwd,
            &format!("<!-- orbyn-review:{} -->", row.id),
            row.expected_author,
            &row.body,
        )
        .await
        {
            Ok(Reconciled::Exact(receipt)) => self.store.update_review_publication(
                row,
                if valid {
                    PublicationState::Published
                } else {
                    PublicationState::NeedsPerson
                },
                None,
                Some(receipt),
            ),
            Ok(Reconciled::Ambiguous) => {
                self.store
                    .update_review_publication(row, PublicationState::NeedsPerson, None, None)
            }
            _ => self
                .store
                .update_review_publication(row, PublicationState::Unknown, None, None),
        }
    }
}
