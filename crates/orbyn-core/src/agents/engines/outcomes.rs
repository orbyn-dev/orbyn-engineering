//! Turns one install, update or removal into the outcome kept for the provider page.
//! Every piece of text passes the locked redactor first: the command, the reason and the
//! installer's lines. Nothing raw is stored, and no setting can loosen that.

use orbyn_contracts::{InstallKind, InstallOutcome};

use crate::agents::driver::diagnostics::clean;
use crate::error::CoreError;
use crate::redaction::Redactor;
use crate::util::{new_id, now};

/// The last lines of an installer's output that are kept.
const KEPT_LINES: usize = 20;
const LINE_CHARS: usize = 300;
const COMMAND_CHARS: usize = 300;
const REASON_CHARS: usize = 400;

/// What an installer left behind, for the outcome. Empty when none ran.
#[derive(Debug, Default)]
pub struct Ran {
    /// Its exit status; absent when it was stopped by a signal or never ran.
    pub exit: Option<i32>,
    /// Everything it printed (bounded by the caller), before redaction.
    pub output: String,
    /// It ran and reported failure, as opposed to a check around it failing.
    pub failed: bool,
}

/// An attempt that has started and not yet ended.
pub struct Attempt {
    engine: String,
    kind: InstallKind,
    started_at: String,
    command: Option<String>,
    from_version: Option<String>,
}

impl Attempt {
    pub fn begin(
        engine: &str,
        kind: InstallKind,
        command: Option<&str>,
        from_version: Option<String>,
    ) -> Self {
        Self {
            engine: engine.to_owned(),
            kind,
            started_at: now(),
            command: command.map(hide),
            from_version,
        }
    }

    /// Ends the attempt: `error` is why it failed, `to_version` what the engine reports now.
    pub fn finish(
        self,
        ran: &Ran,
        error: Option<&CoreError>,
        to_version: Option<String>,
    ) -> InstallOutcome {
        InstallOutcome {
            id: new_id(),
            engine: self.engine,
            kind: self.kind,
            started_at: self.started_at,
            finished_at: now(),
            ok: error.is_none(),
            reason: error.map(|error| reason(error, ran)),
            command: self
                .command
                .map(|command| command.chars().take(COMMAND_CHARS).collect()),
            exit_status: ran.exit,
            from_version: self.from_version,
            to_version: if error.is_none() { to_version } else { None },
            output: lines(&ran.output),
        }
    }
}

/// `text` without terminal codes, credential-looking lines or any recognised secret format.
pub fn hide(text: &str) -> String {
    Redactor::locked().replace_each(&clean(text), "[hidden]")
}

/// The failure in plain words. An installer that ran and failed gets its status, and its own
/// words stay in the kept lines; any other failure already reads as a sentence.
fn reason(error: &CoreError, ran: &Ran) -> String {
    let text = if ran.failed {
        match ran.exit {
            Some(code) => format!("The installer stopped with an error (exit status {code})."),
            None => "The installer was stopped before it finished.".to_owned(),
        }
    } else {
        error.public_message()
    };
    hide(&text).chars().take(REASON_CHARS).collect()
}

fn lines(output: &str) -> Vec<String> {
    let hidden = hide(output);
    let all: Vec<&str> = hidden
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    all[all.len().saturating_sub(KEPT_LINES)..]
        .iter()
        .map(|line| line.chars().take(LINE_CHARS).collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";

    fn failed(exit: Option<i32>) -> Ran {
        Ran {
            exit,
            output: "npm error EACCES: permission denied\n".into(),
            failed: true,
        }
    }

    #[test]
    fn a_failed_installer_is_said_by_its_status_and_keeps_its_own_words() {
        let error = CoreError::UpdateFailed("npm error EACCES: permission denied".into());
        let outcome = Attempt::begin("codex", InstallKind::Update, Some("npm i -g x"), None)
            .finish(&failed(Some(243)), Some(&error), None);
        assert!(!outcome.ok);
        assert_eq!(
            outcome.reason.as_deref(),
            Some("The installer stopped with an error (exit status 243).")
        );
        assert_eq!(outcome.exit_status, Some(243));
        assert_eq!(outcome.output, ["npm error EACCES: permission denied"]);
        let stopped = Attempt::begin("codex", InstallKind::Update, None, None).finish(
            &failed(None),
            Some(&error),
            None,
        );
        assert_eq!(
            stopped.reason.as_deref(),
            Some("The installer was stopped before it finished.")
        );
    }

    #[test]
    fn a_check_around_the_installer_keeps_its_own_sentence() {
        let error =
            CoreError::UpdateFailed("The update ran, but this is still version 1.0.0.".into());
        let ran = Ran {
            exit: Some(0),
            ..Ran::default()
        };
        let outcome = Attempt::begin("codex", InstallKind::Update, None, Some("1.0.0".into()))
            .finish(&ran, Some(&error), Some("1.0.0".into()));
        assert!(outcome.reason.unwrap().contains("still version 1.0.0"));
        assert_eq!(outcome.to_version, None);
        assert_eq!(outcome.from_version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn a_success_has_no_reason_and_the_new_version() {
        let ran = Ran {
            exit: Some(0),
            output: "added 1 package\n".into(),
            failed: false,
        };
        let outcome = Attempt::begin("codex", InstallKind::Install, Some("npm i"), None).finish(
            &ran,
            None,
            Some("2.0.0".into()),
        );
        assert!(outcome.ok);
        assert_eq!(outcome.reason, None);
        assert_eq!(outcome.to_version.as_deref(), Some("2.0.0"));
        assert_eq!(outcome.kind, InstallKind::Install);
    }

    #[test]
    fn no_secret_reaches_the_command_the_reason_or_the_lines() {
        let ran = Ran {
            exit: Some(1),
            output: format!(
                "fetching\nusing {SECRET} to log in\nAuthorization: Bearer abc\nlast line\n"
            ),
            failed: false,
        };
        let error = CoreError::UpdateFailed(format!("it broke with {SECRET}"));
        let outcome = Attempt::begin(
            "codex",
            InstallKind::Update,
            Some(&format!("npx -y pkg --token {SECRET}")),
            None,
        )
        .finish(&ran, Some(&error), None);
        let all = format!(
            "{:?} {:?} {:?}",
            outcome.command, outcome.reason, outcome.output
        );
        assert!(!all.contains(SECRET), "{all}");
        assert!(!all.contains("Bearer abc"), "{all}");
        assert!(outcome.output.contains(&"last line".to_owned()));
        assert!(outcome.output.contains(&"fetching".to_owned()));
    }

    #[test]
    fn only_the_last_lines_are_kept_and_each_is_cut() {
        let output: String = (0..50).map(|n| format!("line {n}\n")).collect();
        let long = "x".repeat(1000);
        let ran = Ran {
            output: format!("{output}{long}\n"),
            ..Ran::default()
        };
        let outcome =
            Attempt::begin("codex", InstallKind::Update, None, None).finish(&ran, None, None);
        assert_eq!(outcome.output.len(), KEPT_LINES);
        assert_eq!(outcome.output[KEPT_LINES - 2], "line 49");
        assert_eq!(outcome.output[KEPT_LINES - 1].chars().count(), LINE_CHARS);
    }
}
