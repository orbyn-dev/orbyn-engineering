use super::*;
use crate::{
    CoreConfig,
    testing::{TempDir, library, run_git},
};
use orbyn_contracts::{ProjectGroupingOverride, WorkspaceSetup};
use serde_json::json;

pub(super) async fn core(dir: &TempDir) -> Core {
    let root = library(&dir.path().join("library"));
    let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    let core = Core::open(config).unwrap();
    core.workspace
        .setup(WorkspaceSetup {
            url: None,
            adopt_path: Some(root.to_string_lossy().into_owned()),
        })
        .await
        .unwrap();
    core
}

pub(super) fn repository(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    run_git(path, &["init", "-q", "-b", "main"]);
    run_git(
        path,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-qm",
            "Fixture",
        ],
    );
}

pub(super) fn configure(path: &Path, name: &str, remote: &str) {
    run_git(path, &["config", &format!("remote.{name}.url"), remote]);
}

fn group<'a>(catalog: &'a ProjectScopeCatalog, id: &str) -> &'a ProjectScopeGroup {
    catalog
        .groups
        .iter()
        .find(|group| group.members.iter().any(|member| member.project_id == id))
        .unwrap()
}

#[test]
fn remote_equivalence_is_limited_and_never_retains_userinfo() {
    // Synthetic userinfo exercises stripping without embedding a credential literal.
    // This parser never contacts the remote or invokes credential helpers.
    let mut with_userinfo =
        reqwest::Url::parse("https://GitHub.COM/Owner/Repo.git?token=fixture#fixture").unwrap();
    with_userinfo.set_username("fixture-user").unwrap();
    with_userinfo
        .set_password(Some("fixture-password"))
        .unwrap();
    let keys: Vec<_> = [
        with_userinfo.as_str(),
        "ssh://git@github.com/Owner/Repo.git",
        "git@github.com:Owner/Repo.git",
    ]
    .into_iter()
    .map(|raw| remote_repository(raw).unwrap())
    .collect();
    assert!(
        keys.iter()
            .all(|key| key.canonical == "github.com/Owner/Repo" && key.label == "Owner/Repo")
    );
    let explicit = remote_repository("https://github.com:443/Owner/Repo.git").unwrap();
    assert_ne!(
        keys[0].canonical, explicit.canonical,
        "explicit ports stay distinct"
    );
    assert_ne!(
        keys[0].canonical,
        remote_repository("git@github.com:Owner/repo.git")
            .unwrap()
            .canonical
    );
    for value in [
        "/tmp/repo.git",
        "../repo",
        "file:///tmp/repo",
        r"C:\work\repo",
        "garbage",
        "https://",
        "https://host/a/../b",
        "ext::command",
        "https://host/a%2fb",
    ] {
        assert!(
            remote_repository(value).is_none(),
            "a local or malformed locator must stay physical"
        );
    }
    assert!(remote_repository(&format!("https://host/{}", "x".repeat(MAX_REMOTE_BYTES))).is_none());
}

#[test]
fn fork_origin_wins_and_local_origins_do_not_merge_unrelated_folders() {
    let parse = |text: &str| {
        repository_from_config(text.as_bytes())
            .unwrap()
            .map(|repo| repo.canonical)
    };
    assert_eq!(
        parse(
            "remote.upstream.url\nhttps://host/Parent/Repo.git\0remote.origin.url\ngit@host:Fork/Repo.git\0"
        ),
        Some("host/Fork/Repo".into())
    );
    assert_eq!(parse("remote.origin.url\n/tmp/shared.git\0"), None);
    assert_eq!(
        parse(
            "remote.origin.url\nrelative/folder:repo.git\0remote.upstream.url\nhttps://host/Parent/Repo.git\0"
        ),
        Some("host/Parent/Repo".into())
    );
    assert_eq!(
        parse("remote.upstream.url\nhttps://host/Parent/Repo.git\0remote.origin.url\nhttps://\0"),
        None,
        "an unparseable fork origin must not merge into upstream"
    );
}

