use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tauri::State;

use kaiser_core::{
    get_display_dpi, set_display_dpi, set_display_mode as core_set_display_mode, AudioDevice,
    AudioFlow, AudioSetting, DisplayMode, KaiserConfigStore, KaiserProfile, SharedKaiserBackend,
};
#[cfg(target_os = "windows")]
use kaiser_core::{apply_clone_source, apply_display_rotation};
use monarch::{DisplayId, DisplayInfo, Layout, Profile};

use crate::state::AppState;

// Remaps desired layout display IDs to the live layout's IDs.
// Three tiers (mirrors Monarch's remap_layout_display_ids):
//   1. Exact DisplayId match → keep as-is
//   2. edid_hash match (only when exactly one unused live candidate)
//   3. target_id fallback (only when exactly one unused live candidate)
// This survives adapter LUID changes across reboots.
fn fix_layout_display_ids(
    desired: Layout,
    manager: &monarch::MonarchDisplayManager<SharedKaiserBackend, KaiserConfigStore>,
) -> Layout {
    let Ok(live) = manager.get_layout() else { return desired };
    remap_layout_display_ids(desired, &live)
}

fn remap_layout_display_ids(desired: Layout, current: &Layout) -> Layout {
    use std::collections::{HashMap, HashSet};

    let current_ids: HashSet<DisplayId> =
        current.outputs.iter().map(|o| o.display_id.clone()).collect();

    if desired.outputs.iter().all(|o| current_ids.contains(&o.display_id)) {
        return desired;
    }

    let mut remapped = desired;
    let mut used: HashSet<DisplayId> = remapped
        .outputs
        .iter()
        .filter(|o| current_ids.contains(&o.display_id))
        .map(|o| o.display_id.clone())
        .collect();

    let mut by_edid: HashMap<u64, Vec<&monarch::OutputConfig>> = HashMap::new();
    for o in &current.outputs {
        if let Some(h) = o.display_id.edid_hash {
            by_edid.entry(h).or_default().push(o);
        }
    }

    for output in &mut remapped.outputs {
        if current_ids.contains(&output.display_id) {
            continue;
        }
        let mut replacement = None;

        if let Some(h) = output.display_id.edid_hash {
            let candidates: Vec<_> = by_edid
                .get(&h)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|c| !used.contains(&c.display_id))
                .collect();
            if candidates.len() == 1 {
                replacement = Some(candidates[0].display_id.clone());
            }
        }

        if replacement.is_none() {
            let candidates: Vec<_> = current
                .outputs
                .iter()
                .filter(|c| {
                    c.display_id.target_id == output.display_id.target_id
                        && !used.contains(&c.display_id)
                })
                .collect();
            if candidates.len() == 1 {
                replacement = Some(candidates[0].display_id.clone());
            }
        }

        if let Some(next_id) = replacement {
            used.insert(next_id.clone());
            output.display_id = next_id;
        }
    }

    remapped
}

