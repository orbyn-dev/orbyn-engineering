//! A saved, live lead publication claim is the only special Network context.
use super::{
    chain::{Outcome, Summary, decide},
    hard::{context, refuse_in},
    review::Ground,
    tier::Need,
    tripwire::{injection_in, injection_in_value},
};
use crate::{
    app::Core,
    error::{CoreError, Result},
    governance::approvals::{ActionAsk, Review, RunScope},
    store::PublicationState,
    util::{sha256_hex, unix_now},
};
use orbyn_contracts::ActionKind;
use serde_json::json;
use std::{path::PathBuf, sync::Arc};

impl Core {
    pub(super) async fn review_comment_action(
        self: &Arc<Self>,
        scope: &RunScope,
        ask: &ActionAsk,
    ) -> Review {
        match self.admit_saved_comment(scope, ask).await {
            Ok(review) => review,
            Err(_) => {
                Review::Deny("The saved Crew review comment claim is unavailable or stale.".into())
            }
        }
    }
    async fn admit_saved_comment(
        self: &Arc<Self>,
        scope: &RunScope,
        ask: &ActionAsk,
    ) -> Result<Review> {
        let bad = || CoreError::Invalid("invalid review comment claim".into());
        self.crew_preview_only()?;
        self.publication_live(scope)?;
        let id = ask.detail["crew_review_comment"].as_str().ok_or_else(bad)?;
        let mut row = self.store.review_publication(id)?.ok_or_else(bad)?;
        let thread = self.store.thread(&scope.thread_id)?;
        let mode = self.crew_autopilot_mode(&row.project, &row.crew);
        let expected = json!({"crew_review_comment":row.id,"claim":row.claim,"scope_digest":row.admission_digest(),"revision":row.revision,"body_sha256":row.body_sha256,"head":row.target.head});
        if ask.kind != ActionKind::Network
            || !ask.paths.is_empty()
            || scope.plan
            || row.state != PublicationState::Admitting
            || row.claim.is_none()
            || row.expires <= unix_now()
            || row.caller_run.as_deref() != Some(scope.run_id.as_str())
            || row.caller_thread.as_deref() != Some(scope.thread_id.as_str())
            || row.crew != scope.thread_id
            || row.project != thread.project_id
            || row.mode.as_deref() != Some(mode.name())
            || ask.target != row.target.resource()
            || ask.detail != expected
            || sha256_hex(row.body.as_bytes()) != row.body_sha256
        {
            return Err(bad());
        }
        let task = self
            .store
            .crew_task(&row.subject)?
            .filter(|t| t.crew_id == row.crew && t.state == "passed")
            .ok_or_else(bad)?;
        let run = self.store.crew_run(&row.subject)?.ok_or_else(bad)?;
        let brief = self.store.crew_brief(&row.subject)?.ok_or_else(bad)?;
        let key = self.store.crew_answer_key(&row.subject)?.ok_or_else(bad)?;
        let needles = crate::crew::key_needles(&key);
        crate::crew::refuse_secret(&json!({"body":row.body,"target":row.target}))
            .map_err(|_| bad())?;
        crate::crew::refuse_leak(&json!({"body":row.body,"target":row.target}), &needles)
            .map_err(|_| bad())?;
        let cwd = PathBuf::from(self.store.project(&row.project)?.path);
        let ground = Ground::read(self, &cwd, run.base_commit.as_deref()).await;
        let scene = ground.scene(&key.protected_paths);
        let ctx = context(ask.kind, &ask.target, &ask.paths, &scene);
        if refuse_in(&ctx).is_some()
            || injection_in(&ask.summary).is_some()
            || injection_in(&ask.target).is_some()
            || injection_in_value(&ask.detail).is_some()
        {
            return Err(bad());
        }
        let summary=Summary {kind:"network".into(),target:ask.target.clone(),paths:vec![],brief:super::ledger::shown(&brief.outcome,&needles),
            effects:vec!["same_repository_advisory_review_comment".into()],tier:"two_keys".into(),
            reasons:vec!["Export the exact saved lane summary to its pinned same-repository PR; never a vote, merge or cross-repository message.".into()]};
        let approvers = self
            .crew_autopilot
            .approvers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let outcome = decide(mode, Need::TwoKeys, &summary, &approvers).await;
        let (review, decided_by) = match outcome {
            Outcome::Allow { decided_by, .. } => (Review::Approved, decided_by),
            Outcome::Person { .. } => (Review::Person, "chain_person_or_explicit_policy"),
            Outcome::Deny {
                reason, decided_by, ..
            } => (Review::Deny(reason), decided_by),
        };
        let event = if review == Review::Approved {
            "allowed"
        } else if review == Review::Person {
            "person"
        } else {
            "denied"
        };
        self.autopilot_record(&task,event,&json!({"action":{"kind":"network","target":ask.target},"publication":row.id,"decided_by":decided_by,"mode":mode.name(),"body_sha256":row.body_sha256}),&needles)?;
        self.store.update_review_publication(
            &mut row,
            PublicationState::Admitting,
            Some(decided_by.into()),
            None,
        )?;
        Ok(review)
    }
}
