use serde_json::{Value, json};

use super::{TOOLS_MAX_PAGES, text};
use crate::agents::driver::MCP_SERVER_NAME;
use crate::agents::driver::rpc::RpcClient;

/// One status list, following `nextCursor` while the shared deadline allows. Returns the
/// entries gathered as `{ "data": [...] }` (or `None` if the first page failed) and whether
/// the list was cut short by the deadline or the page cap.
pub(super) async fn listed_pages(
    rpc: &RpcClient,
    method: &str,
    deadline: std::time::Instant,
) -> (Option<Value>, bool) {
    let left = || deadline.saturating_duration_since(std::time::Instant::now());
    let mut gathered: Vec<Value> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut got_any = false;
    for _ in 0..TOOLS_MAX_PAGES {
        if left().is_zero() {
            return (got_any.then(|| json!({ "data": gathered })), true);
        }
        let params = cursor
            .as_ref()
            .map_or(json!({}), |at| json!({ "cursor": at }));
        let Ok(page) = rpc.call(method, params, left()).await else {
            return (got_any.then(|| json!({ "data": gathered })), got_any);
        };
        got_any = true;
        if let Some(items) = page.get("data").and_then(Value::as_array) {
            gathered.extend(items.iter().cloned());
        }
        match page.get("nextCursor").and_then(Value::as_str) {
            Some(next) if !next.is_empty() => cursor = Some(next.to_owned()),
            _ => return (Some(json!({ "data": gathered })), false),
        }
    }
    (Some(json!({ "data": gathered })), true)
}

/// The Tools list for Codex, from what `mcpServerStatus/list`, `app/list` and `plugin/list`
/// answered. MCP servers other than Orbyn's own and Codex's connector bridge are the
/// person's; connectors are the apps the person can reach; plugins are the installed and
/// enabled ones. Codex does not report built-in tools or skills here.
pub(super) fn codex_tools(
    servers: Option<&Value>,
    apps: Option<&Value>,
    plugins: Option<&Value>,
) -> orbyn_contracts::EngineTools {
    use crate::agents::tools_report::{
        assemble, counted, description, detail, entry, group, reason,
    };
    use orbyn_contracts::ToolState;
    let mut groups = Vec::new();
    if let Some(list) = servers
        .and_then(|value| value.get("data"))
        .and_then(Value::as_array)
    {
        let mut mine = Vec::new();
        let mut orbyn = None;
        for server in list {
            let Some(name) = text(server, "name") else {
                continue;
            };
            let tools = server
                .get("tools")
                .and_then(Value::as_object)
                .map_or(0, serde_json::Map::len);
            if name == MCP_SERVER_NAME {
                orbyn = Some(tools);
                continue;
            }
            // The bridge to connectors and the bundled app server are not the person's.
            if name == "codex_apps" || name == "codex_app" {
                continue;
            }
            let failure = server
                .get("toolsError")
                .and_then(Value::as_str)
                .filter(|why| !why.is_empty());
            let signed_out =
                server.get("authStatus").and_then(Value::as_str) == Some("notLoggedIn");
            let state = match (failure, signed_out) {
                (_, true) => ToolState::NeedsSignIn,
                (Some(_), false) => ToolState::Failed,
                (None, false) => ToolState::Ready,
            };
            let mut one = entry(&name, state);
            one.tools = u32::try_from(tools).ok().filter(|count| *count > 0);
            one.description = server
                .pointer("/serverInfo/description")
                .and_then(Value::as_str)
                .and_then(description);
            if let Some(why) = failure {
                one.reason = Some(reason(why));
                one.detail = Some(detail(why));
            }
            mine.push(one);
        }
        if let Some(count) = orbyn {
            groups.push(counted("orbyn", count));
        }
        groups.push(group("mcp", mine));
    }
    if let Some(list) = apps
        .and_then(|value| value.get("data"))
        .and_then(Value::as_array)
    {
        // Thousands of apps are on offer; only the ones the person can reach are theirs.
        let mut mine = Vec::new();
        for app in list.iter().filter(|app| {
            app.get("isAccessible").and_then(Value::as_bool) == Some(true)
                && app.get("isEnabled").and_then(Value::as_bool) != Some(false)
        }) {
            let Some(name) = text(app, "name") else {
                continue;
            };
            let mut one = entry(&name, ToolState::Ready);
            one.description = app
                .get("description")
                .and_then(Value::as_str)
                .and_then(description);
            one.source = Some("Codex app".into());
            mine.push(one);
        }
        groups.push(group("connectors", mine));
    }
    if let Some(markets) = plugins
        .and_then(|value| value.get("marketplaces"))
        .and_then(Value::as_array)
    {
        let mut mine = Vec::new();
        for plugin in markets
            .iter()
            .filter_map(|market| market.get("plugins").and_then(Value::as_array))
            .flatten()
            .filter(|plugin| {
                plugin.get("installed").and_then(Value::as_bool) == Some(true)
                    && plugin.get("enabled").and_then(Value::as_bool) == Some(true)
            })
        {
            let Some(name) = plugin
                .pointer("/interface/displayName")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| text(plugin, "name"))
            else {
                continue;
            };
            let mut one = entry(&name, ToolState::Ready);
            one.description = plugin
                .pointer("/interface/shortDescription")
                .and_then(Value::as_str)
                .and_then(description);
            mine.push(one);
        }
        groups.push(group("plugins", mine));
    }
    assemble("codex", "check", groups, true)
}
