//! The fix loop: a confirmed finding sends a fix brief to a builder. The brief is written by the core and goes through
//! the same checks as `crew_start` (shape, secrets, protected paths, the key's text, a reviewer never writes), by the
//! same function. Its key is the reviewed task's own checks plus checks of the fix itself, and is never sent:
//!
//! - the lead's repro, when that is what confirmed the finding;
//! - a check that the finding's file now differs from the copy the core kept when the fix began, so a fix that changes
//!   nothing fails (a finding clears only when a fix that proves it touched the file has passed);
//! - a scope check: no file changed other than the files already changed when the fix began and the finding's file, and the
//!   files already changed (the work under review) other than the finding's file are held by their bytes, as a reviewer's are.
//!
//! A finding the check confirmed with a repro is cleared by a fix whose key held that repro. A finding confirmed by agreement
//! has no repro, so no fix clears it alone: the lead supplies one that passes after the fix, or a person clears it.
//!
//! A lease is declared, not enforced, so the scope check is what keeps the builder to the finding's file. The core refuses
//! a fix for a file the reviewed task could not write, a protected path, a test file or CI config, and a file that does not exist.
//! One fix brief per lane and file (a reworded claim is the same finding), at most [`MOST_FIXES`] per reviewed task; past
//! either the loop stops and a person decides.

use std::sync::Arc;

use orbyn_contracts::{CrewCheck, CrewError, CrewObject, Thread};
use serde_json::{Value, json};

use super::{MOST_FIXES, RULES_FILE};
use crate::app::Core;
use crate::crew::checks::{Site, changed, covers};
use crate::crew::guard::{key_needles, refuse_leak};
use crate::crew::leases::internal;
use crate::crew::stamp::Stamper;
use crate::crew::start::{Launch, Links};
use crate::crew::tool::invalid;
use crate::store::{CrewFix, CrewTask, StoredFinding};

/// The most files a fix's scope check lists.
const MOST_PATHS: usize = 255;

/// A file the reviewed task did not protect but a fix still must not write: tests, CI and the crew's own rules.
fn sensitive(path: &str) -> bool {
    let lower = path.to_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    lower.starts_with(".github/")
        || lower.starts_with(".orbyn/")
        || lower.starts_with(".circleci/")
        || lower.starts_with(".gitlab")
        || name == "jenkinsfile"
        // Manifests that define the commands a check runs.
        || matches!(
            name,
            "package.json"
                | "makefile"
                | "justfile"
                | "taskfile.yml"
                | "tox.ini"
                | "noxfile.py"
                | "pyproject.toml"
                | "build.rs"
                | "conftest.py"
                | "pytest.ini"
                | "setup.cfg"
                | "setup.py"
                | "cargo.toml"
                | "cargo.lock"
        )
        || name.starts_with("jest.config.")
        || name.starts_with("vitest.config.")
        || lower.ends_with(".cargo/config.toml")
        || lower.ends_with(".cargo/config")
        || lower
            .split('/')
            .any(|s| matches!(s, "tests" | "test" | "__tests__" | "spec" | "specs" | "e2e"))
        || name.starts_with("test_")
        || [".test.", ".spec.", "_test.", "_spec."]
            .iter()
            .any(|m| name.contains(m))
}

/// Characters a path may have in a check command written without quotes (`cmd /C` cannot be handed them).
fn quotable(path: &str) -> bool {
    path.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-'))
}

impl Core {
    /// Tells the lead why no fix brief was sent, and returns the same as the tool's answer.
    fn no_fix(&self, subject: &CrewTask, stopped: &str, why: &str) -> Value {
        self.tell_lead_text(
            &subject.id,
            &format!("No fix brief was sent: {why} A person decides."),
        );
        json!({ "started": false, "stopped": stopped, "why": why })
    }

