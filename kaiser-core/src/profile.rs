use std::collections::HashMap;
use std::path::PathBuf;

use monarch::{AppConfig, AppSettings, ConfigStore, Layout, ManagerError, Profile};
use serde::{Deserialize, Serialize};

use crate::audio::AudioFlow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSetting {
    /// glob-style pattern matched against device friendly name
    pub pattern: String,
    pub flow: AudioFlow,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_default: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KaiserProfile {
    pub layout: Layout,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio: Vec<AudioSetting>,
    /// Per-monitor DPI scaling percentages. Key = "adapter_luid:target_id", value = percent (100, 125, 150, …)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub dpi_scales: HashMap<String, u32>,
    /// Friendly display names captured at save time. Key = "adapter_luid:target_id", value = Windows friendly name.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub display_names: HashMap<String, String>,
    /// Per-monitor rotation in degrees (0, 90, 180, 270). Key = "adapter_luid:target_id".
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub display_rotations: HashMap<String, u32>,
    /// Clone relationships. Key = "luid:tid" (clone), value = "luid:tid" (source).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub clone_sources: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KaiserConfig {
    #[serde(default)]
    pub profiles: Vec<KaiserProfile>,
    #[serde(default)]
    pub profile_names: Vec<String>,
    #[serde(default)]
    pub settings: AppSettings,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_known_good_layout: Option<Layout>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_restorable_layout: Option<Layout>,
}

impl Default for KaiserConfig {
    fn default() -> Self {
        Self {
            profiles: Vec::new(),
            profile_names: Vec::new(),
            settings: AppSettings::default(),
            last_known_good_layout: None,
            last_restorable_layout: None,
        }
    }
}

pub struct KaiserConfigStore {
    path: PathBuf,
}

impl KaiserConfigStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn default_path() -> PathBuf {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(appdata).join("Kaiser").join("config.json")
    }

    fn read(&self) -> KaiserConfig {
        if let Ok(content) = std::fs::read_to_string(&self.path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            KaiserConfig::default()
        }
    }

    fn write(&self, config: &KaiserConfig) -> Result<(), ManagerError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ManagerError::Backend(format!("create dirs: {e}"))
            })?;
        }
        let json = serde_json::to_string_pretty(config)
            .map_err(|e| ManagerError::Backend(format!("serialize: {e}")))?;
        std::fs::write(&self.path, json)
            .map_err(|e| ManagerError::Backend(format!("write: {e}")))
    }

    pub fn load_kaiser_profile(&self, name: &str) -> Option<KaiserProfile> {
        let config = self.read();
        let idx = config.profile_names.iter().position(|n| n == name)?;
        config.profiles.get(idx).cloned()
    }

    pub fn save_kaiser_profile(&self, name: &str, profile: KaiserProfile) -> Result<(), ManagerError> {
        let mut config = self.read();
        if let Some(idx) = config.profile_names.iter().position(|n| n == name) {
            config.profiles[idx] = profile;
        } else {
            config.profile_names.push(name.to_string());
            config.profiles.push(profile);
        }
        self.write(&config)
    }

    pub fn delete_kaiser_profile(&self, name: &str) -> Result<(), ManagerError> {
        let mut config = self.read();
        if let Some(idx) = config.profile_names.iter().position(|n| n == name) {
            config.profile_names.remove(idx);
            config.profiles.remove(idx);
        }
        self.write(&config)
    }

    pub fn list_profile_names(&self) -> Vec<String> {
        self.read().profile_names
    }
}

impl ConfigStore for KaiserConfigStore {
    fn load(&self) -> Result<AppConfig, ManagerError> {
        let kaiser = self.read();
        let profiles: Vec<Profile> = kaiser
            .profile_names
            .iter()
            .zip(kaiser.profiles.iter())
            .map(|(name, kp)| Profile { name: name.clone(), layout: kp.layout.clone() })
            .collect();
        Ok(AppConfig {
            profiles,
            display_fingerprints: Vec::new(),
            last_known_good_layout: kaiser.last_known_good_layout,
            last_restorable_layout: kaiser.last_restorable_layout,
            settings: kaiser.settings,
        })
    }

    fn save(&self, config: &AppConfig) -> Result<(), ManagerError> {
        let existing = self.read();
        let mut names = Vec::new();
        let mut profiles = Vec::new();
        for profile in &config.profiles {
            let existing_kp = existing
                .profile_names
                .iter()
                .zip(existing.profiles.iter())
                .find(|(n, _)| *n == &profile.name)
                .map(|(_, kp)| kp);
            // Preserve the Kaiser-file layout (may include user edits from update_profile).
            // Only fall back to Monarch's layout when no Kaiser entry exists yet.
            let layout = existing_kp.map(|kp| kp.layout.clone())
                .unwrap_or_else(|| profile.layout.clone());
            let audio = existing_kp.map(|kp| kp.audio.clone()).unwrap_or_default();
            let dpi_scales = existing_kp.map(|kp| kp.dpi_scales.clone()).unwrap_or_default();
            let display_names = existing_kp.map(|kp| kp.display_names.clone()).unwrap_or_default();
            names.push(profile.name.clone());
            profiles.push(KaiserProfile { layout, audio, dpi_scales, display_names });
        }
        let kaiser = KaiserConfig {
            profiles,
            profile_names: names,
            settings: config.settings.clone(),
            last_known_good_layout: config.last_known_good_layout.clone(),
            last_restorable_layout: config.last_restorable_layout.clone(),
        };
        self.write(&kaiser)
    }
}
