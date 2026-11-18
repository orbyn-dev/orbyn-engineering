//! Person-owned review controls. HTTP supplies no actor or authorization flag;
//! no model tool can answer consent or waive a required lane.

use std::sync::Arc;

use orbyn_contracts::{AuthState, CrewError, CrewToken, RouteDecision, RouteRole, Thread};
use serde_json::{Value, json};

use super::lanes::Run;
use super::rules::read_rules_at;
use crate::app::Core;
use crate::crew::autopilot::Mode;
use crate::crew::guard::{refuse_leak, refuse_secret};
use crate::crew::leases::internal;
use crate::crew::router::{RouteFailure, RouteInput, route};
use crate::crew::score::EvalHistory;
use crate::crew::tool::{invalid, stamp_now};
use crate::error::{CoreError, Result};
use crate::store::{CrewReview, ReviewConsent, ReviewControl};

fn core_error(error: CrewError) -> CoreError {
    CoreError::Invalid(error.message)
}

fn same_engine(decision: &RouteDecision) -> bool {
    decision
        .reasons
        .iter()
        .any(|r| r.code.as_str() == "same_engine_as_author")
}

impl Core {
    pub(crate) fn review_lane_removed(&self, subject: &str, lane: &str) -> Result<bool> {
        Ok(self
            .store
            .crew_review_controls(subject)?
            .iter()
            .any(|c| c.kind == "remove_lane" && c.state == "removed" && c.lane == lane))
    }

    /// UI's local-person boundary. Stored scope is returned for display, never
    /// accepted back from the client as a grant.
    pub fn crew_review_controls(&self, subject: &str) -> Result<Value> {
        self.crew_preview_only()?;
        let controls = self.store.crew_review_controls(subject)?;
        Ok(json!({"task":subject,"controls":controls}))
    }

    /// Once or wait, for the exact current request. Allow starts a new separate
    /// reviewer run through normal admission; it never reopens a finished run.
    pub async fn crew_review_consent(
        self: &Arc<Self>,
        subject: &str,
        body: &Value,
    ) -> Result<Value> {
        self.crew_preview_only()?;
        let fields = body
            .as_object()
            .ok_or_else(|| CoreError::Invalid("an object is required".into()))?;
        if fields
            .keys()
            .any(|k| !["request", "choice"].contains(&k.as_str()))
        {
            return Err(CoreError::Invalid(
                "only request and choice are accepted".into(),
            ));
        }
        let id = body["request"]
            .as_str()
            .ok_or_else(|| CoreError::Invalid("request is required".into()))?;
        let allow = match body["choice"].as_str() {
            Some("once") => true,
            Some("wait") => false,
            _ => return Err(CoreError::Invalid("choice is once or wait".into())),
        };
        let request = self
            .store
            .crew_review_controls(subject)?
            .into_iter()
            .find(|c| c.id == id && c.kind == "same_engine")
            .ok_or_else(|| CoreError::NotFound("review consent".into()))?;
        let run = self
            .store
            .crew_run(subject)?
            .ok_or_else(|| CoreError::NotFound("crew run".into()))?;
        let thread = self.store.thread(&run.lead_thread_id)?;
        let (task, _) = self.review_subject(&thread, subject).map_err(core_error)?;
        let latest = self
            .store
            .crew_reviews_of(subject)?
            .iter()
            .filter(|r| r.role == "review" && r.lane == request.lane)
            .map(|r| r.round)
            .max()
            .unwrap_or(0);
        let head = crate::source_control::git::git_hardened(
            std::path::Path::new(&thread.cwd),
            &["rev-parse", "HEAD"],
        )
        .await?;
        if request.round != latest + 1
            || request.commit != crate::source_control::git::text(&head)
            || self.review_lane_removed(subject, &request.lane)?
        {
            return Err(CoreError::Invalid("this review consent is stale".into()));
        }
        let answered = self
            .store
            .answer_review_consent(id, allow, crate::util::unix_now())?;
        if !allow {
            return Ok(
                json!({"task":subject,"subject":subject,"request":id,"state":"wait","consent":answered,"started":false}),
            );
        }
        let stamper = self
            .review_stamper(&thread, &thread.engine)
            .map_err(core_error)?;
        let mut answer = self
            .start_review_run(
                &thread,
                &stamper,
                &task,
                &CrewToken::new(request.lane).map_err(CoreError::Invalid)?,
                &request.commit,
                Run::Lane,
            )
            .await
            .map_err(core_error)?;
        answer["subject"] = json!(subject);
        answer["request"] = json!(id);
        answer["started"] = json!(answer["worker"].is_string() && answer["routed"] == true);
        Ok(answer)
    }

