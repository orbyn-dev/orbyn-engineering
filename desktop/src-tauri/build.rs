fn main() {
    println!("cargo:rerun-if-env-changed=ORBYN_UPDATE_PUBKEY");
    println!("cargo:rerun-if-env-changed=ORBYN_UPDATE_ENDPOINT");
    // The main window capability grants these commands. Each also checks the current
    // document: startup gets retry/log/quit; product gets presence/person_key/pick_folder.
    let manifest = tauri_build::AppManifest::new().commands(&[
        "restart_core",
        "show_core_log",
        "quit_core",
        "crew_presence",
        "person_key",
        "pick_folder",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("run tauri-build");
}
