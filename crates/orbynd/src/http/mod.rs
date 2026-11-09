//! Plumbing shared by every route: state, errors, JSON bodies, activity and access.

pub mod access;
mod activity;
mod detached;
mod error;
mod json;
mod query;
mod state;

pub use activity::note_activity;
pub use detached::to_the_end;
pub use error::{ApiFailure, ApiResult};
pub use json::ApiJson;
pub use query::QueryMap;
pub use state::{AppState, LOCAL_USER};
