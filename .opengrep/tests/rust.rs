use std::process::Command;
fn spawn(cmd: &str, db: &Db, name: &str) {
    // ruleid: orbyn-rust-command-nonliteral
    let _ = Command::new(cmd);
    // ok: orbyn-rust-command-nonliteral
    let _ = Command::new("git");
    // ruleid: orbyn-rust-shell-spawn
    let _ = Command::new("sh");
    // ok: orbyn-rust-shell-spawn
    let _ = Command::new("git");
    // ruleid: orbyn-rust-sql-format
    db.execute(&format!("DELETE FROM t WHERE n = '{}'", name), []);
    // ok: orbyn-rust-sql-format
    db.execute("DELETE FROM t WHERE n = ?1", [name]);
    // ruleid: orbyn-rust-tls-verify-off
    let _ = client.danger_accept_invalid_certs(true);
    // ok: orbyn-rust-tls-verify-off
    let _ = client.danger_accept_invalid_certs(false);
    // ruleid: orbyn-rust-unsafe-block
    unsafe { libc_call() };
}

fn serve(request: &Request, root: &Path) {
    let path = request.uri().path().to_owned();
    // ruleid: orbyn-rust-request-path-to-fs
    let _ = std::fs::read(&path);
    // ok: orbyn-rust-request-path-to-fs
    let confined = resolve(root, &path).unwrap_or_default();
    // ok: orbyn-rust-request-path-to-fs
    let _ = std::fs::read(&confined);
    // ok: orbyn-rust-request-path-to-fs
    let _ = std::fs::read(root.join("index.html"));
    // ok: orbyn-rust-request-path-to-fs
    let _ = std::fs::read("static/index.html");
}

fn sql(db: &Db, name: &str) {
    let built = format!("DELETE FROM t WHERE n = '{}'", name);
    // ruleid: orbyn-rust-sql-built-query-reaches-execute
    db.execute(&built, []);
    // ok: orbyn-rust-sql-built-query-reaches-execute
    db.execute("DELETE FROM t WHERE n = ?1", [name]);
}

struct Cred {
    token: String,
    name: String,
}

fn audit(cred: &Cred, secret: &str) {
    let shown = cred.token.clone();
    // ruleid: orbyn-rust-secret-field-in-log
    tracing::info!("signed in with {}", shown);
    // ok: orbyn-rust-secret-field-in-log
    tracing::info!("signed in as {}", cred.name);
    // ok: orbyn-rust-secret-field-in-log
    tracing::info!("token length {}", cred.token.len());
    // ok: orbyn-rust-secret-field-in-log
    tracing::info!("redacted {}", redact(secret));
}

fn log(token: &str, name: &str, count: usize, api_key: String) {
    // ruleid: orbyn-rust-secret-named-value-in-log
    println!("signed in with {}", token);
    // ruleid: orbyn-rust-secret-named-value-in-log
    tracing::info!("x {}", token);
    // ruleid: orbyn-rust-secret-named-value-in-log
    warn!("rotating {}", &api_key);
    // ruleid: orbyn-rust-secret-named-value-in-log
    dbg!(token);
    // ok: orbyn-rust-secret-named-value-in-log
    println!("signed in as {}", name);
    // ok: orbyn-rust-secret-named-value-in-log
    println!("{} tokens counted", count);
    // ok: orbyn-rust-secret-named-value-in-log
    tracing::info!("redacted {}", redact(token));
}
