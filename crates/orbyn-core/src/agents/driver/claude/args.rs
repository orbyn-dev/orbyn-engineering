use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::settings::{base_settings, library_agents, run_settings_path};
use crate::agents::console::ConsoleSpec;
use crate::agents::driver::{CLAUDE_APPROVE_TOOL, RunSpec};

/// The exact command line for one run. Tests pin this.
pub fn run_args(spec: &RunSpec, mcp_config: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "-p",
        "--output-format",
        "stream-json",
        "--input-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--permission-mode",
        if spec.plan { "plan" } else { "default" },
        "--mcp-config",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    args.push(mcp_config.into());
    // Every settings source still loads, so the checkout's CLAUDE.md, skills and `.mcp.json` work: a real 2.1.318
    // run with `--setting-sources user` lost CLAUDE.md, the project's skills and the library's skills alike. What a
    // checkout's `.claude/settings.json` could do to Orbyn's write scope is shut off in the run's own `--settings`
    // instead (see `guarded_settings`). The library's agents come below, because `--add-dir` loads only skills.
    if let Some(agents) = library_agents(&spec.library) {
        args.push("--agents".into());
        args.push(agents.to_string().into());
    }
    // The run starts in the checkout, so the project's own instructions, hooks and
    // `.mcp.json` are the primary ones, and the library comes in with `--add-dir`:
    // on 2.1.150 that loads its skills and nothing else. A session begun before this
    // started in the library, and resumes there with the checkout added instead.
    for extra in [&spec.checkout, &spec.library] {
        if *extra != spec.cwd && (extra != &spec.library || spec.library != spec.checkout) {
            args.push("--add-dir".into());
            args.push(extra.clone().into());
        }
    }
    args.push("--permission-prompt-tool".into());
    args.push(CLAUDE_APPROVE_TOOL.into());
    if let Some(model) = &spec.model {
        args.push("--model".into());
        args.push(model.into());
    }
    if let Some(effort) = &spec.effort {
        args.push("--effort".into());
        args.push(effort.into());
    }
    args.push("--settings".into());
    args.push(run_settings_path(spec).into());
    if let Some(session) = &spec.resume {
        args.push("--resume".into());
        args.push(session.into());
    } else if let Some(session) = spec
        .confine
        .as_ref()
        .and_then(|rules| rules.fresh_session())
    {
        // A confined worker's first turn names its session, so the rules can allow that one
        // transcript before Claude has written it.
        args.push("--session-id".into());
        args.push(session.into());
    }
    args
}

/// Claude keeps a session under the folder it started in. Sessions begun when runs
/// started in the library are still there, so one found only under the library's
/// folder resumes from the library; everything else starts in the checkout.
pub fn session_cwd(root: &Path, checkout: &Path, session: Option<&str>) -> PathBuf {
    session_cwd_in(
        crate::agents::usage::history_root(crate::agents::usage::Engine::Claude).as_deref(),
        root,
        checkout,
        session,
    )
}

pub(super) fn session_cwd_in(
    history: Option<&Path>,
    root: &Path,
    checkout: &Path,
    session: Option<&str>,
) -> PathBuf {
    let (Some(history), Some(session)) = (history, session) else {
        return checkout.to_path_buf();
    };
    let has = |dir: &Path| {
        history
            .join(crate::agents::claude_history::project_folder_name(
                &dir.to_string_lossy(),
            ))
            .join(format!("{session}.jsonl"))
            .is_file()
    };
    if checkout != root && !has(checkout) && has(root) {
        root.to_path_buf()
    } else {
        checkout.to_path_buf()
    }
}

/// Claude's interactive command line on a run's session, started where the session
/// lives and given the other folder with `--add-dir`.
pub fn console_args(console: &ConsoleSpec<'_>) -> Vec<OsString> {
    let cwd = session_cwd(console.root, console.checkout, Some(console.session));
    let mut args: Vec<OsString> = vec!["--resume".into(), console.session.into()];
    if let Some(model) = console.model {
        args.extend(["--model".into(), model.into()]);
    }
    args.extend([
        "--permission-mode".into(),
        if console.plan { "plan" } else { "default" }.into(),
    ]);
    // The same protection as a run: a checkout's allow rules and hooks must not decide a tool call that Orbyn's
    // write scope covers, and the library's settings and agents apply as they do to a run. The person at the
    // terminal still answers every write, so a project's `allow` never skips that question.
    // The `mcp__*` ask rule a run adds in `launch_settings` is left off on purpose: the person is
    // at the keyboard and answers connector calls in their own CLI as they always have.
    args.extend([
        "--settings".into(),
        Value::Object(base_settings(console.root))
            .to_string()
            .into(),
    ]);
    if let Some(agents) = library_agents(console.root) {
        args.extend(["--agents".into(), agents.to_string().into()]);
    }
    for extra in [console.checkout, console.root] {
        if extra != cwd && (extra != console.root || console.root != console.checkout) {
            args.extend(["--add-dir".into(), extra.into()]);
        }
    }
    args
}

/// The command line for reading capabilities. These flags are what keep it inert:
/// a real 2.1.150 ran a `SessionStart` hook and started a user-level MCP server
/// without them, even in an empty folder, and did neither with them. Hooks and
/// servers both live in the user's own config, so a temp folder is not enough.
pub fn capability_args(mcp_config: &Path, settings: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "-p",
        "--output-format",
        "stream-json",
        "--input-format",
        "stream-json",
        "--verbose",
        "--no-session-persistence",
        "--strict-mcp-config",
        "--mcp-config",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    args.push(mcp_config.into());
    args.push("--settings".into());
    args.push(settings.into());
    args
}

/// Servers from a claude.ai account are not in any config file, so these stay out
/// of reach of the flags above.
pub(super) const CAPABILITY_ENV: &[(&str, &str)] = &[
    ("ENABLE_CLAUDEAI_MCP_SERVERS", "false"),
    ("CLAUDE_CODE_AUTO_CONNECT_IDE", "0"),
];
