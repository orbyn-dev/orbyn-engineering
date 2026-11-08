//! What a thread's engine has available, for the Tools tab and Settings. The engine is
//! asked (a CLI status command or a short app-server), what its last run reported is added,
//! and the answer is kept for half a minute so a tab opened twice asks once.

use std::path::Path;
use std::time::{Duration, Instant};

use orbyn_contracts::{EngineTools, ToolGroup};

use crate::app::Core;
use crate::error::{CoreError, Result};

const KEEP: Duration = Duration::from_secs(30);

/// The last answer per engine and folder.
#[derive(Default)]
pub(crate) struct ToolsCache {
    entries: std::collections::HashMap<String, (Instant, EngineTools)>,
}

impl ToolsCache {
    fn fresh(&self, key: &str) -> Option<EngineTools> {
        self.entries
            .get(key)
            .filter(|(at, _)| at.elapsed() < KEEP)
            .map(|(_, tools)| tools.clone())
    }

    fn last(&self, key: &str) -> Option<EngineTools> {
        self.entries.get(key).map(|(_, tools)| tools.clone())
    }

    fn keep(&mut self, key: String, tools: EngineTools) {
        if self.entries.len() > 64 {
            self.entries.clear();
        }
        self.entries.insert(key, (Instant::now(), tools));
    }
}

impl Core {
    /// The tools of a thread's engine, with what that thread's last run reported.
    pub async fn thread_tools(&self, thread_id: &str, refresh: bool) -> Result<EngineTools> {
        let thread = self.store.thread(thread_id)?;
        let family = self.engines.driver(&thread.engine)?.family();
        let init =
            self.store
                .latest_custom_event(Some(thread_id), "engine.tools", Some(&family))?;
        let cwd = Path::new(&thread.cwd).to_path_buf();
        self.tools_for(&thread.engine, &cwd, init, refresh, Some(thread_id))
            .await
    }

    /// The tools of an engine for Settings: the engine asked now, with the newest run's
    /// report from any thread, in the project's folder when one is chosen.
    pub async fn engine_tools(
        &self,
        engine: &str,
        project: Option<&str>,
        refresh: bool,
    ) -> Result<EngineTools> {
        let cwd = match project {
            Some(id) => Path::new(&self.store.project(id)?.path).to_path_buf(),
            None => std::env::temp_dir(),
        };
        let family = self.engines.driver(engine)?.family();
        let init = self
            .store
            .latest_custom_event(None, "engine.tools", Some(&family))?;
        self.tools_for(engine, &cwd, init, refresh, None).await
    }

    async fn tools_for(
        &self,
        engine: &str,
        cwd: &Path,
        init: Option<(String, serde_json::Value)>,
        refresh: bool,
        thread: Option<&str>,
    ) -> Result<EngineTools> {
        let driver = self.engines.driver(engine)?;
        let key = format!("{engine}\n{}\n{}", cwd.display(), thread.unwrap_or(""));
        if !refresh && let Some(fresh) = lock(&self.tool_lists).fresh(&key) {
            return Ok(fresh);
        }
        let answered = driver
            .tools(cwd, init.as_ref().map(|(_, value)| value))
            .await;
        let mut tools = match answered {
            Ok(Some(tools)) => tools,
            Ok(None) => {
                return Err(CoreError::Unsupported(
                    "This engine does not report its tools.".into(),
                ));
            }
            Err(error) => {
                // The last list stays, marked, so nothing the person knew disappears.
                let mut last = lock(&self.tool_lists).last(&key).ok_or(error)?;
                last.error = Some("The engine did not answer.".into());
                return Ok(last);
            }
        };
        tools.engine = engine.to_owned();
        if let Some((ts, _)) = &init
            && tools.source == "run"
        {
            tools.checked_at = Some(ts.clone());
        }
        self.add_skills(&mut tools);
        lock(&self.tool_lists).keep(key, tools.clone());
        Ok(tools)
    }

    /// Orbyn's own skills library is known without asking the engine, so it fills the
    /// Skills group when the engine did not report one.
    fn add_skills(&self, tools: &mut EngineTools) {
        if tools.groups.iter().any(|group| group.id == "skills") {
            return;
        }
        let Ok(catalog) = self.skills.catalog() else {
            return;
        };
        if catalog.is_empty() {
            return;
        }
        tools.groups.push(ToolGroup {
            id: "skills".into(),
            count: u32::try_from(catalog.len()).unwrap_or(u32::MAX),
            items: Vec::new(),
        });
        tools.not_reported.retain(|id| id != "skills");
    }
}

fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
