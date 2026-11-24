//! Crew lessons over HTTP (Crew Preview, KC5 slice 3; docs/crew-lessons.md). Two reads, [`routes`]: the list for one
//! repository (`repo` required, `filter` one of active, disabled, hidden or all, active by default) and one lesson with its
//! evidence ids. They are class Local, behind the Preview gate, and listed in `orbyn_core::crew::changes::READS`: a lesson
//! is data in a brief, never a rule, and nothing read here feeds a gate. The person's writes, [`writes`], add, edit, switch
//! on and off, promote, demote, delete with its 10 s Undo, and the per-repository "use lessons" switch. They are `Person`
//! and enforced (the desktop shell's key; browser mode and the CLI get `403 desktop_required`), behind `local_only`, and in
//! `orbyn_core::crew::changes::STATE_CHANGING`, so an agent's request is refused by path. Merged into `crew::routes` before
//! its Preview layer, so with the flag off every route answers as a route that is not there.
//!
//! Transport only: `orbyn_core::crew::lessons_person` builds every view and holds the rules; the store's one transaction
//! keeps the audit row and the change together. A write from an older screen is `409 stale`; a scan that finds a secret is
//! `400 secret_refused` with the core's exact sentence; an unknown lesson or repository is `404 not_found`; and a refusal the
//! contract calls `invalid` is `400 invalid`.

use axum::Json;
use axum::Router;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, patch, post, put};
use orbyn_contracts as wire;
use orbyn_core::CoreError;
use orbyn_core::crew::lessons::LessonScope as CoreScope;
use orbyn_core::crew::lessons_view::{LessonDetailView, LessonFilter, LessonListView, LessonView};
use orbyn_core::store::CrewLesson;
use serde::Deserialize;

use crate::http::{ApiFailure, ApiJson, ApiResult, AppState};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/crew/lessons", get(list))
        .route("/api/crew/lessons/{id}", get(detail))
}

/// The person's writes. Merged into the `local_only` block of `crew::routes`.
pub fn writes() -> Router<AppState> {
    Router::new()
        .route("/api/crew/lessons", post(add))
        // A static path, so the detail read's `{id}` never sees it: the method router picks the more specific route.
        .route("/api/crew/lessons/settings", put(settings))
        .route("/api/crew/lessons/{id}", patch(edit).delete(remove))
        .route("/api/crew/lessons/{id}/disable", post(disable))
        .route("/api/crew/lessons/{id}/enable", post(enable))
        .route("/api/crew/lessons/{id}/undo", post(undo))
        .route("/api/crew/lessons/{id}/promote", post(promote))
        .route("/api/crew/lessons/{id}/demote", post(demote))
}

/// `repo` (required), `filter` (active, disabled, hidden or all; active when absent).
#[derive(Deserialize)]
struct ListQuery {
    repo: Option<String>,
    filter: Option<wire::LessonFilter>,
}

/// `repo` (optional: a repository lesson ignores it, a global one answers for it, and without it a global lesson's
/// `in_next_brief` is false).
#[derive(Deserialize)]
struct DetailQuery {
    repo: Option<String>,
}

/// `rev` (required), the version the screen showed.
#[derive(Deserialize)]
struct RevQuery {
    rev: Option<u32>,
}

/// A query the extractor refused: an unknown filter, a bad percent escape or a repeated key is one `400 invalid`.
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
/// and code (`stale`, `secret_refused`, `not_found`).
fn lesson<T>(result: orbyn_core::Result<T>) -> ApiResult<T> {
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
    let filter = query.filter.map_or(LessonFilter::Active, core_filter);
    let view = lesson(state.core.crew_lessons_list(repo, filter))?;
    Ok(Json(list_view(view)))
}

async fn detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
    query: QueryArg<DetailQuery>,
) -> ApiResult<impl IntoResponse> {
    let query = read_query(query)?;
    let view = lesson(state.core.crew_lesson_detail(&id, query.repo.as_deref()))?;
    Ok(Json(detail_view(view)))
}

async fn add(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<wire::LessonAdd>,
) -> ApiResult<impl IntoResponse> {
    let scope = core_scope(body.scope, body.repo)?;
    let written = lesson(state.core.crew_lesson_add(scope, &body.text))?;
    Ok((StatusCode::CREATED, Json(answer_lesson(&state, &written)?)))
}

async fn edit(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<wire::LessonEdit>,
) -> ApiResult<impl IntoResponse> {
    let written = lesson(state.core.crew_lesson_edit(&id, &body.text, body.rev))?;
    Ok(Json(answer_lesson(&state, &written)?))
}

async fn disable(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<wire::LessonRev>,
) -> ApiResult<impl IntoResponse> {
    let written = lesson(state.core.crew_lesson_set_enabled(&id, false, body.rev))?;
    Ok(Json(answer_lesson(&state, &written)?))
}

async fn enable(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<wire::LessonRev>,
) -> ApiResult<impl IntoResponse> {
    let written = lesson(state.core.crew_lesson_set_enabled(&id, true, body.rev))?;
    Ok(Json(answer_lesson(&state, &written)?))
}

