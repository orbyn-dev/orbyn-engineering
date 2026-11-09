use std::ffi::OsString;

use serde_json::json;

use super::tools::{codex_tools, listed_pages};
use super::{CALL_TIMEOUT, CodexDriver, TOOLS_DEADLINE};
use crate::agents::driver::pipes::{resolve_engine, spawn_engine};
use crate::agents::driver::rpc::RpcClient;
use crate::error::Result;

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
