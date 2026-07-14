use std::sync::{Arc, Mutex};

use kaiser_core::{AudioManager, KaiserBackend, KaiserConfigStore, SharedKaiserBackend};
use monarch::MonarchDisplayManager;

pub struct AppState {
    pub manager: Mutex<MonarchDisplayManager<SharedKaiserBackend, KaiserConfigStore>>,
    pub backend: Arc<KaiserBackend>,
    pub audio: Mutex<AudioManager>,
    pub store_path: std::path::PathBuf,
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
        }
    }

    pub fn new_store(&self) -> KaiserConfigStore {
        KaiserConfigStore::new(self.store_path.clone())
    }
}
