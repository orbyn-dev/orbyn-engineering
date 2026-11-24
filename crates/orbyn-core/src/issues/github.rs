//! GitHub issues through the person's own `gh` (docs/issues.md). Orbyn never reads gh's token. The JSON
//! `gh` prints is the tracker's text and is read defensively: a field that is missing or the wrong type
//! becomes empty, never an error that hides the other issues.

use super::query::{AssigneeFilter, IssueQuery, StateFilter};
use super::{DependencyFeed, DependencyUpdate, IssueSource, ScopeList};
use crate::error::{CoreError, Result};
use crate::source_control::forge::{Forge, call};
use async_trait::async_trait;
use orbyn_contracts::{
    IssueActor, IssueCloseReason, IssueComment, IssueCounts, IssueDetail, IssueLabel,
    IssueLinkedPull, IssueState, IssueStateCategory, IssueSummary, LinkedPullState, TrackerKind,
};
use serde_json::Value;
use std::path::PathBuf;

const LIST_FIELDS: &str = "number,title,url,author,state,stateReason,createdAt,updatedAt,closedAt,assignees,labels,comments";
/// Open Dependabot alerts per `gh api` page, and the pages one read takes (1000 alerts).
const DEPENDENCY_PAGE_SIZE: usize = 100;
const DEPENDENCY_PAGES: u32 = 10;
const DETAIL_FIELDS: &str = "number,title,url,author,state,stateReason,createdAt,updatedAt,closedAt,assignees,labels,comments,body";
const MAX_COMMENTS: usize = 50;

pub(crate) struct GitHub {
    /// Where `gh` runs. With `--repo` it needs no repository, so any folder will do.
    pub(crate) cwd: PathBuf,
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn actor(value: &Value) -> Option<IssueActor> {
    value
        .get("login")
        .and_then(Value::as_str)
        .filter(|l| !l.is_empty())
        .map(|login| IssueActor {
            login: login.to_owned(),
        })
}

fn state(value: &Value) -> IssueState {
    let closed = text(value, "state").eq_ignore_ascii_case("closed");
    let close_reason = closed
        .then(
            || match text(value, "stateReason").to_ascii_uppercase().as_str() {
                "NOT_PLANNED" => Some(IssueCloseReason::NotPlanned),
                "COMPLETED" => Some(IssueCloseReason::Completed),
                _ => None,
            },
        )
        .flatten();
    let name = match (closed, close_reason) {
        (false, _) => "Open",
        (true, Some(IssueCloseReason::NotPlanned)) => "Closed as not planned",
        (true, _) => "Closed",
    };
    IssueState {
        category: if closed {
            IssueStateCategory::Closed
        } else {
            IssueStateCategory::Open
        },
        name: name.to_owned(),
        close_reason,
    }
}

pub(crate) fn summary(scope: &str, value: &Value) -> Option<IssueSummary> {
    let number = value.get("number").and_then(Value::as_u64)?;
    Some(IssueSummary {
        tracker: TrackerKind::Github,
        scope: scope.to_owned(),
        key: number.to_string(),
        display_key: format!("#{number}"),
        title: text(value, "title"),
        url: text(value, "url"),
        author: value.get("author").and_then(actor),
        state: state(value),
        created_at: text(value, "createdAt"),
        updated_at: text(value, "updatedAt"),
        closed_at: Some(text(value, "closedAt")).filter(|t| !t.is_empty()),
        assignees: value
            .get("assignees")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(actor).collect())
            .unwrap_or_default(),
        labels: value
            .get("labels")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|l| l.get("name").and_then(Value::as_str))
                    .map(|name| IssueLabel {
                        name: name.to_owned(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        comment_count: value
            .get("comments")
            .and_then(Value::as_array)
            .map_or(0, |c| u32::try_from(c.len()).unwrap_or(u32::MAX)),
    })
}

/// The ecosystem as a crew stores it: GitHub names Rust `rust`, a crew (and the Dependabot config) names it `cargo`.
/// Every other name is GitHub's own, lowercased.
fn ecosystem_name(name: &str) -> String {
    let name = name.to_ascii_lowercase();
    if name == "rust" { "cargo".into() } else { name }
}

/// The open Dependabot alerts in `gh api`'s answer: one update per alert, keyed by the alert's number. Only the
/// number and the package ecosystem are read; no package name, advisory text or title is kept, so no alert text
/// can reach a prompt. A missing or wrong-typed field is skipped or none.
pub(crate) fn dependabot_updates(value: &Value) -> Vec<DependencyUpdate> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|alert| {
            let number = alert.get("number").and_then(Value::as_u64)?;
            let ecosystem = alert
                .get("dependency")
                .and_then(|dependency| dependency.get("package"))
                .and_then(|package| package.get("ecosystem"))
                .and_then(Value::as_str)
                .map(ecosystem_name);
            Some(DependencyUpdate {
                key: number.to_string(),
                ecosystem,
            })
        })
        .collect()
}

