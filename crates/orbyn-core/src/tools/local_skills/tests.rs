use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::*;
use crate::testing::TempDir;

const SKILL: &str = "---
name: redirect-checks
description: \"Check where a redirect or returnTo link goes before changing code that follows it.\"
---

# Redirect checks

Use when a task changes how the app follows a redirect.
";

fn fixture() -> (TempDir, Arc<Store>, LocalSkills) {
    let dir = TempDir::new();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let store = Arc::new(Store::open(&data.join("orbyn.sqlite3"), "epoch").unwrap());
    let local = LocalSkills::new(&data, store.clone());
    (dir, store, local)
}

fn with_name(name: &str) -> String {
    format!("---\nname: {name}\ndescription: \"A skill.\"\n---\n\n# {name}\n")
}

fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(root: &Path, at: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in std::fs::read_dir(at).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            let file_type = entry.file_type().unwrap();
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                walk(root, &path, out);
            } else {
                out.push((relative, std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut files = Vec::new();
    if root.exists() {
        walk(root, root, &mut files);
    }
    files.sort();
    files
}

fn audits(store: &Store) -> Vec<orbyn_contracts::AuditEvent> {
    store
        .list_audit(None, 100)
        .unwrap()
        .into_iter()
        .filter(|row| row.action == AUDIT_ACTION)
        .collect()
}

fn refusal(error: CoreError, code: &'static str, status: u16) {
    assert_eq!(error.code(), code, "{error}");
    assert_eq!(error.status(), status, "{error}");
}

#[test]
fn a_written_skill_is_one_folder_with_a_skill_file_and_its_revision() {
    let (_dir, _store, local) = fixture();
    let skill = local.write(LocalSkillWrite::create(SKILL), &[]).unwrap();
    assert_eq!(skill.name, "redirect-checks");
    assert_eq!(skill.local_revision, 1);
    assert!(skill.description.starts_with("Check where a redirect"));
    let folder = local.root().join("redirect-checks");
    assert_eq!(
        std::fs::read_dir(&folder).unwrap().count(),
        1,
        "only SKILL.md"
    );
    let text = std::fs::read_to_string(folder.join("SKILL.md")).unwrap();
    assert_eq!(
        crate::tools::skills::parse_frontmatter(&text),
        Some((
            "redirect-checks".to_owned(),
            "Check where a redirect or returnTo link goes before changing code that follows it."
                .to_owned()
        ))
    );
    assert!(text.contains("local_revision: 1"), "{text}");
    assert!(text.ends_with(
        "# Redirect checks\n\nUse when a task changes how the app follows a redirect.\n"
    ));
    assert_eq!(local.list().unwrap(), vec![skill.clone()]);
    assert!(local.contains("redirect-checks"));
    assert_eq!(local.read("redirect-checks", None).unwrap(), text);
}

#[test]
fn front_matter_keeps_its_other_values_and_gets_one_revision_line() {
    let (_dir, _store, local) = fixture();
    let text =
        "---\nname: one\ndescription: \"a: b\"\nlocal_revision: 9\n---\nbody: with a colon\n";
    let skill = local.write(LocalSkillWrite::create(text), &[]).unwrap();
    assert_eq!(skill.local_revision, 1);
    let stored = local.read("one", None).unwrap();
    assert_eq!(stored.matches("local_revision:").count(), 1, "{stored}");
    assert!(stored.contains("description: \"a: b\""), "{stored}");
    assert!(stored.contains("local_revision: 1"), "{stored}");
    assert!(stored.ends_with("---\nbody: with a colon\n"), "{stored}");
}

#[test]
fn the_audit_row_carries_the_name_and_ids_and_never_the_text() {
    let (_dir, store, local) = fixture();
    local
        .write(
            LocalSkillWrite::create(SKILL)
                .from_proposal("prop_abc", vec!["les_a".into(), "les_b".into()]),
            &[],
        )
        .unwrap();
    let rows = audits(&store);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].target, "redirect-checks");
    assert_eq!(rows[0].actor, "user");
    assert_eq!(rows[0].decision, None);
    assert_eq!(rows[0].detail["name"], "redirect-checks");
    assert_eq!(rows[0].detail["local_revision"], 1);
    assert_eq!(rows[0].detail["created"], true);
    assert_eq!(rows[0].detail["proposal"], "prop_abc");
    assert_eq!(rows[0].detail["lessons"], json!(["les_a", "les_b"]));
    let shown = serde_json::to_string(&rows[0].detail).unwrap();
    assert!(!shown.contains("Redirect checks"), "{shown}");
    assert!(!shown.contains("Use when a task changes"), "{shown}");

    local.write(LocalSkillWrite::edit(SKILL, 1), &[]).unwrap();
    let rows = audits(&store);
    assert_eq!(rows.len(), 2);
    // Newest first: the edit's row, then the create's.
    assert_eq!(rows[0].detail["created"], false);
    assert_eq!(rows[0].detail["local_revision"], 2);
    assert_eq!(rows[1].detail["created"], true);
}

