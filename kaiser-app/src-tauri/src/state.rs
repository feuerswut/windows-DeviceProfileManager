use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use kaiser_core::{AudioManager, KaiserBackend, KaiserConfigStore, SharedKaiserBackend};
use monarch::MonarchDisplayManager;

pub struct AppState {
    pub manager: Mutex<MonarchDisplayManager<SharedKaiserBackend, KaiserConfigStore>>,
    pub backend: Arc<KaiserBackend>,
    pub audio: Mutex<AudioManager>,
    pub store_path: std::path::PathBuf,
    /// DPI state captured just before a profile apply, keyed by "luid:tid".
    /// Restored on revert so DPI rolls back together with the layout.
    pub pending_dpi_rollback: Mutex<Option<HashMap<String, u32>>>,
    /// Target IDs of displays seen on the last snapshot poll.
    /// None = first run; triggers profile sync whenever the set changes.
    pub known_display_keys: Mutex<Option<BTreeSet<u32>>>,
}

impl AppState {
    pub fn new() -> Self {
        let store_path = KaiserConfigStore::default_path();
        let backend = Arc::new(KaiserBackend::new());
        let shared = SharedKaiserBackend(Arc::clone(&backend));
        let store = KaiserConfigStore::new(store_path.clone());
        let mut manager = MonarchDisplayManager::new(shared, store)
            .expect("failed to initialize display manager");
        manager.set_confirmation_timeout(std::time::Duration::from_secs(20));
        Self {
            manager: Mutex::new(manager),
            backend,
            audio: Mutex::new(AudioManager::new()),
            store_path,
            pending_dpi_rollback: Mutex::new(None),
            known_display_keys: Mutex::new(None),
        }
    }

    pub fn new_store(&self) -> KaiserConfigStore {
        KaiserConfigStore::new(self.store_path.clone())
    }
}
