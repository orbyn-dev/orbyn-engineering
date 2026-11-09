use std::io::Read;

/// A one-off prompt on stdin, answered on stdout. Each call is recorded,
/// because in a real engine each one spends the user's usage.
pub(crate) fn one_shot() {
    let mut prompt = String::new();

    std::io::stdin().read_to_string(&mut prompt).expect("stdin");

    let scm = prompt.contains("Return only JSON with string fields");

    if let Ok(path) = std::env::var("ORBYN_FAKE_ONE_SHOT_FILE") {
        std::fs::write(&path, &prompt).expect("record");

        let args: Vec<String> = std::env::args().skip(1).collect();

        let args_path = std::path::Path::new(&path).with_extension("args");

        std::fs::write(args_path, args.join(" ")).expect("record args");

        if scm {
            let after = |flag: &str| {
                args.iter()
                    .position(|arg| arg == flag)
                    .and_then(|index| args.get(index + 1))
            };

            let config =
                |flag| after(flag).map(|path| std::fs::read_to_string(path).expect("owned config"));

            std::fs::write(
                std::path::Path::new(&path).with_extension("writing.json"),
                serde_json::json!({

                    "args":args,"cwd":std::env::current_dir().unwrap(),

                    "mcp":config("--mcp-config"),"settings":config("--settings"),

                    "cloud_mcp":std::env::var("ENABLE_CLAUDEAI_MCP_SERVERS").ok(),

                    "ide":std::env::var("CLAUDE_CODE_AUTO_CONNECT_IDE").ok(),

                })
                .to_string(),
            )
            .expect("record writing boundary");
        }
    }

    if scm {
        if let Ok(path) = std::env::var("ORBYN_FAKE_SCM_BARRIER") {
            let ready = std::path::Path::new(&path).with_extension("ready");

            let release = std::path::Path::new(&path).with_extension("release");

            std::fs::write(ready, "synthetic request admitted").expect("barrier ready");

            let started = std::time::Instant::now();

            while !release.exists() {
                assert!(
                    started.elapsed() < std::time::Duration::from_secs(10),
                    "synthetic barrier was not released"
                );

                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }

        if prompt.contains("[slow-scm-text]") {
            std::thread::sleep(std::time::Duration::from_millis(600));
        }

        if prompt.contains("[bad-scm-text]")
            || (prompt.contains("[bad-pr-text]") && !prompt.contains("string fields subject"))
        {
            println!("invalid structured text");
        } else if prompt.contains("[fenced-scm-text]") {
            // Claude's usual shape: the JSON inside one tagged Markdown fence.

            let fields = if prompt.contains("string fields subject") {
                serde_json::json!({"subject":"feat(git): fenced reply","body":"Inside a fence"})
            } else {
                serde_json::json!({"title":"feat(git): fenced reply","body":"## Summary\n\n```sh\nnpm test\n```"})
            };

            println!(
                "```json\n{}\n```",
                serde_json::to_string_pretty(&fields).unwrap()
            );
        } else if prompt.contains("string fields subject") {
            println!(
                "{}",
                serde_json::json!({"subject":"feat(git): describe the change","body":"Describe the actual diff"})
            );
        } else {
            println!(
                "{}",
                serde_json::json!({"title":"feat(git): describe the change","body":"## Summary\n- Describe the actual diff\n\n## Testing\n- Not run"})
            );
        }

        return;
    }

    if prompt.contains("[slow-title]") {
        std::thread::sleep(std::time::Duration::from_millis(600));
    }

    if prompt.contains("[bad-title]") {
        eprintln!("title failure");

        std::process::exit(4);
    }

    println!("\"A fresh title.\"\nextra line");
}
