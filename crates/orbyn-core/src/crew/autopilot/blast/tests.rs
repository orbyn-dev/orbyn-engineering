//! The blast-radius guard's reading of commands: categories, targets, classification, SQL, git and ssh forms.

use orbyn_contracts::ActionKind;

use super::super::hard::{Scene, context};
use super::*;

fn entry(list: GuardList, kind: GuardTargetKind, pattern: &str) -> GuardTarget {
    GuardTarget {
        id: format!("gt-{pattern}"),
        list,
        pattern: pattern.to_owned(),
        kind,
        note: None,
        added_at: "2026-01-01T00:00:00Z".to_owned(),
    }
}

fn remotes() -> HashMap<String, Vec<String>> {
    [
        ("origin", "git@github.com:orbyn-dev/app.git"),
        ("mirror", "/srv/git/app.git"),
        ("self", "file:///srv/git/app.git"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), vec![v.to_owned()]))
    .collect()
}

fn facts(entries: Vec<GuardTarget>) -> Facts {
    Facts::new(entries, remotes())
}

fn look(command: &str, facts: &Facts) -> Option<Found> {
    let scene = Scene {
        worktree: Path::new("/work/tree"),
        protected: &[],
        allowed_hosts: &[],
        data_dir: None,
        branch: None,
        default_branch: None,
    };
    let ctx = context(ActionKind::Command, command, &[], &scene);
    inspect(&ctx, facts)
}

fn guarded(command: &str) -> bool {
    look(command, &facts(Vec::new())).is_some_and(|f| f.guarded())
}

fn category(command: &str) -> Option<GuardCategory> {
    look(command, &facts(Vec::new())).map(|f| f.category)
}

#[test]
fn each_category_is_found_in_a_command_for_a_remote_target() {
    use GuardCategory::*;
    let table = [
        ("ssh prod.example.com 'rm -rf /srv/data'", FileDeletion),
        ("ssh deploy@box rm -rf /srv/data", FileDeletion),
        (
            "rsync -a --delete ./out/ deploy@prod.example.com:/srv/www/",
            FileDeletion,
        ),
        (
            "ssh prod.example.com 'find /srv -name x -delete'",
            FileDeletion,
        ),
        ("ssh prod.example.com shred /srv/key", FileDeletion),
        (
            "ssh prod.example.com truncate -s 0 /var/log/x",
            FileDeletion,
        ),
        (
            "psql -h db.prod.example.com -c 'DROP TABLE users'",
            Database,
        ),
        ("mysql -h db.example.com -e 'TRUNCATE users'", Database),
        ("mysql -h db.example.com -e 'DELETE FROM users'", Database),
        (
            "psql postgres://u:pw@db.example.com/app -c 'UPDATE users SET x = 1'",
            Database,
        ),
        ("redis-cli -h cache.example.com FLUSHALL", Database),
        (
            "mongosh mongodb://db.example.com/app --eval 'db.dropDatabase()'",
            Database,
        ),
        ("cqlsh db.example.com -e 'DROP KEYSPACE app'", Database),
        ("aws s3 rm s3://logs --recursive --profile prod", Cloud),
        ("aws s3 rb s3://logs --profile prod", Cloud),
        (
            "aws ec2 terminate-instances --instance-ids i-1 --profile prod",
            Cloud,
        ),
        ("aws rds delete-db-instance --profile prod", Cloud),
        ("gcloud compute instances delete vm --project acme", Cloud),
        ("az group delete --name rg --subscription sub-1", Cloud),
        ("gsutil -m rm -r gs://bucket/x", Cloud),
        ("docker -H tcp://10.0.0.5:2376 rm -f web", Containers),
        ("docker --context prod system prune -af", Containers),
        ("git push --force origin feature", Git),
        ("git push origin --delete feature", Git),
        ("git push origin +feature", Git),
        ("git push -f origin feature", Git),
    ];
    for (command, want) in table {
        assert_eq!(category(command), Some(want), "{command}");
        assert!(guarded(command), "{command}");
    }
}

