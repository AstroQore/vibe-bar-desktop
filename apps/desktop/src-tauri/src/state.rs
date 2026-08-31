use std::path::PathBuf;

use vibebar_desktop_core::cost::CostEngine;
use vibebar_desktop_core::paths::DataRoot;
use vibebar_desktop_core::refresh::QuotaEngine;
use vibebar_desktop_core::sessions::SessionsService;
use vibebar_desktop_core::shared::settings_writer::SettingsWriter;
use vibebar_desktop_core::status::ServiceStatusEngine;

pub struct AppState {
    engine: QuotaEngine,
    /// The one shared store Desktop writes. Behind a lock because it carries
    /// what this process has changed since it started, which is what tells a
    /// setting the user chose here from one the native app has always owned.
    settings: std::sync::Mutex<SettingsWriter>,
    /// Wakes the refresh loop when the cadence it is sleeping on is no longer
    /// the cadence the settings ask for.
    cadence_changed: std::sync::Arc<tokio::sync::Notify>,
    sessions: SessionsService,
    status: ServiceStatusEngine,
    cost: CostEngine,
    data_root: DataRoot,
}

impl AppState {
    pub fn new() -> Self {
        let data_root = DataRoot::discover();
        let scan_home: PathBuf = if data_root.is_demo() {
            data_root
                .shared()
                .parent()
                .map(PathBuf::from)
                .unwrap_or_else(|| data_root.shared().to_path_buf())
        } else {
            vibebar_desktop_core::paths::home_directory()
        };
        Self {
            engine: QuotaEngine::new(data_root.clone()),
            settings: std::sync::Mutex::new(SettingsWriter::new(data_root.settings_file())),
            cadence_changed: std::sync::Arc::new(tokio::sync::Notify::new()),
            sessions: SessionsService::with_home(data_root.clone(), scan_home.clone()),
            status: ServiceStatusEngine::new(data_root.clone()),
            cost: CostEngine::new(data_root.clone(), scan_home),
            data_root,
        }
    }

    pub fn engine(&self) -> &QuotaEngine {
        &self.engine
    }

    pub fn sessions(&self) -> &SessionsService {
        &self.sessions
    }

    pub fn status(&self) -> &ServiceStatusEngine {
        &self.status
    }

    pub fn cost(&self) -> &CostEngine {
        &self.cost
    }

    pub fn settings(&self) -> &std::sync::Mutex<SettingsWriter> {
        &self.settings
    }

    pub fn cadence_changed(&self) -> std::sync::Arc<tokio::sync::Notify> {
        self.cadence_changed.clone()
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
