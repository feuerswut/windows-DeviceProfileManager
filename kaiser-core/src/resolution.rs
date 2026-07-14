#[cfg(target_os = "windows")]
mod win32;

#[cfg(target_os = "windows")]
pub use win32::{
    get_current_display_mode, get_display_dpi, list_display_modes, set_display_dpi,
    set_display_mode, DisplayMode,
};

#[cfg(not(target_os = "windows"))]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    pub refresh_rate_hz: u32,
    pub bit_depth: u32,
}

#[cfg(not(target_os = "windows"))]
pub fn list_display_modes(_gdi_device_name: &str) -> anyhow::Result<Vec<DisplayMode>> {
    Err(anyhow::anyhow!("display modes not supported on this platform"))
}

#[cfg(not(target_os = "windows"))]
pub fn set_display_mode(_gdi_device_name: &str, _mode: &DisplayMode) -> anyhow::Result<()> {
    Err(anyhow::anyhow!("display modes not supported on this platform"))
}

#[cfg(not(target_os = "windows"))]
pub fn get_display_dpi(_adapter_luid: u64, _target_id: u32) -> anyhow::Result<u32> {
    Ok(100)
}

#[cfg(not(target_os = "windows"))]
pub fn set_display_dpi(_adapter_luid: u64, _target_id: u32, _percent: u32) -> anyhow::Result<()> {
    Err(anyhow::anyhow!("DPI scaling not supported on this platform"))
}
