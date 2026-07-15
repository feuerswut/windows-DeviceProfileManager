#![cfg(target_os = "windows")]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use monarch::{DisplayBackend, DisplayInfo, Layout, ManagerError};
use serde::{Deserialize, Serialize};

use super::apply::{
    active_color_state_signature, capture_sdr_gamma_ramps, force_topology_extend,
    gamma_ramp_looks_identity, reapply_color_calibration_for_active_with_cached_sdr,
    GammaRampKey, GammaRampWords,
};
use super::enumerate::query_active_topology;
use super::win32_types::{luid_to_u64, RawTopologySnapshot, TopologySnapshot};

struct BackendCache {
    last_snapshot: Option<TopologySnapshot>,
    last_color_state_signature: Option<String>,
    sdr_gamma_cache: HashMap<GammaRampKey, GammaRampWords>,
}

impl BackendCache {
    fn new() -> Self {
        Self {
            last_snapshot: None,
            last_color_state_signature: None,
            sdr_gamma_cache: HashMap::new(),
        }
    }
}

pub struct KaiserBackend {
    cache: Mutex<BackendCache>,
    snapshot_path: PathBuf,
}

impl std::fmt::Debug for KaiserBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KaiserBackend").finish()
    }
}

impl KaiserBackend {
    pub fn new() -> Self {
        let snapshot_path = kaiser_data_dir().join("topology_snapshot.json");
        Self { cache: Mutex::new(BackendCache::new()), snapshot_path }
    }

    fn ensure_snapshot(&self) -> Result<(), ManagerError> {
        let mut cache = self.cache.lock().unwrap();
        if cache.last_snapshot.is_some() {
            return Ok(());
        }
        log::info!("KaiserBackend: querying initial display topology");
        let snapshot = query_active_topology()?;
        log::info!(
            "KaiserBackend: found {} active display(s)",
            snapshot.displays.len()
        );
        let color_sig = active_color_state_signature(&snapshot);
        let sdr_gamma = capture_sdr_gamma_ramps(&snapshot);
        cache.last_snapshot = Some(snapshot.clone());
        cache.last_color_state_signature = Some(color_sig);
        cache.sdr_gamma_cache = sdr_gamma;
        // Persist immediately so disabled-display tracking works from the first toggle.
        drop(cache);
        self.persist_snapshot(&snapshot);
        Ok(())
    }

    fn refresh_snapshot(&self, next_snapshot: TopologySnapshot) {
        let mut cache = self.cache.lock().unwrap();
        let new_sig = active_color_state_signature(&next_snapshot);
        let old_sig = cache.last_color_state_signature.as_deref().unwrap_or("");

        if new_sig != old_sig {
            let fresh_sdr = capture_sdr_gamma_ramps(&next_snapshot);
            let merged_sdr: HashMap<GammaRampKey, GammaRampWords> = cache
                .sdr_gamma_cache
                .iter()
                .filter(|(k, ramp)| !gamma_ramp_looks_identity(ramp) && !fresh_sdr.contains_key(k))
                .map(|(k, v)| (*k, *v))
                .chain(fresh_sdr.iter().filter(|(_, v)| !gamma_ramp_looks_identity(v)).map(|(k, v)| (*k, *v)))
                .collect();
            cache.sdr_gamma_cache = merged_sdr;
            cache.last_color_state_signature = Some(new_sig);
        }
        let merged = merge_snapshot_for_cache(cache.last_snapshot.as_ref(), next_snapshot);
        cache.last_snapshot = Some(merged);
    }
}

impl Default for KaiserBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl KaiserBackend {
    pub fn get_gdi_name(&self, adapter_luid: u64, target_id: u32) -> Option<String> {
        let cache = self.cache.lock().unwrap();
        cache
            .last_snapshot
            .as_ref()
            .and_then(|s| s.gdi_names.get(&(adapter_luid, target_id)).cloned())
    }

    pub fn invalidate_snapshot(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.last_snapshot = None;
    }

