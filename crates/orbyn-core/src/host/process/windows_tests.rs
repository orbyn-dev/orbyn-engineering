use super::*;

const FIXTURE: &str = "ORBYN_RELATIVE_PATH_FIXTURE";

// Re-execution keeps cwd and PATH changes out of the parallel test process.
#[test]
fn resolver_fixture() {
    let Ok(mode) = std::env::var(FIXTURE) else {
        return;
    };
    let root = std::env::current_dir().unwrap();
    let relative = PathBuf::from("bin with spaces");
    let bin = if mode == "absolute" {
        root.join(&relative)
    } else {
        relative
    };
    let path = std::env::join_paths([bin]).unwrap();
    let command = resolve("demo.cmd").unwrap();
    let args = ["space value", "& echo injected", "quote\"value", "%PATH%"];
    let output = std::process::Command::new(&command.program)
        .args(&command.prefix)
        .args(args)
        .current_dir(root.join("other cwd"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(actual, args);
    assert!(command.path.is_absolute());
    assert!(command.program.is_absolute());
    assert!(Path::new(&command.prefix[0]).is_absolute());

    let node_file = if mode == "fallback" {
        "node host/node.exe"
    } else {
        "bin with spaces/node.exe"
    };
    for name in if mode == "absolute" {
        vec![root.join(node_file), root.join("bin with spaces/check.ps1")]
    } else {
        vec![
            PathBuf::from(node_file),
            PathBuf::from("bin with spaces/check.ps1"),
        ]
    } {
        let found = find_on_path(name.to_str().unwrap(), Some(&path)).unwrap();
        assert!(found.is_absolute(), "{found:?}");
        let resolved = shim_target(&found).unwrap();
        assert!(resolved.path.is_absolute());
        if name.extension().is_some_and(|ext| ext == "ps1") {
            assert!(Path::new(resolved.prefix.last().unwrap()).is_absolute());
        }
    }
    let unsupported = find_on_path("unsupported.cmd", Some(&path)).unwrap();
    assert!(shim_target(&unsupported).is_none());
}

#[test]
fn relative_path_shim_survives_a_child_cwd_change() {
    run_fixture("relative");
}

#[test]
fn absolute_path_shim_survives_a_child_cwd_change() {
    run_fixture("absolute");
}

#[test]
fn relative_path_node_fallback_survives_a_child_cwd_change() {
    run_fixture("fallback");
}

fn run_fixture(mode: &str) {
    let dir = crate::testing::TempDir::new();
    let bin = dir.path().join("bin with spaces");
    std::fs::create_dir_all(bin.join("node_modules/demo")).unwrap();
    std::fs::create_dir(dir.path().join("other cwd")).unwrap();
    // Only an owned argument-echo fixture runs; no provider CLI is invoked.
    let node = resolve("node").expect("Windows resolver regression requires Node.js");
    let node_host = dir.path().join("node host");
    std::fs::create_dir(&node_host).unwrap();
    let node_dir = if mode == "fallback" { &node_host } else { &bin };
    std::fs::copy(node.program, node_dir.join("node.exe")).unwrap();
    let mut search = if mode == "absolute" {
        vec![bin.clone(), node_host]
    } else {
        vec![PathBuf::from("bin with spaces"), PathBuf::from("node host")]
    };
    search.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    std::fs::write(
        bin.join("demo.cmd"),
        "@echo off\r\n\"%~dp0\\node.exe\" \"%~dp0\\node_modules\\demo\\cli.js\" %*\r\n",
    )
    .unwrap();
    std::fs::write(
        bin.join("node_modules/demo/cli.js"),
        "process.stdout.write(JSON.stringify(process.argv.slice(2)))",
    )
    .unwrap();
    std::fs::write(bin.join("check.ps1"), "Write-Output fixture").unwrap();
    std::fs::write(
        bin.join("unsupported.cmd"),
        "@echo off\r\nunknown.exe %*\r\n",
    )
    .unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "host::process::windows_tests::resolver_fixture",
            "--nocapture",
        ])
        .env(FIXTURE, mode)
        .env("PATH", std::env::join_paths(search).unwrap())
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
