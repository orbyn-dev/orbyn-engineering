//! The native folder dialog for the add-repository field. The shell only shows it and hands the chosen path back as
//! text, or `null` on cancel. It reads nothing in the folder and grants nothing: the page fills a text field and the
//! repository is added untrusted through the Person route as usual.
//!
//! Only the bundled app's main window may ask (the same check as `crew_presence`). The dialog plugin is used from Rust
//! only: no capability hands its commands to a page, so this command is the only way a page opens a folder dialog.

use std::path::Path;
use std::sync::mpsc;

use tauri::{AppHandle, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

/// The stable strings the page can see.
const FORBIDDEN: &str = "forbidden";
const UNAVAILABLE: &str = "unavailable";

/// The path as text, or `None` when the dialog was cancelled.
fn chosen_text(path: Option<&Path>) -> Option<String> {
    path.map(|path| path.to_string_lossy().into_owned())
}

/// Opens the folder dialog and returns the chosen path, or `None` on cancel.
#[tauri::command]
pub async fn pick_folder(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<Option<String>, &'static str> {
    crate::presence::require_caller(&window).map_err(|_| FORBIDDEN)?;
    // The dialog opens through its callback form, so no thread waits inside the event loop; the single answer comes
    // back over a one-shot channel and the wait is off the async runtime's own threads.
    let (answer, reply) = mpsc::sync_channel(1);
    app.dialog()
        .file()
        .set_title("Choose a repository folder")
        .pick_folder(move |folder| {
            let _ = answer.send(folder.and_then(|folder| folder.into_path().ok()));
        });
    let folder = tauri::async_runtime::spawn_blocking(move || reply.recv())
        .await
        .map_err(|_| UNAVAILABLE)?
        .map_err(|_| UNAVAILABLE)?;
    Ok(chosen_text(folder.as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_is_the_path_text_or_null() {
        let path = std::env::temp_dir().join("repo");
        assert_eq!(
            chosen_text(Some(&path)),
            Some(path.to_string_lossy().into_owned())
        );
        assert_eq!(chosen_text(None), None);
        assert_eq!(
            serde_json::to_value(chosen_text(None)).unwrap(),
            serde_json::Value::Null
        );
    }
}
