//! A Snap Shot source at rest: kept beside its attachment, read with it, replaced, and
//! gone with the attachment. The send markers that make a later write a `not_found` are
//! the queued message and the `user.attachments` run event.

use super::*;
use crate::testing::TempDir;
use orbyn_contracts::{Attachment, AttachmentSource, ModelSelection};
use serde_json::json;

fn fixture() -> (TempDir, Store, String) {
    let dir = TempDir::new();
    let path = dir.path().join("orbyn.sqlite3");
    let store = Store::open(&path, "epoch-a").unwrap();
    let project = store
        .add_project(NewProject {
            path: "/repo",
            name: "repo",
            is_git: true,
            managed: false,
            scratch: false,
        })
        .unwrap();
    store.trust_project(&project.id, "tester").unwrap();
    let thread = store
        .create_thread(NewThread {
            project_id: project.id,
            title: "t".into(),
            engine: "fake".into(),
            model: None,
            cwd: "/repo".into(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap()
        .id;
    (dir, store, thread)
}

fn attachment(store: &Store, thread: &str, name: &str, mime: &str) -> Attachment {
    let attachment = Attachment {
        id: new_id(),
        thread_id: thread.to_owned(),
        name: name.to_owned(),
        mime: mime.to_owned(),
        size: 4,
        created_at: now(),
        source: None,
    };
    store
        .add_attachment(&attachment, "/tmp/orbyn-attachment")
        .unwrap();
    attachment
}

fn snap_shot(app_name: &str, title: &str, text: Option<&str>) -> AttachmentSource {
    AttachmentSource::SnapShot {
        captured_at: "2026-11-22T10:00:00Z".into(),
        app_name: app_name.into(),
        window_title: title.into(),
        app_id: Some("com.example.app".into()),
        accessible_text: text.map(str::to_owned),
    }
}

#[test]
fn a_source_is_listed_with_its_attachment_and_replaced_in_place() {
    let (_dir, store, thread) = fixture();
    let image = attachment(&store, &thread, "shot.png", "image/png");
    let first = snap_shot("Notes", "Weekly plan", Some("buy milk"));
    store.set_attachment_source(&image.id, &first).unwrap();
    assert_eq!(
        store.attachment_source(&image.id).unwrap(),
        Some(first.clone())
    );
    assert_eq!(
        store.attachment(&image.id).unwrap().0.source,
        Some(first.clone())
    );
    assert_eq!(
        store.list_attachments(&thread).unwrap()[0].0.source,
        Some(first.clone())
    );

    let replacement = snap_shot("Browser", "Docs", None);
    store
        .set_attachment_source(&image.id, &replacement)
        .unwrap();
    assert_eq!(
        store.attachment_source(&image.id).unwrap(),
        Some(replacement.clone())
    );
    assert_eq!(
        store.list_attachments(&thread).unwrap()[0].0.source,
        Some(replacement)
    );
}

#[test]
fn removing_its_attachment_removes_the_source_row() {
    let (_dir, store, thread) = fixture();
    let image = attachment(&store, &thread, "shot.png", "image/png");
    store
        .set_attachment_source(&image.id, &snap_shot("Notes", "Weekly plan", None))
        .unwrap();
    store.remove_attachment(&image.id).unwrap();
    assert_eq!(store.attachment_source(&image.id).unwrap(), None);
    let rows: i64 = store
        .reader()
        .query_row(
            "SELECT COUNT(*) FROM attachment_sources WHERE attachment_id = ?1",
            [&image.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rows, 0);
}

#[test]
fn a_queued_message_marks_its_attachments_as_sent() {
    let (_dir, store, thread) = fixture();
    let image = attachment(&store, &thread, "shot.png", "image/png");
    assert!(!store.attachment_is_sent(&image.id, &thread).unwrap());
    store
        .enqueue(
            &thread,
            "later",
            std::slice::from_ref(&image.id),
            &ModelSelection::default(),
        )
        .unwrap();
    assert!(store.attachment_is_sent(&image.id, &thread).unwrap());
}

#[test]
fn a_run_input_event_marks_its_attachments_as_sent() {
    let (_dir, store, thread) = fixture();
    let image = attachment(&store, &thread, "shot.png", "image/png");
    assert!(!store.attachment_is_sent(&image.id, &thread).unwrap());
    store
        .append_thread_event(
            &thread,
            &AgUiEvent::Custom {
                name: "user.attachments".into(),
                value: json!({ "attachments": [{ "id": image.id.clone() }] }),
            },
        )
        .unwrap();
    assert!(store.attachment_is_sent(&image.id, &thread).unwrap());
}

#[test]
fn migration_73_follows_72_and_adds_the_attachment_sources_table() {
    let versions: Vec<i64> = migrations::MIGRATIONS
        .iter()
        .map(|(version, _)| *version)
        .collect();
    assert_eq!(versions[versions.len() - 2], 72);
    assert_eq!(versions[versions.len() - 1], 73);
    let (_, sql) = migrations::MIGRATIONS
        .iter()
        .find(|(version, _)| *version == 73)
        .unwrap();
    assert!(sql.contains("CREATE TABLE attachment_sources"));
    assert!(sql.contains("attachment_id TEXT PRIMARY KEY"));
    assert!(sql.contains("source TEXT NOT NULL"));
    assert!(sql.contains("created_at TEXT NOT NULL"));

    let (_dir, store, _thread) = fixture();
    let columns: Vec<String> = store
        .reader()
        .prepare("PRAGMA table_info(attachment_sources)")
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        columns,
        vec![
            "attachment_id".to_owned(),
            "source".to_owned(),
            "created_at".to_owned()
        ]
    );
}
