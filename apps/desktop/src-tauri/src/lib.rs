#![forbid(unsafe_code)]

mod capture;
mod coordinator;
mod dictation;
mod insertion;
mod ipc;
mod models;
mod settings;
mod shortcut;

use coordinator::ShortcutDictationCoordinator;
use ipc::{
    DesktopState, audio_input_discovery, desktop_status, dictation_cancel, dictation_finish,
    dictation_start, dictation_start_configured, insertion_capability, microphone_test_cancel,
    microphone_test_finish, microphone_test_start, model_catalog, model_install, model_remove,
    settings_get, settings_set_input_device, settings_set_language_hint, settings_set_model,
};
use shortcut::{ShortcutService, install_shortcut_backend, shortcut_capability};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Manager, Runtime, WindowEvent};

#[tauri::command]
fn core_status() -> String {
    blcvoice_core::status_line()
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn install_tray<R: Runtime>(app: &App<R>) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show BLCVoice", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit BLCVoice", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut builder = TrayIconBuilder::with_id("blcvoice-main")
        .tooltip("BLCVoice")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            if event.id() == "show" {
                show_main_window(app);
            } else if event.id() == "quit" {
                app.exit(0);
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(ShortcutService::production())
        .manage(ShortcutDictationCoordinator::default())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let desktop =
                DesktopState::production(config_dir, data_dir).map_err(std::io::Error::other)?;
            app.manage(desktop);
            install_tray(app)?;
            install_shortcut_backend(app);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main"
                && let WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            core_status,
            audio_input_discovery,
            desktop_status,
            microphone_test_start,
            microphone_test_finish,
            microphone_test_cancel,
            dictation_start,
            dictation_start_configured,
            dictation_finish,
            dictation_cancel,
            insertion_capability,
            settings_get,
            settings_set_input_device,
            settings_set_model,
            settings_set_language_hint,
            model_catalog,
            model_install,
            model_remove,
            shortcut_capability,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run BLCVoice desktop shell");
}