#[test]
fn a_command_that_is_not_destructive_is_not_read() {
    for command in [
        "ls -la",
        "rm file.txt",
        "psql -h db.example.com -c 'SELECT 1'",
        "psql -h db.example.com -c 'UPDATE t SET x = 1 WHERE id = 3'",
        "aws s3 ls s3://logs --profile prod",
        "git push origin feature",
        "docker ps",
        "docker -H tcp://10.0.0.5:2376 ps",
        "cargo test",
    ] {
        assert_eq!(look(command, &facts(Vec::new())), None, "{command}");
    }
}

#[test]
fn a_variable_or_a_missing_target_is_guarded() {
    for command in [
        "psql -h $DB_HOST -c 'DROP TABLE x'",
        "psql -c 'DROP TABLE x'",
        "redis-cli FLUSHALL",
        "mysql -e 'DELETE FROM t'",
        "aws s3 rm s3://bucket --recursive",
        "aws ec2 terminate-instances --instance-ids i-1",
        "gcloud compute instances delete vm",
        "ssh $TARGET 'rm -rf /srv'",
        "ssh \"$TARGET\" rm -rf /srv",
        "git push --force",
        "git push --force nowhere",
        "docker --context $CTX rm x",
        "docker -H $HOST rm x",
    ] {
        assert!(guarded(command), "{command}");
    }
    let found = look("psql -c 'DROP TABLE x'", &facts(Vec::new())).unwrap();
    assert_eq!(found.targets[0].class, Class::Unresolved);
    assert_eq!(found.targets[0].name, "not named");
}

#[test]
fn targets_come_from_env_prefixes_urls_and_flags() {
    let f = |c: &str| look(c, &facts(Vec::new())).unwrap();
    let found = f("PGHOST=db.example.com psql -c 'DROP TABLE x'");
    assert_eq!(found.targets[0].name, "db.example.com");
    let found = f("DATABASE_URL=postgres://u:pw@db.example.com:5432/app psql -c 'DROP TABLE x'");
    assert_eq!(found.targets[0].name, "db.example.com");
    assert!(!found.shown_target().contains("pw"));
    let found = f("AWS_PROFILE=prod aws s3 rb s3://x");
    assert!(
        found
            .targets
            .iter()
            .any(|t| t.name == "prod" && t.kind == GuardTargetKind::Profile)
    );
    let found = f("docker --context staging rm x");
    assert_eq!(found.targets[0].kind, GuardTargetKind::Context);
    let found = f("gcloud compute instances delete vm --project=acme-prod");
    assert!(found.targets.iter().any(|t| t.name == "acme-prod"));
    let found = f("git push -f origin main");
    assert_eq!(found.targets[0].name, "github.com");
}

#[test]
fn local_targets_are_not_guarded() {
    for command in [
        "psql -h localhost -c 'DROP TABLE x'",
        "psql -h 127.0.0.1 -c 'DROP TABLE x'",
        "psql -h ::1 -c 'DROP TABLE x'",
        "psql -h /var/run/postgresql -c 'DROP TABLE x'",
        "psql postgres:///app -c 'DROP TABLE x'",
        "PGHOST=localhost psql -c 'DROP TABLE x'",
        "mysql -S /tmp/mysql.sock -e 'DROP TABLE x'",
        "redis-cli -h 127.0.0.1 FLUSHALL",
        "rm -rf build",
        "rm -rf /work/tree/target",
        "find . -name '*.tmp' -delete",
        "rsync -a --delete src/ dest/",
        "docker rm web",
        "docker system prune -af",
        "docker --context default rm web",
        "docker -H unix:///var/run/docker.sock rm web",
        "docker -H tcp://127.0.0.1:2375 rm web",
        "git push --force mirror feature",
        "git push --force self feature",
        "git push --force /srv/git/app.git feature",
        "git push --force ../mirror.git feature",
        "aws s3 rm s3://b --recursive --endpoint-url http://localhost:4566",
    ] {
        let found = look(command, &facts(Vec::new()));
        assert!(
            found.is_some_and(|f| !f.guarded()),
            "{command} should be read, and local"
        );
    }
}

