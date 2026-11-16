//! One ephemeral CLI turn with builtins, MCP, hooks and customizations disabled.
//! The process stays under the existing ManagedChild owner. No login or provider
//! session is read by this adapter; credentials remain the CLI's responsibility.

use std::ffi::OsString;
use std::path::Path;

use orbyn_contracts::{AgUiEvent, ThreadUsage};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::ClaudeDriver;
use crate::agents::driver::pipes::{EngineProcess, resolve_engine, spawn_engine_with};
use crate::agents::driver::{ApprovalSpec, EngineEvent, EngineOutcome, ProbeDir, RunHandle};
use crate::agents::selection::FAST_MODE;
use crate::error::{CoreError, Result};
use crate::util::write_private;

const OUTPUT_BYTES: u64 = 64 * 1024;

#[cfg(test)]
mod tests;

fn refused() -> CoreError {
    CoreError::EngineUnavailable("Claude could not complete a bounded tool-free approval.".into())
}

/// 2.1.295 fixed --tools restrictions missing tools registered after launch.
/// Earlier versions and unparseable reports must not opt into this capability.
pub(super) fn version_supported(version: Option<&str>) -> bool {
    let Some(version) = version.and_then(|v| v.split_whitespace().next()) else {
        return false;
    };
    let parts = version
        .split('.')
        .map(str::parse::<u32>)
        .collect::<std::result::Result<Vec<_>, _>>();
    matches!(parts.as_deref(), Ok([major, minor, patch]) if (*major,*minor,*patch) >= (2,1,295))
}

/// Full Anthropic model IDs can be pinned directly. Account-dependent aliases
/// require the exact model reported by the active lead run, never a lookup table.
/// Receipt validation still compares the entire ID for equality.
pub(super) fn pinned_model(selected: &str, effective: Option<&str>) -> Option<String> {
    let full_id = |model: &str| {
        model.starts_with("claude-")
            && model.len() > "claude-".len()
            && model.len() <= 200
            && model
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
    };
    effective
        .or(Some(selected))
        .filter(|model| full_id(model))
        .map(str::to_owned)
}

