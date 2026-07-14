use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tauri::State;

use kaiser_core::{
    get_display_dpi, set_display_dpi, set_display_mode as core_set_display_mode, AudioDevice,
    AudioFlow, AudioSetting, DisplayMode, KaiserConfigStore, KaiserProfile, SharedKaiserBackend,
};
use monarch::{DisplayId, DisplayInfo, Layout, Profile};

use crate::state::AppState;

// u64 values (edid_hash) lose precision in JavaScript's f64 Number type.
// Re-resolves display IDs from the live layout using only adapter_luid + target_id.
fn fix_layout_display_ids(
    mut layout: Layout,
    manager: &monarch::MonarchDisplayManager<SharedKaiserBackend, KaiserConfigStore>,
) -> Layout {
    if let Ok(live) = manager.get_layout() {
        for output in &mut layout.outputs {
            if let Some(live_out) = live.outputs.iter().find(|o| {
                o.display_id.adapter_luid == output.display_id.adapter_luid
                    && o.display_id.target_id == output.display_id.target_id
            }) {
                output.display_id = live_out.display_id.clone();
            }
        }
    }
    layout
}

fn resolve_display_id(
    display_id: &DisplayId,
    manager: &monarch::MonarchDisplayManager<SharedKaiserBackend, KaiserConfigStore>,
) -> DisplayId {
    manager
        .get_layout()
        .ok()
        .and_then(|layout| {
            layout
                .outputs
                .into_iter()
                .find(|o| {
                    o.display_id.adapter_luid == display_id.adapter_luid
                        && o.display_id.target_id == display_id.target_id
                })
                .map(|o| o.display_id)
        })
        .unwrap_or_else(|| display_id.clone())
}

// ---- DTOs ---------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SnapshotDto {
    pub displays: Vec<DisplayInfo>,
    pub layout: Layout,
    pub profiles: Vec<ProfileDto>,
    pub pending_confirmation: bool,
    pub pending_confirmation_remaining_secs: Option<f64>,
    /// GDI device names keyed as "LUID:TID" strings (avoids u64 JSON precision issues).
    /// Only present for active displays.
    pub gdi_names: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProfileDto {
    pub name: String,
    pub layout: Layout,
    pub audio: Vec<AudioSetting>,
    /// Per-monitor DPI percentages keyed by "adapter_luid:target_id"
    pub dpi_scales: HashMap<String, u32>,
}

fn to_profile_dto(profile: &Profile, store: &KaiserConfigStore) -> ProfileDto {
    let kp = store.load_kaiser_profile(&profile.name);
    let audio = kp.as_ref().map(|k| k.audio.clone()).unwrap_or_default();
    let dpi_scales = kp.as_ref().map(|k| k.dpi_scales.clone()).unwrap_or_default();
    // Kaiser config layout is authoritative (may be user-edited); fall back to Monarch's
    let layout = kp.map(|k| k.layout).unwrap_or_else(|| profile.layout.clone());
    ProfileDto { name: profile.name.clone(), layout, audio, dpi_scales }
}

// ---- Display commands ---------------------------------------------------

#[tauri::command]
pub fn get_snapshot(state: State<AppState>) -> Result<SnapshotDto, String> {
    let mut manager = state.manager.lock().unwrap();
    // Auto-rollback if the confirmation window expired (acts as the daemon heartbeat).
    if let Ok(true) = manager.rollback_if_confirmation_expired() {
        log::info!("get_snapshot: confirmation expired — auto-rolled back");
    }
    let store = state.new_store();
    let displays = manager.list_displays().map_err(|e| e.to_string())?;
    let layout = manager.get_layout().map_err(|e| e.to_string())?;
    let profiles = manager
        .list_profiles()
        .iter()
        .map(|p| to_profile_dto(p, &store))
        .collect();
    let pending = manager.has_pending_confirmation();
    let remaining = manager
        .pending_confirmation_remaining()
        .map(|d| d.as_secs_f64());

    // Build GDI name map from the shared backend using "LUID:TID" string keys.
    let gdi_names: HashMap<String, String> = displays
        .iter()
        .filter(|d| d.is_active)
        .filter_map(|d| {
            state
                .backend
                .get_gdi_name(d.id.adapter_luid, d.id.target_id)
                .map(|name| (format!("{}:{}", d.id.adapter_luid, d.id.target_id), name))
        })
        .collect();

    log::debug!("get_snapshot: {} displays, {} gdi_names", displays.len(), gdi_names.len());

    Ok(SnapshotDto {
        displays,
        layout,
        profiles,
        pending_confirmation: pending,
        pending_confirmation_remaining_secs: remaining,
        gdi_names,
    })
}

