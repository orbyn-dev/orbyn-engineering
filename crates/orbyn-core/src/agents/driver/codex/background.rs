//! Background work Codex started for a run: sub-agent threads.
//!
//! The sub-agents share the app-server's stdio, and they die with it. The run
//! holds its parent's completed turn while one is still running (see the README,
//! "Codex run end"). The signals are those of the app-server protocol (generated
//! with `codex app-server generate-json-schema`, Codex 0.166.0):
//!
//! A thread's own status is the authority: an item's state may mark an agent running,
//! but never one that a `thread/status/changed` or `thread/closed` has ended; only a
//! later `active` status revives it. Sub-agents the engine runs for itself (review,
//! compact, memory consolidation, `other`) are not the person's work and are not held.
//!
//! - `thread/started`: `thread.parentThreadId`, or `thread.source.subAgent.thread_spawn
//!   .parent_thread_id`, names the parent; `thread.status.type` is the first state.
//! - `thread/status/changed`: `threadId` and `status.type`, `active` while it runs
//!   and `idle`, `notLoaded` or `systemError` once it does not.
//! - `thread/closed`: `threadId`.
//! - an `item/started` or `item/completed` of type `collabAgentToolCall`:
//!   `receiverThreadIds` and `agentsStates.<id>.status` (`pendingInit` and `running`
//!   are live; `completed`, `errored`, `interrupted`, `shutdown`, `notFound` are not).
//! - an item of type `subAgentActivity`: `agentThreadId` and `kind` (`started` is
//!   live; `completed` and `interrupted` are not; `interacted` changes nothing).
//!
//! Background shells have no such signal: a `commandExecution` item that has not
//! completed looks the same whether a dev server or an interrupted command left it,
//! and `process/exited` is only for processes this client spawned. They are not held.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde_json::Value;

/// How long a held run waits for its background work in all, when the person does
/// not cancel first. Overridden in tests by `ORBYN_CODEX_BACKGROUND_LIMIT_MS`.
const BACKGROUND_LIMIT: Duration = Duration::from_secs(30 * 60);

/// How long a held run waits with no notification from its thread or any agent it
/// tracks: a finish signal that never came must not hold the run for the whole limit.
/// Overridden in tests by `ORBYN_CODEX_BACKGROUND_IDLE_MS`.
const IDLE_LIMIT: Duration = Duration::from_secs(5 * 60);

/// After the last agent ends, how long to wait for the turn its result starts.
pub(super) const FOLLOW_UP_GRACE: Duration = Duration::from_secs(2);

