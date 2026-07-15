#![cfg(target_os = "windows")]

use std::collections::HashMap;
use std::ffi::OsStr;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

use monarch::{Layout, ManagerError};
use windows::core::{w, PCWSTR};
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, SetDisplayConfig,
    DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
    DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_DEVICE_INFO_HEADER,
    DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE,
    DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME,
    SDC_ALLOW_CHANGES, SDC_APPLY, SDC_SAVE_TO_DATABASE, SDC_TOPOLOGY_EXTEND,
    SDC_USE_SUPPLIED_DISPLAY_CONFIG, DISPLAYCONFIG_ROTATION,
};
use windows::Win32::Graphics::Gdi::{CreateDCW, DeleteDC};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::ColorSystem::{
    GetDeviceGammaRamp, SetDeviceGammaRamp, WcsGetCalibrationManagementState,
    WcsSetCalibrationManagementState,
};
use windows::Win32::UI::Shell::{DesktopWallpaper, IDesktopWallpaper, DESKTOP_WALLPAPER_POSITION};

use super::win32_types::{luid_to_u64, TopologySnapshot};

const DISPLAYCONFIG_PATH_ACTIVE_FLAG: u32 = 0x0000_0001;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const GAMMA_RAMP_WORDS: usize = 3 * 256;
pub(super) type GammaRampKey = (u64, u32);
pub(super) type GammaRampWords = [u16; GAMMA_RAMP_WORDS];

