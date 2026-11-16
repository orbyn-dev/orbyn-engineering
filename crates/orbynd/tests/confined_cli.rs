//! A crew worker's process cannot find the service from the data folder: the same
//! `orbyn status` that reaches a stand-in service unrestricted fails under the worker's rules.
#![cfg(target_os = "macos")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;

use orbyn_core::host::confine::Confinement;
use orbyn_core::host::process::ResolvedCommand;

const ORBYN: &str = env!("CARGO_BIN_EXE_orbyn");

#[test]
fn a_confined_orbyn_status_cannot_find_the_service() {
    // Not canonicalised on purpose: the temp folder is behind the `/var` link.
    let base = std::env::temp_dir().join(format!("orbyn-confined-cli-{}", std::process::id()));
    let data = base.join("data");
    std::fs::create_dir_all(data.join("runs/mine")).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::fs::write(
        data.join("service.json"),
        format!(r#"{{"url":"http://127.0.0.1:{port}","token":"t","pid":1}}"#),
    )
    .unwrap();
    // Answers every request with an empty object.
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut buffer = [0; 4096];
            let _ = stream.read(&mut buffer);
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
            );
        }
    });

    let status = |wrapped: Option<&Confinement>| {
        let mut command = ResolvedCommand {
            program: ORBYN.into(),
            prefix: Vec::new(),
            path: ORBYN.into(),
        };
        if let Some(rules) = wrapped {
            rules.wrap(&mut command).unwrap();
        }
        Command::new(&command.program)
            .args(&command.prefix)
            .args(["--data-dir"])
            .arg(&data)
            .arg("status")
            .env_remove("ORBYN_DATA_DIR")
            .output()
            .unwrap()
    };

    let open = status(None);
    assert!(open.status.success(), "unrestricted: {open:?}");
    let rules = Confinement::new(&data, &data.join("runs/mine"), &[], &[]).unwrap();
    let shut = status(Some(&rules));
    assert!(!shut.status.success(), "confined: {shut:?}");
    let _ = std::fs::remove_dir_all(&base);
}
