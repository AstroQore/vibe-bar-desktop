use vibebar_desktop_core::paths::DataRoot;
use vibebar_desktop_core::refresh::QuotaEngine;
use vibebar_desktop_core::sessions::SessionsService;

pub struct AppState {
    engine: QuotaEngine,
    sessions: SessionsService,
    data_root: DataRoot,
}

impl AppState {
    pub fn new() -> Self {
        let data_root = DataRoot::discover();
        Self {
            engine: QuotaEngine::new(data_root.clone()),
            sessions: SessionsService::new(data_root.clone()),
            data_root,
        }
    }

    pub fn engine(&self) -> &QuotaEngine {
        &self.engine
    }

    pub fn sessions(&self) -> &SessionsService {
        &self.sessions
    }

    pub fn data_root(&self) -> &DataRoot {
        &self.data_root
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