/// Five-step apply mirroring kaiser.ps1 exactly — each step is a separate
/// SetDisplayConfig call, never mixing flag changes with mode changes:
///
/// Step 1: Enable desired primary target if inactive (EnableMonitorByName)
/// Step 2: Enable remaining desired-active targets if inactive
/// Step 3: Shift all source modes so the desired primary lands at (0,0) (SetPrimaryByName)
/// Step 4: Deactivate unwanted monitors — flag clear ONLY, no mode changes (DeactivateAllExcept)
/// Step 5: Apply desired resolutions/positions to the now-active monitors
pub fn apply_layout_against_snapshot(
    desired: &Layout,
    snapshot: &TopologySnapshot,
) -> Result<TopologySnapshot, ManagerError> {
    desired.ensure_valid()?;
    let saved_gamma_ramps = capture_active_gamma_ramps(snapshot);
    let saved_wallpapers = capture_active_wallpapers(snapshot);
    let saved_wallpaper_position = capture_wallpaper_position();
    let desired_outputs = desired_output_index(desired);

    // Which target is the desired primary?
    let primary_key = desired.outputs.iter()
        .find(|o| o.enabled && o.primary)
        .or_else(|| desired.outputs.iter().find(|o| o.enabled))
        .map(|o| (o.display_id.adapter_luid, o.display_id.target_id));

    // ── Step 1: enable the desired primary first ──────────────────────────────
    if let Some(pk) = primary_key {
        let cur = super::enumerate::query_active_topology()?;
        let active = active_keys(&cur.raw.paths);
        if !active.contains(&pk) {
            log::info!("apply s1: enabling primary {:016x}:{}", pk.0, pk.1);
            match phase1_enable_targets(&[pk], &cur.raw.paths, &cur.raw.modes) {
                Ok(()) => { wait_for_targets_active(&[pk], 12_000); }
                Err(e) => log::warn!("apply s1: primary enable failed: {e}"),
            }
        } else {
            log::info!("apply s1: primary {:016x}:{} already active", pk.0, pk.1);
        }
    }

    // ── Step 2: enable remaining desired-active targets ───────────────────────
    {
        let cur = super::enumerate::query_active_topology()?;
        let active = active_keys(&cur.raw.paths);
        let remaining: Vec<(u64, u32)> = desired.outputs.iter()
            .filter(|o| o.enabled)
            .map(|o| (o.display_id.adapter_luid, o.display_id.target_id))
            .filter(|k| !active.contains(k))
            .collect();
        if !remaining.is_empty() {
            log::info!("apply s2: enabling {} more target(s): {:?}", remaining.len(), remaining);
            match phase1_enable_targets(&remaining, &cur.raw.paths, &cur.raw.modes) {
                Ok(()) => { wait_for_targets_active(&remaining, 12_000); }
                Err(e) => log::warn!("apply s2: enable failed: {e}"),
            }
        }
    }

    // ── Step 3: set primary position (shift all source modes → primary at 0,0) ─
    {
        let cur = super::enumerate::query_active_topology()?;
        let mut paths = cur.raw.paths.clone();
        let mut modes = cur.raw.modes.clone();
        if shift_primary_to_origin(&paths, &mut modes, &desired_outputs) {
            let status = unsafe {
                SetDisplayConfig(
                    Some(paths.as_slice()),
                    Some(modes.as_slice()),
                    SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_SAVE_TO_DATABASE | SDC_ALLOW_CHANGES,
                )
            };
            reorder_paths_for_desired_priority(&mut paths, &desired_outputs);
            log::info!("apply s3: set_primary SetDisplayConfig → {}", status);
        }
    }

    // ── Step 4: deactivate unwanted (flag-clear ONLY, no mode changes) ────────
    {
        let cur = super::enumerate::query_active_topology()?;
        let mut paths = cur.raw.paths.clone();
        let modes = cur.raw.modes.clone();
        let mut changed = false;
        for path in &mut paths {
            let key = path_target_key(path);
            let keep = desired_outputs.get(&key).map(|o| o.enabled).unwrap_or(false);
            if !keep && path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG != 0 {
                path.flags &= !DISPLAYCONFIG_PATH_ACTIVE_FLAG;
                log::info!("apply s4: deactivating {:016x}:{}", key.0, key.1);
                changed = true;
            }
        }
        let remaining_active = paths.iter().filter(|p| p.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG != 0).count();
        log::info!("apply s4: {} path(s) remain active after deactivation", remaining_active);
        if remaining_active == 0 {
            return Err(ManagerError::Backend("s4: would leave 0 active displays".to_string()));
        }
        if changed {
            let status = unsafe {
                SetDisplayConfig(
                    Some(paths.as_slice()),
                    Some(modes.as_slice()),
                    SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_SAVE_TO_DATABASE | SDC_ALLOW_CHANGES,
                )
            };
            if status != 0 {
                return Err(ManagerError::Backend(format!("SetDisplayConfig failed: {status}")));
            }
        }
    }

    // ── Step 5: apply desired resolutions / positions ────────────────────────
    {
        let cur = super::enumerate::query_active_topology()?;
        let mut paths = cur.raw.paths.clone();
        let mut modes = cur.raw.modes.clone();
        let mut changed = false;

        for path in paths.iter_mut() {
            if path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG == 0 { continue; }
            let key = path_target_key(path);
            let Some(output) = desired_outputs.get(&key).copied() else { continue };
            if !output.enabled { continue; }

            let mode_idx = unsafe { path.sourceInfo.Anonymous.modeInfoIdx } as usize;
            if let Some(mode) = modes.get_mut(mode_idx) {
                if mode.infoType.0 == DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE.0 {
                    let (cx, cy, cw, ch) = unsafe {
                        let s = &mode.Anonymous.sourceMode;
                        (s.position.x, s.position.y, s.width, s.height)
                    };
                    if cx != output.position.x || cy != output.position.y
                        || cw != output.resolution.width || ch != output.resolution.height
                    {
                        unsafe {
                            let s = &mut mode.Anonymous.sourceMode;
                            s.position.x = output.position.x;
                            s.position.y = output.position.y;
                            s.width = output.resolution.width;
                            s.height = output.resolution.height;
                        }
                        log::info!(
                            "apply s5: {:016x}:{} → {}x{} @({},{})",
                            key.0, key.1,
                            output.resolution.width, output.resolution.height,
                            output.position.x, output.position.y
                        );
                        changed = true;
                    }
                }
            }
            apply_desired_target_refresh(path, desired_outputs.get(&key));
        }

        if changed {
            let status = unsafe {
                SetDisplayConfig(
                    Some(paths.as_slice()),
                    Some(modes.as_slice()),
                    SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_SAVE_TO_DATABASE | SDC_ALLOW_CHANGES,
                )
            };
            log::info!("apply s5: resolution SetDisplayConfig → {}", status);
        }
    }

    let next_snapshot = super::enumerate::query_active_topology()?;
    best_effort_reload_color_calibration();
    best_effort_restore_gamma_ramps(&next_snapshot, &saved_gamma_ramps);
    best_effort_restore_wallpapers(&next_snapshot, &saved_wallpapers);
    best_effort_restore_wallpaper_position(saved_wallpaper_position);
    Ok(next_snapshot)
}

fn degrees_to_displayconfig_rotation(degrees: u32) -> DISPLAYCONFIG_ROTATION {
    DISPLAYCONFIG_ROTATION(match degrees {
        90  => 2,
        180 => 3,
        270 => 4,
        _   => 1, // identity
    })
}

fn active_keys(paths: &[DISPLAYCONFIG_PATH_INFO]) -> std::collections::HashSet<(u64, u32)> {
    paths.iter()
        .filter(|p| p.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG != 0)
        .map(|p| path_target_key(p))
        .collect()
}

