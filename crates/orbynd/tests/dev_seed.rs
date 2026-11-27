//! The debug-only dev seed seam over a real `orbynd` process (KC9): with `ORBYN_DEV_SEED_PROPOSALS=1` the route writes
//! generic escape-source lessons and the proposals read lists the proposal after three; without it the route is absent
//! and answers 404. The service is the real binary from `support`, so the env flag is read at start as production reads
//! it.

mod support;

use serde_json::{Value, json};
use support::{Options, Service, TempDir};

/// Crew Preview on or off, and the dev seam only when asked.
fn options(seed: bool, preview: bool) -> Options {
    let mut env = Vec::new();
    if preview {
        env.push(("ORBYN_CREW_PREVIEW".into(), "1".into()));
    }
    if seed {
        env.push(("ORBYN_DEV_SEED_PROPOSALS".into(), "1".into()));
    }
    Options {
        env,
        ..Options::default()
    }
}

async fn seed(service: &Service, repo: &str, lessons: u32) -> (u16, Value) {
    service
        .send(
            "/api/crew/dev/seed-proposal",
            json!({ "repo": repo, "lessons": lessons }),
        )
        .await
}

#[tokio::test]
async fn dev_seed_with_the_flag_makes_a_proposal_only_after_three_lessons() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), options(true, true));
    let folder = dir.path().join("project");
    std::fs::create_dir_all(&folder).unwrap();
    let repo = service.project(&folder, true).await;

    let (status, one) = seed(&service, &repo, 1).await;
    assert_eq!(status, 200, "{one}");
    assert_eq!(one["lessons"].as_array().unwrap().len(), 1);
    assert_eq!(one["proposal_id"], Value::Null);

    let (status, two) = seed(&service, &repo, 2).await;
    assert_eq!(status, 200, "{two}");
    assert_eq!(two["lessons"].as_array().unwrap().len(), 2);
    let proposal_id = two["proposal_id"]
        .as_str()
        .expect("the third lesson makes the proposal")
        .to_owned();

    let (status, repeated) = seed(&service, &repo, 3).await;
    assert_eq!(status, 200, "{repeated}");
    assert_eq!(repeated["proposal_id"], proposal_id.as_str());

    // The existing proposals read lists the one proposal of the group, with the dev lessons' evidence.
    let list = service
        .json(&format!("/api/crew/skill-proposals?repo={repo}"))
        .await;
    let proposals = list["proposals"].as_array().unwrap();
    assert_eq!(proposals.len(), 1, "{list}");
    let proposal = &proposals[0];
    assert_eq!(proposal["id"], proposal_id.as_str());
    assert_eq!(proposal["repo"], repo.as_str());
    assert_eq!(proposal["state"], "open");
    assert_eq!(proposal["can_approve"], true);
    let text = proposal["text"].as_str().unwrap();
    assert!(
        text.contains("- Dev seed lesson 1 for src/dev-seed."),
        "the default area is src/dev-seed: {text}"
    );
    let lessons = proposal["lessons"].as_array().unwrap();
    assert_eq!(lessons.len(), 3);
    for lesson in lessons {
        let evidence = lesson["evidence"].as_array().unwrap();
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0]["kind"], "escape");
        assert!(
            evidence[0]["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("dev-seed-"))
        );
    }

    // A bad count and an unknown repository keep their core codes over HTTP.
    let (status, bad) = seed(&service, &repo, 0).await;
    assert_eq!((status, &bad["code"]), (400, &json!("invalid")), "{bad}");
    let (status, missing) = seed(&service, "no-such-repository", 3).await;
    assert_eq!(
        (status, &missing["code"]),
        (404, &json!("not_found")),
        "{missing}"
    );
    let (status, unknown) = service
        .send(
            "/api/crew/dev/seed-proposal",
            json!({ "repo": repo, "extra": true }),
        )
        .await;
    assert_eq!(
        (status, &unknown["code"]),
        (400, &json!("invalid_input")),
        "{unknown}"
    );

    // Absent fields take their defaults: 3 lessons in another area make that group's proposal.
    let (status, defaulted) = service
        .send(
            "/api/crew/dev/seed-proposal",
            json!({ "repo": repo, "area": "src/dev-default" }),
        )
        .await;
    assert_eq!(status, 200, "{defaulted}");
    assert_eq!(defaulted["lessons"].as_array().unwrap().len(), 3);
    assert!(
        defaulted["proposal_id"].is_string(),
        "three default lessons make a proposal: {defaulted}"
    );
    service.stop().await;
}

#[tokio::test]
async fn without_the_flag_the_dev_seed_route_is_absent() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), options(false, true));
    let folder = dir.path().join("project");
    std::fs::create_dir_all(&folder).unwrap();
    let repo = service.project(&folder, true).await;

    let (status, answer) = seed(&service, &repo, 3).await;
    assert_eq!(status, 404, "{answer}");
    assert_eq!(answer["code"], "not_found");
    let (_, unknown) = service
        .send("/api/crew/not-a-route", json!({ "repo": repo }))
        .await;
    assert_eq!(answer, unknown, "it answers as an absent route does");
    let list = service
        .json(&format!("/api/crew/skill-proposals?repo={repo}"))
        .await;
    assert_eq!(list, json!({ "proposals": [] }));
    service.stop().await;
}

#[tokio::test]
async fn dev_seed_with_preview_off_is_not_found() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), options(true, false));
    let folder = dir.path().join("project");
    std::fs::create_dir_all(&folder).unwrap();
    let repo = service.project(&folder, true).await;

    let (status, answer) = seed(&service, &repo, 3).await;
    assert_eq!(status, 404, "{answer}");
    assert_eq!(answer["code"], "not_found");
    let (_, unknown) = service
        .send("/api/crew/not-a-route", json!({ "repo": repo }))
        .await;
    assert_eq!(
        answer, unknown,
        "the Preview gate answers as an absent route does"
    );
    let listed = service
        .get(&format!("/api/crew/skill-proposals?repo={repo}"))
        .send()
        .await
        .unwrap();
    assert_eq!(listed.status().as_u16(), 404);
    service.stop().await;
}
