//! Alibaba Bailian Coding Plan quota via a DashScope API key.
//!
//! Native Vibe Bar prefers its secure per-instance credential slot and can
//! fall back to console cookies. Desktop deliberately reads only
//! `DASHSCOPE_API_KEY` (then `ALIBABA_API_KEY`) until its secure credential
//! contract exists. Alibaba Token Plan is a separate console-cookie product;
//! this key must never be used for it.

use std::collections::HashMap;
use std::path::Path;

use chrono::DateTime;
use reqwest::{Client, Url};
use serde_json::{Map, Value};

use crate::error::QuotaError;
use crate::model::{AccountQuota, QuotaBucket, QuotaOrigin, ToolType};

const ACCOUNT_ID: &str = "misc-alibaba";
const API_NAME: &str = "zeldaEasy.broadscope-bailian.codingPlan.queryCodingPlanInstanceInfoV2";

#[derive(Clone, Copy)]
enum Region {
    International,
    ChinaMainland,
}

impl Region {
    const ALL: [Self; 2] = [Self::International, Self::ChinaMainland];

    fn base_url(self) -> &'static str {
        match self {
            Self::International => "https://modelstudio.console.alibabacloud.com",
            Self::ChinaMainland => "https://bailian.console.aliyun.com",
        }
    }

    fn region_id(self) -> &'static str {
        match self {
            Self::International => "ap-southeast-1",
            Self::ChinaMainland => "cn-beijing",
        }
    }

    fn commodity_code(self) -> &'static str {
        match self {
            Self::International => "sfm_codingplan_public_intl",
            Self::ChinaMainland => "sfm_codingplan_public_cn",
        }
    }

    fn quota_url(self) -> Url {
        let mut url = Url::parse(self.base_url()).expect("built-in Alibaba URL is valid");
        url.set_path("/data/api.json");
        url.query_pairs_mut()
            .append_pair("action", API_NAME)
            .append_pair("product", "broadscope-bailian")
            .append_pair("api", "queryCodingPlanInstanceInfoV2")
            .append_pair("currentRegionId", self.region_id());
        url
    }
}

