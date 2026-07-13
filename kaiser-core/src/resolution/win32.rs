#![cfg(target_os = "windows")]

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use serde::{Deserialize, Serialize};
use windows::core::PCWSTR;
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