fn args(spec: &ApprovalSpec, dir: &Path) -> Result<Vec<OsString>> {
    if !version_supported(spec.engine_version.as_deref())
        || spec.family != "claude"
        || spec.prompt.len() > 16 * 1024
        || pinned_model(&spec.model, None).as_deref() != Some(spec.model.as_str())
        || spec.max_output_tokens == 0
        || spec.max_output_tokens > 4096
        || spec.max_reported_tokens == 0
        || !spec.estimated_spend_stop_usd.is_finite()
        || spec.estimated_spend_stop_usd <= 0.0
        || spec.deadline <= std::time::Instant::now()
        || spec
            .options
            .iter()
            .any(|(key, value)| key != FAST_MODE || !["on", "off"].contains(&value.as_str()))
    {
        return Err(refused());
    }
    let settings = dir.join("settings.json");
    let mcp = dir.join("mcp.json");
    let mut config = json!({ "disableAllHooks": true });
    if let Some(fast) = spec.options.get(FAST_MODE) {
        config["fastMode"] = json!(fast == "on");
    }
    write_private(&settings, &serde_json::to_vec(&config)?)?;
    write_private(&mcp, br#"{"mcpServers":{}}"#)?;
    let mut args = [
        "-p",
        "--output-format",
        "json",
        "--tools",
        "",
        "--disallowedTools",
        "*",
        "--strict-mcp-config",
        "--safe-mode",
        "--no-session-persistence",
        "--disable-slash-commands",
        "--no-chrome",
        "--permission-mode",
        "dontAsk",
        "--max-turns",
        "1",
        "--mcp-config",
    ]
    .into_iter()
    .map(OsString::from)
    .collect::<Vec<_>>();
    args.push(mcp.into_os_string());
    args.extend([
        "--settings".into(),
        settings.into_os_string(),
        "--model".into(),
        spec.model.clone().into(),
        "--max-budget-usd".into(),
        spec.estimated_spend_stop_usd.to_string().into(),
    ]);
    if let Some(effort) = &spec.effort {
        args.extend(["--effort".into(), effort.into()]);
    }
    Ok(args)
}

pub(super) fn start(
    driver: &ClaudeDriver,
    spec: ApprovalSpec,
    events: mpsc::Sender<EngineEvent>,
) -> Result<RunHandle> {
    let dir = ProbeDir::new()?;
    let args = args(&spec, dir.path())?;
    let command = resolve_engine(&driver.program)?;
    let output = spec.max_output_tokens.to_string();
    let process = spawn_engine_with(
        &command,
        &args,
        dir.path(),
        &[
            ("ENABLE_CLAUDEAI_MCP_SERVERS", "false"),
            ("CLAUDE_CODE_AUTO_CONNECT_IDE", "0"),
            ("CLAUDE_CODE_MAX_OUTPUT_TOKENS", &output),
        ],
    )?;
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    let task = tokio::spawn(async move {
        let _dir = dir;
        let EngineProcess {
            mut child,
            mut stdin,
            stdout,
            stderr: _stderr,
        } = process;
        let work = async {
            stdin.write_all(spec.prompt.as_bytes()).await?;
            drop(stdin);
            let mut bytes = Vec::new();
            stdout
                .take(OUTPUT_BYTES + 1)
                .read_to_end(&mut bytes)
                .await?;
            if bytes.len() as u64 > OUTPUT_BYTES {
                return Err(refused());
            }
            let status = child.wait().await?;
            let (text, usage, accepted) = parse(&bytes, &spec)?;
            Ok::<_, CoreError>((text, usage, accepted && status.success()))
        };
        let reply = tokio::select! {
            biased;
            () = stop.cancelled() => Err(CoreError::RunClosed),
            () = tokio::time::sleep_until(tokio::time::Instant::from_std(spec.deadline)) => Err(refused()),
            result = work => result,
        };
        // Dropping the owned process here kills its process tree on every path.
        drop(child);
        let outcome = match reply {
            Ok((text, usage, accepted)) => {
                if events.send(EngineEvent::Usage(usage)).await.is_err() {
                    return;
                }
                if !accepted {
                    let _ = events
                        .send(EngineEvent::Finished(EngineOutcome::Failed(
                            "The reported approval exceeded its acceptance limits.".into(),
                        )))
                        .await;
                    return;
                }
                for event in [
                    AgUiEvent::TextMessageStart {
                        message_id: "approval".into(),
                        role: "assistant".into(),
                    }
                    .into(),
                    AgUiEvent::TextMessageContent {
                        message_id: "approval".into(),
                        delta: text,
                    }
                    .into(),
                    AgUiEvent::TextMessageEnd {
                        message_id: "approval".into(),
                    }
                    .into(),
                ] {
                    if events.send(event).await.is_err() {
                        return;
                    }
                }
                EngineOutcome::Completed
            }
            Err(_) if stop.is_cancelled() => EngineOutcome::Cancelled,
            Err(_) => EngineOutcome::Failed(
                "The approval was unavailable, invalid or over budget.".into(),
            ),
        };
        let _ = events.send(EngineEvent::Finished(outcome)).await;
    });
    Ok(RunHandle {
        cancel,
        task,
        input: None,
        control: None,
    })
}

fn parse(bytes: &[u8], spec: &ApprovalSpec) -> Result<(String, ThreadUsage, bool)> {
    let frame: Value = serde_json::from_slice(bytes).map_err(|_| refused())?;
    let usage = frame
        .get("usage")
        .and_then(Value::as_object)
        .ok_or_else(refused)?;
    let count = |name| usage.get(name).and_then(Value::as_u64).ok_or_else(refused);
    let input = count("input_tokens")?;
    let output = count("output_tokens")?;
    let cache_read = count("cache_read_input_tokens")?;
    let cache_write = count("cache_creation_input_tokens")?;
    let total = input
        .checked_add(output)
        .and_then(|n| n.checked_add(cache_read))
        .and_then(|n| n.checked_add(cache_write))
        .ok_or_else(refused)?;
    let cost = frame
        .get("total_cost_usd")
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite() && *n >= 0.0)
        .ok_or_else(refused)?;
    let model_matches = frame
        .get("modelUsage")
        .and_then(Value::as_object)
        .is_some_and(|models| models.len() == 1 && models.contains_key(&spec.model));
    let text = frame.get("result").and_then(Value::as_str).unwrap_or("");
    let accepted = frame.get("type").and_then(Value::as_str) == Some("result")
        && frame.get("subtype").and_then(Value::as_str) == Some("success")
        && frame.get("is_error").and_then(Value::as_bool) == Some(false)
        && frame.get("num_turns").and_then(Value::as_u64) == Some(1)
        && model_matches
        && !text.is_empty()
        && text.len() <= 4096
        && output <= u64::from(spec.max_output_tokens)
        && total <= spec.max_reported_tokens
        && cost <= spec.estimated_spend_stop_usd;
    Ok((
        text.chars().take(4096).collect(),
        ThreadUsage {
            total_tokens: Some(total),
            context_window: None,
            raw: json!({ "input_tokens": input, "output_tokens": output, "cache_read_input_tokens": cache_read, "cache_creation_input_tokens": cache_write, "estimated_cost_usd": cost }),
        },
        accepted,
    ))
}
