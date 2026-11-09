use std::collections::HashMap;

use serde_json::Value;

/// Bounds on a terminal sign-in an agent names: arguments, and characters in each.
pub(super) const MAX_SIGN_IN_ARGS: usize = 16;
const MAX_SIGN_IN_WORD: usize = 256;

/// The terminal sign-in each ACP engine last named in its handshake, by engine id.
/// Orbyn only shows it for the person to run; it never runs it or `authenticate`.
static SIGN_IN: std::sync::LazyLock<std::sync::Mutex<HashMap<String, String>>> =
    std::sync::LazyLock::new(Default::default);

/// The sign-in command an ACP engine named, if one has run its handshake here.
pub fn known_sign_in(id: &str) -> Option<String> {
    SIGN_IN.lock().ok()?.get(id).cloned()
}

/// Which engine a handshake belongs to and the command that launched it.
#[derive(Clone)]
pub(super) struct SignInSlot {
    pub(super) id: String,
    pub(super) launch: Vec<String>,
}

impl SignInSlot {
    pub(super) fn record(&self, init: &Value) {
        let Ok(mut known) = SIGN_IN.lock() else {
            return;
        };
        match terminal_sign_in(&self.launch, init) {
            Some(command) => known.insert(self.id.clone(), command),
            None => known.remove(&self.id),
        };
    }
}

/// The first `terminal` auth method an agent offers, as one command line: the launch
/// command followed by the method's arguments. A method that sets environment
/// variables, or a word that would need shell-specific quoting, gives none, because
/// the line has to mean the same in a Windows and a Unix terminal.
pub(super) fn terminal_sign_in(launch: &[String], init: &Value) -> Option<String> {
    let method = init
        .get("authMethods")?
        .as_array()?
        .iter()
        .take(MAX_SIGN_IN_ARGS)
        .find(|method| method.get("type").and_then(Value::as_str) == Some("terminal"))?;
    match method.get("env") {
        None | Some(Value::Null) => {}
        Some(Value::Object(env)) if env.is_empty() => {}
        Some(_) => return None,
    }
    let args: Vec<&str> = match method.get("args") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(args)) if args.len() <= MAX_SIGN_IN_ARGS => {
            args.iter().map(Value::as_str).collect::<Option<_>>()?
        }
        Some(_) => return None,
    };
    let words = launch
        .iter()
        .map(String::as_str)
        .chain(args)
        .map(shell_word)
        .collect::<Option<Vec<_>>>()?;
    (!words.is_empty()).then(|| words.join(" "))
}

/// One word of a command line, quoted when it holds spaces, or none when it holds a
/// character a Windows or Unix shell would read differently.
fn shell_word(word: &str) -> Option<String> {
    if word.is_empty()
        || word.chars().count() > MAX_SIGN_IN_WORD
        || word
            .chars()
            .any(|c| c.is_control() || "\"'`$%!^&|<>;(){}[]*?~#".contains(c))
    {
        return None;
    }
    Some(if word.contains(' ') {
        format!("\"{word}\"")
    } else {
        word.to_owned()
    })
}
