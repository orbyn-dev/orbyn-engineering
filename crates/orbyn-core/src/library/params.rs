//! Parameters and rendering (docs/execution-profiles-and-library.md, Part 2). A placeholder is `{{name}}`. For a workflow every value
//! is quoted for the target shell at expansion, so a value is always one word and can never add a second command; a placeholder that
//! sits inside the command's own quotes is refused at save, because quoting there would not hold. For a prompt a value is plain text.

use std::collections::BTreeMap;

use orbyn_contracts::{Param, ParamKind};

use crate::error::{CoreError, Result};

pub const PARAMS_PER_ITEM: usize = 16;
pub const NAME_MAX: usize = 32;
pub const LABEL_MAX: usize = 60;
pub const DEFAULT_MAX: usize = 500;
pub const CHOICE_MAX: usize = 60;
pub const CHOICES_MIN: usize = 2;
pub const CHOICES_MAX: usize = 20;
/// A value a person types into a parameter, in bytes.
pub const VALUE_MAX: usize = 4000;
/// The most a rendered command or text may be, so sixteen long values cannot build a body past a sane size.
pub const RENDER_MAX: usize = 256 * 1024;

fn invalid(field: &str, rule: &str) -> CoreError {
    CoreError::Invalid(format!("{field}: {rule}"))
}

fn name_shaped(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() <= NAME_MAX
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn plain(text: &str) -> bool {
    !text.chars().any(char::is_control)
}

/// A path relative to the project: no root, no drive, no `..`, no leading `-` (it would read as an option), no control characters.
pub fn path_ok(path: &str) -> bool {
    let first = path.chars().next();
    let drive =
        path.len() >= 2 && path.as_bytes()[1] == b':' && path.as_bytes()[0].is_ascii_alphabetic();
    !path.is_empty()
        && plain(path)
        && !matches!(first, Some('/' | '\\' | '~' | '-'))
        && !drive
        && !path.split(['/', '\\']).any(|part| part == "..")
}

/// The parameters as stored: names shaped and unique, labels trimmed (a missing one reads as the name), choices and defaults checked.
pub fn clean_params(params: &[Param]) -> Result<Vec<Param>> {
    if params.len() > PARAMS_PER_ITEM {
        return Err(invalid(
            "params",
            &format!("use at most {PARAMS_PER_ITEM} parameters."),
        ));
    }
    let mut out: Vec<Param> = Vec::with_capacity(params.len());
    for (index, param) in params.iter().enumerate() {
        let at = format!("params[{index}]");
        if !name_shaped(&param.name) {
            return Err(invalid(
                &format!("{at}.name"),
                "use a lowercase letter, then lowercase letters, digits or underscores, at most 32.",
            ));
        }
        if out.iter().any(|one| one.name == param.name) {
            return Err(invalid(&format!("{at}.name"), "use each name once."));
        }
        let label = param.label.trim();
        if label.chars().count() > LABEL_MAX || !plain(label) {
            return Err(invalid(
                &format!("{at}.label"),
                &format!("use at most {LABEL_MAX} characters, no control characters."),
            ));
        }
        let default = match &param.default {
            Some(text) if text.chars().count() > DEFAULT_MAX || text.contains('\0') => {
                return Err(invalid(
                    &format!("{at}.default"),
                    &format!("use at most {DEFAULT_MAX} characters."),
                ));
            }
            Some(text) if text.is_empty() => None,
            other => other.clone(),
        };
        let choices = match (param.kind, &param.choices) {
            (ParamKind::Choice, Some(choices)) => {
                let fine = (CHOICES_MIN..=CHOICES_MAX).contains(&choices.len())
                    && choices
                        .iter()
                        .all(|c| !c.is_empty() && c.chars().count() <= CHOICE_MAX && plain(c))
                    && choices
                        .iter()
                        .enumerate()
                        .all(|(i, c)| !choices[..i].contains(c));
                if !fine {
                    return Err(invalid(
                        &format!("{at}.choices"),
                        &format!(
                            "use {CHOICES_MIN} to {CHOICES_MAX} different choices of 1 to {CHOICE_MAX} characters."
                        ),
                    ));
                }
                Some(choices.clone())
            }
            (ParamKind::Choice, None) => {
                return Err(invalid(
                    &format!("{at}.choices"),
                    &format!("a choice needs {CHOICES_MIN} to {CHOICES_MAX} choices."),
                ));
            }
            (_, Some(choices)) if !choices.is_empty() => {
                return Err(invalid(
                    &format!("{at}.choices"),
                    "only a choice parameter has choices.",
                ));
            }
            _ => None,
        };
        if let Some(text) = &default {
            let allowed = match (param.kind, &choices) {
                (ParamKind::Choice, Some(choices)) => choices.contains(text),
                (ParamKind::Path, _) => path_ok(text),
                _ => true,
            };
            if !allowed {
                return Err(invalid(
                    &format!("{at}.default"),
                    "the default must be one of the choices, or a path inside the project.",
                ));
            }
        }
        out.push(Param {
            name: param.name.clone(),
            label: if label.is_empty() {
                param.name.clone()
            } else {
                label.to_owned()
            },
            kind: param.kind,
            default,
            required: param.required,
            choices,
        });
    }
    Ok(out)
}

/// The quote state a placeholder sits in, in the way a POSIX shell reads the text before it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Quote {
    Outside,
    Single,
    Double,
    /// `$'...'`, where a backslash escapes.
    AnsiC,
}

