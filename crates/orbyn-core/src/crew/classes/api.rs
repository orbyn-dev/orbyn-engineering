//! The read-only views the routes serve: the rules at the commit, a path tester and a task's routing. They read git and the
//! ledger and change nothing. With the Crew Preview flag off none of them exists.

use std::path::Path;

use orbyn_contracts as wire;

use super::record::{ROUTE_CLASS, Stored};
use super::{
    BUILT_IN, ClassFile, INVALID_RULE, PathClasses, PinnedRef, base_commit, classify_paths,
    path_problem, pinned_ref, read_class_file,
};
use crate::app::Core;
use crate::crew::EngineView;
use crate::crew::RULES_FILE;
use crate::crew::valid_id;
use crate::error::{CoreError, Result};
use crate::source_control::git::{git_hardened as git, text};

/// Paths the tester takes at once.
pub const TEST_PATHS_MAX: usize = 10;
const PATH_MAX: usize = 512;

fn engine(id: &str, views: &[EngineView]) -> wire::CrewClassEngine {
    match views.iter().find(|v| v.id.as_str() == id) {
        Some(view) => wire::CrewClassEngine {
            id: id.to_owned(),
            name: view.name.clone(),
            on_this_machine: true,
        },
        None => wire::CrewClassEngine {
            id: id.to_owned(),
            name: id.to_owned(),
            on_this_machine: false,
        },
    }
}

