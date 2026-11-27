//! Forge reads and writes through each forge's own CLI, so a login stays the
//! user's and Orbyn holds no credential. No shell parses an argument. GitHub and
//! GitLab cover everything their APIs offer through `gh api` and `glab api`, Azure
//! DevOps and Forgejo cover the basics their CLIs have, and Bitbucket has no CLI to
//! go through, so it reports that instead of asking for a token.

pub(crate) mod cache;
mod create;
pub(crate) mod crew;
pub(crate) mod crew_review;
mod details;
mod github;
mod gitlab;
mod merge;
mod parse;
mod process;
mod pulls;
mod review;
mod status;

use std::path::Path;

use crate::error::{CoreError, Result};
use crate::source_control::git::{git, text};
pub(crate) use create::create_pull_in;
pub use create::{create_pull, number_from_url, publish};
pub use details::{
    DETAIL_FIELDS, DETAILS_ROW_LIMIT, DetailsOutcome, github_details, github_graphql_reset,
    graphql_reset,
};
pub(crate) use merge::merge_in;
pub use merge::{
    check_head, current_head, head_changed, head_in, head_precheck_needed, merge, pull_action,
    strip_agent_credits, update_pull,
};
pub(crate) use parse::github_checks;
pub use parse::parse_pull;
pub(crate) use process::call_input;
#[cfg(test)]
pub(crate) use process::with_crew_fixture;
#[cfg(test)]
pub(crate) use process::with_crew_input_fixture;
pub use process::{call, no_such_pull};
pub use pulls::{
    PullFilter, approve_workflow, check_log, conversation, fetch_pull, list_pulls, pull_checks,
    pull_detail, pull_files, pull_node_id, pull_revisions, pull_viewed, rerun_failed,
    revision_files, run_id_of, set_file_viewed, timeline,
};
pub use review::{
    add_comment, edit_comment, label_candidates, react, reply_to_thread, reviewer_candidates,
    set_labels, set_reviewers, set_thread_resolved, submit_review,
};
pub use status::{forge_status, viewer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Forge {
    GitHub,
    GitLab,
    Azure,
    Forgejo,
    Bitbucket,
}

pub const ALL_FORGES: [Forge; 5] = [
    Forge::GitHub,
    Forge::GitLab,
    Forge::Azure,
    Forge::Forgejo,
    Forge::Bitbucket,
];

impl Forge {
    /// Reads the host from a remote URL. A word in the path does not count. An
    /// unknown host is GitHub, so GitHub Enterprise works through `gh`.
    pub fn of_remote(url: &str) -> Self {
        let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
        let rest = rest.rsplit_once('@').map_or(rest, |(_, rest)| rest);
        let host = rest.split(['/', ':']).next().unwrap_or("").to_lowercase();
        if host.contains("gitlab") {
            Self::GitLab
        } else if host == "dev.azure.com"
            || host.ends_with(".dev.azure.com")
            || host.ends_with(".visualstudio.com")
        {
            Self::Azure
        } else if host.contains("bitbucket") {
            Self::Bitbucket
        } else if host.contains("codeberg") || host.contains("forgejo") || host.contains("gitea") {
            Self::Forgejo
        } else {
            Self::GitHub
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::GitLab => "gitlab",
            Self::Azure => "azure",
            Self::Forgejo => "forgejo",
            Self::Bitbucket => "bitbucket",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::GitLab => "GitLab",
            Self::Azure => "Azure DevOps",
            Self::Forgejo => "Forgejo",
            Self::Bitbucket => "Bitbucket",
        }
    }

    pub fn program(self) -> Option<&'static str> {
        match self {
            Self::GitHub => Some("gh"),
            Self::GitLab => Some("glab"),
            Self::Azure => Some("az"),
            Self::Forgejo => Some("tea"),
            Self::Bitbucket => None,
        }
    }

    pub(crate) fn unsupported(self, what: &str) -> CoreError {
        CoreError::Unsupported(format!("{} cannot {what} from Orbyn.", self.name()))
    }
}

pub async fn forge_of(cwd: &Path) -> Result<Forge> {
    let url = git(cwd, &["remote", "get-url", "origin"])
        .await
        .map_err(|_| CoreError::Invalid("This project has no origin remote.".into()))?;
    Ok(Forge::of_remote(&text(&url)))
}

#[cfg(test)]
mod revision_tests;

#[cfg(test)]
mod tests;
