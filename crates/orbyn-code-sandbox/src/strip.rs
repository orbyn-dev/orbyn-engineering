//! TypeScript to JavaScript: types are stripped and nothing is checked.

use crate::{ErrorKind, RunError};
use deno_ast::{TypeStripOptions, type_strip};
use url::Url;

/// The script as the body of `__orbyn_main`, on the same lines as the source.
pub(crate) fn to_js(source: &str) -> Result<String, RunError> {
    let wrapped = format!("async function __orbyn_main() {{{source}\n}}");
    let url = Url::parse("file:///script.ts").expect("static url");
    type_strip(&url, wrapped, TypeStripOptions { module: Some(true) }).map_err(|error| RunError {
        kind: ErrorKind::Syntax,
        message: error.to_string(),
        line: None,
        code: None,
    })
}
