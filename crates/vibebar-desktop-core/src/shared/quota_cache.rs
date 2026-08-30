//! Read-only view of the shared quota cache (`<root>/quotas/`).
//!
//! Files are named `quota-v1-<sha256hex(accountId)>.json` — the account id
//! is not recoverable from the filename, so this reader does both halves:
//! it enumerates every cache file (so nothing the native app tracks is
//! invisible to Desktop), and separately hashes a list of candidate ids to
//! label the ones it recognizes.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::model::{AccountQuota, QuotaBucket, QuotaOrigin, ToolType};
use crate::paths::DataRoot;

const MAX_QUOTA_BYTES: u64 = 4 * 1024 * 1024;

/// On-disk shape written by the native `QuotaCacheStore.StoredQuota`.
/// `email`, `error` and `providerExtras` are deliberately never persisted by
/// the writer, so there is nothing sensitive to read here.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredQuota {
    tool: String,
    #[serde(default)]
    buckets: Vec<StoredBucket>,
    #[serde(default)]
    plan: Option<String>,
    /// Apple reference-date seconds.
    queried_at: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredBucket {
    id: String,
    title: String,
    short_label: String,
    used_percent: f64,
    #[serde(default)]
    reset_at: Option<f64>,
    #[serde(default)]
    raw_window_seconds: Option<i64>,
    #[serde(default)]
    group_title: Option<String>,
}

/// Privacy-preserving cache filename, mirroring the native
/// `PrivacyPreservingHash.fileComponent(prefix:rawValue:)`.
pub fn cache_file_component(account_id: &str) -> String {
    let digest = Sha256::digest(account_id.as_bytes());
    format!("quota-v1-{digest:x}")
}

/// Every quota the shared cache holds, newest observation per account.
///
/// `candidate_account_ids` lets recognized accounts carry their real id;
/// anything else is surfaced under its opaque cache key so an account this
/// build cannot name is still displayed rather than silently dropped.
pub fn load_all(root: &DataRoot, candidate_account_ids: &[String]) -> Vec<AccountQuota> {
    let mut known: HashMap<String, String> = HashMap::new();
    for id in candidate_account_ids {
        known.insert(cache_file_component(id), id.clone());
    }

    let mut out = Vec::new();
    for path in super::json_files_in(&root.quotas_dir()) {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(quota) = load_file(&path, known.get(stem).map(String::as_str).unwrap_or(stem))
        else {
            continue;
        };
        out.push(quota);
    }
    // Newest first, then by account id so the order is stable across reads.
    out.sort_by(|a, b| {
        b.queried_at
            .partial_cmp(&a.queried_at)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.account_id.cmp(&b.account_id))
    });
    out
}

fn load_file(path: &Path, account_id: &str) -> Option<AccountQuota> {
    let stored: StoredQuota = super::read_json_file(path, MAX_QUOTA_BYTES)?;
    let tool = ToolType::from_raw(&stored.tool)?;
    let buckets = stored
        .buckets
        .into_iter()
        .map(|b| {
            QuotaBucket::new(
                b.id,
                b.title,
                b.short_label,
                b.used_percent,
                b.reset_at.map(super::apple_seconds_to_unix),
                b.raw_window_seconds,
                b.group_title,
            )
        })
        .collect();
    Some(AccountQuota {
        account_id: account_id.to_string(),
        tool,
        buckets,
        plan: stored.plan,
        queried_at: super::apple_seconds_to_unix(stored.queried_at),
        origin: QuotaOrigin::SharedCache,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_cache(root: &DataRoot, account_id: &str, body: serde_json::Value) {
        let dir = root.quotas_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir
            .join(cache_file_component(account_id))
            .with_extension("json");
        std::fs::write(path, body.to_string()).unwrap();
    }

    #[test]
    fn hash_matches_native_naming_scheme() {
        let component = cache_file_component("oauth-claude");
        assert!(component.starts_with("quota-v1-"));
        assert_eq!(component.len(), "quota-v1-".len() + 64);
        // Stable across calls, and distinct per id.
        assert_eq!(component, cache_file_component("oauth-claude"));
        assert_ne!(component, cache_file_component("cli-claude"));
    }

    #[test]
    fn reads_buckets_and_converts_timestamps() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        write_cache(
            &root,
            "oauth-claude",
            serde_json::json!({
                "tool": "claude",
                "plan": "Max",
                "queriedAt": 809_731_205.0,
                "buckets": [{
                    "id": "weekly_fable",
                    "title": "Weekly",
                    "shortLabel": "Fable wk",
                    "usedPercent": 12.5,
                    "resetAt": 810_319_619.0,
                    "rawWindowSeconds": 604800,
                    "groupTitle": "Fable"
                }]
            }),
        );

        let quotas = load_all(&root, &["oauth-claude".to_string()]);
        assert_eq!(quotas.len(), 1);
        let q = &quotas[0];
        assert_eq!(q.account_id, "oauth-claude");
        assert_eq!(q.tool, ToolType::Claude);
        assert_eq!(q.plan.as_deref(), Some("Max"));
        assert_eq!(q.origin, QuotaOrigin::SharedCache);
        // Apple reference seconds became Unix seconds.
        assert!((q.queried_at - 1_788_038_405.0).abs() < 1.0);
        assert!((q.buckets[0].reset_at.unwrap() - 1_788_626_819.0).abs() < 1.0);
        assert_eq!(q.buckets[0].short_label, "Fable Weekly");
    }

    #[test]
    fn unrecognized_accounts_still_surface_under_their_cache_key() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        write_cache(
            &root,
            "misc-some-clone-uuid",
            serde_json::json!({"tool": "kimi", "queriedAt": 809_731_205.0, "buckets": []}),
        );
        let quotas = load_all(&root, &[]);
        assert_eq!(quotas.len(), 1);
        assert!(quotas[0].account_id.starts_with("quota-v1-"));
        assert_eq!(quotas[0].tool, ToolType::Kimi);
    }

    #[test]
    fn unknown_tools_and_junk_files_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        let quotas_dir = root.quotas_dir();
        std::fs::create_dir_all(&quotas_dir).unwrap();
        // A provider a newer native build knows and this one does not.
        std::fs::write(
            quotas_dir.join("quota-v1-aaaa.json"),
            serde_json::json!({"tool": "someFutureProvider", "queriedAt": 1.0, "buckets": []})
                .to_string(),
        )
        .unwrap();
        // An interrupted atomic write.
        std::fs::write(quotas_dir.join("quota-v1-bbbb.json.sb-123-abc"), "{}").unwrap();
        std::fs::write(quotas_dir.join(".DS_Store"), "junk").unwrap();

        assert!(load_all(&root, &[]).is_empty());
    }
}
