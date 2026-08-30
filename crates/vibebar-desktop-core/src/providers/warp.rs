//! Warp request credits via its GraphQL usage endpoint.

use std::time::Duration;

use chrono::{DateTime, Datelike, Utc};
use serde_json::Value;

use crate::error::QuotaError;
use crate::model::{AccountQuota, QuotaBucket, QuotaOrigin, ToolType};

const API_URL: &str = "https://app.warp.dev/graphql/v2?op=GetRequestLimitInfo";
const USER_AGENT: &str = "Warp/1.0";
const CLIENT_ID: &str = "warp-app";

const GRAPHQL_QUERY: &str = r#"query GetRequestLimitInfo($requestContext: RequestContext!) {
  user(requestContext: $requestContext) {
    __typename
    ... on UserOutput {
      user {
        requestLimitInfo {
          isUnlimited
          nextRefreshTime
          requestLimit
          requestsUsedSinceLastRefresh
        }
        bonusGrants {
          requestCreditsGranted
          requestCreditsRemaining
          expiration
        }
        workspaces {
          bonusGrantsInfo {
            grants {
              requestCreditsGranted
              requestCreditsRemaining
              expiration
            }
          }
        }
      }
    }
  }
}"#;

pub async fn fetch(client: &reqwest::Client) -> Result<AccountQuota, QuotaError> {
    let api_key = env_value("WARP_API_KEY")
        .or_else(|| env_value("WARP_TOKEN"))
        .ok_or(QuotaError::NoCredential)?;
    let (os_category, os_name) = os_identity();
    let os_version = env_value("WARP_OS_VERSION").unwrap_or_else(|| "unknown".to_string());
    let body = serde_json::json!({
        "query": GRAPHQL_QUERY,
        "operationName": "GetRequestLimitInfo",
        "variables": {
            "requestContext": {
                "clientContext": {},
                "osContext": {
                    "category": os_category,
                    "name": os_name,
                    "version": os_version.clone(),
                }
            }
        }
    });
    let response = client
        .post(API_URL)
        .timeout(Duration::from_secs(15))
        .bearer_auth(api_key)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header("x-warp-client-id", CLIENT_ID)
        .header("x-warp-os-category", os_category)
        .header("x-warp-os-name", os_name)
        .header("x-warp-os-version", &os_version)
        .json(&body)
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
    let parsed = buckets(parse(&body, super::now_unix())?);
    Ok(AccountQuota {
        account_id: "misc-warp".to_string(),
        tool: ToolType::Warp,
        buckets: parsed.buckets,
        plan: parsed.plan,
        queried_at: super::now_unix(),
        origin: QuotaOrigin::Live,
        error: None,
    })
}

fn os_identity() -> (&'static str, &'static str) {
    match std::env::consts::OS {
        "macos" => ("macOS", "macOS"),
        "windows" => ("Windows", "Windows"),
        _ => ("Linux", "Linux"),
    }
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Clone, PartialEq)]
struct Snapshot {
    request_limit: i64,
    requests_used: i64,
    next_refresh_time: Option<f64>,
    is_unlimited: bool,
    bonus_remaining: i64,
    bonus_total: i64,
    bonus_next_expiration: Option<f64>,
    bonus_next_expiration_remaining: i64,
}

struct ParsedBuckets {
    buckets: Vec<QuotaBucket>,
    plan: Option<String>,
}

