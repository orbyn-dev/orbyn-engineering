//! The HTTP routes, one module per area. Each module owns its route list and the
//! handlers behind it; this one merges them and applies the credential, activity and
//! origin layers.

pub mod account;
mod agent_mcp;
mod auth;
mod automations;
mod checkpoints;
mod composer;
mod crew;
mod engines;
mod explorer;
mod files;
mod governance;
mod interactions;
mod mcp_apps;
mod media;
mod preview;
mod projects;
mod pulls;
mod raw_files;
mod renders;
mod runs;
mod secrets;
mod settings;
mod shell;
mod skills;
mod source_control;
mod stream;
mod terminal;
mod threads;
mod ui;
mod updates;
pub mod usage;
mod workspace;

#[cfg(test)]
mod account_approval_tests;
#[cfg(test)]
mod account_logging_tests;
#[cfg(test)]
mod account_tests;
#[cfg(test)]
mod crew_background_tests;
#[cfg(test)]
mod crew_guard_tests;
#[cfg(test)]
mod crew_held_tests;
#[cfg(test)]
mod crew_tests;
#[cfg(test)]
mod secrets_tests;
#[cfg(test)]
mod tests;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware::from_fn_with_state;

use crate::http::access;
use crate::http::{AppState, note_activity};

const BODY_LIMIT: usize = 4 * 1024 * 1024;

pub fn router(state: AppState) -> Router {
    let access = state.access.clone();
    let protected = Router::new()
        .merge(account::routes(state.clone()))
        .merge(crew::routes(state.clone()))
        .merge(media::routes())
        .merge(settings::routes())
        .merge(secrets::routes(state.clone()))
        .merge(projects::routes())
        .merge(threads::routes())
        .merge(workspace::routes())
        .merge(shell::routes())
        .merge(updates::routes())
        .merge(composer::routes())
        .merge(checkpoints::routes())
        .merge(explorer::routes())
        .merge(files::routes())
        .merge(raw_files::routes())
        .merge(mcp_apps::routes())
        .merge(renders::routes())
        .merge(source_control::routes())
        .merge(pulls::routes())
        .merge(usage::routes())
        .merge(engines::routes())
        .merge(automations::routes())
        .merge(preview::routes())
        .merge(runs::routes())
        .merge(interactions::routes())
        .merge(skills::routes())
        .merge(governance::routes())
        .merge(stream::routes())
        .merge(terminal::routes())
        .route_layer(from_fn_with_state(state.clone(), note_activity))
        .route_layer(from_fn_with_state(access.clone(), access::require_auth));
    Router::new()
        .merge(protected)
        .merge(auth::routes())
        .merge(automations::public_routes())
        .merge(agent_mcp::routes())
        .fallback(ui::serve)
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .layer(from_fn_with_state(access, access::guard_origin))
        .with_state(state)
}
