use super::*;
use crate::source_control::forge::with_crew_input_fixture;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn closed_gitlab_snapshot_is_readable_for_receipts_but_ineligible_for_send() {
    for state in ["closed", "merged"] {
        let repo = CrewRepository {
            forge: Forge::GitLab,
            host: "gitlab.fixture.invalid".into(),
            repository: "group/repo".into(),
        };
        let fixture = Arc::new(
            move |forge: Forge, _: &Path, args: &[&str], input: Option<&str>| {
                assert_eq!(forge, Forge::GitLab);
                assert!(input.is_none());
                assert!(!args.contains(&"POST"));
                Ok(match args[1] {
                "projects/group%2Frepo"=>json!({"id":77,"path_with_namespace":"group/repo"}),
                "projects/77/merge_requests/7"=>json!({"iid":7,"state":state,"source_project_id":77,"target_project_id":77,"sha":"a".repeat(40),"diff_refs":{"base_sha":"b".repeat(40)}}),
                "user"=>json!({"id":8}),
                other=>panic!("unexpected fake request {other}"),
            }.to_string())
            },
        );
        with_crew_input_fixture(fixture, async {
            let (target, author, open) = receipt_snapshot(&repo, Path::new("."), 7).await.unwrap();
            assert!(!open);
            assert_eq!(target.repository_id, 77);
            assert_eq!(author, 8);
            assert!(snapshot(&repo, Path::new("."), 7).await.is_err());
        })
        .await;
    }
}

#[tokio::test]
async fn gitlab_uses_numeric_same_project_identity_explicit_host_and_exact_stdin() {
    let repo = CrewRepository {
        forge: Forge::GitLab,
        host: "gitlab.fixture.invalid".into(),
        repository: "group/repo".into(),
    };
    let body = "Advisory snapshot\n<!-- orbyn-review:fixture-operation -->\n";
    let calls = Arc::new(Mutex::new(0));
    let posts = calls.clone();
    let fixture = Arc::new(move |forge, _: &Path, args: &[&str], input: Option<&str>| {
        assert_eq!(forge, Forge::GitLab);
        assert!(
            args.windows(2)
                .any(|p| p == ["--hostname", "gitlab.fixture.invalid"])
        );
        assert!(
            !args
                .iter()
                .any(|a| a.contains(":id") || a.contains("{owner}"))
        );
        let value = match args[1] {
            "projects/group%2Frepo" => json!({"id":77,"path_with_namespace":"group/repo"}),
            "projects/77/merge_requests/7" => {
                json!({"iid":7,"state":"opened","source_project_id":77,"target_project_id":77,"sha":"a".repeat(40),"diff_refs":{"base_sha":"b".repeat(40)}})
            }
            "user" => json!({"id":8}),
            "projects/77/merge_requests/7/notes" => {
                assert!(args.windows(2).any(|p| p == ["--method", "POST"]));
                assert_eq!(input, Some(json!({"body":body}).to_string().as_str()));
                *posts.lock().unwrap() += 1;
                json!({"id":91})
            }
            "projects/77/merge_requests/7/notes/91" => {
                json!({"id":91,"author":{"id":8},"body":body})
            }
            "projects/77/merge_requests/7/notes?per_page=100" => {
                assert!(args.contains(&"--paginate"));
                return Ok(format!(
                    "[]\n{}",
                    json!([{"id":91,"author":{"id":8},"body":body}])
                ));
            }
            other => panic!("unexpected fake request {other}"),
        };
        Ok(value.to_string())
    });
    with_crew_input_fixture(fixture, async {
        let (target, author) = snapshot(&repo, Path::new("."), 7).await.unwrap();
        assert_eq!(target.repository_id, 77);
        assert_eq!(author, 8);
        let id = send(&target, Path::new("."), body).await.unwrap();
        let receipt = verify(&target, Path::new("."), id, author, body)
            .await
            .unwrap();
        assert_eq!(
            receipt.url,
            "https://gitlab.fixture.invalid/group/repo/-/merge_requests/7#note_91"
        );
        assert!(matches!(
            reconcile(
                &target,
                Path::new("."),
                "<!-- orbyn-review:fixture-operation -->",
                author,
                body
            )
            .await
            .unwrap(),
            Reconciled::Exact(_)
        ));
        assert_eq!(*calls.lock().unwrap(), 1);
    })
    .await;
}

#[tokio::test]
async fn foreign_source_repository_never_produces_a_trusted_comment_target() {
    let repo = CrewRepository {
        forge: Forge::GitHub,
        host: "github.com".into(),
        repository: "fixture/repo".into(),
    };
    let fixture = Arc::new(|_: Forge, _: &Path, args: &[&str], input: Option<&str>| {
        assert!(input.is_none());
        assert!(!args.contains(&"POST"));
        Ok(match args[1] {
            "repos/fixture/repo"=>json!({"id":77,"full_name":"fixture/repo"}),
            "repos/fixture/repo/pulls/7"=>json!({"number":7,"state":"open","head":{"sha":"a".repeat(40),"repo":{"id":999,"full_name":"foreign/repo"}},"base":{"sha":"b".repeat(40),"repo":{"id":77,"full_name":"fixture/repo"}}}),
            other=>panic!("unexpected fake request {other}"),
        }.to_string())
    });
    with_crew_input_fixture(fixture, async {
        assert!(snapshot(&repo, Path::new("."), 7).await.is_err());
    })
    .await;
}
