//! Quota refresh orchestration.
//!
//! Merges two sources into the one list the UI renders:
//!
//! 1. What this client fetched through a live adapter, including its last
//!    successful private snapshot — explicitly labeled as Desktop cache.
//! 2. What the shared cache holds — every other provider the native app
//!    tracks, labeled as cache so the UI never overstates freshness.
//!
//! Per account, the newer observation wins regardless of which side produced
//! it: on a Mac running both clients, whichever refreshed last is the truth.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use crate::client_store::ClientStore;
use crate::model::{AccountQuota, QuotaBucket, QuotaOrigin, ToolType};
use crate::paths::DataRoot;
use crate::shared::settings::SharedSettings;

pub struct QuotaEngine {
    store: ClientStore,
    home: PathBuf,
    client: reqwest::Client,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaView {
    pub accounts: Vec<AccountQuota>,
    /// Unix seconds of the newest observation in `accounts`, if any.
    pub last_updated: Option<f64>,
    /// True when a shared data root written by another client is present.
    pub has_shared_data: bool,
    pub is_demo: bool,
}

impl QuotaEngine {
    pub fn new(root: DataRoot) -> Self {
        let home = crate::paths::home_directory();
        let client = reqwest::Client::builder()
            .user_agent(concat!("VibeBarDesktop/", env!("CARGO_PKG_VERSION")))
            .timeout(crate::providers::REQUEST_TIMEOUT)
            // A quota endpoint never legitimately redirects a bearer token
            // to another host.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();
        Self {
            store: ClientStore::new(root),
            home,
            client,
        }
    }

    pub fn data_root(&self) -> &DataRoot {
        self.store.data_root()
    }

    /// Refresh cadence from the shared settings, so both clients poll at the
    /// rate the user configured once.
    pub fn refresh_interval(&self) -> Duration {
        SharedSettings::load(self.data_root()).refresh_interval()
    }

    /// The current view without going to the network: this client's own
    /// persisted observations merged with the shared cache.
    pub fn cached_view(&self) -> QuotaView {
        let own = self.store.load_quotas();
        let (shared, has_shared_data) = self.load_shared(&own);
        let accounts = merge(own, shared);
        Self::view(accounts, has_shared_data, self.data_root().is_demo())
    }

    /// Read the shared cache, naming as many accounts as we can.
    ///
    /// Cache files are keyed by `sha256(accountId)`, so an id is only
    /// recoverable by guessing it and hashing. Beyond the stable ids the
    /// native `AccountStore` mints, the strongest guesses available are the
    /// ids *this* client has already seen — a provider that reports a real
    /// account UUID (Codex does) writes its shared entry under that UUID, and
    /// without this the same account appears twice: once under its UUID from
    /// our own store and once under an opaque cache key.
    fn load_shared(&self, own: &[AccountQuota]) -> (Vec<AccountQuota>, bool) {
        let root = self.data_root();
        let settings = SharedSettings::load(root);
        let mut candidates = settings.candidate_account_ids();
        candidates.extend(own.iter().map(|quota| quota.account_id.clone()));
        candidates.sort();
        candidates.dedup();

        let shared = crate::shared::quota_cache::load_all(root, &candidates);
        let has_shared_data = !shared.is_empty() || root.settings_file().is_file();
        (shared, has_shared_data)
    }

    /// Fetch every provider with a live adapter, persist the results, and
    /// return the merged view.
    ///
    /// Demo mode short-circuits before any network or credential access —
    /// the same contract the native app's demo mode holds.
    pub async fn refresh(&self) -> QuotaView {
        if self.data_root().is_demo() {
            return self.cached_view();
        }

        let mut fetched: Vec<AccountQuota> = Vec::new();
        for tool in ToolType::ALL.iter().copied().filter(|t| t.has_live_adapter()) {
            match crate::providers::fetch(tool, &self.home, &self.client).await {
                Ok(quota) => {
                    // A failed persist must not lose the observation we
                    // already have in hand.
                    let _ = self.store.save_quota(&quota);
                    fetched.push(quota);
                }
                Err(error) => fetched.push(AccountQuota {
                    account_id: format!("{}-unavailable", tool.raw_value()),
                    tool,
                    buckets: Vec::new(),
                    plan: None,
                    queried_at: crate::providers::now_unix(),
                    origin: QuotaOrigin::Live,
                    error: Some(error),
                }),
            }
        }

        // A cached success hides transient failures, but never hides an
        // authentication problem that requires the user to act.
        let (ok, failed): (Vec<_>, Vec<_>) =
            fetched.into_iter().partition(|q| q.error.is_none());

        // Everything this client knows about, live or previously persisted,
        // so the shared cache's hashed filenames can be matched back to real
        // account ids rather than surfacing the same account twice.
        let mut current = self.store.load_quotas();
        current.extend(ok);
        let desktop_snapshot_tools: HashSet<_> = current
            .iter()
            .filter(|quota| quota.origin == QuotaOrigin::DesktopCache)
            .map(|quota| quota.tool)
            .collect();
        let (shared, has_shared_data) = self.load_shared(&current);

        let mut accounts = merge(current, shared);
        for failure in failed {
            let covered = accounts.iter().any(|a| a.tool == failure.tool);
            if should_keep_failure(
                covered,
                desktop_snapshot_tools.contains(&failure.tool),
                &failure,
            ) {
                accounts.push(failure);
            }
        }
        Self::view(accounts, has_shared_data, self.data_root().is_demo())
    }

