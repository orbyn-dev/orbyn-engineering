# Tools

What a running agent can call: Orbyn's per-run MCP server, its tools and the skills library.

| Module | Owns |
| --- | --- |
| `mcp` | The per-run MCP server behind a random per-run token |
| `mcp::tools` | Orbyn's own tools for a running agent: the lookups and argument helpers (`mod.rs`), the tool table (`definitions`), running a tool behind the gate (`dispatch`), and the bounds and checks for the guide, project, page and render tools (`guide`, `project`, `preview`, `render`) |
| `mcp::apps` | MCP Apps: a tool's `ui://` page shown beside its row |
| `skills` | Skills read in place from the workspace library |
| `claude_skills` | Claude Code's own skills, where its CLI reads them |
| `workflow_scripts` | The script a Claude Code workflow runs, for display |

The old paths `tools::mcp_tools` and `tools::mcp_apps` still resolve as aliases in `mod.rs`. Tests of the tools sit in `mcp/tools/tests.rs`.

`claude_skills` discovers Claude Code skill names and resolves its configuration
directory. The conversation composer uses discovery, and `workflow_scripts` uses
the configuration directory to locate scripts. The Claude-specific format does
not make this part of engine session lifecycle.

The crate's [group table](../../README.md#groups) lists every group.
