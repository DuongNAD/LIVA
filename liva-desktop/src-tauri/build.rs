fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "toggle_ghost_mode",
            "set_eco_mode",
            "update_interactive_zones",
            "open_dashboard",
            "open_setup",
            "vault_secret_present",
            "store_vault_secret",
            "delete_vault_secret",
            "issue_websocket_session",
            "native_ipc_call",
            "native_ipc_call_stream",
        ]),
    ))
    .expect("failed to build Tauri application manifest")
}
