#![cfg(target_os = "windows")]

use std::collections::HashMap;
use std::hash::Hasher;
use std::mem::size_of;

use monarch::{DisplayId, DisplayInfo, Layout, ManagerError, OutputConfig, Position, Resolution};
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO,
    DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE, DISPLAYCONFIG_MODE_INFO_TYPE_TARGET,
    DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME,
    DISPLAYCONFIG_ROTATION, DISPLAYCONFIG_ROTATION_ROTATE90, DISPLAYCONFIG_ROTATION_ROTATE270,
    QDC_ALL_PATHS, QDC_ONLY_ACTIVE_PATHS, QUERY_DISPLAY_CONFIG_FLAGS,
};
use windows::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;

use super::win32_types::{luid_to_u64, make_display_id, RawTopologySnapshot, TopologySnapshot};

const DISPLAYCONFIG_PATH_ACTIVE_FLAG: u32 = 0x0000_0001;
const QUERY_FLAGS: QUERY_DISPLAY_CONFIG_FLAGS = QDC_ONLY_ACTIVE_PATHS;

pub(super) fn query_all_paths() -> Result<(Vec<DISPLAYCONFIG_PATH_INFO>, Vec<DISPLAYCONFIG_MODE_INFO>), ManagerError> {
    query_raw(QDC_ALL_PATHS)
}

pub fn query_active_topology() -> Result<TopologySnapshot, ManagerError> {
    let (paths, modes) = query_raw(QUERY_FLAGS)?;
    let raw = RawTopologySnapshot {
        paths: paths.clone(),
        modes: modes.clone(),
    };

    let mut displays = Vec::<DisplayInfo>::new();
    let mut outputs = Vec::new();
    let mut gdi_names = HashMap::new();
    let mut rotation_values: HashMap<(u64, u32), u32> = HashMap::new();
    let mut clone_pairs: HashMap<(u64, u32), (u64, u32)> = HashMap::new();
    let mode_map = modes_by_key(&modes);

    // Track (adapter_high, adapter_low, sourceInfo.id) → first target's (luid, target_id).
    // If two active paths share the same source slot, the second is a clone of the first.
    let mut source_slot_to_target: HashMap<(i32, u32, u32), (u64, u32)> = HashMap::new();

    for path in &paths {
        if path.flags & DISPLAYCONFIG_PATH_ACTIVE_FLAG == 0 {
            continue;
        }

        let adapter_luid = luid_to_u64(
            path.targetInfo.adapterId.HighPart,
            path.targetInfo.adapterId.LowPart,
        );
        let target_key_self = (adapter_luid, path.targetInfo.id);
        let (friendly_name, stable_edid_hash) = target_name_and_stable_hash(path)
            .unwrap_or_else(|_| {
                (format!("Display {}:{}", adapter_luid, path.targetInfo.id), None)
            });
        let display_id = make_display_id(adapter_luid, path.targetInfo.id, stable_edid_hash);

        if let Ok(gdi_name) = source_gdi_device_name(path) {
            gdi_names.insert(target_key_self, gdi_name);
        }

        // Clone detection: two active paths sharing the same (adapterId, sourceInfo.id).
        let source_slot_key = (
            path.sourceInfo.adapterId.HighPart,
            path.sourceInfo.adapterId.LowPart,
            path.sourceInfo.id,
        );
        if let Some(&src_target) = source_slot_to_target.get(&source_slot_key) {
            clone_pairs.insert(target_key_self, src_target);
        } else {
            source_slot_to_target.insert(source_slot_key, target_key_self);
        }

        let source_key = (
            path.sourceInfo.adapterId.HighPart,
            path.sourceInfo.adapterId.LowPart,
            path.sourceInfo.id,
            DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE.0 as u32,
        );
        let target_key = (
            path.targetInfo.adapterId.HighPart,
            path.targetInfo.adapterId.LowPart,
            path.targetInfo.id,
            DISPLAYCONFIG_MODE_INFO_TYPE_TARGET.0 as u32,
        );

        let (position, source_resolution) = mode_map
            .get(&source_key)
            .map(source_mode_position_and_resolution)
            .transpose()?
            .unwrap_or((Position { x: 0, y: 0 }, Resolution { width: 0, height: 0 }));

        let refresh_rate_mhz = mode_map
            .get(&target_key)
            .map(target_mode_refresh_mhz)
            .transpose()?
            .unwrap_or(60_000);

        let rotation = rotation_to_degrees(path.targetInfo.rotation);
        if rotation != 0 {
            rotation_values.insert(target_key_self, rotation);
        }

        let display_resolution =
            effective_resolution_for_rotation(source_resolution.clone(), path.targetInfo.rotation);

        let display = DisplayInfo {
            id: display_id,
            friendly_name,
            is_active: true,
            is_primary: position.x == 0 && position.y == 0,
            resolution: display_resolution,
            refresh_rate_mhz,
        };
        outputs.push(OutputConfig {
            display_id: display.id.clone(),
            enabled: true,
            position,
            resolution: source_resolution,
            refresh_rate_mhz: display.refresh_rate_mhz,
            primary: display.is_primary,
        });
        displays.push(display);
    }

    if !outputs.iter().any(|o| o.primary && o.enabled) {
        if let Some(first) = outputs.iter_mut().find(|o| o.enabled) {
            first.primary = true;
        }
        if let Some(first_display) = displays.iter_mut().find(|d| d.is_active) {
            first_display.is_primary = true;
        }
    }

    Ok(TopologySnapshot { raw, layout: Layout { outputs }, displays, gdi_names, rotation_values, clone_pairs })
}

