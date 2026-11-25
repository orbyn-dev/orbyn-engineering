# Tools

What a running agent can call: Orbyn's per-run MCP server, its tools and the skills library.

| Module | Owns |
| --- | --- |
| `mcp` | The per-run MCP server behind a random per-run token |
| `mcp::tools` | Orbyn's own tools for a running agent: the lookups and argument helpers (`mod.rs`), the tool table (`definitions`), running a tool behind the gate (`dispatch`), and the bounds and checks for the guide, project, page and render tools (`guide`, `project`, `preview`, `render`) |
| `mcp::apps` | MCP Apps: a tool's `ui://` page shown beside its row |
| `mcp::flight` | The cap and single-flight on an app's tool-info read: a few engine sessions at once, identical concurrent reads share one, nothing kept |
| `skills` | Skills read in place from the workspace library |
| `local_skills` | The device-local skills area an approved Crew proposal writes into: one folder per skill slug, a `SKILL.md` with front matter and its own revision, written atomically, delivered through the MCP skills tools only |
| `claude_skills` | Claude Code's own skills, where its CLI reads them |
| `workflow_scripts` | The script a Claude Code workflow runs, for display |

The old paths `tools::mcp_tools` and `tools::mcp_apps` still resolve as aliases in `mod.rs`. Tests of the tools sit in `mcp/tools/tests.rs`.

## Stack budget

A tool call's debug-build cost is frames, not future payloads: the payloads on the device path
(`McpService::handle` 16 KiB, the device `act`/`list` futures 38 KiB) are small, while an
unoptimized build keeps every frame of the chain live at once. Measured on a 2 MiB test thread,
`orbyn_device_screenshot` through the transport needs about 1.4 MiB; before the device tools were
taken out of `run_orbyn_tool`'s match it needed about 1.8 MiB, and awaiting the device call
unboxed overflowed the thread. `run_orbyn_tool`'s frame is about 450 KiB here because its match
holds every arm's locals, so `call_orbyn_tool` dispatches the device tools through
`run_device_tool` before that frame exists (#1515). The device tool tests call through
`McpService::handle`, so a change that deepens the chain fails them instead of passing quietly;
re-measure with
`cargo test -p orbyn-core --lib -- tools::mcp::tools::tests::the_device_screenshot_succeeds_through_the_mcp_transport`.

`claude_skills` discovers Claude Code skill names and resolves its configuration
directory. The conversation composer uses discovery, and `workflow_scripts` uses
the configuration directory to locate scripts. The Claude-specific format does
not make this part of engine session lifecycle.

The crate's [group table](../../README.md#groups) lists every group.