/// Mirrors PS `SetPrimaryByName`: subtract the desired primary's current position
/// from every source-mode so it ends up at (0,0). Returns true if a shift was needed.
fn shift_primary_to_origin(
    paths: &[DISPLAYCONFIG_PATH_INFO],
    modes: &mut Vec<DISPLAYCONFIG_MODE_INFO>,
    desired_outputs: &HashMap<(u64, u32), &monarch::OutputConfig>,
) -> bool {
    let pidx = paths.iter()
        .filter(|p| p.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG != 0)
        .filter_map(|p| {
            let key = path_target_key(p);
            let out = desired_outputs.get(&key)?;
            if !out.primary { return None; }
            let idx = unsafe { p.sourceInfo.Anonymous.modeInfoIdx };
            if idx == 0xFFFF_FFFF { return None; }
            Some(idx as usize)
        })
        .next();

    let Some(pidx) = pidx else { return false };
    let Some(pm) = modes.get(pidx) else { return false };
    if pm.infoType.0 != DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE.0 { return false; }

    let (ox, oy) = unsafe { (pm.Anonymous.sourceMode.position.x, pm.Anonymous.sourceMode.position.y) };
    if ox == 0 && oy == 0 {
        log::debug!("shift_primary: already at (0,0)");
        return false;
    }
    log::info!("shift_primary: subtracting ({ox},{oy}) from all source modes");
    for m in modes.iter_mut() {
        if m.infoType.0 == DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE.0 {
            unsafe {
                m.Anonymous.sourceMode.position.x -= ox;
                m.Anonymous.sourceMode.position.y -= oy;
            }
        }
    }
    true
}

/// Poll QDC_ONLY_ACTIVE_PATHS until all `needs_enable` targets appear as active,
/// or the timeout elapses. Mirrors PS `Wait-MonitorActive`.
fn wait_for_targets_active(needs_enable: &[(u64, u32)], timeout_ms: u64) -> bool {
    use std::collections::HashSet;
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(timeout_ms);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        match super::enumerate::query_active_topology() {
            Ok(snap) => {
                let active: HashSet<(u64, u32)> = snap.raw.paths.iter()
                    .filter(|p| p.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG != 0)
                    .map(|p| path_target_key(p))
                    .collect();
                let missing: Vec<_> = needs_enable.iter().filter(|k| !active.contains(*k)).collect();
                if missing.is_empty() {
                    log::info!("wait_for_targets_active: all targets active after {:?}", start.elapsed());
                    return true;
                }
                log::debug!("wait_for_targets_active: still waiting, missing={:?}", missing);
            }
            Err(e) => log::warn!("wait_for_targets_active: query failed: {e}"),
        }
        if start.elapsed() >= timeout {
            return false;
        }
    }
}

/// Check whether the active topology matches what the desired layout requires.
pub(super) fn verify_layout_applied(desired: &Layout, snapshot: &TopologySnapshot) -> bool {
    use std::collections::HashSet;
    let active: HashSet<(u64, u32)> = snapshot.raw.paths.iter()
        .filter(|p| p.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG != 0)
        .map(|p| path_target_key(p))
        .collect();

    let mut ok = true;
    for output in &desired.outputs {
        let key = (output.display_id.adapter_luid, output.display_id.target_id);
        if output.enabled && !active.contains(&key) {
            log::warn!(
                "verify: {:016x}:{} should be ACTIVE but is not",
                key.0, key.1
            );
            ok = false;
        }
        if !output.enabled && active.contains(&key) {
            log::warn!(
                "verify: {:016x}:{} should be INACTIVE but is still active",
                key.0, key.1
            );
            ok = false;
        }
    }
    if ok { log::info!("verify: layout matches active topology"); }
    ok
}

/// Phase 1: append inactive targets to the active paths array and call SetDisplayConfig.
/// Uses the **active** modes array — mirrors PS `EnableMonitorByName`.
fn phase1_enable_targets(
    needs_enable: &[(u64, u32)],
    active_paths: &[DISPLAYCONFIG_PATH_INFO],
    active_modes: &[DISPLAYCONFIG_MODE_INFO],
) -> Result<(), ManagerError> {
    let (all_paths, _) = super::enumerate::query_all_paths()?;
    let mut phase1_paths: Vec<DISPLAYCONFIG_PATH_INFO> = active_paths.to_vec();

    for &(adapter_luid, target_id) in needs_enable {
        match all_paths.iter().find(|p| {
            luid_to_u64(p.targetInfo.adapterId.HighPart, p.targetInfo.adapterId.LowPart)
                == adapter_luid
                && p.targetInfo.id == target_id
                && p.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG == 0
        }) {
            Some(p) => {
                let mut p = p.clone();
                p.flags |= DISPLAYCONFIG_PATH_ACTIVE_FLAG;
                p.sourceInfo.Anonymous.modeInfoIdx = 0xFFFF_FFFF;
                p.targetInfo.Anonymous.modeInfoIdx = 0xFFFF_FFFF;
                log::info!("phase1: appending inactive target {:016x}:{}", adapter_luid, target_id);
                phase1_paths.push(p);
            }
            None => {
                // Not found as inactive — might already be active from a prior attempt.
                log::warn!(
                    "phase1: {:016x}:{} not found as inactive in QDC_ALL_PATHS",
                    adapter_luid, target_id
                );
            }
        }
    }

    let status = unsafe {
        SetDisplayConfig(
            Some(phase1_paths.as_slice()),
            Some(active_modes),
            SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_SAVE_TO_DATABASE | SDC_ALLOW_CHANGES,
        )
    };
    if status != 0 {
        return Err(ManagerError::Backend(format!("phase1 SetDisplayConfig failed: {}", status)));
    }
    Ok(())
}