#[test]
fn the_safe_list_exempts_and_the_prod_list_forces_and_prod_wins() {
    use GuardList::*;
    use GuardTargetKind::*;
    let safe = facts(vec![entry(Safe, Host, "*.staging.example.com")]);
    let command = "psql -h db.staging.example.com -c 'DROP TABLE x'";
    let found = look(command, &safe).unwrap();
    assert!(!found.guarded());
    assert!(matches!(found.targets[0].class, Class::Safe(_)));
    assert_eq!(
        found.entry().map(|e| e.pattern.as_str()),
        Some("*.staging.example.com")
    );
    // The exemption is for the listed host only.
    assert!(
        look("psql -h db.example.com -c 'DROP TABLE x'", &safe)
            .unwrap()
            .guarded()
    );
    assert!(
        look(
            "psql -h staging.example.com.evil.io -c 'DROP TABLE x'",
            &safe
        )
        .unwrap()
        .guarded()
    );
    // A kind must match: a profile entry does not exempt a host.
    let wrong = facts(vec![entry(Safe, Profile, "db.staging.example.com")]);
    assert!(look(command, &wrong).unwrap().guarded());
    // The prod list forces, even for a name that looks local.
    let forced = facts(vec![entry(Prod, Host, "localhost")]);
    assert!(
        look("psql -h localhost -c 'DROP TABLE x'", &forced)
            .unwrap()
            .guarded()
    );
    // Prod wins when a target is in both.
    let both = facts(vec![
        entry(Safe, Host, "db.staging.example.com"),
        entry(Prod, Host, "db.staging.example.com"),
    ]);
    let found = look(command, &both).unwrap();
    assert!(found.guarded());
    assert!(matches!(found.targets[0].class, Class::Prod(_)));
    let held = {
        let scene = Scene {
            worktree: Path::new("/work/tree"),
            protected: &[],
            allowed_hosts: &[],
            data_dir: None,
            branch: None,
            default_branch: None,
        };
        blast(&context(ActionKind::Command, command, &[], &scene), &both).unwrap()
    };
    assert_eq!(held.list, "prod");
}

#[test]
fn a_safe_profile_does_not_exempt_a_command_with_another_unlisted_target() {
    use GuardList::*;
    use GuardTargetKind::*;
    let safe = facts(vec![entry(Safe, Profile, "dev")]);
    assert!(
        !look("aws s3 rm s3://any --profile dev", &safe)
            .unwrap()
            .guarded()
    );
    // A bucket only describes the command, but a prod bucket forces it.
    let both = facts(vec![
        entry(Safe, Profile, "dev"),
        entry(Prod, Bucket, "customer-data"),
    ]);
    assert!(
        look("aws s3 rm s3://customer-data --profile dev", &both)
            .unwrap()
            .guarded()
    );
    // Two profiles, one safe: guarded.
    assert!(
        look("AWS_PROFILE=prod aws s3 rm s3://b --profile dev", &safe)
            .unwrap()
            .guarded()
    );
}

#[test]
fn an_unreadable_command_with_a_destructive_and_a_non_local_word_is_guarded() {
    assert_eq!(
        category("echo 'oops; rm -rf /srv user@prod.example.com"),
        Some(GuardCategory::Unparsed)
    );
    assert!(guarded("$CMD psql -h db.example.com DROP TABLE x"));
    // No non-local word, or no destructive word: not read.
    assert_eq!(look("echo 'oops; rm -rf build", &facts(Vec::new())), None);
    assert_eq!(
        look("echo 'oops prod.example.com", &facts(Vec::new())),
        None
    );
}

#[test]
fn a_sql_file_is_not_read_and_the_limit_is_documented() {
    // Known limit: what `-f` runs is not read. A file whose name holds a keyword is guarded all the same (it fails closed).
    assert_eq!(
        look("psql -h db.example.com -f migrate.sql", &facts(Vec::new())),
        None
    );
    assert!(guarded("psql -h db.example.com -f drop.sql"));
}

#[test]
fn stdin_queries_are_read() {
    assert!(guarded("echo 'DROP TABLE users' | psql -h db.example.com"));
    assert!(guarded(
        "psql -h db.example.com <<EOF\nTRUNCATE users;\nEOF"
    ));
}

