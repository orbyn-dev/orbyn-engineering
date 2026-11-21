use std::ffi::OsString;

use serde_json::{Value, json};

use super::tools::{codex_tools, listed_pages};
use super::{CALL_TIMEOUT, CodexDriver, TOOLS_DEADLINE};
use crate::agents::driver::pipes::{resolve_engine, spawn_engine};
use crate::agents::driver::rpc::RpcClient;
use crate::error::{CoreError, Result};

/// An app-server kept alive only for a read. Dropping it ends the process.
pub(super) struct ShortSession {
    pub(super) rpc: RpcClient,
    _process: crate::host::process::ManagedChild,
}

impl CodexDriver {
    pub(super) async fn short_session(&self) -> Result<ShortSession> {
        self.short_session_in(&std::env::temp_dir()).await
    }

    /// The three status lists of a short app-server, reduced to names and states. Each is
    /// read on its own: one that does not answer leaves its group out rather than hiding
    /// the rest. The app and plugin lists are very large, so they never leave this function whole.
    pub(super) async fn read_tools(
        &self,
        cwd: &std::path::Path,
    ) -> Option<orbyn_contracts::EngineTools> {
        let deadline = std::time::Instant::now() + TOOLS_DEADLINE;
        let left = || deadline.saturating_duration_since(std::time::Instant::now());
        let session = self.short_session_experimental(cwd).await.ok()?;
        let (servers, servers_cut) =
            listed_pages(&session.rpc, "mcpServerStatus/list", deadline).await;
        let (apps, apps_cut) = listed_pages(&session.rpc, "app/list", deadline).await;
        let plugins = if left().is_zero() {
            None
        } else {
            session
                .rpc
                .call("plugin/list", json!({}), left())
                .await
                .ok()
        };
        if servers.is_none() && apps.is_none() && plugins.is_none() {
            return None;
        }
        let mut tools = codex_tools(servers.as_ref(), apps.as_ref(), plugins.as_ref());
        tools.incomplete = servers_cut || apps_cut;
        Some(tools)
    }

    /// One tool's descriptor from the app's own server: `mcpServerStatus/list` limited
    /// to that server. A server the engine no longer has, one that failed or needs a
    /// sign-in, or a list that answered with an error is an error rather than an
    /// unknown tool, so a caller never reports empty metadata as fact.
    pub(super) async fn app_tool_info(
        &self,
        cwd: &std::path::Path,
        server: &str,
        tool: &str,
    ) -> Result<Option<Value>> {
        let unreachable = || {
            CoreError::EngineUnavailable(
                "The app's server could not be asked for its tool list.".into(),
            )
        };
        let session = self
            .short_session_experimental(cwd)
            .await
            .map_err(|_| unreachable())?;
        let reply = session
            .rpc
            .call(
                "mcpServerStatus/list",
                json!({ "serverName": server, "detail": "toolsAndAuthOnly" }),
                CALL_TIMEOUT,
            )
            .await
            .map_err(|_| unreachable())?;
        tool_in_reply(&reply, server, tool)
    }

    /// A short app-server that takes the experimental calls the status lists need.
    async fn short_session_experimental(&self, cwd: &std::path::Path) -> Result<ShortSession> {
        let command = resolve_engine(&self.program)?;
        let args = [OsString::from("app-server")];
        let process = spawn_engine(&command, &args, cwd)?;
        let (rpc, _inbound) = RpcClient::start(process.stdin, process.stdout, false);
        rpc.call(
            "initialize",
            json!({
                "clientInfo": { "name": "orbyn", "version": env!("CARGO_PKG_VERSION") },
                "capabilities": { "experimentalApi": true },
            }),
            CALL_TIMEOUT,
        )
        .await?;
        rpc.notify("initialized", None).await?;
        Ok(ShortSession {
            rpc,
            _process: process.child,
        })
    }

    pub(super) async fn short_session_in(&self, cwd: &std::path::Path) -> Result<ShortSession> {
        let command = resolve_engine(&self.program)?;
        let args = [OsString::from("app-server")];
        let process = spawn_engine(&command, &args, cwd)?;
        let (rpc, _inbound) = RpcClient::start(process.stdin, process.stdout, false);
        rpc.call(
            "initialize",
            json!({ "clientInfo": { "name": "orbyn", "version": env!("CARGO_PKG_VERSION") } }),
            CALL_TIMEOUT,
        )
        .await?;
        rpc.notify("initialized", None).await?;
        Ok(ShortSession {
            rpc,
            _process: process.child,
        })
    }
}

