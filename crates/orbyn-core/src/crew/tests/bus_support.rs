//! Two fake engines with a worker each, for the bus tests. `alpha` takes input mid-turn (as
//! Claude and Codex do); `beta` does not, so a message to it waits for its next turn. Both
//! workers hang in their first turn, so every call below happens in the middle of a turn.

use std::sync::Arc;

use serde_json::{Value, json};

use super::harness::{Turn, World, bind, brief, key, world_with};

pub struct Duo {
    pub w: World,
    /// Task ids and member ids of the worker on `alpha` and the worker on `beta`.
    pub a: String,
    pub b: String,
    pub ma: String,
    pub mb: String,
    /// Their MCP tokens, as the engines would hold them.
    pub ta: String,
    pub tb: String,
}

fn is_running(w: &World, thread: &str) -> bool {
    w.core
        .store
        .thread(thread)
        .unwrap()
        .active_run
        .is_some_and(|run| run.state == orbyn_contracts::RunState::Running)
}

fn engine(id: &str) -> Value {
    json!({ "id": id, "assigned_by": "person" })
}

/// Two writers on different files, each on its own engine, both in a running turn.
pub async fn duo() -> Duo {
    let w = world_with(
        true,
        Arc::new(|_, _| Turn::Hang),
        &[("alpha", true), ("beta", false)],
    )
    .await;
    let (first, error) = w
        .start(brief(json!({ "engine": engine("alpha") })), key())
        .await;
    assert!(!error, "{first}");
    let (second, error) = w
        .start(
            brief(json!({
                "engine": engine("beta"),
                "outcome": "Document it.",
                "files": [{ "path": "docs/**", "mode": "write" }],
            })),
            key(),
        )
        .await;
    assert!(!error, "{second}");
    let a = first["task"].as_str().unwrap().to_owned();
    let b = second["task"].as_str().unwrap().to_owned();
    let (wa, wb) = (w.worker(&a), w.worker(&b));
    w.until("both turns run", |w| {
        is_running(w, &wa) && is_running(w, &wb)
    })
    .await;
    let ma = w.core.store.crew_run(&a).unwrap().unwrap().member;
    let mb = w.core.store.crew_run(&b).unwrap().unwrap().member;
    let (ta, tb) = (bind(&w.core, &wa, "run-a"), bind(&w.core, &wb, "run-b"));
    Duo {
        w,
        a,
        b,
        ma,
        mb,
        ta,
        tb,
    }
}

impl Duo {
    pub async fn call(&self, token: &str, tool: &str, args: Value) -> (Value, bool) {
        self.w.call(token, tool, args).await
    }

    /// The code of a refusal; panics when the call was not refused.
    pub async fn refused(&self, token: &str, tool: &str, args: Value) -> String {
        let (answer, error) = self.call(token, tool, args).await;
        assert!(error, "expected a refusal, got {answer}");
        answer["code"].as_str().unwrap_or("?").to_owned()
    }

    /// A fact message of this body from `token` to `to`.
    pub fn fact(to: &str, body: &str) -> Value {
        json!({ "to": to, "kind": "fact", "body": body })
    }

    pub fn queued(&self, thread: &str) -> Vec<String> {
        self.w
            .core
            .store
            .list_queue(thread)
            .unwrap()
            .into_iter()
            .map(|m| m.prompt)
            .collect()
    }

    pub fn steered_to(&self, thread: &str) -> Vec<String> {
        self.w
            .steered
            .lock()
            .unwrap()
            .iter()
            .filter(|(to, _)| to == thread)
            .map(|(_, text)| text.clone())
            .collect()
    }
}