    pub fn get_rotation_values(&self) -> HashMap<(u64, u32), u32> {
        let cache = self.cache.lock().unwrap();
        cache.last_snapshot.as_ref()
            .map(|s| s.rotation_values.clone())
            .unwrap_or_default()
    }

    pub fn get_clone_pairs(&self) -> HashMap<(u64, u32), (u64, u32)> {
        let cache = self.cache.lock().unwrap();
        cache.last_snapshot.as_ref()
            .map(|s| s.clone_pairs.clone())
            .unwrap_or_default()
    }
}

impl DisplayBackend for KaiserBackend {
    fn color_state_signature(&self) -> Result<Option<String>, ManagerError> {
        self.ensure_snapshot()?;
        let cache = self.cache.lock().unwrap();
        Ok(cache.last_color_state_signature.clone())
    }

    fn reapply_color_calibration(&self) -> Result<(), ManagerError> {
        let sdr_cache = {
            let cache = self.cache.lock().unwrap();
            cache.sdr_gamma_cache.clone()
        };
        reapply_color_calibration_for_active_with_cached_sdr(&sdr_cache)
    }

    fn list_displays(&self) -> Result<Vec<DisplayInfo>, ManagerError> {
        self.ensure_snapshot()?;
        let cache = self.cache.lock().unwrap();
        let snapshot = cache.last_snapshot.as_ref().unwrap();
        let mut all_displays = snapshot.displays.clone();

        // Try to add known-inactive displays from the topology snapshot (previously active)
        // so the UI can show them as "off" and let the user re-enable them
        let active_ids: std::collections::HashSet<_> =
            all_displays.iter().map(|d| d.id.clone()).collect();
        let active_edid_hashes: std::collections::HashSet<u64> =
            all_displays.iter().filter_map(|d| d.id.edid_hash).collect();

        if let Ok(content) = std::fs::read_to_string(&self.snapshot_path) {
            if let Ok(persisted) = serde_json::from_str::<PersistedSnapshot>(&content) {
                for display in persisted.displays {
                    // Match by stable edid_hash first to avoid ghost entries after a
                    // reboot where the adapter LUID changes for the same physical monitor.
                    let matched_by_hash = display
                        .id
                        .edid_hash
                        .map(|h| active_edid_hashes.contains(&h))
                        .unwrap_or(false);
                    if !active_ids.contains(&display.id) && !matched_by_hash {
                        let mut inactive = display;
                        inactive.is_active = false;
                        all_displays.push(inactive);
                    }
                }
            }
        }
        Ok(all_displays)
    }

    fn get_layout(&self) -> Result<Layout, ManagerError> {
        self.ensure_snapshot()?;
        let mut layout = {
            let cache = self.cache.lock().unwrap();
            cache.last_snapshot.as_ref().unwrap().layout.clone()
        };
        // Merge in disabled outputs from the persisted snapshot so that monitors
        // disabled in a previous apply are still present (as enabled=false) and
        // can be toggled back on.
        let active_keys: std::collections::HashSet<(u64, u32)> = layout
            .outputs
            .iter()
            .map(|o| (o.display_id.adapter_luid, o.display_id.target_id))
            .collect();
        let active_edid_hashes: std::collections::HashSet<u64> =
            layout.outputs.iter().filter_map(|o| o.display_id.edid_hash).collect();
        if let Ok(content) = std::fs::read_to_string(&self.snapshot_path) {
            if let Ok(persisted) = serde_json::from_str::<PersistedSnapshot>(&content) {
                for mut output in persisted.layout.outputs {
                    let key = (output.display_id.adapter_luid, output.display_id.target_id);
                    let matched_by_hash = output
                        .display_id
                        .edid_hash
                        .map(|h| active_edid_hashes.contains(&h))
                        .unwrap_or(false);
                    if !active_keys.contains(&key) && !matched_by_hash {
                        output.enabled = false;
                        layout.outputs.push(output);
                    }
                }
            }
        }
        Ok(layout)
    }