    fn view(accounts: Vec<AccountQuota>, has_shared_data: bool, is_demo: bool) -> QuotaView {
        Self::view_at(accounts, has_shared_data, is_demo, crate::providers::now_unix())
    }

    fn view_at(
        accounts: Vec<AccountQuota>,
        has_shared_data: bool,
        is_demo: bool,
        now: f64,
    ) -> QuotaView {
        let mut accounts = consolidate(accounts, now);
        accounts.sort_by(|a, b| {
            provider_rank(a.tool)
                .cmp(&provider_rank(b.tool))
                .then_with(|| a.tool.raw_value().cmp(b.tool.raw_value()))
                .then_with(|| a.account_id.cmp(&b.account_id))
        });
        let last_updated = accounts
            .iter()
            .filter(|a| !a.buckets.is_empty())
            .map(|a| a.queried_at)
            .fold(None::<f64>, |acc, value| {
                Some(acc.map_or(value, |current: f64| current.max(value)))
            });
        QuotaView {
            accounts,
            last_updated,
            has_shared_data,
            is_demo,
        }
    }
}

/// Collapse a provider's accounts into the one card a user should see.
///
/// A shared data root accumulates an entry per credential route the native
/// app ever tried — a real one held five Claude entries for one subscription:
/// two with identical buckets, three empty. Rendering them verbatim is a wall
/// of duplicate and blank cards that says nothing.
///
/// So: one card per provider, and each quota window takes its value from the
/// newest believable observation of *that window*, wherever it came from.
/// That is also what makes the card correct rather than merely tidy — a
/// route that still reports `five_hour` and another that still reports
/// `weekly` together describe the subscription that neither does alone.
///
/// A provider with nothing believable left keeps one error card if it has
/// one, and is otherwise dropped: an account with no windows and no failure
/// is not information.
fn consolidate(accounts: Vec<AccountQuota>, now: f64) -> Vec<AccountQuota> {
    let mut by_tool: HashMap<ToolType, Vec<AccountQuota>> = HashMap::new();
    for account in accounts {
        by_tool.entry(account.tool).or_default().push(account);
    }

    let mut out = Vec::new();
    for (tool, group) in by_tool {
        let (usable, failures): (Vec<_>, Vec<_>) = group
            .into_iter()
            .partition(|a| a.error.is_none() && a.has_plausible_timestamp(now));

        // bucket id -> (observed at, bucket), newest wins.
        let mut newest: HashMap<String, (f64, QuotaBucket)> = HashMap::new();
        let mut source: Option<&AccountQuota> = None;
        for account in &usable {
            for bucket in &account.buckets {
                let entry = newest.get(&bucket.id);
                if entry.is_none_or(|(at, _)| account.queried_at > *at) {
                    newest.insert(bucket.id.clone(), (account.queried_at, bucket.clone()));
                }
            }
            if !account.buckets.is_empty()
                && source.is_none_or(|current| account.queried_at > current.queried_at)
            {
                source = Some(account);
            }
        }

        if newest.is_empty() {
            // Nothing to show: keep a failure so the user learns why.
            if let Some(failure) = failures.into_iter().next() {
                out.push(failure);
            }
            continue;
        }

        let Some(source) = source else { continue };
        let mut buckets: Vec<(f64, QuotaBucket)> = newest.into_values().collect();
        // Preserve the source account's bucket order, then append anything
        // only other routes reported.
        let order: Vec<&str> = source.buckets.iter().map(|b| b.id.as_str()).collect();
        buckets.sort_by_key(|(_, bucket)| {
            order
                .iter()
                .position(|id| *id == bucket.id)
                .unwrap_or(usize::MAX)
        });

        let queried_at = buckets
            .iter()
            .map(|(at, _)| *at)
            .fold(f64::MIN, f64::max);
        let origin = if usable
            .iter()
            .any(|a| a.origin == QuotaOrigin::Live && a.queried_at >= queried_at)
        {
            QuotaOrigin::Live
        } else if usable
            .iter()
            .any(|a| a.origin == QuotaOrigin::DesktopCache && a.queried_at >= queried_at)
        {
            QuotaOrigin::DesktopCache
        } else {
            QuotaOrigin::SharedCache
        };
        let auth_error = failures
            .iter()
            .filter_map(|failure| failure.error.as_ref())
            .find(|error| is_auth_error(error))
            .cloned();

        out.push(AccountQuota {
            account_id: source.account_id.clone(),
            tool,
            buckets: buckets.into_iter().map(|(_, bucket)| bucket).collect(),
            plan: usable
                .iter()
                .filter(|a| a.plan.is_some())
                .max_by(|a, b| a.queried_at.total_cmp(&b.queried_at))
                .and_then(|a| a.plan.clone()),
            queried_at,
            origin,
            error: auth_error,
        });
    }
    out
}

/// Per account id, keep the newest observation. Live results are preferred
/// only as a tiebreak — a fresher cache entry is still fresher.
fn merge(live: Vec<AccountQuota>, cached: Vec<AccountQuota>) -> Vec<AccountQuota> {
    let mut best: HashMap<String, AccountQuota> = HashMap::new();
    for quota in cached.into_iter().chain(live) {
        match best.get(&quota.account_id) {
            Some(existing) if existing.queried_at > quota.queried_at => {}
            Some(existing)
                if (existing.queried_at - quota.queried_at).abs() < f64::EPSILON
                    && origin_rank(existing.origin) >= origin_rank(quota.origin) =>
            {
                // Same instant, prefer the stronger provenance already held.
            }
            _ => {
                best.insert(quota.account_id.clone(), quota);
            }
        }
    }
    best.into_values().collect()
}

fn origin_rank(origin: QuotaOrigin) -> u8 {
    match origin {
        QuotaOrigin::Live => 2,
        QuotaOrigin::DesktopCache => 1,
        QuotaOrigin::SharedCache => 0,
    }
}

fn is_auth_failure(quota: &AccountQuota) -> bool {
    quota.error.as_ref().is_some_and(is_auth_error)
}

fn should_keep_failure(
    covered: bool,
    had_desktop_snapshot: bool,
    failure: &AccountQuota,
) -> bool {
    !covered || (had_desktop_snapshot && is_auth_failure(failure))
}

fn is_auth_error(error: &crate::error::QuotaError) -> bool {
    matches!(
        error,
        crate::error::QuotaError::NoCredential | crate::error::QuotaError::NeedsLogin
    )
}

/// Core providers first, in the native app's display order, then the rest.
fn provider_rank(tool: ToolType) -> usize {
    match tool {
        ToolType::Codex => 0,
        ToolType::Claude => 1,
        ToolType::Gemini => 2,
        ToolType::Antigravity => 3,
        ToolType::Grok => 4,
        ToolType::Cursor => 5,
        _ => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::QuotaBucket;

    fn quota(id: &str, tool: ToolType, at: f64, origin: QuotaOrigin) -> AccountQuota {
        AccountQuota {
            account_id: id.into(),
            tool,
            buckets: vec![QuotaBucket::new("weekly", "Weekly", "wk", 10.0, None, None, None)],
            plan: None,
            queried_at: at,
            origin,
            error: None,
        }
    }

    #[test]
    fn newest_observation_wins_regardless_of_origin() {
        let live = vec![quota("a", ToolType::Codex, 100.0, QuotaOrigin::Live)];
        let cached = vec![
            quota("a", ToolType::Codex, 200.0, QuotaOrigin::SharedCache),
            quota("b", ToolType::Kimi, 50.0, QuotaOrigin::SharedCache),
        ];
        let merged = merge(live, cached);
        assert_eq!(merged.len(), 2);
        let a = merged.iter().find(|q| q.account_id == "a").unwrap();
        assert_eq!(a.queried_at, 200.0, "a fresher cache entry must win");

        // …and the reverse.
        let live = vec![quota("a", ToolType::Codex, 300.0, QuotaOrigin::Live)];
        let cached = vec![quota("a", ToolType::Codex, 200.0, QuotaOrigin::SharedCache)];
        let merged = merge(live, cached);
        assert_eq!(merged[0].queried_at, 300.0);
        assert_eq!(merged[0].origin, QuotaOrigin::Live);
    }

    fn quota_with(
        id: &str,
        tool: ToolType,
        at: f64,
        buckets: &[(&str, f64)],
    ) -> AccountQuota {
        AccountQuota {
            account_id: id.into(),
            tool,
            buckets: buckets
                .iter()
                .map(|(bucket_id, used)| {
                    QuotaBucket::new(*bucket_id, "Weekly", "wk", *used, None, None, None)
                })
                .collect(),
            plan: None,
            queried_at: at,
            origin: QuotaOrigin::SharedCache,
            error: None,
        }
    }

    const NOW: f64 = 1_788_040_000.0;

    #[test]
    fn one_card_per_provider_built_from_the_newest_window_observations() {
        // The shape a real data root had: one Claude subscription with five
        // cached entries — two identical, three empty — plus one route that
        // alone still reports `weekly`.
        let accounts = vec![
            quota_with("a", ToolType::Claude, NOW - 900.0, &[("five_hour", 10.0)]),
            quota_with("b", ToolType::Claude, NOW - 900.0, &[("five_hour", 10.0)]),
            quota_with("c", ToolType::Claude, NOW - 900.0, &[]),
            quota_with("d", ToolType::Claude, NOW - 900.0, &[]),
            quota_with(
                "web-claude",
                ToolType::Claude,
                NOW - 300.0,
                &[("five_hour", 8.0), ("weekly", 34.0)],
            ),
        ];
        let view = QuotaEngine::view_at(accounts, true, false, NOW);

        assert_eq!(view.accounts.len(), 1, "one card per provider");
        let card = &view.accounts[0];
        // Every window the subscription has, each from its newest reading.
        let mut got: Vec<(&str, f64)> = card
            .buckets
            .iter()
            .map(|b| (b.id.as_str(), b.used_percent))
            .collect();
        got.sort_by(|a, b| a.0.cmp(b.0));
        assert_eq!(got, vec![("five_hour", 8.0), ("weekly", 34.0)]);
        assert_eq!(card.queried_at, NOW - 300.0);
    }

    #[test]
    fn a_window_only_an_older_route_reports_is_still_shown() {
        // The newest route lost a window the older one still has; dropping it
        // would silently delete a real limit from the card.
        let accounts = vec![
            quota_with("old", ToolType::Codex, NOW - 3600.0, &[("weekly", 50.0), ("five_hour", 20.0)]),
            quota_with("new", ToolType::Codex, NOW - 60.0, &[("weekly", 55.0)]),
        ];
        let card = &QuotaEngine::view_at(accounts, true, false, NOW).accounts[0];
        let mut got: Vec<(&str, f64)> = card
            .buckets
            .iter()
            .map(|b| (b.id.as_str(), b.used_percent))
            .collect();
        got.sort_by(|a, b| a.0.cmp(b.0));
        assert_eq!(got, vec![("five_hour", 20.0), ("weekly", 55.0)]);
    }

    #[test]
    fn future_timestamps_and_empty_accounts_never_reach_the_ui() {
        let accounts = vec![
            // Stamped months ahead — cannot be an observation.
            quota_with("bogus", ToolType::Claude, NOW + 86_400.0 * 150.0, &[("weekly", 1.0)]),
            quota_with("real", ToolType::Claude, NOW - 60.0, &[("weekly", 34.0)]),
            // No windows and no failure: not information.
            quota_with("empty", ToolType::Grok, NOW - 60.0, &[]),
        ];
        let view = QuotaEngine::view_at(accounts, true, false, NOW);
        assert_eq!(view.accounts.len(), 1);
        assert_eq!(view.accounts[0].tool, ToolType::Claude);
        assert_eq!(view.accounts[0].buckets[0].used_percent, 34.0);
    }

    #[test]
    fn a_provider_with_only_a_failure_keeps_its_error_card() {
        let failure = AccountQuota {
            error: Some(crate::error::QuotaError::NoCredential),
            ..quota_with("codex-unavailable", ToolType::Codex, NOW, &[])
        };
        let view = QuotaEngine::view_at(vec![failure], false, false, NOW);
        assert_eq!(view.accounts.len(), 1);
        assert!(view.accounts[0].error.is_some());
        assert_eq!(view.last_updated, None);
    }

    #[test]
    fn auth_failure_keeps_a_desktop_snapshot_visible_and_actionable() {
        let cached = quota(
            "oauth-codex",
            ToolType::Codex,
            NOW - 60.0,
            QuotaOrigin::DesktopCache,
        );
        let failure = AccountQuota {
            error: Some(crate::error::QuotaError::NeedsLogin),
            ..quota_with("codex-unavailable", ToolType::Codex, NOW, &[])
        };
        assert!(should_keep_failure(true, true, &failure));
        assert!(!should_keep_failure(true, false, &failure));

        let view = QuotaEngine::view_at(vec![cached, failure], false, false, NOW);

        assert_eq!(view.accounts.len(), 1);
        assert_eq!(view.accounts[0].origin, QuotaOrigin::DesktopCache);
        assert_eq!(view.accounts[0].buckets.len(), 1);
        assert_eq!(
            view.accounts[0].error,
            Some(crate::error::QuotaError::NeedsLogin)
        );
        assert_eq!(view.last_updated, Some(NOW - 60.0));
    }

    #[test]
    fn view_sorts_core_providers_first_and_reports_latest() {
        let accounts = vec![
            quota("k", ToolType::Kimi, 10.0, QuotaOrigin::SharedCache),
            quota("c", ToolType::Claude, 30.0, QuotaOrigin::Live),
            quota("x", ToolType::Codex, 20.0, QuotaOrigin::Live),
        ];
        let view = QuotaEngine::view(accounts, true, false);
        let order: Vec<&str> = view.accounts.iter().map(|a| a.tool.raw_value()).collect();
        assert_eq!(order, vec!["codex", "claude", "kimi"]);
        assert_eq!(view.last_updated, Some(30.0));
    }

    #[test]
    fn errors_do_not_count_as_a_fresh_observation() {
        let accounts = vec![AccountQuota {
            account_id: "codex-unavailable".into(),
            tool: ToolType::Codex,
            buckets: Vec::new(),
            plan: None,
            queried_at: 999.0,
            origin: QuotaOrigin::Live,
            error: Some(crate::error::QuotaError::NoCredential),
        }];
        let view = QuotaEngine::view(accounts, false, false);
        assert_eq!(view.last_updated, None);
    }

    #[tokio::test]
    async fn demo_mode_never_touches_the_network() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        assert!(root.is_demo());
        let engine = QuotaEngine::new(root);
        let view = engine.refresh().await;
        assert!(view.is_demo);
        assert!(view.accounts.is_empty());
    }

    #[test]
    fn an_account_present_in_both_stores_renders_once() {
        // Codex reports a real account UUID, so both clients write their
        // entry under it. The shared cache is keyed by sha256(accountId), so
        // without claiming the id from our own store the same account shows
        // up twice — once under its UUID, once under an opaque cache key.
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        let account_id = "11111111-2222-3333-4444-555555555555";

        let shared_dir = root.quotas_dir();
        std::fs::create_dir_all(&shared_dir).unwrap();
        std::fs::write(
            shared_dir
                .join(crate::shared::quota_cache::cache_file_component(account_id))
                .with_extension("json"),
            serde_json::json!({
                "tool": "codex", "queriedAt": 809_757_943.0,
                "buckets": [{"id": "weekly", "title": "Weekly", "shortLabel": "wk",
                             "usedPercent": 1.0}]
            })
            .to_string(),
        )
        .unwrap();

        let engine = QuotaEngine::new(root.clone());
        // Nothing of our own yet: the account is unnameable, shown as-is.
        assert_eq!(engine.cached_view().accounts.len(), 1);

        // Once we have fetched it ourselves, both entries are one account.
        ClientStore::new(root)
            .save_quota(&quota(account_id, ToolType::Codex, 1_788_038_500.0, QuotaOrigin::Live))
            .unwrap();
        let view = engine.cached_view();
        assert_eq!(view.accounts.len(), 1, "got {:?}", view.accounts);
        assert_eq!(view.accounts[0].account_id, account_id);
    }

    #[test]
    fn cached_view_reads_the_shared_cache_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        let quotas_dir = root.quotas_dir();
        std::fs::create_dir_all(&quotas_dir).unwrap();
        let file = quotas_dir
            .join(crate::shared::quota_cache::cache_file_component("misc-kimi"))
            .with_extension("json");
        std::fs::write(
            &file,
            serde_json::json!({
                "tool": "kimi", "queriedAt": 809_731_205.0,
                "buckets": [{"id": "kimi.weekly", "title": "Weekly", "shortLabel": "Weekly",
                             "usedPercent": 4.06}]
            })
            .to_string(),
        )
        .unwrap();
        let before = std::fs::read(&file).unwrap();

        let engine = QuotaEngine::new(root.clone());
        let view = engine.cached_view();
        assert_eq!(view.accounts.len(), 1);
        assert_eq!(view.accounts[0].account_id, "misc-kimi");
        assert_eq!(view.accounts[0].origin, QuotaOrigin::SharedCache);
        assert!(view.has_shared_data);
        assert_eq!(std::fs::read(&file).unwrap(), before);
    }
}