fn comments(value: &Value) -> Vec<IssueComment> {
    value
        .get("comments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(MAX_COMMENTS)
        .map(|c| IssueComment {
            id: text(c, "id"),
            author: c.get("author").and_then(actor),
            body: text(c, "body"),
            created_at: text(c, "createdAt"),
        })
        .collect()
}

fn linked_pulls(value: &Value) -> Vec<IssueLinkedPull> {
    value
        .get("closedByPullRequestsReferences")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let number = u32::try_from(p.get("number").and_then(Value::as_u64)?).ok()?;
            let repo = p.get("repository");
            let repository = repo
                .and_then(|r| {
                    r.get("nameWithOwner")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .or_else(|| {
                            let owner = r
                                .get("owner")
                                .and_then(|o| o.get("login"))
                                .and_then(Value::as_str)?;
                            Some(format!(
                                "{owner}/{}",
                                r.get("name").and_then(Value::as_str)?
                            ))
                        })
                })
                .unwrap_or_default();
            let state = match text(p, "state").to_ascii_uppercase().as_str() {
                "MERGED" => LinkedPullState::Merged,
                "CLOSED" => LinkedPullState::Closed,
                _ => LinkedPullState::Open,
            };
            Some(IssueLinkedPull {
                repository,
                number,
                title: text(p, "title"),
                url: text(p, "url"),
                state,
                is_draft: p.get("isDraft").and_then(Value::as_bool).unwrap_or(false),
                closes_issue: true,
            })
        })
        .collect()
}

/// A failure of `gh`, in the service's own words. Raw output is never passed on.
pub(crate) fn classify(error: CoreError) -> CoreError {
    let message = match &error {
        CoreError::Unsupported(_) => {
            return CoreError::TrackerNotConnected(
                "Install gh and sign in to it to read GitHub issues.".into(),
            );
        }
        CoreError::Git(message) => message.to_lowercase(),
        _ => return CoreError::TrackerUnavailable("GitHub did not answer.".into()),
    };
    let any = |phrases: &[&str]| phrases.iter().any(|p| message.contains(p));
    if any(&[
        "gh auth login",
        "not logged in",
        "authentication required",
        "bad credentials",
        "http 401",
    ]) {
        CoreError::TrackerNotConnected("Sign in to gh to read GitHub issues.".into())
    } else if any(&["rate limit", "http 429", "secondary rate"]) {
        CoreError::TrackerRateLimited("GitHub asked us to slow down. Try again in a minute.".into())
    } else if any(&["could not resolve to an issue", "http 404", "not found"]) {
        CoreError::NotFound("issue".into())
    } else if any(&[
        "http 403",
        "resource not accessible",
        "forbidden",
        "must have admin",
    ]) {
        CoreError::TrackerForbidden("GitHub refused access to this repository.".into())
    } else {
        CoreError::TrackerUnavailable("GitHub did not answer.".into())
    }
}