    /// Sends a fix brief for a confirmed finding, or stops the loop.
    pub(super) async fn start_fix(
        self: &Arc<Self>,
        thread: &Thread,
        stamper: &Stamper,
        found: &StoredFinding,
        subject: &CrewTask,
        repro: Option<CrewCheck>,
    ) -> Result<Value, CrewError> {
        let finding = &found.finding;
        let path = finding.path.as_str();
        // The guard is keyed on the lane and the file, so rewording a claim is no way to a second fix.
        let earlier = self.store.crew_fixes_of(&subject.id).map_err(internal)?;
        let mut same_place = false;
        for fix in &earlier {
            if let Some(old) = self.store.crew_finding(&fix.finding_id).map_err(internal)?
                && old.lane == found.lane
                && old.finding.path == finding.path
            {
                same_place = true;
            }
        }
        if same_place {
            return Ok(self.no_fix(
                subject,
                "repeated_finding",
                &format!("The {} lane reported {path} again after a fix.", found.lane),
            ));
        }
        if earlier.len() >= MOST_FIXES {
            return Ok(self.no_fix(
                subject,
                "fix_limit",
                &format!("This work has been sent {MOST_FIXES} fix briefs already."),
            ));
        }
        let subject_brief = self
            .store
            .crew_brief(&subject.id)
            .map_err(internal)?
            .ok_or_else(|| invalid("the task has no brief"))?;
        let subject_key = self
            .store
            .crew_answer_key(&subject.id)
            .map_err(internal)?
            .ok_or_else(|| invalid("the task has no key"))?;
        // Which file may a fix write? Only one the reviewed task could.
        let writable = subject_brief.write_paths().any(|p| covers(p, path));
        // A file that a check command of the reviewed task's key names is part of what the checks run. (The lead's repro names the
        // finding's file by design, so it is not held to this.)
        let named = subject_key
            .checks
            .iter()
            .filter_map(CrewCheck::command)
            .any(|run| run.contains(path));
        if !writable || sensitive(path) || named || !quotable(path) {
            return Ok(self.no_fix(
                subject,
                "refused_path",
                &format!("{path} is not a file a fix may write: it is outside what the task could write, or a test, CI or rule file, or its name needs care."),
            ));
        }
        let cwd = std::path::PathBuf::from(&thread.cwd);
        if !cwd.join(path).is_file() {
            return Ok(self.no_fix(
                subject,
                "refused_path",
                &format!("{path} is not a file in the checkout."),
            ));
        }
        // What the file holds now, kept in the repository's own git folder, so a fix that leaves it as it is can be told. The check
        // runs through `cmd /C` on Windows, which does not take the quotes a path with spaces would need, so both paths are
        // plain relative names; a place that is not is refused.
        let git_path = crate::source_control::git::git_hardened(
            &cwd,
            &["rev-parse", "--git-path", "orbyn-fix"],
        )
        .await
        .map(|out| crate::source_control::git::text(&out).replace('\\', "/"))
        .map_err(|e| invalid(e.public_message()))?;
        let copy = format!("{git_path}/{}.orig", &finding.id.as_str()[7..23]);
        if !quotable(&copy) || copy.starts_with('/') {
            return Ok(self.no_fix(
                subject,
                "refused_path",
                "The repository's git folder is not a plain relative place the core can keep a copy in.",
            ));
        }
        std::fs::create_dir_all(cwd.join(&git_path)).map_err(|e| invalid(e.to_string()))?;
        std::fs::copy(cwd.join(path), cwd.join(&copy)).map_err(|e| invalid(e.to_string()))?;
        let dirty = changed(&Site {
            worktree: &cwd,
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(120),
            base: None,
        })
        .await
        .map_err(|e| invalid(e.public_message()))?;
        if dirty.len() > MOST_PATHS {
            return Ok(self.no_fix(subject, "refused_path", "Too many files are changed in the checkout to hold a fix to one file; commit the work first."));
        }
        // The work already changed in the checkout, bar the finding's file, is held by its bytes: the builder may not touch it.
        let held: Vec<&String> = dirty.iter().filter(|p| p.as_str() != path).collect();
        let mut only: Vec<String> = dirty.clone();
        only.push(path.to_owned());
        only.sort();
        only.dedup();

        let claim: String = finding.claim.as_str().chars().take(1000).collect();
        let outcome = format!(
            "Fix a {} finding from the {} review at {}:{}: {claim} Change only {path} and keep all other behaviour.",
            crate::crew::wire_name(&finding.severity),
            found.lane,
            finding.path,
            finding.line,
        );
        let mut checks: Vec<Value> = subject_key
            .checks
            .iter()
            .map(|c| serde_json::to_value(c).map_err(|e| invalid(e.to_string())))
            .collect::<Result<_, _>>()?;
        if let Some(mut repro) = repro {
            if subject_key.checks.iter().any(|c| c.id == repro.id) {
                repro.id =
                    orbyn_contracts::CrewId::new(format!("{}-fix", repro.id)).map_err(invalid)?;
            }
            checks.push(serde_json::to_value(&repro).map_err(|e| invalid(e.to_string()))?);
        }
        checks.push(json!({
            "id": "chk-fix-touched", "kind": "command",
            "params": { "run": format!("git diff --no-index --quiet {copy} {path}"), "expect_exit": 1 },
            "proves": "The finding's file now differs from what it held when the fix began.",
            "does_not_prove": "That the difference fixes the finding.",
        }));
        checks.push(json!({
            "id": "chk-fix-scope", "kind": "file_scope", "params": { "only": only },
            "proves": "No file changed other than the files already changed and the finding's file.",
        }));
        let mut protected: Vec<Value> = subject_key
            .protected_paths
            .iter()
            .map(|p| json!(p))
            .collect();
        protected.push(json!(RULES_FILE));
        protected.extend(held.iter().map(|p| json!(p)));
        if protected.len() > 256 {
            return Ok(self.no_fix(subject, "refused_path", "Too many files would have to be held unchanged for the fix; commit the work first."));
        }
        let draft = |with_inputs: bool| {
            json!({
                "kind": "implement",
                "outcome": outcome,
                "inputs": if with_inputs { json!([{ "kind": "text", "text": finding.evidence }]) } else { json!([]) },
                "files": [{ "path": finding.path, "mode": "write" }],
                "allowed_effects": ["read_repo", "write_leased", "run_checks"],
                "stop_when": ["The finding no longer holds and the checks pass."],
                "budget": { "turns": subject_brief.budget.turns, "minutes": subject_brief.budget.minutes },
                "refs": [finding.id],
            })
        };
        let crew_needles = self.crew_key_needles(&subject.crew_id)?;
        let key_draft = json!({ "checks": checks, "protected_paths": protected });
        // What the reviewer quoted may hold a check's text. Such evidence is left out of the brief, not the brief refused.
        let mut brief = stamper.brief(&draft(true))?;
        let mut key = stamper.answer_key(&key_draft, &brief.id)?;
        let mut needles = crew_needles.clone();
        needles.extend(key_needles(&key));
        if refuse_leak(&CrewObject::Brief(brief.clone()).to_value(), &needles).is_err() {
            brief = stamper.brief(&draft(false))?;
            key = stamper.answer_key(&key_draft, &brief.id)?;
            needles = crew_needles;
            needles.extend(key_needles(&key));
        }
        refuse_leak(&CrewObject::Brief(brief.clone()).to_value(), &needles)?;
        let fix = CrewFix {
            finding_id: finding.id.as_str().to_owned(),
            subject_id: subject.id.clone(),
            fix_task_id: brief.id.as_str().to_owned(),
            fingerprint: found.fingerprint.clone(),
        };
        let started = self
            .launch_crew_task(Launch {
                lead: thread,
                now: stamper.now.clone(),
                brief,
                key,
                role: None,
                lane: None,
                author_engine: None,
                prefix: "worker",
                prompt: None,
                links: Links {
                    review: None,
                    fix: Some(fix),
                },
            })
            .await;
        Ok(match started {
            Ok(mut answer) => {
                answer["started"] = json!(true);
                answer
            }
            Err(error) => json!({ "started": false, "error": error }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::sensitive;

    #[test]
    fn tests_ci_and_the_files_that_define_check_commands_are_sensitive() {
        for path in [
            ".github/workflows/ci.yml",
            "tests/a.rs",
            "src/tests/a.txt",
            "web/app.test.ts",
            "web/package.json",
            "Makefile",
            "justfile",
            ".orbyn/crew.json",
            "Jenkinsfile",
            "tox.ini",
        ] {
            assert!(sensitive(path), "{path}");
        }
        for path in ["src/lib.rs", "docs/readme.md", "crates/x/src/main.rs"] {
            assert!(!sensitive(path), "{path}");
        }
    }
}
