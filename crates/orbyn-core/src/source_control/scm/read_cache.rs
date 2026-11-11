//! Pull request and forge reads answered from the last known value. A forge read costs
//! a process and a round trip, seconds for a detail or a list across projects, and the
//! pull request screens ask for eight of them at once, so each answer is kept by every
//! input of its read and refreshed in the background (see `util::swr`).

use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

use orbyn_contracts::{
    ForgeStatus, Project, PullCandidate, PullCheck, PullConversation, PullFile, PullList,
    PullRequest, PullRequestDetail, PullRevision, PullStack, PullTimelineEvent,
};

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::source_control::forge::{Forge, forge_of};
use crate::util::swr::{Notify, Swr, key};

/// A list of pull requests is served without a refresh for this long.
pub(super) const LIST_FRESH: Duration = Duration::from_secs(20);
/// One pull request's detail, checks, conversation and the rest: checks and comments
/// move quickly, so they are the shortest.
pub(super) const PULL_FRESH: Duration = Duration::from_secs(10);
/// Who can review and which labels exist change rarely.
pub(super) const CANDIDATES_FRESH: Duration = Duration::from_secs(120);
/// Which forge CLIs are installed and signed in.
pub(super) const FORGES_FRESH: Duration = Duration::from_secs(60);
/// A value older than this is read again before it is shown, so a screen opened after
/// a long time never starts from hours-old data.
pub(super) const MAX_AGE: Duration = Duration::from_secs(15 * 60);
const KEYS: usize = 128;

/// What each pull request read keeps. Keys lead with the project id and the pull request
/// number, so a write drops everything of one pull request by that prefix.
pub struct ScmReads {
    pub(super) list: Swr<PullList>,
    pub(super) project_list: Swr<Vec<PullRequest>>,
    pub(super) detail: Swr<PullRequestDetail>,
    pub(super) checks: Swr<Vec<PullCheck>>,
    pub(super) conversation: Swr<PullConversation>,
    pub(super) timeline: Swr<Vec<PullTimelineEvent>>,
    pub(super) files: Swr<Vec<PullFile>>,
    pub(super) revisions: Swr<Vec<PullRevision>>,
    pub(super) stack: Swr<PullStack>,
    pub(super) reviewers: Swr<Vec<PullCandidate>>,
    pub(super) labels: Swr<Vec<PullCandidate>>,
    pub(super) forges: Swr<Vec<ForgeStatus>>,
}

impl Default for ScmReads {
    fn default() -> Self {
        fn pull<V: Clone + PartialEq + Send + Sync + 'static>() -> Swr<V> {
            Swr::new(PULL_FRESH, MAX_AGE, KEYS)
        }
        fn candidates<V: Clone + PartialEq + Send + Sync + 'static>() -> Swr<V> {
            Swr::new(CANDIDATES_FRESH, MAX_AGE, KEYS)
        }
        Self {
            list: Swr::new(LIST_FRESH, MAX_AGE, KEYS),
            project_list: Swr::new(LIST_FRESH, MAX_AGE, KEYS),
            detail: pull(),
            checks: pull(),
            conversation: pull(),
            timeline: pull(),
            files: pull(),
            revisions: pull(),
            stack: pull(),
            reviewers: candidates(),
            labels: candidates(),
            forges: Swr::new(FORGES_FRESH, MAX_AGE, 1),
        }
    }
}

pub(super) fn pull_topic(project_id: &str, number: u64) -> String {
    format!("pull:{project_id}:{number}")
}

impl ScmReads {
    pub fn set_notify(&self, notify: &Notify) {
        self.list.set_notify(notify.clone());
        self.project_list.set_notify(notify.clone());
        self.detail.set_notify(notify.clone());
        self.checks.set_notify(notify.clone());
        self.conversation.set_notify(notify.clone());
        self.timeline.set_notify(notify.clone());
        self.files.set_notify(notify.clone());
        self.revisions.set_notify(notify.clone());
        self.stack.set_notify(notify.clone());
        self.reviewers.set_notify(notify.clone());
        self.labels.set_notify(notify.clone());
        self.forges.set_notify(notify.clone());
    }

    /// A write changed one pull request, and so possibly every list and stack it is in.
    pub(crate) fn pull_changed(&self, project_id: &str, number: u64) {
        let pull = key(&[project_id, &number.to_string()]);
        self.detail.invalidate(&pull);
        self.checks.invalidate(&pull);
        self.conversation.invalidate(&pull);
        self.timeline.invalidate(&pull);
        self.files.invalidate(&pull);
        self.revisions.invalidate(&pull);
        self.project_changed(project_id);
        self.list.announce(&pull_topic(project_id, number));
    }

    /// Pull requests were opened, closed or merged in a project.
    pub(crate) fn project_changed(&self, project_id: &str) {
        let project = key(&[project_id]);
        self.project_list.invalidate(&project);
        self.stack.invalidate(&project);
        self.list.invalidate_all();
        self.list.announce("pulls");
    }

    /// Pull requests changed somewhere, as when a thread opened one.
    pub(crate) fn lists_changed(&self) {
        self.project_list.invalidate_all();
        self.stack.invalidate_all();
        self.list.invalidate_all();
        self.list.announce("pulls");
    }
}

/// The key of a read of one pull request: its project, number and folder.
fn pull_key(project: &Project, number: u64) -> String {
    key(&[&project.id, &number.to_string(), &project.path])
}