#[tokio::test]
async fn local_clones_group_by_repository_with_forks_and_physical_overrides_separate() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let source = dir.path().join("first");
    repository(&source);
    let clone = dir.path().join("second");
    run_git(
        dir.path(),
        &[
            "clone",
            "-q",
            "--no-hardlinks",
            source.to_str().unwrap(),
            clone.to_str().unwrap(),
        ],
    );
    let fork = dir.path().join("fork");
    run_git(
        dir.path(),
        &[
            "clone",
            "-q",
            "--no-hardlinks",
            source.to_str().unwrap(),
            fork.to_str().unwrap(),
        ],
    );
    configure(
        &source,
        "origin",
        "https://user:PRIVATE@host/Owner/Repo.git?key=PRIVATE",
    );
    configure(&clone, "origin", "git@host:Owner/Repo.git");
    configure(&fork, "origin", "ssh://git@host/Fork/Repo.git");
    configure(&fork, "upstream", "https://host/Owner/Repo.git");
    let a = core.add_project(source.to_str().unwrap()).await.unwrap();
    let b = core.add_project(clone.to_str().unwrap()).await.unwrap();
    let c = core.add_project(fork.to_str().unwrap()).await.unwrap();
    let mut query = ProjectScopesQuery::default();
    let catalog = core.project_scope_catalog(&query).await.unwrap();
    assert!(catalog.metadata_complete);
    assert_eq!(group(&catalog, &a.id).members.len(), 2);
    assert_eq!(group(&catalog, &a.id).key, group(&catalog, &b.id).key);
    assert_ne!(group(&catalog, &a.id).key, group(&catalog, &c.id).key);
    assert!(!serde_json::to_string(&catalog).unwrap().contains("PRIVATE"));
    assert!(
        !serde_json::to_string(&catalog)
            .unwrap()
            .contains("https://")
    );
    let original_key = group(&catalog, &a.id).key.clone();
    core.rename_project(&a.id, "Unrelated title").unwrap();
    let renamed = core.project_scope_catalog(&query).await.unwrap();
    assert_eq!(original_key, group(&renamed, &a.id).key);
    assert_eq!(
        catalog.version, renamed.version,
        "names are not membership identity"
    );
    query.recipe.mode = ProjectGroupingMode::RepositoryPath;
    let by_path = core.project_scope_catalog(&query).await.unwrap();
    assert_eq!(group(&by_path, &a.id).key, group(&by_path, &b.id).key);
    assert_eq!(
        group(&by_path, &a.id).members[0]
            .repository_relative_path
            .as_deref(),
        Some("")
    );
    query.recipe.overrides.push(ProjectGroupingOverride {
        project_id: b.id.clone(),
        mode: ProjectGroupingMode::Separate,
    });
    let separated = core.project_scope_catalog(&query).await.unwrap();
    assert_ne!(group(&separated, &a.id).key, group(&separated, &b.id).key);
    assert_ne!(by_path.version, separated.version);
    query.recipe.mode = ProjectGroupingMode::Separate;
    assert_eq!(
        core.project_scope_catalog(&query)
            .await
            .unwrap()
            .groups
            .len(),
        3
    );
}