/// The arguments of `gh issue list` for one scope. No shell reads them.
pub(crate) fn list_args(scope: &str, query: &IssueQuery) -> Vec<String> {
    let mut args: Vec<String> = [
        "issue",
        "list",
        "--repo",
        scope,
        "--json",
        LIST_FIELDS,
        "--limit",
    ]
    .map(str::to_owned)
    .into();
    args.push(query.limit.to_string());
    args.push("--state".into());
    args.push(
        match query.state {
            StateFilter::Open => "open",
            StateFilter::Closed => "closed",
            StateFilter::All => "all",
        }
        .into(),
    );
    for label in &query.labels {
        args.push("--label".into());
        args.push(label.clone());
    }
    let mut search = query.text.clone();
    match &query.assignee {
        AssigneeFilter::Any => {}
        AssigneeFilter::Me => args.extend(["--assignee".to_owned(), "@me".to_owned()]),
        AssigneeFilter::Login(login) => args.extend(["--assignee".to_owned(), login.clone()]),
        AssigneeFilter::NoOne => {
            search = format!("{search} no:assignee").trim().to_owned();
        }
    }
    if !search.is_empty() {
        args.push("--search".into());
        args.push(search);
    }
    args
}

async fn run(cwd: &std::path::Path, args: &[String]) -> Result<Value> {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = call(Forge::GitHub, cwd, &refs).await.map_err(classify)?;
    serde_json::from_str(&out)
        .map_err(|_| CoreError::TrackerUnavailable("GitHub answered something unreadable.".into()))
}

fn scope_ok(scope: &str) -> Result<()> {
    let ok = scope.split('/').count() == 2
        && scope.split('/').all(|p| {
            !p.is_empty()
                && !p.starts_with(['-', '.'])
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        });
    if ok {
        Ok(())
    } else {
        Err(CoreError::Invalid("A GitHub scope is owner/repo.".into()))
    }
}

#[async_trait]
impl IssueSource for GitHub {
    async fn list(&self, scope: &str, query: &IssueQuery) -> Result<ScopeList> {
        scope_ok(scope)?;
        if query.cursors.contains_key(&format!("github {scope}")) {
            // GitHub's answer has one page and issues no cursor, so any that arrives was not ours.
            return Err(CoreError::Invalid(
                "That page cursor was not issued for this repository.".into(),
            ));
        }
        let listed = run(&self.cwd, &list_args(scope, query)).await?;
        let issues = listed
            .as_array()
            .map(|a| a.iter().filter_map(|v| summary(scope, v)).collect())
            .unwrap_or_default();
        let count = |state: &str| {
            let args: Vec<String> = [
                "api",
                "-X",
                "GET",
                "search/issues",
                "-f",
                &format!("q=repo:{scope} is:issue is:{state}"),
                "--jq",
                ".total_count",
            ]
            .map(str::to_owned)
            .into();
            let cwd = self.cwd.clone();
            async move {
                let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                let out = call(Forge::GitHub, &cwd, &refs).await.ok()?;
                out.trim().parse::<u32>().ok()
            }
        };
        let (open, closed) = tokio::join!(count("open"), count("closed"));
        Ok(ScopeList {
            issues,
            counts: open
                .zip(closed)
                .map(|(open, closed)| IssueCounts { open, closed }),
            next: None,
        })
    }

    async fn dependency_updates(&self, scope: &str) -> Result<DependencyFeed> {
        scope_ok(scope)?;
        // Every page of open alerts through the person's own gh. Any failure (no access, alerts disabled, signed
        // out, gh missing) means the tracker cannot answer, never a refusal, so no event is spent. A feed that still
        // has a full last page after DEPENDENCY_PAGES pages is incomplete, and an incomplete feed cannot be a baseline:
        // it answers "cannot" too.
        let mut updates = Vec::new();
        for page in 1..=DEPENDENCY_PAGES {
            let args: Vec<String> = [
                "api",
                "-X",
                "GET",
                &format!("repos/{scope}/dependabot/alerts"),
                "-f",
                "state=open",
                "-f",
                &format!("per_page={DEPENDENCY_PAGE_SIZE}"),
                "-f",
                &format!("page={page}"),
            ]
            .map(str::to_owned)
            .into();
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            let Ok(out) = call(Forge::GitHub, &self.cwd, &refs).await else {
                return Ok(DependencyFeed::Cannot);
            };
            let Ok(value) = serde_json::from_str::<Value>(&out) else {
                return Ok(DependencyFeed::Cannot);
            };
            let Some(alerts) = value.as_array() else {
                return Ok(DependencyFeed::Cannot);
            };
            updates.extend(dependabot_updates(&value));
            if alerts.len() < DEPENDENCY_PAGE_SIZE {
                return Ok(DependencyFeed::Updates(updates));
            }
        }
        Ok(DependencyFeed::Cannot)
    }

