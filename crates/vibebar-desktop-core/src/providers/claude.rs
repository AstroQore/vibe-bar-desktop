//! Claude quota via `api.anthropic.com/api/oauth/usage`.

use std::path::Path;

use serde_json::Value;

use crate::credentials::claude as credential;
use crate::error::QuotaError;
use crate::model::{AccountQuota, QuotaBucket, QuotaOrigin, ToolType};

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";

/// One legacy top-level usage key and how it renders.
struct BucketSpec {
    /// The payload key Anthropic writes.
    key: &'static str,
    /// Stable bucket id, shared with the native app's cache.
    id: &'static str,
    title: &'static str,
    short_label: &'static str,
    window_seconds: i64,
    /// `Some` means "render as its own labeled lane".
    group: Option<&'static str>,
}

const fn spec(
    key: &'static str,
    id: &'static str,
    title: &'static str,
    short_label: &'static str,
    window_seconds: i64,
    group: Option<&'static str>,
) -> BucketSpec {
    BucketSpec {
        key,
        id,
        title,
        short_label,
        window_seconds,
        group,
    }
}

/// Legacy top-level keys, in the order they render.
const KNOWN_BUCKETS: &[BucketSpec] = &[
    spec("five_hour", "five_hour", "5 Hours", "5h", 18_000, None),
    spec("seven_day", "weekly", "Weekly", "All models", 604_800, None),
    spec(
        "seven_day_sonnet",
        "weekly_sonnet",
        "Weekly",
        "Sonnet wk",
        604_800,
        Some("Sonnet"),
    ),
    spec(
        "seven_day_omelette",
        "weekly_design",
        "Weekly",
        "Designs",
        604_800,
        Some("Designs"),
    ),
    spec(
        "seven_day_opus",
        "weekly_opus",
        "Weekly",
        "Opus wk",
        604_800,
        Some("Opus"),
    ),
    spec(
        "seven_day_fable",
        "weekly_fable",
        "Weekly",
        "Fable wk",
        604_800,
        Some("Fable"),
    ),
    spec(
        "seven_day_oauth_apps",
        "weekly_oauth_apps",
        "Weekly",
        "OAuth wk",
        604_800,
        Some("OAuth Apps"),
    ),
];

/// Aliases the API has used for the same logical key.
const BUCKET_ALIASES: &[(&str, &[&str])] = &[(
    "seven_day_omelette",
    &[
        "seven_day_design",
        "seven_day_claude_design",
        "claude_design",
        "design",
        "omelette",
        "omelette_promotional",
    ],
)];

pub async fn fetch(home: &Path, client: &reqwest::Client) -> Result<AccountQuota, QuotaError> {
    let credential = credential::load(home)?;
    let account_id = credential::account_id(&credential);

    let response = client
        .get(USAGE_URL)
        .timeout(super::REQUEST_TIMEOUT)
        .bearer_auth(&credential.access_token)
        .header("anthropic-beta", OAUTH_BETA)
        .send()
        .await
        .map_err(|e| super::classify_transport(&e))?;
    if let Some(error) = super::classify_status(response.status()) {
        return Err(error);
    }
    let body = response
        .bytes()
        .await
        .map_err(|e| super::classify_transport(&e))?;

    Ok(AccountQuota {
        account_id,
        tool: ToolType::Claude,
        buckets: parse(&body)?,
        plan: credential.rate_limit_tier,
        queried_at: super::now_unix(),
        origin: QuotaOrigin::Live,
        error: None,
    })
}