/// One tool's descriptor from a `mcpServerStatus/list` reply that was asked for one server.
/// The entry is picked by exact name, so an engine that ignores `serverName` and lists
/// other servers (or only another one) can never give this server another's tool: that is
/// "not available", an error. An entry with no `tools` object says nothing about its
/// tools, so that state is unknown (an error), not "not advertised" (`None`).
fn tool_in_reply(reply: &Value, server: &str, tool: &str) -> Result<Option<Value>> {
    let entry = reply
        .get("data")
        .and_then(Value::as_array)
        .and_then(|servers| {
            servers
                .iter()
                .find(|entry| entry.get("name").and_then(Value::as_str) == Some(server))
        })
        .ok_or_else(|| CoreError::EngineUnavailable("The app's server is not available.".into()))?;
    let broken = entry
        .get("toolsError")
        .and_then(Value::as_str)
        .is_some_and(|why| !why.trim().is_empty())
        || entry.get("authStatus").and_then(Value::as_str) == Some("notLoggedIn")
        || matches!(
            entry.get("runtimeStatus").and_then(Value::as_str),
            Some("failed" | "authenticationRequired" | "disabled" | "cancelled")
        );
    if broken {
        return Err(CoreError::EngineUnavailable(
            "The app's server cannot list its tools.".into(),
        ));
    }
    let Some(tools) = entry.get("tools").and_then(Value::as_object) else {
        return Err(CoreError::EngineUnavailable(
            "The app's server did not say which tools it has.".into(),
        ));
    };
    if let Some(found) = tools.get(tool) {
        return Ok(Some(found.clone()));
    }
    if matches!(
        entry.get("runtimeStatus").and_then(Value::as_str),
        Some("notStarted" | "starting")
    ) {
        return Err(CoreError::EngineUnavailable(
            "The app's server is still starting, so it listed no tools.".into(),
        ));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, description: &str) -> Value {
        json!({
            "name": name, "authStatus": "unsupported", "runtimeStatus": "connected",
            "tools": { "show_widget": { "name": "show_widget", "description": description } },
        })
    }

    fn unavailable(result: Result<Option<Value>>) -> bool {
        matches!(result, Err(CoreError::EngineUnavailable(_)))
    }

    #[test]
    fn mcp_app_a_reply_without_a_tools_object_is_unknown_not_unadvertised() {
        let base = |extra: Value| {
            let mut entry = json!({ "name": "demo", "authStatus": "unsupported", "runtimeStatus": "connected" });
            entry
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            json!({ "data": [entry] })
        };
        for reply in [
            base(json!({})),
            base(json!({ "tools": null })),
            base(json!({ "tools": [{ "name": "show_widget" }] })),
            base(json!({ "tools": "show_widget" })),
        ] {
            assert!(
                unavailable(tool_in_reply(&reply, "demo", "show_widget")),
                "{reply}"
            );
        }
        // An object that does not list the tool is still the honest "not advertised".
        let empty = base(json!({ "tools": {} }));
        assert!(matches!(
            tool_in_reply(&empty, "demo", "show_widget"),
            Ok(None)
        ));
    }

    #[test]
    fn mcp_app_the_exact_server_name_picks_the_entry_in_a_multi_entry_reply() {
        let reply = json!({ "data": [
            entry("other", "OTHER"), entry("demo", "DEMO"), entry("demo-two", "TWO"),
            entry("Demo", "CASE"), entry(" demo", "SPACE"),
        ] });
        let found = tool_in_reply(&reply, "demo", "show_widget")
            .unwrap()
            .unwrap();
        assert_eq!(found["description"], "DEMO");
        let found = tool_in_reply(&reply, "demo-two", "show_widget")
            .unwrap()
            .unwrap();
        assert_eq!(found["description"], "TWO");
    }

    #[test]
    fn mcp_app_a_reply_that_ignores_the_server_name_cannot_yield_another_servers_tool() {
        // An engine that ignored `serverName` and answered with other servers only.
        let reply = json!({ "data": [entry("other", "OTHER"), entry("third", "THIRD")] });
        assert!(unavailable(tool_in_reply(&reply, "demo", "show_widget")));
        // Nor does a near name, or a tool whose own name matches but whose server does not.
        let reply = json!({ "data": [entry("demo2", "NEAR"), entry("DEMO", "UPPER")] });
        assert!(unavailable(tool_in_reply(&reply, "demo", "show_widget")));
        let empty = json!({ "data": [] });
        assert!(unavailable(tool_in_reply(&empty, "demo", "show_widget")));
        assert!(unavailable(tool_in_reply(
            &json!({}),
            "demo",
            "show_widget"
        )));
    }

    #[test]
    fn mcp_app_a_broken_or_starting_entry_is_never_none() {
        for entry in [
            json!({ "name": "demo", "tools": {}, "toolsError": "x" }),
            json!({ "name": "demo", "tools": {}, "authStatus": "notLoggedIn" }),
            json!({ "name": "demo", "tools": {}, "runtimeStatus": "failed" }),
            json!({ "name": "demo", "tools": {}, "runtimeStatus": "starting" }),
        ] {
            let reply = json!({ "data": [entry] });
            assert!(
                unavailable(tool_in_reply(&reply, "demo", "show_widget")),
                "{reply}"
            );
        }
    }

    /// A real `mcpServerStatus/list` reply asked for one server, captured from a
    /// 0.166.0 app-server with two stdio servers configured. In the same session the
    /// call without `serverName` listed both servers and started both, while this one
    /// listed and started only `alpha`; `runtimeStatus` is null on the live entry.
    const RECORDED: &str = include_str!("../fixtures/codex-0.166.0-mcp-server-status.json");

    #[test]
    fn mcp_app_a_recorded_server_status_reply_reads_as_advertised() {
        let reply: Value = serde_json::from_str(RECORDED).unwrap();
        let found = tool_in_reply(&reply, "alpha", "alpha_tool")
            .unwrap()
            .unwrap();
        assert_eq!(found["name"], "alpha_tool");
        assert_eq!(found["description"], "alpha_tool from alpha");
        assert_eq!(found["inputSchema"]["properties"]["text"]["type"], "string");
        // The reply names this one server only, so the other configured server is an error.
        assert!(unavailable(tool_in_reply(&reply, "beta", "beta_tool")));
        // The live entry's null `runtimeStatus` is neither broken nor still starting, so a
        // tool it does not advertise is the honest "not advertised".
        assert!(matches!(
            tool_in_reply(&reply, "alpha", "missing"),
            Ok(None)
        ));
    }
}