/// Fallback: query fresh active topology, find inactive targets from QDC_ALL_PATHS,
/// append them, and apply. Kept as a second-chance strategy alongside the main
/// two-phase approach.
pub(super) fn try_attach_inactive_for_layout(
    desired: &Layout,
    active_snapshot: &TopologySnapshot,
) -> Result<TopologySnapshot, ManagerError> {
    let active_keys: std::collections::HashSet<(u64, u32)> = active_snapshot
        .raw
        .paths
        .iter()
        .filter(|p| p.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG != 0)
        .map(|p| (
            luid_to_u64(p.targetInfo.adapterId.HighPart, p.targetInfo.adapterId.LowPart),
            p.targetInfo.id,
        ))
        .collect();

    let needs_attach: Vec<(u64, u32)> = desired
        .outputs
        .iter()
        .filter(|o| {
            o.enabled
                && !active_keys.contains(&(o.display_id.adapter_luid, o.display_id.target_id))
        })
        .map(|o| (o.display_id.adapter_luid, o.display_id.target_id))
        .collect();

    if needs_attach.is_empty() {
        return Err(ManagerError::Backend(
            "try_attach_inactive: no inactive outputs to attach".to_string(),
        ));
    }

    let (all_paths, _) = super::enumerate::query_all_paths()?;
    let desired_outputs = desired_output_index(desired);
    let mut next_paths = active_snapshot.raw.paths.clone();
    let mut next_modes = active_snapshot.raw.modes.clone();

    for path in &mut next_paths {
        let key = path_target_key(path);
        let desired_output = desired_outputs.get(&key);
        let enabled = desired_output.map(|o| o.enabled).unwrap_or(false);
        if enabled {
            apply_desired_source_mode(path, &mut next_modes, desired_output);
            apply_desired_target_refresh(path, desired_output);
        } else {
            path.flags &= !DISPLAYCONFIG_PATH_ACTIVE_FLAG;
        }
    }

    for (adapter_luid, target_id) in &needs_attach {
        let Some(mut inactive_path) = all_paths
            .iter()
            .find(|p| {
                luid_to_u64(p.targetInfo.adapterId.HighPart, p.targetInfo.adapterId.LowPart)
                    == *adapter_luid
                    && p.targetInfo.id == *target_id
                    && p.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG == 0
            })
            .cloned()
        else {
            log::warn!(
                "try_attach_inactive: no path found for adapter={adapter_luid:#018x} target={target_id}"
            );
            continue;
        };
        inactive_path.flags |= DISPLAYCONFIG_PATH_ACTIVE_FLAG;
        inactive_path.sourceInfo.Anonymous.modeInfoIdx = 0xFFFF_FFFF;
        inactive_path.targetInfo.Anonymous.modeInfoIdx = 0xFFFF_FFFF;
        apply_desired_target_refresh(
            &mut inactive_path,
            desired_outputs.get(&(*adapter_luid, *target_id)),
        );
        next_paths.push(inactive_path);
    }

    reorder_paths_for_desired_priority(&mut next_paths, &desired_outputs);

    unsafe {
        let status = SetDisplayConfig(
            Some(next_paths.as_slice()),
            Some(next_modes.as_slice()),
            SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_SAVE_TO_DATABASE | SDC_ALLOW_CHANGES,
        );
        if status != 0 {
            return Err(ManagerError::Backend(format!(
                "SetDisplayConfig (inactive-attach fallback) failed: {}",
                status
            )));
        }
    }

    super::enumerate::query_active_topology()
}

