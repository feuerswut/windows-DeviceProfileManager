#![cfg(target_os = "windows")]

use std::collections::HashMap;

use monarch::{DisplayId, DisplayInfo, Layout};
use serde::{Deserialize, Serialize};
use windows::Win32::Devices::Display::{DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO};

/// Raw EDID fields extracted per-display. Fields are None when the display
/// reports zero (virtual/headless displays with no real EDID).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EdidFields {
    pub manufacture_id: Option<u16>,
    pub product_code: Option<u16>,
    pub connector_instance: Option<u32>,
}

impl EdidFields {
    pub fn has_any(&self) -> bool {
        self.manufacture_id.is_some()
            || self.product_code.is_some()
            || self.connector_instance.is_some()
    }

    /// Stable hash from the EDID fields only — no device path, so it survives
    /// adapter LUID changes across reboots. Returns None for virtual displays
    /// (all fields None).
    pub fn compute_hash(&self) -> Option<u64> {
        if !self.has_any() {
            return None;
        }
        const OFFSET: u64 = 0xcbf29ce484222325;
        const PRIME: u64 = 0x0000_0100_0000_01B3;
        let mut h = OFFSET;
        let feed = |h: &mut u64, bytes: &[u8]| {
            for &b in bytes {
                *h ^= b as u64;
                *h = h.wrapping_mul(PRIME);
            }
        };
        // Sentinel bytes separate fields so (mfr=1,prod=None) ≠ (mfr=None,prod=1).
        if let Some(v) = self.manufacture_id {
            feed(&mut h, &[0x01]);
            feed(&mut h, &v.to_le_bytes());
        }
        if let Some(v) = self.product_code {
            feed(&mut h, &[0x02]);
            feed(&mut h, &v.to_le_bytes());
        }
        if let Some(v) = self.connector_instance {
            feed(&mut h, &[0x03]);
            feed(&mut h, &v.to_le_bytes());
        }
        Some(h)
    }

    /// Returns true if both sides have EDID info and all non-None fields match.
    /// Returns false if either side has no EDID info (virtual/headless displays
    /// should not be matched by EDID).
    pub fn matches(&self, other: &EdidFields) -> bool {
        if !self.has_any() || !other.has_any() {
            return false;
        }
        self.manufacture_id.map_or(true, |v| other.manufacture_id == Some(v))
            && self.product_code.map_or(true, |v| other.product_code == Some(v))
            && self.connector_instance.map_or(true, |v| other.connector_instance == Some(v))
    }
}

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
    /// Current rotation in degrees per display: 0, 90, 180, or 270.
    pub rotation_values: HashMap<(u64, u32), u32>,
    /// Clone relationships: (clone_adapter_luid, clone_target_id) → (source_adapter_luid, source_target_id).
    pub clone_pairs: HashMap<(u64, u32), (u64, u32)>,
    /// Raw EDID fields per display, keyed by (adapter_luid, target_id).
    pub edid_fields: HashMap<(u64, u32), EdidFields>,
}

pub fn luid_to_u64(high_part: i32, low_part: u32) -> u64 {
    ((high_part as i64 as u64) << 32) | (low_part as u64)
}

pub fn make_display_id(adapter_luid: u64, target_id: u32, edid: &EdidFields) -> DisplayId {
    DisplayId {
        adapter_luid,
        target_id,
        edid_hash: edid.compute_hash(),
    }
}
