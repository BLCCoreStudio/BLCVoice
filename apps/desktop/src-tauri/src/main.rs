#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
fn apply_linux_webkit_compatibility() {
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let kde = std::env::var("XDG_CURRENT_DESKTOP")
        .ok()
        .is_some_and(|desktop| desktop.to_ascii_lowercase().contains("kde"));
    let override_present = std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_some();

    if !wayland || !kde || override_present {
        return;
    }

    let Ok(executable) = std::env::current_exe() else {
        return;
    };

    let error = Command::new(executable)
        .args(std::env::args_os().skip(1))
        .env("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
        .exec();

    eprintln!("failed to restart BLCVoice with the KDE/Wayland WebKit compatibility guard: {error}");
}

fn main() {
    #[cfg(target_os = "linux")]
    apply_linux_webkit_compatibility();

    blcvoice_desktop_lib::run();
}