#[test]
fn a_name_that_already_exists_is_refused_and_the_skill_is_unchanged() {
    let (_dir, store, local) = fixture();
    local.write(LocalSkillWrite::create(SKILL), &[]).unwrap();
    let before = snapshot(local.root());
    let error = local
        .write(LocalSkillWrite::create(SKILL), &[])
        .unwrap_err();
    refusal(error, "name_taken", 409);
    assert_eq!(snapshot(local.root()), before);
    assert_eq!(audits(&store).len(), 1);
}

#[test]
fn an_edit_needs_the_revision_it_read_and_a_stale_one_is_refused() {
    let (_dir, store, local) = fixture();
    local.write(LocalSkillWrite::create(SKILL), &[]).unwrap();
    let edited = SKILL.replace("follows a redirect.", "follows a redirect now.");
    let skill = local
        .write(LocalSkillWrite::edit(edited.clone(), 1), &[])
        .unwrap();
    assert_eq!(skill.local_revision, 2);
    let before = snapshot(local.root());
    let error = local
        .write(LocalSkillWrite::edit(SKILL, 1), &[])
        .unwrap_err();
    refusal(error, "stale", 409);
    assert_eq!(snapshot(local.root()), before);
    assert!(
        local
            .read("redirect-checks", None)
            .unwrap()
            .contains("follows a redirect now.")
    );
    assert_eq!(audits(&store).len(), 2);
    let skill = local.write(LocalSkillWrite::edit(edited, 2), &[]).unwrap();
    assert_eq!(skill.local_revision, 3);
}

#[test]
fn an_edit_cannot_rename_a_local_skill() {
    let (_dir, store, local) = fixture();
    local.write(LocalSkillWrite::create(SKILL), &[]).unwrap();
    let before = snapshot(local.root());
    let error = local
        .write(LocalSkillWrite::edit(with_name("another-name"), 1), &[])
        .unwrap_err();
    assert_eq!(error.code(), "not_found", "{error}");
    assert_eq!(snapshot(local.root()), before);
    assert_eq!(audits(&store).len(), 1);
}

#[test]
fn two_edits_from_one_revision_leave_one_winner_and_no_lost_text() {
    let (_dir, _store, local) = fixture();
    let local = Arc::new(local);
    local.write(LocalSkillWrite::create(SKILL), &[]).unwrap();
    let one = Arc::clone(&local);
    let two = Arc::clone(&local);
    let (left, right) = std::thread::scope(|scope| {
        let left = scope.spawn(move || {
            one.write(
                LocalSkillWrite::edit(SKILL.replace("follows a redirect.", "one."), 1),
                &[],
            )
        });
        let right = scope.spawn(move || {
            two.write(
                LocalSkillWrite::edit(SKILL.replace("follows a redirect.", "two."), 1),
                &[],
            )
        });
        (left.join().unwrap(), right.join().unwrap())
    });
    let (left_ok, right_ok) = (left.is_ok(), right.is_ok());
    assert_eq!(usize::from(left_ok) + usize::from(right_ok), 1);
    let loser = left.err().or(right.err()).unwrap();
    refusal(loser, "stale", 409);
    let text = local.read("redirect-checks", None).unwrap();
    if left_ok {
        assert!(text.contains("how the app one."), "{text}");
        assert!(!text.contains("how the app two."));
    } else {
        assert!(text.contains("how the app two."), "{text}");
        assert!(!text.contains("how the app one."));
    }
    assert_eq!(local.get("redirect-checks").unwrap().local_revision, 2);
}