fn parse(body: &[u8], now: f64) -> Result<Snapshot, QuotaError> {
    let root: Value = serde_json::from_slice(body)
        .map_err(|_| QuotaError::ParseFailure("Warp root JSON is not an object".into()))?;
    let root = root
        .as_object()
        .ok_or_else(|| QuotaError::ParseFailure("Warp root JSON is not an object".into()))?;

    if let Some(errors) = root.get("errors").and_then(Value::as_array) {
        if !errors.is_empty() {
            let message = errors
                .iter()
                .filter_map(error_message)
                .take(3)
                .collect::<Vec<_>>()
                .join(" | ");
            let lower = message.to_ascii_lowercase();
            if lower.contains("authenticated") || lower.contains("unauthorized") {
                return Err(QuotaError::NeedsLogin);
            }
            return Err(QuotaError::ParseFailure(if message.is_empty() {
                "Warp GraphQL error".into()
            } else {
                message
            }));
        }
    }

    let user_output = root
        .get("data")
        .and_then(|value| value.get("user"))
        .and_then(Value::as_object)
        .ok_or_else(|| QuotaError::ParseFailure("Warp missing data.user".into()))?;
    let type_name = user_output.get("__typename").and_then(Value::as_str);
    let user = user_output
        .get("user")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            QuotaError::ParseFailure(match type_name {
                Some(name) if !name.is_empty() && name != "UserOutput" => {
                    format!("Warp unexpected user type '{name}'")
                }
                _ => "Warp missing requestLimitInfo".into(),
            })
        })?;
    let limit = user
        .get("requestLimitInfo")
        .and_then(Value::as_object)
        .ok_or_else(|| QuotaError::ParseFailure("Warp missing requestLimitInfo".into()))?;
    let is_unlimited = limit
        .get("isUnlimited")
        .and_then(Value::as_bool)
        .ok_or_else(|| QuotaError::ParseFailure("Warp missing or invalid isUnlimited".into()))?;
    let request_limit = match optional_int(limit.get("requestLimit")) {
        Some(value) if value >= 0 => value,
        None if is_unlimited => 0,
        _ => {
            return Err(QuotaError::ParseFailure(
                "Warp missing or invalid requestLimit".into(),
            ));
        }
    };
    let requests_used = match optional_int(limit.get("requestsUsedSinceLastRefresh")) {
        Some(value) if value >= 0 => value,
        None if is_unlimited => 0,
        _ => {
            return Err(QuotaError::ParseFailure(
                "Warp missing or invalid requestsUsedSinceLastRefresh".into(),
            ));
        }
    };
    let bonus = bonus_summary(user, now)?;
    Ok(Snapshot {
        request_limit,
        requests_used,
        next_refresh_time: limit
            .get("nextRefreshTime")
            .and_then(Value::as_str)
            .and_then(parse_date),
        is_unlimited,
        bonus_remaining: bonus.remaining,
        bonus_total: bonus.total,
        bonus_next_expiration: bonus.next_expiration,
        bonus_next_expiration_remaining: bonus.next_expiration_remaining,
    })
}

fn buckets(snapshot: Snapshot) -> ParsedBuckets {
    let (used_percent, group, reset_at) = if snapshot.is_unlimited {
        (0.0, "Unlimited".to_string(), None)
    } else if snapshot.request_limit > 0 {
        (
            snapshot.requests_used.max(0) as f64 / snapshot.request_limit as f64 * 100.0,
            format!(
                "{} / {} credits",
                snapshot.requests_used, snapshot.request_limit
            ),
            snapshot.next_refresh_time,
        )
    } else {
        (
            100.0,
            "No active plan".to_string(),
            snapshot.next_refresh_time,
        )
    };
    let mut result = vec![QuotaBucket::new(
        "warp.credits",
        "Credits",
        "Credits",
        used_percent,
        reset_at,
        None,
        Some(group),
    )];

    if snapshot.bonus_total > 0 || snapshot.bonus_remaining > 0 {
        let used = (snapshot.bonus_total - snapshot.bonus_remaining).max(0);
        let percent = if snapshot.bonus_total > 0 {
            used as f64 / snapshot.bonus_total as f64 * 100.0
        } else if snapshot.bonus_remaining > 0 {
            0.0
        } else {
            100.0
        };
        let group = match (
            snapshot.bonus_next_expiration,
            snapshot.bonus_next_expiration_remaining,
        ) {
            (Some(expiry), remaining) if remaining > 0 => format!(
                "{} bonus left · expires {}",
                remaining,
                format_date(expiry)
            ),
            _ => format!("{} bonus credits left", snapshot.bonus_remaining),
        };
        result.push(QuotaBucket::new(
            "warp.bonus",
            "Bonus",
            "Bonus",
            percent,
            snapshot.bonus_next_expiration,
            None,
            Some(group),
        ));
    }

    ParsedBuckets {
        buckets: result,
        plan: snapshot.is_unlimited.then(|| "Unlimited".to_string()),
    }
}

