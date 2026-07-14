#[cfg(target_os = "windows")]
mod win32;

#[cfg(target_os = "windows")]
pub use win32::{AudioDevice, AudioFlow, AudioManager};

#[cfg(not(target_os = "windows"))]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub flow: AudioFlow,
    pub enabled: bool,
    pub volume: f32,
    pub muted: bool,
    pub is_default: bool,
}

#[cfg(not(target_os = "windows"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFlow {
    Render,
    Capture,
}

#[cfg(not(target_os = "windows"))]
pub struct AudioManager;

#[cfg(not(target_os = "windows"))]
impl AudioManager {
    pub fn new() -> Self {
        Self
    }

    pub fn list_devices(&self) -> anyhow::Result<Vec<AudioDevice>> {
        Err(anyhow::anyhow!("audio not supported on this platform"))
    }

    pub fn set_volume(&self, _device_id: &str, _volume: f32) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("audio not supported on this platform"))
    }

    pub fn set_mute(&self, _device_id: &str, _muted: bool) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("audio not supported on this platform"))
    }

    pub fn set_default(&self, _device_id: &str) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("audio not supported on this platform"))
    }

    pub fn set_enabled(&self, _device_id: &str, _enabled: bool) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("audio not supported on this platform"))
    }
}