fn rotation_to_degrees(rotation: DISPLAYCONFIG_ROTATION) -> u32 {
    match rotation.0 {
        2 => 90,
        3 => 180,
        4 => 270,
        _ => 0,
    }
}

fn effective_resolution_for_rotation(
    source_resolution: Resolution,
    rotation: DISPLAYCONFIG_ROTATION,
) -> Resolution {
    if rotation == DISPLAYCONFIG_ROTATION_ROTATE90 || rotation == DISPLAYCONFIG_ROTATION_ROTATE270 {
        return Resolution { width: source_resolution.height, height: source_resolution.width };
    }
    source_resolution
}

fn query_raw(
    flags: QUERY_DISPLAY_CONFIG_FLAGS,
) -> Result<(Vec<DISPLAYCONFIG_PATH_INFO>, Vec<DISPLAYCONFIG_MODE_INFO>), ManagerError> {
    unsafe {
        let mut path_count = 0u32;
        let mut mode_count = 0u32;

        let mut status =
            GetDisplayConfigBufferSizes(flags, &mut path_count, &mut mode_count);
        if status.0 != 0 {
            return Err(ManagerError::Backend(format!(
                "GetDisplayConfigBufferSizes failed: {}",
                status.0
            )));
        }

        loop {
            let mut paths =
                vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
            let mut modes =
                vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
            let mut out_paths = path_count;
            let mut out_modes = mode_count;

            status = QueryDisplayConfig(
                flags,
                &mut out_paths,
                paths.as_mut_ptr(),
                &mut out_modes,
                modes.as_mut_ptr(),
                None,
            );

            if status == ERROR_INSUFFICIENT_BUFFER {
                let retry = GetDisplayConfigBufferSizes(
                    flags,
                    &mut path_count,
                    &mut mode_count,
                );
                if retry.0 != 0 {
                    return Err(ManagerError::Backend(format!(
                        "GetDisplayConfigBufferSizes retry failed: {}",
                        retry.0
                    )));
                }
                continue;
            }
            if status.0 != 0 {
                return Err(ManagerError::Backend(format!(
                    "QueryDisplayConfig failed: {}",
                    status.0
                )));
            }
            paths.truncate(out_paths as usize);
            modes.truncate(out_modes as usize);
            return Ok((paths, modes));
        }
    }
}

