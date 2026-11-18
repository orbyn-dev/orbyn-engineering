//! Uncached, commit-bound forge receipts for the Core-owned Crew queue.
//! All CLI calls use the forge process owner; credentials stay with the CLI.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Forge;
use super::process::{call_json, call_pages};
use crate::error::{CoreError, Result};

/// The destination is derived from pinned origin, never CLI cwd/defaults or GH_REPO/GH_HOST.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrewRepository {
    pub forge: Forge,
    pub host: String,
    pub repository: String,
}

impl CrewRepository {
    pub fn from_remote(remote: &str) -> Result<Self> {
        #[cfg(test)]
        if Path::new(remote).is_absolute() && Path::new(remote).is_dir() {
            return Ok(Self {
                forge: Forge::GitHub,
                host: "fixture.invalid".into(),
                repository: "fixture/repo".into(),
            });
        }
        let url = if remote.contains("://") {
            remote.to_owned()
        } else {
            let (host, path) = remote
                .split_once(':')
                .ok_or_else(|| invalid("origin must be an HTTPS or SSH forge URL"))?;
            format!("ssh://{host}/{path}")
        };
        let url = reqwest::Url::parse(&url).map_err(|_| invalid("origin URL"))?;
        if !matches!(url.scheme(), "https" | "ssh")
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || (url.scheme() == "https" && (!url.username().is_empty() || url.port().is_some()))
        {
            return Err(invalid(
                "origin must not contain credentials, query parameters or an unsupported API port",
            ));
        }
        let host = url
            .host_str()
            .filter(|host| !host.is_empty())
            .ok_or_else(|| invalid("origin host"))?
            .to_ascii_lowercase();
        let path = url.path().trim_start_matches('/').trim_end_matches('/');
        let repository = path.strip_suffix(".git").unwrap_or(path).to_owned();
        let parts: Vec<_> = repository.split('/').collect();
        let forge = Forge::of_remote(remote);
        if !matches!(forge, Forge::GitHub | Forge::GitLab)
            || parts.len() < 2
            || (forge == Forge::GitHub && parts.len() != 2)
            || parts.iter().any(|part| {
                part.is_empty()
                    || [".", ".."].contains(part)
                    || !part
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
            })
        {
            return Err(invalid("unsupported origin repository path"));
        }
        Ok(Self {
            forge,
            host,
            repository,
        })
    }

    pub fn selector(&self) -> String {
        if self.forge == Forge::GitHub {
            format!("{}/{}", self.host, self.repository)
        } else {
            format!("https://{}/{}", self.host, self.repository)
        }
    }

    fn api_root(&self) -> String {
        if self.forge == Forge::GitHub {
            format!("repos/{}", self.repository)
        } else {
            format!("projects/{}", operand(&self.repository))
        }
    }

    async fn read(&self, cwd: &Path, path: &str) -> Result<Value> {
        call_json(self.forge, cwd, &["api", path, "--hostname", &self.host]).await
    }

    async fn pages(&self, cwd: &Path, path: &str) -> Result<Vec<Value>> {
        call_pages(
            self.forge,
            cwd,
            &["api", path, "--hostname", &self.host, "--paginate"],
        )
        .await
    }
}

/// What one check reported at the head. Skipped and neutral are not failures, but they never satisfy a required check.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CrewCheckState {
    Success,
    /// Skipped or neutral: fine for a check that is not required.
    Neutral,
    /// Not finished, or not run yet.
    Pending,
    /// Failed, cancelled, timed out or any other conclusion.
    Failed,
}

impl CrewCheckState {
    fn github_run(status: &serde_json::Value, conclusion: &serde_json::Value) -> Self {
        if status != "completed" {
            return Self::Pending;
        }
        match conclusion.as_str() {
            Some("success") => Self::Success,
            Some("skipped" | "neutral") => Self::Neutral,
            _ => Self::Failed,
        }
    }

    fn github_status(state: &serde_json::Value) -> Self {
        match state.as_str() {
            Some("success") => Self::Success,
            Some("pending") => Self::Pending,
            _ => Self::Failed,
        }
    }