/// Parse the OAuth usage payload.
///
/// Two schemas coexist. The legacy top-level `seven_day_<model>` keys are
/// read first so existing bucket ids stay stable, then the 2026-07 `limits[]`
/// array fills in everything else — its scoped entries carry the model's
/// display name, so a brand-new model surfaces with no code change
/// (`Fable` → `weekly_fable`). Legacy keys win on conflict.
pub fn parse(body: &[u8]) -> Result<Vec<QuotaBucket>, QuotaError> {
    let root: Value = serde_json::from_slice(body)
        .map_err(|_| QuotaError::ParseFailure("invalid json".into()))?;
    if !root.is_object() {
        return Err(QuotaError::ParseFailure("root is not an object".into()));
    }

    let mut out: Vec<QuotaBucket> = Vec::new();
    for spec in KNOWN_BUCKETS {
        let aliases = BUCKET_ALIASES
            .iter()
            .find(|(key, _)| *key == spec.key)
            .map(|(_, aliases)| *aliases)
            .unwrap_or(&[]);
        let entry = std::iter::once(spec.key)
            .chain(aliases.iter().copied())
            .find_map(|candidate| root.get(candidate).filter(|v| v.is_object()));
        let Some(entry) = entry else { continue };
        let Some(used) = utilization(entry) else {
            continue;
        };
        out.push(QuotaBucket::new(
            spec.id,
            spec.title,
            spec.short_label,
            used,
            reset_at(entry),
            Some(spec.window_seconds),
            spec.group.map(str::to_string),
        ));
    }

    append_limits_array(&root, &mut out);

    if out.is_empty() {
        return Err(QuotaError::ParseFailure("no recognized buckets".into()));
    }
    Ok(out)
}

/// The 2026-07 `limits[]` array. Scoped entries become their own lane;
/// headline `session` / `weekly` entries are a fallback for the legacy keys.
fn append_limits_array(root: &Value, out: &mut Vec<QuotaBucket>) {
    let Some(entries) = root.get("limits").and_then(|v| v.as_array()) else {
        return;
    };
    let mut seen: std::collections::HashSet<String> = out.iter().map(|b| b.id.clone()).collect();

    for entry in entries {
        let Some(percent) =
            number(entry.get("percent")).or_else(|| number(entry.get("utilization")))
        else {
            continue;
        };
        let kind = entry.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let group = entry.get("group").and_then(|v| v.as_str()).unwrap_or(kind);
        let is_session = group == "session";
        let window = if is_session { 18_000 } else { 604_800 };
        let title = if is_session { "5 Hours" } else { "Weekly" };

        let scope = entry.get("scope");
        let scope_name = scope
            .and_then(|s| {
                s.get("model")
                    .and_then(|m| m.get("display_name"))
                    .and_then(|v| v.as_str())
                    .or_else(|| s.get("surface").and_then(|v| v.as_str()))
            })
            .map(str::trim)
            .filter(|s| !s.is_empty());

        let bucket = if let Some(name) = scope_name {
            let prefix = if is_session { "session" } else { "weekly" };
            let id = format!("{prefix}_{}", slug(name));
            if seen.contains(&id) {
                continue;
            }
            let short = if is_session {
                format!("{name} 5h")
            } else {
                format!("{name} wk")
            };
            QuotaBucket::new(
                id,
                title,
                short,
                percent,
                reset_at(entry),
                Some(window),
                Some(name.to_string()),
            )
        } else if group == "session" || group == "weekly" {
            let id = if is_session { "five_hour" } else { "weekly" };
            if seen.contains(id) {
                continue;
            }
            let short = if is_session { "5h" } else { "All models" };
            QuotaBucket::new(
                id,
                title,
                short,
                percent,
                reset_at(entry),
                Some(window),
                None,
            )
        } else {
            continue;
        };
        seen.insert(bucket.id.clone());
        out.push(bucket);
    }
}

/// `Fable` → `fable`, `Opus 4.8` → `opus_4_8`. Keeps derived ids aligned with
/// the legacy `weekly_<model>` naming.
fn slug(name: &str) -> String {
    let mapped: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    mapped
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn utilization(entry: &Value) -> Option<f64> {
    number(entry.get("utilization")).or_else(|| number(entry.get("used_percent")))
}

/// Reset instants come as either an ISO-8601 string or an epoch number.
fn reset_at(entry: &Value) -> Option<f64> {
    let value = entry.get("resets_at").or_else(|| entry.get("reset_at"))?;
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => parse_iso8601(s),
        _ => None,
    }
}

/// Minimal RFC 3339 parse — enough for the `2026-08-30T05:00:00(.123)?Z` and
/// `+08:00`-offset forms these APIs emit.
fn parse_iso8601(text: &str) -> Option<f64> {
    use chrono::DateTime;
    DateTime::parse_from_rfc3339(text.trim())
        .ok()
        .map(|dt| dt.timestamp() as f64 + f64::from(dt.timestamp_subsec_millis()) / 1000.0)
}

fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_keys_with_their_stable_ids() {
        let body = serde_json::json!({
            "five_hour": {"utilization": 12.0, "resets_at": "2026-08-30T09:00:00Z"},
            "seven_day": {"utilization": 44.0, "resets_at": "2026-09-02T09:00:00Z"},
            "seven_day_opus": {"utilization": 7.0},
            "seven_day_cowork": null
        })
        .to_string();
        let buckets = parse(body.as_bytes()).unwrap();
        let ids: Vec<&str> = buckets.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, vec!["five_hour", "weekly", "weekly_opus"]);
        // The headline ids carry canonical window names: the label expansion
        // rewrites both `five_hour` and `weekly` regardless of what the
        // parser proposed, exactly as the native app does.
        assert_eq!(buckets[0].short_label, "5 Hours");
        assert_eq!(buckets[1].short_label, "Weekly");
        assert_eq!(buckets[2].group_title.as_deref(), Some("Opus"));
        // A null key must not synthesize a misleading 0% lane.
        assert!(!ids.contains(&"daily_routines"));
        assert!(buckets[0].reset_at.unwrap() > 1_700_000_000.0);
    }

    #[test]
    fn derives_new_model_lanes_from_the_limits_array() {
        // The 2026-07 schema: legacy per-model keys come back null and the
        // real numbers live in `limits[]`.
        let body = serde_json::json!({
            "five_hour": {"utilization": 3.0},
            "seven_day_fable": null,
            "limits": [
                {"kind": "session", "group": "session", "percent": 3.0},
                {"kind": "weekly_all", "group": "weekly", "percent": 40.0},
                {"kind": "weekly_scoped", "group": "weekly", "percent": 1.0,
                 "scope": {"model": {"display_name": "Fable"}}},
                {"kind": "weekly_scoped", "group": "weekly", "percent": 2.0,
                 "scope": {"model": {"display_name": "Opus 4.8"}}}
            ]
        })
        .to_string();
        let buckets = parse(body.as_bytes()).unwrap();
        let ids: Vec<&str> = buckets.iter().map(|b| b.id.as_str()).collect();
        // Legacy five_hour wins over the limits[] session entry; the weekly
        // headline and both scoped models come from limits[].
        assert_eq!(
            ids,
            vec!["five_hour", "weekly", "weekly_fable", "weekly_opus_4_8"]
        );
        assert_eq!(buckets[0].used_percent, 3.0);
        assert_eq!(buckets[2].group_title.as_deref(), Some("Fable"));
        assert_eq!(buckets[2].short_label, "Fable Weekly");
        assert_eq!(buckets[3].group_title.as_deref(), Some("Opus 4.8"));
    }

    #[test]
    fn an_unknown_future_model_still_renders() {
        let body = serde_json::json!({
            "limits": [{"kind": "weekly_scoped", "group": "weekly", "percent": 5.0,
                        "scope": {"model": {"display_name": "Brand New"}}}]
        })
        .to_string();
        let buckets = parse(body.as_bytes()).unwrap();
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].id, "weekly_brand_new");
        assert_eq!(buckets[0].group_title.as_deref(), Some("Brand New"));
    }

    #[test]
    fn design_aliases_map_onto_one_id() {
        for alias in ["seven_day_omelette", "seven_day_design", "claude_design"] {
            let body = serde_json::json!({ alias: {"utilization": 8.0} }).to_string();
            let buckets = parse(body.as_bytes()).unwrap();
            assert_eq!(buckets[0].id, "weekly_design", "alias {alias}");
        }
    }

    #[test]
    fn rejects_unrecognizable_payloads() {
        assert!(matches!(parse(b"nope"), Err(QuotaError::ParseFailure(_))));
        assert!(matches!(parse(b"[]"), Err(QuotaError::ParseFailure(_))));
        assert!(matches!(
            parse(b"{\"unrelated\": 1}"),
            Err(QuotaError::ParseFailure(_))
        ));
    }
}