#[tokio::test]
async fn physical_fallbacks_and_stale_selections_never_broaden_to_all() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let a_path = dir.path().join("a");
    let b_path = dir.path().join("b");
    repository(&a_path);
    repository(&b_path);
    configure(&a_path, "origin", "/tmp/same.git");
    configure(&b_path, "origin", "/tmp/same.git");
    let a = core.add_project(a_path.to_str().unwrap()).await.unwrap();
    let b = core.add_project(b_path.to_str().unwrap()).await.unwrap();
    let mut query = ProjectScopesQuery::default();
    let before = core.project_scope_catalog(&query).await.unwrap();
    assert!(before.metadata_complete);
    assert_ne!(group(&before, &a.id).key, group(&before, &b.id).key);
    assert!(group(&before, &a.id).members[0].repository_key.is_none());
    query.selection = Some(ProjectSettingsSelection::ProjectGroup {
        group_key: "stale".into(),
    });
    assert!(matches!(
        core.project_scope_catalog(&query).await.unwrap().selection,
        ResolvedProjectSettingsScope::Unavailable { .. }
    ));
    query.selection = Some(ProjectSettingsSelection::Checkout {
        group_key: group(&before, &a.id).key.clone(),
        project_id: b.id.clone(),
    });
    assert!(matches!(
        core.project_scope_catalog(&query).await.unwrap().selection,
        ResolvedProjectSettingsScope::Unavailable {
            reason: ProjectScopeUnavailableReason::CheckoutMissing
        }
    ));
    std::fs::remove_dir_all(&a_path).unwrap();
    query.selection = Some(ProjectSettingsSelection::Checkout {
        group_key: group(&before, &a.id).key.clone(),
        project_id: a.id.clone(),
    });
    let after = core.project_scope_catalog(&query).await.unwrap();
    assert_ne!(before.version, after.version);
    assert!(matches!(
        after.selection,
        ResolvedProjectSettingsScope::Unavailable {
            reason: ProjectScopeUnavailableReason::CheckoutMissing
        }
    ));
    query.recipe.overrides.push(ProjectGroupingOverride {
        project_id: "removed".into(),
        mode: ProjectGroupingMode::Separate,
    });
    assert!(matches!(
        core.project_scope_catalog(&query).await,
        Err(CoreError::NotFound(_))
    ));
}

#[tokio::test]
async fn registered_linked_worktrees_are_real_checkout_members() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let root = dir.path().join("repo");
    repository(&root);
    configure(&root, "origin", "git@host:Owner/Repo.git");
    let linked = dir.path().join("linked");
    run_git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "linked",
            linked.to_str().unwrap(),
        ],
    );
    let a = core.add_project(root.to_str().unwrap()).await.unwrap();
    let b = core.add_project(linked.to_str().unwrap()).await.unwrap();
    let catalog = core
        .project_scope_catalog(&ProjectScopesQuery::default())
        .await
        .unwrap();
    let members = &group(&catalog, &a.id).members;
    assert_eq!(members.len(), 2);
    assert_ne!(members[0].physical_key, members[1].physical_key);
    assert_eq!(group(&catalog, &a.id).key, group(&catalog, &b.id).key);
    assert!(
        members
            .iter()
            .all(|member| member.repository_relative_path.as_deref() == Some(""))
    );
}

#[tokio::test]
async fn metadata_deadline_stops_and_reaps_its_owned_git_process() {
    let dir = TempDir::new();
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        metadata(
            dir.path(),
            &[
                "-c",
                "alias.scope-wait=!(sleep 1; echo late > late) & wait",
                "scope-wait",
            ],
            Instant::now() + Duration::from_millis(100),
        ),
    )
    .await
    .unwrap();
    assert!(result.is_none());
    let late = tokio::time::timeout(Duration::from_secs(2), async {
        while !dir.path().join("late").exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    assert!(
        late.is_err(),
        "a cancelled metadata helper wrote after its owner returned"
    );
}

#[tokio::test]
async fn subfolder_registration_retains_the_selected_folder_and_plain_folders_are_complete() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let root = dir.path().join("repo");
    repository(&root);
    std::fs::create_dir_all(root.join("nested")).unwrap();
    let project = core
        .add_project(root.join("nested").to_str().unwrap())
        .await
        .unwrap();
    assert_eq!(Path::new(&project.path), root.join("nested"));
    assert!(project.is_git);
    assert!(!project.trusted);
    let plain = dir.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let plain = core.add_project(plain.to_str().unwrap()).await.unwrap();
    let catalog = core
        .project_scope_catalog(&ProjectScopesQuery::default())
        .await
        .unwrap();
    assert_eq!(
        group(&catalog, &project.id).members[0]
            .repository_relative_path
            .as_deref(),
        Some("nested")
    );
    let member = &group(&catalog, &plain.id).members[0];
    assert!(member.metadata_complete && member.repository_key.is_none());
    assert!(member.repository_relative_path.is_none());
}