    /// A person may waive an unlocked required lane for this subject. The rule
    /// comes from its pinned base; a dirty rule file cannot loosen it.
    pub async fn crew_remove_review_lane(&self, subject: &str, body: &Value) -> Result<Value> {
        self.crew_preview_only()?;
        let fields = body
            .as_object()
            .ok_or_else(|| CoreError::Invalid("an object is required".into()))?;
        if fields
            .keys()
            .any(|k| !["lane", "reason"].contains(&k.as_str()))
        {
            return Err(CoreError::Invalid(
                "only lane and reason are accepted".into(),
            ));
        }
        let lane = CrewToken::new(body["lane"].as_str().unwrap_or_default())
            .map_err(CoreError::Invalid)?;
        let reason = body["reason"].as_str().unwrap_or_default().trim();
        if reason.is_empty() || reason.chars().count() > 1000 {
            return Err(CoreError::Invalid("reason is 1 to 1000 characters".into()));
        }
        refuse_secret(body).map_err(core_error)?;
        let _held = self.crew_starts.lock().await;
        let run = self
            .store
            .crew_run(subject)?
            .ok_or_else(|| CoreError::NotFound("crew run".into()))?;
        let thread = self.store.thread(&run.lead_thread_id)?;
        self.review_subject(&thread, subject).map_err(core_error)?;
        refuse_leak(
            body,
            &self.crew_key_needles(&thread.id).map_err(core_error)?,
        )
        .map_err(core_error)?;
        let base = run
            .base_commit
            .ok_or_else(|| CoreError::Invalid("the task has no pinned base".into()))?;
        let rules = read_rules_at(std::path::Path::new(&thread.cwd), &base)
            .await
            .map_err(CoreError::Invalid)?;
        if rules.iter().any(|r| r.lane == lane && r.locked) {
            return Err(CoreError::Invalid("a locked lane cannot be removed".into()));
        }
        if !rules.iter().any(|r| r.lane == lane)
            && ![super::CODE_LANE, super::SECURITY_LANE].contains(&lane.as_str())
        {
            return Err(CoreError::Invalid("this is not a required lane".into()));
        }
        self.store.remove_review_lane(
            subject,
            lane.as_str(),
            &base,
            reason,
            crate::util::unix_now(),
        )?;
        let receipt = self
            .store
            .crew_review_controls(subject)?
            .into_iter()
            .find(|r| r.kind == "remove_lane" && r.state == "removed" && r.lane == lane.as_str())
            .ok_or_else(|| CoreError::Internal("the lane removal receipt is missing".into()))?;
        Ok(
            json!({"task":subject,"receipt":orbyn_contracts::CrewLaneRemovalReceipt {id:receipt.id,lane:receipt.lane,reason:receipt.reason},"controls":self.store.crew_review_controls(subject)?}),
        )
    }

    /// Before creating a review task, discover the actual candidate using the
    /// same pure router and filters. No refused round is created just to ask.
    pub(super) async fn review_consent_needed(
        &self,
        thread: &Thread,
        brief: &orbyn_contracts::Brief,
        review: &CrewReview,
        author_engine: Option<CrewToken>,
    ) -> std::result::Result<Option<ReviewControl>, CrewError> {
        if review.role != "review" {
            return Ok(None);
        }
        if self
            .review_lane_removed(&review.subject_id, &review.lane)
            .map_err(internal)?
        {
            return Err(invalid("this required lane was removed by a person"));
        }
        let (_, classes) = self.crew_path_classes(thread, brief, false).await;
        let machine = self
            .crew_machine(Some(&thread.project_id))
            .await
            .map_err(internal)?;
        let lane = CrewToken::new(review.lane.clone()).map_err(invalid)?;
        // The same locked-lane answer as the real launch, so preflight and launch route a lane to the same tier.
        let base = self
            .store
            .crew_run(&review.subject_id)
            .map_err(internal)?
            .and_then(|run| run.base_commit);
        let lane_keeps_lead = self
            .lane_keeps_lead(
                std::path::Path::new(&thread.cwd),
                base.as_deref(),
                Some(&review.subject_id),
                &lane,
            )
            .await;
        let input = RouteInput {
            brief,
            run_id: orbyn_contracts::CrewId::new(crate::util::new_id()).map_err(invalid)?,
            created_at: stamp_now()?,
            role: Some(RouteRole::Review),
            lane: Some(lane),
            lane_keeps_lead,
            author_engine,
            lead_engine: CrewToken::new(thread.engine.clone()).ok(),
            default_engine: machine.default_engine.clone(),
            classes: Some(&classes),
        };
        let decision = match route(
            &input,
            &machine.views,
            &self.crew_router_config_with_mappings()?,
            &EvalHistory::default(),
        ) {
            Ok(d) => d,
            Err(RouteFailure::Invalid(e)) => return Err(e),
            // The actual start path records ordinary route refusals as before.
            Err(RouteFailure::Refused(_)) => return Ok(None),
        };
        if !same_engine(&decision) {
            return Ok(None);
        }
        // Preflight persists a request before normal route persistence runs, so
        // apply the same secret/key boundary before storing candidate metadata.
        let shown = orbyn_contracts::CrewObject::RouteDecision(decision.clone()).to_value();
        refuse_secret(&shown)?;
        refuse_leak(&shown, &self.crew_key_needles(&thread.id)?)?;
        if self.full_security_fallback(thread, review, &decision, &machine.views) {
            return Ok(None);
        }
        let scope = ReviewConsent {
            subject: &review.subject_id,
            lane: &review.lane,
            commit: &review.commit,
            round: review.round,
            engine: decision.engine.as_str(),
            model: decision.model.as_str(),
        };
        let control = self
            .store
            .request_review_consent(&scope, crate::util::unix_now())
            .map_err(internal)?;
        if control.state == "allowed" && control.actor == "person" {
            Ok(None)
        } else {
            Ok(Some(control))
        }
    }

