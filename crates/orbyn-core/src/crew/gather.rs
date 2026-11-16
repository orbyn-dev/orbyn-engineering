//! The router's view of this machine, read the way the engine picker reads it: the
//! registry's statuses, each engine's model list (custom models added, hidden ones removed)
//! and its own limit report. Nothing here is typed or sent by the lead, so a brief cannot
//! name an engine, a model or a limit the machine does not have.

use serde_json::Value;

use super::view::EngineView;
use crate::app::Core;
use crate::error::Result;

/// The views, and the ids left out because a decision cannot name them (not a protocol token).
pub struct Machine {
    pub views: Vec<EngineView>,
    pub left_out: Vec<LeftOut>,
    pub default_engine: Option<String>,
}

/// An engine the router cannot name in a decision, with the reason.
#[derive(Clone, Debug, serde::Serialize)]
pub struct LeftOut {
    pub engine: String,
    pub why: String,
}

impl Core {
    pub async fn crew_machine(&self, project_id: Option<&str>) -> Result<Machine> {
        let statuses = self.engines.statuses(false).await?;
        let off = self.preference("disabled_engines", project_id)?;
        let off = off.as_array().cloned().unwrap_or_default();
        let mut views = Vec::new();
        let mut left_out = Vec::new();
        for status in &statuses {
            let turned_off = off.iter().any(|id| id.as_str() == Some(status.id.as_str()));
            match EngineView::from_status(status, turned_off) {
                Some(view) => views.push(view),
                None => left_out.push(LeftOut {
                    engine: status.id.clone(),
                    why: "Its id is not a lowercase protocol token, so a decision cannot name it."
                        .to_owned(),
                }),
            }
        }
        // Only an engine that passes the cheap filters is asked for its models. Limits come
        // from the cache only: asking an engine for them can start its app-server, and a dry
        // run starts no engine. No cached report counts as not reported.
        let reads = views.into_iter().map(|view| async move {
            if !view.worth_reading() {
                return view;
            }
            let id = view.id.as_str().to_owned();
            let view = match self.engine_models(&id, false).await {
                Ok(models) => view.with_models(&models),
                Err(_) => EngineView {
                    models: Some(Vec::new()),
                    ..view
                },
            };
            match self.cached_limits(&id) {
                Some(limits) => view.with_limits(&limits),
                None => view,
            }
        });
        let views = futures_util::future::join_all(reads).await;
        let default_engine = match self.preference("default_engine", project_id)? {
            Value::String(id) if !id.is_empty() => Some(id),
            _ => None,
        };
        Ok(Machine {
            views,
            left_out,
            default_engine,
        })
    }
}
