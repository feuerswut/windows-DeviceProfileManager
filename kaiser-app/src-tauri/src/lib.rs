pub mod cli;
mod commands;
mod state;

use state::AppState;
use tauri::Manager;

pub fn run() {
    // In debug builds default to DEBUG so log::debug! calls in the apply pipeline are visible.
    #[cfg(debug_assertions)]
    let default_level = "debug";
    #[cfg(not(debug_assertions))]
    let default_level = "info";
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_level)).init();

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
            commands::frontend_log,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kaiser");
}