#[test]
fn a_pattern_is_exact_or_a_suffix_and_never_a_look_alike() {
    assert!(pattern_matches("db.example.com", "DB.example.com"));
    assert!(pattern_matches("*.example.com", "a.b.example.com"));
    assert!(pattern_matches("*.example.com", "example.com"));
    assert!(!pattern_matches("*.example.com", "notexample.com"));
    assert!(!pattern_matches("*.example.com", "example.com.evil.io"));
    assert!(!pattern_matches("db.example.com", "xdb.example.com"));
}

#[test]
fn only_a_command_is_read() {
    let scene = Scene {
        worktree: Path::new("/work/tree"),
        protected: &[],
        allowed_hosts: &[],
        data_dir: None,
        branch: None,
        default_branch: None,
    };
    let ctx = context(ActionKind::Network, "psql -c 'DROP TABLE x'", &[], &scene);
    assert_eq!(inspect(&ctx, &facts(Vec::new())), None);
}

fn guarded_all(commands: &[&str]) {
    for command in commands {
        assert!(guarded(command), "{command} should be guarded");
    }
}

fn unguarded_all(commands: &[&str]) {
    for command in commands {
        let found = look(command, &facts(Vec::new()));
        assert!(
            found.as_ref().is_none_or(|f| !f.guarded()),
            "{command} should not be guarded: {found:?}"
        );
    }
}

#[test]
fn a_host_is_local_only_as_a_loopback_literal_and_every_host_of_a_list_must_be() {
    guarded_all(&[
        "psql -h 127.evil.com -c 'DROP TABLE x'",
        "psql -h 127.0.0.1.evil.com -c 'DROP TABLE x'",
        "psql -h localhost.evil.com -c 'DROP TABLE x'",
        "psql -h 0.0.0.0 -c 'DROP TABLE x'",
        "psql -h localhost,db.prod -c 'DROP TABLE x'",
        "psql -h 127.0.0.1,db.prod -c 'DROP TABLE x'",
        "psql postgresql://127.0.0.1:1,db.prod/app -c 'DROP TABLE x'",
        "psql postgresql://127.evil.com/app -c 'DROP TABLE x'",
        "psql --dbname=postgres://u@db.prod/app -c 'DROP TABLE x'",
        "psql -h localhost -d postgresql://db.prod/x -c 'DROP TABLE x'",
        "psql -h localhost -d 'host=db.prod dbname=x' -c 'DROP TABLE x'",
        "psql 'host=localhost,db.prod' -c 'DROP TABLE x'",
        "psql postgres:///app?host=db.prod -c 'DROP TABLE x'",
        "psql 'service=prod' -c 'DROP TABLE x'",
        "PGSERVICE=prod psql -h localhost -c 'DROP TABLE x'",
        "PGHOST=127.evil.com psql -c 'DROP TABLE x'",
        "docker -H tcp://127.evil.com:2375 rm x",
    ]);
    unguarded_all(&[
        "psql -h 127.1.2.3 -c 'DROP TABLE x'",
        "psql -h localhost,127.0.0.1 -c 'DROP TABLE x'",
        "psql -h ::1 -c 'DROP TABLE x'",
        "psql postgresql://127.0.0.1:1,localhost/app -c 'DROP TABLE x'",
        "psql -h localhost -d 'host=127.0.0.1 dbname=x' -c 'DROP TABLE x'",
        "psql -h localhost -d 'dbname=x' -c 'DROP TABLE x'",
    ]);
}