async fn promote(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<wire::LessonRev>,
) -> ApiResult<impl IntoResponse> {
    let written = lesson(state.core.crew_lesson_promote(&id, body.rev))?;
    Ok(Json(answer_lesson(&state, &written)?))
}

async fn demote(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<wire::LessonDemote>,
) -> ApiResult<impl IntoResponse> {
    let written = lesson(state.core.crew_lesson_demote(&id, &body.repo, body.rev))?;
    Ok(Json(answer_lesson(&state, &written)?))
}

/// `DELETE /api/crew/lessons/{id}?rev=N`: the 10 s Undo's first half answers the id and the instant the window ends.
async fn remove(
    State(state): State<AppState>,
    Path(id): Path<String>,
    query: QueryArg<RevQuery>,
) -> ApiResult<impl IntoResponse> {
    let query = read_query(query)?;
    let rev = query.rev.ok_or_else(|| invalid("rev is required"))?;
    let (id, undo_until) = lesson(state.core.crew_lesson_delete(&id, rev))?;
    Ok(Json(wire::LessonDeleted { id, undo_until }))
}

async fn undo(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let written = lesson(state.core.crew_lesson_undo(&id))?;
    Ok(Json(answer_lesson(&state, &written)?))
}

async fn settings(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<wire::LessonSettings>,
) -> ApiResult<impl IntoResponse> {
    let answer = wire::LessonSettingsView {
        repo: body.repo.clone(),
        use_lessons: body.use_lessons,
    };
    lesson(
        state
            .core
            .crew_lessons_set_use(&body.repo, body.use_lessons),
    )?;
    Ok(Json(answer))
}

fn core_scope(scope: wire::LessonScope, repo: Option<String>) -> ApiResult<CoreScope> {
    match scope {
        wire::LessonScope::Repo => repo
            .filter(|repo| !repo.is_empty())
            .map(CoreScope::Repo)
            .ok_or_else(|| invalid("repo is required for a repository lesson")),
        wire::LessonScope::Global => Ok(CoreScope::Global),
    }
}

fn core_filter(filter: wire::LessonFilter) -> LessonFilter {
    match filter {
        wire::LessonFilter::Active => LessonFilter::Active,
        wire::LessonFilter::Disabled => LessonFilter::Disabled,
        wire::LessonFilter::Hidden => LessonFilter::Hidden,
        wire::LessonFilter::All => LessonFilter::All,
    }
}

/// One written lesson as the contract shows it. A repository lesson's `in_next_brief` is the brief's own choice for its
/// repository; a global one's needs a repository the write did not name, so it answers false, as a detail without `repo`.
fn answer_lesson(state: &AppState, written: &CrewLesson) -> ApiResult<wire::Lesson> {
    let view = lesson(state.core.crew_lesson_detail(&written.id, None))?;
    Ok(lesson_of(view.lesson))
}

fn lesson_of(view: LessonView) -> wire::Lesson {
    let LessonView {
        lesson,
        in_next_brief,
        skill,
    } = view;
    let evidence = lesson
        .evidence
        .iter()
        .map(|evidence| wire::LessonEvidence {
            kind: evidence.kind.clone(),
            id: evidence.id.clone(),
        })
        .collect();
    wire::Lesson {
        id: lesson.id,
        scope: if lesson.scope == "global" {
            wire::LessonScope::Global
        } else {
            wire::LessonScope::Repo
        },
        repo: lesson.project_id,
        source: lesson.source,
        text: lesson.text,
        state: lesson.state,
        hidden_by_cap: lesson.hidden_by_cap,
        names_path: lesson.names_path,
        edited_by_person: lesson.edited_by_person,
        uses: lesson.uses,
        left_out_of: lesson.left_out_of,
        skill,
        evidence,
        created_at: lesson.created_at,
        updated_at: lesson.updated_at,
        last_used_at: lesson.last_used_at,
        rev: lesson.rev,
        in_next_brief,
    }
}

fn list_view(view: LessonListView) -> wire::LessonList {
    wire::LessonList {
        repo: view.repo,
        use_lessons: view.use_lessons,
        brief_cap: wire::LessonBriefCap {
            lessons: u32::try_from(view.brief_cap_lessons).unwrap_or(u32::MAX),
            chars: u32::try_from(view.brief_cap_chars).unwrap_or(u32::MAX),
        },
        counts: wire::LessonCounts {
            active: view.counts.0,
            disabled: view.counts.1,
            hidden: view.counts.2,
        },
        lessons: view.lessons.into_iter().map(lesson_of).collect(),
    }
}

fn detail_view(view: LessonDetailView) -> wire::LessonDetail {
    wire::LessonDetail {
        lesson: lesson_of(view.lesson),
        evidence: view
            .evidence
            .into_iter()
            .map(|evidence| wire::LessonEvidence {
                kind: evidence.kind,
                id: evidence.id,
            })
            .collect(),
    }
}