#[test]
fn a_name_in_the_shared_library_is_refused_before_anything_is_written() {
    let (_dir, store, local) = fixture();
    let before = snapshot(local.root());
    let error = local
        .write(
            LocalSkillWrite::create(with_name("how")),
            &["how".to_owned()],
        )
        .unwrap_err();
    refusal(error, "name_taken", 409);
    assert!(!local.root().join("how").exists());
    assert_eq!(snapshot(local.root()), before);
    assert!(audits(&store).is_empty());
}

#[test]
fn a_secret_is_refused_with_the_contracts_words_and_nothing_is_written() {
    let (_dir, store, local) = fixture();
    let secret = "ghp_0123456789abcdefghijklmnopqrstuvwxyz";
    let text = with_name("leaky").replace("# leaky", &format!("# leaky\n\nTOKEN: {secret}"));
    let error = local.write(LocalSkillWrite::create(text), &[]).unwrap_err();
    assert_eq!(
        error.public_message(),
        "Not saved: this looks like it holds a secret. Edit it and save again."
    );
    refusal(error, "secret_refused", 400);
    assert!(!local.root().join("leaky").exists());
    assert!(audits(&store).is_empty());
}

#[test]
fn a_secret_in_the_description_is_refused_too() {
    let (_dir, _store, local) = fixture();
    let text = format!(
        "---\nname: leaky\ndescription: \"{}\"\n---\n",
        "ghp_0123456789abcdefghijklmnopqrstuvwxyz"
    );
    let error = local.write(LocalSkillWrite::create(text), &[]).unwrap_err();
    refusal(error, "secret_refused", 400);
}

#[test]
fn a_skill_over_64_kib_is_refused() {
    let (_dir, store, local) = fixture();
    let text = with_name("big");
    let text = format!("{text}{}", "x".repeat(MAX_SKILL_BYTES));
    assert!(text.len() > MAX_SKILL_BYTES);
    let error = local.write(LocalSkillWrite::create(text), &[]).unwrap_err();
    refusal(error, "invalid", 400);
    assert!(!local.root().join("big").exists());
    assert!(audits(&store).is_empty());
}

#[test]
fn a_skill_needs_front_matter_with_a_name() {
    for text in [
        "no front matter at all",
        "---\ndescription: \"no name\"\n---\nbody",
        "---\nname:\n---\nbody",
    ] {
        let (_dir, _store, local) = fixture();
        let error = local.write(LocalSkillWrite::create(text), &[]).unwrap_err();
        refusal(error, "invalid", 400);
        assert_eq!(local.list().unwrap(), Vec::new());
    }
}