/// Set the rotation of a single active display. Queries fresh active paths,
/// updates the target rotation, and calls SetDisplayConfig. Rotation is in
/// degrees: 0 (identity), 90, 180, 270.
pub fn apply_display_rotation(
    adapter_luid: u64,
    target_id: u32,
    degrees: u32,
) -> Result<(), ManagerError> {
    let snap = super::enumerate::query_active_topology()?;
    let mut paths = snap.raw.paths.clone();
    let modes = snap.raw.modes.clone();

    let desired_rot = degrees_to_displayconfig_rotation(degrees);
    let mut found = false;
    for path in paths.iter_mut() {
        if path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG == 0 { continue; }
        let key = path_target_key(path);
        if key != (adapter_luid, target_id) { continue; }
        path.targetInfo.rotation = desired_rot;
        found = true;
        break;
    }
    if !found {
        return Err(ManagerError::Backend(format!(
            "display {:016x}:{} not active", adapter_luid, target_id
        )));
    }

    let status = unsafe {
        SetDisplayConfig(
            Some(paths.as_slice()),
            Some(modes.as_slice()),
            SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_SAVE_TO_DATABASE | SDC_ALLOW_CHANGES,
        )
    };
    if status != 0 {
        return Err(ManagerError::Backend(format!("apply_rotation SetDisplayConfig failed: {status}")));
    }
    log::info!("apply_display_rotation: {:016x}:{} → {}°", adapter_luid, target_id, degrees);
    Ok(())
}

/// Make one active display clone another by assigning them the same GDI source slot.
/// Pass `src_adapter_luid = 0, src_target_id = 0` to remove cloning (make it extended).
pub fn apply_clone_source(
    clone_adapter_luid: u64,
    clone_target_id: u32,
    src_adapter_luid: u64,
    src_target_id: u32,
) -> Result<(), ManagerError> {
    let snap = super::enumerate::query_active_topology()?;
    let mut paths = snap.raw.paths.clone();
    let modes = snap.raw.modes.clone();

    if src_adapter_luid == 0 {
        // Remove clone: find a free sourceInfo.id on the adapter and assign it
        let clone_path = paths.iter().find(|p| path_target_key(p) == (clone_adapter_luid, clone_target_id)).cloned();
        let Some(mut cp) = clone_path else {
            return Err(ManagerError::Backend(format!("clone target not active")));
        };
        let used_ids: std::collections::HashSet<u32> = paths.iter()
            .filter(|p| p.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG != 0)
            .filter(|p| p.sourceInfo.adapterId.HighPart == cp.sourceInfo.adapterId.HighPart
                && p.sourceInfo.adapterId.LowPart == cp.sourceInfo.adapterId.LowPart)
            .map(|p| p.sourceInfo.id)
            .collect();
        let mut free_id = 0u32;
        while used_ids.contains(&free_id) { free_id += 1; }
        for p in paths.iter_mut() {
            if path_target_key(p) == (clone_adapter_luid, clone_target_id) {
                p.sourceInfo.id = free_id;
                p.sourceInfo.Anonymous.modeInfoIdx = 0xFFFF_FFFF;
                p.targetInfo.Anonymous.modeInfoIdx = 0xFFFF_FFFF;
                break;
            }
        }
    } else {
        // Set clone: copy source slot from the src path to the clone path
        let src_path = paths.iter().find(|p| path_target_key(p) == (src_adapter_luid, src_target_id)).cloned();
        let Some(sp) = src_path else {
            return Err(ManagerError::Backend(format!("clone source not active")));
        };
        let src_source_id = sp.sourceInfo.id;
        let src_mode_idx = unsafe { sp.sourceInfo.Anonymous.modeInfoIdx };
        for p in paths.iter_mut() {
            if path_target_key(p) == (clone_adapter_luid, clone_target_id) {
                p.sourceInfo.id = src_source_id;
                p.sourceInfo.Anonymous.modeInfoIdx = src_mode_idx;
                break;
            }
        }
    }

    let status = unsafe {
        SetDisplayConfig(
            Some(paths.as_slice()),
            Some(modes.as_slice()),
            SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_SAVE_TO_DATABASE | SDC_ALLOW_CHANGES,
        )
    };
    if status != 0 {
        return Err(ManagerError::Backend(format!("apply_clone_source SetDisplayConfig failed: {status}")));
    }
    log::info!(
        "apply_clone_source: {:016x}:{} → clone of {:016x}:{}",
        clone_adapter_luid, clone_target_id, src_adapter_luid, src_target_id
    );
    Ok(())
}

pub(super) fn force_topology_extend() -> Result<(), ManagerError> {
    let set_display_status = unsafe {
        SetDisplayConfig(
            None,
            None,
            SDC_APPLY | SDC_TOPOLOGY_EXTEND | SDC_ALLOW_CHANGES | SDC_SAVE_TO_DATABASE,
        )
    };
    if set_display_status == 0 {
        return Ok(());
    }
    let display_switch_status = Command::new("DisplaySwitch.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .arg("/extend")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|err| {
            ManagerError::Backend(format!(
                "SetDisplayConfig (topology extend) failed: {set_display_status}; DisplaySwitch /extend launch failed: {err}"
            ))
        })?;
    if !display_switch_status.success() {
        return Err(ManagerError::Backend(format!(
            "SetDisplayConfig (topology extend) failed: {set_display_status}; DisplaySwitch /extend failed with exit code {:?}",
            display_switch_status.code()
        )));
    }
    Ok(())
}

