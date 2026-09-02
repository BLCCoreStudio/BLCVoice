#![forbid(unsafe_code)]

mod capture;
mod ipc;
mod shortcut;

use ipc::{
    DesktopState, audio_input_discovery, desktop_status, microphone_test_cancel,
    microphone_test_finish, microphone_test_start,
};
use shortcut::shortcut_capability;

#[tauri::command]
fn core_status() -> String {
    blcvoice_core::status_line()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(DesktopState::production())
        .invoke_handler(tauri::generate_handler![
            core_status,
            audio_input_discovery,
            desktop_status,
            microphone_test_start,
            microphone_test_finish,
            microphone_test_cancel,
            shortcut_capability,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run BLCVoice desktop shell");
}
