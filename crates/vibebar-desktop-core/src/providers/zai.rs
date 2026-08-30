//! Z.ai / BigModel Coding Plan quota adapter.
//!
//! Desktop currently accepts only `Z_AI_API_KEY`. Native Vibe Bar also reads
//! its macOS Keychain-backed misc-provider vault; Desktop must not write or
//! reinterpret that shared vault before the cross-platform credential contract
//! exists.

use std::collections::HashMap;
use std::path::Path;

use reqwest::Url;
use serde::Deserialize;

use crate::error::QuotaError;
use crate::model::{AccountQuota, QuotaBucket, QuotaOrigin, ToolType};

const DEFAULT_QUOTA_URL: &str = "https://api.z.ai/api/monitor/usage/quota/limit";
const ACCOUNT_ID: &str = "misc-zai";

/// Fetch the current Z.ai Coding Plan quota with the explicitly supplied
/// environment credential. No credential is persisted or logged.
pub async fn fetch(_home: &Path, client: &reqwest::Client) -> Result<AccountQuota, QuotaError> {
    let environment: HashMap<String, String> = std::env::vars().collect();
    let api_key = api_key(&environment).ok_or(QuotaError::NoCredential)?;
    let endpoint = endpoint(&environment);

    let response = client
        .get(endpoint)
        .timeout(super::REQUEST_TIMEOUT)
        .bearer_auth(api_key)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| super::classify_transport(&error))?;
    if let Some(error) = super::classify_status(response.status()) {
        return Err(error);
    }
    let body = response
        .bytes()
        .await
        .map_err(|error| super::classify_transport(&error))?;
    let snapshot = parse_snapshot(&body)?;

    Ok(AccountQuota {
        account_id: ACCOUNT_ID.to_string(),
        tool: ToolType::Zai,
        buckets: snapshot.buckets,
        plan: snapshot.plan,
        queried_at: super::now_unix(),
        origin: QuotaOrigin::Live,
        error: None,
    })
}

fn api_key(environment: &HashMap<String, String>) -> Option<&str> {
    environment
        .get("Z_AI_API_KEY")
        .map(String::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
}

/// Native-compatible endpoint precedence, limited to Desktop's environment
/// configuration until it has a secure per-provider settings store.
fn endpoint(environment: &HashMap<String, String>) -> Url {
    if let Some(url) = environment
        .get("Z_AI_QUOTA_URL")
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| super::trusted_https_url(value, &["z.ai", "bigmodel.cn"]))
    {
        return url;
    }
    if let Some(url) = environment
        .get("Z_AI_API_HOST")
        .map(String::as_str)
        .and_then(endpoint_from_host)
    {
        return url;
    }
    Url::parse(DEFAULT_QUOTA_URL).expect("the built-in Z.ai URL is valid")
}

fn endpoint_from_host(host: &str) -> Option<Url> {
    let host = host.trim();
    if host.is_empty() {
        return None;
    }
    let base = if host.contains("://") {
        host.to_string()
    } else {
        format!("https://{host}")
    };
    let mut url = super::trusted_https_url(&base, &["z.ai", "bigmodel.cn"])?;
    let existing = url.path().trim_end_matches('/');
    let path = if existing.is_empty() || existing == "/" {
        "/api/monitor/usage/quota/limit".to_string()
    } else {
        format!("{existing}/api/monitor/usage/quota/limit")
    };
    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);
    Some(url)
}

#[derive(Debug, PartialEq)]
struct Snapshot {
    buckets: Vec<QuotaBucket>,
    plan: Option<String>,
}

/// Parse the Z.ai `api/monitor/usage/quota/limit` envelope.
pub fn parse(body: &[u8]) -> Result<Vec<QuotaBucket>, QuotaError> {
    Ok(parse_snapshot(body)?.buckets)
}