pub(super) fn reapply_color_calibration_for_active_with_cached_sdr(
    cached_sdr_ramps: &HashMap<GammaRampKey, GammaRampWords>,
) -> Result<(), ManagerError> {
    best_effort_reload_color_calibration();
    let refreshed_snapshot = super::enumerate::query_active_topology()?;
    best_effort_restore_gamma_ramps(&refreshed_snapshot, cached_sdr_ramps);
    Ok(())
}

pub(super) fn capture_sdr_gamma_ramps(
    snapshot: &TopologySnapshot,
) -> HashMap<GammaRampKey, GammaRampWords> {
    let mut ramps = HashMap::new();
    for path in &snapshot.raw.paths {
        if path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG == 0 {
            continue;
        }
        if target_advanced_color_enabled(path).unwrap_or(false) {
            continue;
        }
        let key = (
            luid_to_u64(path.targetInfo.adapterId.HighPart, path.targetInfo.adapterId.LowPart),
            path.targetInfo.id,
        );
        let Some(device_name) = source_gdi_device_name(path) else { continue };
        let Some(ramp) = get_gamma_ramp_for_device(&device_name) else { continue };
        ramps.insert(key, ramp);
    }
    ramps
}

pub(super) fn gamma_ramp_looks_identity(ramp: &GammaRampWords) -> bool {
    let tolerance = 384u16;
    for channel in 0..3 {
        let base = channel * 256;
        for i in 0..256usize {
            let expected = (i as u32 * 257) as i32;
            let actual = ramp[base + i] as i32;
            if (actual - expected).unsigned_abs() > tolerance as u32 {
                return false;
            }
        }
    }
    true
}

pub(super) fn active_color_state_signature(snapshot: &TopologySnapshot) -> String {
    let mut entries: Vec<(u64, u32, Option<bool>)> = Vec::new();
    for path in &snapshot.raw.paths {
        if path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG == 0 {
            continue;
        }
        let key = (
            luid_to_u64(path.targetInfo.adapterId.HighPart, path.targetInfo.adapterId.LowPart),
            path.targetInfo.id,
        );
        entries.push((key.0, key.1, target_advanced_color_enabled(path)));
    }
    entries.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut signature = String::new();
    for (index, (adapter_luid, target_id, hdr_enabled)) in entries.iter().enumerate() {
        if index > 0 {
            signature.push(';');
        }
        let hdr_flag = match hdr_enabled {
            Some(true) => '1',
            Some(false) => '0',
            None => 'x',
        };
        signature.push_str(&format!("{adapter_luid:016x}:{target_id}:{hdr_flag}"));
    }
    signature
}

