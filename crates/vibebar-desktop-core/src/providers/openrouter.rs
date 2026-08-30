//! OpenRouter credits and per-key quota via the public API.

use serde::Deserialize;

use crate::error::QuotaError;
use crate::model::{AccountQuota, QuotaBucket, QuotaOrigin, ToolType};

const DEFAULT_API_URL: &str = "https://openrouter.ai/api/v1";

pub async fn fetch(client: &reqwest::Client) -> Result<AccountQuota, QuotaError> {
    let api_key = env_value("OPENROUTER_API_KEY").ok_or(QuotaError::NoCredential)?;
    let base = env_value("OPENROUTER_API_URL")
        .and_then(|value| super::trusted_https_url(&value, &["openrouter.ai"]))
        .map(|url| url.to_string())
        .unwrap_or_else(|| DEFAULT_API_URL.to_string());
    let credits = get_json(client, &endpoint(&base, "credits"), &api_key).await?;
    let credits = parse_credits(&credits)?;
    let key_stats = match get_json(client, &endpoint(&base, "key"), &api_key).await {
        Ok(body) => parse_key_stats(&body).ok(),
        Err(_) => None,
    };
    let snapshot = snapshot(credits, key_stats);

    Ok(AccountQuota {
        account_id: "misc-openRouter".to_string(),
        tool: ToolType::OpenRouter,
        buckets: snapshot.buckets,
        plan: snapshot.plan,
        queried_at: super::now_unix(),
        origin: QuotaOrigin::Live,
        error: None,
    })
}

async fn get_json(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
) -> Result<Vec<u8>, QuotaError> {
    let mut request = client
        .get(url)
        .timeout(super::REQUEST_TIMEOUT)
        .bearer_auth(api_key)
        .header(reqwest::header::ACCEPT, "application/json");
    if let Some(referer) = env_value("OPENROUTER_HTTP_REFERER") {
        request = request.header("HTTP-Referer", referer);
    }
    if let Some(title) = env_value("OPENROUTER_X_TITLE") {
        request = request.header("X-Title", title);
    }
    let response = request
        .send()
        .await
        .map_err(|error| super::classify_transport(&error))?;
    match response.status().as_u16() {
        200 => {}
        401 | 403 => return Err(QuotaError::NeedsLogin),
        429 => return Err(QuotaError::RateLimited),
        status => {
            return Err(QuotaError::Network(format!(
                "OpenRouter returned HTTP {status}"
            )))
        }
    }
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| super::classify_transport(&error))
}

fn endpoint(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), path)
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Deserialize, PartialEq)]
struct CreditsEnvelope {
    data: Option<Credits>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Credits {
    #[serde(rename = "total_credits")]
    total_credits: f64,
    #[serde(rename = "total_usage")]
    total_usage: f64,
}

#[derive(Debug, Deserialize, PartialEq)]
struct KeyStatsEnvelope {
    data: Option<KeyStats>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct KeyStats {
    label: Option<String>,
    limit: Option<f64>,
    usage: Option<f64>,
}

struct Snapshot {
    buckets: Vec<QuotaBucket>,
    plan: Option<String>,
}

fn parse_credits(body: &[u8]) -> Result<Credits, QuotaError> {
    let envelope: CreditsEnvelope = serde_json::from_slice(body).map_err(|_| {
        QuotaError::ParseFailure("OpenRouter credits response not parseable".into())
    })?;
    envelope
        .data
        .ok_or_else(|| QuotaError::ParseFailure("OpenRouter credits response missing data".into()))
}

fn parse_key_stats(body: &[u8]) -> Result<KeyStats, QuotaError> {
    let envelope: KeyStatsEnvelope = serde_json::from_slice(body)
        .map_err(|_| QuotaError::ParseFailure("OpenRouter key response not parseable".into()))?;
    envelope
        .data
        .ok_or_else(|| QuotaError::ParseFailure("OpenRouter key response missing data".into()))
}

fn snapshot(credits: Credits, key_stats: Option<KeyStats>) -> Snapshot {
    let mut buckets = Vec::new();
    if let Some(stats) = &key_stats {
        if let Some(limit) = stats.limit.filter(|limit| *limit > 0.0) {
            let usage = stats.usage.unwrap_or(0.0).max(0.0);
            buckets.push(QuotaBucket::new(
                "openrouter.key",
                "Key Limit",
                "Key",
                usage / limit * 100.0,
                None,
                None,
                Some(format!("{} / {}", money(usage), money(limit))),
            ));
        }
    }

    let used = credits.total_usage.max(0.0);
    let remaining = (credits.total_credits - credits.total_usage).max(0.0);
    if credits.total_credits > 0.0 {
        buckets.push(QuotaBucket::new(
            "openrouter.credits",
            "Credits",
            "Credits",
            used / credits.total_credits * 100.0,
            None,
            None,
            Some(format!("{} left", money(remaining))),
        ));
    }
    if buckets.is_empty() {
        buckets.push(QuotaBucket::new(
            "openrouter.credits",
            "Credits",
            "Credits",
            0.0,
            None,
            None,
            Some(format!("{} left", money(remaining))),
        ));
    }

    Snapshot {
        buckets,
        plan: key_stats.and_then(|stats| safe_label(stats.label.as_deref())),
    }
}

fn money(value: f64) -> String {
    format!("${value:.2}")
}

fn safe_label(raw: Option<&str>) -> Option<String> {
    let value = raw?.trim();
    if value.is_empty() {
        return None;
    }
    let lower = value.to_ascii_lowercase();
    let jwt_like = value.split('.').filter(|part| part.len() >= 10).count() == 3;
    let sensitive = (lower.starts_with("sk-") && value.len() >= 11)
        || lower.contains("bearer ")
        || lower.contains("authorization:")
        || lower.contains("cookie:")
        || jwt_like;
    (!sensitive).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credits_and_key_stats_match_native_bucket_ids() {
        let credits =
            parse_credits(br#"{"data":{"total_credits":20.0,"total_usage":7.5}}"#).unwrap();
        let key = parse_key_stats(br#"{"data":{"label":"Production","limit":10.0,"usage":2.5}}"#)
            .unwrap();
        let result = snapshot(credits, Some(key));
        assert_eq!(
            result
                .buckets
                .iter()
                .map(|bucket| bucket.id.as_str())
                .collect::<Vec<_>>(),
            ["openrouter.key", "openrouter.credits"]
        );
        assert_eq!(result.buckets[0].used_percent, 25.0);
        assert_eq!(result.buckets[1].used_percent, 37.5);
        assert_eq!(result.plan.as_deref(), Some("Production"));
    }

    #[test]
    fn zero_credit_account_still_has_an_honest_bucket() {
        let result = snapshot(
            Credits {
                total_credits: 0.0,
                total_usage: 0.0,
            },
            None,
        );
        assert_eq!(result.buckets.len(), 1);
        assert_eq!(result.buckets[0].id, "openrouter.credits");
        assert_eq!(result.buckets[0].used_percent, 0.0);
    }

    #[test]
    fn malformed_envelopes_and_secret_labels_fail_closed() {
        assert!(matches!(
            parse_credits(br#"{"data":null}"#),
            Err(QuotaError::ParseFailure(_))
        ));
        assert!(safe_label(Some("sk-or-v1-abcdef123456")).is_none());
        assert!(safe_label(Some("aaaabbbbbcc.cccccddddde.eeeeefffff")).is_none());
        assert_eq!(
            endpoint("https://example.test/v1/", "credits"),
            "https://example.test/v1/credits"
        );
    }
}