/// Fetch the Coding Plan quota. Regions are tried in native order; only an
/// authentication-style failure advances to the other region.
pub async fn fetch(_home: &Path, client: &Client) -> Result<AccountQuota, QuotaError> {
    let environment: HashMap<String, String> = std::env::vars().collect();
    let key = api_key(&environment).ok_or(QuotaError::NoCredential)?;
    let queried_at = super::now_unix();
    let mut last_error = None;

    for region in Region::ALL {
        match fetch_region(client, key, region).await {
            Ok(snapshot) => {
                return Ok(AccountQuota {
                    account_id: ACCOUNT_ID.to_string(),
                    tool: ToolType::Alibaba,
                    buckets: snapshot.buckets,
                    plan: snapshot.plan,
                    queried_at,
                    origin: QuotaOrigin::Live,
                    error: None,
                });
            }
            Err(error @ (QuotaError::NeedsLogin | QuotaError::NoCredential)) => {
                last_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| QuotaError::Unknown("Alibaba: no usable region".into())))
}

fn api_key(environment: &HashMap<String, String>) -> Option<&str> {
    ["DASHSCOPE_API_KEY", "ALIBABA_API_KEY"]
        .into_iter()
        .filter_map(|name| environment.get(name).map(String::as_str))
        .map(str::trim)
        .find(|key| !key.is_empty())
}

async fn fetch_region(
    client: &Client,
    api_key: &str,
    region: Region,
) -> Result<Snapshot, QuotaError> {
    let body = serde_json::json!({
        "queryCodingPlanInstanceInfoRequest": {"commodityCode": region.commodity_code()}
    });
    let response = client
        .post(region.quota_url())
        .timeout(super::REQUEST_TIMEOUT)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .bearer_auth(api_key)
        .header("x-api-key", api_key)
        .header("X-DashScope-API-Key", api_key)
        .header("Origin", region.base_url())
        .json(&body)
        .send()
        .await
        .map_err(|error| super::classify_transport(&error))?;
    match response.status().as_u16() {
        200 => {}
        401 | 403 => return Err(QuotaError::NeedsLogin),
        429 => return Err(QuotaError::RateLimited),
        status => {
            return Err(QuotaError::Network(format!(
                "Alibaba returned HTTP {status}"
            )))
        }
    }
    let body = response
        .bytes()
        .await
        .map_err(|error| super::classify_transport(&error))?;
    parse_snapshot(&body)
}

#[derive(Debug, PartialEq)]
struct Snapshot {
    buckets: Vec<QuotaBucket>,
    plan: Option<String>,
}

/// Parse both API-key response variants used by the Alibaba Coding Plan API.
pub fn parse(body: &[u8]) -> Result<Vec<QuotaBucket>, QuotaError> {
    Ok(parse_snapshot(body)?.buckets)
}

fn parse_snapshot(body: &[u8]) -> Result<Snapshot, QuotaError> {
    if body.is_empty() {
        return Err(QuotaError::ParseFailure(
            "Alibaba returned an empty body".into(),
        ));
    }
    let mut value: Value = serde_json::from_slice(body)
        .map_err(|_| QuotaError::ParseFailure("Alibaba response is not JSON".into()))?;
    expand_json_strings(&mut value);
    let root = value
        .as_object()
        .ok_or_else(|| QuotaError::ParseFailure("Alibaba response is not an object".into()))?;
    classify_envelope(root, &value)?;

    let quota = find_named_object(&value, &["codingPlanQuotaInfo", "coding_plan_quota_info"])
        .or_else(|| {
            find_object_with_keys(
                &value,
                &[
                    "per5HourUsedQuota",
                    "per5HourTotalQuota",
                    "perWeekUsedQuota",
                    "perWeekTotalQuota",
                    "perBillMonthUsedQuota",
                    "perBillMonthTotalQuota",
                    "perMonthUsedQuota",
                    "perMonthTotalQuota",
                ],
            )
        })
        .ok_or_else(|| QuotaError::ParseFailure("Alibaba response has no quota envelope".into()))?;

    let mut buckets = Vec::new();
    if let Some(bucket) = make_bucket(
        "alibaba.5h",
        "5 Hours",
        "5h",
        &["per5HourUsedQuota", "perFiveHourUsedQuota"],
        &["per5HourTotalQuota", "perFiveHourTotalQuota"],
        &[
            "per5HourQuotaNextRefreshTime",
            "perFiveHourQuotaNextRefreshTime",
        ],
        quota,
    ) {
        buckets.push(bucket);
    }
    if let Some(bucket) = make_bucket(
        "alibaba.weekly",
        "Weekly",
        "Wk",
        &["perWeekUsedQuota"],
        &["perWeekTotalQuota"],
        &["perWeekQuotaNextRefreshTime"],
        quota,
    ) {
        buckets.push(bucket);
    }
    if let Some(bucket) = make_bucket(
        "alibaba.monthly",
        "Monthly",
        "Mo",
        &["perBillMonthUsedQuota", "perMonthUsedQuota"],
        &["perBillMonthTotalQuota", "perMonthTotalQuota"],
        &[
            "perBillMonthQuotaNextRefreshTime",
            "perMonthQuotaNextRefreshTime",
        ],
        quota,
    ) {
        buckets.push(bucket);
    }
    if buckets.is_empty() {
        return Err(QuotaError::ParseFailure(
            "Alibaba response has no usable quota windows".into(),
        ));
    }
    Ok(Snapshot {
        buckets,
        plan: find_plan_name(&value),
    })
}

fn classify_envelope(root: &Map<String, Value>, value: &Value) -> Result<(), QuotaError> {
    if let Some(code) = find_number(value, &["statusCode", "status_code", "code"]) {
        if code != 0.0 && code != 200.0 {
            let message = find_string(value, &["statusMessage", "status_msg", "message", "msg"])
                .unwrap_or_else(|| format!("status code {code}"));
            if code == 401.0 || code == 403.0 || message.to_lowercase().contains("api key") {
                return Err(QuotaError::NeedsLogin);
            }
            return Err(QuotaError::Network(format!("Alibaba: {message}")));
        }
    }
    if let Some(code) = root
        .get("code")
        .or_else(|| root.get("status"))
        .or_else(|| root.get("statusCode"))
        .and_then(Value::as_str)
    {
        let code = code.to_lowercase();
        if code.contains("needlogin")
            || code.contains("notlogin")
            || code.contains("unauthenticated")
            || code == "login"
        {
            return Err(QuotaError::NeedsLogin);
        }
        if root.get("successResponse").and_then(Value::as_bool) == Some(false) {
            let message = find_string(value, &["message", "msg"])
                .unwrap_or_else(|| format!("Alibaba code {code}"));
            return Err(QuotaError::Network(format!("Alibaba: {message}")));
        }
    }
    Ok(())
}

fn make_bucket(
    id: &str,
    title: &str,
    short_label: &str,
    used_keys: &[&str],
    total_keys: &[&str],
    reset_keys: &[&str],
    quota: &Map<String, Value>,
) -> Option<QuotaBucket> {
    let total = first_number(quota, total_keys)?;
    if total <= 0.0 {
        return None;
    }
    let used = first_number(quota, used_keys).unwrap_or(0.0);
    Some(QuotaBucket::new(
        id,
        title,
        short_label,
        (used / total) * 100.0,
        first_date(quota, reset_keys),
        None,
        None,
    ))
}

fn expand_json_strings(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for value in object.values_mut() {
                expand_json_strings(value);
            }
        }
        Value::Array(array) => {
            for value in array {
                expand_json_strings(value);
            }
        }
        Value::String(text) => {
            if let Ok(mut nested @ (Value::Object(_) | Value::Array(_))) =
                serde_json::from_str(text)
            {
                expand_json_strings(&mut nested);
                *value = nested;
            }
        }
        _ => {}
    }
}

