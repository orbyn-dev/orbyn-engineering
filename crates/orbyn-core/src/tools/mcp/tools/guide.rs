//! The pull request guide tools: checking a guide's head and stops before it is written.

use crate::error::{CoreError, Result};
use serde_json::{Value, json};

/// A review guide's bounds. The stored stops stay well under the 256 KiB reply cap,
/// so reading a guide back always fits.
pub(super) const MAX_GUIDE_STOPS: usize = 30;

pub(super) const MAX_STOP_TITLE_CHARS: usize = 200;

pub(super) const MAX_STOP_SUMMARY_CHARS: usize = 4000;

pub(super) const MAX_STOP_FILES: usize = 50;

pub(super) const MAX_GUIDE_PATH_BYTES: usize = 1024;

pub(super) const MAX_FILE_NOTE_CHARS: usize = 1000;

pub(super) const MAX_STOP_QUESTIONS: usize = 10;

pub(super) const MAX_QUESTION_CHARS: usize = 500;

pub(super) const MAX_GUIDE_BYTES: usize = 128 * 1024;

/// An optional commit id: 7 to 64 hexadecimal characters, lowercased.
pub(super) fn guide_head(args: &Value) -> Result<Option<String>> {
    match args.get("head_sha") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(sha))
            if (7..=64).contains(&sha.trim().len())
                && sha.trim().chars().all(|c| c.is_ascii_hexdigit()) =>
        {
            Ok(Some(sha.trim().to_ascii_lowercase()))
        }
        Some(_) => Err(CoreError::Invalid(
            "head_sha is a commit id of 7 to 64 hexadecimal characters.".into(),
        )),
    }
}

pub(super) fn guide_text(
    value: Option<&Value>,
    what: &str,
    max_chars: usize,
    required: bool,
) -> Result<String> {
    let text = match value {
        None | Some(Value::Null) if !required => "",
        Some(Value::String(text)) => text.trim(),
        _ => return Err(CoreError::Invalid(format!("{what} must be text."))),
    };
    if required && text.is_empty() {
        return Err(CoreError::Invalid(format!("{what} is required.")));
    }
    if text.chars().count() > max_chars || text.contains('\0') {
        return Err(CoreError::Invalid(format!(
            "{what} is at most {max_chars} characters and cannot contain NUL."
        )));
    }
    Ok(text.to_owned())
}

pub(super) fn guide_list<'a>(
    value: Option<&'a Value>,
    what: &str,
    max: usize,
) -> Result<&'a [Value]> {
    match value {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(items)) if items.len() <= max => Ok(items),
        Some(Value::Array(_)) => Err(CoreError::Invalid(format!("{what} holds at most {max}."))),
        Some(_) => Err(CoreError::Invalid(format!("{what} must be a list."))),
    }
}

/// `orbyn_pull_guide_write`'s stops, checked and rebuilt with only the known
/// fields, so what is stored is exactly what the guide shape allows. A file path
/// is display text for the reader; nothing opens it.
/// Answers the stored JSON and the number of stops.
pub(super) fn guide_stops(args: &Value) -> Result<(String, usize)> {
    let stops = match args.get("stops") {
        Some(Value::Array(stops)) if !stops.is_empty() => stops,
        _ => {
            return Err(CoreError::Invalid(
                "stops must list at least one stop.".into(),
            ));
        }
    };
    if stops.len() > MAX_GUIDE_STOPS {
        return Err(CoreError::Invalid(format!(
            "A guide holds at most {MAX_GUIDE_STOPS} stops."
        )));
    }
    let mut built = Vec::with_capacity(stops.len());
    for stop in stops {
        if !stop.is_object() {
            return Err(CoreError::Invalid("Each stop must be an object.".into()));
        }
        let mut files = Vec::new();
        for file in guide_list(stop.get("files"), "files", MAX_STOP_FILES)? {
            let path = guide_text(
                file.get("path"),
                "A file's path",
                MAX_GUIDE_PATH_BYTES,
                true,
            )?;
            if path.len() > MAX_GUIDE_PATH_BYTES {
                return Err(CoreError::Invalid(format!(
                    "A file's path is at most {MAX_GUIDE_PATH_BYTES} bytes."
                )));
            }
            let note = guide_text(
                file.get("note"),
                "A file's note",
                MAX_FILE_NOTE_CHARS,
                false,
            )?;
            files.push(json!({ "path": path, "note": note }));
        }
        let mut questions = Vec::new();
        for question in guide_list(stop.get("questions"), "questions", MAX_STOP_QUESTIONS)? {
            questions.push(guide_text(
                Some(question),
                "A question",
                MAX_QUESTION_CHARS,
                true,
            )?);
        }
        built.push(json!({
            "title": guide_text(stop.get("title"), "A stop's title", MAX_STOP_TITLE_CHARS, true)?,
            "summary": guide_text(stop.get("summary"), "A stop's summary", MAX_STOP_SUMMARY_CHARS, false)?,
            "files": files,
            "questions": questions,
        }));
    }
    let count = built.len();
    let stored = Value::Array(built).to_string();
    if stored.len() > MAX_GUIDE_BYTES {
        return Err(CoreError::Invalid(format!(
            "A guide is at most {MAX_GUIDE_BYTES} bytes."
        )));
    }
    Ok((stored, count))
}
