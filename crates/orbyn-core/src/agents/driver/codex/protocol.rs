use orbyn_contracts::{
    ActionKind, EngineFeedbackReceipt, EngineFeedbackRequest, Question, QuestionOption,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::events::TurnMapper;
use super::{APP_READ_TIMEOUT, METHOD_NOT_FOUND, text};
use crate::agents::driver::rpc::RpcClient;
use crate::agents::driver::{EngineEvent, ask_paths, ask_question};
use crate::conversation::questions::Answers;
use crate::error::{CoreError, Result};
use crate::governance::approvals::Verdict;

/// Answers one server request on its own task so the turn keeps flowing while a
/// person decides. Anything unrecognized is refused, never ignored.
pub(super) fn answer_request(
    rpc: &RpcClient,
    events: &mpsc::Sender<EngineEvent>,
    mapper: &TurnMapper,
    id: Value,
    method: String,
    params: Value,
) {
    let rpc = rpc.clone();
    if method == "item/tool/requestUserInput" {
        let events = events.clone();
        tokio::spawn(async move {
            let answers = ask_question(&events, parse_questions(&params)).await;
            let _ = rpc.respond(id, question_response(answers)).await;
        });
        return;
    }
    // A form that collects data from the person is declined with a valid answer: an
    // error would read as a broken server, and we never fill in fields for anyone.
    if method == "mcpServer/elicitation/request" && !is_tool_approval(&params) {
        tokio::spawn(async move {
            let _ = rpc
                .respond(id, json!({ "action": "cancel", "content": null }))
                .await;
        });
        return;
    }
    let Some(request) = classify_request(&method, &params, mapper) else {
        tracing::info!(%method, "refusing an unknown Codex server request");
        tokio::spawn(async move {
            let text = format!("Orbyn does not serve {method}.");
            let _ = rpc.respond_error(id, METHOD_NOT_FOUND, &text).await;
        });
        return;
    };
    let events = events.clone();
    tokio::spawn(async move {
        let verdict = ask_paths(
            &events,
            request.kind,
            request.target,
            request.summary,
            params.clone(),
            request.paths,
        )
        .await;
        let result = approval_response(&method, &params, &verdict);
        let _ = rpc.respond(id, result).await;
    });
}

/// Reads the MCP App page of a finished tool call from the run's own app-server,
/// which is the only process that still knows the call. A read that fails or times
/// out leaves the row as an ordinary tool call.
pub(super) async fn read_app(
    rpc: &RpcClient,
    session: &str,
    params: &Value,
) -> Option<crate::tools::mcp_apps::McpAppSnapshot> {
    if params
        .get("threadId")
        .and_then(Value::as_str)
        .is_some_and(|thread| thread != session)
    {
        return None;
    }
    let item = params.get("item")?;
    let (server, tool, uri, display_mode) = crate::tools::mcp_apps::codex_app_target(item)?;
    let tool_call_id = text(item, "id")?;
    let read = rpc
        .call(
            "mcpServer/resource/read",
            json!({ "server": server, "uri": uri, "threadId": session, "originCallId": tool_call_id }),
            APP_READ_TIMEOUT,
        )
        .await
        .map_err(|error| tracing::debug!(%error, "could not read an MCP App page"))
        .ok()?;
    let (html, prefers_border) = crate::tools::mcp_apps::page_from_read(&read, &uri)?;
    Some(crate::tools::mcp_apps::McpAppSnapshot {
        tool_call_id,
        server,
        tool,
        resource_uri: uri,
        html,
        display_mode,
        prefers_border,
        structured: crate::tools::mcp_apps::structured_output(item),
    })
}

/// Codex names a thread it no longer has in one of these ways.
pub(super) fn is_missing_thread(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("no rollout") || lower.contains("not found") || lower.contains("no thread")
}

fn parse_questions(params: &Value) -> Vec<Question> {
    let items = params.get("questions").and_then(Value::as_array);
    items
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let options = item.get("options").and_then(Value::as_array);
            Some(Question {
                id: text(item, "id")?,
                header: text(item, "header").unwrap_or_default(),
                question: text(item, "question").unwrap_or_default(),
                options: options
                    .into_iter()
                    .flatten()
                    .map(|option| QuestionOption {
                        label: text(option, "label").unwrap_or_default(),
                        description: text(option, "description").unwrap_or_default(),
                    })
                    .collect(),
                multi_select: false,
                allow_other: item.get("isOther").and_then(Value::as_bool) == Some(true),
                secret: item.get("isSecret").and_then(Value::as_bool) == Some(true),
            })
        })
        .collect()
}

