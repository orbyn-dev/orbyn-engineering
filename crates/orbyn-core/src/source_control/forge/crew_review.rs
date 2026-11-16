//! Pinned same-repository advisory comments. No votes, inline comments or CLI defaults.
use super::{
    Forge,
    crew::{CrewRepository, is_commit},
    process::{call_input, call_json, call_pages},
};
use crate::error::{CoreError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewTarget {
    pub github: bool,
    pub host: String,
    pub repository: String,
    pub repository_id: u64,
    pub number: u64,
    pub head: String,
    pub base: String,
}
impl ReviewTarget {
    pub fn resource(&self) -> String {
        format!(
            "https://{}/{}/{}/{}",
            self.host,
            self.repository,
            if self.github {
                "pull"
            } else {
                "-/merge_requests"
            },
            self.number
        )
    }
    fn forge(&self) -> Forge {
        if self.github {
            Forge::GitHub
        } else {
            Forge::GitLab
        }
    }
    fn root(&self) -> String {
        if self.github {
            format!("repos/{}", self.repository)
        } else {
            format!("projects/{}", self.repository_id)
        }
    }
    fn comments(&self) -> String {
        if self.github {
            format!("{}/issues/{}/comments", self.root(), self.number)
        } else {
            format!("{}/merge_requests/{}/notes", self.root(), self.number)
        }
    }
    fn url(&self, id: u64) -> String {
        if self.github {
            format!(
                "https://{}/{}/pull/{}#issuecomment-{id}",
                self.host, self.repository, self.number
            )
        } else {
            format!(
                "https://{}/{}/-/merge_requests/{}#note_{id}",
                self.host, self.repository, self.number
            )
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct CommentReceipt {
    pub id: u64,
    pub author_id: u64,
    pub url: String,
}
pub(crate) enum Reconciled {
    Missing,
    Exact(CommentReceipt),
    Ambiguous,
}
fn bad() -> CoreError {
    CoreError::Invalid("Crew review comment identity or receipt is invalid".into())
}
async fn get(forge: Forge, cwd: &Path, host: &str, path: &str) -> Result<Value> {
    call_json(forge, cwd, &["api", path, "--hostname", host]).await
}
fn id(v: &Value) -> Result<u64> {
    v.as_u64().filter(|n| *n > 0).ok_or_else(bad)
}
fn text(v: &Value) -> Result<&str> {
    v.as_str().filter(|s| !s.is_empty()).ok_or_else(bad)
}
fn encoded(path: &str) -> String {
    path.replace('/', "%2F")
}

/// Reads repository DB identity, same-repository PR snapshot and authenticated author, without CI prerequisites.
pub(crate) async fn snapshot(
    repo: &CrewRepository,
    cwd: &Path,
    number: u64,
) -> Result<(ReviewTarget, u64)> {
    let (target, author, open) = receipt_snapshot(repo, cwd, number).await?;
    if !open {
        return Err(bad());
    }
    Ok((target, author))
}

/// Immutable destination/auth reads also work after close/merge. `open` is a
/// separate eligibility fact: receipt recovery never grants another POST.
pub(crate) async fn receipt_snapshot(
    repo: &CrewRepository,
    cwd: &Path,
    number: u64,
) -> Result<(ReviewTarget, u64, bool)> {
    if number == 0 || !matches!(repo.forge, Forge::GitHub | Forge::GitLab) {
        return Err(bad());
    }
    let github = repo.forge == Forge::GitHub;
    let root = if github {
        format!("repos/{}", repo.repository)
    } else {
        format!("projects/{}", encoded(&repo.repository))
    };
    let repository = get(repo.forge, cwd, &repo.host, &root).await?;
    let repository_id = id(&repository["id"])?;
    if text(
        &repository[if github {
            "full_name"
        } else {
            "path_with_namespace"
        }],
    )? != repo.repository
    {
        return Err(bad());
    }
    let path = if github {
        format!("{root}/pulls/{number}")
    } else {
        format!("projects/{repository_id}/merge_requests/{number}")
    };
    let pull = get(repo.forge, cwd, &repo.host, &path).await?;
    let state = text(&pull["state"])?;
    let open = state == if github { "open" } else { "opened" };
    let closed = state == "closed" || (!github && state == "merged");
    if id(&pull[if github { "number" } else { "iid" }])? != number || (!open && !closed) {
        return Err(bad());
    }
    let (head, base) = if github {
        for side in ["base", "head"] {
            if id(&pull[side]["repo"]["id"])? != repository_id
                || text(&pull[side]["repo"]["full_name"])? != repo.repository
            {
                return Err(bad());
            }
        }
        (text(&pull["head"]["sha"])?, text(&pull["base"]["sha"])?)
    } else {
        if id(&pull["target_project_id"])? != repository_id
            || id(&pull["source_project_id"])? != repository_id
        {
            return Err(bad());
        }
        (text(&pull["sha"])?, text(&pull["diff_refs"]["base_sha"])?)
    };
    if !is_commit(head) || !is_commit(base) {
        return Err(bad());
    }
    let author = get(repo.forge, cwd, &repo.host, "user").await?;
    Ok((
        ReviewTarget {
            github,
            host: repo.host.clone(),
            repository: repo.repository.clone(),
            repository_id,
            number,
            head: head.into(),
            base: base.into(),
        },
        id(&author["id"])?,
        open,
    ))
}
fn receipt(target: &ReviewTarget, row: &Value, author: u64, body: &str) -> Result<CommentReceipt> {
    let comment = id(&row["id"])?;
    let actual_author = id(&row[if target.github { "user" } else { "author" }]["id"])?;
    if actual_author != author || row["body"].as_str() != Some(body) {
        return Err(bad());
    }
    let url = target.url(comment);
    if target.github {
        if row["html_url"].as_str() != Some(url.as_str()) {
            return Err(bad());
        }
        let expected = format!(
            "https://api.{}/repos/{}/issues/{}",
            target.host, target.repository, target.number
        );
        // Enterprise GitHub uses /api/v3, github.com uses api.github.com.
        let enterprise = format!(
            "https://{}/api/v3/repos/{}/issues/{}",
            target.host, target.repository, target.number
        );
        if ![expected.as_str(), enterprise.as_str()]
            .contains(&row["issue_url"].as_str().unwrap_or(""))
        {
            return Err(bad());
        }
    }
    Ok(CommentReceipt {
        id: comment,
        author_id: actual_author,
        url,
    })
}
/// Caller has already committed sending and revalidated admission. This is the only POST, never retried here.
pub(crate) async fn send(target: &ReviewTarget, cwd: &Path, body: &str) -> Result<u64> {
    let payload = json!({"body":body}).to_string();
    let raw = call_input(
        target.forge(),
        cwd,
        &[
            "api",
            &target.comments(),
            "--hostname",
            &target.host,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &payload,
    )
    .await?;
    id(&serde_json::from_str::<Value>(&raw)?["id"])
}
pub(crate) async fn verify(
    target: &ReviewTarget,
    cwd: &Path,
    comment: u64,
    author: u64,
    body: &str,
) -> Result<CommentReceipt> {
    let path = if target.github {
        format!("{}/issues/comments/{comment}", target.root())
    } else {
        format!("{}/{comment}", target.comments())
    };
    let row = get(target.forge(), cwd, &target.host, &path).await?;
    let result = receipt(target, &row, author, body)?;
    if result.id != comment {
        return Err(bad());
    }
    Ok(result)
}
pub(crate) async fn reconcile(
    target: &ReviewTarget,
    cwd: &Path,
    marker: &str,
    author: u64,
    body: &str,
) -> Result<Reconciled> {
    let path = format!("{}?per_page=100", target.comments());
    let rows = call_pages(
        target.forge(),
        cwd,
        &["api", &path, "--hostname", &target.host, "--paginate"],
    )
    .await?;
    let matches: Vec<_> = rows
        .iter()
        .filter(|r| r["body"].as_str().is_some_and(|b| b.contains(marker)))
        .collect();
    match matches.as_slice() {
        [] => Ok(Reconciled::Missing),
        [row] => match receipt(target, row, author, body) {
            Ok(receipt) => Ok(Reconciled::Exact(
                verify(target, cwd, receipt.id, author, body).await?,
            )),
            Err(_) => Ok(Reconciled::Ambiguous),
        },
        _ => Ok(Reconciled::Ambiguous),
    }
}

#[cfg(test)]
#[path = "crew_review_tests.rs"]
mod tests;
