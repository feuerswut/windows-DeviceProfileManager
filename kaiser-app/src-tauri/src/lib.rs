pub mod cli;
mod commands;
mod state;

use state::AppState;
use tauri::Manager;

/// Enable ANSI escape code processing on Windows (cmd.exe / PowerShell).
/// Without this the console ignores color codes and prints raw ESC sequences.
#[cfg(target_os = "windows")]
fn enable_ansi_console() {
    use windows::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode,
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, STD_ERROR_HANDLE,
    };
    unsafe {
        let handle = GetStdHandle(STD_ERROR_HANDLE).unwrap_or_default();
        let mut mode = Default::default();
        if GetConsoleMode(handle, &mut mode).is_ok() {
            let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        }
    }
}

pub fn run() {
    #[cfg(target_os = "windows")]
    enable_ansi_console();

    // dev builds: TRACE so all frontend command traces + apply pipeline debug are visible.
    // release builds: INFO only.
    #[cfg(debug_assertions)]
    let default_level = "trace,tao=warn,wry=warn,tauri=warn";
    #[cfg(not(debug_assertions))]
    let default_level = "info";
    use env_logger::fmt::style::{AnsiColor, Style};
    use env_logger::WriteStyle;

    let trace_s = Style::new().fg_color(Some(AnsiColor::BrightBlack.into())).italic();
    let debug_lvl = Style::new().bold();
    let info_lvl  = Style::new().fg_color(Some(AnsiColor::Green.into())).bold();
    let warn_s    = Style::new().fg_color(Some(AnsiColor::Yellow.into()));
    let error_s   = Style::new().fg_color(Some(AnsiColor::Red.into()));

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_level))
        .write_style(WriteStyle::Always)
        .format(move |buf, record| {
            use std::io::Write;
            let target = record.target();
            let msg = record.args();
            match record.level() {
                log::Level::Trace => writeln!(buf, "{}TRACE [{target}] {msg}{}", trace_s.render(), trace_s.render_reset()),
                log::Level::Debug => writeln!(buf, "{}DEBUG{} [{target}] {msg}", debug_lvl.render(), debug_lvl.render_reset()),
                log::Level::Info  => writeln!(buf, "{}INFO {} [{target}] {msg}", info_lvl.render(),  info_lvl.render_reset()),
                log::Level::Warn  => writeln!(buf, "{}WARN  [{target}] {msg}{}", warn_s.render(),    warn_s.render_reset()),
                log::Level::Error => writeln!(buf, "{}ERROR [{target}] {msg}{}", error_s.render(),   error_s.render_reset()),
            }
        })
        .init();

    tauri::Builder::default()
        .setup(|app| {
            let state = AppState::new();
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::list_displays,
            commands::toggle_display,
            commands::apply_layout,
            commands::save_profile,
            commands::apply_profile,
            commands::delete_profile,
            commands::list_profiles,
            commands::list_audio_devices,
            commands::set_audio_volume,
            commands::set_audio_mute,
            commands::set_default_audio_device,
            commands::list_display_modes,
            commands::set_display_mode,
            commands::list_display_modes_for_id,
            commands::set_display_mode_for_id,
            commands::confirm_layout,
            commands::revert_layout,
            commands::make_primary,
            commands::get_display_dpi_cmd,
            commands::set_display_dpi_cmd,
            commands::update_profile,
            commands::set_display_rotation,
            commands::set_clone_source,
            commands::refresh_backend,
            commands::frontend_log,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kaiser");
}