#[test]
fn only_slug_names_are_used() {
    let mut bad: Vec<String> = [
        "UPPER",
        "../escape",
        "a/b",
        "with space",
        "-leading",
        "trailing-",
        "two--hyphens",
        "con",
        "lpt1",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    bad.push("x".repeat(MAX_NAME_CHARS + 1));
    for name in bad {
        let (_dir, _store, local) = fixture();
        let error = local
            .write(LocalSkillWrite::create(with_name(&name)), &[])
            .unwrap_err();
        refusal(error, "invalid", 400);
        assert_eq!(local.list().unwrap(), Vec::new(), "{name}");
    }
    let (_dir, _store, local) = fixture();
    for name in ["a", "a1", "check-1"] {
        let skill = local
            .write(LocalSkillWrite::create(with_name(name)), &[])
            .unwrap();
        assert_eq!(skill.name, name);
    }
    let longest = "y".repeat(MAX_NAME_CHARS);
    local
        .write(LocalSkillWrite::create(with_name(&longest)), &[])
        .unwrap();
}

#[test]
fn reading_and_listing_cannot_escape_the_area() {
    let (dir, _store, local) = fixture();
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("SKILL.md"), "---\nname: escape\n---\n").unwrap();
    local.write(LocalSkillWrite::create(SKILL), &[]).unwrap();
    let folder = local.root().join("redirect-checks");
    std::fs::write(folder.join("notes.md"), "a reference\n").unwrap();
    assert_eq!(
        local.read("redirect-checks", Some("notes.md")).unwrap(),
        "a reference\n"
    );

    for file in [
        "../SKILL.md",
        "../../data/orbyn.sqlite3",
        "/etc/hosts",
        "notes/../../x",
    ] {
        assert!(local.read("redirect-checks", Some(file)).is_err(), "{file}");
    }
    assert!(local.read("../redirect-checks", None).is_err());
    assert!(
        local
            .read("redirect-checks/../redirect-checks", None)
            .is_err()
    );
    assert!(
        local
            .read("redirect-checks", Some("../redirect-checks/SKILL.md"))
            .is_err()
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(outside.join("SKILL.md"), folder.join("linked.md")).unwrap();
        assert!(local.read("redirect-checks", Some("linked.md")).is_err());
        let linked = local.root().join("escape");
        symlink(&outside, &linked).unwrap();
        assert!(local.get("escape").is_err());
        assert!(
            local
                .list()
                .unwrap()
                .iter()
                .all(|skill| skill.name != "escape")
        );
        assert!(!local.contains("escape"));
    }
}

#[test]
fn a_damaged_entry_is_left_out_of_the_listing_rather_than_breaking_it() {
    let (_dir, _store, local) = fixture();
    local.write(LocalSkillWrite::create(SKILL), &[]).unwrap();
    let root = local.root();
    let partial = root.join("partial");
    std::fs::create_dir_all(&partial).unwrap();
    let invalid_name = root.join("Not-A-Slug");
    std::fs::create_dir_all(&invalid_name).unwrap();
    std::fs::write(invalid_name.join("SKILL.md"), with_name("not-a-slug")).unwrap();
    let mismatch = root.join("mismatch");
    std::fs::create_dir_all(&mismatch).unwrap();
    std::fs::write(mismatch.join("SKILL.md"), with_name("other")).unwrap();
    std::fs::write(root.join("loose.txt"), "not a skill").unwrap();
    let listed: Vec<String> = local
        .list()
        .unwrap()
        .into_iter()
        .map(|skill| skill.name)
        .collect();
    assert_eq!(listed, ["redirect-checks"]);
}

#[test]
fn a_refused_write_leaves_the_area_byte_for_byte() {
    let (_dir, store, local) = fixture();
    local.write(LocalSkillWrite::create(SKILL), &[]).unwrap();
    let before = snapshot(local.root());
    let rows = audits(&store).len();
    let huge = format!("{}{}", with_name("big"), "x".repeat(MAX_SKILL_BYTES));
    let attempts = [
        (LocalSkillWrite::create(SKILL), Vec::new()),
        (
            LocalSkillWrite::create(with_name("how")),
            vec!["how".to_owned()],
        ),
        (LocalSkillWrite::create(huge), Vec::new()),
        (LocalSkillWrite::create(with_name("con")), Vec::new()),
        (LocalSkillWrite::edit(SKILL, 0), Vec::new()),
        (LocalSkillWrite::edit(with_name("other"), 1), Vec::new()),
    ];
    for (index, (attempt, reserved)) in attempts.into_iter().enumerate() {
        assert!(local.write(attempt, &reserved).is_err(), "{index}");
    }
    assert_eq!(snapshot(local.root()), before);
    assert_eq!(audits(&store).len(), rows);
}