    async fn get(&self, scope: &str, key: &str) -> Result<IssueDetail> {
        scope_ok(scope)?;
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_digit()) || key.len() > 9 {
            return Err(CoreError::Invalid("A GitHub issue key is a number.".into()));
        }
        let args: Vec<String> = [
            "issue",
            "view",
            key,
            "--repo",
            scope,
            "--json",
            DETAIL_FIELDS,
        ]
        .map(str::to_owned)
        .into();
        let value = run(&self.cwd, &args).await?;
        let summary = summary(scope, &value).ok_or_else(|| CoreError::NotFound("issue".into()))?;
        // Linked pull requests are a separate field that older `gh` lacks; without it there are none.
        let links: Vec<String> = [
            "issue",
            "view",
            key,
            "--repo",
            scope,
            "--json",
            "closedByPullRequestsReferences",
        ]
        .map(str::to_owned)
        .into();
        let linked = run(&self.cwd, &links)
            .await
            .map(|v| linked_pulls(&v))
            .unwrap_or_default();
        Ok(IssueDetail {
            summary,
            body: text(&value, "body"),
            comments: comments(&value),
            linked_pull_requests: linked,
            linked_threads: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_gh_issue_becomes_a_summary_with_its_close_reason() {
        let open = summary("o/r", &json!({ "number": 7, "title": "T", "url": "u", "author": { "login": "ann" }, "state": "OPEN",
            "createdAt": "c", "updatedAt": "u", "closedAt": "", "assignees": [{ "login": "bo" }], "labels": [{ "name": "bug" }], "comments": [1, 2] })).unwrap();
        assert_eq!(
            (
                open.key.as_str(),
                open.display_key.as_str(),
                open.comment_count
            ),
            ("7", "#7", 2)
        );
        assert_eq!(open.state.category, IssueStateCategory::Open);
        assert_eq!(open.closed_at, None);
        assert_eq!(open.labels[0].name, "bug");
        let closed = summary(
            "o/r",
            &json!({ "number": 8, "state": "CLOSED", "stateReason": "NOT_PLANNED" }),
        )
        .unwrap();
        assert_eq!(
            closed.state.close_reason,
            Some(IssueCloseReason::NotPlanned)
        );
        assert_eq!(closed.state.name, "Closed as not planned");
        let done = summary(
            "o/r",
            &json!({ "number": 9, "state": "CLOSED", "stateReason": "COMPLETED" }),
        )
        .unwrap();
        assert_eq!(done.state.close_reason, Some(IssueCloseReason::Completed));
        let plain = summary("o/r", &json!({ "number": 10, "state": "CLOSED" })).unwrap();
        assert_eq!(
            plain.state.close_reason, None,
            "a tracker that does not say reports none"
        );
    }

    #[test]
    fn wrong_types_become_empty_and_a_row_without_a_number_is_dropped() {
        assert!(summary("o/r", &json!({ "title": "no number" })).is_none());
        let odd = summary(
            "o/r",
            &json!({ "number": 1, "title": 5, "labels": "x", "assignees": 3, "author": "ann" }),
        )
        .unwrap();
        assert_eq!(
            (
                odd.title.as_str(),
                odd.labels.len(),
                odd.assignees.len(),
                odd.author
            ),
            ("", 0, 0, None)
        );
    }

    #[test]
    fn list_arguments_carry_only_the_four_filters() {
        let q = IssueQuery {
            state: StateFilter::All,
            labels: vec!["bug".into()],
            assignee: AssigneeFilter::Me,
            text: "crash".into(),
            ..IssueQuery::default()
        };
        let args = list_args("o/r", &q);
        assert_eq!(&args[..4], ["issue", "list", "--repo", "o/r"]);
        let at = |flag: &str| {
            args.iter()
                .position(|a| a == flag)
                .map(|i| args[i + 1].as_str())
        };
        assert_eq!(
            (
                at("--state"),
                at("--label"),
                at("--assignee"),
                at("--search")
            ),
            (Some("all"), Some("bug"), Some("@me"), Some("crash"))
        );
        let none = list_args(
            "o/r",
            &IssueQuery {
                assignee: AssigneeFilter::NoOne,
                text: "x".into(),
                ..IssueQuery::default()
            },
        );
        assert!(none.contains(&"x no:assignee".to_owned()));
        // A text that looks like an option is a value of --search, never an option.
        let odd = list_args(
            "o/r",
            &IssueQuery {
                text: "--web".into(),
                ..IssueQuery::default()
            },
        );
        let i = odd.iter().position(|a| a == "--search").unwrap();
        assert_eq!(odd[i + 1], "--web");
    }

    #[test]
    fn linked_pulls_read_state_and_repository() {
        let pulls = linked_pulls(&json!({ "closedByPullRequestsReferences": [
            { "number": 3, "title": "Fix", "url": "u", "state": "MERGED", "isDraft": false, "repository": { "nameWithOwner": "o/r" } },
            { "number": 4, "state": "OPEN", "isDraft": true, "repository": { "owner": { "login": "o" }, "name": "r2" } },
            { "title": "no number" }] }));
        assert_eq!(pulls.len(), 2);
        assert_eq!(
            (pulls[0].state, pulls[0].repository.as_str()),
            (LinkedPullState::Merged, "o/r")
        );
        assert_eq!(
            (pulls[1].is_draft, pulls[1].repository.as_str()),
            (true, "o/r2")
        );
    }

    #[test]
    fn a_gh_failure_is_named_without_its_raw_text() {
        let git = |m: &str| classify(CoreError::Git(m.into()));
        assert!(matches!(
            git("To get started with GitHub CLI, please run: gh auth login"),
            CoreError::TrackerNotConnected(_)
        ));
        assert!(matches!(
            git("HTTP 403: API rate limit exceeded for user"),
            CoreError::TrackerRateLimited(_)
        ));
        assert!(matches!(
            git("GraphQL: Could not resolve to an Issue with the number of 99."),
            CoreError::NotFound(_)
        ));
        assert!(matches!(
            git("HTTP 403: Resource not accessible by integration"),
            CoreError::TrackerForbidden(_)
        ));
        let other = git("secret-token-ghp_abc leaked in this line");
        assert!(matches!(&other, CoreError::TrackerUnavailable(m) if !m.contains("ghp_")));
        assert!(matches!(
            classify(CoreError::Unsupported("Install gh".into())),
            CoreError::TrackerNotConnected(_)
        ));
    }

    #[tokio::test]
    async fn a_page_cursor_is_refused_because_github_issues_none() {
        let github = GitHub {
            cwd: std::env::temp_dir(),
        };
        let query = IssueQuery {
            cursors: [("github o/r".to_owned(), "2.abc".to_owned())].into(),
            ..IssueQuery::default()
        };
        let refused = github.list("o/r", &query).await.map(|_| ());
        assert!(matches!(refused, Err(CoreError::Invalid(_))));
    }

    #[test]
    fn a_scope_and_a_key_are_checked_before_gh_runs() {
        assert!(scope_ok("o/r").is_ok() && scope_ok("a-b/c.d_e").is_ok());
        for bad in [
            "o", "o/r/x", "o/ r", "--repo/x", "o/-r", "o/r;rm", "../x", "",
        ] {
            assert!(scope_ok(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn dependabot_alerts_become_updates_with_only_their_number_and_ecosystem() {
        let alerts = json!([
            {
                "number": 12,
                "dependency": { "package": { "ecosystem": "npm", "name": "left-pad" } },
                "security_advisory": { "summary": "never read" }
            },
            {
                "number": 13,
                "dependency": { "package": { "ecosystem": "Rust", "name": "serde" } }
            },
            { "title": "no number" },
            { "number": 14 }
        ]);
        assert_eq!(
            dependabot_updates(&alerts),
            vec![
                DependencyUpdate {
                    key: "12".into(),
                    ecosystem: Some("npm".into()),
                },
                DependencyUpdate {
                    key: "13".into(),
                    ecosystem: Some("cargo".into()),
                },
                DependencyUpdate {
                    key: "14".into(),
                    ecosystem: None,
                },
            ]
        );
        assert!(
            dependabot_updates(&json!({ "not": "an array" })).is_empty(),
            "a non-array answer reports no updates"
        );
    }
}
