use orbyn_contracts::{
    EngineCapabilities, EngineStartIssue, EngineStatus, PermissionMode, ProviderGroup,
    ProviderLabel,
};
use serde_json::json;

use super::{Refused, apps_found};

fn list(statuses: &[EngineStatus], apps: &[&str]) -> orbyn_contracts::ProviderList {
    super::list(statuses, apps, &[])
}

fn caps() -> EngineCapabilities {
    EngineCapabilities {
        streaming: true,
        approvals: true,
        resume: true,
        mcp: true,
        sandbox: false,
        managed: true,
        skill_delivery: Vec::new(),
        goal_control: false,
        decline_feedback: false,
        permission_modes: PermissionMode::ALL.to_vec(),
    }
}

fn status(
    id: &str,
    name: &str,
    tier: &str,
    managed: bool,
    compatibility: Option<&str>,
) -> EngineStatus {
    let mut capabilities = caps();
    capabilities.managed = managed;
    capabilities.approvals = managed;
    serde_json::from_value(json!({
        "id": id, "name": name, "tier": tier, "capabilities": capabilities,
        "installed": true, "path": "/bin/x", "version": null, "auth": "not_applicable",
        "installer": "unknown", "latest": null, "support": "unknown",
        "compatibility": compatibility.map(|state| json!({ "status": state, "message": "Older." })),
        "update_command": null, "manual_hint": null, "sign_in_command": null,
        "updating": false, "problem": null
    }))
    .unwrap()
}

fn issue(kind: &str, message: &str) -> EngineStartIssue {
    serde_json::from_value(json!({
        "kind": kind, "message": message, "at": "2026-01-01T00:00:00Z"
    }))
    .unwrap()
}

fn row<'a>(
    list: &'a orbyn_contracts::ProviderList,
    id: &str,
) -> &'a orbyn_contracts::ProviderEntry {
    list.providers
        .iter()
        .find(|entry| entry.id == id)
        .unwrap_or_else(|| panic!("no row for {id}"))
}

#[test]
fn a_known_acp_engine_is_found_and_not_verified_until_a_real_install_ran() {
    let found = list(
        &[
            status("cursor", "Cursor", "acp", true, None),
            status("opencode", "OpenCode", "acp", true, None),
            status("claude", "Claude Code", "native", true, None),
        ],
        &[],
    );
    assert_eq!(row(&found, "cursor").group, ProviderGroup::Found);
    assert_eq!(row(&found, "cursor").labels, [ProviderLabel::NotVerified]);
    assert_eq!(
        row(&found, "cursor").note.as_deref(),
        Some("Starts over ACP. Not verified on a real install.")
    );
    for id in ["opencode", "claude"] {
        assert_eq!(row(&found, id).group, ProviderGroup::InUse, "{id}");
        assert!(row(&found, id).labels.is_empty(), "{id}");
    }
}

#[test]
fn fx_is_unmanaged_and_says_which_mode() {
    let found = list(&[status("fx", "fx", "acp", false, None)], &[]);
    let fx = row(&found, "fx");
    assert_eq!(
        fx.labels,
        [ProviderLabel::Unmanaged, ProviderLabel::NotVerified]
    );
    assert_eq!(
        fx.note.as_deref(),
        Some("Starts over ACP. Its Auto mode is described as writing without asking.")
    );
}

#[test]
fn cline_is_not_verified_and_partly_planned() {
    let found = list(&[status("cline", "Cline", "acp", true, None)], &[]);
    let cline = row(&found, "cline");
    assert_eq!(
        cline.labels,
        [ProviderLabel::NotVerified, ProviderLabel::Planned]
    );
    assert_eq!(cline.group, ProviderGroup::More);
    assert_eq!(
        cline.note.as_deref(),
        Some("cline · its effort flag is not built")
    );
}

#[test]
fn an_older_version_is_limited_support_and_a_broken_one_cannot_start() {
    let found = list(
        &[
            status("codex", "Codex", "native", true, Some("unsupported")),
            status("claude", "Claude Code", "native", true, Some("broken")),
        ],
        &[],
    );
    assert_eq!(row(&found, "codex").labels, [ProviderLabel::LimitedSupport]);
    assert_eq!(row(&found, "codex").group, ProviderGroup::InUse);
    assert_eq!(row(&found, "claude").labels, [ProviderLabel::CantStart]);
    assert_eq!(row(&found, "claude").group, ProviderGroup::CantStart);
    assert_eq!(row(&found, "claude").note.as_deref(), Some("Older."));
}

#[test]
fn an_engine_that_is_not_found_stays_listed_so_it_can_be_edited_or_removed() {
    let mut gone = status("mine", "Mine", "acp", true, None);
    gone.installed = false;
    let found = list(&[gone], &[]);
    let mine = row(&found, "mine");
    assert_eq!(mine.group, ProviderGroup::NotFound);
    assert_eq!(mine.labels, [ProviderLabel::NotFound]);
    assert!(mine.registered && !mine.openable);
}

#[test]
fn a_signed_out_start_adds_its_label_and_note_without_moving_the_group() {
    let mut cursor = status("cursor", "Cursor", "acp", true, None);
    let message = "Cursor isn't signed in. Run cursor-agent login, then try again.";
    cursor.start_issue = Some(issue("signed_out", message));
    let found = list(&[cursor], &[]);
    let cursor = row(&found, "cursor");
    assert_eq!(
        cursor.labels,
        [ProviderLabel::SignedOut, ProviderLabel::NotVerified]
    );
    assert_eq!(cursor.note.as_deref(), Some(message));
    assert_eq!(cursor.group, ProviderGroup::Found);
}

