#![forbid(unsafe_code)]

#[tauri::command]
fn core_status() -> String {
    blcvoice_core::status_line()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![core_status])
        .run(tauri::generate_context!())
        .expect("failed to run BLCVoice desktop shell");
}