#[test]
fn explicit_null_and_malformed_selections_are_not_default_all() {
    assert!(
        serde_json::from_value::<ProjectScopesQuery>(json!({}))
            .unwrap()
            .selection
            .is_none()
    );
    for value in [
        json!({ "selection": null }),
        json!({ "selection": {} }),
        json!({ "selection": { "kind": "all_projects", "group_key": "stale" } }),
    ] {
        assert!(
            serde_json::from_value::<ProjectScopesQuery>(value.clone()).is_err(),
            "{value}"
        );
    }
}

#[tokio::test]
async fn exhausted_metadata_budget_is_unknown_and_cannot_hide_group_members() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let path = dir.path().join("repo");
    repository(&path);
    configure(&path, "origin", "https://host/Owner/Repo.git");
    let project = core.add_project(path.to_str().unwrap()).await.unwrap();
    let member = checkout(
        &ScopeGitFacts::default(),
        project,
        ProjectGroupingMode::Repository,
        Instant::now(),
    )
    .await;
    assert!(!member.metadata_complete && member.repository_key.is_none());
    let catalog = core
        .project_scope_catalog(&ProjectScopesQuery::default())
        .await
        .unwrap();
    let key = &catalog.groups[0].key;
    assert!(matches!(
        resolve_project_settings_scope(
            &catalog.groups,
            Some(&ProjectSettingsSelection::ProjectGroup {
                group_key: key.clone()
            }),
            false
        ),
        ResolvedProjectSettingsScope::Unavailable {
            reason: ProjectScopeUnavailableReason::MetadataUnavailable
        }
    ));
    assert!(matches!(
        resolve_project_settings_scope(
            &catalog.groups,
            Some(&ProjectSettingsSelection::Checkout {
                group_key: key.clone(),
                project_id: catalog.groups[0].members[0].project_id.clone()
            }),
            false
        ),
        ResolvedProjectSettingsScope::Checkout { .. }
    ));
    assert_eq!(
        resolve_project_settings_scope(&catalog.groups, None, false),
        ResolvedProjectSettingsScope::AllProjects
    );
}

#[test]
fn first_fetch_url_is_authoritative_and_unobserved_config_is_not_guessed() {
    let repo = repository_from_config(b"remote.origin.url\nhttps://host/First/Repo.git\0remote.origin.url\nhttps://host/Second/Repo.git\0").unwrap().unwrap();
    assert_eq!(repo.canonical, "host/First/Repo");
    assert!(
        repository_from_config(
            b"remote.origin.url\n/tmp/local.git\0remote.origin.url\nhttps://host/Second/Repo.git\0"
        )
        .unwrap()
        .is_none()
    );
    for bytes in [
        b"include.path\n/not/read\0".as_slice(),
        b"includeif.gitdir:/x.path\n/not/read\0",
        b"url.https://host/.insteadof\nshort:\0",
    ] {
        assert!(repository_from_config(bytes).is_err());
    }
}

#[tokio::test]
async fn worktree_origin_is_observed_without_overriding_first_common_fetch_url() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let root = dir.path().join("repo");
    repository(&root);
    configure(&root, "upstream", "https://host/Parent/Repo.git");
    run_git(&root, &["config", "extensions.worktreeConfig", "true"]);
    let linked = dir.path().join("linked");
    run_git(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "linked",
            linked.to_str().unwrap(),
        ],
    );
    run_git(
        &linked,
        &[
            "config",
            "--worktree",
            "remote.origin.url",
            "https://host/Fork/Repo.git",
        ],
    );
    let a = core.add_project(root.to_str().unwrap()).await.unwrap();
    let b = core.add_project(linked.to_str().unwrap()).await.unwrap();
    let query = ProjectScopesQuery::default();
    let catalog = core.project_scope_catalog(&query).await.unwrap();
    assert!(catalog.metadata_complete);
    assert_ne!(group(&catalog, &a.id).key, group(&catalog, &b.id).key);
    assert_eq!(
        group(&catalog, &b.id).members[0]
            .repository_label
            .as_deref(),
        Some("Fork/Repo")
    );
    configure(&root, "origin", "https://host/Parent/Repo.git");
    let catalog = core.project_scope_catalog(&query).await.unwrap();
    assert_eq!(group(&catalog, &a.id).key, group(&catalog, &b.id).key);
    // This file must not be followed, even by the subsequent root probe.
    let include = dir.path().join("invalid-include");
    std::fs::write(&include, "INVALID FIXTURE CONFIG\n").unwrap();
    run_git(
        &root,
        &["config", "include.path", include.to_str().unwrap()],
    );
    let catalog = core.project_scope_catalog(&query).await.unwrap();
    assert!(!catalog.metadata_complete);
    assert!(group(&catalog, &a.id).members[0].repository_key.is_none());
}