/// A dismissal is an empty answer set, which Codex reads as no answer given.
fn question_response(answers: Option<Answers>) -> Value {
    let answers: serde_json::Map<String, Value> = answers
        .into_iter()
        .flatten()
        .map(|(id, values)| (id, json!({ "answers": values })))
        .collect();
    json!({ "answers": answers })
}

pub(super) struct ClassifiedRequest {
    kind: ActionKind,
    pub(super) target: String,
    pub(super) summary: String,
    paths: Vec<String>,
}

pub(super) fn classify_request(
    method: &str,
    params: &Value,
    mapper: &TurnMapper,
) -> Option<ClassifiedRequest> {
    match method {
        "item/commandExecution/requestApproval" => {
            let target = params
                .get("command")
                .and_then(Value::as_str)
                .or_else(|| params.get("reason").and_then(Value::as_str))
                .unwrap_or("command")
                .to_owned();
            let network = params
                .get("networkApprovalContext")
                .is_some_and(|context| !context.is_null());
            Some(ClassifiedRequest {
                kind: if network {
                    ActionKind::Network
                } else {
                    ActionKind::Command
                },
                summary: format!("Run {target}"),
                target,
                paths: Vec::new(),
            })
        }
        "item/fileChange/requestApproval" => {
            let item = params.get("itemId").and_then(Value::as_str).unwrap_or("");
            let mut paths = mapper.file_paths.get(item).cloned().unwrap_or_default();
            paths.extend(text(params, "grantRoot"));
            let target = if paths.is_empty() {
                "file changes".to_owned()
            } else {
                paths.join(" ")
            };
            Some(ClassifiedRequest {
                kind: ActionKind::FileChange,
                summary: format!("Change {target}"),
                target,
                paths,
            })
        }
        "item/permissions/requestApproval" => {
            let network = params
                .pointer("/permissions/network")
                .is_some_and(|value| !value.is_null());
            let target = text(params, "reason")
                .or_else(|| text(params, "cwd"))
                .unwrap_or_else(|| "additional permissions".to_owned());
            Some(ClassifiedRequest {
                kind: if network {
                    ActionKind::Network
                } else {
                    ActionKind::Tool
                },
                summary: format!("Grant permissions: {target}"),
                target,
                paths: Vec::new(),
            })
        }
        "mcpServer/elicitation/request" if is_tool_approval(params) => {
            let server = text(params, "serverName").unwrap_or_else(|| "an MCP server".to_owned());
            let message = text(params, "message").unwrap_or_default();
            // Remembering an answer is keyed on the target, so it names the tool as well as
            // the server: allowing one Gmail call must not allow every connector call. A
            // request that does not name its tool is keyed on its whole message instead.
            let tool = elicited_tool(params).unwrap_or_else(|| message.clone());
            Some(ClassifiedRequest {
                kind: ActionKind::Tool,
                summary: if message.is_empty() {
                    format!("Use a tool from {server}")
                } else {
                    format!("{server}: {message}")
                },
                target: format!("{server}/{tool}"),
                paths: Vec::new(),
            })
        }
        "execCommandApproval" => {
            let target = params
                .get("command")
                .and_then(Value::as_array)
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_else(|| "command".to_owned());
            Some(ClassifiedRequest {
                kind: ActionKind::Command,
                summary: format!("Run {target}"),
                target,
                paths: Vec::new(),
            })
        }
        "applyPatchApproval" => {
            let paths: Vec<String> = params
                .get("fileChanges")
                .and_then(Value::as_object)
                .map(|changes| changes.keys().cloned().collect())
                .unwrap_or_default();
            let target = if paths.is_empty() {
                "file changes".to_owned()
            } else {
                paths.join(" ")
            };
            Some(ClassifiedRequest {
                kind: ActionKind::FileChange,
                summary: format!("Change {target}"),
                target,
                paths,
            })
        }
        _ => None,
    }
}