/// A placeholder in `text`: its name and where the quote state was when it began.
fn placeholders(text: &str) -> Vec<(String, Quote)> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut state = Quote::Outside;
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at..].starts_with(b"{{")
            && let Some((name, end)) = placeholder_at(text, at)
        {
            found.push((name, state));
            at = end;
            continue;
        }
        let byte = bytes[at];
        match state {
            Quote::Outside => match byte {
                b'\\' => at += 1,
                b'\'' => state = Quote::Single,
                b'"' => state = Quote::Double,
                b'$' if bytes.get(at + 1) == Some(&b'\'') => {
                    state = Quote::AnsiC;
                    at += 1;
                }
                _ => {}
            },
            Quote::Single => {
                if byte == b'\'' {
                    state = Quote::Outside;
                }
            }
            Quote::Double => match byte {
                b'\\' => at += 1,
                b'"' => state = Quote::Outside,
                _ => {}
            },
            Quote::AnsiC => match byte {
                b'\\' => at += 1,
                b'\'' => state = Quote::Outside,
                _ => {}
            },
        }
        at += 1;
    }
    found
}

/// `{{name}}` at byte `at`: the name and the byte after the closing braces.
fn placeholder_at(text: &str, at: usize) -> Option<(String, usize)> {
    let rest = &text[at + 2..];
    let close = rest.find("}}")?;
    let name = &rest[..close];
    name_shaped(name).then(|| (name.to_owned(), at + 2 + close + 2))
}

/// Every placeholder names a parameter. For a workflow, none sits inside the command's own quotes and none shares a command with a
/// heredoc: neither can be quoted safely.
pub fn check_template(field: &str, text: &str, params: &[Param], shell: bool) -> Result<()> {
    let found = placeholders(text);
    if found
        .iter()
        .any(|(name, _)| !params.iter().any(|one| &one.name == name))
    {
        return Err(invalid(
            field,
            "a {{placeholder}} names no parameter of this item.",
        ));
    }
    if shell {
        if found.iter().any(|(_, state)| *state != Quote::Outside) {
            return Err(invalid(
                field,
                "put each {{placeholder}} outside quotes: Orbyn quotes the value for the shell.",
            ));
        }
        if !found.is_empty() && text.contains("<<") {
            return Err(invalid(
                field,
                "a command with a here-document cannot take {{placeholders}}.",
            ));
        }
    }
    Ok(())
}

/// How a value is written into a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shell {
    /// `sh`, `bash`, `zsh`, `fish`: single quotes.
    Posix,
    /// `cmd.exe`: double quotes, and values with characters it expands are refused.
    Cmd,
}

impl Shell {
    pub const fn native() -> Self {
        if cfg!(windows) {
            Self::Cmd
        } else {
            Self::Posix
        }
    }
}

/// One value as one word. An empty value is no word at all (an optional parameter left empty disappears).
fn quote(value: &str, shell: Shell) -> Result<String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    match shell {
        Shell::Posix => Ok(format!("'{}'", value.replace('\'', "'\\''"))),
        Shell::Cmd => {
            if value
                .chars()
                .any(|c| matches!(c, '"' | '%' | '^' | '!' | '\n' | '\r'))
            {
                return Err(invalid(
                    "values",
                    "a value cannot hold a quote, %, ^, ! or a line break on this system.",
                ));
            }
            let trailing = value.len() - value.trim_end_matches('\\').len();
            Ok(format!("\"{value}{}\"", "\\".repeat(trailing)))
        }
    }
}

