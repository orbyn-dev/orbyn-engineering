//! What an engine reads of an attached file. A text file is read here and the engine gets a
//! masked copy under the run's own folder; the person's upload is never rewritten.

use super::*;
use std::io::Read;

/// The largest attached text file that is masked, in bytes. A larger file is passed as it is.
pub(super) const CAP: usize = 1024 * 1024;

/// `Some(text)` when the bytes are a text file: valid UTF-8 with no NUL byte.
fn as_text(bytes: &[u8]) -> Option<&str> {
    if bytes.contains(&0) {
        return None;
    }
    std::str::from_utf8(bytes).ok()
}

impl RunManager {
    /// The paths the engine is told for a message's attached files. A text file within the cap
    /// whose masked form differs is written to `<run_dir>/attachments/<n>/<file name>` and that
    /// path is given instead. Anything else keeps its own path: a file over the cap, a binary or
    /// an image, one that cannot be read here, and a text file with nothing to hide. Fails when
    /// the settings cannot be read, so no file goes out unmasked.
    pub(super) fn masked_attachment_copies(
        &self,
        project_id: &str,
        files: &[PathBuf],
        run_dir: &Path,
    ) -> Result<Vec<PathBuf>> {
        if files.is_empty() {
            return Ok(Vec::new());
        }
        let redactor = crate::redaction::redactor_from(
            &self.store,
            crate::redaction::Surface::Agents,
            Some(project_id),
        )?;
        let mut told = Vec::with_capacity(files.len());
        for (index, file) in files.iter().enumerate() {
            let mut bytes = Vec::new();
            let read = std::fs::File::open(file)
                .and_then(|f| f.take(CAP as u64 + 1).read_to_end(&mut bytes));
            let masked = match (&read, as_text(&bytes)) {
                (Ok(_), Some(text)) if bytes.len() <= CAP => {
                    Some(redactor.labelled(text)).filter(|masked| masked != text)
                }
                _ => None,
            };
            let Some(masked) = masked else {
                told.push(file.clone());
                continue;
            };
            let dir = run_dir.join("attachments").join(index.to_string());
            std::fs::create_dir_all(&dir)?;
            let copy = dir.join(file.file_name().unwrap_or_default());
            std::fs::write(&copy, masked)?;
            told.push(copy);
        }
        Ok(told)
    }
}
