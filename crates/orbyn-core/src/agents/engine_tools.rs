//! What a thread's engine has available, for the Tools tab and Settings. The engine is
//! asked in a neutral folder (the drivers keep each engine-wide answer for half a minute and
//! share one read between callers), what the thread's last run reported is added, and the
//! last good list stays when a later check fails.

use std::path::PathBuf;

use orbyn_contracts::{EngineTools, ToolGroup};

use crate::app::Core;
use crate::error::{CoreError, Result};

/// The last good list per engine and thread, kept so a failed check can still show it.
#[derive(Default)]
pub(crate) struct ToolsCache {
    last: std::collections::HashMap<String, EngineTools>,
}

impl ToolsCache {
    fn last(&self, key: &str) -> Option<EngineTools> {
        self.last.get(key).cloned()
    }

    fn keep(&mut self, key: String, tools: EngineTools) {
        if self.last.len() > 64 {
            self.last.clear();
        }
        self.last.insert(key, tools);
    }
}

/// Whether a list holds any entry at all (counted-only groups such as Orbyn's count too).
fn has_content(tools: &EngineTools) -> bool {
    tools
        .groups
        .iter()
        .any(|group| group.count > 0 || !group.items.is_empty())
}

/// The answer to show, given what the engine just said and the last good list, and whether it
/// becomes the new last good one. A check that came back with an error keeps the list the person
/// already had, marked with that error. A list that was merely cut short is shown, marked
/// incomplete, because it is newer; it becomes the last good one only when it holds something.
fn settle(answer: EngineTools, last: Option<EngineTools>) -> (EngineTools, bool) {
    match (answer.error.clone(), last) {
        (Some(error), Some(mut last)) => {
            last.error = Some(error);
            (last, false)
        }
        (Some(_), None) => (answer, false),
        (None, _) => {
            let keep = !answer.incomplete || has_content(&answer);
            (answer, keep)
        }
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
        self.tools_for(&thread.engine, init, refresh, Some(thread_id))
            .await
    }

    /// The tools of an engine for Settings: the engine asked now, with the newest run's
    /// report from any thread.
    pub async fn engine_tools(&self, engine: &str, refresh: bool) -> Result<EngineTools> {
        let family = self.engines.driver(engine)?.family();
        let init = self
            .store
            .latest_custom_event(None, "engine.tools", Some(&family))?;
        self.tools_for(engine, init, refresh, None).await
    }

    async fn tools_for(
        &self,
        engine: &str,
        init: Option<(String, serde_json::Value)>,
        refresh: bool,
        thread: Option<&str>,
    ) -> Result<EngineTools> {
        let driver = self.engines.driver(engine)?;
        let key = format!("{engine}\n{}", thread.unwrap_or(""));
        // A neutral folder, never the thread's: asking Codex starts an app-server that
        // loads its folder's `.codex` config and servers, and a project's must not run
        // because a tab was opened. A project's own servers show through a run's frame.
        let cwd: PathBuf = std::env::temp_dir();
        let probe = lock(&self.tool_probes)
            .entry(engine.to_owned())
            .or_default()
            .clone();
        let answered = driver
            .tools(&cwd, init.as_ref().map(|(_, value)| value), refresh, &probe)
            .await;
        let mut tools = match answered {
            Ok(Some(tools)) => tools,
            Ok(None) => {
                return Err(CoreError::Unsupported(
                    "This engine does not report its tools.".into(),
                ));
            }
            Err(error) => {
                let mut last = lock(&self.tool_lists).last(&key).ok_or(error)?;
                last.error = Some("The engine did not answer.".into());
                return Ok(last);
            }
        };
        tools.engine = engine.to_owned();
        self.add_skills(&mut tools);
        let last = lock(&self.tool_lists).last(&key);
        let (shown, good) = settle(tools, last);
        if good {
            lock(&self.tool_lists).keep(key, shown.clone());
        }
        Ok(shown)
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn list(error: Option<&str>, names: &[&str]) -> EngineTools {
        EngineTools {
            engine: "claude".into(),
            source: "check".into(),
            checked_at: Some("2026-11-08T10:00:00Z".into()),
            groups: vec![ToolGroup {
                id: "mcp".into(),
                count: names.len() as u32,
                items: Vec::new(),
            }],
            not_reported: Vec::new(),
            error: error.map(str::to_owned),
            reported: true,
            incomplete: false,
        }
    }

    #[test]
    fn a_good_answer_replaces_the_last_one_and_is_kept() {
        let (shown, keep) = settle(list(None, &["a", "b"]), Some(list(None, &["a"])));
        assert_eq!(shown.groups[0].count, 2);
        assert!(keep);
    }

    #[test]
    fn a_check_with_an_error_keeps_the_last_good_list_marked_with_that_error() {
        let (shown, keep) = settle(
            list(Some("did not list"), &[]),
            Some(list(None, &["a", "b"])),
        );
        assert_eq!(shown.groups[0].count, 2, "the earlier list stays");
        assert_eq!(shown.error.as_deref(), Some("did not list"));
        assert!(!keep, "an errored answer never becomes the last good one");
    }

    #[test]
    fn with_no_earlier_list_the_errored_answer_is_all_there_is() {
        let (shown, keep) = settle(list(Some("did not list"), &[]), None);
        assert_eq!(shown.error.as_deref(), Some("did not list"));
        assert!(!keep);
    }

    #[test]
    fn a_cut_short_list_is_shown_marked_and_kept_only_if_it_holds_something() {
        let mut partial = list(None, &["a"]);
        partial.incomplete = true;
        let (shown, keep) = settle(partial.clone(), Some(list(None, &["a", "b", "c"])));
        assert!(shown.incomplete, "the newer, partial list is what is shown");
        assert_eq!(shown.groups[0].count, 1);
        assert!(keep);
        let mut empty = list(None, &[]);
        empty.incomplete = true;
        let (_, keep) = settle(empty, Some(list(None, &["a"])));
        assert!(!keep, "an empty cut-short list never replaces a good one");
    }

    #[test]
    fn engine_ids_each_get_their_own_probe_so_an_instance_is_cached_too() {
        let probes: Mutex<
            std::collections::HashMap<
                String,
                std::sync::Arc<crate::agents::tools_report::ToolsProbe>,
            >,
        > = Mutex::default();
        let get = |id: &str| lock(&probes).entry(id.to_owned()).or_default().clone();
        assert!(std::sync::Arc::ptr_eq(
            &get("claude-wide"),
            &get("claude-wide")
        ));
        assert!(!std::sync::Arc::ptr_eq(&get("claude-wide"), &get("claude")));
    }
}
