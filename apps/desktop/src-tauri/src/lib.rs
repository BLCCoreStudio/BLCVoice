#![forbid(unsafe_code)]

mod capture;
mod ipc;
mod shortcut;

use std::sync::Arc;

use ipc::{
    DesktopState, audio_input_discovery, desktop_status, microphone_test_cancel,
    microphone_test_finish, microphone_test_start,
};
use shortcut::{
    ShortcutState, global_shortcut_set_mode, global_shortcut_status, install_global_shortcut,
};

#[tauri::command]
fn core_status() -> String {
    blcvoice_core::status_line()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let shortcut_state = ShortcutState::production();
    let shortcut_service = shortcut_state.service();

    tauri::Builder::default()
        .manage(DesktopState::production())
        .manage(shortcut_state)
        .setup(move |app| {
            install_global_shortcut(app, Arc::clone(&shortcut_service));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            core_status,
            audio_input_discovery,
            desktop_status,
            microphone_test_start,
            microphone_test_finish,
            microphone_test_cancel,
            global_shortcut_status,
            global_shortcut_set_mode,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run BLCVoice desktop shell");
}
