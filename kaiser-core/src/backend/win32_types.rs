#![cfg(target_os = "windows")]

use std::collections::HashMap;

use monarch::{DisplayId, DisplayInfo, Layout};
use windows::Win32::Devices::Display::{DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO};

#[derive(Clone)]
pub struct RawTopologySnapshot {
    pub paths: Vec<DISPLAYCONFIG_PATH_INFO>,
    pub modes: Vec<DISPLAYCONFIG_MODE_INFO>,
}

#[derive(Clone)]
pub struct TopologySnapshot {
    pub raw: RawTopologySnapshot,
    pub layout: Layout,
    pub displays: Vec<DisplayInfo>,
    /// Maps (adapter_luid, target_id) → GDI source device name (e.g. "\\.\DISPLAY1")
    pub gdi_names: HashMap<(u64, u32), String>,
}

pub fn luid_to_u64(high_part: i32, low_part: u32) -> u64 {
    ((high_part as i64 as u64) << 32) | (low_part as u64)
}

pub fn make_display_id(adapter_luid: u64, target_id: u32, edid_hash: Option<u64>) -> DisplayId {
    DisplayId {
        adapter_luid,
        target_id,
        edid_hash,
    }
}