    fn apply_layout(&self, layout: Layout) -> Result<(), ManagerError> {
        self.ensure_snapshot()?;

        let any_enabled = layout.outputs.iter().any(|o| o.enabled);
        if !any_enabled {
            return Err(ManagerError::Backend(
                "refusing to apply layout with all displays disabled".to_string(),
            ));
        }

        log::info!(
            "KaiserBackend: applying layout ({} outputs, {} enabled)",
            layout.outputs.len(),
            layout.outputs.iter().filter(|o| o.enabled).count(),
        );

        let sdr_cache = {
            let cache = self.cache.lock().unwrap();
            cache.sdr_gamma_cache.clone()
        };

        // Retry loop: keep attempting until the active topology matches the desired
        // layout or we exhaust all attempts. Windows topology changes are async and
        // may require multiple passes to fully settle.
        const MAX_ATTEMPTS: u32 = 6;
        let mut last_err: Option<ManagerError> = None;

        for attempt in 0..MAX_ATTEMPTS {
            if attempt > 0 {
                log::info!("KaiserBackend: retry attempt {}/{}", attempt, MAX_ATTEMPTS - 1);
                std::thread::sleep(std::time::Duration::from_millis(1500));
            }

            // Always use the freshest cached snapshot for each attempt.
            let snapshot = {
                let cache = self.cache.lock().unwrap();
                cache.last_snapshot.as_ref().unwrap().clone()
            };

            let result = self.try_apply_once(&layout, &snapshot);

            match result {
                Ok(next_snapshot) => {
                    if super::apply::verify_layout_applied(&layout, &next_snapshot) {
                        log::info!("KaiserBackend: layout verified on attempt {}", attempt + 1);
                        self.persist_snapshot(&next_snapshot);
                        let _ = reapply_color_calibration_for_active_with_cached_sdr(&sdr_cache);
                        self.refresh_snapshot(next_snapshot);
                        return Ok(());
                    }
                    log::warn!(
                        "KaiserBackend: apply attempt {} returned Ok but verification failed — will retry",
                        attempt + 1
                    );
                    self.refresh_snapshot(next_snapshot);
                    last_err = Some(ManagerError::Backend(
                        "topology verification failed after apply".to_string(),
                    ));
                }
                Err(e) => {
                    log::error!("KaiserBackend: apply attempt {} failed: {e}", attempt + 1);
                    // Refresh snapshot so the next attempt sees current state.
                    if let Ok(fresh) = query_active_topology() {
                        self.refresh_snapshot(fresh);
                    }
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            ManagerError::Backend("apply failed after all attempts".to_string())
        }))
    }

}

impl KaiserBackend {
    /// Single attempt: two-phase apply → inactive-attach fallback → extend+retry.
    fn try_apply_once(
        &self,
        layout: &Layout,
        snapshot: &TopologySnapshot,
    ) -> Result<TopologySnapshot, ManagerError> {
        match super::apply::apply_layout_against_snapshot(layout, snapshot) {
            Ok(s) => Ok(s),
            Err(e) => {
                log::error!(
                    "KaiserBackend: main apply failed ({e}); trying inactive-path attach fallback"
                );
                let active = query_active_topology()?;
                match super::apply::try_attach_inactive_for_layout(layout, &active) {
                    Ok(s) => {
                        log::info!("KaiserBackend: inactive-path attach succeeded");
                        Ok(s)
                    }
                    Err(attach_err) => {
                        log::error!(
                            "KaiserBackend: attach fallback failed ({attach_err}); \
                             trying topology extend as last resort"
                        );
                        force_topology_extend()?;
                        std::thread::sleep(std::time::Duration::from_millis(900));
                        let recovered = query_active_topology()?;
                        super::apply::apply_layout_against_snapshot(layout, &recovered)
                    }
                }
            }
        }
    }
}

/// Newtype wrapping `Arc<KaiserBackend>` so we can implement the foreign
/// `DisplayBackend` trait without violating the orphan rule.
pub struct SharedKaiserBackend(pub Arc<KaiserBackend>);

