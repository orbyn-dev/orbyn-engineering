//! Query-string helpers.

use std::collections::HashMap;

use axum::extract::Query;

pub type QueryMap = Query<HashMap<String, String>>;
