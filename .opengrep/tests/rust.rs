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
