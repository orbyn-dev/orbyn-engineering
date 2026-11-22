//! What a quiet engine console's last line has to look like to be waiting for the
//! person. The words are a heuristic, never an authority: a build that stopped
//! quietly is not a question, and a fake prompt can only draw text.

use orbyn_contracts::TerminalAttentionKind;

/// The choice markers that end a yes/no prompt, compared lowercased.
const YES_NO_MARKERS: &[&str] = &["[y/n]", "(y/n)", "[yes/no]", "(yes/no)"];

/// A line that is a build tool's output, not a prompt, even when it ends in `?`.
const BUILD_LOG_PREFIXES: &[&str] = &[
    "compiling ",
    "finished ",
    "error:",
    "error[",
    "warning:",
    "warning[",
    "note:",
    "help:",
    "make:",
    "npm err!",
    "npm warn",
    "test result:",
    "installing ",
    "downloading ",
];

/// The words that make a trailing colon read like a prompt.
const PROMPT_WORDS: &[&str] = &[
    "password",
    "passphrase",
    "passcode",
    "secret",
    "token",
    "username",
    "login",
    "enter",
    "choose",
    "select",
    "confirm",
    "continue",
    "type",
    "input",
    "provide",
    "specify",
    "which",
    "what",
    "pick",
];

/// The kind of waiting a cleaned terminal text reads like, if any. ANSI and control
/// characters are stripped first, so a test may pass raw PTY output. The watcher
/// already has the plain text and calls `classify_plain`.
#[cfg(test)]
pub(crate) fn classify(text: &str) -> Option<TerminalAttentionKind> {
    classify_plain(&super::excerpt::plain(text))
}

/// `classify` for text already passed through `excerpt::plain`.
pub(crate) fn classify_plain(text: &str) -> Option<TerminalAttentionKind> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect();
    let last = *lines.last()?;
    let lower = last.to_ascii_lowercase();
    let marker = last.trim_end_matches(':').trim_end().to_ascii_lowercase();
    if YES_NO_MARKERS.iter().any(|one| marker.ends_with(one)) {
        return Some(TerminalAttentionKind::YesNo);
    }
    if build_log(&lower) {
        return None;
    }
    if last.ends_with('?') {
        return Some(TerminalAttentionKind::Question);
    }
    if last.ends_with(':') && prompt_like(&lower) {
        return Some(TerminalAttentionKind::Question);
    }
    if choice_list(&lines) {
        return Some(TerminalAttentionKind::Question);
    }
    None
}

fn build_log(lower: &str) -> bool {
    let line = lower.trim_start();
    BUILD_LOG_PREFIXES
        .iter()
        .any(|prefix| line.starts_with(prefix))
}

/// A trailing colon after one of the words a prompt uses, such as `Password:` or
/// `Enter a name:`.
fn prompt_like(lower: &str) -> bool {
    let stem = lower.trim_end_matches(':').trim_end();
    if stem.contains("api key") || stem.contains("access key") {
        return true;
    }
    stem.split(|char: char| !(char.is_ascii_alphanumeric() || char == '_' || char == '-'))
        .any(|word| PROMPT_WORDS.contains(&word))
}

/// A numbered or arrow-marked choice list whose header ended like a prompt.
fn choice_list(lines: &[&str]) -> bool {
    let mut first = lines.len();
    while first > 0 && is_choice_item(lines[first - 1]) {
        first -= 1;
    }
    let count = lines.len() - first;
    if count == 0 {
        return false;
    }
    if let Some(header) = first.checked_sub(1).map(|at| lines[at])
        && (header.ends_with('?') || header.ends_with(':') || prompt_like(header))
    {
        return true;
    }
    // An arrow marks the selected row, so the list itself is the prompt.
    lines[first..].iter().any(|line| is_arrow_item(line))
}

fn is_arrow_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    ["❯", ">", "›", "→"]
        .iter()
        .any(|marker| trimmed.starts_with(marker))
}

fn is_choice_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    let arrow = ["❯", ">", "›", "→"]
        .iter()
        .find_map(|marker| trimmed.strip_prefix(marker));
    let rest = arrow.map_or(trimmed, str::trim_start);
    if starts_with_number(rest) {
        return true;
    }
    arrow.is_some() && !rest.is_empty()
}

fn starts_with_number(text: &str) -> bool {
    let digits = text
        .chars()
        .take_while(|char| char.is_ascii_digit())
        .count();
    if digits == 0 {
        return false;
    }
    let mut rest = text.chars().skip(digits);
    matches!(rest.next(), Some('.') | Some(')'))
        && rest.next().is_some_and(|char| char.is_whitespace())
}
