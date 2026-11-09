//! A deterministic protocol double. It speaks each engine's wire format over
//! real pipes or sockets so tests exercise real parsing. Frames are written by
//! hand from the Codex 0.166.0 generated schema, Claude Code's stream-json
//! output, ACP v1 and the AG-UI 1.0 schema; none are recorded from live engines.
//!
//! Prompts carry scenario tags:
//! `[approve]`, `[crash-in-approval]`, `[hang]`, `[late]`, `[fail]`,
//! `[unknown-request]`, `[mcp-tools]`, `[question]`. With `ORBYN_FAKE_LOSE_SESSION`
//! set, Claude and Codex refuse every resume.

use serde_json::json;

mod acp;
mod agui;
mod claude;
mod codex;
mod one_shot;
mod pty;
mod shared;

use acp::acp;
use agui::agui;
use claude::claude;
use codex::codex;
use one_shot::one_shot;
use pty::pty;
use shared::{console, record_launch, record_pid};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    record_pid();
    let first = args.first().map(String::as_str).unwrap_or_default();
    match first {
        "api" if std::env::var_os("ORBYN_FAKE_GH_STAR_STATE").is_some() => {
            let state =
                std::fs::read_to_string(std::env::var_os("ORBYN_FAKE_GH_STAR_STATE").unwrap())
                    .unwrap();
            if let Some(path) = std::env::var_os("ORBYN_FAKE_GH_ARGUMENTS") {
                std::fs::write(
                    path,
                    serde_json::to_vec(&json!({"args":args,"pid":std::process::id()})).unwrap(),
                )
                .unwrap();
            }
            if state.trim() == "hang" {
                std::thread::sleep(std::time::Duration::from_secs(30));
            }
            std::process::exit(if state.trim() == "starred" { 0 } else { 1 });
        }
        "mcp" if args.get(1).map(String::as_str) == Some("list") => {
            record_launch("claude-mcp-list", &args);
            // `claude mcp list` as 2.1.318 prints it.
            let text = std::env::var("ORBYN_FAKE_MCP_LIST").unwrap_or_else(|_| {
                "Checking MCP server health…\n\n\
                 github: npx -y github-mcp - ✔ Connected\n\
                 postgres-dev: pg-mcp - ✘ Failed to connect — Command not found: pg-mcp\n\
                 claude.ai Vercel: https://mcp.vercel.com - ✔ Connected\n\
                 claude.ai Google Drive: https://drive.example/mcp - ! Needs authentication\n"
                    .into()
            });
            print!("{text}");
        }
        "--version" => println!(
            "{}",
            std::env::var("FAKE_VERSION").unwrap_or("0.166.0".into())
        ),
        "auth" => println!("{}", json!({ "loggedIn": true, "authMethod": "fake" })),
        "login" => println!("Logged in using fake"),
        "app-server" => {
            record_launch("codex", &args);
            codex();
        }
        "-p" if !args.iter().any(|arg| arg == "--input-format") => one_shot(),
        "exec" => one_shot(),
        "-p" => {
            record_launch("claude", &args);
            if let Some(at) = args.iter().position(|arg| arg == "--settings")
                && let Ok(text) = std::fs::read_to_string(&args[at + 1])
            {
                record_launch("claude-settings", &[text]);
            }
            claude(&args);
        }
        "acp" => {
            record_launch("acp", &args);
            acp();
        }
        "agui" => agui(&args),
        "pty" => pty(),
        "--resume" => console("claude-console", &args),
        "resume" => console("codex-console", &args),
        other => {
            eprintln!("unknown mode {other}");
            std::process::exit(2);
        }
    }
}