fn best_effort_reload_color_calibration() {
    unsafe {
        let mut enabled = windows::core::BOOL(0);
        if WcsGetCalibrationManagementState(&mut enabled).as_bool() && enabled.as_bool() {
            let disabled = WcsSetCalibrationManagementState(false);
            let reenabled = WcsSetCalibrationManagementState(true);
            if disabled.as_bool() && reenabled.as_bool() {
                return;
            }
        }
    }
    let _ = Command::new("schtasks.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["/Run", "/TN", r"\Microsoft\Windows\WindowsColorSystem\Calibration Loader"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn capture_active_gamma_ramps(snapshot: &TopologySnapshot) -> HashMap<(u64, u32), GammaRampWords> {
    let mut ramps = HashMap::new();
    for path in &snapshot.raw.paths {
        if path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG == 0 {
            continue;
        }
        let key = (
            luid_to_u64(path.targetInfo.adapterId.HighPart, path.targetInfo.adapterId.LowPart),
            path.targetInfo.id,
        );
        let Some(device_name) = source_gdi_device_name(path) else { continue };
        let Some(ramp) = get_gamma_ramp_for_device(&device_name) else { continue };
        ramps.insert(key, ramp);
    }
    ramps
}

fn capture_active_wallpapers(snapshot: &TopologySnapshot) -> HashMap<(u64, u32), String> {
    let Some(session) = create_desktop_wallpaper_session() else {
        return HashMap::new();
    };
    let mut wallpapers = HashMap::new();
    for path in &snapshot.raw.paths {
        if path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG == 0 {
            continue;
        }
        let key = (
            luid_to_u64(path.targetInfo.adapterId.HighPart, path.targetInfo.adapterId.LowPart),
            path.targetInfo.id,
        );
        let Some(monitor_device_path) = target_monitor_device_path(path) else { continue };
        let Some(wallpaper_path) =
            get_wallpaper_for_monitor(&session.desktop_wallpaper, &monitor_device_path)
        else {
            continue;
        };
        wallpapers.insert(key, wallpaper_path);
    }
    wallpapers
}

fn best_effort_restore_gamma_ramps(
    snapshot: &TopologySnapshot,
    ramps: &HashMap<(u64, u32), GammaRampWords>,
) {
    for path in &snapshot.raw.paths {
        if path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG == 0 {
            continue;
        }
        let key = (
            luid_to_u64(path.targetInfo.adapterId.HighPart, path.targetInfo.adapterId.LowPart),
            path.targetInfo.id,
        );
        let Some(ramp) = ramps.get(&key) else { continue };
        let Some(device_name) = source_gdi_device_name(path) else { continue };
        let _ = set_gamma_ramp_for_device(&device_name, ramp);
    }
}

fn best_effort_restore_wallpapers(
    snapshot: &TopologySnapshot,
    wallpapers: &HashMap<(u64, u32), String>,
) {
    if wallpapers.is_empty() {
        return;
    }
    let Some(session) = create_desktop_wallpaper_session() else { return };
    for path in &snapshot.raw.paths {
        if path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG == 0 {
            continue;
        }
        let key = (
            luid_to_u64(path.targetInfo.adapterId.HighPart, path.targetInfo.adapterId.LowPart),
            path.targetInfo.id,
        );
        let Some(wallpaper_path) = wallpapers.get(&key) else { continue };
        let Some(monitor_device_path) = target_monitor_device_path(path) else { continue };
        let _ = set_wallpaper_for_monitor(&session.desktop_wallpaper, &monitor_device_path, wallpaper_path);
    }
}

fn capture_wallpaper_position() -> Option<DESKTOP_WALLPAPER_POSITION> {
    let session = create_desktop_wallpaper_session()?;
    unsafe { session.desktop_wallpaper.GetPosition().ok() }
}

fn best_effort_restore_wallpaper_position(position: Option<DESKTOP_WALLPAPER_POSITION>) {
    let Some(position) = position else { return };
    let Some(session) = create_desktop_wallpaper_session() else { return };
    let _ = unsafe { session.desktop_wallpaper.SetPosition(position) };
}

fn desired_output_index(desired: &Layout) -> HashMap<(u64, u32), &monarch::OutputConfig> {
    desired
        .outputs
        .iter()
        .map(|output| {
            ((output.display_id.adapter_luid, output.display_id.target_id), output)
        })
        .collect()
}

fn path_target_key(path: &DISPLAYCONFIG_PATH_INFO) -> (u64, u32) {
    (
        luid_to_u64(path.targetInfo.adapterId.HighPart, path.targetInfo.adapterId.LowPart),
        path.targetInfo.id,
    )
}

fn apply_desired_source_mode(
    path: &DISPLAYCONFIG_PATH_INFO,
    modes: &mut [DISPLAYCONFIG_MODE_INFO],
    desired_output: Option<&&monarch::OutputConfig>,
) {
    let Some(output) = desired_output.copied() else { return };
    let mode_index = unsafe { path.sourceInfo.Anonymous.modeInfoIdx } as usize;
    let Some(mode) = modes.get_mut(mode_index) else { return };
    if mode.infoType.0 != DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE.0 {
        return;
    }
    unsafe {
        let source = &mut mode.Anonymous.sourceMode;
        source.position.x = output.position.x;
        source.position.y = output.position.y;
        source.width = output.resolution.width;
        source.height = output.resolution.height;
    }
}

fn apply_desired_target_refresh(
    path: &mut DISPLAYCONFIG_PATH_INFO,
    desired_output: Option<&&monarch::OutputConfig>,
) {
    let Some(output) = desired_output.copied() else { return };
    let desired_refresh_mhz = output.refresh_rate_mhz.max(1);
    path.targetInfo.refreshRate.Numerator = desired_refresh_mhz;
    path.targetInfo.refreshRate.Denominator = 1000;
}

fn reorder_paths_for_desired_priority(
    paths: &mut [DISPLAYCONFIG_PATH_INFO],
    desired_outputs: &HashMap<(u64, u32), &monarch::OutputConfig>,
) {
    paths.sort_by(|left, right| {
        let left_rank = path_priority_rank(left, desired_outputs);
        let right_rank = path_priority_rank(right, desired_outputs);
        left_rank.cmp(&right_rank)
    });
}

fn path_priority_rank(
    path: &DISPLAYCONFIG_PATH_INFO,
    desired_outputs: &HashMap<(u64, u32), &monarch::OutputConfig>,
) -> (u8, i32, i32, u64, u32) {
    let key = path_target_key(path);
    let Some(output) = desired_outputs.get(&key) else {
        return (3, 0, 0, key.0, key.1);
    };
    if !output.enabled {
        return (2, 0, 0, key.0, key.1);
    }
    let bucket = if output.primary { 0 } else { 1 };
    (bucket, output.position.y, output.position.x, key.0, key.1)
}

pub(super) fn target_advanced_color_enabled(path: &DISPLAYCONFIG_PATH_INFO) -> Option<bool> {
    unsafe {
        let mut info = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO::default();
        info.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
            size: size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>() as u32,
            adapterId: path.targetInfo.adapterId,
            id: path.targetInfo.id,
        };
        let status = DisplayConfigGetDeviceInfo(&mut info.header);
        if status != 0 {
            return None;
        }
        let flags = info.Anonymous.value;
        Some((flags & (1 << 1)) != 0)
    }
}