/// The value each parameter takes: what was given, else its default, else empty. A required one with nothing is refused, a choice
/// outside its list is refused, and so is a path that leaves the project. The refusal names the parameter, never the value.
fn resolve(
    params: &[Param],
    values: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>> {
    if let Some(unknown) = values
        .keys()
        .find(|name| !params.iter().any(|one| &one.name == *name))
    {
        // Parameter names are shaped by the same rule, so naming it cannot carry a value.
        let shown: String = unknown
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(32)
            .collect();
        return Err(invalid(
            &format!("values.{shown}"),
            "this is not a parameter of this item.",
        ));
    }
    let mut out = BTreeMap::new();
    for param in params {
        let given = values.get(&param.name).filter(|text| !text.is_empty());
        let value = given
            .or(param.default.as_ref())
            .cloned()
            .unwrap_or_default();
        let at = format!("values.{}", param.name);
        if value.is_empty() {
            if param.required {
                return Err(invalid(&at, "this parameter is required."));
            }
        } else {
            if value.len() > VALUE_MAX || value.contains('\0') {
                return Err(invalid(
                    &at,
                    &format!("use at most {VALUE_MAX} bytes, no NUL."),
                ));
            }
            match (param.kind, &param.choices) {
                (ParamKind::Choice, Some(choices)) if !choices.contains(&value) => {
                    return Err(invalid(&at, "choose one of the listed choices."));
                }
                (ParamKind::Path, _) if !path_ok(&value) => {
                    return Err(invalid(
                        &at,
                        "use a path inside the project: relative, no `..`.",
                    ));
                }
                _ => {}
            }
        }
        out.insert(param.name.clone(), value);
    }
    Ok(out)
}

fn fill(
    template: &str,
    params: &[Param],
    values: &BTreeMap<String, String>,
    mut write: impl FnMut(&str) -> Result<String>,
) -> Result<String> {
    let resolved = resolve(params, values)?;
    let mut out = String::with_capacity(template.len());
    let mut at = 0;
    while at < template.len() {
        if template[at..].starts_with("{{")
            && let Some((name, end)) = placeholder_at(template, at)
        {
            let value = resolved.get(&name).ok_or_else(|| {
                invalid(
                    "template",
                    "a {{placeholder}} names no parameter of this item.",
                )
            })?;
            out.push_str(&write(value)?);
            at = end;
        } else {
            let ch = template[at..].chars().next().unwrap_or('\0');
            out.push(ch);
            at += ch.len_utf8();
        }
        if out.len() > RENDER_MAX {
            return Err(invalid("values", "the result is too long."));
        }
    }
    Ok(out)
}

/// The exact command that would run: each value quoted for `shell`. It holds no environment value.
pub fn render_command(
    command: &str,
    params: &[Param],
    values: &BTreeMap<String, String>,
    shell: Shell,
) -> Result<String> {
    // A saved workflow was checked, but a stored row is read again here: the same rule holds on the way out.
    check_template("command", command, params, true)?;
    fill(command, params, values, |value| quote(value, shell))
}

/// The prompt text: each value as typed.
pub fn render_prompt(
    body: &str,
    params: &[Param],
    values: &BTreeMap<String, String>,
) -> Result<String> {
    fill(body, params, values, |value| Ok(value.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One value rendered by the `cmd` path as the whole command.
    fn cmd_word(value: &str) -> Result<String> {
        let one = Param {
            name: "v".into(),
            label: "v".into(),
            kind: ParamKind::Text,
            default: Some(value.to_owned()),
            required: false,
            choices: None,
        };
        render_command("{{v}}", &[one], &BTreeMap::new(), Shell::Cmd)
    }

    /// What the `cmd` path should write for `value`: one double-quoted word, with `extra` backslashes
    /// added before the closing quote when the value ends in a backslash.
    fn quoted(value: &str, extra: usize) -> String {
        format!("\"{value}{}\"", "\\".repeat(extra))
    }

    #[test]
    fn the_cmd_shell_refuses_every_character_it_cannot_quote() {
        for refused in ['"', '%', '^', '!', '\n', '\r'] {
            let value = format!("safe{refused}value");
            let error = cmd_word(&value).unwrap_err();
            assert!(matches!(error, CoreError::Invalid(_)), "{refused:?}");
            assert!(
                !error.to_string().contains(&value),
                "{refused:?}: never the value"
            );
            // The same character alone, so the check is on the value rather than on a pattern around it.
            assert!(cmd_word(&refused.to_string()).is_err(), "{refused:?}");
        }
    }

    #[test]
    fn the_cmd_shell_writes_one_double_quoted_word_and_doubles_trailing_backslashes() {
        assert_eq!(cmd_word("plain").unwrap(), quoted("plain", 0));
        assert_eq!(cmd_word("two words").unwrap(), quoted("two words", 0));
        // cmd's own metacharacters are literal inside the one pair of double quotes.
        assert_eq!(
            cmd_word("a & b | c < d > e").unwrap(),
            quoted("a & b | c < d > e", 0)
        );
        // A single quote is ordinary text to cmd, where a naive single-quoting would break it.
        assert_eq!(cmd_word("it's").unwrap(), quoted("it's", 0));
        // A trailing backslash is doubled, or it would escape the closing quote: the case a naive quoting breaks.
        assert_eq!(
            cmd_word("C:\\Program Files\\").unwrap(),
            quoted("C:\\Program Files\\", 1)
        );
        assert_eq!(cmd_word("C:\\\\").unwrap(), quoted("C:\\\\", 2));
        // A backslash that does not touch the closing quote is left alone.
        assert_eq!(cmd_word("a\\\\b").unwrap(), quoted("a\\\\b", 0));
        // An empty optional value is no word at all (quote's contract).
        assert_eq!(cmd_word("").unwrap(), "");
    }
}