    fn full_security_fallback(
        &self,
        thread: &Thread,
        review: &CrewReview,
        decision: &RouteDecision,
        views: &[crate::crew::EngineView],
    ) -> bool {
        let family = views
            .iter()
            .find(|v| v.id == decision.engine)
            .map(|v| v.family.as_str());
        review.role == "review"
            && review.lane == super::SECURITY_LANE
            && self.crew_autopilot_mode(&thread.project_id, &thread.id) == Mode::Full
            && family.is_some_and(|f| {
                !views.iter().any(|v| {
                    v.family != f
                        && v.installed
                        && matches!(
                            v.auth,
                            AuthState::LoggedIn | AuthState::NotApplicable | AuthState::Unknown
                        )
                })
            })
    }

    /// Actual start admission, under crew_starts. This must run before the route
    /// is stored or an engine is launched. Preflight alone never grants authority.
    pub(in crate::crew) async fn admit_review_route(
        &self,
        thread: &Thread,
        review: Option<&CrewReview>,
        decision: &mut RouteDecision,
        views: &[crate::crew::EngineView],
        now: i64,
    ) -> std::result::Result<(), CrewError> {
        let Some(review) = review else {
            return Ok(());
        };
        if self
            .review_lane_removed(&review.subject_id, &review.lane)
            .map_err(internal)?
        {
            return Err(invalid("this required lane was removed by a person"));
        }
        if !same_engine(decision) {
            return Ok(());
        }
        if review.role != "review" || decision.role != RouteRole::Review {
            return Err(invalid(
                "an independent finding check cannot use the author's vendor",
            ));
        }
        let head = crate::source_control::git::git_hardened(
            std::path::Path::new(&thread.cwd),
            &["rev-parse", "HEAD"],
        )
        .await
        .map_err(internal)?;
        if crate::source_control::git::text(&head) != review.commit {
            return Err(invalid("this same-engine review names a stale head"));
        }
        if self.full_security_fallback(thread, review, decision, views) {
            return stamp_same_engine_authority(
                decision,
                "same_engine_full_security",
                "Autopilot Full: separate lead-tier Security review, no other vendor signed in; High risk.",
            );
        }
        let scope = ReviewConsent {
            subject: &review.subject_id,
            lane: &review.lane,
            commit: &review.commit,
            round: review.round,
            engine: decision.engine.as_str(),
            model: decision.model.as_str(),
        };
        if !self
            .store
            .consume_review_consent(&scope, now)
            .map_err(internal)?
        {
            return Err(invalid(
                "same-engine review needs the person's current once consent",
            ));
        }
        stamp_same_engine_authority(
            decision,
            "same_engine_person_once",
            "The person allowed this exact same-engine review once.",
        )
    }

    /// Persistent route reasons are the badge source. Same-engine Security raises
    /// risk to High (human policy decision, 16 November 2026); it never lowers it.
    pub fn review_security_requires_high(&self, subject: &str) -> Result<bool> {
        for review in self
            .store
            .crew_reviews_of(subject)?
            .iter()
            .filter(|r| r.role == "review" && r.lane == super::SECURITY_LANE)
        {
            if self
                .store
                .crew_route_decisions(&review.task_id)?
                .iter()
                .any(same_engine)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Execution-path refresh for a lane that completed after queueing. Views
    /// remain read-only; the durable queue and after-merge window retain High.
    pub(in crate::crew) fn refresh_merge_review_risk(
        &self,
        entry: &crate::store::MergeEntry,
    ) -> Result<crate::crew::Risk> {
        if self.review_security_requires_high(&entry.task_id)? {
            self.store.raise_merge_review_risk(
                &entry.task_id,
                "Same engine as author: Security review raises risk to High.",
            )?;
            Ok(crate::crew::Risk::High)
        } else {
            Ok(crate::crew::Risk::parse(&entry.risk).unwrap_or(crate::crew::Risk::High))
        }
    }
}

fn stamp_same_engine_authority(
    decision: &mut RouteDecision,
    code: &str,
    text: &str,
) -> std::result::Result<(), CrewError> {
    if decision.reasons.len() >= 32 {
        let optional = decision
            .reasons
            .iter()
            .rposition(|r| r.code.as_str().starts_with("score_"))
            .ok_or_else(|| {
                invalid("the route has no room for its same-engine authorization record")
            })?;
        decision.reasons.remove(optional);
    }
    decision.reasons.push(orbyn_contracts::RouteReason {
        code: CrewToken::new(code).map_err(invalid)?,
        text: crate::crew::verify::short(text)?,
    });
    decision.id = orbyn_contracts::CrewObject::RouteDecision(decision.clone()).computed_id()?;
    decision.validate()
}
