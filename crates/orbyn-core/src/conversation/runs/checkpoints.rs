use super::*;

impl RunManager {
    /// The folders a run snapshots, and the repository each one belongs to: every trusted repository's worktree of a
    /// multi-repository thread, or the engine's own checkout for an ordinary one. A task folder is never one, because
    /// it is not a repository: a run in it used to keep no checkpoint at all (docs/multi-repo.md).
    fn checkpoint_roots(
        &self,
        thread: &Thread,
        checkout: &Path,
    ) -> Result<Vec<(Option<String>, PathBuf)>> {
        let worktrees = crate::repos::trusted_worktrees(&self.store, thread)?;
        Ok(if worktrees.is_empty() {
            vec![(None, checkout.to_path_buf())]
        } else {
            worktrees
                .into_iter()
                .map(|(repo_id, worktree)| (Some(repo_id), worktree))
                .collect()
        })
    }

    /// A failed snapshot costs that run its checkpoint, never the run itself.
    pub(super) async fn checkpoint_start(
        &self,
        thread: &Thread,
        scope: &RunScope,
        checkout: &Path,
        run_dir: &Path,
    ) {
        let index = run_dir.join("checkpoint-index");
        let roots = match self.checkpoint_roots(thread, checkout) {
            Ok(roots) => roots,
            Err(error) => {
                tracing::warn!(%error, "could not list the folders to snapshot");
                return;
            }
        };
        for (position, (repo_id, root)) in roots.into_iter().enumerate() {
            // Each repository's ref carries its own name: two worktrees of one repository share a Git common
            // directory, so one ref per point would keep only the last snapshot reachable (docs/architecture.md).
            let reference =
                checkpoint_ref(&scope.thread_id, &scope.run_id, repo_id.as_deref(), "start");
            let sha = match snapshot(&root, Some(&reference), &index).await {
                Ok(sha) => sha,
                Err(error) => {
                    tracing::warn!(%error, "could not take a checkpoint");
                    continue;
                }
            };
            let recorded = match &repo_id {
                Some(repo_id) => self
                    .store
                    .add_repository_checkpoint(&scope.thread_id, &scope.run_id, repo_id, &sha)
                    // One run is one row for the turn picker, so the first repository's ends are the run's too.
                    .and_then(|()| match position {
                        0 => self
                            .store
                            .add_checkpoint(&scope.thread_id, &scope.run_id, &sha),
                        _ => Ok(()),
                    }),
                None => self
                    .store
                    .add_checkpoint(&scope.thread_id, &scope.run_id, &sha),
            };
            if let Err(error) = recorded {
                tracing::warn!(%error, "could not record a checkpoint");
            }
        }
    }

    pub(super) async fn checkpoint_end(&self, scope: &RunScope, thread: &Thread, run_dir: &Path) {
        if !matches!(self.store.run_has_checkpoint(&scope.run_id), Ok(true)) {
            return;
        }
        let index = run_dir.join("checkpoint-index");
        let roots = match self.checkpoint_roots(thread, Path::new(&thread.cwd)) {
            Ok(roots) => roots,
            Err(error) => {
                tracing::warn!(%error, "could not list the folders to snapshot");
                return;
            }
        };
        // The run's own start/end pair belongs to the repository that owns the run: the first one it was
        // snapshotted in, whose start the run-level row keeps. The pairing is by repository and never by the start
        // commit, because two repositories can share one. A repository that took a start but is not the run-level one
        // keeps its own pair only, and a repository trusted after the run began has no start row and never does
        // (docs/multi-repo.md).
        let owner = self
            .store
            .run_checkpoint_owner(&scope.thread_id, &scope.run_id)
            .ok()
            .flatten();
        for (repo_id, root) in roots {
            // A repository that took no start keeps no end either: the snapshot would only be thrown away, and its
            // ref would name a checkpoint the run never had. The start is read first, so nothing is snapshotted for it.
            let has_start = match &repo_id {
                Some(repo_id) => {
                    match self
                        .store
                        .repository_checkpoint(&scope.thread_id, &scope.run_id, repo_id)
                    {
                        Ok(found) => found.is_some(),
                        Err(error) => {
                            tracing::warn!(%error, "could not read a checkpoint");
                            continue;
                        }
                    }
                }
                None => true,
            };
            if !has_start {
                continue;
            }
            // Each repository's ref carries its own name: two worktrees of one repository share a Git common
            // directory, so one ref per point would keep only the last snapshot reachable (docs/architecture.md).
            let reference =
                checkpoint_ref(&scope.thread_id, &scope.run_id, repo_id.as_deref(), "end");
            let sha = match snapshot(&root, Some(&reference), &index).await {
                Ok(sha) => sha,
                Err(error) => {
                    tracing::warn!(%error, "could not take a checkpoint");
                    continue;
                }
            };
            let recorded = match &repo_id {
                Some(repo_id) => {
                    let owns_run = owner.as_deref() == Some(repo_id.as_str());
                    self.store
                        .finish_repository_checkpoint(&scope.run_id, repo_id, &sha)
                        .and_then(|()| {
                            if owns_run {
                                self.store.finish_checkpoint(&scope.run_id, &sha)
                            } else {
                                Ok(())
                            }
                        })
                }
                None => self.store.finish_checkpoint(&scope.run_id, &sha),
            };
            if let Err(error) = recorded {
                tracing::warn!(%error, "could not record a checkpoint");
            }
        }
    }