    fn gitlab(status: &serde_json::Value) -> Self {
        match status.as_str() {
            Some("success") => Self::Success,
            Some("skipped") => Self::Neutral,
            Some(
                "created"
                | "pending"
                | "running"
                | "waiting_for_resource"
                | "preparing"
                | "scheduled"
                | "manual"
                | "waiting_for_callback",
            ) => Self::Pending,
            _ => Self::Failed,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CrewCheckReceipt {
    pub name: String,
    pub head: String,
    pub state: CrewCheckState,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CrewPullReceipt {
    pub host: String,
    pub repository: String,
    pub number: u64,
    pub source: String,
    pub target: String,
    pub head: String,
    pub base: String,
    pub state: String,
    pub merge_commit: Option<String>,
    pub checks: Vec<CrewCheckReceipt>,
}

fn invalid(why: &str) -> CoreError {
    CoreError::Invalid(format!("Crew forge receipt: {why}"))
}

pub fn is_commit(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid(key))
}

fn commit(value: &Value, key: &str) -> Result<String> {
    let sha = string(value, key)?;
    if !is_commit(sha) {
        return Err(invalid("missing full commit identity"));
    }
    Ok(sha.to_owned())
}

fn operand(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// An uncertain create is reconciled by its deterministic source branch, including closed pulls.
/// More than one match is ambiguous and must never select an arbitrary PR.
pub async fn crew_find_pull(
    context: &CrewRepository,
    cwd: &Path,
    source: &str,
    target: &str,
) -> Result<Option<u64>> {
    let forge = context.forge;
    let path = match forge {
        Forge::GitHub => format!("{}/pulls?state=all&per_page=100", context.api_root()),
        Forge::GitLab => format!(
            "{}/merge_requests?scope=all&source_branch={}&per_page=100",
            context.api_root(),
            operand(source)
        ),
        _ => {
            return Err(invalid(
                "only GitHub and GitLab support guarded Crew merges",
            ));
        }
    };
    let rows = context.pages(cwd, &path).await?;
    let mut found = Vec::new();
    for row in rows {
        let (head, base, number) = if forge == Forge::GitHub {
            (
                row["head"]["ref"].as_str(),
                row["base"]["ref"].as_str(),
                row["number"].as_u64(),
            )
        } else {
            (
                row["source_branch"].as_str(),
                row["target_branch"].as_str(),
                row["iid"].as_u64(),
            )
        };
        if head == Some(source) && base == Some(target) {
            if forge == Forge::GitHub
                && row["head"]["repo"]["full_name"] != row["base"]["repo"]["full_name"]
            {
                continue;
            }
            if forge == Forge::GitLab && row["source_project_id"] != row["target_project_id"] {
                continue;
            }
            found.push(
                number
                    .filter(|n| *n > 0)
                    .ok_or_else(|| invalid("pull number"))?,
            );
        }
    }
    match found.as_slice() {
        [] => Ok(None),
        [number] => Ok(Some(*number)),
        _ => Err(invalid("multiple pulls have this source and target")),
    }
}

/// Reads the actual target tip, separate from a pull's merge-base field.
pub async fn crew_base(context: &CrewRepository, cwd: &Path, target: &str) -> Result<String> {
    let forge = context.forge;
    let path = match forge {
        Forge::GitHub => format!("{}/commits/{}", context.api_root(), operand(target)),
        Forge::GitLab => format!(
            "{}/repository/branches/{}",
            context.api_root(),
            operand(target)
        ),
        _ => return Err(invalid("unsupported forge")),
    };
    let value = context.read(cwd, &path).await?;
    if forge == Forge::GitHub {
        commit(&value, "sha")
    } else {
        commit(&value["commit"], "id")
    }
}

/// Every receipt names its commit. An error, empty check set or unknown state cannot become green.
pub async fn crew_pull(
    context: &CrewRepository,
    cwd: &Path,
    number: u64,
) -> Result<CrewPullReceipt> {
    let forge = context.forge;
    if number == 0 {
        return Err(invalid("pull number"));
    }
    let path = match forge {
        Forge::GitHub => format!("{}/pulls/{number}", context.api_root()),
        Forge::GitLab => format!("{}/merge_requests/{number}", context.api_root()),
        _ => return Err(invalid("unsupported forge")),
    };
    let raw = context.read(cwd, &path).await?;
    let number_key = if forge == Forge::GitHub {
        "number"
    } else {
        "iid"
    };
    if raw[number_key].as_u64() != Some(number) {
        return Err(invalid("wrong PR identity"));
    }
    if forge == Forge::GitHub {
        let source_repo = string(&raw["head"]["repo"], "full_name")?;
        if source_repo != string(&raw["base"]["repo"], "full_name")?
            || source_repo != context.repository
        {
            return Err(invalid("foreign source or target repository"));
        }
    } else if raw["source_project_id"].as_u64().is_none()
        || raw["source_project_id"] != raw["target_project_id"]
    {
        return Err(invalid("foreign source project"));
    }
    if forge == Forge::GitLab {
        let project = context.read(cwd, &context.api_root()).await?;
        if string(&project, "path_with_namespace")? != context.repository
            || project["id"].as_u64().is_none()
            || project["id"] != raw["target_project_id"]
        {
            return Err(invalid("foreign target project"));
        }
    }
    let (source, target, head, state, merge_commit) = if forge == Forge::GitHub {
        let merged = raw["merged"]
            .as_bool()
            .ok_or_else(|| invalid("merged state"))?;
        (
            string(&raw["head"], "ref")?.to_owned(),
            string(&raw["base"], "ref")?.to_owned(),
            commit(&raw["head"], "sha")?,
            if merged {
                "merged"
            } else {
                string(&raw, "state")?
            }
            .to_owned(),
            if merged {
                Some(commit(&raw, "merge_commit_sha")?)
            } else {
                None
            },
        )
    } else {
        let state = string(&raw, "state")?;
        (
            string(&raw, "source_branch")?.to_owned(),
            string(&raw, "target_branch")?.to_owned(),
            commit(&raw, "sha")?,
            if state == "opened" { "open" } else { state }.to_owned(),
            if state == "merged" {
                Some(commit(&raw, "merge_commit_sha")?)
            } else {
                None
            },
        )
    };
    let base = crew_base(context, cwd, &target).await?;
    let checks = if state == "merged" {
        Vec::new()
    } else if forge == Forge::GitHub {
        let path = format!(
            "{}/commits/{head}/check-runs?per_page=100",
            context.api_root()
        );
        let pages = context.pages(cwd, &path).await?;
        let mut checks = Vec::new();
        for page in pages {
            let rows = page["check_runs"]
                .as_array()
                .ok_or_else(|| invalid("check runs"))?;
            for row in rows {
                checks.push(CrewCheckReceipt {
                    name: string(row, "name")?.to_owned(),
                    head: commit(row, "head_sha")?,
                    state: CrewCheckState::github_run(&row["status"], &row["conclusion"]),
                });
            }
        }
        let path = format!("{}/commits/{head}/status?per_page=100", context.api_root());
        let status = context.read(cwd, &path).await?;
        let status_head = commit(&status, "sha")?;
        let rows = status["statuses"]
            .as_array()
            .ok_or_else(|| invalid("statuses"))?;
        // Refuse truncated legacy status results rather than overlook a failure on another page.
        if status["total_count"].as_u64() != Some(rows.len() as u64) {
            return Err(invalid("incomplete statuses"));
        }
        for row in rows {
            checks.push(CrewCheckReceipt {
                name: string(row, "context")?.to_owned(),
                head: status_head.clone(),
                state: CrewCheckState::github_status(&row["state"]),
            });
        }
        checks
    } else {
        let pipeline = &raw["head_pipeline"];
        let id = pipeline["id"]
            .as_u64()
            .ok_or_else(|| invalid("head pipeline missing"))?;
        let pipeline_head = commit(pipeline, "sha")?;
        let path = format!("{}/pipelines/{id}/jobs?per_page=100", context.api_root());
        let rows = context.pages(cwd, &path).await?;
        let mut checks = vec![CrewCheckReceipt {
            name: "pipeline".into(),
            head: pipeline_head.clone(),
            state: CrewCheckState::gitlab(&pipeline["status"]),
        }];
        for row in rows {
            checks.push(CrewCheckReceipt {
                name: string(&row, "name")?.to_owned(),
                head: pipeline_head.clone(),
                state: CrewCheckState::gitlab(&row["status"]),
            });
        }
        checks
    };
    Ok(CrewPullReceipt {
        host: context.host.clone(),
        repository: context.repository.clone(),
        number,
        source,
        target,
        head,
        base,
        state,
        merge_commit,
        checks,
    })
}

/// Required names come from the person's service configuration, never the agent's arguments.
///
/// Every required check must be present and succeed. Any other check may succeed, be skipped or be neutral, but
/// never fail, be cancelled or time out, or still be pending. A required check that is missing is not green.
pub fn crew_ci_green(
    receipt: &CrewPullReceipt,
    required: &[String],
    head: &str,
    base: &str,
) -> bool {
    receipt.state == "open"
        && receipt.head == head
        && receipt.base == base
        && !required.is_empty()
        && !receipt.checks.is_empty()
        && receipt.checks.iter().all(|c| {
            c.head == head
                && if required.contains(&c.name) {
                    c.state == CrewCheckState::Success
                } else {
                    matches!(c.state, CrewCheckState::Success | CrewCheckState::Neutral)
                }
        })
        && required
            .iter()
            .all(|name| receipt.checks.iter().any(|c| &c.name == name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ci_needs_every_configured_check_at_the_actual_head_and_base() {
        let mut receipt = CrewPullReceipt {
            host: "fixture.invalid".into(),
            repository: "fixture/repo".into(),
            number: 1,
            source: "crew-task".into(),
            target: "main".into(),
            head: "a".repeat(40),
            base: "b".repeat(40),
            state: "open".into(),
            merge_commit: None,
            checks: vec![CrewCheckReceipt {
                name: "rust".into(),
                head: "a".repeat(40),
                state: CrewCheckState::Success,
            }],
        };
        let required = vec!["rust".into()];
        assert!(crew_ci_green(
            &receipt,
            &required,
            &"a".repeat(40),
            &"b".repeat(40)
        ));
        assert!(!crew_ci_green(&receipt, &[], &receipt.head, &receipt.base));
        assert!(!crew_ci_green(
            &receipt,
            &["web".into()],
            &receipt.head,
            &receipt.base
        ));
        assert!(!crew_ci_green(
            &receipt,
            &required,
            &"c".repeat(40),
            &receipt.base
        ));
        assert!(!crew_ci_green(
            &receipt,
            &required,
            &receipt.head,
            &"c".repeat(40)
        ));
        receipt.checks[0].state = CrewCheckState::Failed;
        assert!(!crew_ci_green(
            &receipt,
            &required,
            &receipt.head,
            &receipt.base
        ));
        receipt.checks[0].state = CrewCheckState::Success;
        receipt.checks[0].head = "c".repeat(40);
        assert!(!crew_ci_green(
            &receipt,
            &required,
            &receipt.head,
            &receipt.base
        ));
        receipt.checks.clear();
        assert!(!crew_ci_green(
            &receipt,
            &required,
            &receipt.head,
            &receipt.base
        ));
    }

    fn check(name: &str, state: CrewCheckState) -> CrewCheckReceipt {
        CrewCheckReceipt {
            name: name.into(),
            head: "a".repeat(40),
            state,
        }
    }

    fn green(checks: Vec<CrewCheckReceipt>, required: &[&str]) -> bool {
        let receipt = CrewPullReceipt {
            host: "fixture.invalid".into(),
            repository: "fixture/repo".into(),
            number: 1,
            source: "crew-task".into(),
            target: "main".into(),
            head: "a".repeat(40),
            base: "b".repeat(40),
            state: "open".into(),
            merge_commit: None,
            checks,
        };
        let required: Vec<String> = required.iter().map(|n| (*n).to_owned()).collect();
        crew_ci_green(&receipt, &required, &"a".repeat(40), &"b".repeat(40))
    }

    #[test]
    fn skipped_and_neutral_checks_do_not_block_when_required_ones_pass() {
        use CrewCheckState::*;
        assert!(green(
            vec![
                check("Rust", Success),
                check("Web build", Neutral),
                check("Desktop cache warm", Neutral),
            ],
            &["Rust"]
        ));
    }

    #[test]
    fn a_skipped_or_pending_required_check_or_a_missing_one_is_not_green() {
        use CrewCheckState::*;
        assert!(!green(vec![check("Rust", Neutral)], &["Rust"]));
        assert!(!green(vec![check("Rust", Pending)], &["Rust"]));
        assert!(!green(vec![check("Web build", Success)], &["Rust"]));
    }

    #[test]
    fn another_failed_or_pending_check_is_not_green() {
        use CrewCheckState::*;
        assert!(!green(
            vec![check("Rust", Success), check("Lint", Failed)],
            &["Rust"]
        ));
        assert!(!green(
            vec![check("Rust", Success), check("Lint", Pending)],
            &["Rust"]
        ));
    }

    #[test]
    fn check_states_map_from_github_and_gitlab_words() {
        use CrewCheckState::*;
        use serde_json::json;
        assert_eq!(
            CrewCheckState::github_run(&json!("queued"), &json!(null)),
            Pending
        );
        assert_eq!(
            CrewCheckState::github_run(&json!("completed"), &json!("skipped")),
            Neutral
        );
        assert_eq!(
            CrewCheckState::github_run(&json!("completed"), &json!("neutral")),
            Neutral
        );
        for bad in [
            "failure",
            "cancelled",
            "timed_out",
            "action_required",
            "stale",
        ] {
            assert_eq!(
                CrewCheckState::github_run(&json!("completed"), &json!(bad)),
                Failed
            );
        }
        assert_eq!(CrewCheckState::github_status(&json!("pending")), Pending);
        assert_eq!(CrewCheckState::github_status(&json!("error")), Failed);
        assert_eq!(CrewCheckState::gitlab(&json!("skipped")), Neutral);
        assert_eq!(CrewCheckState::gitlab(&json!("running")), Pending);
        assert_eq!(CrewCheckState::gitlab(&json!("canceled")), Failed);
    }

    #[tokio::test]
    async fn gitlab_pipeline_receipt_pins_head_and_refuses_missing_pipeline() {
        use std::sync::Arc;
        let context =
            CrewRepository::from_remote("https://gitlab.fixture.invalid/fixture/repo.git").unwrap();
        let fixture = Arc::new(|_: Forge, _: &Path, args: &[&str]| -> Result<String> {
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["--hostname", "gitlab.fixture.invalid"])
            );
            let value = match args {
                ["api", path, ..] if path.ends_with("/merge_requests/7") => {
                    serde_json::json!({"iid":7,"source_project_id":1,"target_project_id":1,"source_branch":"crew/task","target_branch":"main","sha":"a".repeat(40),"state":"opened","head_pipeline":{"id":9,"sha":"a".repeat(40),"status":"success"}})
                }
                ["api", path, ..] if *path == "projects/fixture%2Frepo" => {
                    serde_json::json!({"id":1,"path_with_namespace":"fixture/repo"})
                }
                ["api", path, ..] if path.ends_with("/branches/main") => {
                    serde_json::json!({"commit":{"id":"b".repeat(40)}})
                }
                ["api", path, ..] if path.contains("/pipelines/9/jobs") => {
                    serde_json::json!([{"name":"ci","status":"success"}])
                }
                _ => panic!("unexpected fake glab call {args:?}"),
            };
            Ok(value.to_string())
        });
        super::super::process::with_crew_fixture(fixture, async {
            let receipt = crew_pull(&context, Path::new("owned-fixture"), 7)
                .await
                .unwrap();
            assert!(crew_ci_green(
                &receipt,
                &["ci".into()],
                &"a".repeat(40),
                &"b".repeat(40)
            ));
        })
        .await;
        let missing = Arc::new(|_: Forge, _: &Path, args: &[&str]| -> Result<String> {
            if args[1].contains("/merge_requests/") {
                Ok(serde_json::json!({"iid":7,"source_project_id":1,"target_project_id":1,"source_branch":"crew/task","target_branch":"main","sha":"a".repeat(40),"state":"opened"}).to_string())
            } else if args[1] == "projects/fixture%2Frepo" {
                Ok(serde_json::json!({"id":1,"path_with_namespace":"fixture/repo"}).to_string())
            } else {
                Ok(serde_json::json!({"commit":{"id":"b".repeat(40)}}).to_string())
            }
        });
        super::super::process::with_crew_fixture(missing, async {
            assert!(
                crew_pull(&context, Path::new("owned-fixture"), 7)
                    .await
                    .is_err()
            );
        })
        .await;
    }

    #[test]
    fn origin_identity_is_canonical_and_credential_urls_are_refused() {
        for remote in [
            "https://github.com/owner/repo.git",
            "git@github.com:owner/repo.git",
            "ssh://git@github.com/owner/repo.git",
        ] {
            let context = CrewRepository::from_remote(remote).unwrap();
            assert_eq!(context.selector(), "github.com/owner/repo");
            assert_eq!(context.api_root(), "repos/owner/repo");
        }
        for remote in [
            "https://user:secret@github.com/owner/repo.git",
            "https://github.com/owner/repo?token=secret",
            "file:///private/repo",
            "https://github.com/owner/../repo",
            "https://github.com/owner/repo/extra",
        ] {
            assert!(CrewRepository::from_remote(remote).is_err(), "{remote}");
        }
    }

    #[tokio::test]
    async fn matching_foreign_source_and_target_cannot_substitute_for_origin() {
        use std::sync::Arc;
        let context = CrewRepository::from_remote("https://github.com/intended/repo.git").unwrap();
        let fixture = Arc::new(|_: Forge, _: &Path, args: &[&str]| -> Result<String> {
            assert_eq!(
                args,
                [
                    "api",
                    "repos/intended/repo/pulls/7",
                    "--hostname",
                    "github.com"
                ]
            );
            Ok(serde_json::json!({"number":7,"head":{"repo":{"full_name":"foreign/repo"}},"base":{"repo":{"full_name":"foreign/repo"}}}).to_string())
        });
        super::super::process::with_crew_fixture(fixture, async {
            let error = crew_pull(&context, Path::new("owned-fixture"), 7)
                .await
                .unwrap_err();
            assert!(error.public_message().contains("foreign"));
        })
        .await;
    }
}
