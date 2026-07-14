#![cfg(target_os = "windows")]

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use serde::{Deserialize, Serialize};
use windows::core::PCWSTR;
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, DisplayConfigSetDeviceInfo, GetDisplayConfigBufferSizes,
    QueryDisplayConfig, DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_DEVICE_INFO_TYPE,
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, QDC_ONLY_ACTIVE_PATHS,
};
use windows::Win32::Foundation::LUID;
use windows::Win32::Graphics::Gdi::{
    ChangeDisplaySettingsExW, EnumDisplaySettingsW, CDS_UPDATEREGISTRY,
    DEVMODEW, DISP_CHANGE_SUCCESSFUL, ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    pub refresh_rate_hz: u32,
    pub bit_depth: u32,
}

pub fn list_display_modes(gdi_device_name: &str) -> anyhow::Result<Vec<DisplayMode>> {
    let device_wide = to_wide_null(gdi_device_name);
    let mut modes = BTreeSet::new();
    let mut devmode = DEVMODEW::default();
    devmode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;

    let mut i: u32 = 0;
    loop {
        let ok = unsafe {
            EnumDisplaySettingsW(
                PCWSTR(device_wide.as_ptr()),
                ENUM_DISPLAY_SETTINGS_MODE(i),
                &mut devmode,
            )
        };
        if !ok.as_bool() {
            break;
        }
        if devmode.dmPelsWidth > 0 && devmode.dmPelsHeight > 0 {
            modes.insert(DisplayMode {
                width: devmode.dmPelsWidth,
                height: devmode.dmPelsHeight,
                refresh_rate_hz: devmode.dmDisplayFrequency,
                bit_depth: devmode.dmBitsPerPel,
            });
        }
        i += 1;
    }
    if modes.is_empty() {
        anyhow::bail!("no display modes found for device: {gdi_device_name}");
    }
    Ok(modes.into_iter().collect())
}

pub fn set_display_mode(gdi_device_name: &str, mode: &DisplayMode) -> anyhow::Result<()> {
    let device_wide = to_wide_null(gdi_device_name);
    let mut devmode = DEVMODEW::default();
    devmode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    devmode.dmFields = windows::Win32::Graphics::Gdi::DM_PELSWIDTH
        | windows::Win32::Graphics::Gdi::DM_PELSHEIGHT
        | windows::Win32::Graphics::Gdi::DM_DISPLAYFREQUENCY
        | windows::Win32::Graphics::Gdi::DM_BITSPERPEL;
    devmode.dmPelsWidth = mode.width;
    devmode.dmPelsHeight = mode.height;
    devmode.dmDisplayFrequency = mode.refresh_rate_hz;
    devmode.dmBitsPerPel = mode.bit_depth;

    let result = unsafe {
        ChangeDisplaySettingsExW(
            PCWSTR(device_wide.as_ptr()),
            Some(&devmode),
            None,
            CDS_UPDATEREGISTRY,
            None,
        )
    };

    if result == DISP_CHANGE_SUCCESSFUL {
        Ok(())
    } else {
        anyhow::bail!(
            "ChangeDisplaySettingsEx failed for {gdi_device_name}: code {}",
            result.0
        )
    }
}

#[allow(dead_code)]
pub fn get_current_display_mode(gdi_device_name: &str) -> anyhow::Result<DisplayMode> {
    let device_wide = to_wide_null(gdi_device_name);
    let mut devmode = DEVMODEW::default();
    devmode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;

    let ok = unsafe {
        EnumDisplaySettingsW(
            PCWSTR(device_wide.as_ptr()),
            ENUM_CURRENT_SETTINGS,
            &mut devmode,
        )
    };
    if !ok.as_bool() {
        anyhow::bail!("failed to get current display mode for {gdi_device_name}");
    }
    Ok(DisplayMode {
        width: devmode.dmPelsWidth,
        height: devmode.dmPelsHeight,
        refresh_rate_hz: devmode.dmDisplayFrequency,
        bit_depth: devmode.dmBitsPerPel,
    })
}

fn to_wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(std::iter::once(0)).collect()
}

// ---- Per-monitor DPI via DisplayConfig undocumented API ---------------------
// Port of the C++ DpiHelper implementation. DPI is a property of the CCD
// source (sourceInfo.id), not the target. Values are relative to the OS-
// recommended value for each monitor.

const DPI_VALS: [u32; 12] = [100, 125, 150, 175, 200, 225, 250, 300, 350, 400, 450, 500];

const DISPLAYCONFIG_DEVICE_INFO_GET_DPI_SCALE: i32 = -3;
const DISPLAYCONFIG_DEVICE_INFO_SET_DPI_SCALE: i32 = -4;

// struct sizes are asserted in the C++ reference: GET=0x20, SET=0x18
#[repr(C)]
struct DpiScaleGet {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER, // 20 bytes
    min_scale_rel: i32,                       // offset 20
    cur_scale_rel: i32,                       // offset 24
    max_scale_rel: i32,                       // offset 28 → total 32 = 0x20
}

#[repr(C)]
struct DpiScaleSet {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER, // 20 bytes
    scale_rel: i32,                           // offset 20 → total 24 = 0x18
}

fn luid_from_u64(val: u64) -> LUID {
    LUID {
        LowPart: val as u32,
        HighPart: (val >> 32) as i32,
    }
}

fn luid_to_u64(luid: LUID) -> u64 {
    ((luid.HighPart as u64) << 32) | (luid.LowPart as u64)
}

