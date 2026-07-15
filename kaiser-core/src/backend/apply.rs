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
    SDC_ALLOW_CHANGES, SDC_ALLOW_PATH_ORDER_CHANGES, SDC_APPLY, SDC_NO_OPTIMIZATION,
    SDC_SAVE_TO_DATABASE, SDC_TOPOLOGY_EXTEND, SDC_USE_SUPPLIED_DISPLAY_CONFIG,
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

/// Two-phase apply mirroring the PowerShell kaiser.ps1 approach.
///
/// Phase 1 (EnableMonitorByName): inactive desired targets are appended to the
/// **active** paths array (active modes untouched) and SetDisplayConfig is called.
/// We then poll until every target appears in QDC_ONLY_ACTIVE_PATHS.
///
/// Phase 2 (DeactivateAllExcept + SetPrimaryByName): fresh QDC_ONLY_ACTIVE_PATHS
/// is queried; unwanted paths get their active flag cleared (mode indices kept);
/// all source-mode positions are shifted so the desired primary lands at (0,0);
/// SetDisplayConfig is called.
pub fn apply_layout_against_snapshot(
    desired: &Layout,
    snapshot: &TopologySnapshot,
) -> Result<TopologySnapshot, ManagerError> {
    desired.ensure_valid()?;
    let saved_gamma_ramps = capture_active_gamma_ramps(snapshot);
    let saved_wallpapers = capture_active_wallpapers(snapshot);
    let saved_wallpaper_position = capture_wallpaper_position();

    let desired_outputs = desired_output_index(desired);

    let currently_active_keys: std::collections::HashSet<(u64, u32)> = snapshot
        .raw.paths.iter()
        .filter(|p| p.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG != 0)
        .map(|p| path_target_key(p))
        .collect();

    let needs_enable: Vec<(u64, u32)> = desired.outputs.iter()
        .filter(|o| o.enabled)
        .map(|o| (o.display_id.adapter_luid, o.display_id.target_id))
        .filter(|key| !currently_active_keys.contains(key))
        .collect();

    // --- Phase 1: bring inactive targets online (mirrors EnableMonitorByName) ---
    if !needs_enable.is_empty() {
        log::info!(
            "apply phase1: enabling {} inactive target(s): {:?}",
            needs_enable.len(),
            needs_enable
        );
        match phase1_enable_targets(&needs_enable, &snapshot.raw.paths, &snapshot.raw.modes) {
            Ok(()) => {
                log::info!("apply phase1: SetDisplayConfig OK — polling for topology settle (up to 12s)");
                if !wait_for_targets_active(&needs_enable, 12_000) {
                    log::warn!("apply phase1: target(s) did not appear in active topology within 12s, proceeding anyway");
                } else {
                    log::info!("apply phase1: all targets confirmed active");
                }
            }
            Err(e) => {
                log::warn!("apply phase1: failed ({e}); proceeding to phase 2 with current topology");
            }
        }
    }

    // --- Phase 2: query fresh active set, disable unwanted, normalise positions ---
    let phase2_snap = super::enumerate::query_active_topology()?;
    let mut next_paths = phase2_snap.raw.paths.clone();
    let mut next_modes = phase2_snap.raw.modes.clone();

    log::info!(
        "apply phase2: fresh active has {} paths; desired enabled: {}",
        next_paths.len(),
        desired.outputs.iter().filter(|o| o.enabled).count()
    );

    for path in &mut next_paths {
        let key = path_target_key(path);
        let desired_output = desired_outputs.get(&key);
        let should_be_active = desired_output.map(|o| o.enabled).unwrap_or(false);
        let mode_idx = unsafe { path.sourceInfo.Anonymous.modeInfoIdx };

        log::debug!(
            "apply phase2: path {:016x}:{} active={} desired={} mode_idx={:#x}",
            key.0, key.1,
            (path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG) != 0,
            should_be_active,
            mode_idx
        );

        if should_be_active {
            path.flags |= DISPLAYCONFIG_PATH_ACTIVE_FLAG;
            if mode_idx != 0xFFFF_FFFF {
                apply_desired_source_mode(path, &mut next_modes, desired_output);
            }
            apply_desired_target_refresh(path, desired_output);
        } else {
            // Mirror PS DeactivateAllExcept: clear flag, keep mode indices intact.
            path.flags &= !DISPLAYCONFIG_PATH_ACTIVE_FLAG;
        }
    }

    // Normalise all source-mode positions so the desired primary lands at (0,0).
    // Mirrors PS SetPrimaryByName which subtracts the primary's current offset from
    // every source mode — required so Windows accepts the config when only the
    // newly-enabled display remains active.
    normalize_primary_position(&next_paths, &mut next_modes, &desired_outputs);

    // Safety: refuse to call SetDisplayConfig with zero active paths — that would
    // always be rejected and could leave the desktop in an unusable state.
    let active_count = next_paths.iter()
        .filter(|p| p.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG != 0)
        .count();
    if active_count == 0 {
        return Err(ManagerError::Backend(
            "phase2: all paths would be inactive — refusing to apply (would black-screen)".to_string(),
        ));
    }
    log::info!("apply phase2: {} path(s) will be active", active_count);

    reorder_paths_for_desired_priority(&mut next_paths, &desired_outputs);

    unsafe {
        let exact_flags = SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_SAVE_TO_DATABASE | SDC_NO_OPTIMIZATION;
        let mut status = SetDisplayConfig(Some(next_paths.as_slice()), Some(next_modes.as_slice()), exact_flags);
        if status != 0 {
            log::debug!("apply phase2: exact flags failed ({status}), retrying with ALLOW_CHANGES");
            status = SetDisplayConfig(
                Some(next_paths.as_slice()),
                Some(next_modes.as_slice()),
                SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_SAVE_TO_DATABASE
                    | SDC_ALLOW_CHANGES | SDC_ALLOW_PATH_ORDER_CHANGES,
            );
        }
        if status != 0 {
            return Err(ManagerError::Backend(format!("SetDisplayConfig failed: {}", status)));
        }
    }

    let next_snapshot = super::enumerate::query_active_topology()?;
    best_effort_reload_color_calibration();
    best_effort_restore_gamma_ramps(&next_snapshot, &saved_gamma_ramps);
    best_effort_restore_wallpapers(&next_snapshot, &saved_wallpapers);
    best_effort_restore_wallpaper_position(saved_wallpaper_position);
    Ok(next_snapshot)
}

