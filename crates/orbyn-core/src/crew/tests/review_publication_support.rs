//! Owned Git + fake forge/approvers only; unexpected calls never reach a real CLI.
use super::{
    harness::{World, bind, world_publication},
    review_support::{arc, built, commit_all, reviewed},
};
use crate::{
    error::{CoreError, Result},
    source_control::forge::Forge,
    testing::run_git,
};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
pub struct Setup {
    pub w: World,
    pub subject: String,
    pub reviewer: String,
    pub token: String,
    pub run: String,
}
pub async fn setup() -> Setup {
    setup_timeout(None).await
}
pub async fn setup_timeout(timeout: Option<std::time::Duration>) -> Setup {
    let w = world_publication(
        arc(|n, root| {
            if n == 0 {
                let turn = built(root);
                commit_all(root, "candidate");
                turn
            } else {
                reviewed(vec![])
            }
        }),
        timeout,
    )
    .await;
    std::fs::create_dir_all(w.repo.join(".orbyn")).unwrap();
    std::fs::write(
        w.repo.join(".orbyn/crew.json"),
        r#"{"allowed_hosts":["github.com"]}"#,
    )
    .unwrap();
    commit_all(&w.repo, "trusted hosts");
    run_git(
        &w.repo,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/fixture/repo.git",
        ],
    );
    // A forge fixture has a fetched, trusted default-branch snapshot. Seed only
    // local fixture refs; the fake HTTPS origin is never contacted.
    run_git(&w.repo, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    run_git(
        &w.repo,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );
    let subject = w.built_work().await;
    let reviewer = w.review(&subject, &["correctness"]).await.remove(0);
    w.until_state(&reviewer, "passed").await;
    // Result/review notices start short fake lead turns. Admission remains the
    // authoritative CAS: wait for those turns instead of racing or replacing one.
    let mut admitted = None;
    w.until("the publication lead can be admitted", |w| {
        match w.core.store.admit_run(crate::store::Admission {
            thread_id: &w.lead,
            prompt: "Publish the completed advisory review.",
            actor: "test",
        }) {
            Ok(run) => {
                admitted = Some(run);
                true
            }
            Err(CoreError::ThreadBusy) => false,
            Err(error) => panic!("publication lead admission: {error}"),
        }
    })
    .await;
    let run = admitted.unwrap();
    w.core.store.mark_starting(&run.id).unwrap();
    w.core.store.mark_running(&run.id).unwrap();
    let token = bind(&w.core, &w.lead, &run.id);
    Setup {
        w,
        subject,
        reviewer,
        token,
        run: run.id,
    }
}
#[derive(Default)]
pub struct Remote {
    pub base: String,
    pub head: String,
    pub body: Option<String>,
    pub posts: usize,
    pub lose_reply: bool,
    pub invisible: bool,
    pub spoof: bool,
    pub duplicate: bool,
    pub wrong_url: bool,
    pub move_after_post: bool,
}
pub type Fixture = Arc<dyn Fn(Forge, &Path, &[&str], Option<&str>) -> Result<String> + Send + Sync>;
pub fn fake(s: &Setup) -> (Arc<Mutex<Remote>>, Fixture) {
    let state = Arc::new(Mutex::new(Remote {
        base: s
            .w
            .core
            .store
            .crew_run(&s.subject)
            .unwrap()
            .unwrap()
            .base_commit
            .unwrap(),
        head: s
            .w
            .core
            .store
            .crew_review(&s.reviewer)
            .unwrap()
            .unwrap()
            .commit,
        ..Remote::default()
    }));
    let shared = state.clone();
    let f: Fixture = Arc::new(move |forge, _, args, input| {
        assert_eq!(forge, Forge::GitHub);
        assert_eq!(args.first(), Some(&"api"));
        assert!(args.windows(2).any(|p| p == ["--hostname", "github.com"]));
        let mut state = shared.lock().unwrap();
        let comment = |state: &Remote| json!({"id":91,"user":{"id":if state.spoof {999}else{8}},"body":state.body,"html_url":if state.wrong_url {"https://hostile.invalid/receipt"}else{"https://github.com/fixture/repo/pull/7#issuecomment-91"},"issue_url":"https://api.github.com/repos/fixture/repo/issues/7"});
        let result = match args[1] {
            "repos/fixture/repo" => json!({"id":77,"full_name":"fixture/repo"}),
            "repos/fixture/repo/pulls/7" => {
                json!({"number":7,"state":"open","head":{"sha":state.head,"repo":{"id":77,"full_name":"fixture/repo"}},"base":{"sha":state.base,"repo":{"id":77,"full_name":"fixture/repo"}}})
            }
            "user" => json!({"id":8}),
            "repos/fixture/repo/issues/7/comments" => {
                assert!(args.windows(2).any(|p| p == ["--method", "POST"]));
                assert!(args.windows(2).any(|p| p == ["--input", "-"]));
                let payload: Value =
                    serde_json::from_str(input.expect("exact body must use stdin")).unwrap();
                assert_eq!(payload.as_object().unwrap().len(), 1);
                state.body = Some(payload["body"].as_str().unwrap().to_owned());
                state.posts += 1;
                if state.move_after_post {
                    state.head = "c".repeat(40);
                }
                if state.lose_reply {
                    return Err(CoreError::Git("fake accepted comment; reply lost".into()));
                }
                json!({"id":91})
            }
            "repos/fixture/repo/issues/comments/91" => comment(&state),
            "repos/fixture/repo/issues/7/comments?per_page=100" => {
                assert!(args.contains(&"--paginate"));
                if state.invisible || state.body.is_none() {
                    json!([])
                } else {
                    // A later page must be read, even when an earlier page is empty.
                    let rows = if state.duplicate {
                        json!([comment(&state), comment(&state)])
                    } else {
                        json!([comment(&state)])
                    };
                    return Ok(format!("[]\n{rows}"));
                }
            }
            other => panic!("unexpected fake forge request {other}"),
        };
        Ok(result.to_string())
    });
    (state, f)
}
pub fn args(s: &Setup) -> Value {
    json!({"task":s.subject,"review":s.reviewer,"pull":7})
}
pub fn keys(s: &Setup) {
    use super::autopilot_support::{Scripted, approvers};
    s.w.core.set_crew_approvers(approvers(
        &Scripted::yes("alpha", "alpha"),
        &[&Scripted::yes("beta", "beta")],
    ));
}