struct BonusSummary {
    remaining: i64,
    total: i64,
    next_expiration: Option<f64>,
    next_expiration_remaining: i64,
}

fn bonus_summary(
    user: &serde_json::Map<String, Value>,
    now: f64,
) -> Result<BonusSummary, QuotaError> {
    let mut grants = Vec::new();
    if let Some(values) = user.get("bonusGrants").and_then(Value::as_array) {
        for value in values {
            grants.push(grant(value)?);
        }
    }
    if let Some(workspaces) = user.get("workspaces").and_then(Value::as_array) {
        for workspace in workspaces {
            if let Some(values) = workspace
                .get("bonusGrantsInfo")
                .and_then(|value| value.get("grants"))
                .and_then(Value::as_array)
            {
                for value in values {
                    grants.push(grant(value)?);
                }
            }
        }
    }
    grants.retain(|grant| {
        grant
            .expiration
            .is_none_or(|expiration| expiration > now)
    });
    let remaining = grants.iter().map(|grant| grant.remaining).sum();
    let total = grants.iter().map(|grant| grant.granted).sum();
    let next_expiration = grants
        .iter()
        .filter(|grant| grant.remaining > 0)
        .filter_map(|grant| grant.expiration)
        .min_by(f64::total_cmp);
    let next_expiration_remaining = next_expiration.map_or(0, |earliest| {
        grants
            .iter()
            .filter(|grant| {
                grant.remaining > 0
                    && grant.expiration.map(|value| value as i64) == Some(earliest as i64)
            })
            .map(|grant| grant.remaining)
            .sum()
    });
    Ok(BonusSummary {
        remaining,
        total,
        next_expiration,
        next_expiration_remaining,
    })
}

struct BonusGrant {
    granted: i64,
    remaining: i64,
    expiration: Option<f64>,
}

fn grant(value: &Value) -> Result<BonusGrant, QuotaError> {
    let object = value
        .as_object()
        .ok_or_else(|| QuotaError::ParseFailure("Warp bonus grant is not an object".into()))?;
    let granted = optional_int(object.get("requestCreditsGranted"))
        .filter(|value| *value >= 0)
        .ok_or_else(|| QuotaError::ParseFailure("Warp bonus grant has invalid counters".into()))?;
    let remaining = optional_int(object.get("requestCreditsRemaining"))
        .filter(|value| *value >= 0 && *value <= granted)
        .ok_or_else(|| QuotaError::ParseFailure("Warp bonus grant has invalid counters".into()))?;
    Ok(BonusGrant {
        granted,
        remaining,
        expiration: object
            .get("expiration")
            .and_then(Value::as_str)
            .and_then(parse_date),
    })
}

fn optional_int(value: Option<&Value>) -> Option<i64> {
    value
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|number| i64::try_from(number).ok()))
                .or_else(|| {
                    value.as_f64().and_then(|number| {
                        (number.is_finite()
                            && number.fract() == 0.0
                            && number >= i64::MIN as f64
                            && number <= i64::MAX as f64)
                            .then_some(number as i64)
                    })
                })
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
}

fn error_message(value: &Value) -> Option<String> {
    value
        .as_str()
        .or_else(|| value.get("message").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_date(value: &str) -> Option<f64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.timestamp_millis() as f64 / 1_000.0)
}