pub(super) fn limit() -> Duration {
    std::env::var("ORBYN_CODEX_BACKGROUND_LIMIT_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .map_or(BACKGROUND_LIMIT, Duration::from_millis)
}

pub(super) fn idle_limit() -> Duration {
    std::env::var("ORBYN_CODEX_BACKGROUND_IDLE_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .map_or(IDLE_LIMIT, Duration::from_millis)
}

/// A sub-agent thread, the run's own or a descendant's.
struct Agent {
    running: bool,
    /// Its thread status or `thread/closed` said it ended; an item cannot revive it.
    ended: bool,
    label: Option<String>,
}

/// What a run knows of the work its thread started.
pub(super) struct Background {
    session: String,
    agents: HashMap<String, Agent>,
    /// Turns running on the run's thread that are not the prompt's own, such as
    /// the one an agent's result starts.
    follow_ups: HashSet<String>,
    /// A turn of that kind has completed.
    followed: bool,
}

/// A thread the run stopped while work on it was running.
pub(super) struct Stopped {
    pub(super) id: String,
    pub(super) label: Option<String>,
}

impl Background {
    pub(super) fn new(session: &str) -> Self {
        Self {
            session: session.to_owned(),
            agents: HashMap::new(),
            follow_ups: HashSet::new(),
            followed: false,
        }
    }

    /// A sub-agent is still running.
    pub(super) fn running(&self) -> bool {
        self.agents.values().any(|agent| agent.running)
    }

    /// A turn the engine started on the run's thread is still running.
    pub(super) fn follow_up_open(&self) -> bool {
        !self.follow_ups.is_empty()
    }

    /// A turn the engine started on the run's thread has completed.
    pub(super) fn followed(&self) -> bool {
        self.followed
    }

    /// The agents still running, each to be reported as stopped.
    pub(super) fn stopped(&self) -> Vec<Stopped> {
        let mut running: Vec<_> = self
            .agents
            .iter()
            .filter(|(_, agent)| agent.running)
            .map(|(id, agent)| Stopped {
                id: id.clone(),
                label: agent.label.clone(),
            })
            .collect();
        running.sort_by(|a, b| a.id.cmp(&b.id));
        running
    }

    fn ours(&self, thread: &str) -> bool {
        thread == self.session || self.agents.contains_key(thread)
    }

    fn agent(&mut self, id: &str) -> &mut Agent {
        self.agents.entry(id.to_owned()).or_insert(Agent {
            running: false,
            ended: false,
            label: None,
        })
    }

    /// The thread's own word: running or ended.
    fn thread_says(&mut self, id: &str, running: bool) {
        let agent = self.agent(id);
        agent.running = running;
        agent.ended = !running;
        if running {
            self.followed = false;
        }
    }

    /// An item's word: it may say an agent runs only if its thread has not ended.
    fn item_says(&mut self, id: &str, running: bool) {
        let agent = self.agent(id);
        if running && agent.ended {
            return;
        }
        agent.running = running;
        if running {
            self.followed = false;
        }
    }

    /// The notification is about this run's thread or an agent it tracks.
    pub(super) fn involves(&self, params: &Value) -> bool {
        let thread = params
            .get("threadId")
            .or_else(|| params.pointer("/thread/id"))
            .and_then(Value::as_str);
        thread.is_some_and(|thread| self.ours(thread))
    }

    /// Reads one notification. `ours` is the turn started for the person's prompt.
    pub(super) fn observe(&mut self, method: &str, params: &Value, ours: Option<&str>) {
        match method {
            "thread/started" => self.thread_started(params),
            "thread/status/changed" => {
                let thread = params.get("threadId").and_then(Value::as_str);
                let status = params.pointer("/status/type").and_then(Value::as_str);
                if let (Some(thread), Some(status)) = (thread, status)
                    && self.agents.contains_key(thread)
                {
                    self.thread_says(thread, status == "active");
                }
            }
            "thread/closed" => {
                if let Some(thread) = params.get("threadId").and_then(Value::as_str)
                    && self.agents.contains_key(thread)
                {
                    self.thread_says(thread, false);
                }
            }
            "item/started" | "item/completed" => {
                let thread = params.get("threadId").and_then(Value::as_str);
                if thread.is_some_and(|thread| self.ours(thread))
                    && let Some(item) = params.get("item")
                {
                    self.item(item);
                }
            }
            "turn/started" | "turn/completed" => self.turn(method, params, ours),
            _ => {}
        }
    }

    fn thread_started(&mut self, params: &Value) {
        let Some(thread) = params.get("thread") else {
            return;
        };
        let Some(id) = thread.get("id").and_then(Value::as_str) else {
            return;
        };
        let parent = thread
            .get("parentThreadId")
            .and_then(Value::as_str)
            .or_else(|| {
                thread
                    .pointer("/source/subAgent/thread_spawn/parent_thread_id")
                    .and_then(Value::as_str)
            });
        if parent.is_none_or(|parent| !self.ours(parent)) {
            return;
        }
        // Review, compaction, memory and other agents the engine runs for itself.
        let internal = thread
            .pointer("/source/subAgent")
            .is_some_and(|source| source.is_string() || source.get("other").is_some());
        if internal {
            return;
        }
        let status = thread.pointer("/status/type").and_then(Value::as_str);
        let label = thread
            .pointer("/source/subAgent/thread_spawn/agent_nickname")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let agent = self.agent(id);
        agent.running = status == Some("active");
        agent.ended = status == Some("systemError");
        if label.is_some() {
            agent.label = label;
        }
        if agent.running {
            self.followed = false;
        }
    }

    fn item(&mut self, item: &Value) {
        match item.get("type").and_then(Value::as_str) {
            Some("collabAgentToolCall") => {
                let tool = item.get("tool").and_then(Value::as_str);
                let call = item.get("status").and_then(Value::as_str);
                let receivers = item.get("receiverThreadIds").and_then(Value::as_array);
                for id in receivers.into_iter().flatten().filter_map(Value::as_str) {
                    let state = item
                        .pointer(&format!("/agentsStates/{id}/status"))
                        .and_then(Value::as_str);
                    let running = match state {
                        Some("pendingInit" | "running") => Some(true),
                        Some(_) => Some(false),
                        // No state: a call that hands an agent work leaves it running.
                        None => match (tool, call) {
                            (_, Some("failed" | "interrupted")) => None,
                            (
                                Some(
                                    "spawnAgent" | "sendInput" | "resumeAgent" | "sendMessage"
                                    | "followupTask",
                                ),
                                _,
                            ) => Some(true),
                            (Some("closeAgent" | "interruptAgent"), _) => Some(false),
                            _ => None,
                        },
                    };
                    if let Some(running) = running {
                        self.item_says(id, running);
                    }
                }
            }
            Some("subAgentActivity") => {
                let id = item.get("agentThreadId").and_then(Value::as_str);
                let kind = item.get("kind").and_then(Value::as_str);
                if let (Some(id), Some(kind)) = (id, kind) {
                    match kind {
                        "started" => self.item_says(id, true),
                        "completed" | "interrupted" => self.item_says(id, false),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn turn(&mut self, method: &str, params: &Value, ours: Option<&str>) {
        if params
            .get("threadId")
            .and_then(Value::as_str)
            .is_some_and(|thread| thread != self.session)
        {
            return;
        }
        let id = params
            .pointer("/turn/id")
            .or_else(|| params.get("turnId"))
            .and_then(Value::as_str);
        let Some(id) = id else {
            return;
        };
        if ours == Some(id) {
            return;
        }
        if method == "turn/started" {
            self.follow_ups.insert(id.to_owned());
        } else if self.follow_ups.remove(id) {
            self.followed = true;
        }
    }
}
