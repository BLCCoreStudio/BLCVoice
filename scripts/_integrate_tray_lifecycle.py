from pathlib import Path

cargo_path = Path("apps/desktop/src-tauri/Cargo.toml")
cargo = cargo_path.read_text()
old = 'tauri = { version = "2", features = [] }\n'
new = 'tauri = { version = "2", features = ["tray-icon"] }\n'
if old not in cargo:
    raise SystemExit("tauri dependency marker not found")
cargo_path.write_text(cargo.replace(old, new, 1))

lib_path = Path("apps/desktop/src-tauri/src/lib.rs")
source = lib_path.read_text()
old = '''use shortcut::{ShortcutService, install_shortcut_backend, shortcut_capability};\nuse tauri::Manager;\n'''
new = '''use shortcut::{ShortcutService, install_shortcut_backend, shortcut_capability};\nuse tauri::menu::{Menu, MenuItem};\nuse tauri::tray::TrayIconBuilder;\nuse tauri::{App, AppHandle, Manager, Runtime, WindowEvent};\n'''
if old not in source:
    raise SystemExit("import marker not found")
source = source.replace(old, new, 1)

marker = '''#[tauri::command]\nfn core_status() -> String {\n    blcvoice_core::status_line()\n}\n\n'''
addition = marker + '''fn show_main_window<R: Runtime>(app: &AppHandle<R>) {\n    if let Some(window) = app.get_webview_window("main") {\n        let _ = window.unminimize();\n        let _ = window.show();\n        let _ = window.set_focus();\n    }\n}\n\nfn install_tray<R: Runtime>(app: &App<R>) -> tauri::Result<()> {\n    let show = MenuItem::with_id(app, "show", "Show BLCVoice", true, None::<&str>)?;\n    let quit = MenuItem::with_id(app, "quit", "Quit BLCVoice", true, None::<&str>)?;\n    let menu = Menu::with_items(app, &[&show, &quit])?;\n\n    let mut builder = TrayIconBuilder::with_id("blcvoice-main")\n        .tooltip("BLCVoice")\n        .menu(&menu)\n        .show_menu_on_left_click(true)\n        .on_menu_event(|app, event| {\n            if event.id() == "show" {\n                show_main_window(app);\n            } else if event.id() == "quit" {\n                app.exit(0);\n            }\n        });\n    if let Some(icon) = app.default_window_icon().cloned() {\n        builder = builder.icon(icon);\n    }\n    builder.build(app)?;\n    Ok(())\n}\n\n'''
if marker not in source:
    raise SystemExit("core status marker not found")
source = source.replace(marker, addition, 1)

old = '''            app.manage(desktop);\n            install_shortcut_backend(app);\n            Ok(())\n        })\n        .invoke_handler(tauri::generate_handler![\n'''
new = '''            app.manage(desktop);\n            install_tray(app)?;\n            install_shortcut_backend(app);\n            Ok(())\n        })\n        .on_window_event(|window, event| {\n            if window.label() == "main"\n                && let WindowEvent::CloseRequested { api, .. } = event\n            {\n                api.prevent_close();\n                let _ = window.hide();\n            }\n        })\n        .invoke_handler(tauri::generate_handler![\n'''
if old not in source:
    raise SystemExit("setup marker not found")
source = source.replace(old, new, 1)

lib_path.write_text(source)
