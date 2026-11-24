fn main() {
    println!("cargo:rerun-if-env-changed=ORBYN_UPDATE_PUBKEY");
    println!("cargo:rerun-if-env-changed=ORBYN_UPDATE_ENDPOINT");
    // The capture crate links Swift libraries that live in the system runtime folder. A dependency's own link
    // arguments do not reach this binary, so name the folder here or the app and its tests abort at launch.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
    // The main window capability grants these commands. Each also checks the current
    // document: startup gets retry/log/quit; product gets presence/person_key/pick_folder/snapshot.
    let manifest = tauri_build::AppManifest::new().commands(&[
        "restart_core",
        "show_core_log",
        "quit_core",
        "crew_presence",
        "person_key",
        "pick_folder",
        "snapshot_state",
        "snapshot_configure",
        "snapshot_capture",
        "snapshot_take",
        "snapshot_discard",
        "snapshot_setup",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("run tauri-build");
}