/// Mirrors PS `SetPrimaryByName`: find the desired primary in the modes array and
/// subtract its current (x,y) offset from every source-mode entry. This ensures
/// the primary always lands at (0,0) as Windows requires.
fn normalize_primary_position(
    paths: &[DISPLAYCONFIG_PATH_INFO],
    modes: &mut Vec<DISPLAYCONFIG_MODE_INFO>,
    desired_outputs: &HashMap<(u64, u32), &monarch::OutputConfig>,
) {
    // Find the desired primary's current mode index
    let primary_mode_idx = paths.iter()
        .filter(|p| p.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG != 0)
        .filter_map(|p| {
            let key = path_target_key(p);
            let output = desired_outputs.get(&key)?;
            if !output.primary { return None; }
            let idx = unsafe { p.sourceInfo.Anonymous.modeInfoIdx };
            if idx == 0xFFFF_FFFF { return None; }
            Some(idx as usize)
        })
        .next();

    let Some(pidx) = primary_mode_idx else { return };
    let Some(primary_mode) = modes.get(pidx) else { return };
    if primary_mode.infoType.0 != DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE.0 { return; }

    let (offset_x, offset_y) = unsafe {
        (primary_mode.Anonymous.sourceMode.position.x,
         primary_mode.Anonymous.sourceMode.position.y)
    };

    if offset_x == 0 && offset_y == 0 {
        log::debug!("normalize_primary_position: primary already at (0,0), no shift needed");
        return;
    }

    log::info!(
        "normalize_primary_position: shifting all source modes by (-{}, -{})",
        offset_x, offset_y
    );
    for mode in modes.iter_mut() {
        if mode.infoType.0 == DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE.0 {
            unsafe {
                mode.Anonymous.sourceMode.position.x -= offset_x;
                mode.Anonymous.sourceMode.position.y -= offset_y;
            }
        }
    }
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
                unsafe {
                    p.sourceInfo.Anonymous.modeInfoIdx = 0xFFFF_FFFF;
                    p.targetInfo.Anonymous.modeInfoIdx = 0xFFFF_FFFF;
                }
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
        unsafe {
            inactive_path.sourceInfo.Anonymous.modeInfoIdx = 0xFFFF_FFFF;
            inactive_path.targetInfo.Anonymous.modeInfoIdx = 0xFFFF_FFFF;
        }
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
