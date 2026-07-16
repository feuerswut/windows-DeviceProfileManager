pub mod cli;
mod commands;
mod state;

use state::AppState;
use tauri::Manager;

pub fn run() {
    // dev builds: TRACE so all frontend command traces + apply pipeline debug are visible.
    // release builds: INFO only.
    #[cfg(debug_assertions)]
    let default_level = "trace,tao=warn,wry=warn,tauri=warn";
    #[cfg(not(debug_assertions))]
    let default_level = "info";
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_level))
        .format(|buf, record| {
            use std::io::Write;
            use env_logger::fmt::style::{AnsiColor, Style};

            let (level_style, line_style) = match record.level() {
                log::Level::Error => (
                    Style::new().fg_color(Some(AnsiColor::Red.into())).bold(),
                    Some(Style::new().fg_color(Some(AnsiColor::Red.into()))),
                ),
                log::Level::Warn => (
                    Style::new().fg_color(Some(AnsiColor::Yellow.into())).bold(),
                    Some(Style::new().fg_color(Some(AnsiColor::Yellow.into()))),
                ),
                log::Level::Info => (
                    Style::new().fg_color(Some(AnsiColor::Green.into())).bold(),
                    None,
                ),
                log::Level::Debug => (
                    Style::new().bold(),
                    None,
                ),
                log::Level::Trace => (
                    Style::new().fg_color(Some(AnsiColor::BrightBlack.into())).italic(),
                    Some(Style::new().fg_color(Some(AnsiColor::BrightBlack.into())).italic()),
                ),
            };

            let reset = Style::new();
            let level_str = format!("{level_style}{:<5}{reset}", record.level());
            let target = record.target();
            let message = record.args();

            if let Some(ls) = line_style {
                writeln!(buf, "{level_str} {ls}[{target}] {message}{reset}")
            } else {
                writeln!(buf, "{level_str} [{target}] {message}")
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