#[test]
fn a_failed_start_adds_its_label_and_note() {
    let mut codex = status("codex", "Codex", "native", true, None);
    let message = "Codex did not start: the agent did not answer its handshake.";
    codex.start_issue = Some(issue("failed", message));
    let found = list(&[codex], &[]);
    let codex = row(&found, "codex");
    assert_eq!(codex.labels, [ProviderLabel::StartFailed]);
    assert_eq!(codex.note.as_deref(), Some(message));
    assert_eq!(codex.group, ProviderGroup::InUse);
}

#[test]
fn a_not_found_engine_keeps_saying_only_that_even_with_a_leftover_issue() {
    let mut gone = status("mine", "Mine", "acp", true, None);
    gone.installed = false;
    gone.start_issue = Some(issue("failed", "Mine did not start: it is gone."));
    let found = list(&[gone], &[]);
    let mine = row(&found, "mine");
    assert_eq!(mine.labels, [ProviderLabel::NotFound]);
    assert_eq!(mine.note, None);
    assert_eq!(mine.group, ProviderGroup::NotFound);
}

#[test]
fn the_other_unverified_engines_read_as_their_program() {
    let found = list(
        &[
            status("kimi", "Kimi Code", "acp", true, None),
            status("deepseek", "DeepSeek", "acp", false, None),
        ],
        &[],
    );
    assert_eq!(row(&found, "kimi").group, ProviderGroup::More);
    assert_eq!(row(&found, "kimi").note.as_deref(), Some("kimi"));
    let deepseek = row(&found, "deepseek");
    assert_eq!(
        deepseek.labels,
        [ProviderLabel::Unmanaged, ProviderLabel::NotVerified]
    );
    assert_eq!(deepseek.note.as_deref(), Some("dsh"));
}

#[test]
fn only_beta_families_have_a_detail_page_including_named_instances() {
    let mut instance = status("my-open", "My OpenCode", "acp", true, None);
    instance.instance_of = Some("opencode".into());
    let mut other_instance = status("my-kimi", "My Kimi", "acp", true, None);
    other_instance.instance_of = Some("kimi".into());
    let found = list(
        &[
            status("cursor", "Cursor", "acp", true, None),
            status("kimi", "Kimi Code", "acp", true, None),
            status("opencode", "OpenCode", "acp", true, None),
            status("claude", "Claude", "native", true, None),
            status("codex", "Codex", "native", true, None),
            instance,
            other_instance,
        ],
        &[],
    );
    assert!(!row(&found, "cursor").openable);
    for id in ["kimi", "my-kimi"] {
        assert!(!row(&found, id).openable, "{id}");
    }
    for id in ["opencode", "claude", "codex", "my-open"] {
        assert!(row(&found, id).openable, "{id}");
    }
    assert!(!row(&found, "pi").openable);
}

#[test]
fn a_program_that_is_another_cli_is_a_card_that_says_so() {
    let refused = [Refused {
        id: "grok",
        path: "/usr/local/bin/grok".into(),
    }];
    let found = super::list(&[], &[], &refused);
    let grok = row(&found, "grok");
    assert_eq!(grok.group, ProviderGroup::NotFound);
    assert_eq!(grok.labels, [ProviderLabel::NotFound]);
    assert!(!grok.registered && !grok.openable);
    assert!(grok.note.as_deref().unwrap().starts_with(
        "Found a program named grok at /usr/local/bin/grok, but it is not xAI’s Grok CLI. Orbyn does not use it."
    ));
    // A real Grok that was found wins.
    let real = super::list(
        &[status("grok", "Grok Build", "acp", true, None)],
        &[],
        &refused,
    );
    assert_eq!(
        real.providers
            .iter()
            .filter(|entry| entry.id == "grok")
            .count(),
        1
    );
    assert!(row(&real, "grok").registered);
}

#[test]
fn an_installed_app_without_an_acp_server_cannot_start_and_a_found_program_wins() {
    let with_app = list(&[], &["antigravity"]);
    let app = row(&with_app, "antigravity");
    assert_eq!(app.group, ProviderGroup::CantStart);
    assert_eq!(app.labels, [ProviderLabel::CantStart]);
    assert!(!app.registered);
    // Its ACP program was found, so it is an engine and not an app.
    let both = list(
        &[status("antigravity", "Antigravity", "acp", true, None)],
        &["antigravity"],
    );
    assert_eq!(
        both.providers
            .iter()
            .filter(|entry| entry.id == "antigravity")
            .count(),
        1
    );
    assert_eq!(row(&both, "antigravity").group, ProviderGroup::More);
    assert!(
        list(&[], &[])
            .providers
            .iter()
            .all(|entry| entry.id != "antigravity")
    );
}

#[test]
fn the_apps_are_looked_for_in_the_given_folders_only() {
    let folder = std::env::temp_dir().join(format!("orbyn-apps-{}", std::process::id()));
    std::fs::create_dir_all(folder.join("Antigravity.app")).unwrap();
    assert_eq!(apps_found(std::slice::from_ref(&folder)), ["antigravity"]);
    assert!(apps_found(&[folder.join("elsewhere")]).is_empty());
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn engines_with_no_driver_are_planned_and_last() {
    let found = list(
        &[status("claude", "Claude Code", "native", true, None)],
        &["antigravity"],
    );
    let tail: Vec<&str> = found
        .providers
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(tail, ["claude", "antigravity", "pi", "muse", "zcode"]);
    let pi = row(&found, "pi");
    assert_eq!(pi.group, ProviderGroup::Planned);
    assert_eq!(pi.labels, [ProviderLabel::Planned]);
    assert!(!pi.registered);
    assert_eq!(row(&found, "muse").name, "Muse Code");
}
