//! GitHub Copilot quota with an explicitly supplied environment token.
//!
//! This slice reads only `COPILOT_TOKEN` and an optional
//! `COPILOT_ENTERPRISE_HOST`. It intentionally does not inspect GitHub CLI
//! state, `GITHUB_TOKEN`, native Keychain slots, or Device Flow state.

use std::collections::HashMap;
use std::path::Path;

use chrono::DateTime;
use reqwest::{Client, Url};
use serde_json::{Map, Value};

use crate::error::QuotaError;
use crate::model::{AccountQuota, QuotaBucket, QuotaOrigin, ToolType};

const ACCOUNT_ID: &str = "misc-copilot";

pub async fn fetch(_home: &Path, client: &Client) -> Result<AccountQuota, QuotaError> {
    let environment: HashMap<String, String> = std::env::vars().collect();
    let token = token(&environment).ok_or(QuotaError::NoCredential)?;
    let endpoint = usage_url(
        environment
            .get("COPILOT_ENTERPRISE_HOST")
            .map(String::as_str),
    )
    .ok_or_else(|| QuotaError::Network("Copilot enterprise host invalid".into()))?;

    let response = client
        .get(endpoint)
        .timeout(super::REQUEST_TIMEOUT)
        .header("Authorization", format!("token {token}"))
        .header("Accept", "application/json")
        .header("Editor-Version", "vscode/1.96.2")
        .header("Editor-Plugin-Version", "copilot-chat/0.26.7")
        .header("User-Agent", "GitHubCopilotChat/0.26.7")
        .header("X-Github-Api-Version", "2025-04-01")
        .send()
        .await
        .map_err(|error| super::classify_transport(&error))?;
    match response.status().as_u16() {
        200 => {}
        401 | 403 => return Err(QuotaError::NeedsLogin),
        429 => return Err(QuotaError::RateLimited),
        status => {
            return Err(QuotaError::Network(format!(
                "Copilot returned HTTP {status}"
            )))
        }
    }
    let body = response
        .bytes()
        .await
        .map_err(|error| super::classify_transport(&error))?;
    let snapshot = parse_snapshot(&body)?;
    Ok(AccountQuota {
        account_id: ACCOUNT_ID.to_string(),
        tool: ToolType::Copilot,
        buckets: snapshot.buckets,
        plan: snapshot.plan,
        queried_at: super::now_unix(),
        origin: QuotaOrigin::Live,
        error: None,
    })
}