#[tokio::test]
async fn one_checkout_keeps_its_physical_title_instead_of_repository_fallback() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let path = dir.path().join("Repo");
    repository(&path);
    configure(&path, "origin", "https://fixture.invalid/Owner/Repo.git");
    let project = core
        .store
        .add_project(crate::store::NewProject {
            path: path.to_str().unwrap(),
            name: "Repo",
            is_git: true,
            managed: false,
            scratch: false,
        })
        .unwrap();
    let catalog = core
        .project_scope_catalog(&ProjectScopesQuery::default())
        .await
        .unwrap();
    assert_eq!(group(&catalog, &project.id).label, "Repo");
    let names = BTreeMap::from([(project.id.clone(), project.name.clone())]);
    core.store
        .rename_project(&project.id, "Concurrent")
        .unwrap();
    assert!(matches!(
        core.store.rename_selected_projects(
            &catalog,
            &core.workspace.root().unwrap(),
            &names,
            "Rejected"
        ),
        Err(CoreError::SettingsConflict)
    ));
    assert_eq!(core.store.project(&project.id).unwrap().name, "Concurrent");
    let names = BTreeMap::from([(project.id.clone(), "Concurrent".into())]);
    let other = dir.path().join("added");
    std::fs::create_dir_all(&other).unwrap();
    core.store
        .add_project(crate::store::NewProject {
            path: other.to_str().unwrap(),
            name: "Added",
            is_git: false,
            managed: false,
            scratch: false,
        })
        .unwrap();
    assert!(matches!(
        core.store.rename_selected_projects(
            &catalog,
            &core.workspace.root().unwrap(),
            &names,
            "Rejected"
        ),
        Err(CoreError::SettingsConflict)
    ));
    assert_eq!(core.store.project(&project.id).unwrap().name, "Concurrent");
}

#[tokio::test]
async fn incomplete_repository_metadata_disables_checkout_and_group_writes_consistently() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let path = dir.path().join("checkout");
    repository(&path);
    configure(&path, "origin", "https://fixture.invalid/Owner/Repo.git");
    let project = core
        .store
        .add_project(crate::store::NewProject {
            path: path.to_str().unwrap(),
            name: "Checkout",
            is_git: true,
            managed: false,
            scratch: false,
        })
        .unwrap();
    let mut catalog = core
        .project_scope_catalog(&ProjectScopesQuery::default())
        .await
        .unwrap();
    let index = catalog
        .groups
        .iter()
        .position(|group| {
            group
                .members
                .iter()
                .any(|member| member.project_id == project.id)
        })
        .unwrap();
    let key = catalog.groups[index].key.clone();
    catalog.groups[index].members[0].metadata_complete = false;
    catalog.selection = ResolvedProjectSettingsScope::Checkout {
        group_key: key.clone(),
        project_id: project.id.clone(),
    };
    assert!(matches!(
        selected_members(&catalog),
        Err(CoreError::SettingsScopeUnavailable)
    ));
    catalog.selection = ResolvedProjectSettingsScope::ProjectGroup {
        group_key: key.clone(),
        project_ids: vec![project.id.clone()],
    };
    assert!(matches!(
        selected_members(&catalog),
        Err(CoreError::SettingsScopeUnavailable)
    ));
    catalog.groups[index].members[0].grouping_mode = ProjectGroupingMode::Separate;
    assert_eq!(
        selected_members(&catalog).unwrap(),
        vec![project.id.clone()]
    );
    catalog.selection = ResolvedProjectSettingsScope::Checkout {
        group_key: key,
        project_id: project.id.clone(),
    };
    assert_eq!(selected_members(&catalog).unwrap(), vec![project.id]);
}