fn source_gdi_device_name(path: &DISPLAYCONFIG_PATH_INFO) -> Option<String> {
    unsafe {
        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
        source.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
            size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
            adapterId: path.sourceInfo.adapterId,
            id: path.sourceInfo.id,
        };
        let status = DisplayConfigGetDeviceInfo(&mut source.header);
        if status != 0 {
            return None;
        }
        Some(super::enumerate::wide_array_to_string(&source.viewGdiDeviceName))
    }
}

fn target_monitor_device_path(path: &DISPLAYCONFIG_PATH_INFO) -> Option<String> {
    unsafe {
        let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
        target.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
            adapterId: path.targetInfo.adapterId,
            id: path.targetInfo.id,
        };
        let status = DisplayConfigGetDeviceInfo(&mut target.header);
        if status != 0 {
            return None;
        }
        Some(super::enumerate::wide_array_to_string(&target.monitorDevicePath))
    }
}

fn get_gamma_ramp_for_device(device_name: &str) -> Option<GammaRampWords> {
    let hdc = create_display_dc(device_name)?;
    let mut ramp = [0u16; GAMMA_RAMP_WORDS];
    let ok = unsafe { GetDeviceGammaRamp(hdc, ramp.as_mut_ptr().cast()) }.as_bool();
    unsafe { let _ = DeleteDC(hdc); }
    if ok { Some(ramp) } else { None }
}

fn set_gamma_ramp_for_device(device_name: &str, ramp: &GammaRampWords) -> bool {
    let Some(hdc) = create_display_dc(device_name) else { return false };
    let ok = unsafe { SetDeviceGammaRamp(hdc, ramp.as_ptr().cast()) }.as_bool();
    unsafe { let _ = DeleteDC(hdc); }
    ok
}

fn create_display_dc(device_name: &str) -> Option<windows::Win32::Graphics::Gdi::HDC> {
    let device_wide = to_wide_null(device_name);
    let hdc = unsafe {
        CreateDCW(w!("DISPLAY"), PCWSTR(device_wide.as_ptr()), PCWSTR::null(), None)
    };
    if hdc.is_invalid() { None } else { Some(hdc) }
}

fn to_wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(std::iter::once(0)).collect()
}

struct DesktopWallpaperSession {
    desktop_wallpaper: IDesktopWallpaper,
    should_uninitialize: bool,
}

impl Drop for DesktopWallpaperSession {
    fn drop(&mut self) {
        if self.should_uninitialize {
            unsafe { CoUninitialize(); }
        }
    }
}

fn create_desktop_wallpaper_session() -> Option<DesktopWallpaperSession> {
    let mut should_uninitialize = false;
    unsafe {
        if CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() {
            should_uninitialize = true;
        }
        let desktop_wallpaper: IDesktopWallpaper =
            CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL).ok()?;
        Some(DesktopWallpaperSession { desktop_wallpaper, should_uninitialize })
    }
}

fn get_wallpaper_for_monitor(
    desktop_wallpaper: &IDesktopWallpaper,
    monitor_device_path: &str,
) -> Option<String> {
    let monitor_wide = to_wide_null(monitor_device_path);
    let wallpaper = unsafe {
        desktop_wallpaper.GetWallpaper(PCWSTR(monitor_wide.as_ptr())).ok()?
    };
    let wallpaper_path = unsafe { wallpaper.to_string().ok() };
    unsafe { CoTaskMemFree(Some(wallpaper.0.cast())); }
    wallpaper_path
}

fn set_wallpaper_for_monitor(
    desktop_wallpaper: &IDesktopWallpaper,
    monitor_device_path: &str,
    wallpaper_path: &str,
) -> bool {
    let monitor_wide = to_wide_null(monitor_device_path);
    let wallpaper_wide = to_wide_null(wallpaper_path);
    unsafe {
        desktop_wallpaper
            .SetWallpaper(PCWSTR(monitor_wide.as_ptr()), PCWSTR(wallpaper_wide.as_ptr()))
            .is_ok()
    }
}
