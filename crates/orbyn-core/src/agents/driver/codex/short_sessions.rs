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
        let entry = reply
            .get("data")
            .and_then(Value::as_array)
            .and_then(|servers| {
                servers
                    .iter()
                    .find(|entry| entry.get("name").and_then(Value::as_str) == Some(server))
            })
            .ok_or_else(|| {
                CoreError::EngineUnavailable("The app's server is not available.".into())
            })?;
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
        let found = entry
            .get("tools")
            .and_then(Value::as_object)
            .and_then(|tools| tools.get(tool))
            .cloned();
        if found.is_some() {
            return Ok(found);
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