fn parse_snapshot(body: &[u8]) -> Result<Snapshot, QuotaError> {
    let response: Response = serde_json::from_slice(body)
        .map_err(|_| QuotaError::ParseFailure("invalid Z.ai response".into()))?;
    if !response.success || response.code != 200 {
        return Err(QuotaError::Network(format!(
            "Z.ai API error: {}",
            response.message.trim()
        )));
    }
    let data = response
        .data
        .ok_or_else(|| QuotaError::ParseFailure("Z.ai response missing data".into()))?;

    let mut token_limits = Vec::new();
    let mut time_limit = None;
    for limit in &data.limits {
        let Some(bucket) = limit.bucket() else {
            continue;
        };
        match limit.kind.as_str() {
            "TOKENS_LIMIT" => token_limits.push(bucket),
            "TIME_LIMIT" => time_limit = Some(bucket),
            _ => {}
        }
    }

    // With two token limits, native renders the longer window as primary and
    // the shorter one as the session lane. Keep only those two for parity.
    token_limits
        .sort_by_key(|bucket| std::cmp::Reverse(bucket.raw_window_seconds.unwrap_or(i64::MAX)));
    token_limits.truncate(2);
    if let Some(bucket) = time_limit {
        token_limits.push(bucket);
    }
    Ok(Snapshot {
        buckets: token_limits,
        plan: data.plan_name(),
    })
}

#[derive(Deserialize)]
struct Response {
    code: i64,
    #[serde(default, alias = "msg")]
    message: String,
    data: Option<ResponseData>,
    success: bool,
}

#[derive(Deserialize)]
struct ResponseData {
    #[serde(default)]
    limits: Vec<RawLimit>,
    #[serde(rename = "planName")]
    plan_name: Option<String>,
    plan: Option<String>,
    #[serde(rename = "plan_type")]
    plan_type: Option<String>,
    #[serde(rename = "packageName")]
    package_name: Option<String>,
}

impl ResponseData {
    fn plan_name(&self) -> Option<String> {
        [
            self.plan_name.as_deref(),
            self.plan.as_deref(),
            self.plan_type.as_deref(),
            self.package_name.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(str::to_string)
    }
}

#[derive(Deserialize)]
struct RawLimit {
    #[serde(rename = "type")]
    kind: String,
    unit: i64,
    number: i64,
    usage: Option<i64>,
    remaining: Option<i64>,
    percentage: i64,
    #[serde(rename = "nextResetTime")]
    next_reset_time: Option<i64>,
}

impl RawLimit {
    fn window_seconds(&self) -> Option<i64> {
        match self.unit {
            1 => Some(self.number * 86_400),
            3 => Some(self.number * 3_600),
            5 => Some(self.number * 30 * 86_400),
            6 => Some(self.number * 7 * 86_400),
            _ => None,
        }
    }