#[test]
fn catalog_reads_share_only_their_own_scope() {
    let parse =
        |value: serde_json::Value| serde_json::from_value::<ProjectScopesQuery>(value).unwrap();
    let all = parse(json!({}));
    let all_explicit = parse(json!({ "selection": { "kind": "all_projects" } }));
    let group = parse(json!({ "selection": { "kind": "project_group", "group_key": "g" } }));
    let checkout = parse(json!({
        "selection": { "kind": "checkout", "group_key": "g", "project_id": "p" }
    }));
    let separate = parse(json!({ "recipe": { "mode": "separate" } }));
    let override_a = parse(json!({
        "recipe": { "overrides": [{ "project_id": "p", "mode": "separate" }] }
    }));
    let override_b = parse(json!({
        "recipe": { "overrides": [{ "project_id": "q", "mode": "separate" }] }
    }));
    // A project id is an opaque string on the wire: a delimiter inside one override
    // must not stand in for another override's boundary.
    let merged = parse(json!({
        "recipe": { "overrides": [{ "project_id": "p=separate;q", "mode": "repository" }] }
    }));
    let split = parse(json!({
        "recipe": { "overrides": [
            { "project_id": "p", "mode": "separate" },
            { "project_id": "q", "mode": "repository" }
        ] }
    }));
    assert_ne!(
        catalog_read_key(&merged),
        catalog_read_key(&split),
        "an override delimiter cannot merge two distinct recipes"
    );
    assert_eq!(
        catalog_read_key(&all),
        catalog_read_key(&all_explicit),
        "an absent selection and an explicit All projects observe the same thing"
    );
    for (left, right) in [
        (&all, &group),
        (&all, &checkout),
        (&all, &separate),
        (&all, &override_a),
        (&group, &checkout),
        (&override_a, &override_b),
    ] {
        assert_ne!(
            catalog_read_key(left),
            catalog_read_key(right),
            "a different scope must not share another scope's observation"
        );
    }
}

#[test]
fn catalog_reads_ignore_override_order() {
    let parse =
        |value: serde_json::Value| serde_json::from_value::<ProjectScopesQuery>(value).unwrap();
    let first = parse(json!({
        "recipe": { "overrides": [
            { "project_id": "a", "mode": "separate" },
            { "project_id": "b", "mode": "repository" }
        ] }
    }));
    let reordered = parse(json!({
        "recipe": { "overrides": [
            { "project_id": "b", "mode": "repository" },
            { "project_id": "a", "mode": "separate" }
        ] }
    }));
    let changed = parse(json!({
        "recipe": { "overrides": [
            { "project_id": "a", "mode": "repository" },
            { "project_id": "b", "mode": "repository" }
        ] }
    }));
    assert_eq!(catalog_read_key(&first), catalog_read_key(&reordered));
    assert_ne!(catalog_read_key(&first), catalog_read_key(&changed));
}