    /// Writes the terminal state and hands expired approvals back to their waiters.
    pub(super) fn close_run(
        &self,
        run_id: &str,
        state: RunState,
        error: Option<&str>,
        failure_note: Option<&Value>,
    ) -> Option<Run> {
        // An agent may have committed, switched branch or added a worktree during the run.
        self.workspace.invalidate_status();
        match self
            .store
            .finish_run_with_sign_in(run_id, state, error, failure_note)
        {
            Ok((run, expired)) => {
                self.gate.settle(&expired);
                // A run may have pushed to a branch that has a pull request.
                if let Ok(thread) = self.store.thread(&run.thread_id) {
                    self.scm_reads.project_all_changed(&thread.project_id);
                    // And the run's files: change letters and the dirty summary moved.
                    self.explorer.changed(&thread.project_id);
                }
                Some(run)
            }
            Err(CoreError::RunClosed) => None,
            Err(failure) => {
                tracing::error!(%failure, %run_id, "could not close a run");
                None
            }
        }
    }

    pub(super) async fn drive(
        &self,
        thread: &Thread,
        scope: &RunScope,
        message: (String, Vec<PathBuf>),
        run_dir: &Path,
        token: &mut Option<String>,
        stops: &Stops,
    ) -> EngineOutcome {
        let first = self
            .attempt(thread, scope, message.clone(), run_dir, token, stops, false)
            .await;
        if first != EngineOutcome::SessionLost {
            return first;
        }
        // The engine's session is gone, so a fresh one gets the budgeted history
        // from the log instead, once.
        match self.store.run_owns_engine(&scope.run_id) {
            Ok(true) => {}
            Ok(false) => return EngineOutcome::SessionLost,
            Err(error) => return EngineOutcome::Failed(error.public_message()),
        }
        if let Err(error) = self.recover_session(thread, &scope.run_id) {
            return EngineOutcome::Failed(error.public_message());
        }
        self.attempt(thread, scope, message, run_dir, token, stops, true)
            .await
    }