impl DisplayBackend for SharedKaiserBackend {
    fn list_displays(&self) -> Result<Vec<DisplayInfo>, ManagerError> {
        self.0.list_displays()
    }
    fn get_layout(&self) -> Result<Layout, ManagerError> {
        self.0.get_layout()
    }
    fn apply_layout(&self, layout: Layout) -> Result<(), ManagerError> {
        self.0.apply_layout(layout)
    }
    fn color_state_signature(&self) -> Result<Option<String>, ManagerError> {
        self.0.color_state_signature()
    }
    fn reapply_color_calibration(&self) -> Result<(), ManagerError> {
        self.0.reapply_color_calibration()
    }
}

#[derive(Serialize, Deserialize)]
struct PersistedSnapshot {
    displays: Vec<DisplayInfo>,
    layout: Layout,
}

impl KaiserBackend {
    fn persist_snapshot(&self, snapshot: &TopologySnapshot) {
        // Merge with old persisted data so that previously-seen-but-now-inactive
        // displays and outputs are retained (as is_active=false / enabled=false).
        let mut displays = snapshot.displays.clone();
        let mut outputs = snapshot.layout.outputs.clone();

        let active_ids: std::collections::HashSet<_> =
            snapshot.displays.iter().map(|d| d.id.clone()).collect();
        let active_edid_hashes: std::collections::HashSet<u64> =
            snapshot.displays.iter().filter_map(|d| d.id.edid_hash).collect();
        let active_keys: std::collections::HashSet<(u64, u32)> = snapshot
            .layout
            .outputs
            .iter()
            .map(|o| (o.display_id.adapter_luid, o.display_id.target_id))
            .collect();
        let active_output_edid_hashes: std::collections::HashSet<u64> =
            snapshot.layout.outputs.iter().filter_map(|o| o.display_id.edid_hash).collect();

        if let Ok(content) = std::fs::read_to_string(&self.snapshot_path) {
            if let Ok(old) = serde_json::from_str::<PersistedSnapshot>(&content) {
                for mut d in old.displays {
                    let matched_by_hash = d
                        .id
                        .edid_hash
                        .map(|h| active_edid_hashes.contains(&h))
                        .unwrap_or(false);
                    if !active_ids.contains(&d.id) && !matched_by_hash {
                        d.is_active = false;
                        displays.push(d);
                    }
                }
                for mut o in old.layout.outputs {
                    let key = (o.display_id.adapter_luid, o.display_id.target_id);
                    let matched_by_hash = o
                        .display_id
                        .edid_hash
                        .map(|h| active_output_edid_hashes.contains(&h))
                        .unwrap_or(false);
                    if !active_keys.contains(&key) && !matched_by_hash {
                        o.enabled = false;
                        outputs.push(o);
                    }
                }
            }
        }

        let data = PersistedSnapshot {
            displays,
            layout: Layout { outputs },
        };
        if let Ok(json) = serde_json::to_string_pretty(&data) {
            let _ = std::fs::create_dir_all(self.snapshot_path.parent().unwrap());
            let _ = std::fs::write(&self.snapshot_path, json);
        }
    }

}

fn merge_snapshot_for_cache(previous: Option<&TopologySnapshot>, fresh: TopologySnapshot) -> TopologySnapshot {
    if let Some(prev) = previous {
        if prev.raw.paths.len() > fresh.raw.paths.len()
            && raw_covers_active_outputs(&prev.raw, &fresh.layout)
        {
            let mut merged = fresh;
            merged.raw = prev.raw.clone();
            return merged;
        }
    }
    fresh
}

fn raw_covers_active_outputs(raw: &RawTopologySnapshot, layout: &Layout) -> bool {
    layout.outputs.iter().filter(|o| o.enabled).all(|output| {
        raw.paths.iter().any(|path| {
            let adapter_luid = luid_to_u64(
                path.targetInfo.adapterId.HighPart,
                path.targetInfo.adapterId.LowPart,
            );
            adapter_luid == output.display_id.adapter_luid
                && path.targetInfo.id == output.display_id.target_id
        })
    })
}

pub fn kaiser_data_dir() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(appdata).join("Kaiser")
}
