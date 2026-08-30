//! Codex / ChatGPT quota via `chatgpt.com/backend-api/wham/usage`.

use std::path::Path;

use serde_json::Value;

use crate::credentials::codex as credential;
use crate::error::QuotaError;
use crate::model::{AccountQuota, QuotaBucket, QuotaOrigin, ToolType};

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const USER_AGENT: &str = "codex-cli";

pub async fn fetch(home: &Path, client: &reqwest::Client) -> Result<AccountQuota, QuotaError> {
    let credential = credential::load(home)?;
    let account_id = credential::account_id(&credential);

    let mut request = client
        .get(USAGE_URL)
        .timeout(super::REQUEST_TIMEOUT)
        .bearer_auth(&credential.access_token)
        .header("User-Agent", USER_AGENT);
    if let Some(id) = &credential.account_id {
        request = request.header("ChatGPT-Account-Id", id);
    }

    let response = request.send().await.map_err(|e| super::classify_transport(&e))?;
    if let Some(error) = super::classify_status(response.status()) {
        return Err(error);
    }
    let body = response
        .bytes()
        .await
        .map_err(|e| super::classify_transport(&e))?;

    let buckets = parse(&body)?;
    Ok(AccountQuota {
        account_id,
        tool: ToolType::Codex,
        buckets,
        plan: plan_type(&body).or(credential.plan),
        queried_at: super::now_unix(),
        origin: QuotaOrigin::Live,
        error: None,
    })
}

/// Parse the `/wham/usage` payload.
///
/// Bucket ids come from the window length so both clients agree on identity:
/// 18000 s → `five_hour`, 604800 s → `weekly`, otherwise days or hours.
/// Nested `additional_rate_limits` entries (GPT-5.3 Codex Spark and friends)
/// are prefixed with a slug of their limit name and carry a group title, so
/// they render as their own section rather than colliding with the headline.
pub fn parse(body: &[u8]) -> Result<Vec<QuotaBucket>, QuotaError> {
    let root: Value =
        serde_json::from_slice(body).map_err(|_| QuotaError::ParseFailure("invalid json".into()))?;
    let rate_limit = root
        .get("rate_limit")
        .ok_or_else(|| QuotaError::ParseFailure("missing rate_limit".into()))?;

    let mut buckets = Vec::new();
    if let Some(window) = window_object(rate_limit, "primary_window") {
        buckets.push(make_bucket(window, "primary", None, None, None));
    }
    if let Some(window) = window_object(rate_limit, "secondary_window") {
        buckets.push(make_bucket(window, "secondary", None, None, None));
    }

    if let Some(entries) = root.get("additional_rate_limits").and_then(|v| v.as_array()) {
        for entry in entries {
            let Some(nested) = entry.get("rate_limit") else {
                continue;
            };
            let raw_name = entry
                .get("limit_name")
                .and_then(|v| v.as_str())
                .unwrap_or("Additional");
            let id_prefix = slug(raw_name);
            let group_title = display_limit_name(raw_name);
            let short_prefix = short_limit_name(raw_name);

            for (key, fallback) in [
                ("primary_window", "primary"),
                ("secondary_window", "secondary"),
            ] {
                if let Some(window) = window_object(nested, key) {
                    buckets.push(make_bucket(
                        window,
                        &format!("{id_prefix}_{fallback}"),
                        Some(&id_prefix),
                        Some(&short_prefix),
                        Some(&group_title),
                    ));
                }
            }
        }
    }

    if buckets.is_empty() {
        return Err(QuotaError::ParseFailure("no windows in rate_limit".into()));
    }
    Ok(buckets)
}

/// The subscription plan, when the payload states one.
pub fn plan_type(body: &[u8]) -> Option<String> {
    let root: Value = serde_json::from_slice(body).ok()?;
    let raw = root.get("plan_type")?.as_str()?.trim();
    (!raw.is_empty()).then(|| raw.to_string())
}

fn make_bucket(
    window: &Value,
    fallback_id: &str,
    id_prefix: Option<&str>,
    short_prefix: Option<&str>,
    group_title: Option<&str>,
) -> QuotaBucket {
    let used_percent = number(window.get("used_percent")).unwrap_or(0.0);
    let window_seconds = number(window.get("limit_window_seconds")).map(|v| v as i64);
    let reset_at = number(window.get("reset_at"));

    let (base_id, base_title, base_short) = match window_seconds {
        Some(18_000) => ("five_hour".to_string(), "5 Hours".to_string(), "5h".to_string()),
        Some(604_800) => ("weekly".to_string(), "Weekly".to_string(), "wk".to_string()),
        Some(seconds) if seconds >= 86_400 => {
            let days = seconds / 86_400;
            (
                format!("{days}d_window"),
                format!("{days} Days"),
                format!("{days}d"),
            )
        }
        Some(seconds) => {
            let hours = (seconds / 3_600).max(1);
            (
                format!("{hours}h_window"),
                format!("{hours} Hours"),
                format!("{hours}h"),
            )
        }
        None => (
            fallback_id.to_string(),
            capitalized(fallback_id),
            fallback_id.to_string(),
        ),
    };

    let id = match id_prefix {
        Some(prefix) => format!("{prefix}_{base_id}"),
        None => base_id,
    };
    let short_label = match short_prefix {
        Some(prefix) => format!("{prefix} {base_short}"),
        None => base_short,
    };
    QuotaBucket::new(
        id,
        base_title,
        short_label,
        used_percent,
        reset_at,
        window_seconds,
        group_title.map(str::to_string),
    )
}

