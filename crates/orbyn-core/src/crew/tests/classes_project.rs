//! Data classes read by project (slice 4i): the thread-or-project choice, the project read against the thread read, the
//! default branch pin and the refusals. Fake engines only; the only processes are `git`.

use std::sync::Arc;

use serde_json::Value;

use super::classes::{commit_crew, file, pin_origin_here, rule, two};
use super::harness::{Turn, World, world_with};

fn project_of(w: &World) -> String {
    w.core.store.thread(&w.lead).unwrap().project_id
}

/// A view as JSON without the two fields that only say how it was asked for.
fn asked_neutrally(view: &impl serde::Serialize) -> Value {
    let mut value = serde_json::to_value(view).unwrap();
    let map = value.as_object_mut().unwrap();
    map.remove("thread");
    map.remove("worktree");
    value
}

#[tokio::test]
async fn a_class_read_names_a_thread_or_a_project_and_nothing_else() {
    let w = two().await;
    let project = project_of(&w);
    let (thread, project) = (Some(w.lead.as_str()), Some(project.as_str()));
    let invalid = |error: crate::error::CoreError| match error {
        crate::error::CoreError::Invalid(words) => words,
        other => panic!("not a refusal: {other:?}"),
    };
    let both = w.core.crew_classes_view(thread, project).await.unwrap_err();
    assert_eq!(invalid(both), "give thread or project, not both");
    let neither = w.core.crew_classes_view(None, None).await.unwrap_err();
    assert_eq!(invalid(neither), "thread or project is required");
    let paths = ["a".to_owned()];
    let both = w.core.crew_class_test(thread, project, &paths).await;
    assert_eq!(
        invalid(both.unwrap_err()),
        "give thread or project, not both"
    );
    let neither = w.core.crew_class_test(None, None, &paths).await;
    assert_eq!(
        invalid(neither.unwrap_err()),
        "thread or project is required"
    );
    for bad in ["a b", "../x", "", "a/b"] {
        let one = w.core.crew_classes_view(None, Some(bad)).await;
        assert_eq!(invalid(one.unwrap_err()), "that id is not valid", "{bad}");
        let one = w.core.crew_class_test(None, Some(bad), &paths).await;
        assert_eq!(invalid(one.unwrap_err()), "that id is not valid", "{bad}");
    }
    let unknown = w.core.crew_classes_view(None, Some("nope")).await;
    assert!(matches!(unknown, Err(crate::error::CoreError::NotFound(_))));
    let unknown = w.core.crew_class_test(None, Some("nope"), &paths).await;
    assert!(matches!(unknown, Err(crate::error::CoreError::NotFound(_))));
    // The flag still hides it all, whatever was asked.
    let off = world_with(false, Arc::new(|_, _| Turn::Hang), &[("alpha", false)]).await;
    for result in [
        off.core.crew_classes_view(None, None).await.map(|_| ()),
        off.core
            .crew_classes_view(None, Some("p"))
            .await
            .map(|_| ()),
    ] {
        assert!(matches!(result, Err(crate::error::CoreError::NotFound(_))));
    }
}

#[tokio::test]
async fn a_project_read_equals_the_thread_read_of_the_same_checkout() {
    let w = two().await;
    commit_crew(
        &w.repo,
        &file(vec![rule("auth", &["src/auth/**"], &["bravo"])]),
    );
    // A working copy edit that is not committed shows on both reads alike.
    std::fs::write(
        w.repo.join(".orbyn/crew.json"),
        file(vec![rule("auth", &["src/auth/**"], &["alpha", "bravo"])]),
    )
    .unwrap();
    let project = project_of(&w);
    let by_thread = w.core.crew_classes_view(Some(&w.lead), None).await.unwrap();
    let by_project = w
        .core
        .crew_classes_view(None, Some(&project))
        .await
        .unwrap();
    assert_eq!(by_thread.thread.as_deref(), Some(w.lead.as_str()));
    assert!(by_thread.worktree.is_some());
    assert!(by_project.thread.is_none() && by_project.worktree.is_none());
    assert!(by_project.working_copy_differs && by_project.commit.is_some());
    assert_eq!(asked_neutrally(&by_thread), asked_neutrally(&by_project));
    let paths = vec!["src/auth/x.rs".to_owned(), "docs/a.md".to_owned()];
    let by_thread = w
        .core
        .crew_class_test(Some(&w.lead), None, &paths)
        .await
        .unwrap();
    let by_project = w
        .core
        .crew_class_test(None, Some(&project), &paths)
        .await
        .unwrap();
    assert_eq!(by_thread.thread.as_deref(), Some(w.lead.as_str()));
    assert!(by_project.thread.is_none());
    assert_eq!(asked_neutrally(&by_thread), asked_neutrally(&by_project));
    assert_eq!(by_project.paths[0].classes[0].name, "auth");
}

#[tokio::test]
async fn a_project_read_keeps_the_default_branch_pin() {
    let w = two().await;
    commit_crew(
        &w.repo,
        &file(vec![rule("keep", &["src/**"], &["alpha", "bravo"])]),
    );
    pin_origin_here(&w.repo);
    // A later commit narrows the rule on the checkout: the two sides differ and the read says so.
    commit_crew(&w.repo, &file(vec![rule("keep", &["src/**"], &["bravo"])]));
    let project = project_of(&w);
    let view = w
        .core
        .crew_classes_view(None, Some(&project))
        .await
        .unwrap();
    let notes = view.notes.join(" ");
    assert!(
        notes.contains("The rules differ") && notes.contains("origin/main (the default branch)"),
        "{notes}"
    );
    let paths = vec!["src/a.rs".to_owned()];
    let test = w
        .core
        .crew_class_test(None, Some(&project), &paths)
        .await
        .unwrap();
    let card = &test.paths[0].route_card;
    assert!(card.contains("The rules differ"), "{card}");
    // The stricter side applies: only what both allow.
    let engines: Vec<&str> = test.paths[0]
        .allowed_engines
        .as_ref()
        .unwrap()
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(engines, ["bravo"]);
}

#[tokio::test]
async fn a_project_read_refuses_an_untrusted_or_removed_project() {
    let w = two().await;
    let paths = ["a".to_owned()];
    // A project nobody has trusted is refused as a thread's diff refuses it.
    let loose = w.core.new_project("loose", false, None).await.unwrap();
    let view = w.core.crew_classes_view(None, Some(&loose.id)).await;
    assert!(matches!(
        view,
        Err(crate::error::CoreError::FolderUntrusted)
    ));
    let test = w.core.crew_class_test(None, Some(&loose.id), &paths).await;
    assert!(matches!(
        test,
        Err(crate::error::CoreError::FolderUntrusted)
    ));
    // A removed project is refused with project_removed (409), never read.
    let id = project_of(&w);
    w.core
        .store
        .remove_project_keeping_threads(&id, "tester")
        .unwrap();
    let view = w.core.crew_classes_view(None, Some(&id)).await;
    assert!(matches!(
        view,
        Err(crate::error::CoreError::ProjectRemoved(_))
    ));
    let test = w.core.crew_class_test(None, Some(&id), &paths).await;
    assert!(matches!(
        test,
        Err(crate::error::CoreError::ProjectRemoved(_))
    ));
}