#[tokio::test]
async fn catalog_reads_together_share_one_observation_and_a_later_one_sees_changes() {
    let dir = TempDir::new();
    let core = core(&dir).await;
    let root = dir.path().join("repo");
    repository(&root);
    configure(&root, "origin", "git@host:Owner/Repo.git");
    core.add_project(root.to_str().unwrap()).await.unwrap();
    // A selection no other test asks for, so the gate counts only these reads.
    let query = serde_json::from_value::<ProjectScopesQuery>(json!({
        "selection": { "kind": "project_group", "group_key": "catalog-reads-together" }
    }))
    .unwrap();
    let observation = super::observation_hooks::hold(&catalog_read_key(&query));
    let together =
        futures_util::future::join_all((0..5).map(|_| core.project_scope_catalog(&query)));
    // Hold the first observation open until every read has joined it.
    let gate = async {
        tokio::time::timeout(Duration::from_secs(10), observation.started())
            .await
            .expect("the concurrent reads start an observation");
        assert_eq!(
            observation.count(),
            1,
            "concurrent identical reads make one observation"
        );
        observation.release();
    };
    let (together, ()) = tokio::join!(together, gate);
    let first = together[0].as_ref().unwrap();
    for catalog in &together {
        assert_eq!(catalog.as_ref().unwrap(), first);
    }
    // No answer is kept: a read after a change observes the change, not the last answer.
    let other = dir.path().join("other");
    repository(&other);
    core.add_project(other.to_str().unwrap()).await.unwrap();
    let after = core.project_scope_catalog(&query).await.unwrap();
    assert_eq!(
        after.groups.iter().flat_map(|group| &group.members).count(),
        2
    );
    assert_ne!(after.version, first.version);
    assert_eq!(
        observation.count(),
        2,
        "a later read observes the workspace again"
    );
}

/// A folder whose name ends in a space. Windows keeps such a name only when the folder is named
/// through its verbatim form, and Git refuses to enter one at all (it drops the space and finds no
/// such folder), so this is how a test can hold the path Git reports for one.
fn space_ending_folder(path: &Path) -> PathBuf {
    #[cfg(windows)]
    let path = PathBuf::from(format!(r"\\?\{}", path.display()));
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// Git names the work tree root it resolved, and `lines` has already dropped the line terminator:
/// a root whose name ends in a space is that root. Trimming it names the folder beside it, which
/// is not the checkout Git answered for.
#[test]
fn a_work_tree_root_ending_in_a_space_is_read_whole() {
    let dir = TempDir::new();
    let root = space_ending_folder(&dir.path().join("checkout "));
    let answer = super::Answer::read(format!("{}\n\n", root.display()).as_bytes()).unwrap();
    assert_eq!(
        answer.root,
        crate::util::canonicalize(&root).unwrap(),
        "the root Git named keeps the space in its name"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

/// A checkout whose separate Git directory is another folder, where that folder's name ends in a
/// space: the answer keeps the directory Git named, so the watch the catalog keeps covers the files
/// Git read instead of a folder beside them that holds neither. A trimmed name leaves the checkout
/// unrecognisable: no watch covers it, and every catalog read spawns Git again.
#[test]
fn a_separate_git_directory_ending_in_a_space_is_watched_where_git_names_it() {
    let dir = TempDir::new();
    let checkout = dir.path().join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    let git_dir = space_ending_folder(&dir.path().join("gitdir "));
    std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::write(git_dir.join("config"), "[core]\n").unwrap();
    for name in ["objects", "refs"] {
        std::fs::create_dir_all(git_dir.join(name)).unwrap();
    }
    let answer =
        super::Answer::read(format!("{}\n{}\n", checkout.display(), git_dir.display()).as_bytes())
            .unwrap();
    // The discovery asked again after the watch is taken answers with the same directory, which is
    // what keeps the observation: a trimmed name names no folder, so `names` cannot canonicalize it
    // and `observe` gives up, leaving a valid checkout incomplete on every catalog read.
    let again =
        super::Answer::read(format!("{}\n{}\n", checkout.display(), git_dir.display()).as_bytes())
            .unwrap();
    assert!(
        again.names(&answer),
        "the second discovery names the same checkout"
    );
    assert_eq!(
        answer.git_dir.as_deref(),
        Some(git_dir.as_path()),
        "the Git directory Git named keeps the space in its name"
    );
    let real = crate::util::canonicalize(&checkout).unwrap();
    assert!(
        super::git_facts::Watch::work_tree(&real, &real, &git_dir)
            .is_some_and(|watch| watch.holds()),
        "the watch the catalog keeps covers the directory Git named, not the name beside it"
    );
    std::fs::remove_dir_all(&git_dir).unwrap();
}