#[tauri::command]
pub fn list_displays(state: State<AppState>) -> Result<Vec<DisplayInfo>, String> {
    let manager = state.manager.lock().unwrap();
    manager.list_displays().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn toggle_display(display_id: DisplayId, state: State<AppState>) -> Result<(), String> {
    let mut manager = state.manager.lock().unwrap();
    let resolved = resolve_display_id(&display_id, &manager);
    log::info!(
        "toggle_display: {}:{} (edid_hash restored: {})",
        resolved.adapter_luid,
        resolved.target_id,
        resolved.edid_hash.is_some()
    );
    manager.toggle_display(&resolved).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn apply_layout(layout: Layout, state: State<AppState>) -> Result<(), String> {
    let mut manager = state.manager.lock().unwrap();
    let layout = fix_layout_display_ids(layout, &manager);
    log::info!(
        "apply_layout: {} outputs ({} enabled)",
        layout.outputs.len(),
        layout.outputs.iter().filter(|o| o.enabled).count()
    );
    manager.apply_layout(layout).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_profile(name: String, state: State<AppState>) -> Result<(), String> {
    let manager = state.manager.lock().unwrap();
    let layout = manager.get_layout().map_err(|e| e.to_string())?;
    drop(manager);

    // Auto-capture current default audio devices (render + capture)
    let audio: Vec<AudioSetting> = {
        let audio_mgr = state.audio.lock().unwrap();
        match audio_mgr.list_devices() {
            Ok(devices) => devices
                .into_iter()
                .filter(|d| d.is_default_console)
                .map(|d| AudioSetting {
                    pattern: d.name.clone(),
                    flow: d.flow,
                    set_default: Some(true),
                    volume: Some(d.volume),
                    muted: Some(d.muted),
                })
                .collect(),
            Err(e) => {
                log::warn!("save_profile: could not list audio devices: {e}");
                Vec::new()
            }
        }
    };

    // Auto-capture per-monitor DPI for all active outputs
    let dpi_scales: HashMap<String, u32> = layout
        .outputs
        .iter()
        .filter(|o| o.enabled)
        .filter_map(|o| {
            let key = format!("{}:{}", o.display_id.adapter_luid, o.display_id.target_id);
            match get_display_dpi(o.display_id.adapter_luid, o.display_id.target_id) {
                Ok(pct) => Some((key, pct)),
                Err(e) => {
                    log::warn!("save_profile: get_display_dpi for {key} failed: {e}");
                    None
                }
            }
        })
        .collect();

    log::info!("save_profile: '{name}' dpi_scales={dpi_scales:?} audio={}", audio.len());
    let store = state.new_store();
    store
        .save_kaiser_profile(&name, KaiserProfile { layout, audio, dpi_scales })
        .map_err(|e| e.to_string())?;

    let mut manager = state.manager.lock().unwrap();
    manager.save_profile(&name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn apply_profile(name: String, state: State<AppState>) -> Result<(), String> {
    log::info!("apply_profile: '{name}'");
    let store = state.new_store();
    let kaiser_profile = store.load_kaiser_profile(&name);

    {
        let mut manager = state.manager.lock().unwrap();
        if let Some(ref kp) = kaiser_profile {
            // Kaiser config layout is authoritative (may have been user-edited)
            let layout = fix_layout_display_ids(kp.layout.clone(), &manager);
            manager.apply_layout(layout).map_err(|e| e.to_string())?;
        } else {
            manager.apply_profile(&name).map_err(|e| e.to_string())?;
        }
    }

    if let Some(kaiser_profile) = kaiser_profile {
        if !kaiser_profile.audio.is_empty() {
            let audio = state.audio.lock().unwrap();
            apply_audio_settings(&audio, &kaiser_profile.audio);
        }
        for (key, percent) in &kaiser_profile.dpi_scales {
            if let Some((luid_str, tid_str)) = key.split_once(':') {
                if let (Ok(luid), Ok(tid)) =
                    (luid_str.parse::<u64>(), tid_str.parse::<u32>())
                {
                    if let Err(e) = set_display_dpi(luid, tid, *percent) {
                        log::warn!("apply_profile: set_display_dpi {key}={percent}% failed: {e}");
                    } else {
                        log::info!("apply_profile: {key} DPI → {percent}%");
                    }
                }
            }
        }
    }
    Ok(())
}

/// Save edited profile settings (layout, DPI, audio) without applying to the system.
#[tauri::command]
pub fn update_profile(
    name: String,
    layout: Layout,
    dpi_scales: HashMap<String, u32>,
    audio: Vec<AudioSetting>,
    state: State<AppState>,
) -> Result<(), String> {
    log::info!("update_profile: '{name}'");
    let store = state.new_store();
    store
        .save_kaiser_profile(&name, KaiserProfile { layout, audio, dpi_scales })
        .map_err(|e| e.to_string())
}

fn apply_audio_settings(audio: &kaiser_core::AudioManager, settings: &[AudioSetting]) {
    let devices = match audio.list_devices() {
        Ok(d) => d,
        Err(e) => {
            log::warn!("apply_audio_settings: list_devices failed: {e}");
            return;
        }
    };
    for setting in settings {
        let matching: Vec<&AudioDevice> = devices
            .iter()
            .filter(|d| {
                d.name.to_lowercase().contains(&setting.pattern.to_lowercase())
                    && matches!(
                        (d.flow, setting.flow),
                        (AudioFlow::Render, AudioFlow::Render)
                            | (AudioFlow::Capture, AudioFlow::Capture)
                    )
            })
            .collect();
        for device in matching {
            if let Some(vol) = setting.volume {
                let _ = audio.set_volume(&device.id, vol);
            }
            if let Some(muted) = setting.muted {
                let _ = audio.set_mute(&device.id, muted);
            }
            if setting.set_default == Some(true) {
                log::info!("apply_audio_settings: setting default → {}", device.name);
                let _ = audio.set_default(&device.id);
            }
        }
    }
}

#[tauri::command]
pub fn delete_profile(name: String, state: State<AppState>) -> Result<(), String> {
    log::info!("delete_profile: '{name}'");
    let mut manager = state.manager.lock().unwrap();
    manager.delete_profile(&name).map_err(|e| e.to_string())?;
    drop(manager);
    let store = state.new_store();
    store.delete_kaiser_profile(&name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_profiles(state: State<AppState>) -> Result<Vec<ProfileDto>, String> {
    let manager = state.manager.lock().unwrap();
    let store = state.new_store();
    Ok(manager
        .list_profiles()
        .iter()
        .map(|p| to_profile_dto(p, &store))
        .collect())
}

// ---- Audio commands -----------------------------------------------------

#[tauri::command]
pub fn list_audio_devices(state: State<AppState>) -> Result<Vec<AudioDevice>, String> {
    let audio = state.audio.lock().unwrap();
    audio.list_devices().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_audio_volume(
    device_id: String,
    volume: f32,
    state: State<AppState>,
) -> Result<(), String> {
    log::debug!("set_audio_volume: {device_id} → {volume:.2}");
    let audio = state.audio.lock().unwrap();
    audio.set_volume(&device_id, volume).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_audio_mute(
    device_id: String,
    muted: bool,
    state: State<AppState>,
) -> Result<(), String> {
    log::debug!("set_audio_mute: {device_id} → {muted}");
    let audio = state.audio.lock().unwrap();
    audio.set_mute(&device_id, muted).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_default_audio_device(
    device_id: String,
    state: State<AppState>,
) -> Result<(), String> {
    log::info!("set_default_audio_device: {device_id}");
    let audio = state.audio.lock().unwrap();
    audio.set_default(&device_id).map_err(|e| e.to_string())
}

// ---- Resolution commands ------------------------------------------------

#[tauri::command]
pub fn list_display_modes(gdi_device_name: String) -> Result<Vec<DisplayMode>, String> {
    kaiser_core::list_display_modes(&gdi_device_name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_display_mode(gdi_device_name: String, mode: DisplayMode, state: State<AppState>) -> Result<(), String> {
    core_set_display_mode(&gdi_device_name, &mode).map_err(|e| e.to_string())?;
    state.backend.invalidate_snapshot();
    Ok(())
}

/// List display modes for a display identified by DisplayId (frontend-friendly;
/// looks up the GDI name from the backend cache so callers don't need it).
#[tauri::command]
pub fn list_display_modes_for_id(
    display_id: DisplayId,
    state: State<AppState>,
) -> Result<Vec<DisplayMode>, String> {
    let gdi_name = state
        .backend
        .get_gdi_name(display_id.adapter_luid, display_id.target_id)
        .ok_or_else(|| {
            format!(
                "no GDI name for display {}:{}",
                display_id.adapter_luid, display_id.target_id
            )
        })?;
    log::debug!("list_display_modes_for_id: {gdi_name}");
    kaiser_core::list_display_modes(&gdi_name).map_err(|e| e.to_string())
}

/// Set display mode for a display identified by DisplayId.
#[tauri::command]
pub fn set_display_mode_for_id(
    display_id: DisplayId,
    mode: DisplayMode,
    state: State<AppState>,
) -> Result<(), String> {
    let gdi_name = state
        .backend
        .get_gdi_name(display_id.adapter_luid, display_id.target_id)
        .ok_or_else(|| {
            format!(
                "no GDI name for display {}:{}",
                display_id.adapter_luid, display_id.target_id
            )
        })?;
    log::info!(
        "set_display_mode_for_id: {gdi_name} → {}x{}@{}Hz",
        mode.width,
        mode.height,
        mode.refresh_rate_hz
    );
    core_set_display_mode(&gdi_name, &mode).map_err(|e| e.to_string())?;
    state.backend.invalidate_snapshot();
    Ok(())
}

// ---- DPI commands -------------------------------------------------------

#[tauri::command]
pub fn get_display_dpi_cmd(adapter_luid: u64, target_id: u32) -> Result<u32, String> {
    get_display_dpi(adapter_luid, target_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_display_dpi_cmd(adapter_luid: u64, target_id: u32, percent: u32) -> Result<(), String> {
    log::info!("set_display_dpi: {adapter_luid}:{target_id} → {percent}%");
    set_display_dpi(adapter_luid, target_id, percent).map_err(|e| e.to_string())
}

// ---- Confirmation commands -----------------------------------------------

#[tauri::command]
pub fn confirm_layout(state: State<AppState>) -> Result<(), String> {
    log::info!("confirm_layout");
    let mut manager = state.manager.lock().unwrap();
    manager.confirm_current_layout().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn revert_layout(state: State<AppState>) -> Result<(), String> {
    log::info!("revert_layout");
    let mut manager = state.manager.lock().unwrap();
    manager.rollback_pending().map_err(|e| e.to_string())
}

// ---- Frontend logging ---------------------------------------------------

#[tauri::command]
pub fn frontend_log(level: String, message: String) {
    match level.as_str() {
        "error" => log::error!("[frontend] {}", message),
        "warn"  => log::warn!("[frontend] {}", message),
        "info"  => log::info!("[frontend] {}", message),
        _       => log::debug!("[frontend] {}", message),
    }
}

// ---- Primary display command --------------------------------------------

#[tauri::command]
pub fn make_primary(display_id: DisplayId, state: State<AppState>) -> Result<(), String> {
    let mut manager = state.manager.lock().unwrap();
    let resolved = resolve_display_id(&display_id, &manager);
    let mut layout = manager.get_layout().map_err(|e| e.to_string())?;
    for output in &mut layout.outputs {
        output.primary = output.display_id.adapter_luid == resolved.adapter_luid
            && output.display_id.target_id == resolved.target_id
            && output.enabled;
    }
    log::info!("make_primary: {}:{}", resolved.adapter_luid, resolved.target_id);
    manager.apply_layout(layout).map_err(|e| e.to_string())
}
