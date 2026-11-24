//! Command blocks in a terminal (`docs/command-blocks.md`): two local reads and the
//! Person-class clear. A thread, terminal or block that is gone answers `404`.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use orbyn_contracts::{BlockClear, BlockList};
use orbyn_core::CoreError;
use orbyn_core::host::blocks::{
    DEFAULT_OUTPUT_BYTES, DEFAULT_PAGE, MAX_OUTPUT_BYTES, MAX_PAGE, TerminalBlocks, limits,
};
use serde_json::json;

use crate::http::{ApiResult, AppState, LOCAL_USER};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/threads/{id}/terminals/{terminal}/blocks",
            get(list).delete(clear),
        )
        .route(
            "/api/threads/{id}/terminals/{terminal}/blocks/{block_id}/output",
            get(output),
        )
}

/// The terminal's recorder, or `None` when this service keeps no blocks. A missing
/// thread or terminal is the same `404` as the other terminal reads.
fn recorder(
    state: &AppState,
    thread_id: &str,
    terminal_id: &str,
) -> ApiResult<Option<Arc<TerminalBlocks>>> {
    state.core.store.thread(thread_id)?;
    let session = state
        .core
        .terminals
        .named(thread_id, terminal_id)
        .ok_or_else(|| CoreError::NotFound("terminal".into()))?;
    Ok(session.blocks())
}

fn page_limit(query: &HashMap<String, String>, default: usize, max: usize) -> usize {
    query
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
        .clamp(1, max)
}

/// `GET /api/threads/{id}/terminals/{terminal}/blocks?before=&limit=`.
pub async fn list(
    State(state): State<AppState>,
    Path((thread_id, terminal_id)): Path<(String, String)>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<impl IntoResponse> {
    let Some(blocks) = recorder(&state, &thread_id, &terminal_id)? else {
        return Ok(Json(BlockList {
            blocks: Vec::new(),
            next_before: None,
            limits: limits(),
        }));
    };
    let limit = page_limit(&query, DEFAULT_PAGE, MAX_PAGE);
    Ok(Json(
        blocks.list(query.get("before").map(String::as_str), limit)?,
    ))
}

/// `DELETE /api/threads/{id}/terminals/{terminal}/blocks`: drops this terminal's finished
/// blocks and their kept output, keeping a running block. The audit row is written first.
pub async fn clear(
    State(state): State<AppState>,
    Path((thread_id, terminal_id)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    let Some(blocks) = recorder(&state, &thread_id, &terminal_id)? else {
        return Ok(Json(BlockClear {
            cleared: 0,
            kept_running: 0,
        }));
    };
    let kept_running = blocks.running();
    let outcome = blocks.clear_with_audit(|counts| {
        state
            .core
            .record_user_audit(
                LOCAL_USER,
                "terminal.blocks.clear",
                &terminal_id,
                &thread_id,
                json!({ "blocks": counts.blocks, "bytes": counts.bytes }),
            )
            .map(|_| ())
    })?;
    Ok(Json(BlockClear {
        cleared: outcome.blocks,
        kept_running: u8::from(kept_running),
    }))
}

/// `GET /api/threads/{id}/terminals/{terminal}/blocks/{block_id}/output?after=&limit=`.
pub async fn output(
    State(state): State<AppState>,
    Path((thread_id, terminal_id, block_id)): Path<(String, String, String)>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<impl IntoResponse> {
    let Some(blocks) = recorder(&state, &thread_id, &terminal_id)? else {
        return Err(CoreError::NotFound("block".into()).into());
    };
    let limit = page_limit(&query, DEFAULT_OUTPUT_BYTES, MAX_OUTPUT_BYTES);
    Ok(Json(blocks.output(
        &block_id,
        query.get("after").map(String::as_str),
        limit,
    )?))
}