    fn recover_session(&self, thread: &Thread, run: &str) -> Result<()> {
        let handoff = self.store.recovery_handoff(&thread.id)?;
        self.store.set_run_handoff(run, &handoff)?;
        self.store
            .note_thread_event(&thread.id, "engine.session_lost", json!({}))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use orbyn_contracts::{AddRepoFolder, PermissionMode, WorkspaceSetup};

    use super::*;
    use crate::app::{Core, CoreConfig};
    use crate::governance::approvals::RunScope;
    use crate::store::thread_repos::ThreadRepoRecord;
    use crate::store::{NewProject, NewThread};
    use crate::testing::{TempDir, library, run_git};

    /// A repository with one commit on `main`, so HEAD exists.
    fn repo(path: &Path) -> PathBuf {
        std::fs::create_dir_all(path).unwrap();
        run_git(path, &["init", "-q", "-b", "main"]);
        run_git(path, &["config", "user.email", "t@example.com"]);
        run_git(path, &["config", "user.name", "T"]);
        std::fs::write(path.join("base.txt"), "base\n").unwrap();
        run_git(path, &["add", "."]);
        run_git(path, &["commit", "-q", "-m", "first"]);
        crate::util::canonicalize(path).unwrap()
    }

    fn text_of(path: &Path) -> String {
        crate::util::strip_verbatim(&path.to_string_lossy())
    }

    fn head(worktree: &Path) -> String {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(worktree)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    /// A thread over two repositories that share every commit: the second is a clone of the first, so both
    /// worktrees start at the same commit and the same start SHA. What a start over both makes, without an engine.
    struct SharedHistory {
        dir: TempDir,
        core: Arc<Core>,
        thread: Thread,
        primary: String,
        other: String,
        primary_worktree: PathBuf,
        other_worktree: PathBuf,
    }

    async fn shared_history() -> SharedHistory {
        let dir = TempDir::new();
        let first = repo(&dir.path().join("first"));
        let second = dir.path().join("second");
        run_git(
            dir.path(),
            &["clone", "-q", &text_of(&first), &text_of(&second)],
        );
        let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
        config.engines.lookup_latest = false;
        config.multi_repo = true;
        let core = Arc::new(Core::open(config).unwrap());
        let root = library(&dir.path().join("library"));
        core.workspace
            .setup(WorkspaceSetup {
                url: None,
                adopt_path: Some(text_of(&root)),
            })
            .await
            .unwrap();
        let project = core
            .store
            .add_project(NewProject {
                path: &text_of(&first),
                name: "first",
                is_git: true,
                managed: false,
                scratch: false,
            })
            .unwrap();
        core.store.trust_project(&project.id, "tester").unwrap();
        let other = core
            .add_repo_folder(
                &project.id,
                AddRepoFolder {
                    path: text_of(&second),
                },
            )
            .await
            .unwrap();
        core.trust_repo(&project.id, &other.id).await.unwrap();
        // A task folder outside both repositories, with one worktree of each, on one branch.
        let folder = core.config.data_dir.join("threads").join("task");
        std::fs::create_dir_all(&folder).unwrap();
        let mut worktrees = Vec::new();
        for (repo_id, name, source) in [
            (&project.id, "first", &first),
            (&other.id, "second", &second),
        ] {
            let worktree = folder.join(name);
            run_git(
                source,
                &[
                    "worktree",
                    "add",
                    "-q",
                    "-b",
                    "orbyn/thread",
                    "--",
                    &text_of(&worktree),
                ],
            );
            worktrees.push((repo_id.clone(), name.to_owned(), worktree));
        }
        let thread = core
            .store
            .create_thread(NewThread {
                project_id: project.id.clone(),
                title: "t".into(),
                engine: "codex".into(),
                model: None,
                cwd: text_of(&folder),
                worktree: None,
                skill_profile: None,
                parent_thread_id: None,
                relation: None,
                scratch: false,
            })
            .unwrap();
        for (position, (repo_id, name, worktree)) in worktrees.iter().enumerate() {
            core.store
                .thread_repo_insert(
                    &ThreadRepoRecord {
                        thread_id: thread.id.clone(),
                        repo_id: repo_id.clone(),
                        position: position as u32,
                        name: name.clone(),
                        branch: "orbyn/thread".into(),
                        // The fixture writes the row itself, with no base: a thread stored before the column.
                        base_branch: None,
                        worktree_path: text_of(worktree),
                    },
                    &crate::util::now(),
                )
                .unwrap();
        }
        SharedHistory {
            primary_worktree: worktrees[0].2.clone(),
            other_worktree: worktrees[1].2.clone(),
            dir,
            core,
            thread,
            primary: project.id,
            other: other.id,
        }
    }

    /// Two repositories can share a start commit: one is a clone of the other, or both were snapshotted from the
    /// same bytes in the same second. The run's own end still belongs to the repository that owns the run, which is
    /// the one the run was first snapshotted in; pairing by the start SHA would let the other one fill it instead.
    #[tokio::test]
    async fn only_the_repository_that_owns_the_run_fills_its_end_when_two_share_a_start() {
        let f = shared_history().await;
        let run = "run-shared-start";
        let start = head(&f.primary_worktree);
        assert_eq!(
            head(&f.other_worktree),
            start,
            "the two repositories start at the same commit"
        );
        // Both repositories keep that same commit as the run's start, as a snapshot of each would.
        f.core
            .store
            .add_repository_checkpoint(&f.thread.id, run, &f.primary, &start)
            .unwrap();
        f.core
            .store
            .add_repository_checkpoint(&f.thread.id, run, &f.other, &start)
            .unwrap();
        f.core
            .store
            .add_checkpoint(&f.thread.id, run, &start)
            .unwrap();
        // The run wrote in the first repository only, so the two repositories end in different commits.
        std::fs::write(f.primary_worktree.join("written.txt"), "written\n").unwrap();
        let scope = RunScope {
            run_id: run.into(),
            thread_id: f.thread.id.clone(),
            engine: "codex".into(),
            family: "codex".into(),
            write_scope: None,
            mode: PermissionMode::Supervised,
            plan: false,
        };
        let run_dir = f.dir.path().join("run");
        std::fs::create_dir_all(&run_dir).unwrap();
        let thread = f.core.store.thread(&f.thread.id).unwrap();
        f.core.runs.checkpoint_end(&scope, &thread, &run_dir).await;

        let owned = f
            .core
            .store
            .checkpoint(run)
            .unwrap()
            .expect("the run keeps its own row");
        let primary = f
            .core
            .store
            .repository_checkpoint(&f.thread.id, run, &f.primary)
            .unwrap()
            .unwrap();
        let other = f
            .core
            .store
            .repository_checkpoint(&f.thread.id, run, &f.other)
            .unwrap()
            .unwrap();
        assert!(
            primary.end.is_some() && other.end.is_some(),
            "each repository kept its own end"
        );
        assert_ne!(
            primary.end, other.end,
            "the run ended in a different commit in each repository"
        );
        assert_eq!(
            owned.end, primary.end,
            "the run's own end belongs to the repository that owns the run, not to one that only shares its start"
        );
    }

    /// A repository's checkpoint refs are its own. Two linked worktrees of one repository share a Git common
    /// directory, so one ref per point would keep only the last snapshot reachable and leave the other recorded one
    /// to be collected.
    fn linked_worktrees() -> LinkedWorktrees {
        let dir = TempDir::new();
        let base = repo(&dir.path().join("base"));
        let first_worktree = dir.path().join("first");
        let second_worktree = dir.path().join("second");
        run_git(
            &base,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "wt-first",
                "--",
                &text_of(&first_worktree),
            ],
        );
        run_git(
            &base,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "wt-second",
                "--",
                &text_of(&second_worktree),
            ],
        );
        let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
        config.engines.lookup_latest = false;
        let core = Arc::new(Core::open(config).unwrap());
        let project = core
            .store
            .add_project(NewProject {
                path: &text_of(&base),
                name: "base",
                is_git: true,
                managed: false,
                scratch: false,
            })
            .unwrap();
        core.store.trust_project(&project.id, "tester").unwrap();
        let folder = core.config.data_dir.join("threads").join("task");
        std::fs::create_dir_all(&folder).unwrap();
        let thread = core
            .store
            .create_thread(NewThread {
                project_id: project.id.clone(),
                title: "t".into(),
                engine: "codex".into(),
                model: None,
                cwd: text_of(&folder),
                worktree: None,
                skill_profile: None,
                parent_thread_id: None,
                relation: None,
                scratch: false,
            })
            .unwrap();
        let second = "repo-second".to_owned();
        for (position, (repo_id, name, worktree)) in [
            (project.id.clone(), "first", &first_worktree),
            (second.clone(), "second", &second_worktree),
        ]
        .into_iter()
        .enumerate()
        {
            core.store
                .thread_repo_insert(
                    &ThreadRepoRecord {
                        thread_id: thread.id.clone(),
                        repo_id,
                        position: position as u32,
                        name: name.to_owned(),
                        branch: "orbyn/thread".into(),
                        // The fixture writes the row itself, with no base: a thread stored before the column.
                        base_branch: None,
                        worktree_path: text_of(worktree),
                    },
                    &crate::util::now(),
                )
                .unwrap();
        }
        LinkedWorktrees {
            dir,
            core,
            thread,
            first: project.id,
            second,
            first_worktree,
            second_worktree,
        }
    }