/// A window slot that is actually an object.
///
/// The API sends `"secondary_window": null` on accounts that have no second
/// window, and a `null` is not a window: reading one produces a bucket with
/// no window length and 0% used, which renders as a phantom "100% left" lane
/// the native app never shows.
fn window_object<'a>(parent: &'a Value, key: &str) -> Option<&'a Value> {
    parent.get(key).filter(|value| value.is_object())
}

fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

/// `GPT-5.3-Codex-Spark` → `gpt_5_3_codex_spark`.
fn slug(raw: &str) -> String {
    let mapped: String = raw
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

/// `GPT-5.3-Codex-Spark` → `GPT-5.3 Codex Spark`.
fn display_limit_name(raw: &str) -> String {
    let parts: Vec<&str> = raw.split('-').collect();
    if parts.len() >= 2 && parts[0].eq_ignore_ascii_case("GPT") {
        let mut out = vec![format!("GPT-{}", parts[1])];
        out.extend(parts[2..].iter().map(|s| s.to_string()));
        return out.join(" ");
    }
    raw.replace('-', " ")
}

fn short_limit_name(raw: &str) -> String {
    if raw.to_lowercase().contains("spark") {
        "Spark".to_string()
    } else {
        raw.to_string()
    }
}

fn capitalized(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> Vec<u8> {
        serde_json::json!({
            "plan_type": "pro",
            "rate_limit": {
                "primary_window": {
                    "used_percent": 23.5,
                    "limit_window_seconds": 18000,
                    "reset_at": 1_788_038_405.0
                },
                "secondary_window": {
                    "used_percent": 61.0,
                    "limit_window_seconds": 604800,
                    "reset_at": 1_788_626_819.0
                }
            },
            "additional_rate_limits": [{
                "limit_name": "GPT-5.3-Codex-Spark",
                "rate_limit": {
                    "primary_window": {"used_percent": 4.0, "limit_window_seconds": 18000},
                    "secondary_window": {"used_percent": 9.0, "limit_window_seconds": 604800}
                }
            }]
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn parses_headline_and_additional_windows() {
        let buckets = parse(&payload()).unwrap();
        let ids: Vec<&str> = buckets.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "five_hour",
                "weekly",
                "gpt_5_3_codex_spark_five_hour",
                "gpt_5_3_codex_spark_weekly"
            ],
            "bucket ids must match the ids the native app caches"
        );
        assert_eq!(buckets[0].used_percent, 23.5);
        assert_eq!(buckets[0].reset_at, Some(1_788_038_405.0));
        assert_eq!(buckets[2].group_title.as_deref(), Some("GPT-5.3 Codex Spark"));
        assert_eq!(buckets[2].short_label, "Spark 5 Hours");
        assert_eq!(plan_type(&payload()).as_deref(), Some("pro"));
    }

    #[test]
    fn null_windows_are_not_buckets() {
        // Observed live: an account with no second window gets
        // `"secondary_window": null`, and the same for a nested limit.
        // Reading those as objects invents phantom "100% left" lanes the
        // native app does not show.
        let body = serde_json::json!({
            "rate_limit": {
                "primary_window": {"used_percent": 1.0, "limit_window_seconds": 604800},
                "secondary_window": null
            },
            "additional_rate_limits": [{
                "limit_name": "GPT-Reserve",
                "rate_limit": {
                    "primary_window": {"used_percent": 0.0, "limit_window_seconds": 604800},
                    "secondary_window": null
                }
            }]
        })
        .to_string();
        let ids: Vec<String> = parse(body.as_bytes())
            .unwrap()
            .into_iter()
            .map(|b| b.id)
            .collect();
        assert_eq!(ids, vec!["weekly", "gpt_reserve_weekly"]);
    }

    #[test]
    fn derives_ids_for_unusual_windows() {
        let body = serde_json::json!({
            "rate_limit": {
                "primary_window": {"used_percent": 1.0, "limit_window_seconds": 2_592_000},
                "secondary_window": {"used_percent": 2.0, "limit_window_seconds": 7_200}
            }
        })
        .to_string();
        let buckets = parse(body.as_bytes()).unwrap();
        assert_eq!(buckets[0].id, "30d_window");
        assert_eq!(buckets[0].title, "30 Days");
        assert_eq!(buckets[1].id, "2h_window");
    }

    #[test]
    fn rejects_payloads_without_windows() {
        assert!(matches!(
            parse(b"not json"),
            Err(QuotaError::ParseFailure(_))
        ));
        assert!(matches!(
            parse(b"{\"unexpected\": true}"),
            Err(QuotaError::ParseFailure(_))
        ));
        assert!(matches!(
            parse(b"{\"rate_limit\": {}}"),
            Err(QuotaError::ParseFailure(_))
        ));
    }

    #[test]
    fn slug_and_display_name_round_trip_the_spark_limit() {
        assert_eq!(slug("GPT-5.3-Codex-Spark"), "gpt_5_3_codex_spark");
        assert_eq!(display_limit_name("GPT-5.3-Codex-Spark"), "GPT-5.3 Codex Spark");
        assert_eq!(short_limit_name("GPT-5.3-Codex-Spark"), "Spark");
        assert_eq!(display_limit_name("Reserve-Pool"), "Reserve Pool");
    }
}