fn token(environment: &HashMap<String, String>) -> Option<&str> {
    environment
        .get("COPILOT_TOKEN")
        .map(String::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

fn usage_url(enterprise_host: Option<&str>) -> Option<Url> {
    let host = normalized_host(enterprise_host)?;
    let api_host = if host == "github.com" {
        "api.github.com".to_string()
    } else if host.starts_with("api.") {
        host
    } else {
        format!("api.{host}")
    };
    Url::parse(&format!("https://{api_host}/copilot_internal/user")).ok()
}

fn normalized_host(raw: Option<&str>) -> Option<String> {
    let Some(raw) = raw.map(str::trim).filter(|host| !host.is_empty()) else {
        return Some("github.com".to_string());
    };
    let parseable = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("https://{raw}")
    };
    let url = Url::parse(&parseable).ok()?;
    let host = url.host_str()?.trim_matches('.').to_lowercase();
    (!host.is_empty()).then(|| match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

#[derive(Debug, PartialEq)]
struct Snapshot {
    buckets: Vec<QuotaBucket>,
    plan: Option<String>,
}

pub fn parse(body: &[u8]) -> Result<Vec<QuotaBucket>, QuotaError> {
    Ok(parse_snapshot(body)?.buckets)
}

fn parse_snapshot(body: &[u8]) -> Result<Snapshot, QuotaError> {
    let root: Value = serde_json::from_slice(body)
        .map_err(|_| QuotaError::ParseFailure("invalid Copilot response".into()))?;
    let object = root
        .as_object()
        .ok_or_else(|| QuotaError::ParseFailure("Copilot response is not an object".into()))?;
    let reset_at = object
        .get("quota_reset_date")
        .and_then(Value::as_str)
        .and_then(parse_reset_date);

    let direct = object.get("quota_snapshots").and_then(Value::as_object);
    let monthly = object.get("monthly_quotas").and_then(Value::as_object);
    let limited = object.get("limited_user_quotas").and_then(Value::as_object);
    let derived_premium = count_snapshot(monthly, limited, "completions");
    let derived_chat = count_snapshot(monthly, limited, "chat");
    let direct_premium = direct
        .and_then(|snapshots| snapshots.get("premium_interactions"))
        .and_then(snapshot_from_value);
    let direct_chat = direct
        .and_then(|snapshots| snapshots.get("chat"))
        .and_then(snapshot_from_value);
    let (fallback_premium, fallback_chat, first) =
        direct.map(dynamic_snapshots).unwrap_or((None, None, None));
    let premium = direct_premium.or(derived_premium).or(fallback_premium);
    let chat = direct_chat.or(derived_chat).or(fallback_chat).or_else(|| {
        if premium.is_none() {
            first
        } else {
            None
        }
    });

    let mut buckets = Vec::new();
    if let Some(snapshot) = premium {
        buckets.push(bucket("copilot.premium", "Premium", snapshot, reset_at));
    }
    if let Some(snapshot) = chat {
        buckets.push(bucket("copilot.chat", "Chat", snapshot, reset_at));
    }
    if buckets.is_empty() {
        return Err(QuotaError::ParseFailure(
            "Copilot response has no usable quota snapshots".into(),
        ));
    }
    Ok(Snapshot {
        buckets,
        plan: plan_name(object.get("copilot_plan").and_then(Value::as_str)),
    })
}

#[derive(Clone, Copy)]
struct QuotaSnapshot {
    percent_remaining: f64,
}

fn snapshot_from_value(value: &Value) -> Option<QuotaSnapshot> {
    let object = value.as_object()?;
    let entitlement = number(object.get("entitlement"));
    let remaining = number(object.get("remaining"));
    let percent = number(object.get("percent_remaining"))
        .or_else(|| match (entitlement, remaining) {
            (Some(entitlement), Some(remaining)) if entitlement > 0.0 => {
                Some((remaining / entitlement) * 100.0)
            }
            _ => None,
        })?
        .clamp(0.0, 100.0);
    let quota_id = object.get("quota_id").and_then(Value::as_str).unwrap_or("");
    let placeholder = entitlement.unwrap_or(0.0) == 0.0
        && remaining.unwrap_or(0.0) == 0.0
        && percent == 0.0
        && quota_id.is_empty();
    (!placeholder).then_some(QuotaSnapshot {
        percent_remaining: percent,
    })
}

fn count_snapshot(
    monthly: Option<&Map<String, Value>>,
    limited: Option<&Map<String, Value>>,
    key: &str,
) -> Option<QuotaSnapshot> {
    let entitlement = monthly
        .and_then(|values| values.get(key))
        .and_then(|value| number(Some(value)))?;
    let remaining = limited
        .and_then(|values| values.get(key))
        .and_then(|value| number(Some(value)))?;
    if entitlement <= 0.0 {
        return None;
    }
    Some(QuotaSnapshot {
        percent_remaining: (remaining.max(0.0) / entitlement) * 100.0,
    })
}

fn dynamic_snapshots(
    snapshots: &Map<String, Value>,
) -> (
    Option<QuotaSnapshot>,
    Option<QuotaSnapshot>,
    Option<QuotaSnapshot>,
) {
    let mut premium = None;
    let mut chat = None;
    let mut first = None;
    for (name, value) in snapshots {
        let Some(snapshot) = snapshot_from_value(value) else {
            continue;
        };
        if first.is_none() {
            first = Some(snapshot);
        }
        let name = name.to_lowercase();
        if chat.is_none() && name.contains("chat") {
            chat = Some(snapshot);
        } else if premium.is_none()
            && (name.contains("premium") || name.contains("completion") || name.contains("code"))
        {
            premium = Some(snapshot);
        }
    }
    (premium, chat, first)
}

fn bucket(id: &str, title: &str, snapshot: QuotaSnapshot, reset_at: Option<f64>) -> QuotaBucket {
    QuotaBucket::new(
        id,
        title,
        title,
        100.0 - snapshot.percent_remaining,
        reset_at,
        None,
        None,
    )
}

fn plan_name(raw: Option<&str>) -> Option<String> {
    match raw.unwrap_or("").trim().to_lowercase().as_str() {
        "free" => Some("Free".into()),
        "individual" | "pro" => Some("Pro".into()),
        "business" => Some("Business".into()),
        "enterprise" => Some("Enterprise".into()),
        "unknown" | "" => None,
        other => {
            let mut chars = other.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        }
    }
}

fn parse_reset_date(raw: &str) -> Option<f64> {
    if let Ok(date) = DateTime::parse_from_rfc3339(raw) {
        return Some(date.timestamp() as f64 + f64::from(date.timestamp_subsec_nanos()) / 1e9);
    }
    let date = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .ok()?
        .and_hms_opt(0, 0, 0)?
        .and_utc()
        .timestamp();
    Some(date as f64)
}

fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_premium_chat_plan_and_date_only_reset() {
        let body = serde_json::json!({
            "copilot_plan": "individual", "quota_reset_date": "2026-06-01",
            "quota_snapshots": {
                "premium_interactions": {"entitlement":300,"remaining":132,"percent_remaining":44,"quota_id":"premium"},
                "chat": {"entitlement":1000,"remaining":750,"percent_remaining":75,"quota_id":"chat"}
            }
        }).to_string();
        let snapshot = parse_snapshot(body.as_bytes()).unwrap();
        assert_eq!(snapshot.plan.as_deref(), Some("Pro"));
        assert_eq!(snapshot.buckets[0].id, "copilot.premium");
        assert_eq!(snapshot.buckets[0].used_percent, 56.0);
        assert_eq!(snapshot.buckets[1].id, "copilot.chat");
        assert_eq!(snapshot.buckets[1].used_percent, 25.0);
        assert!(snapshot.buckets[0].reset_at.is_some());
    }

    #[test]
    fn drops_placeholders_and_derives_remaining_percent() {
        let body = serde_json::json!({"copilot_plan":"business","quota_snapshots":{
            "premium_interactions":{"entitlement":0,"remaining":0,"percent_remaining":0,"quota_id":""},
            "chat":{"entitlement":500,"remaining":100,"quota_id":"chat-x"}
        }}).to_string();
        let buckets = parse(body.as_bytes()).unwrap();
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].id, "copilot.chat");
        assert_eq!(buckets[0].used_percent, 80.0);
    }

    #[test]
    fn derives_monthly_and_dynamic_snapshots() {
        let counts = serde_json::json!({
            "copilot_plan":"pro",
            "monthly_quotas":{"completions":"300","chat":1000},
            "limited_user_quotas":{"completions":"132","chat":750}
        })
        .to_string();
        let buckets = parse(counts.as_bytes()).unwrap();
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].used_percent, 56.0);
        let dynamic = serde_json::json!({"quota_snapshots":{"chat_custom":{"entitlement":"500","remaining":"125","quota_id":"chat-custom"}}}).to_string();
        let buckets = parse(dynamic.as_bytes()).unwrap();
        assert_eq!(buckets[0].id, "copilot.chat");
        assert_eq!(buckets[0].used_percent, 75.0);
    }

    #[test]
    fn rejects_invalid_shapes_and_resolves_env_only_token() {
        assert!(matches!(
            parse(b"not json"),
            Err(QuotaError::ParseFailure(_))
        ));
        assert!(matches!(parse(b"{}"), Err(QuotaError::ParseFailure(_))));
        let mut environment = HashMap::new();
        assert_eq!(token(&environment), None);
        environment.insert("COPILOT_TOKEN".into(), "  synthetic  ".into());
        assert_eq!(token(&environment), Some("synthetic"));
    }

    #[test]
    fn resolves_default_and_enterprise_hosts() {
        assert_eq!(
            usage_url(None).unwrap().as_str(),
            "https://api.github.com/copilot_internal/user"
        );
        assert_eq!(
            usage_url(Some("octocorp.ghe.com:8443")).unwrap().as_str(),
            "https://api.octocorp.ghe.com:8443/copilot_internal/user"
        );
        assert_eq!(
            usage_url(Some("api.github.example.com")).unwrap().as_str(),
            "https://api.github.example.com/copilot_internal/user"
        );
    }
}
