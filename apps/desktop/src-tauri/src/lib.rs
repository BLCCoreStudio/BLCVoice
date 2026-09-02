#![forbid(unsafe_code)]

mod capture;
mod ipc;
mod shortcut;
mod shortcut_ipc;

use std::sync::Arc;

use ipc::{
    DesktopState, audio_input_discovery, desktop_status, microphone_test_cancel,
    microphone_test_finish, microphone_test_start,
};
use shortcut::DesktopShortcutService;
use shortcut_ipc::shortcut_status;

#[tauri::command]
fn core_status() -> String {
    blcvoice_core::status_line()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let shortcut_service = Arc::new(DesktopShortcutService::production());
    let setup_shortcut_service = Arc::clone(&shortcut_service);

    tauri::Builder::default()
        .manage(DesktopState::production())
        .manage(shortcut_service)
        .setup(move |app| {
            shortcut::install_backend(app, Arc::clone(&setup_shortcut_service));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            core_status,
            audio_input_discovery,
            desktop_status,
            shortcut_status,
            microphone_test_start,
            microphone_test_finish,
            microphone_test_cancel,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run BLCVoice desktop shell");
}