#[test]
fn sql_is_read_after_comments_whitespace_and_case_are_normalised() {
    guarded_all(&[
        "psql -h db.prod -c '/* x */ DROP TABLE t'",
        "psql -h db.prod -c 'DROP/**/TABLE t'",
        "psql -h db.prod -c 'drop   table t -- later'",
        "psql -h db.prod -c 'DELETE FROM t'",
        "psql -h db.prod -c 'DELETE u FROM users u'",
        "psql -h db.prod -c 'delete t'",
        "psql -h db.prod -c 'ALTER TABLE t DROP COLUMN c'",
        "psql -h db.prod -c 'TRUNCATE t'",
        "psql -h db.prod -c 'UPDATE users AS u SET x = 1'",
        "psql -h db.prod -c \"UPDATE t SET note = 'where'\"",
        "psql -h db.prod -c 'UPDATE t SET a = 1 -- WHERE id = 1'",
        "psql -h db.prod -c 'UPDATE t SET a = 1 /* WHERE id = 1 */'",
        "psql -h db.prod -c 'SELECT 1; DELETE FROM t'",
        "psql -h db.prod -c 'UPDATE t SET a = 1; UPDATE u SET b = 2 WHERE id = 1'",
        "psql -h db.prod -c 'DO $$ BEGIN DELETE FROM t; END $$'",
        "psql -h db.prod -c 'select 1' -c 'drop table t'",
        "echo 'DROP TABLE x' | psql -h db.prod -c 'SELECT 1'",
        "psql -h db.prod -f - <<EOF\nDELETE FROM t;\nEOF",
        "mysql -h db.prod -e 'delete from t where 1=1; drop table u'",
    ]);
    unguarded_all(&[
        "psql -h db.prod -c 'UPDATE t SET a = 1 WHERE id = 1'",
        "psql -h db.prod -c \"UPDATE t SET a = 1 WHERE note = 'x'\"",
        "psql -h db.prod -c 'SELECT 1'",
        "psql -h db.prod -c \"SELECT 'drop table t'\"",
        "psql -h db.prod -c 'CREATE TABLE t (a int REFERENCES u ON DELETE CASCADE)'",
        "psql -h drop.example.com -c 'SELECT 1'",
        "psql -h db.prod -c \"SELECT 'unterminated\"",
    ]);
}

#[test]
fn a_git_push_is_read_against_every_push_url_and_anything_that_could_redirect_it() {
    let mut remotes = remotes();
    remotes.insert(
        "both".into(),
        vec!["/srv/git/app.git".into(), "git@github.com:o/r.git".into()],
    );
    remotes.insert("locals".into(), vec!["/a.git".into(), "/b.git".into()]);
    let facts = Facts::new(Vec::new(), remotes);
    let check = |command: &str| look(command, &facts).is_some_and(|f| f.guarded());
    // Any push URL that is not local guards the push.
    assert!(check("git push --force both feature"));
    assert!(!check("git push --force locals feature"));
    assert!(!check("git push --delete mirror old"));
    for command in [
        "git push origin :old",
        "git push :old",
        "git push --force-with-lease origin x",
        "GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=remote.mirror.pushurl GIT_CONFIG_VALUE_0=git@x.io:a.git git push --force mirror x",
        "GIT_CONFIG_PARAMETERS=remote.mirror.pushurl=x git push -f mirror x",
        "git -c remote.mirror.pushurl=git@x.io:a.git push --force mirror x",
        "git -cremote.mirror.pushurl=x push --force mirror x",
        "git -C ../other push -f mirror x",
        "git --git-dir=/x push -f mirror x",
        "GIT_DIR=/x git push -f mirror x",
    ] {
        assert!(check(command), "{command}");
    }
}

#[test]
fn ssh_options_their_values_and_wrappers_do_not_hide_the_host() {
    guarded_all(&[
        "ssh -B eth0 prod.internal 'rm -rf /srv/x'",
        "ssh -p 22 -i key.pem -o StrictHostKeyChecking=no prod.internal rm -rf x",
        "ssh -l root -p22 prod.internal rm -rf x",
        "ssh -vvp 22 prod.internal rm -rf x",
        "ssh -- prod.internal rm -rf x",
        "ssh -E log -F cfg -J jump -L 1:h:2 -m hmac -O check -P tag -Q cipher -R 1:h:2 -S sock -W h:1 -w 0:0 -b addr -c aes -D 1080 -e none -I lib prod.internal rm -rf x",
        "sudo -u root ssh prod.internal rm -rf x",
        "sudo -h box -g wheel ssh prod.internal rm -rf x",
        "xargs -I {} ssh {} rm -rf x",
        "ssh prod.internal find /srv -name x -exec rm {} \\;",
        "ssh prod.internal find /srv -delete",
        "ssh prod.internal 'if true; then rm -rf /srv; fi'",
        "ssh prod.internal \"$CMD\"",
    ]);
    unguarded_all(&[
        "ssh localhost 'if true; then rm -rf /srv; fi'",
        "ssh -p 22 127.0.0.1 rm -rf x",
        "find . -name x -exec rm {} \\;",
        "find . -delete",
        "sudo -u root rm -rf build",
        "ssh prod.internal ls /srv",
    ]);
    let found = look(
        "sudo -u root ssh prod.internal rm -rf x",
        &facts(Vec::new()),
    )
    .unwrap();
    assert_eq!(found.targets[0].name, "prod.internal");
}

