//! Scheduled tasks and webhooks.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use ts_rs::TS;

wire! {
pub struct TaskSchedule {
    /// `interval`, `fixed_time`, `webhook` or `after_thread`.
    pub kind: String,
    #[serde(default)]
    pub every_minutes: Option<u32>,
    /// `HH:MM` for `fixed_time`.
    #[serde(default)]
    pub time: Option<String>,
    /// 0 is Sunday. Empty means every day.
    #[serde(default)]
    pub weekdays: Vec<u8>,
    #[serde(default)]
    pub utc_offset_minutes: i32,
    /// For `after_thread`: the thread whose runs set this off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub after_thread: Option<String>,
    /// For `after_thread`: only when its run completed.
    #[serde(default)]
    pub only_on_success: bool,
}
}

wire! {
pub struct WebhookSignature {
    pub header: String,
    /// `hex` or `base64`.
    pub encoding: String,
    #[serde(default)]
    pub prefix: String,
}
}

wire! {
pub struct WebhookFilter {
    /// Dotted path into the body; `headers.<name>` reads a header.
    pub path: String,
    /// `equals`, `not_equals`, `starts_with`, `in` or `exists`.
    pub op: String,
    #[serde(default)]
    pub value: Option<Value>,
}
}

wire! {
pub struct WebhookDedupe {
    pub path: String,
    pub window_seconds: u64,
}
}

wire! {
pub struct ScheduledTask {
    pub id: String,
    pub project_id: String,
    /// Runs here if set; otherwise each firing opens a new thread.
    pub thread_id: Option<String>,
    pub name: String,
    /// `{{body.a.b}}` and `{{headers.x}}` read a webhook request.
    pub prompt: String,
    pub engine: String,
    pub schedule: TaskSchedule,
    pub enabled: bool,
    pub signature: Option<WebhookSignature>,
    #[serde(default)]
    pub has_secret: bool,
    #[serde(default)]
    pub filters: Vec<WebhookFilter>,
    pub dedupe: Option<WebhookDedupe>,
    /// Refuses a request older than this, read from `timestamp_path`.
    pub max_age_seconds: Option<u64>,
    pub timestamp_path: Option<String>,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
    /// `never`, `started`, or why the last firing failed.
    pub last_status: String,
    pub created_at: String,
    /// The last run of `after_thread` this task has answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub after_run: Option<String>,
}
}

/// Scheduled-task create or update. `secret` is a webhook signing secret: this type prints nothing of it.
#[derive(Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../web/src/generated/")]
pub struct UpsertTask {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub id: Option<String>,
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub thread_id: Option<String>,
    pub name: String,
    pub prompt: String,
    pub engine: String,
    pub schedule: TaskSchedule,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub signature: Option<WebhookSignature>,
    /// Absent keeps the stored secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub secret: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<WebhookFilter>>")]
    pub filters: Vec<WebhookFilter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dedupe: Option<WebhookDedupe>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_age_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub timestamp_path: Option<String>,
}

/// The signing secret is a secret: this type never prints it, as `SetEnvValue` does. The identity fields stay
/// visible, so a refusal that names the task is readable.
impl std::fmt::Debug for UpsertTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpsertTask")
            .field("project_id", &self.project_id)
            .field("name", &self.name)
            .field("engine", &self.engine)
            .finish_non_exhaustive()
    }
}

wire! {
pub struct TaskEnabled {
    pub enabled: bool,
}
}

/// The token appears here once; only its digest is kept.
#[derive(Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../web/src/generated/")]
pub struct WebhookEndpoint {
    pub path: String,
    pub token: String,
}

/// The token is a live webhook secret: this type never prints it.
impl std::fmt::Debug for WebhookEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebhookEndpoint")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

wire! {
pub struct WebhookDelivery {
    pub id: String,
    pub task_id: String,
    /// `accepted`, `bad_signature`, `filtered`, `duplicate`, `stale`, `disabled`
    /// or `failed`.
    pub status: String,
    pub received_at: String,
    /// With credentials removed.
    pub headers: Value,
    pub body: String,
    pub prompt: Option<String>,
    pub thread_id: Option<String>,
    /// Who sent it, from the request's headers: a known service such as `GitHub`, or
    /// the sender's `User-Agent`. Absent when the request names none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from: Option<String>,
}
}

wire! {
/// Where a task's webhook listens. The token is kept only as a digest, so it cannot
/// be shown again; `rotate` issues a new one.
pub struct WebhookAddress {
    /// A token has been issued, so the address accepts requests.
    pub configured: bool,
    /// The address up to the token: `/hooks/{task id}/`.
    pub path_prefix: String,
}
}

wire! {
/// One firing of a task, newest first. The service keeps the last 50 per task.
pub struct TaskRun {
    pub id: String,
    pub task_id: String,
    /// `schedule`, `manual`, `webhook` or `after_thread`.
    pub source: String,
    pub started_at: String,
    /// When the run ended; absent while it is queued or running, and when it never began.
    pub ended_at: Option<String>,
    /// `queued`, `running`, `completed`, `failed`, `cancelled`, `interrupted`,
    /// `refused` (it could not start, see `error`) or `skipped` (near a usage limit).
    pub outcome: String,
    /// The thread the run works in, to open from the history.
    pub thread_id: Option<String>,
    pub error: Option<String>,
}
}

wire! {
pub struct DelegateTask {
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    /// Absent uses the parent's engine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub engine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, String>>")]
    pub options: BTreeMap<String, String>,
}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schedule() -> TaskSchedule {
        TaskSchedule {
            kind: "interval".into(),
            every_minutes: Some(30),
            time: None,
            weekdays: Vec::new(),
            utc_offset_minutes: 0,
            after_thread: None,
            only_on_success: false,
        }
    }

    #[test]
    fn a_task_upsert_never_prints_its_signing_secret() {
        let upsert = UpsertTask {
            id: Some("task-1".into()),
            project_id: "project-1".into(),
            thread_id: None,
            name: "Nightly".into(),
            prompt: "check".into(),
            engine: "claude".into(),
            schedule: schedule(),
            signature: None,
            secret: Some("SECRET-signing-value".into()),
            filters: Vec::new(),
            dedupe: None,
            max_age_seconds: None,
            timestamp_path: None,
        };
        let printed = format!("{upsert:?}");
        assert!(!printed.contains("SECRET"), "{printed}");
        assert!(printed.contains("Nightly"), "{printed}");
        let parsed: UpsertTask = serde_json::from_str(
            r#"{"project_id":"p","name":"n","prompt":"x","engine":"claude","schedule":{"kind":"interval"},"secret":"SECRET-2"}"#,
        )
        .unwrap();
        assert!(!format!("{parsed:?}").contains("SECRET-2"));
    }

    #[test]
    fn a_webhook_endpoint_never_prints_its_token() {
        let endpoint = WebhookEndpoint {
            path: "/hooks/task-1/".into(),
            token: "SECRET-token".into(),
        };
        let printed = format!("{endpoint:?}");
        assert!(!printed.contains("SECRET"), "{printed}");
        assert!(printed.contains("/hooks/task-1/"), "{printed}");
        let parsed: WebhookEndpoint =
            serde_json::from_str(r#"{"path":"/hooks/t/","token":"SECRET-2"}"#).unwrap();
        assert!(!format!("{parsed:?}").contains("SECRET-2"));
    }
}