    struct LinkedWorktrees {
        dir: TempDir,
        core: Arc<Core>,
        thread: Thread,
        first: String,
        second: String,
        first_worktree: PathBuf,
        second_worktree: PathBuf,
    }

    fn scope_of(thread: &Thread, run: &str) -> RunScope {
        RunScope {
            run_id: run.into(),
            thread_id: thread.id.clone(),
            engine: "codex".into(),
            family: "codex".into(),
            write_scope: None,
            mode: PermissionMode::Supervised,
            plan: false,
        }
    }

    /// Every checkpoint ref in the repository, as the worktree sees it: a linked worktree's refs live in the common
    /// directory both of them write to.
    fn checkpoint_refs(worktree: &Path) -> Vec<String> {
        let output = std::process::Command::new("git")
            .args([
                "for-each-ref",
                "--format=%(objectname)",
                "refs/orbyn/checkpoints",
            ])
            .current_dir(worktree)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// The refs under one prefix, by name.
    fn refs_under(worktree: &Path, prefix: &str) -> Vec<String> {
        let output = std::process::Command::new("git")
            .args(["for-each-ref", "--format=%(refname)", prefix])
            .current_dir(worktree)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    #[tokio::test]
    async fn two_worktrees_of_one_repository_keep_their_own_checkpoint_refs() {
        let f = linked_worktrees();
        let run = "run-linked";
        let scope = scope_of(&f.thread, run);
        let run_dir = f.dir.path().join("run");
        std::fs::create_dir_all(&run_dir).unwrap();
        // The two worktrees hold different files, so each snapshot is its own commit.
        std::fs::write(f.first_worktree.join("first.txt"), "first\n").unwrap();
        std::fs::write(f.second_worktree.join("second.txt"), "second\n").unwrap();

        f.core
            .runs
            .checkpoint_start(&f.thread, &scope, Path::new(&f.thread.cwd), &run_dir)
            .await;
        let first = f
            .core
            .store
            .repository_checkpoint(&f.thread.id, run, &f.first)
            .unwrap()
            .unwrap()
            .start;
        let second = f
            .core
            .store
            .repository_checkpoint(&f.thread.id, run, &f.second)
            .unwrap()
            .unwrap()
            .start;
        assert_ne!(first, second, "each repository was snapshotted separately");
        let listed = checkpoint_refs(&f.first_worktree);
        assert!(
            listed.contains(&first),
            "the first repository's start stays reachable: {listed:?}"
        );
        assert!(
            listed.contains(&second),
            "the second repository's start stays reachable: {listed:?}"
        );
        assert_eq!(
            f.core.store.checkpoint(run).unwrap().unwrap().start,
            first,
            "the run's own row keeps the owning repository's start"
        );

        std::fs::write(f.first_worktree.join("more.txt"), "more\n").unwrap();
        std::fs::write(f.second_worktree.join("more.txt"), "more\n").unwrap();
        let thread = f.core.store.thread(&f.thread.id).unwrap();
        f.core.runs.checkpoint_end(&scope, &thread, &run_dir).await;
        let first_end = f
            .core
            .store
            .repository_checkpoint(&f.thread.id, run, &f.first)
            .unwrap()
            .unwrap()
            .end
            .expect("the first repository keeps an end");
        let second_end = f
            .core
            .store
            .repository_checkpoint(&f.thread.id, run, &f.second)
            .unwrap()
            .unwrap()
            .end
            .expect("the second repository keeps an end");
        let listed = checkpoint_refs(&f.first_worktree);
        assert!(
            listed.contains(&first_end),
            "the first repository's end stays reachable: {listed:?}"
        );
        assert!(
            listed.contains(&second_end),
            "the second repository's end stays reachable: {listed:?}"
        );
        assert_eq!(
            f.core.store.checkpoint(run).unwrap().unwrap().end,
            Some(first_end),
            "the run's own row keeps the owning repository's end"
        );
    }

    /// A repository trusted after the run began took no start, so it takes no end either: nothing is snapshotted for
    /// it, and no end ref names a checkpoint the run never had.
    #[tokio::test]
    async fn a_repository_without_a_start_takes_no_end_snapshot() {
        let f = linked_worktrees();
        let run = "run-no-start";
        let scope = scope_of(&f.thread, run);
        let run_dir = f.dir.path().join("run");
        std::fs::create_dir_all(&run_dir).unwrap();
        let start = head(&f.first_worktree);
        f.core
            .store
            .add_repository_checkpoint(&f.thread.id, run, &f.first, &start)
            .unwrap();
        f.core
            .store
            .add_checkpoint(&f.thread.id, run, &start)
            .unwrap();

        let thread = f.core.store.thread(&f.thread.id).unwrap();
        f.core.runs.checkpoint_end(&scope, &thread, &run_dir).await;

        assert!(
            f.core
                .store
                .repository_checkpoint(&f.thread.id, run, &f.first)
                .unwrap()
                .unwrap()
                .end
                .is_some(),
            "the repository that took a start keeps its end"
        );
        assert!(
            f.core
                .store
                .repository_checkpoint(&f.thread.id, run, &f.second)
                .unwrap()
                .is_none(),
            "a repository with no start keeps no end"
        );
        let end_ref = crate::conversation::checkpoints::checkpoint_ref(
            &f.thread.id,
            run,
            Some(&f.second),
            "end",
        );
        let named = refs_under(&f.second_worktree, &end_ref);
        assert!(
            named.is_empty(),
            "no end ref names a checkpoint the run never had: {named:?}"
        );
    }
}