fn resolve_display_id(
    display_id: &DisplayId,
    manager: &monarch::MonarchDisplayManager<SharedKaiserBackend, KaiserConfigStore>,
) -> DisplayId {
    let Ok(live) = manager.get_layout() else { return display_id.clone() };
    // Exact match first, then edid_hash, then target_id
    live.outputs
        .iter()
        .find(|o| &o.display_id == display_id)
        .or_else(|| {
            display_id.edid_hash.and_then(|h| {
                let candidates: Vec<_> =
                    live.outputs.iter().filter(|o| o.display_id.edid_hash == Some(h)).collect();
                if candidates.len() == 1 { Some(candidates[0]) } else { None }
            })
        })
        .or_else(|| {
            let candidates: Vec<_> = live
                .outputs
                .iter()
                .filter(|o| o.display_id.target_id == display_id.target_id)
                .collect();
            if candidates.len() == 1 { Some(candidates[0]) } else { None }
        })
        .map(|o| o.display_id.clone())
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
    pub gdi_names: HashMap<String, String>,
    /// Current DPI scaling percentages keyed by "LUID:TID".
    pub dpi_values: HashMap<String, u32>,
    /// Current rotation in degrees (0/90/180/270) keyed by "LUID:TID". Absent = 0°.
    pub rotation_values: HashMap<String, u32>,
    /// Clone relationships: "LUID:TID" (clone) → "LUID:TID" (source).
    pub clone_pairs: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProfileDto {
    pub name: String,
    pub layout: Layout,
    pub audio: Vec<AudioSetting>,
    /// Per-monitor DPI percentages keyed by "adapter_luid:target_id"
    pub dpi_scales: HashMap<String, u32>,
    /// Friendly display names captured at save time, keyed by "adapter_luid:target_id"
    pub display_names: HashMap<String, String>,
}

/// Deduplicate outputs by target_id (prefer enabled over disabled) and strip
/// `primary` from any output that is not enabled. Fixes profiles corrupted by
/// sync adding duplicate entries or by stale in-memory layouts.
fn sanitize_layout(layout: Layout) -> Layout {
    use std::collections::HashSet;
    let mut seen: HashSet<u32> = HashSet::new();
    let mut outputs = Vec::new();
    // Enabled outputs first so they "win" the dedup over disabled ones.
    for o in layout.outputs.iter().filter(|o| o.enabled) {
        if seen.insert(o.display_id.target_id) {
            outputs.push(o.clone());
        }
    }
    for o in layout.outputs.iter().filter(|o| !o.enabled) {
        if seen.insert(o.display_id.target_id) {
            let mut o = o.clone();
            o.primary = false; // disabled outputs cannot be primary
            outputs.push(o);
        }
    }
    Layout { outputs }
}

fn to_profile_dto(profile: &Profile, store: &KaiserConfigStore) -> ProfileDto {
    let kp = store.load_kaiser_profile(&profile.name);
    let audio = kp.as_ref().map(|k| k.audio.clone()).unwrap_or_default();
    let dpi_scales = kp.as_ref().map(|k| k.dpi_scales.clone()).unwrap_or_default();
    let display_names = kp.as_ref().map(|k| k.display_names.clone()).unwrap_or_default();
    // Kaiser config layout is authoritative (may be user-edited); fall back to Monarch's.
    // Sanitize on every read to fix any duplicates or stale primary flags.
    let layout = kp
        .map(|k| sanitize_layout(k.layout))
        .unwrap_or_else(|| profile.layout.clone());
    ProfileDto { name: profile.name.clone(), layout, audio, dpi_scales, display_names }
}

// ---- Display commands ---------------------------------------------------

#[tauri::command]
pub fn get_snapshot(state: State<AppState>) -> Result<SnapshotDto, String> {
    log::trace!("→ get_snapshot");
    let mut manager = state.manager.lock().unwrap();
    // Auto-rollback if the confirmation window expired (acts as the daemon heartbeat).
    if let Ok(true) = manager.rollback_if_confirmation_expired() {
        log::info!("get_snapshot: confirmation expired — auto-rolled back");
        drop(manager);
        if let Some(dpi_snapshot) = state.pending_dpi_rollback.lock().unwrap().take() {
            for (key, percent) in &dpi_snapshot {
                if let Some((ls, ts)) = key.split_once(':') {
                    if let (Ok(luid), Ok(tid)) = (ls.parse::<u64>(), ts.parse::<u32>()) {
                        if let Err(e) = set_display_dpi(luid, tid, *percent) {
                            log::warn!("get_snapshot: restore DPI {key}={percent}% failed: {e}");
                        }
                    }
                }
            }
        }
        manager = state.manager.lock().unwrap();
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

    let dpi_values: HashMap<String, u32> = displays
        .iter()
        .filter(|d| d.is_active)
        .filter_map(|d| {
            get_display_dpi(d.id.adapter_luid, d.id.target_id)
                .ok()
                .map(|pct| (format!("{}:{}", d.id.adapter_luid, d.id.target_id), pct))
        })
        .collect();

    let rotation_values: HashMap<String, u32> = state.backend.get_rotation_values()
        .into_iter()
        .map(|((luid, tid), deg)| (format!("{luid}:{tid}"), deg))
        .collect();

    let clone_pairs: HashMap<String, String> = state.backend.get_clone_pairs()
        .into_iter()
        .map(|((cl, ct), (sl, st))| (format!("{cl}:{ct}"), format!("{sl}:{st}")))
        .collect();

    log::trace!("get_snapshot: {} displays, {} gdi_names", displays.len(), gdi_names.len());

    let snapshot = SnapshotDto {
        displays,
        layout,
        profiles,
        pending_confirmation: pending,
        pending_confirmation_remaining_secs: remaining,
        gdi_names,
        dpi_values,
        rotation_values,
        clone_pairs,
    };

    // Release manager lock before file I/O, then sync any newly connected displays.
    drop(manager);
    sync_new_displays_to_profiles(&state, &store, &snapshot.displays, &snapshot.layout);

    Ok(snapshot)
}

/// Adds newly connected displays (not yet in any profile) as disabled outputs.
/// Runs only when the active display set changes; no-ops otherwise.
fn sync_new_displays_to_profiles(
    state: &AppState,
    store: &KaiserConfigStore,
    displays: &[DisplayInfo],
    live_layout: &Layout,
) {
    use std::collections::BTreeSet;

    let current_keys: BTreeSet<u32> = displays
        .iter()
        .filter(|d| d.is_active)
        .map(|d| d.id.target_id)
        .collect();

    {
        let mut known = state.known_display_keys.lock().unwrap();
        if known.as_ref() == Some(&current_keys) {
            return;
        }
        *known = Some(current_keys);
    }

    let profile_names = store.list_profile_names();
    for name in &profile_names {
        let Some(mut kp) = store.load_kaiser_profile(name) else { continue };
        let mut changed = false;

        for live_out in live_layout.outputs.iter().filter(|o| o.enabled) {
            let in_profile = kp.layout.outputs.iter().any(|o| {
                // Check edid_hash first; always fall back to target_id so virtual
                // displays with unstable EDIDs (e.g. VDD) don't produce duplicates.
                let hash_match = matches!(
                    (o.display_id.edid_hash, live_out.display_id.edid_hash),
                    (Some(h1), Some(h2)) if h1 == h2
                );
                hash_match || o.display_id.target_id == live_out.display_id.target_id
            });
            if !in_profile {
                let mut new_out = live_out.clone();
                new_out.enabled = false;
                new_out.primary = false;
                kp.layout.outputs.push(new_out);
                changed = true;
                log::info!(
                    "sync_profiles: added display :{} to profile '{name}' as disabled",
                    live_out.display_id.target_id
                );
            }
        }

        if changed {
            let _ = store.save_kaiser_profile(name, kp);
        }
    }
}

#[tauri::command]
pub fn list_displays(state: State<AppState>) -> Result<Vec<DisplayInfo>, String> {
    log::trace!("→ list_displays");
    let manager = state.manager.lock().unwrap();
    manager.list_displays().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn toggle_display(display_id: DisplayId, state: State<AppState>) -> Result<(), String> {
    log::trace!("→ toggle_display {:?}", display_id);
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
    log::trace!("→ apply_layout {} outputs", layout.outputs.len());
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

    // Auto-capture per-monitor DPI and friendly names for all active outputs
    let mut dpi_scales: HashMap<String, u32> = HashMap::new();
    let mut display_names: HashMap<String, String> = HashMap::new();
    {
        let manager = state.manager.lock().unwrap();
        let displays = manager.list_displays().unwrap_or_default();
        drop(manager);
        for o in layout.outputs.iter().filter(|o| o.enabled) {
            let key = format!("{}:{}", o.display_id.adapter_luid, o.display_id.target_id);
            match get_display_dpi(o.display_id.adapter_luid, o.display_id.target_id) {
                Ok(pct) => { dpi_scales.insert(key.clone(), pct); }
                Err(e) => log::warn!("save_profile: get_display_dpi for {key} failed: {e}"),
            }
            if let Some(d) = displays.iter().find(|d| {
                d.id.adapter_luid == o.display_id.adapter_luid
                    && d.id.target_id == o.display_id.target_id
            }) {
                display_names.insert(key, d.friendly_name.clone());
            }
        }
    }

    log::info!("save_profile: '{name}' dpi_scales={dpi_scales:?} audio={}", audio.len());
    let store = state.new_store();
    store
        .save_kaiser_profile(&name, KaiserProfile { layout, audio, dpi_scales, display_names })
        .map_err(|e| e.to_string())?;

    let mut manager = state.manager.lock().unwrap();
    manager.save_profile(&name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn apply_profile(name: String, state: State<AppState>) -> Result<(), String> {
    log::info!("apply_profile: '{name}'");
    let store = state.new_store();
    let kaiser_profile = store.load_kaiser_profile(&name);

    // Spawn audio on its own thread immediately — it runs concurrently with display work.
    // AudioManager initialises its own COM apartment, so creating a fresh one is correct.
    if let Some(ref kp) = kaiser_profile {
        if !kp.audio.is_empty() {
            let audio_settings = kp.audio.clone();
            std::thread::spawn(move || {
                let mgr = kaiser_core::AudioManager::new();
                apply_audio_settings(&mgr, &audio_settings);
            });
        }
    }

    // Maps stored "luid:tid" DPI keys to the live (luid, tid) after ID remapping.
    let mut dpi_key_remap: HashMap<String, (u64, u32)> = HashMap::new();

    {
        let mut manager = state.manager.lock().unwrap();
        if let Some(ref kp) = kaiser_profile {
            let original = kp.layout.clone();
            let remapped = fix_layout_display_ids(kp.layout.clone(), &manager);
            for (old_out, new_out) in original.outputs.iter().zip(remapped.outputs.iter()) {
                let old_key = format!(
                    "{}:{}",
                    old_out.display_id.adapter_luid, old_out.display_id.target_id
                );
                dpi_key_remap.insert(
                    old_key,
                    (new_out.display_id.adapter_luid, new_out.display_id.target_id),
                );
            }
            manager.apply_layout(remapped).map_err(|e| e.to_string())?;
        } else {
            manager.apply_profile(&name).map_err(|e| e.to_string())?;
        }
    }

    if let Some(kaiser_profile) = kaiser_profile {
        // Snapshot current DPI for rollback.
        let pre_apply_dpi: HashMap<String, u32> = dpi_key_remap
            .values()
            .filter_map(|&(luid, tid)| {
                get_display_dpi(luid, tid)
                    .ok()
                    .map(|pct| (format!("{luid}:{tid}"), pct))
            })
            .collect();
        *state.pending_dpi_rollback.lock().unwrap() = Some(pre_apply_dpi);

        for (key, percent) in &kaiser_profile.dpi_scales {
            let (luid, tid) = if let Some(&(l, t)) = dpi_key_remap.get(key) {
                (l, t)
            } else if let Some((ls, ts)) = key.split_once(':') {
                match (ls.parse::<u64>(), ts.parse::<u32>()) {
                    (Ok(l), Ok(t)) => (l, t),
                    _ => continue,
                }
            } else {
                continue;
            };
            if let Err(e) = set_display_dpi(luid, tid, *percent) {
                log::warn!("apply_profile: set_display_dpi {key}={percent}% failed: {e}");
            } else {
                log::info!("apply_profile: {key} DPI → {percent}%");
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
    let display_names = store.load_kaiser_profile(&name)
        .map(|kp| kp.display_names)
        .unwrap_or_default();
    store
        .save_kaiser_profile(&name, KaiserProfile { layout, audio, dpi_scales, display_names })
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
    *state.pending_dpi_rollback.lock().unwrap() = None;
    let mut manager = state.manager.lock().unwrap();
    manager.confirm_current_layout().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn revert_layout(state: State<AppState>) -> Result<(), String> {
    log::info!("revert_layout");
    let mut manager = state.manager.lock().unwrap();
    manager.rollback_pending().map_err(|e| e.to_string())?;
    drop(manager);
    // Restore DPI to the state captured just before the profile was applied.
    if let Some(dpi_snapshot) = state.pending_dpi_rollback.lock().unwrap().take() {
        for (key, percent) in &dpi_snapshot {
            if let Some((ls, ts)) = key.split_once(':') {
                if let (Ok(luid), Ok(tid)) = (ls.parse::<u64>(), ts.parse::<u32>()) {
                    if let Err(e) = set_display_dpi(luid, tid, *percent) {
                        log::warn!("revert_layout: restore DPI {key}={percent}% failed: {e}");
                    } else {
                        log::info!("revert_layout: {key} DPI restored → {percent}%");
                    }
                }
            }
        }
    }
    Ok(())
}

// ---- Rotation & clone ---------------------------------------------------

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn set_display_rotation(
    adapter_luid: u64,
    target_id: u32,
    degrees: u32,
    state: State<AppState>,
) -> Result<(), String> {
    log::info!("set_display_rotation: {adapter_luid}:{target_id} → {degrees}°");
    apply_display_rotation(adapter_luid, target_id, degrees).map_err(|e| e.to_string())?;
    state.backend.invalidate_snapshot();
    Ok(())
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn set_clone_source(
    clone_adapter_luid: u64,
    clone_target_id: u32,
    src_adapter_luid: u64,
    src_target_id: u32,
    state: State<AppState>,
) -> Result<(), String> {
    log::info!(
        "set_clone_source: {clone_adapter_luid}:{clone_target_id} mirrors {src_adapter_luid}:{src_target_id}"
    );
    apply_clone_source(clone_adapter_luid, clone_target_id, src_adapter_luid, src_target_id)
        .map_err(|e| e.to_string())?;
    state.backend.invalidate_snapshot();
    Ok(())
}

// ---- Backend refresh ----------------------------------------------------

/// Invalidate the cached display snapshot, forcing a fresh QueryDisplayConfig
/// on the next backend call. Useful after external topology changes.
#[tauri::command]
pub fn refresh_backend(state: State<AppState>) -> Result<(), String> {
    log::info!("refresh_backend: invalidating snapshot cache");
    state.backend.invalidate_snapshot();
    Ok(())
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
