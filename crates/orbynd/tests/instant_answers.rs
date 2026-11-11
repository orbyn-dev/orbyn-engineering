//! Slow reads answer from the last known value. The webview holds a handful of
//! connections to the service, so a request that waits seconds on a forge starves the
//! quick ones behind it. A stand-in `gh` that sleeps and counts its calls stands for a
//! slow forge; these tests check that a second request does not reach it, that
//! concurrent first requests share one read, and what a write, a forced refresh and a
//! failing forge do to what is kept.

#![cfg(unix)]

mod support;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use support::{Options, Service, TempDir, run_git, setup};

/// A forge that takes `GH_DELAY` seconds per call when that file exists, fails every
/// call while `GH_FAIL` exists, and names a pull request after its folder and `GH_TITLE`,
/// so what one project's answer holds is never another's.
const GH: &str = r#"#!/bin/sh
echo "$@" >> "$GH_LOG"
[ -f "$GH_DELAY" ] && sleep "$(cat "$GH_DELAY")"
case "$1 $2" in
  "--version "*|"auth status"|"api user") ;;
  *) [ -f "$GH_FAIL" ] && { echo "the forge is down" >&2; exit 1; } ;;
esac
title="$(basename "$PWD") $(cat "$GH_TITLE" 2>/dev/null)"
pull() {
  echo "{\"number\":$1,\"title\":\"$title\",\"url\":\"https://github.com/o/r/pull/$1\",\"state\":\"$(cat "$GH_STATE")\",\"headRefName\":\"$2\",\"baseRefName\":\"main\",\"author\":{\"login\":\"me\"},\"isDraft\":false,\"labels\":[],\"statusCheckRollup\":[]$3}"
}
case "$1 $2" in
  "pr view") pull 12 feature ',"body":"Details","comments":[],"reviews":[],"mergeable":"MERGEABLE","headRefOid":"abc","baseRefOid":"def","additions":3,"reviewRequests":[]' ;;
  "pr checks") echo '[{"name":"ci","state":"SUCCESS","link":"x","bucket":"pass"}]' ;;
  "pr list") echo "[$(pull 3 h),$(pull 4 i)]" ;;
  "pr merge") echo MERGED > "$GH_STATE" ;;
  "api user") echo '{"login":"me"}' ;;
  "auth status") echo "  Logged in to github.com account octo (keyring)" >&2 ;;
  "--version "*) echo "gh version 2.0" ;;
esac
"#;

/// What the slow forge costs per call.
const FORGE_MS: u64 = 400;
/// What an answer from memory may cost, with room for a busy test machine.
const INSTANT: Duration = Duration::from_millis(100);

struct Fixture {
    dir: TempDir,
    service: Service,
    project: String,
    log: PathBuf,
}

