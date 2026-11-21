//! Finds the engine host binary beside the app, if this build ships one. A build for a platform with no
//! host stages an empty placeholder (Tauri needs every `externalBin` file to exist), and the service
//! then keeps using the Rust drivers.

use std::path::{Path, PathBuf};

pub(crate) const ENV: &str = "ORBYN_ENGINE_HOST";

/// The host's path when it is a real, non-empty file next to `app_dir`'s sidecars.
pub(crate) fn beside(app_dir: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "orbyn-engine-host.exe"
    } else {
        "orbyn-engine-host"
    };
    let path = app_dir.join(name);
    let meta = std::fs::metadata(&path).ok()?;
    (meta.is_file() && meta.len() > 0).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("orbyn-engine-host-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn file_name() -> &'static str {
        if cfg!(windows) { "orbyn-engine-host.exe" } else { "orbyn-engine-host" }
    }

    #[test]
    fn a_missing_host_is_none() {
        let dir = temp("missing");
        assert_eq!(beside(&dir), None);
    }

    #[test]
    fn an_empty_placeholder_is_none() {
        let dir = temp("empty");
        std::fs::write(dir.join(file_name()), b"").unwrap();
        assert_eq!(beside(&dir), None);
    }

    #[test]
    fn a_real_file_is_found() {
        let dir = temp("real");
        std::fs::write(dir.join(file_name()), b"x").unwrap();
        assert_eq!(beside(&dir), Some(dir.join(file_name())));
    }

    #[test]
    fn a_folder_of_that_name_is_none() {
        let dir = temp("folder");
        std::fs::create_dir(dir.join(file_name())).unwrap();
        assert_eq!(beside(&dir), None);
    }
}