/// Look up the CCD source ID for a display identified by (adapter_luid, target_id).
/// DPI scaling is a property of the source, not the target.
fn find_source_id(adapter_luid: u64, target_id: u32) -> anyhow::Result<u32> {
    let mut num_paths: u32 = 0;
    let mut num_modes: u32 = 0;
    unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut num_paths, &mut num_modes) }
        .ok()?;

    let mut paths = vec![
        unsafe { std::mem::zeroed::<DISPLAYCONFIG_PATH_INFO>() };
        num_paths as usize
    ];
    let mut modes = vec![
        unsafe { std::mem::zeroed::<DISPLAYCONFIG_MODE_INFO>() };
        num_modes as usize
    ];
    unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut num_paths,
            paths.as_mut_ptr(),
            &mut num_modes,
            modes.as_mut_ptr(),
            None,
        )
    }
    .ok()?;
    paths.truncate(num_paths as usize);

    for path in &paths {
        if luid_to_u64(path.targetInfo.adapterId) == adapter_luid
            && path.targetInfo.id == target_id
        {
            return Ok(path.sourceInfo.id);
        }
    }
    anyhow::bail!("no active CCD path for adapter_luid={adapter_luid} target_id={target_id}")
}

/// Get the current DPI scaling percentage for a specific monitor.
pub fn get_display_dpi(adapter_luid: u64, target_id: u32) -> anyhow::Result<u32> {
    let source_id = find_source_id(adapter_luid, target_id)?;
    let luid = luid_from_u64(adapter_luid);

    let mut req = DpiScaleGet {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_TYPE(DISPLAYCONFIG_DEVICE_INFO_GET_DPI_SCALE),
            size: std::mem::size_of::<DpiScaleGet>() as u32,
            adapterId: luid,
            id: source_id,
        },
        min_scale_rel: 0,
        cur_scale_rel: 0,
        max_scale_rel: 0,
    };

    let res = unsafe {
        DisplayConfigGetDeviceInfo(
            &mut req.header as *mut DISPLAYCONFIG_DEVICE_INFO_HEADER,
        )
    };
    if res != 0 {
        anyhow::bail!("DisplayConfigGetDeviceInfo(GET_DPI_SCALE) failed: {res}");
    }

    let cur = req.cur_scale_rel.max(req.min_scale_rel).min(req.max_scale_rel);
    let min_abs = req.min_scale_rel.unsigned_abs() as usize;
    let idx = min_abs as i32 + cur;
    if idx < 0 || idx as usize >= DPI_VALS.len() {
        anyhow::bail!("DPI index {idx} out of range");
    }
    Ok(DPI_VALS[idx as usize])
}

/// Set the DPI scaling percentage for a specific monitor.
/// percent must be one of: 100, 125, 150, 175, 200, 225, 250, 300 (clamped to monitor max).
pub fn set_display_dpi(adapter_luid: u64, target_id: u32, percent: u32) -> anyhow::Result<()> {
    let source_id = find_source_id(adapter_luid, target_id)?;
    let luid = luid_from_u64(adapter_luid);

    // Query current info to determine recommended value (needed for relative offset)
    let mut get_req = DpiScaleGet {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_TYPE(DISPLAYCONFIG_DEVICE_INFO_GET_DPI_SCALE),
            size: std::mem::size_of::<DpiScaleGet>() as u32,
            adapterId: luid,
            id: source_id,
        },
        min_scale_rel: 0,
        cur_scale_rel: 0,
        max_scale_rel: 0,
    };
    let res = unsafe {
        DisplayConfigGetDeviceInfo(
            &mut get_req.header as *mut DISPLAYCONFIG_DEVICE_INFO_HEADER,
        )
    };
    if res != 0 {
        anyhow::bail!("DisplayConfigGetDeviceInfo(GET_DPI_SCALE) failed: {res}");
    }

    let min_abs = get_req.min_scale_rel.unsigned_abs() as usize;
    let max_idx = (min_abs as i32 + get_req.max_scale_rel) as usize;

    let recommended = *DPI_VALS.get(min_abs)
        .ok_or_else(|| anyhow::anyhow!("recommended DPI index {min_abs} out of DpiVals"))?;
    let maximum = DPI_VALS.get(max_idx).copied().unwrap_or(*DPI_VALS.last().unwrap());

    // Clamp requested percent to monitor's supported range
    let percent = percent.clamp(100, maximum);

    let idx1 = DPI_VALS.iter().position(|&v| v == percent)
        .ok_or_else(|| anyhow::anyhow!("DPI {percent}% not a valid value in DpiVals"))?;
    let idx2 = DPI_VALS.iter().position(|&v| v == recommended)
        .ok_or_else(|| anyhow::anyhow!("recommended DPI {recommended}% not found in DpiVals"))?;
    let scale_rel = idx1 as i32 - idx2 as i32;

    let mut set_req = DpiScaleSet {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_TYPE(DISPLAYCONFIG_DEVICE_INFO_SET_DPI_SCALE),
            size: std::mem::size_of::<DpiScaleSet>() as u32,
            adapterId: luid,
            id: source_id,
        },
        scale_rel,
    };
    let res = unsafe {
        DisplayConfigSetDeviceInfo(
            &mut set_req.header as *mut DISPLAYCONFIG_DEVICE_INFO_HEADER,
        )
    };
    if res != 0 {
        anyhow::bail!("DisplayConfigSetDeviceInfo(SET_DPI_SCALE) failed: {res}");
    }
    Ok(())
}