/// One pull request read: the forge is found from the folder on a miss only, so an
/// answer that is kept costs no process.
async fn read_pull<V, F, Fut>(
    cache: &Swr<V>,
    project: Project,
    number: u64,
    refresh: bool,
    read: F,
) -> Result<V>
where
    V: Clone + PartialEq + Send + Sync + 'static,
    F: FnOnce(Forge, PathBuf) -> Fut + Send + 'static,
    Fut: Future<Output = Result<V>> + Send + 'static,
{
    let (key, topic) = (pull_key(&project, number), pull_topic(&project.id, number));
    cache
        .get(&key, &topic, refresh, move || async move {
            let cwd = PathBuf::from(&project.path);
            let forge = forge_of(&cwd).await?;
            read(forge, cwd).await
        })
        .await
}

impl Core {
    /// A trusted project, without asking its forge. Reads that may be kept start here.
    pub(super) fn trusted_project(&self, project_id: &str) -> Result<Project> {
        let project = self.store.project(project_id)?;
        if project.trusted {
            Ok(project)
        } else {
            Err(CoreError::FolderUntrusted)
        }
    }

    pub(super) async fn cached_detail(
        &self,
        project_id: &str,
        number: u64,
        refresh: bool,
    ) -> Result<PullRequestDetail> {
        let project = self.trusted_project(project_id)?;
        read_pull(
            &self.scm_reads.detail,
            project,
            number,
            refresh,
            move |forge, cwd| async move {
                crate::source_control::forge::pull_detail(forge, &cwd, number).await
            },
        )
        .await
    }

    pub(super) async fn cached_checks(
        &self,
        project_id: &str,
        number: u64,
        refresh: bool,
    ) -> Result<Vec<PullCheck>> {
        let project = self.trusted_project(project_id)?;
        read_pull(
            &self.scm_reads.checks,
            project,
            number,
            refresh,
            move |forge, cwd| async move {
                crate::source_control::forge::pull_checks(forge, &cwd, number).await
            },
        )
        .await
    }

    pub(super) async fn cached_conversation(
        &self,
        project_id: &str,
        number: u64,
        refresh: bool,
    ) -> Result<PullConversation> {
        let project = self.trusted_project(project_id)?;
        read_pull(
            &self.scm_reads.conversation,
            project,
            number,
            refresh,
            move |forge, cwd| async move {
                crate::source_control::forge::conversation(forge, &cwd, number).await
            },
        )
        .await
    }

    pub(super) async fn cached_timeline(
        &self,
        project_id: &str,
        number: u64,
        refresh: bool,
    ) -> Result<Vec<PullTimelineEvent>> {
        let project = self.trusted_project(project_id)?;
        read_pull(
            &self.scm_reads.timeline,
            project,
            number,
            refresh,
            move |forge, cwd| async move {
                crate::source_control::forge::timeline(forge, &cwd, number).await
            },
        )
        .await
    }

    /// The viewed marks are this service's own, so they are read on every call and
    /// never kept with the forge's answer.
    pub(super) async fn cached_files(
        &self,
        project_id: &str,
        number: u64,
        refresh: bool,
    ) -> Result<Vec<PullFile>> {
        let project = self.trusted_project(project_id)?;
        let mut files = read_pull(
            &self.scm_reads.files,
            project,
            number,
            refresh,
            move |forge, cwd| async move {
                crate::source_control::forge::pull_files(forge, &cwd, number).await
            },
        )
        .await?;
        let viewed = self.store.viewed_files(project_id, number)?;
        for file in &mut files {
            file.viewed = viewed.contains(&file.path);
        }
        Ok(files)
    }

    pub(super) async fn cached_revisions(
        &self,
        project_id: &str,
        number: u64,
        refresh: bool,
    ) -> Result<Vec<PullRevision>> {
        let project = self.trusted_project(project_id)?;
        read_pull(
            &self.scm_reads.revisions,
            project,
            number,
            refresh,
            move |forge, cwd| async move {
                crate::source_control::forge::pull_revisions(forge, &cwd, number).await
            },
        )
        .await
    }

    pub(super) async fn cached_stack(
        &self,
        project_id: &str,
        number: u64,
        refresh: bool,
    ) -> Result<PullStack> {
        let project = self.trusted_project(project_id)?;
        read_pull(
            &self.scm_reads.stack,
            project,
            number,
            refresh,
            move |forge, cwd| async move {
                let filter = crate::source_control::forge::PullFilter {
                    state: "open".into(),
                    ..Default::default()
                };
                let pulls = crate::source_control::forge::list_pulls(forge, &cwd, &filter).await?;
                super::stack::stack_of(&pulls, number)
                    .ok_or_else(|| CoreError::NotFound("pull request".into()))
            },
        )
        .await
    }

    pub(super) async fn cached_candidates(
        &self,
        project_id: &str,
        labels: bool,
        refresh: bool,
    ) -> Result<Vec<PullCandidate>> {
        let project = self.trusted_project(project_id)?;
        let cache = if labels {
            &self.scm_reads.labels
        } else {
            &self.scm_reads.reviewers
        };
        let key = key(&[&project.id, &project.path]);
        let topic = format!("pull-people:{}", project.id);
        cache
            .get(&key, &topic, refresh, move || async move {
                let cwd = PathBuf::from(&project.path);
                let forge = forge_of(&cwd).await?;
                if labels {
                    crate::source_control::forge::label_candidates(forge, &cwd).await
                } else {
                    crate::source_control::forge::reviewer_candidates(forge, &cwd).await
                }
            })
            .await
    }
}
