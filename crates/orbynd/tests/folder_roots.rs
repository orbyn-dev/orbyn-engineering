//! The folder browser's window over HTTP. Nothing is above a filesystem root, so
//! a root also offers the mounted drives; otherwise the picker is stuck on the
//! drive it started on. A folder that is not a root never grows a drive row.

mod support;

use std::path::Path;

use serde_json::Value;
use support::{Options, Service, TempDir};

async fn browse(service: &Service, path: &Path) -> Value {
    let response = service
        .get("/api/fs/browse")
        .query(&[("path", path.to_str().expect("a test path is UTF-8"))])
        .send()
        .await
        .expect("browse answers");
    assert!(response.status().is_success(), "{}", response.status());
    response.json().await.expect("browse answers JSON")
}

fn paths(listing: &Value) -> Vec<String> {
    listing["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| entry["path"].as_str().expect("entry path").to_owned())
        .collect()
}

#[tokio::test]
async fn a_folder_offers_only_its_folders() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), Options::default());
    std::fs::create_dir_all(dir.path().join("root/plain")).unwrap();
    std::fs::create_dir_all(dir.path().join("root/.hidden")).unwrap();
    std::fs::write(dir.path().join("root/file.txt"), "x").unwrap();

    let root = dir.path().join("root");
    let listing = browse(&service, &root).await;
    assert_eq!(listing["path"], root.to_str().unwrap());
    assert_eq!(listing["parent"], dir.path().to_str().unwrap());
    // A folder below a root lists folders only: no drive row, no file, no dot folder.
    assert_eq!(
        paths(&listing),
        [root.join("plain").to_string_lossy().into_owned()]
    );
}

#[cfg(windows)]
#[tokio::test]
async fn a_drive_root_offers_the_other_mounted_drives() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), Options::default());
    let current = dir
        .path()
        .ancestors()
        .last()
        .expect("the temporary folder is under a drive")
        .to_path_buf();

    let listing = browse(&service, &current).await;
    assert_eq!(listing["path"], current.to_str().unwrap());
    assert_eq!(listing["parent"], Value::Null);

    // What this machine has that the current drive is not, and that can be read:
    // an empty card reader is no way in.
    let expected: Vec<String> = orbyn_core::util::drive_roots()
        .into_iter()
        .filter(|drive| drive != &current && std::fs::read_dir(drive).is_ok())
        .map(|drive| drive.to_string_lossy().into_owned())
        .collect();
    let rows = paths(&listing);
    assert!(
        rows.starts_with(&expected),
        "{rows:?} does not start with the mounted drives {expected:?}"
    );
    assert!(
        !rows.contains(&current.to_string_lossy().into_owned()),
        "the root being browsed is not one of its own rows"
    );
    let names: Vec<String> = listing["entries"]
        .as_array()
        .unwrap()
        .iter()
        .take(expected.len())
        .map(|entry| entry["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, expected, "a drive row is named by its whole path");

    // The point of the row: the picker can open the drive it names.
    for drive in &expected {
        let inside = browse(&service, Path::new(drive)).await;
        assert_eq!(inside["path"], drive.as_str());
    }
}

#[cfg(unix)]
#[tokio::test]
async fn one_unix_tree_offers_no_drive_at_its_root() {
    let dir = TempDir::new();
    let service = Service::start(&dir.path().join("data"), Options::default());

    let listing = browse(&service, Path::new("/")).await;
    assert_eq!(listing["path"], "/");
    assert_eq!(listing["parent"], Value::Null);
    assert!(
        !paths(&listing)
            .iter()
            .any(|path| path.len() == 3 && path.ends_with(":\\")),
        "a Unix root has no drive to switch to"
    );
}
