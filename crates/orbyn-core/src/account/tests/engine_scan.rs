//! The app names one Engine crate, `orbyn-device-client`, by tag (rule D6), and its lock holds
//! one revision. The scan reads the manifests, the lock and every Rust file; names are built from
//! pieces so this file does not match itself. The app's own crates named like Engine ones
//! (`orbyn-contracts`, `orbyn-core`) are fine and are checked as path dependencies.

use std::path::{Path, PathBuf};

const TAG: &str = "engine-v0.2.4";
const REVISION: &str = "db006e3bdf4e5dc411af1464665f63c8679172ce";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn files(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !matches!(name.as_str(), "target" | "node_modules" | ".git") {
                files(&path, ext, out);
            }
        } else if name.ends_with(ext) {
            out.push(path);
        }
    }
}

fn other_engine_crates() -> Vec<String> {
    [
        "client",
        "sync",
        "crypto",
        "auth_client",
        "planner",
        "translog",
        "store",
        "server",
        "worker",
        "witness",
        "wasm",
        "cli",
        "keyservice",
        "keyservice_client",
    ]
    .iter()
    .map(|name| format!("orbyn_{name}"))
    // The app's MCP server is also named `orbyn_engineering` in strings, so only its use as a crate counts.
    .chain([format!("orbyn_{}::", "engineering")])
    .collect()
}

#[test]
fn no_source_file_names_another_engine_crate() {
    let mut sources = Vec::new();
    for dir in ["crates", "desktop/src-tauri"] {
        files(&root().join(dir), ".rs", &mut sources);
    }
    assert!(sources.len() > 50, "the scan found no source tree");
    let needles = other_engine_crates();
    let mut hits = Vec::new();
    for path in sources {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        for line in text.lines().filter(|l| !l.trim_start().starts_with("//")) {
            for needle in &needles {
                let found = line.match_indices(needle.as_str()).any(|(at, _)| {
                    let after = line[at + needle.len()..].chars().next();
                    !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
                });
                if found && !line.contains("other_engine_crates") {
                    hits.push(format!("{}: {needle}", path.display()));
                }
            }
        }
    }
    assert!(hits.is_empty(), "{hits:?}");
}

#[test]
fn the_manifests_depend_on_the_device_client_by_tag_and_on_no_other_engine_crate() {
    let mut manifests = Vec::new();
    files(&root(), "Cargo.toml", &mut manifests);
    let mut seen = 0;
    for path in manifests {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        for line in text.lines().map(str::trim).filter(|l| !l.starts_with('#')) {
            let name = line.split('=').next().unwrap_or_default().trim();
            if line.contains("orbyn-engine") && line.contains("git") {
                assert!(name == "orbyn-device-client", "{}: {line}", path.display());
                assert!(line.contains(&format!("tag = \"{TAG}\"")), "{line}");
                seen += 1;
            }
            let own = ["orbyn-contracts", "orbyn-core"];
            if own.contains(&name) && line.contains('=') {
                assert!(line.contains("path"), "{}: {line}", path.display());
            }
            let banned = other_engine_crates()
                .into_iter()
                .map(|n| n.trim_end_matches(':').replace('_', "-"))
                .any(|n| n == name);
            assert!(!banned, "{}: {line}", path.display());
        }
    }
    assert_eq!(seen, 1, "exactly one Engine dependency");
}

#[test]
fn the_lock_holds_one_engine_revision_and_the_pinned_tag_points_at_it() {
    let lock = std::fs::read_to_string(root().join("Cargo.lock")).unwrap();
    let engine: Vec<&str> = lock
        .lines()
        .filter(|l| l.starts_with("source =") && l.contains("github.com/orbyn-dev/orbyn-engine"))
        .collect();
    assert!(!engine.is_empty());
    for line in &engine {
        assert!(line.ends_with(&format!("#{REVISION}\"")), "{line}");
    }
    assert!(
        engine
            .iter()
            .any(|l| l.contains(&format!("?tag={TAG}#{REVISION}")))
    );
}