async fn fixture() -> Fixture {
    let dir = TempDir::new();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let gh = bin.join("gh");
    std::fs::write(&gh, GH).unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    let file = |name: &str| dir.path().join(name).to_string_lossy().into_owned();
    std::fs::write(dir.path().join("gh-state"), "OPEN").unwrap();
    let env = vec![
        (
            "PATH".to_owned(),
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        ),
        ("GH_LOG".to_owned(), file("gh-log")),
        ("GH_STATE".to_owned(), file("gh-state")),
        ("GH_DELAY".to_owned(), file("gh-delay")),
        ("GH_FAIL".to_owned(), file("gh-fail")),
        ("GH_TITLE".to_owned(), file("gh-title")),
    ];
    let (service, thread) = setup(
        &dir,
        Options {
            env,
            ..Options::default()
        },
        "codex",
    )
    .await;
    let project = service.json(&format!("/api/threads/{thread}")).await["project_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let checkout = dir.path().join("library/repo/project");
    host(&checkout);
    let log = dir.path().join("gh-log");
    Fixture {
        dir,
        service,
        project,
        log,
    }
}

/// Points origin at a GitHub-shaped URL, so forge routes go through `gh`.
fn host(checkout: &std::path::Path) {
    run_git(checkout, &["config", "user.email", "t@example.com"]);
    run_git(checkout, &["config", "user.name", "T"]);
    run_git(
        checkout,
        &["remote", "add", "origin", "https://github.com/o/r.git"],
    );
}

impl Fixture {
    fn slow(&self) {
        std::fs::write(
            self.dir.path().join("gh-delay"),
            format!("{}", FORGE_MS as f64 / 1000.0),
        )
        .unwrap();
    }

    fn set(&self, name: &str, text: &str) {
        std::fs::write(self.dir.path().join(name), text).unwrap();
    }

    /// How many times the forge was asked with arguments starting with `prefix`.
    fn asked(&self, prefix: &str) -> usize {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter(|line| line.starts_with(prefix))
            .count()
    }

    /// Every call the forge has had.
    fn calls(&self) -> usize {
        self.asked("")
    }

    async fn timed(&self, path: &str) -> (Duration, Value) {
        let started = Instant::now();
        let response = self.service.get(path).send().await.unwrap();
        assert!(
            response.status().is_success(),
            "{path}: {}",
            response.status()
        );
        let body = response.json().await.unwrap();
        (started.elapsed(), body)
    }

    /// A second project in its own repository, so keys can be told apart.
    async fn other_project(&self) -> String {
        let folder = self.dir.path().join("other");
        std::fs::create_dir_all(&folder).unwrap();
        run_git(&folder, &["init", "-q", "-b", "main"]);
        host(&folder);
        self.service.project(&folder, true).await
    }

    /// Reads the shell's event stream until `what` is announced.
    async fn announced(&self, stream: &mut reqwest::Response, what: &str) -> bool {
        let wanted = format!(r#""type":"data_changed","what":"{what}""#);
        let mut seen = String::new();
        let waited = tokio::time::timeout(Duration::from_secs(10), async {
            while !seen.contains(&wanted) {
                match stream.chunk().await {
                    Ok(Some(chunk)) => seen.push_str(&String::from_utf8_lossy(&chunk)),
                    _ => return false,
                }
            }
            true
        })
        .await;
        waited.unwrap_or(false)
    }
}

fn median(times: &mut [Duration]) -> Duration {
    times.sort();
    times[times.len() / 2]
}

#[tokio::test]
async fn a_repeated_read_never_reaches_the_slow_forge() {
    let f = fixture().await;
    f.slow();
    let project = f.project.clone();
    let paths = [
        "/api/pulls".to_owned(),
        format!("/api/projects/{project}/pulls/12"),
        format!("/api/projects/{project}/pulls/12/checks"),
    ];
    for path in &paths {
        let (cold, _) = f.timed(path).await;
        assert!(cold >= Duration::from_millis(FORGE_MS), "{path}: {cold:?}");
        // The old behaviour, a real read on every call, is what `refresh` still does.
        let mut forced = Vec::new();
        let mut kept = Vec::new();
        for _ in 0..3 {
            forced.push(f.timed(&format!("{path}?refresh=1")).await.0);
        }
        let before = f.calls();
        for _ in 0..40 {
            kept.push(f.timed(path).await.0);
        }
        assert_eq!(f.calls(), before, "{path}: the forge was asked again");
        let slowest = *kept.iter().max().unwrap();
        eprintln!(
            "{path}: first {cold:?}, forced p50 {:?}, kept p50 {:?} max {slowest:?}",
            median(&mut forced),
            median(&mut kept),
        );
        assert!(slowest < INSTANT, "{path}: {slowest:?}");
    }
    // The first call and the three forced ones read the list; the 40 each did not.
    assert_eq!(f.asked("pr list"), 4);
}

#[tokio::test]
async fn callers_that_arrive_together_share_one_read() {
    let f = fixture().await;
    f.slow();
    let project = f.project.clone();
    let reads = (0..12).map(|n| {
        let path = if n % 2 == 0 {
            "/api/pulls".to_owned()
        } else {
            format!("/api/projects/{project}/pulls/12")
        };
        let service = &f.service;
        async move { service.get(&path).send().await.unwrap().status().as_u16() }
    });
    for status in futures_util::future::join_all(reads).await {
        assert_eq!(status, 200);
    }
    assert_eq!(f.asked("pr list"), 1);
    assert_eq!(f.asked("pr view"), 1);
}

#[tokio::test]
async fn an_answer_is_never_served_for_another_project() {
    let f = fixture().await;
    let other = f.other_project().await;
    let mine = f
        .timed(&format!("/api/projects/{}/pulls/12", f.project))
        .await
        .1;
    let theirs = f.timed(&format!("/api/projects/{other}/pulls/12")).await.1;
    assert!(
        mine["pull"]["title"]
            .as_str()
            .unwrap()
            .starts_with("project ")
    );
    assert!(
        theirs["pull"]["title"]
            .as_str()
            .unwrap()
            .starts_with("other ")
    );
    let again = f
        .timed(&format!("/api/projects/{}/pulls/12", f.project))
        .await
        .1;
    assert_eq!(again["pull"]["title"], mine["pull"]["title"]);
    // A different pull request number is a different key too.
    assert_eq!(f.asked("pr view 12"), 2);
    // The list across projects holds each project's own.
    let all = f.timed("/api/pulls").await.1;
    let titles: Vec<&str> = all["pulls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["pull"]["title"].as_str().unwrap())
        .collect();
    assert!(
        titles.iter().any(|title| title.starts_with("project ")),
        "{titles:?}"
    );
    assert!(
        titles.iter().any(|title| title.starts_with("other ")),
        "{titles:?}"
    );
}

#[tokio::test]
async fn refresh_reads_again_and_the_windows_hear_what_changed() {
    let f = fixture().await;
    let list = "/api/pulls";
    assert!(
        f.timed(list).await.1["pulls"][0]["pull"]["title"]
            .as_str()
            .unwrap()
            .ends_with(' ')
    );
    let mut stream = f.service.get("/api/shell/events").send().await.unwrap();
    f.set("gh-title", "renamed");
    // Kept: the forge is not asked, and the old title still answers.
    assert!(
        f.timed(list).await.1["pulls"][0]["pull"]["title"]
            .as_str()
            .unwrap()
            .ends_with(' ')
    );
    assert_eq!(f.asked("pr list"), 1);
    let forced = f.timed(&format!("{list}?refresh=1")).await.1;
    assert!(
        forced["pulls"][0]["pull"]["title"]
            .as_str()
            .unwrap()
            .ends_with("renamed")
    );
    assert_eq!(f.asked("pr list"), 2);
    assert!(
        f.announced(&mut stream, "pulls").await,
        "no data_changed for pulls"
    );
    // What the refresh read is what is kept now.
    assert!(
        f.timed(list).await.1["pulls"][0]["pull"]["title"]
            .as_str()
            .unwrap()
            .ends_with("renamed")
    );
    assert_eq!(f.asked("pr list"), 2);
}

#[tokio::test]
async fn a_merge_drops_what_was_kept_for_that_pull_request() {
    let f = fixture().await;
    let base = format!("/api/projects/{}/pulls/12", f.project);
    assert_eq!(f.timed(&base).await.1["pull"]["state"], "open");
    f.timed("/api/pulls").await;
    let mut stream = f.service.get("/api/shell/events").send().await.unwrap();
    let (code, body) = f
        .service
        .send(&format!("{base}/merge"), json!({ "method": "squash" }))
        .await;
    assert_eq!(code, 202, "{body}");
    // No `refresh`: the write itself dropped the kept detail.
    assert_eq!(f.timed(&base).await.1["pull"]["state"], "merged");
    assert!(
        f.announced(&mut stream, &format!("pull:{}:12", f.project))
            .await
    );
    // The list across projects was dropped as well, not served from before the merge.
    assert_eq!(
        f.timed("/api/pulls").await.1["pulls"][0]["pull"]["state"],
        "merged"
    );
    assert_eq!(f.asked("pr list"), 2);
}

#[tokio::test]
async fn a_failing_forge_keeps_the_last_good_answer_and_reports_a_forced_read_as_before() {
    let f = fixture().await;
    let base = format!("/api/projects/{}/pulls/12", f.project);
    let good = f.timed(&base).await.1;
    f.set("gh-fail", "");
    let response = f
        .service
        .get(&format!("{base}?refresh=1"))
        .send()
        .await
        .unwrap();
    assert!(!response.status().is_success(), "{}", response.status());
    assert_eq!(f.timed(&base).await.1, good);
    // With nothing kept, the error is the read's own.
    let list = format!("/api/projects/{}/pulls", f.project);
    let response = f.service.get(&list).send().await.unwrap();
    assert_eq!(response.status().as_u16(), 409);
    // Nothing was stored for it, so the next call asks again and gets the answer.
    std::fs::remove_file(f.dir.path().join("gh-fail")).unwrap();
    assert_eq!(f.timed(&list).await.1.as_array().unwrap().len(), 2);
}