fn target_name_and_stable_hash(
    path: &DISPLAYCONFIG_PATH_INFO,
) -> Result<(String, Option<u64>), ManagerError> {
    unsafe {
        let mut name = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
        name.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
            adapterId: path.targetInfo.adapterId,
            id: path.targetInfo.id,
        };

        let status = DisplayConfigGetDeviceInfo(&mut name.header);
        if status != 0 {
            return Err(ManagerError::Backend(format!(
                "DisplayConfigGetDeviceInfo failed: {}",
                status
            )));
        }

        let friendly_name = wide_array_to_string(&name.monitorFriendlyDeviceName);
        let device_path = wide_array_to_string(&name.monitorDevicePath);
        let stable_hash = stable_display_hash(
            name.edidManufactureId,
            name.edidProductCodeId,
            name.connectorInstance,
            &device_path,
        );
        Ok((friendly_name, Some(stable_hash)))
    }
}

fn stable_display_hash(
    edid_manufacture_id: u16,
    edid_product_code_id: u16,
    connector_instance: u32,
    monitor_device_path: &str,
) -> u64 {
    let mut hasher = Fnv1a64::new();
    hasher.update(&edid_manufacture_id.to_le_bytes());
    hasher.update(&edid_product_code_id.to_le_bytes());
    hasher.update(&connector_instance.to_le_bytes());
    let normalized_path = monitor_device_path.to_ascii_uppercase();
    hasher.update(normalized_path.as_bytes());
    hasher.finish()
}

struct Fnv1a64(u64);

impl Fnv1a64 {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x0000_0100_0000_01B3;

    fn new() -> Self {
        Self(Self::OFFSET_BASIS)
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= *byte as u64;
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }
}

impl Hasher for Fnv1a64 {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        self.update(bytes);
    }
}

pub fn wide_array_to_string(wide: &[u16]) -> String {
    let len = wide.iter().position(|ch| *ch == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..len])
}

fn modes_by_key(
    modes: &[DISPLAYCONFIG_MODE_INFO],
) -> HashMap<(i32, u32, u32, u32), DISPLAYCONFIG_MODE_INFO> {
    let mut map = HashMap::with_capacity(modes.len());
    for mode in modes.iter().cloned() {
        map.insert(
            (
                mode.adapterId.HighPart,
                mode.adapterId.LowPart,
                mode.id,
                mode.infoType.0 as u32,
            ),
            mode,
        );
    }
    map
}

fn source_mode_position_and_resolution(
    mode: &DISPLAYCONFIG_MODE_INFO,
) -> Result<(Position, Resolution), ManagerError> {
    unsafe {
        let source = mode.Anonymous.sourceMode;
        Ok((
            Position { x: source.position.x, y: source.position.y },
            Resolution { width: source.width, height: source.height },
        ))
    }
}

fn target_mode_refresh_mhz(mode: &DISPLAYCONFIG_MODE_INFO) -> Result<u32, ManagerError> {
    unsafe {
        let target = mode.Anonymous.targetMode;
        let numerator = target.targetVideoSignalInfo.vSyncFreq.Numerator;
        let denominator = target.targetVideoSignalInfo.vSyncFreq.Denominator.max(1);
        Ok(((numerator as u64 * 1000) / denominator as u64) as u32)
    }
}

fn source_gdi_device_name(path: &DISPLAYCONFIG_PATH_INFO) -> Result<String, ManagerError> {
    unsafe {
        let mut info = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
        info.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
            size: std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
            adapterId: path.sourceInfo.adapterId,
            id: path.sourceInfo.id,
        };
        let status = DisplayConfigGetDeviceInfo(&mut info.header);
        if status != 0 {
            return Err(ManagerError::Backend(format!(
                "DisplayConfigGetDeviceInfo(GET_SOURCE_NAME) failed: {status}"
            )));
        }
        Ok(wide_array_to_string(&info.viewGdiDeviceName))
    }
}