fn find_named_object<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Map<String, Value>> {
    match value {
        Value::Object(object) => {
            for key in keys {
                if let Some(found) = object.get(*key).and_then(Value::as_object) {
                    return Some(found);
                }
            }
            object
                .values()
                .find_map(|child| find_named_object(child, keys))
        }
        Value::Array(array) => array
            .iter()
            .find_map(|child| find_named_object(child, keys)),
        _ => None,
    }
}

fn find_object_with_keys<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Map<String, Value>> {
    match value {
        Value::Object(object) => {
            if keys.iter().any(|key| object.contains_key(*key)) {
                return Some(object);
            }
            object
                .values()
                .find_map(|child| find_object_with_keys(child, keys))
        }
        Value::Array(array) => array
            .iter()
            .find_map(|child| find_object_with_keys(child, keys)),
        _ => None,
    }
}

fn find_number(value: &Value, keys: &[&str]) -> Option<f64> {
    match value {
        Value::Object(object) => {
            for key in keys {
                if let Some(number) = object.get(*key).and_then(number) {
                    return Some(number);
                }
            }
            object.values().find_map(|child| find_number(child, keys))
        }
        Value::Array(array) => array.iter().find_map(|child| find_number(child, keys)),
        _ => None,
    }
}

fn find_string(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(object) => {
            for key in keys {
                if let Some(text) = object.get(*key).and_then(non_empty_string) {
                    return Some(text);
                }
            }
            object.values().find_map(|child| find_string(child, keys))
        }
        Value::Array(array) => array.iter().find_map(|child| find_string(child, keys)),
        _ => None,
    }
}

fn find_plan_name(value: &Value) -> Option<String> {
    if let Some(infos) = find_named_array(
        value,
        &["codingPlanInstanceInfos", "coding_plan_instance_infos"],
    ) {
        for info in infos {
            if let Some(object) = info.as_object() {
                for key in [
                    "planName",
                    "plan_name",
                    "instanceName",
                    "instance_name",
                    "packageName",
                    "package_name",
                ] {
                    if let Some(plan) = object.get(key).and_then(non_empty_string) {
                        return Some(plan);
                    }
                }
            }
        }
    }
    find_string(
        value,
        &["planName", "plan_name", "packageName", "package_name"],
    )
}

fn find_named_array<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Vec<Value>> {
    match value {
        Value::Object(object) => {
            for key in keys {
                if let Some(array) = object.get(*key).and_then(Value::as_array) {
                    return Some(array);
                }
            }
            object
                .values()
                .find_map(|child| find_named_array(child, keys))
        }
        Value::Array(array) => array.iter().find_map(|child| find_named_array(child, keys)),
        _ => None,
    }
}

