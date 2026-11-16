//! The person key: a per-launch secret the service prints once, on the ready line this shell reads from the pipe it
//! spawned (docs/worker-token-isolation.md, #441). It is never written to disk, passed in env or argv, or put in a URL.
//! The bundled page asks for it through `person_key` and sends it as `x-orbyn-person` on the routes that need the person.
//! A worker that reads the service's `service.json` has the token and not this, so it cannot call those routes.
//!
//! Only the bundled app's main window may ask (the same check as `crew_presence`): the startup page and a preview window
//! cannot. This is not a defence against a process that can read this one's memory or open devtools in the page.

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::Core;

/// The stable strings the page can see. Neither carries the key.
const FORBIDDEN: &str = "forbidden";
const UNAVAILABLE: &str = "unavailable";

/// Hands the bundled page this launch's person key.
#[tauri::command]
pub fn person_key(app: AppHandle, window: WebviewWindow) -> Result<String, &'static str> {
    crate::presence::require_caller(&window).map_err(|_| FORBIDDEN)?;
    let key = app
        .state::<Core>()
        .0
        .lock()
        .map_err(|_| UNAVAILABLE)?
        .as_ref()
        .and_then(|core| core.person_key());
    key.ok_or(UNAVAILABLE)
}