/// The tool an approval elicitation is about: its meta when Codex gives it, else the
/// quoted name in the message (`... to run tool "send_email"?`).
fn elicited_tool(params: &Value) -> Option<String> {
    for key in ["tool_name", "tool", "toolName"] {
        if let Some(name) = params
            .pointer(&format!("/_meta/{key}"))
            .and_then(Value::as_str)
            && !name.is_empty()
        {
            return Some(name.to_owned());
        }
    }
    let message = params.get("message")?.as_str()?;
    let after = message.split_once("tool \"")?.1;
    let name = after.split_once('"')?.0;
    (!name.is_empty()).then(|| name.to_owned())
}

/// Codex asks for a connector or MCP tool call through an elicitation. This is one that
/// only wants a yes or no: it says so in its meta, or it asks for no fields. A form that
/// collects data from the person is not an approval and is never answered with one, and
/// neither is a URL request, which needs the person to see and open the address.
pub(super) fn is_tool_approval(params: &Value) -> bool {
    if params.get("mode").and_then(Value::as_str) == Some("url") || params.get("url").is_some() {
        return false;
    }
    let named = params
        .pointer("/_meta/codex_approval_kind")
        .is_some_and(|kind| !kind.is_null());
    let schema = params.get("requestedSchema").and_then(Value::as_object);
    let fields = schema
        .and_then(|schema| schema.get("properties"))
        .and_then(Value::as_object)
        .map_or(0, serde_json::Map::len);
    named || (schema.is_some() && fields == 0)
}

/// What to send as `content` when an approval is accepted. A form with required fields
/// is filled only from the defaults its schema gives; with one that has none, there is
/// nothing honest to send and the request is declined instead (`None`).
pub(super) fn approval_content(params: &Value) -> Option<Value> {
    let Some(schema) = params.get("requestedSchema") else {
        return Some(Value::Null);
    };
    let properties = schema.get("properties").and_then(Value::as_object);
    let Some(properties) = properties.filter(|properties| !properties.is_empty()) else {
        return Some(Value::Null);
    };
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut content = serde_json::Map::new();
    for (name, field) in properties {
        match field.get("default") {
            Some(default) => {
                content.insert(name.clone(), default.clone());
            }
            None if required.contains(&name.as_str()) => return None,
            None => {}
        }
    }
    Some(Value::Object(content))
}

/// The response shape each approval method's schema requires.
pub(super) fn approval_response(method: &str, params: &Value, verdict: &Verdict) -> Value {
    let allowed = verdict.is_allowed();
    match method {
        "mcpServer/elicitation/request" => match approval_content(params) {
            Some(content) if allowed => json!({ "action": "accept", "content": content }),
            _ => json!({ "action": "decline", "content": null }),
        },
        "item/permissions/requestApproval" => {
            if allowed {
                json!({
                    "permissions": params.get("permissions").cloned().unwrap_or(json!({})),
                    "scope": "turn",
                })
            } else {
                json!({ "permissions": {}, "scope": "turn" })
            }
        }
        "execCommandApproval" | "applyPatchApproval" => {
            json!({ "decision": if allowed { "approved" } else { "denied" } })
        }
        _ => json!({ "decision": if allowed { "accept" } else { "decline" } }),
    }
}

/// Codex takes only its own classification here, and blank fields are left out
/// rather than sent empty.
pub(super) fn feedback_params(request: &EngineFeedbackRequest) -> Value {
    let mut params = json!({
        "classification": "bug",
        "includeLogs": request.include_logs,
    });
    let object = params
        .as_object_mut()
        .expect("feedback params are an object");
    if let Some(reason) = request
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        object.insert("reason".into(), json!(reason));
    }
    if let Some(thread) = request
        .thread_id
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        object.insert("threadId".into(), json!(thread));
    }
    params
}

/// Codex promises the report's id. A reply without it is not the accepted shape
/// Orbyn knows.
pub(super) fn feedback_receipt(value: &Value) -> Result<EngineFeedbackReceipt> {
    let thread_id = text(value, "threadId")
        .ok_or_else(|| CoreError::Internal("feedback/upload answered without threadId".into()))?;
    Ok(EngineFeedbackReceipt {
        thread_id,
        prompt_hash: text(value, "promptHash"),
    })
}
