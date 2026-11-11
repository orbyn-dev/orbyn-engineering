//! What a merge must not carry unseen. The agents are kept from editing `.orbyn/`, the answer key's protected files and the files a
//! check runs by the always-denied list, which is a heuristic over what reaches the gate. This is the backstop at the point the work
//! lands: a diff that touches any of them, or CI config, is held for a person whatever the mode, and a person's release lets it go.
//! A merge that rewrote `.orbyn/crew.json` or the script a check calls would otherwise widen what later tasks may reach, or turn a
//! check into `exit 0`, and stay green.

use orbyn_contracts::CrewPathPattern;

use crate::crew::autopilot::shell::simple_commands;
use crate::crew::autopilot::tier::is_ci;
use crate::crew::checks::covers;

/// The files and folders the check commands name, as repository-relative words (no `./`, no variable, not absolute).
pub fn check_words(commands: &[String]) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for command in commands.iter().flat_map(|c| simple_commands(c)) {
        for word in std::iter::once(&command.raw)
            .chain(&command.args)
            .chain(&command.redirects)
        {
            let word = word.strip_prefix("./").unwrap_or(word).replace('\\', "/");
            if word.is_empty() || word.starts_with(['-', '/', '$', '~', '%']) || word.contains("..")
            {
                continue;
            }
            // `--file=x` style values are cut at the `=`.
            let word = word.rsplit('=').next().unwrap_or(&word).to_owned();
            if !words.contains(&word) {
                words.push(word);
            }
        }
    }
    words
}

/// Why a diff is held, in words; empty when nothing sensitive is touched.
pub fn sensitive(
    paths: &[String],
    protected: &[CrewPathPattern],
    checks: &[String],
) -> Vec<String> {
    let mut why: Vec<String> = Vec::new();
    let mut note = |text: &str| {
        if !why.iter().any(|w| w == text) {
            why.push(text.to_owned());
        }
    };
    let named = check_words(checks);
    for path in paths {
        if path.starts_with(".orbyn/") {
            note("it changes .orbyn/, the crew's own rules");
        }
        if protected.iter().any(|p| covers(p, path)) {
            note("it changes a file the answer key protects");
        }
        if is_ci(path) {
            note("it changes CI config");
        }
        // A file a check names, or any file under a folder it names with a trailing slash.
        if named
            .iter()
            .any(|w| w == path || w.ends_with('/') && path.starts_with(w.as_str()))
        {
            note("it changes a file one of the repository's checks runs or reads");
        }
    }
    why
}

/// The review lanes a merge is owed: code review for anything that is not low, and security review for high.
pub fn owed_lanes(risk: super::Risk) -> Vec<String> {
    match risk {
        super::Risk::Low => Vec::new(),
        super::Risk::Medium => vec!["code".to_owned()],
        super::Risk::High => vec!["code".to_owned(), "security".to_owned()],
    }
}
