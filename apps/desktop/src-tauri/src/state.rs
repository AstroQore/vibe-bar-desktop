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
    /// Where the main page is in its life: mounted or not, a show waiting
    /// on it, and which load generation is current — so a late report from
    /// a page the watchdog already replaced cannot pass for the new one.
    page: std::sync::Mutex<PageState>,
    /// A first launch records its completion only once the main window has
    /// actually been shown, so a crash during a slow load does not turn the
    /// next launch into a tray-only one nobody has seen the window for.
    first_run_mark_on_show: std::sync::atomic::AtomicBool,
    /// What the last update check found, kept so that installing puts on the
    /// version the person was shown rather than whatever the feed serves by
    /// the time they click.
    pending_update: Pending<tauri_plugin_updater::Update>,
}

impl AppState {
    pub fn hold_update(&self, update: tauri_plugin_updater::Update) -> u64 {
        self.pending_update.hold(update)
    }

    pub fn take_update(&self, id: u64) -> Option<tauri_plugin_updater::Update> {
        self.pending_update.take(id)
    }

    pub fn restore_update(&self, id: u64, update: tauri_plugin_updater::Update) {
        self.pending_update.restore(id, update);
    }

    pub fn drop_update(&self) {
        self.pending_update.clear();
    }

    /// The version and id of the update being held, if any — what a page
    /// opening after the scheduled check needs to offer the install.
    pub fn pending_update_summary(&self) -> Option<crate::commands::PendingUpdate> {
        self.pending_update
            .peek(|update| update.version.clone())
            .map(|(id, version)| crate::commands::PendingUpdate { version, id })
    }

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
            page: std::sync::Mutex::new(PageState::default()),
            first_run_mark_on_show: std::sync::atomic::AtomicBool::new(false),
            pending_update: Pending::default(),
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

    /// The page of load `generation` mounted. Returns whether a show was
    /// waiting — `None` when the report is from a superseded generation.
    pub fn mark_page_ready(&self, generation: u32) -> Option<bool> {
        let mut page = self.page.lock().ok()?;
        if generation != page.generation {
            return None;
        }
        page.ready = true;
        Some(std::mem::take(&mut page.show_pending))
    }

    /// Park a show until the page is ready; `true` when it is ready already,
    /// so the caller shows now. One lock covers the check and the park, so
    /// `mark_page_ready` and this cannot both miss the show.
    pub fn park_show_unless_ready(&self) -> bool {
        let Ok(mut page) = self.page.lock() else {
            return true;
        };
        if page.ready {
            return true;
        }
        page.show_pending = true;
        false
    }

    /// Start a new load generation if the page has not mounted: the number
    /// the reloaded page must report back. `None` when the page mounted
    /// meanwhile — then there is nothing to reload.
    pub fn begin_reload_unless_ready(&self) -> Option<u32> {
        let mut page = self.page.lock().ok()?;
        if page.ready {
            return None;
        }
        page.generation += 1;
        Some(page.generation)
    }

    /// Stop waiting: treat the page as ready whatever it did, and say whether
    /// a show was parked — the watchdog's last resort.
    pub fn give_up_waiting(&self) -> bool {
        let Ok(mut page) = self.page.lock() else {
            return false;
        };
        if page.ready {
            return false;
        }
        page.ready = true;
        std::mem::take(&mut page.show_pending)
    }

    pub fn defer_first_run_mark(&self) {
        self.first_run_mark_on_show
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn take_first_run_mark(&self) -> bool {
        self.first_run_mark_on_show
            .swap(false, std::sync::atomic::Ordering::SeqCst)
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

/// One thing waiting to be acted on, and a name for which one it is.
///
/// A bare slot is not enough: two checks can be in flight at once — leaving
/// About and coming back unmounts the first control but not its request — and
/// whichever finishes last wins the slot. The version on the button would then
/// belong to one check and the bytes to the other. Every hold gets an id, and
/// acting on it means naming that id.
struct Pending<T> {
    slot: std::sync::Mutex<Option<(u64, T)>>,
    next: std::sync::atomic::AtomicU64,
}

impl<T> Default for Pending<T> {
    fn default() -> Self {
        Self {
            slot: std::sync::Mutex::new(None),
            next: std::sync::atomic::AtomicU64::new(1),
        }
    }
}

impl<T> Pending<T> {
    fn hold(&self, value: T) -> u64 {
        let id = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut slot) = self.slot.lock() {
            *slot = Some((id, value));
        }
        id
    }

    /// The value with that id, removed. `None` once a later hold has replaced
    /// it — that request's result is not this one's to install.
    fn take(&self, id: u64) -> Option<T> {
        let mut slot = self.slot.lock().ok()?;
        match slot.as_ref() {
            Some((held, _)) if *held == id => slot.take().map(|(_, value)| value),
            _ => None,
        }
    }

    /// Put it back after failing to use it, unless something newer arrived
    /// meanwhile — the newer one is what the person is looking at.
    fn restore(&self, id: u64, value: T) {
        if let Ok(mut slot) = self.slot.lock() {
            if slot.is_none() {
                *slot = Some((id, value));
            }
        }
    }

    fn clear(&self) {
        if let Ok(mut slot) = self.slot.lock() {
            *slot = None;
        }
    }

    /// Look at what is held without taking it: the id, and whatever the
    /// caller reads off the value.
    fn peek<U>(&self, read: impl FnOnce(&T) -> U) -> Option<(u64, U)> {
        let slot = self.slot.lock().ok()?;
        slot.as_ref().map(|(id, value)| (*id, read(value)))
    }
}

#[cfg(test)]
mod pending_tests {
    use super::Pending;

    #[test]
    fn taking_needs_the_id_that_was_handed_out() {
        let pending: Pending<&str> = Pending::default();
        let id = pending.hold("0.2.0");
        assert_eq!(pending.take(id + 1), None);
        assert_eq!(pending.take(id), Some("0.2.0"));
        assert_eq!(pending.take(id), None);
    }

    #[test]
    fn a_superseded_check_cannot_install_its_result() {
        // Dev check starts, Stable check starts, Stable lands last. The button
        // showing the Stable version installs the Stable bytes; the Dev
        // request that finished first cannot reach in and swap them.
        let pending: Pending<&str> = Pending::default();
        let dev = pending.hold("0.3.0-dev.4");
        let stable = pending.hold("0.2.0");
        assert_eq!(pending.take(dev), None);
        assert_eq!(pending.take(stable), Some("0.2.0"));
    }

    #[test]
    fn a_failed_install_can_be_retried() {
        let pending: Pending<&str> = Pending::default();
        let id = pending.hold("0.2.0");
        let value = pending.take(id).expect("held");
        pending.restore(id, value);
        assert_eq!(pending.take(id), Some("0.2.0"));
    }

    #[test]
    fn restoring_does_not_clobber_a_newer_check() {
        let pending: Pending<&str> = Pending::default();
        let old = pending.hold("0.2.0");
        let value = pending.take(old).expect("held");
        let new = pending.hold("0.4.0");
        pending.restore(old, value);
        assert_eq!(pending.take(new), Some("0.4.0"));
    }
}

/// See `AppState::page`.
#[derive(Default)]
struct PageState {
    ready: bool,
    show_pending: bool,
    generation: u32,
}
