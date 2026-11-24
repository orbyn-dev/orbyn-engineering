//! Crew skill proposals over HTTP (Crew Preview, KC9; docs/crew-lessons.md). The read, [`routes`], lists one repository's
//! proposals in every state, newest first, each with its source lessons' own current evidence ids. It is class Local and
//! listed in `orbyn_core::crew::changes::READS`: a proposal is data, never a rule, and nothing read here feeds a gate. The
//! person's writes, [`writes`], approve one open proposal (which writes the local skill) or dismiss it. They are `Person`
//! and enforced (the desktop shell's key; browser mode and the CLI get `403 desktop_required`), behind `local_only`, and in
//! `orbyn_core::crew::changes::STATE_CHANGING`, so an agent's request is refused by path. Merged into `crew::routes` before
//! its Preview layer, so with the flag off every route answers as a route that is not there.
//!
//! Transport only: `orbyn_core::crew::proposals` builds every view and holds the rules; the store's one transaction keeps
//! each audit row and its change together. A write from an older screen is `409 stale`; a proposal that is no longer open
//! is `409 not_open`; a skill name already held is `409 name_taken`; a scan that finds a secret is `400 secret_refused`
//! with the core's exact sentence; an unknown proposal or repository is `404 not_found`; and a refusal the contract calls
//! `invalid` is `400 invalid`.

use axum::Json;
use axum::Router;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use orbyn_contracts as wire;
use orbyn_core::CoreError;
use orbyn_core::crew::proposals::{ProposalLessonView, ProposalView};
use serde::Deserialize;

use crate::http::{ApiFailure, ApiJson, ApiResult, AppState};

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/crew/skill-proposals", get(list))
}

/// The person's writes. Merged into the `local_only` block of `crew::routes`.
pub fn writes() -> Router<AppState> {
    Router::new()
        .route("/api/crew/skill-proposals/{id}/approve", post(approve))
        .route("/api/crew/skill-proposals/{id}/dismiss", post(dismiss))
}

/// `repo` (required): the project whose proposals are read.
#[derive(Deserialize)]
struct ListQuery {
    repo: Option<String>,
}

/// A query the extractor refused: a bad percent escape or a repeated key is one `400 invalid`.
type QueryArg<T> = Result<Query<T>, QueryRejection>;

fn read_query<T>(query: QueryArg<T>) -> ApiResult<T> {
    query
        .map(|Query(value)| value)
        .map_err(|_| invalid("the query is not valid"))
}

fn invalid(message: &str) -> ApiFailure {
    ApiFailure::new(StatusCode::BAD_REQUEST, "invalid", message)
}

/// The contract's `invalid` code for a core refusal whose own code is `invalid_input`; every other error keeps its status
/// and code (`stale`, `not_open`, `name_taken`, `secret_refused`, `not_found`).
fn map_core<T>(result: orbyn_core::Result<T>) -> ApiResult<T> {
    result.map_err(|error| match error {
        CoreError::Invalid(message) => invalid(&message),
        other => other.into(),
    })
}

async fn list(
    State(state): State<AppState>,
    query: QueryArg<ListQuery>,
) -> ApiResult<impl IntoResponse> {
    let query = read_query(query)?;
    let repo = query
        .repo
        .as_deref()
        .ok_or_else(|| invalid("repo is required"))?;
    let views = map_core(state.core.crew_skill_proposals(repo))?;
    let mut proposals = Vec::with_capacity(views.len());
    for view in views {
        proposals.push(proposal_of(&state, view)?);
    }
    Ok(Json(wire::SkillProposalList { proposals }))
}

async fn approve(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<wire::SkillProposalApprove>,
) -> ApiResult<impl IntoResponse> {
    let view = map_core(
        state
            .core
            .crew_skill_proposal_approve(&id, &body.text, body.rev),
    )?;
    Ok(Json(proposal_of(&state, view)?))
}

async fn dismiss(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<wire::SkillProposalDismiss>,
) -> ApiResult<impl IntoResponse> {
    let view = map_core(state.core.crew_skill_proposal_dismiss(&id, body.rev))?;
    Ok(Json(proposal_of(&state, view)?))
}

fn state_of(state: &str) -> wire::SkillProposalState {
    match state {
        "approved" => wire::SkillProposalState::Approved,
        "dismissed" => wire::SkillProposalState::Dismissed,
        _ => wire::SkillProposalState::Open,
    }
}

/// One proposal as the contract shows it: its source lessons with their own current evidence, and the skill once approved.
fn proposal_of(state: &AppState, view: ProposalView) -> ApiResult<wire::SkillProposal> {
    let ProposalView {
        proposal,
        can_approve,
    } = view;
    let lessons = map_core(state.core.crew_skill_proposal_lessons(&proposal.lessons))?;
    Ok(wire::SkillProposal {
        id: proposal.id,
        repo: proposal.project_id,
        state: state_of(&proposal.state),
        text: proposal.text,
        lessons: lessons
            .into_iter()
            .map(
                |ProposalLessonView { id, evidence }| wire::SkillProposalLesson {
                    id,
                    evidence: evidence
                        .into_iter()
                        .map(|evidence| wire::LessonEvidence {
                            kind: evidence.kind,
                            id: evidence.id,
                        })
                        .collect(),
                },
            )
            .collect(),
        can_approve,
        skill: proposal
            .skill_name
            .map(|name| wire::SkillProposalSkill { name }),
        rev: proposal.rev,
        created_at: proposal.created_at,
        approved_at: proposal.approved_at,
    })
}