#[test]
fn writing_never_touches_a_target_repository_or_the_shared_library() {
    let (dir, _store, local) = fixture();
    let library = dir.path().join("library");
    std::fs::create_dir_all(library.join("skills/how")).unwrap();
    std::fs::write(library.join("AGENTS.md"), "shared policy\n").unwrap();
    std::fs::write(
        library.join("skills/how/SKILL.md"),
        "---\nname: how\ndescription: \"Trace behavior\"\n---\n",
    )
    .unwrap();
    let target = dir.path().join("target");
    std::fs::create_dir_all(target.join("src")).unwrap();
    std::fs::write(target.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(target.join("AGENTS.md"), "target rules\n").unwrap();
    let before_library = snapshot(&library);
    let before_target = snapshot(&target);
    local.write(LocalSkillWrite::create(SKILL), &[]).unwrap();
    local.write(LocalSkillWrite::edit(SKILL, 1), &[]).unwrap();
    assert_eq!(snapshot(&library), before_library);
    assert_eq!(snapshot(&target), before_target);
}

#[test]
fn names_that_read_back_are_the_ones_listed_and_sorted() {
    let (_dir, _store, local) = fixture();
    for name in ["zeta", "alpha", "mike"] {
        local
            .write(LocalSkillWrite::create(with_name(name)), &[])
            .unwrap();
    }
    let listed: Vec<String> = local
        .list()
        .unwrap()
        .into_iter()
        .map(|skill| skill.name)
        .collect();
    assert_eq!(listed, ["alpha", "mike", "zeta"]);
}

#[test]
fn a_staged_file_is_removed_when_the_write_does_not_publish() {
    let (dir, _store, _local) = fixture();
    let area = dir.path().join("area");
    std::fs::create_dir_all(&area).unwrap();
    let staged = Staging::stage(&area, b"half a file").unwrap();
    assert_eq!(std::fs::read_dir(&area).unwrap().count(), 1);
    drop(staged);
    assert_eq!(std::fs::read_dir(&area).unwrap().count(), 0);
}

#[test]
fn publishing_one_staged_file_replaces_the_destination_whole() {
    let (dir, _store, _local) = fixture();
    let area = dir.path().join("area");
    std::fs::create_dir_all(&area).unwrap();
    let target = area.join("SKILL.md");
    std::fs::write(&target, b"old").unwrap();
    let staged = Staging::stage(&area, b"new").unwrap();
    // Before the rename a reader sees the old file whole; the staged one sits beside it.
    assert_eq!(std::fs::read(&target).unwrap(), b"old".to_vec());
    assert_eq!(std::fs::read_dir(&area).unwrap().count(), 2);
    staged.publish(&target).unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"new".to_vec());
    assert_eq!(std::fs::read_dir(&area).unwrap().count(), 1);
}

#[test]
fn a_failed_publish_leaves_the_previous_state_and_no_staging_file() {
    let (dir, _store, _local) = fixture();
    let area = dir.path().join("area");
    // A folder where the file belongs makes the rename fail on every platform.
    std::fs::create_dir_all(area.join("SKILL.md")).unwrap();
    let staged = Staging::stage(&area, b"new").unwrap();
    assert!(staged.publish(&area.join("SKILL.md")).is_err());
    assert_eq!(std::fs::read_dir(&area).unwrap().count(), 1);
    assert!(area.join("SKILL.md").is_dir());
}

#[test]
fn a_staging_leftover_is_not_a_skill_and_does_not_break_reading() {
    let (_dir, _store, local) = fixture();
    local.write(LocalSkillWrite::create(SKILL), &[]).unwrap();
    let folder = local.root().join("redirect-checks");
    std::fs::write(folder.join(".SKILL.md.leftover.tmp"), "half a file").unwrap();
    assert_eq!(local.list().unwrap().len(), 1);
    assert_eq!(local.get("redirect-checks").unwrap().local_revision, 1);
    local.write(LocalSkillWrite::edit(SKILL, 1), &[]).unwrap();
    // The person's unrelated file stays; the write's own staging is gone.
    assert!(folder.join(".SKILL.md.leftover.tmp").exists());
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 2);
}