#[test]
fn docker_env_prefixes_and_more_destructive_forms_are_read() {
    guarded_all(&[
        "DOCKER_HOST=tcp://10.0.0.5:2376 docker rm x",
        "DOCKER_CONTEXT=prod docker rm x",
        "docker --context prod service rm web",
        "docker -H ssh://deploy@prod.internal volume rm v",
        "docker --context prod stack rm web",
    ]);
    unguarded_all(&[
        "docker stack rm web",
        "docker service rm web",
        "docker volume rm v",
        "DOCKER_HOST=unix:///var/run/docker.sock docker rm x",
        "DOCKER_CONTEXT=default docker rm x",
    ]);
}

#[test]
fn aws_global_options_do_not_hide_the_command_and_static_keys_override_a_safe_profile() {
    use GuardList::*;
    use GuardTargetKind::*;
    let safe = facts(vec![entry(Safe, Profile, "dev")]);
    let check = |command: &str| look(command, &safe).unwrap().guarded();
    assert!(!check(
        "aws --cli-binary-format raw-in-base64-out --profile dev s3 rm s3://b"
    ));
    assert!(!check(
        "aws --color off --query x --output json --region r --cli-read-timeout 1 --cli-connect-timeout 1 --ca-bundle b.pem --profile dev ec2 terminate-instances --instance-ids i"
    ));
    for command in [
        "AWS_ACCESS_KEY_ID=k aws s3 rm s3://b --profile dev",
        "AWS_SESSION_TOKEN=t aws s3 rm s3://b --profile dev",
        "AWS_SECRET_ACCESS_KEY=s aws s3 rm s3://b --profile dev",
        "AWS_ENDPOINT_URL=http://localhost:4566 aws s3 rm s3://b --profile dev",
        "AWS_ENDPOINT_URL_S3=http://x aws s3 rm s3://b --profile dev",
    ] {
        assert!(check(command), "{command}");
    }
}

#[test]
fn more_destructive_forms_are_read() {
    guarded_all(&[
        "ssh prod.internal rsync -a --del ./x/ /srv/",
        "rsync -a --del ./x/ u@prod.example.com:/srv/",
        "rsync -a --remove-source-files ./x/ u@prod.example.com:/srv/",
        "gsutil rb gs://b",
        "gsutil -m rsync -d -r a gs://b",
        "az storage blob delete-batch --account-name a --source c",
    ]);
}

#[test]
fn an_approval_is_bound_to_the_resolved_targets() {
    let mut remotes = remotes();
    let at = |url: &str, remotes: &HashMap<String, Vec<String>>| {
        let facts = Facts::new(Vec::new(), remotes.clone());
        let _ = url;
        look("git push origin :old", &facts).unwrap().resolved()
    };
    let before = at("", &remotes);
    remotes.insert("origin".into(), vec!["git@github.com:o/other.git".into()]);
    let same_host_other_repo = at("", &remotes);
    remotes.insert("origin".into(), vec!["git@evil.example:o/r.git".into()]);
    let other_host = at("", &remotes);
    assert_ne!(
        before, same_host_other_repo,
        "the URL is part of the target"
    );
    assert_ne!(before, other_host);
    assert_eq!(before, at("", &super::tests::remotes()));
}
