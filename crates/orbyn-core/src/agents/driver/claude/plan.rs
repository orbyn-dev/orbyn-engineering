use serde_json::{Value, json};

use super::text_of;

/// One row of the plan activity fed by Claude's TaskCreate/TaskUpdate/TaskList.
pub(super) struct TaskStep {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) status: String,
}

/// `TodoWrite`'s list in the `plan` activity's shape. Its statuses are the same words.
pub(super) fn todo_content(input: &Value) -> Value {
    let steps: Vec<Value> = input
        .get("todos")
        .and_then(Value::as_array)
        .map(|todos| {
            todos
                .iter()
                .map(|todo| {
                    json!({
                        "title": text_of(todo, "content").unwrap_or_default(),
                        "status": text_of(todo, "status").unwrap_or_else(|| "pending".into()),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    json!({ "steps": steps })
}

/// The structured result when Claude sends one, otherwise the text the tool
/// result carries.
pub(super) fn task_result(frame: &Value, block: &Value) -> Value {
    if let Some(result) = frame
        .get("tool_use_result")
        .filter(|value| !value.is_null())
    {
        return result.clone();
    }
    match block.get("content") {
        Some(Value::Array(parts)) => Value::String(
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(""),
        ),
        Some(value) => value.clone(),
        None => Value::Null,
    }
}

/// A subagent result carries no structured task, only the created line.
pub(super) fn created_from_text(result: &Value) -> Option<(String, String)> {
    let rest = result.as_str()?.strip_prefix("Task #")?;
    let (id, subject) = rest.split_once(" created successfully: ")?;
    Some((id.trim().to_owned(), subject.trim().to_owned()))
}

pub(super) fn task_status(value: Option<&str>) -> Option<&'static str> {
    match value {
        Some("pending") => Some("pending"),
        Some("in_progress") => Some("running"),
        Some("completed") => Some("completed"),
        _ => None,
    }
}

/// The task list in the same `plan` activity shape `TodoWrite` produces.
pub(super) fn task_content(tasks: &[TaskStep]) -> Value {
    let steps: Vec<Value> = tasks
        .iter()
        .map(|task| json!({ "title": task.title, "status": task.status }))
        .collect();
    json!({ "steps": steps })
}