    fn bucket(&self) -> Option<QuotaBucket> {
        let (title, short_label) = match self.unit {
            1 if self.number == 1 => ("Daily".to_string(), "Day".to_string()),
            1 => (format!("{} Days", self.number), format!("{}d", self.number)),
            3 if self.number == 1 => ("1 Hour".to_string(), "1h".to_string()),
            3 => (
                format!("{} Hours", self.number),
                format!("{}h", self.number),
            ),
            5 if self.kind == "TIME_LIMIT" && self.number == 1 => {
                ("MCP Monthly".to_string(), "Month".to_string())
            }
            5 if self.kind == "TIME_LIMIT" => (
                format!("MCP {} Months", self.number),
                format!("{}mo", self.number),
            ),
            5 if self.number == 1 => ("Monthly".to_string(), "Month".to_string()),
            5 => (
                format!("{} Months", self.number),
                format!("{}mo", self.number),
            ),
            6 if self.number == 1 => ("Weekly".to_string(), "Wk".to_string()),
            6 => (
                format!("{} Weeks", self.number),
                format!("{}w", self.number),
            ),
            _ if self.kind == "TIME_LIMIT" => ("Monthly".to_string(), "Month".to_string()),
            _ => ("Tokens".to_string(), "Tok".to_string()),
        };
        let used_percent = match (self.usage, self.remaining) {
            (Some(usage), Some(remaining)) if usage > 0 => {
                ((usage - remaining).max(0) as f64 / usage as f64) * 100.0
            }
            _ => self.percentage as f64,
        };
        let id = match self.kind.as_str() {
            "TIME_LIMIT" => "zai.time".to_string(),
            "TOKENS_LIMIT" => format!("zai.tokens.{}.{}", self.unit, self.number),
            other => format!("zai.{}.{}.{}", other.to_lowercase(), self.unit, self.number),
        };
        Some(QuotaBucket::new(
            id,
            title.clone(),
            short_label,
            used_percent,
            self.next_reset_time
                .map(|milliseconds| milliseconds as f64 / 1_000.0),
            self.window_seconds(),
            (self.kind == "TOKENS_LIMIT").then_some(title),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limit(kind: &str, unit: i64, number: i64, usage: i64, remaining: i64) -> serde_json::Value {
        serde_json::json!({
            "type": kind,
            "unit": unit,
            "number": number,
            "usage": usage,
            "remaining": remaining,
            "percentage": 99,
            "nextResetTime": 1_788_038_405_000_i64
        })
    }

    fn payload(limits: Vec<serde_json::Value>) -> Vec<u8> {
        serde_json::json!({
            "success": true,
            "code": 200,
            "msg": "ok",
            "data": {"limits": limits, "plan_type": "Pro"}
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn parses_one_token_limit_with_native_identity() {
        let buckets = parse(&payload(vec![limit("TOKENS_LIMIT", 6, 1, 1_000, 250)])).unwrap();
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].id, "zai.tokens.6.1");
        assert_eq!(buckets[0].title, "Weekly");
        assert_eq!(buckets[0].short_label, "Weekly");
        assert_eq!(buckets[0].used_percent, 75.0);
        assert_eq!(buckets[0].raw_window_seconds, Some(604_800));
        assert_eq!(buckets[0].reset_at, Some(1_788_038_405.0));
        assert_eq!(buckets[0].group_title.as_deref(), Some("Weekly"));
    }

    #[test]
    fn keeps_long_and_short_token_windows_then_time_limit() {
        let buckets = parse(&payload(vec![
            limit("TOKENS_LIMIT", 3, 5, 100, 20),
            limit("TOKENS_LIMIT", 6, 1, 100, 40),
            limit("TOKENS_LIMIT", 1, 1, 100, 10),
            limit("TIME_LIMIT", 5, 1, 200, 50),
        ]))
        .unwrap();
        let ids: Vec<_> = buckets.iter().map(|bucket| bucket.id.as_str()).collect();
        assert_eq!(ids, vec!["zai.tokens.6.1", "zai.tokens.1.1", "zai.time"]);
        assert_eq!(buckets[2].title, "MCP Monthly");
        assert_eq!(buckets[2].group_title, None);
    }

    #[test]
    fn plan_aliases_and_server_percent_fallback_match_native() {
        let body = serde_json::json!({
            "success": true,
            "code": 200,
            "data": {
                "limits": [{"type":"TOKENS_LIMIT","unit":3,"number":1,"percentage":23}],
                "packageName": " Team "
            }
        })
        .to_string();
        let snapshot = parse_snapshot(body.as_bytes()).unwrap();
        assert_eq!(snapshot.plan.as_deref(), Some("Team"));
        assert_eq!(snapshot.buckets[0].used_percent, 23.0);
    }

    #[test]
    fn rejects_auth_envelopes_and_invalid_shapes() {
        let denied =
            serde_json::json!({"success": false, "code": 401, "msg": "denied"}).to_string();
        assert!(matches!(
            parse(denied.as_bytes()),
            Err(QuotaError::Network(_))
        ));
        assert!(matches!(
            parse(b"not json"),
            Err(QuotaError::ParseFailure(_))
        ));
        assert!(matches!(
            parse(br#"{"success":true,"code":200,"data":{"limits":{}}}"#),
            Err(QuotaError::ParseFailure(_))
        ));
        assert!(matches!(
            parse(br#"{"success":true,"code":200}"#),
            Err(QuotaError::ParseFailure(_))
        ));
    }

    #[test]
    fn endpoint_and_key_resolution_follow_desktop_contract() {
        let mut environment = HashMap::new();
        assert_eq!(endpoint(&environment).as_str(), DEFAULT_QUOTA_URL);
        environment.insert("Z_AI_API_HOST".into(), "open.bigmodel.cn/base".into());
        assert_eq!(
            endpoint(&environment).as_str(),
            "https://open.bigmodel.cn/base/api/monitor/usage/quota/limit"
        );
        environment.insert("Z_AI_QUOTA_URL".into(), "https://api.z.ai/custom".into());
        assert_eq!(endpoint(&environment).as_str(), "https://api.z.ai/custom");
        environment.insert("Z_AI_QUOTA_URL".into(), "https://example.test/steal".into());
        assert_eq!(
            endpoint(&environment).as_str(),
            "https://open.bigmodel.cn/base/api/monitor/usage/quota/limit"
        );
        environment.insert("Z_AI_API_KEY".into(), "  synthetic-key  ".into());
        assert_eq!(api_key(&environment), Some("synthetic-key"));
    }
}
