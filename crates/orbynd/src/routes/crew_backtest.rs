//! The reviewer backtest over HTTP (Preview, slice 4n PR 3): the estimate, the start input and the report read, the start
//! and the stop are a person's. Transport only: `orbyn_core::crew::backtest` owns every rule.
//!
//! Both routers are merged by [`super::crew::routes`]: the reads under the Preview gate, the writes under the gate and
//! `local_only`, so a remote viewer reads and is refused the start and the stop. The writes are in
//! `orbyn_core::crew::changes::STATE_CHANGING`, so Autopilot refuses an agent's request to them by path. No route starts a run
//! by itself and nothing schedules one. No live engine is connected: a start is refused until one is.
//!
//! The estimate takes the lanes as ordered `lane`, `engine` and `model` triples (`lane=correctness&engine=a&model=m`, repeated).
//! Every lane in an estimate is a review lane. The start input (`start-view`) takes the project only and reads back the lanes
//! and per-engine budgets of the project's most recent run: nothing is defaulted, so a project that never ran one gets none.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use orbyn_contracts as wire;
use orbyn_core::CoreError;

use crate::http::{ApiJson, ApiResult, AppState, QueryMap};

type Pairs =
    Result<axum::extract::Query<Vec<(String, String)>>, axum::extract::rejection::QueryRejection>;

pub fn reads() -> Router<AppState> {
    Router::new()
        .route("/api/crew/backtest/estimate", get(estimate))
        .route("/api/crew/backtest/start-view", get(start_view))
        .route("/api/crew/backtest/{id}/report", get(report))
}

pub fn writes() -> Router<AppState> {
    Router::new()
        .route("/api/crew/backtest/start", post(start))
        .route("/api/crew/backtest/{id}/stop", post(stop))
}

/// How many replays a backtest of these lanes would make. Reads only; the counts are information and never change the
/// router's choice.
async fn estimate(State(state): State<AppState>, pairs: Pairs) -> ApiResult<impl IntoResponse> {
    // The flag comes before the query is read, so with it off every request is the absent route.
    state.core.crew_preview_state()?;
    let pairs = pairs
        .map(|axum::extract::Query(pairs)| pairs)
        .map_err(|_| CoreError::Invalid("the query is not valid".into()))?;
    let (project, lanes) = read_query(&pairs)?;
    Ok(Json(state.core.crew_backtest_estimate(&project, &lanes)?))
}

/// The lanes of the query in order: each `lane` opens one, and the `engine` and `model` after it (once each) fill it.
fn read_query(
    pairs: &[(String, String)],
) -> Result<(String, Vec<wire::BacktestLaneInput>), CoreError> {
    let mut project = None;
    // (lane, engine, model) as far as the query has said.
    let mut open: Vec<(String, Option<String>, Option<String>)> = Vec::new();
    for (name, value) in pairs {
        match name.as_str() {
            "project" if project.is_none() => project = Some(value.clone()),
            "project" => return Err(CoreError::Invalid("give project once".into())),
            "lane" => open.push((value.clone(), None, None)),
            "engine" | "model" => {
                let Some(last) = open.last_mut() else {
                    return Err(CoreError::Invalid(format!("give a lane before its {name}")));
                };
                let slot = if name == "engine" {
                    &mut last.1
                } else {
                    &mut last.2
                };
                if slot.replace(value.clone()).is_some() {
                    return Err(CoreError::Invalid(format!("give {name} once for a lane")));
                }
            }
            other => {
                return Err(CoreError::Invalid(format!(
                    "{other} is not a backtest estimate field"
                )));
            }
        }
    }
    let lanes = open
        .into_iter()
        .map(|(lane, engine, model)| {
            Ok(wire::BacktestLaneInput {
                lane,
                engine: engine
                    .ok_or_else(|| CoreError::Invalid("every lane has an engine".into()))?,
                model: model.ok_or_else(|| CoreError::Invalid("every lane has a model".into()))?,
                kind: wire::BacktestLaneKind::Review,
            })
        })
        .collect::<Result<Vec<_>, CoreError>>()?;
    let project = project.ok_or_else(|| CoreError::Invalid("project is required".into()))?;
    Ok((project, lanes))
}

/// The input a start request needs, read back from the project's most recent run: its lanes (with the kind each was
/// started as) and its per-engine token budgets. Reads only, and nothing is defaulted: a project that never ran a
/// backtest gets no lane and no budget, and a budget the run did not record reads unset.
async fn start_view(
    State(state): State<AppState>,
    query: QueryMap,
) -> ApiResult<impl IntoResponse> {
    let project = query
        .0
        .get("project")
        .cloned()
        .ok_or_else(|| CoreError::Invalid("project is required".into()))?;
    Ok(Json(state.core.crew_backtest_start_view(&project)?))
}

/// A run and each lane's score, or "Not measured" where the escape log cannot say. Reads only.
async fn report(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_backtest_report(&id)?))
}

/// A person starts a backtest with a token budget for each engine. One at a time; refused while another runs.
async fn start(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<wire::BacktestStartInput>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_backtest_start(&body).await?))
}

/// A person asks a running backtest to stop before its next replay.
async fn stop(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.crew_backtest_stop(&id)?))
}
