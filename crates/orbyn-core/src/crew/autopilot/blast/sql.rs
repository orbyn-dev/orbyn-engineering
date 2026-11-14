//! Reading SQL (and the Redis and Mongo shell words) for what destroys data: comments and strings are taken apart before any keyword
//! is looked for, and where that cannot be done with certainty the keywords count wherever they appear.

use std::sync::OnceLock;

use regex::Regex;

/// SQL as it is read: comments gone, case folded. `blanked` has every quoted string and identifier replaced by a placeholder, so a
/// word inside a string is not a keyword; `plain` keeps them. `sure` is false where the reader cannot be certain what is code
/// (an open quote or comment, a dollar-quoted body, a backslash): the caller then falls back to looking for the keywords anywhere.
struct Sql {
    blanked: String,
    plain: String,
    sure: bool,
}

fn normalise_sql(text: &str) -> Sql {
    let mut blanked = String::new();
    let mut plain = String::new();
    let mut sure = true;
    let chars: Vec<char> = text.chars().collect();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        let next = chars.get(at + 1).copied();
        match c {
            '-' if next == Some('-') => {
                while at < chars.len() && chars[at] != '\n' {
                    at += 1;
                }
                blanked.push(' ');
                plain.push(' ');
            }
            '/' if next == Some('*') => {
                at += 2;
                let mut closed = false;
                while at < chars.len() {
                    if chars[at] == '*' && chars.get(at + 1) == Some(&'/') {
                        at += 2;
                        closed = true;
                        break;
                    }
                    at += 1;
                }
                sure &= closed;
                blanked.push(' ');
                plain.push(' ');
            }
            '\'' | '"' => {
                let mut body = String::new();
                at += 1;
                let mut closed = false;
                while at < chars.len() {
                    if chars[at] == c {
                        if chars.get(at + 1) == Some(&c) {
                            body.push(c);
                            at += 2;
                            continue;
                        }
                        at += 1;
                        closed = true;
                        break;
                    }
                    body.push(chars[at]);
                    at += 1;
                }
                sure &= closed;
                blanked.push_str(if c == '\'' { " 's' " } else { " \"i\" " });
                plain.push(' ');
                plain.extend(body.chars().flat_map(char::to_lowercase));
                plain.push(' ');
            }
            '$' if next == Some('$') || next.is_some_and(|n| n.is_alphabetic() || n == '_') => {
                // A dollar-quoted body: its text is code or a string, which is not told apart here.
                sure = false;
                let lowered = c.to_lowercase();
                blanked.extend(lowered.clone());
                plain.extend(lowered);
                at += 1;
            }
            '\\' => {
                sure = false;
                blanked.push(' ');
                plain.push(' ');
                at += 1;
            }
            _ => {
                let lowered: String = c.to_lowercase().collect();
                blanked.push_str(&lowered);
                plain.push_str(&lowered);
                at += 1;
            }
        }
    }
    Sql {
        blanked,
        plain,
        sure,
    }
}

fn words(text: &str) -> Vec<&str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .collect()
}

/// DROP, TRUNCATE, DELETE (with or without FROM) or an UPDATE with no WHERE in code, statement by statement. `ON DELETE` and
/// `ON UPDATE` (foreign-key actions) and `FOR UPDATE` are not those.
fn sql_in_code(blanked: &str) -> Option<&'static str> {
    for statement in blanked.split(';') {
        let tokens = words(statement);
        for (at, token) in tokens.iter().enumerate() {
            let before = at.checked_sub(1).map(|i| tokens[i]);
            match *token {
                "drop" => return Some("DROP"),
                "truncate" => return Some("TRUNCATE"),
                "delete" if before != Some("on") => return Some("DELETE"),
                "update" if before != Some("on") && before != Some("for") => {
                    let rest = &tokens[at + 1..];
                    if rest.contains(&"set") && !rest.contains(&"where") {
                        return Some("UPDATE without WHERE");
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// The fallback where the reader is not sure: any of the keywords anywhere, quoted or not.
fn sql_keyword(plain: &str) -> Option<&'static str> {
    let tokens = words(plain);
    for (at, token) in tokens.iter().enumerate() {
        let before = at.checked_sub(1).map(|i| tokens[i]);
        match *token {
            "drop" => return Some("DROP"),
            "truncate" => return Some("TRUNCATE"),
            "alter" => return Some("ALTER"),
            "delete" if before != Some("on") => return Some("DELETE"),
            "update" if before != Some("on") && before != Some("for") => return Some("UPDATE"),
            _ => {}
        }
    }
    None
}

fn client_patterns() -> &'static [(Regex, &'static str)] {
    static RE: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    RE.get_or_init(|| {
        [
            (r"\bflush(?:all|db)\b", "FLUSHALL"),
            (r"\bdropdatabase\b", "dropDatabase"),
            (r"\.drop\s*\(", "drop()"),
            (r"\bdeletemany\s*\(", "deleteMany"),
            (r"\.remove\s*\(", "remove()"),
        ]
        .into_iter()
        .map(|(pattern, name)| (Regex::new(pattern).expect("a fixed pattern"), name))
        .collect()
    })
}

/// The destructive word a query holds, if any. `anywhere` is the whole command text (a pipe or here-doc feeds the client SQL the
/// parser cannot attribute): there the keywords count wherever they are.
pub(super) fn destructive_sql(text: &str, anywhere: bool) -> Option<&'static str> {
    let sql = normalise_sql(text);
    if let Some((_, name)) = client_patterns()
        .iter()
        .find(|(re, _)| re.is_match(&sql.plain))
    {
        return Some(name);
    }
    if anywhere || !sql.sure {
        return sql_in_code(&sql.blanked).or_else(|| sql_keyword(&sql.plain));
    }
    sql_in_code(&sql.blanked)
}