fn format_date(timestamp: f64) -> String {
    DateTime::<Utc>::from_timestamp(timestamp as i64, 0)
        .map(|date| format!("{} {}", date.format("%b"), date.day()))
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(unlimited: bool) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "data": {"user": {
                "__typename": "UserOutput",
                "user": {
                    "requestLimitInfo": {
                        "isUnlimited": unlimited,
                        "nextRefreshTime": "2026-09-01T00:00:00Z",
                        "requestLimit": 100,
                        "requestsUsedSinceLastRefresh": 25
                    },
                    "bonusGrants": [{
                        "requestCreditsGranted": 20,
                        "requestCreditsRemaining": 5,
                        "expiration": "2026-09-02T00:00:00Z"
                    }],
                    "workspaces": [{"bonusGrantsInfo":{"grants":[{
                        "requestCreditsGranted": "10",
                        "requestCreditsRemaining": "3",
                        "expiration": "2026-09-03T00:00:00Z"
                    }]}}]
                }
            }}
        }))
        .unwrap()
    }

    #[test]
    fn parses_primary_and_bonus_buckets() {
        let parsed = buckets(parse(&payload(false), 0.0).unwrap());
        assert_eq!(
            parsed
                .buckets
                .iter()
                .map(|bucket| bucket.id.as_str())
                .collect::<Vec<_>>(),
            ["warp.credits", "warp.bonus"]
        );
        assert_eq!(parsed.buckets[0].used_percent, 25.0);
        assert!((parsed.buckets[1].used_percent - 73.333_333).abs() < 0.001);
        assert_eq!(
            parsed.buckets[1].reset_at,
            parse_date("2026-09-02T00:00:00Z")
        );
        assert_eq!(
            parsed.buckets[1].group_title.as_deref(),
            Some("5 bonus left · expires Sep 2")
        );
        assert!(parsed.plan.is_none());
    }

    #[test]
    fn unlimited_plan_has_zero_usage_and_no_reset() {
        let parsed = buckets(parse(&payload(true), 0.0).unwrap());
        assert_eq!(parsed.buckets[0].used_percent, 0.0);
        assert_eq!(parsed.buckets[0].reset_at, None);
        assert_eq!(parsed.plan.as_deref(), Some("Unlimited"));
    }

    #[test]
    fn explicit_zero_limit_is_exhausted_not_fully_available() {
        let mut body = payload(false);
        let mut root: Value = serde_json::from_slice(&body).unwrap();
        root["data"]["user"]["user"]["requestLimitInfo"]["requestLimit"] = Value::from(0);
        root["data"]["user"]["user"]["requestLimitInfo"]["requestsUsedSinceLastRefresh"] =
            Value::from(0);
        body = serde_json::to_vec(&root).unwrap();

        let parsed = buckets(parse(&body, 0.0).unwrap());

        assert_eq!(parsed.buckets[0].used_percent, 100.0);
        assert_eq!(
            parsed.buckets[0].group_title.as_deref(),
            Some("No active plan")
        );
    }

    #[test]
    fn expired_bonus_grants_do_not_count_as_available() {
        let body = serde_json::to_vec(&serde_json::json!({
            "data": {"user": {
                "__typename": "UserOutput",
                "user": {
                    "requestLimitInfo": {
                        "isUnlimited": false,
                        "nextRefreshTime": "2026-09-01T00:00:00Z",
                        "requestLimit": 100,
                        "requestsUsedSinceLastRefresh": 25
                    },
                    "bonusGrants": [
                        {
                            "requestCreditsGranted": 20,
                            "requestCreditsRemaining": 7,
                            "expiration": "2026-08-30T00:00:00Z"
                        },
                        {
                            "requestCreditsGranted": 10,
                            "requestCreditsRemaining": 3,
                            "expiration": "2026-09-02T00:00:00Z"
                        }
                    ],
                    "workspaces": [{"bonusGrantsInfo":{"grants":[{
                        "requestCreditsGranted": 50,
                        "requestCreditsRemaining": 0,
                        "expiration": "2026-08-29T00:00:00Z"
                    }]}}]
                }
            }}
        }))
        .unwrap();
        let now = parse_date("2026-08-31T00:00:00Z").unwrap();

        let snapshot = parse(&body, now).unwrap();

        assert_eq!(snapshot.bonus_total, 10);
        assert_eq!(snapshot.bonus_remaining, 3);
        assert_eq!(
            snapshot.bonus_next_expiration,
            parse_date("2026-09-02T00:00:00Z")
        );
        assert_eq!(snapshot.bonus_next_expiration_remaining, 3);
    }

    #[test]
    fn invalid_bonus_grant_counters_fail_the_snapshot_closed() {
        for grant in [
            serde_json::json!({"requestCreditsGranted":20}),
            serde_json::json!({"requestCreditsGranted":20,"requestCreditsRemaining":-5}),
            serde_json::json!({"requestCreditsGranted":20,"requestCreditsRemaining":30}),
            serde_json::json!({"requestCreditsGranted":20.5,"requestCreditsRemaining":10}),
        ] {
            let mut root: Value = serde_json::from_slice(&payload(false)).unwrap();
            root["data"]["user"]["user"]["bonusGrants"] = Value::Array(vec![grant]);
            let body = serde_json::to_vec(&root).unwrap();
            assert!(matches!(parse(&body, 0.0), Err(QuotaError::ParseFailure(_))));
        }
    }

    #[test]
    fn graphql_errors_and_missing_user_fail_closed() {
        assert_eq!(
            parse(br#"{"errors":[{"message":"Unauthenticated"}]}"#, 0.0),
            Err(QuotaError::NeedsLogin)
        );
        assert!(matches!(
            parse(br#"{"data":{}}"#, 0.0),
            Err(QuotaError::ParseFailure(_))
        ));
    }

    #[test]
    fn non_unlimited_response_requires_a_valid_request_limit() {
        let missing = br#"{"data":{"user":{"__typename":"UserOutput","user":{"requestLimitInfo":{"isUnlimited":false,"requestsUsedSinceLastRefresh":0}}}}}"#;
        assert!(matches!(
            parse(missing, 0.0),
            Err(QuotaError::ParseFailure(_))
        ));

        for request_limit in [Value::Null, Value::String("not-a-number".into())] {
            let body = serde_json::to_vec(&serde_json::json!({
                "data": {"user": {
                    "__typename": "UserOutput",
                    "user": {"requestLimitInfo": {
                        "isUnlimited": false,
                        "requestLimit": request_limit,
                        "requestsUsedSinceLastRefresh": 0
                    }}
                }}
            }))
            .unwrap();
            assert!(matches!(parse(&body, 0.0), Err(QuotaError::ParseFailure(_))));
        }
    }

    #[test]
    fn response_requires_an_explicit_boolean_unlimited_flag() {
        let missing = br#"{"data":{"user":{"__typename":"UserOutput","user":{"requestLimitInfo":{"requestLimit":100,"requestsUsedSinceLastRefresh":25}}}}}"#;
        assert!(matches!(
            parse(missing, 0.0),
            Err(QuotaError::ParseFailure(_))
        ));

        for flag in [Value::Null, Value::String("false".into()), Value::from(0)] {
            let body = serde_json::to_vec(&serde_json::json!({
                "data": {"user": {
                    "__typename": "UserOutput",
                    "user": {"requestLimitInfo": {
                        "isUnlimited": flag,
                        "requestLimit": 100,
                        "requestsUsedSinceLastRefresh": 25
                    }}
                }}
            }))
            .unwrap();
            assert!(matches!(parse(&body, 0.0), Err(QuotaError::ParseFailure(_))));
        }
    }

    #[test]
    fn non_unlimited_response_requires_a_valid_usage_count() {
        let missing = br#"{"data":{"user":{"__typename":"UserOutput","user":{"requestLimitInfo":{"isUnlimited":false,"requestLimit":100}}}}}"#;
        assert!(matches!(
            parse(missing, 0.0),
            Err(QuotaError::ParseFailure(_))
        ));

        for usage in [Value::Null, Value::String("not-a-number".into())] {
            let body = serde_json::to_vec(&serde_json::json!({
                "data": {"user": {
                    "__typename": "UserOutput",
                    "user": {"requestLimitInfo": {
                        "isUnlimited": false,
                        "requestLimit": 100,
                        "requestsUsedSinceLastRefresh": usage
                    }}
                }}
            }))
            .unwrap();
            assert!(matches!(parse(&body, 0.0), Err(QuotaError::ParseFailure(_))));
        }
    }
}