fn first_number(object: &Map<String, Value>, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(number))
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn non_empty_string(value: &Value) -> Option<String> {
    let text = value.as_str()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn first_date(object: &Map<String, Value>, keys: &[&str]) -> Option<f64> {
    for key in keys {
        let Some(value) = object.get(*key) else {
            continue;
        };
        if let Some(milliseconds) = number(value).filter(|value| *value > 0.0) {
            return Some(milliseconds / 1_000.0);
        }
        if let Some(text) = value.as_str() {
            if let Ok(date) = DateTime::parse_from_rfc3339(text.trim()) {
                return Some(
                    date.timestamp() as f64 + f64::from(date.timestamp_subsec_nanos()) / 1e9,
                );
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_three_native_windows_and_plan() {
        let body = serde_json::json!({
            "code": 200,
            "data": {"codingPlanInstanceInfos": [{
                "planName": "Coding Plan Pro",
                "codingPlanQuotaInfo": {
                    "per5HourUsedQuota": 56, "per5HourTotalQuota": 100,
                    "per5HourQuotaNextRefreshTime": 1_715_432_400_000_i64,
                    "perWeekUsedQuota": 13, "perWeekTotalQuota": 100,
                    "perBillMonthUsedQuota": 5, "perBillMonthTotalQuota": 100
                }
            }]}
        })
        .to_string();
        let snapshot = parse_snapshot(body.as_bytes()).unwrap();
        let ids: Vec<_> = snapshot
            .buckets
            .iter()
            .map(|bucket| bucket.id.as_str())
            .collect();
        assert_eq!(ids, vec!["alibaba.5h", "alibaba.weekly", "alibaba.monthly"]);
        assert!((snapshot.buckets[0].used_percent - 56.0).abs() < 0.001);
        assert_eq!(snapshot.buckets[0].reset_at, Some(1_715_432_400.0));
        assert_eq!(snapshot.plan.as_deref(), Some("Coding Plan Pro"));
    }

    #[test]
    fn parses_aliases_and_stringified_envelopes() {
        let nested = r#"{"codingPlanQuotaInfo":{"perFiveHourUsedQuota":10,"perFiveHourTotalQuota":50,"perMonthUsedQuota":80,"perMonthTotalQuota":200}}"#;
        let body = serde_json::json!({"data": nested}).to_string();
        let buckets = parse(body.as_bytes()).unwrap();
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].used_percent, 20.0);
        assert_eq!(buckets[1].used_percent, 40.0);
    }

    #[test]
    fn classifies_api_and_console_auth_envelopes() {
        for body in [
            r#"{"code":401,"message":"API key invalid"}"#,
            r#"{"code":"ConsoleNeedLogin","successResponse":false}"#,
            r#"{"code":"NotLogin"}"#,
        ] {
            assert!(matches!(
                parse(body.as_bytes()),
                Err(QuotaError::NeedsLogin)
            ));
        }
        assert!(matches!(
            parse(br#"{"code":"InvalidParameter","message":"bad","successResponse":false}"#),
            Err(QuotaError::Network(_))
        ));
        assert!(matches!(parse(b""), Err(QuotaError::ParseFailure(_))));
    }

    #[test]
    fn rejects_missing_or_zero_windows_and_resolves_env_key() {
        assert!(matches!(
            parse(br#"{"data":{"codingPlanQuotaInfo":{"per5HourTotalQuota":0}}}"#),
            Err(QuotaError::ParseFailure(_))
        ));
        let mut environment = HashMap::new();
        assert_eq!(api_key(&environment), None);
        environment.insert("ALIBABA_API_KEY".into(), " fallback ".into());
        assert_eq!(api_key(&environment), Some("fallback"));
        environment.insert("DASHSCOPE_API_KEY".into(), " primary ".into());
        assert_eq!(api_key(&environment), Some("primary"));
    }

    #[test]
    fn region_requests_keep_native_endpoint_contract() {
        let international = Region::International.quota_url();
        assert_eq!(
            international.host_str(),
            Some("modelstudio.console.alibabacloud.com")
        );
        assert!(international
            .query()
            .unwrap()
            .contains("currentRegionId=ap-southeast-1"));
        let china = Region::ChinaMainland.quota_url();
        assert_eq!(china.host_str(), Some("bailian.console.aliyun.com"));
        assert!(china
            .query()
            .unwrap()
            .contains("currentRegionId=cn-beijing"));
    }
}