/// `Claude, Codex`, with an engine this machine lacks said so, or `none`.
pub(in crate::crew) fn engine_words(ids: &[String], views: &[EngineView]) -> String {
    if ids.is_empty() {
        return "none".to_owned();
    }
    ids.iter()
        .map(|id| {
            let one = engine(id, views);
            if one.on_this_machine {
                one.name
            } else {
                format!("{id} (not on this machine)")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The default branch and the file at it, when the repository has one.
async fn read_pinned(root: &Path) -> Option<(PinnedRef, ClassFile)> {
    let pin = pinned_ref(root).await?;
    let file = read_class_file(root, Some(&pin.commit)).await;
    Some((pin, file))
}

/// What a route card says about a set of classes.
fn route_card(classes: &PathClasses, views: &[EngineView]) -> String {
    let card = route_card_base(classes, views);
    match classes.differs() {
        Some(words) => format!("{card} {words}"),
        None => card,
    }
}

fn route_card_base(classes: &PathClasses, views: &[EngineView]) -> String {
    if classes.invalid() {
        return format!(
            "{INVALID_RULE}: {}. Every engine is blocked until the file is fixed and committed.",
            classes.problem_text()
        );
    }
    let allowed = classes.allowed();
    let hits = classes.matched();
    match (hits.as_slice(), allowed) {
        ([], _) | (_, None) => format!("{BUILT_IN}: any signed-in engine can take this."),
        ([hit], Some(ids)) => {
            format!(
                "{}: {} Allowed engines: {}.",
                hit.name,
                hit.reason,
                engine_words(&ids, views)
            )
        }
        (_, Some(ids)) => {
            let names = classes.names().join(" and ");
            if ids.is_empty() {
                format!(
                    "In {names}: no engine is allowed by all of them, so nothing can take this."
                )
            } else {
                format!(
                    "In {names}: the engines all of them allow are {}.",
                    engine_words(&ids, views)
                )
            }
        }
    }
}

impl Core {
    async fn class_machine(&self, project_id: &str) -> Vec<EngineView> {
        self.crew_machine(Some(project_id))
            .await
            .map(|m| m.views)
            .unwrap_or_default()
    }

    fn class_thread(&self, thread_id: &str) -> Result<orbyn_contracts::Thread> {
        self.crew_preview_only()?;
        valid_id(thread_id)?;
        self.store.thread(thread_id)
    }

    /// `GET /api/crew/classes`: the rules at the commit a task started in this thread would start from.
    pub async fn crew_classes_view(&self, thread_id: &str) -> Result<wire::CrewClassesView> {
        let thread = self.class_thread(thread_id)?;
        let root = Path::new(&thread.cwd);
        let base = base_commit(root).await;
        let file = read_class_file(root, base.as_deref()).await;
        let views = self.class_machine(&thread.project_id).await;
        let differs = working_copy_differs(root, &file).await;
        let mut notes = file.notes.clone();
        if let Some((pin, there)) = read_pinned(root).await {
            let both = PathClasses::of_file(file.clone()).with_pinned(
                pin.name,
                pin.commit,
                PathClasses::of_file(there),
            );
            notes.extend(both.differs());
        }
        let worktree = match &thread.worktree {
            Some(tree) => tree.branch.clone(),
            None => root
                .file_name()
                .map_or_else(|| thread.cwd.clone(), |n| n.to_string_lossy().into_owned()),
        };
        Ok(wire::CrewClassesView {
            thread: thread.id,
            commit: file.commit.clone(),
            blob: file.blob.clone(),
            worktree,
            built_in: BUILT_IN.to_owned(),
            rules: file
                .rules
                .iter()
                .map(|rule| wire::CrewClassRule {
                    name: rule.name.clone(),
                    paths: rule.paths.clone(),
                    engines: rule.engines.iter().map(|id| engine(id, &views)).collect(),
                    reason: rule.reason.clone(),
                })
                .collect(),
            problems: file.problems.clone(),
            notes,
            working_copy_differs: differs,
            working_copy_note: differs.then(|| {
                "This checkout's copy of the file differs from the committed one. An edit is not in force until it is committed."
                    .to_owned()
            }),
        })
    }

    /// `GET /api/crew/classes/test`: up to 10 paths against the rules at the commit.
    pub async fn crew_class_test(
        &self,
        thread_id: &str,
        paths: &[String],
    ) -> Result<wire::CrewClassTest> {
        let thread = self.class_thread(thread_id)?;
        if paths.is_empty() || paths.len() > TEST_PATHS_MAX {
            return Err(CoreError::Invalid(format!(
                "give 1 to {TEST_PATHS_MAX} paths"
            )));
        }
        let root = Path::new(&thread.cwd);
        let base = base_commit(root).await;
        let file = read_class_file(root, base.as_deref()).await;
        let views = self.class_machine(&thread.project_id).await;
        let pinned = read_pinned(root).await;
        let mut results = Vec::new();
        for path in paths {
            let problem = if path.chars().count() > PATH_MAX {
                Some(format!("is longer than {PATH_MAX} characters"))
            } else {
                path_problem(path).map(str::to_owned)
            };
            if let Some(why) = problem {
                results.push(wire::CrewClassTestPath {
                    path: path.chars().take(PATH_MAX).collect(),
                    valid: false,
                    problem: Some(format!("This path {why}.")),
                    classes: Vec::new(),
                    allowed_engines: None,
                    route_card: "Not a repository path, so nothing was checked.".to_owned(),
                });
                continue;
            }
            let mut classes = classify_paths(
                root,
                base.as_deref(),
                file.clone(),
                std::slice::from_ref(path),
            )
            .await;
            if let Some((pin, there)) = &pinned {
                let side = classify_paths(
                    root,
                    base.as_deref(),
                    there.clone(),
                    std::slice::from_ref(path),
                )
                .await;
                classes = classes.with_pinned(pin.name.clone(), pin.commit.clone(), side);
            }
            results.push(wire::CrewClassTestPath {
                path: path.clone(),
                valid: true,
                problem: None,
                classes: classes
                    .matched()
                    .into_iter()
                    .map(|hit| wire::CrewClassMatch {
                        pattern: hit.shown_pattern(),
                        name: hit.name,
                    })
                    .collect(),
                allowed_engines: classes
                    .allowed()
                    .map(|ids| ids.iter().map(|id| engine(id, &views)).collect()),
                route_card: route_card(&classes, &views),
            });
        }
        Ok(wire::CrewClassTest {
            thread: thread.id,
            commit: file.commit.clone(),
            blob: file.blob.clone(),
            problems: file.problems.clone(),
            paths: results,
        })
    }

    /// `GET /api/crew/route/{task}`: the decision or refusal of a task's routing, and its ledger rows.
    pub fn crew_route_view(&self, task_id: &str) -> Result<wire::CrewRouteView> {
        self.crew_preview_only()?;
        valid_id(task_id)?;
        let task = self
            .store
            .crew_task(task_id)?
            .ok_or_else(|| CoreError::NotFound("task".into()))?;
        let decision = self.store.crew_route_decisions(task_id)?.pop();
        let rows: Vec<wire::CrewRouteClassRow> = self
            .store
            .crew_autopilot_events(task_id)?
            .into_iter()
            .filter(|event| event.kind == ROUTE_CLASS)
            .map(|event| {
                let stored: Stored = serde_json::from_value(event.body).unwrap_or_default();
                wire::CrewRouteClassRow {
                    id: event.id.to_string(),
                    at: event.at,
                    commit: stored.commit,
                    blob: stored.blob,
                    problem: stored.problem,
                    unknown: stored.unknown,
                    classes: stored.classes,
                    blocked: stored.blocked,
                    chosen: stored.chosen,
                    rejected: stored.rejected,
                    message: stored.message,
                    requested: stored.requested,
                    withheld: stored.withheld,
                }
            })
            .collect();
        let refusal = match &decision {
            Some(_) => None,
            None => rows
                .iter()
                .rev()
                .find(|row| row.chosen.is_none() && row.message.is_some())
                .map(|row| wire::CrewRouteRefusal {
                    message: row.message.clone().unwrap_or_default(),
                    requested: row.requested.clone(),
                    rejected: row.rejected.clone(),
                }),
        };
        Ok(wire::CrewRouteView {
            task: task.id,
            state: task.state,
            decision,
            refusal,
            rows,
        })
    }
}

/// Whether the checkout's copy of the file is not the committed one. Anything unreadable counts as different.
async fn working_copy_differs(root: &Path, file: &ClassFile) -> bool {
    let path = root.join(RULES_FILE);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return file.blob.is_some();
        }
        Err(_) => return true,
    };
    if !meta.is_file() || meta.len() > super::FILE_MAX {
        return true;
    }
    match git(root, &["hash-object", "--no-filters", "--", RULES_FILE]).await {
        Ok(out) => file.blob.as_deref() != Some(text(&out).as_str()),
        Err(_) => true,
    }
}
