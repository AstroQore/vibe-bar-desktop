//! MiniMax Token Plan quota adapter.

use std::collections::HashMap;

use serde_json::Value;

use crate::error::QuotaError;
use crate::model::{AccountQuota, QuotaBucket, QuotaOrigin, ToolType};

const ACCOUNT_ID: &str = "misc-minimax";

pub fn resolve_api_key(env: &HashMap<String, String>) -> Option<String> {
    ["MINIMAX_CODING_API_KEY", "MINIMAX_API_KEY"]
        .iter()
        .filter_map(|key| env.get(*key))
        .map(String::as_str)
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(str::to_owned)
}

pub fn remains_urls(env: &HashMap<String, String>, mainland: bool) -> Vec<String> {
    let mut urls = Vec::new();
    for key in ["MINIMAX_REMAINS_URL", "MINIMAX_CODING_PLAN_URL"] {
        if let Some(url) = env
            .get(key)
            .and_then(|value| super::trusted_https_url(value, &["minimax.io", "minimaxi.com"]))
        {
            urls.push(url.to_string());
        }
    }
    if let Some(host) = env
        .get("MINIMAX_HOST")
        .filter(|value| !value.trim().is_empty())
    {
        let raw = if host.contains("://") {
            host.trim().to_string()
        } else {
            format!("https://{}", host.trim())
        };
        if let Some(url) = super::trusted_https_url(&raw, &["minimax.io", "minimaxi.com"]) {
            urls.push(format!(
                "{}/v1/api/openplatform/coding_plan/remains",
                url.as_str().trim_end_matches('/')
            ));
        }
    }
    let (www, api) = if mainland {
        ("www.minimaxi.com", "api.minimaxi.com")
    } else {
        ("www.minimax.io", "api.minimax.io")
    };
    urls.extend([
        format!("https://{www}/v1/token_plan/remains"),
        format!("https://{api}/v1/api/openplatform/coding_plan/remains"),
        format!("https://{www}/v1/api/openplatform/coding_plan/remains"),
    ]);
    let mut seen = std::collections::HashSet::new();
    urls.into_iter()
        .filter(|url| seen.insert(url.clone()))
        .collect()
}

pub fn request_headers(api_key: &str) -> [(&'static str, String); 5] {
    [("Authorization", format!("Bearer {api_key}")),
     ("Accept", "application/json, text/plain, */*".into()),
     ("Content-Type", "application/json".into()),
     ("MM-API-Source", "VibeBar".into()),
     ("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36".into())]
}

pub async fn fetch(client: &reqwest::Client) -> Result<AccountQuota, QuotaError> {
    let env = super::read_env(&[
        "MINIMAX_CODING_API_KEY",
        "MINIMAX_API_KEY",
        "MINIMAX_REMAINS_URL",
        "MINIMAX_CODING_PLAN_URL",
        "MINIMAX_HOST",
        "MINIMAX_REGION",
    ]);
    let key = resolve_api_key(&env).ok_or(QuotaError::NoCredential)?;
    let mainland_first = env.get("MINIMAX_REGION").is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "cn" | "china" | "china-mainland" | "minimaxi"
        )
    });
    let regions = if mainland_first {
        [true, false]
    } else {
        [false, true]
    };
    let mut last = None;
    let mut saw_auth_error = false;
    for mainland in regions {
        for url in remains_urls(&env, mainland) {
            let mut request = client.get(&url).timeout(super::REQUEST_TIMEOUT);
            for (name, value) in request_headers(&key) {
                request = request.header(name, value);
            }
            let response = match request.send().await {
                Ok(response) => response,
                Err(error) => {
                    last = Some(super::classify_transport(&error));
                    continue;
                }
            };
            if let Some(error) = super::classify_status(response.status()) {
                match error {
                    QuotaError::RateLimited => return Err(QuotaError::RateLimited),
                    QuotaError::NeedsLogin => {
                        saw_auth_error = true;
                        last = Some(QuotaError::NeedsLogin);
                    }
                    other => last = Some(other),
                }
                continue;
            }
            let body = match response.bytes().await {
                Ok(body) => body,
                Err(error) => {
                    last = Some(super::classify_transport(&error));
                    continue;
                }
            };
            match parse(&body, super::now_unix()) {
                Ok((buckets, plan)) => {
                    return Ok(AccountQuota {
                        account_id: ACCOUNT_ID.to_string(),
                        tool: ToolType::Minimax,
                        buckets,
                        plan,
                        queried_at: super::now_unix(),
                        origin: QuotaOrigin::Live,
                        error: None,
                    });
                }
                Err(QuotaError::RateLimited) => return Err(QuotaError::RateLimited),
                Err(QuotaError::NeedsLogin) => {
                    saw_auth_error = true;
                    last = Some(QuotaError::NeedsLogin);
                }
                Err(error) => last = Some(error),
            }
        }
    }
    if saw_auth_error {
        Err(QuotaError::NeedsLogin)
    } else {
        Err(last.unwrap_or(QuotaError::Network("MiniMax endpoints exhausted".into())))
    }
}

pub fn parse(body: &[u8], now: f64) -> Result<(Vec<QuotaBucket>, Option<String>), QuotaError> {
    if body.is_empty() {
        return Err(QuotaError::ParseFailure(
            "MiniMax returned an empty body".into(),
        ));
    }
    let root: Value =
        serde_json::from_slice(body).map_err(|e| QuotaError::ParseFailure(e.to_string()))?;
    let base = root
        .get("base_resp")
        .or_else(|| root.get("data").and_then(|v| v.get("base_resp")));
    if let Some(code) = base
        .and_then(|value| int(value.get("status_code")))
        .filter(|code| *code != 0)
    {
        let msg = base
            .and_then(|v| v.get("status_msg"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let lower = msg.to_ascii_lowercase();
        if code == 1004
            || lower.contains("login")
            || lower.contains("log in")
            || lower.contains("cookie")
        {
            return Err(QuotaError::NeedsLogin);
        }
        return Err(QuotaError::Network(format!("MiniMax: {msg}")));
    }
    let data = root.get("data").unwrap_or(&root);
    let plan = ["current_subscribe_title", "plan_name", "current_plan_title"]
        .iter()
        .find_map(|key| text(data.get(*key)))
        .or_else(|| {
            data.get("current_combo_card")
                .and_then(|v| v.get("title"))
                .and_then(|value| text(Some(value)))
        })
        .or_else(|| text(data.get("combo_title")));

    let services = service_buckets(data.get("services").and_then(Value::as_array), now);
    if !services.is_empty() {
        return Ok((services, plan));
    }
    let models = model_buckets(data.get("model_remains").and_then(Value::as_array), now);
    if !models.is_empty() {
        return Ok((models, plan));
    }
    Err(QuotaError::ParseFailure(
        "MiniMax response had no model_remains rows".into(),
    ))
}

fn model_buckets(rows: Option<&Vec<Value>>, now: f64) -> Vec<QuotaBucket> {
    let Some(rows) = rows else { return Vec::new() };
    let mut buckets = Vec::new();
    let mut added_weekly = false;
    for (index, row) in rows.iter().enumerate() {
        let total = int(row.get("current_interval_total_count")).unwrap_or(0);
        let remaining = number(row.get("current_interval_remaining_percent"));
        if total <= 0 && remaining.is_none() {
            continue;
        }
        buckets.push(model_bucket(row, index, now));
        if !added_weekly {
            if let Some(weekly) = weekly_bucket(row, now) {
                buckets.push(weekly);
                added_weekly = true;
            }
        }
    }
    buckets
}

fn model_bucket(row: &Value, index: usize, now: f64) -> QuotaBucket {
    let total = int(row.get("current_interval_total_count"))
        .unwrap_or(0)
        .max(0);
    let label = window_label(row.get("start_time"), row.get("end_time"))
        .unwrap_or_else(|| "5 hours".to_string());
    let (used_percent, group) = usage(
        total,
        int(row.get("current_interval_usage_count")),
        number(row.get("current_interval_remaining_percent")),
        &label,
    );
    let model = text(row.get("model_name"));
    let display = [
        "display_name",
        "display_title",
        "resource_name",
        "service_name",
        "name",
        "title",
    ]
    .iter()
    .find_map(|key| text(row.get(*key)));
    let title = model_title(display.as_deref(), model.as_deref());
    let identity = model.as_deref().unwrap_or(&title);
    QuotaBucket::new(
        format!("minimax.coding.{index}.{}", slug(identity)),
        title,
        "5h",
        used_percent,
        reset_at(row.get("end_time"), row.get("remains_time"), now),
        Some(5 * 3_600),
        Some(group),
    )
}

fn weekly_bucket(row: &Value, now: f64) -> Option<QuotaBucket> {
    let total = int(row.get("current_weekly_total_count"))
        .unwrap_or(0)
        .max(0);
    let remaining = number(row.get("current_weekly_remaining_percent"));
    if total <= 0 && remaining.is_none() {
        return None;
    }
    let (used_percent, group) = usage(
        total,
        int(row.get("current_weekly_usage_count")),
        remaining,
        "weekly",
    );
    Some(QuotaBucket::new(
        "minimax.weekly",
        "Weekly",
        "Wk",
        used_percent,
        reset_at(
            row.get("weekly_end_time"),
            row.get("weekly_remains_time"),
            now,
        ),
        Some(7 * 86_400),
        Some(group),
    ))
}

fn usage(total: i64, used: Option<i64>, remaining: Option<f64>, label: &str) -> (f64, String) {
    if let Some(remaining) = remaining {
        let remaining = remaining.clamp(0.0, 100.0);
        let used_percent = 100.0 - remaining;
        if total > 0 {
            let remaining_count = (total as f64 * remaining / 100.0).round() as i64;
            return (used_percent, format!("{remaining_count}/{total} · {label}"));
        }
        return (used_percent, format!("{remaining:.0}% left · {label}"));
    }
    let used = used.unwrap_or(0).max(0);
    (
        if total > 0 {
            used as f64 / total as f64 * 100.0
        } else {
            0.0
        },
        format!("{}/{} · {label}", (total - used).max(0), total),
    )
}

fn service_buckets(services: Option<&Vec<Value>>, now: f64) -> Vec<QuotaBucket> {
    let Some(services) = services else {
        return Vec::new();
    };
    services
        .iter()
        .filter_map(|service| {
            let raw = ["window_type", "time_range", "service_type"]
                .iter()
                .filter_map(|key| text(service.get(*key)))
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase();
            let (id, title, short, window) = if raw.contains("week") {
                ("minimax.weekly", "Weekly", "Wk", 7 * 86_400)
            } else if raw.contains("month") {
                ("minimax.monthly", "Monthly", "Month", 30 * 86_400)
            } else if raw.contains("day") || raw.contains("24") {
                ("minimax.daily", "Daily", "Day", 86_400)
            } else {
                ("minimax.coding", "5 Hours", "5h", 5 * 3_600)
            };
            let total = number(service.get("limit").or_else(|| service.get("total")));
            let used = number(service.get("usage").or_else(|| service.get("used")));
            let percent = number(service.get("percent"));
            if percent.is_none()
                && (total.filter(|total| *total > 0.0).is_none() || used.is_none())
            {
                return None;
            }
            let used = used.unwrap_or(0.0);
            let used_percent = percent.map_or_else(
                || {
                    total
                        .filter(|total| *total > 0.0)
                        .map_or(0.0, |total| used / total * 100.0)
                },
                |percent| {
                    if percent <= 1.0 {
                        percent * 100.0
                    } else {
                        percent
                    }
                },
            );
            let group =
                total.map(|total| format!("{:.0}/{total:.0} left", (total - used).max(0.0)));
            Some(QuotaBucket::new(
                id,
                title,
                short,
                used_percent,
                service_reset_at(service, now),
                Some(window),
                group,
            ))
        })
        .collect()
}

fn service_reset_at(service: &Value, now: f64) -> Option<f64> {
    let end = service
        .get("end_time")
        .or_else(|| service.get("reset_at"))
        .or_else(|| service.get("reset_time"));
    if let Some(reset) = reset_at(end, service.get("remains_time"), now) {
        return Some(reset);
    }
    service
        .get("reset_in_seconds")
        .and_then(|value| number(Some(value)))
        .filter(|seconds| *seconds > 0.0)
        .map(|seconds| now + seconds)
}

fn reset_at(end: Option<&Value>, remains: Option<&Value>, now: f64) -> Option<f64> {
    end.and_then(|value| number(Some(value)))
        .and_then(epoch_seconds)
        .filter(|end| *end > now)
        .or_else(|| {
            remains
                .and_then(|value| number(Some(value)))
                .filter(|value| *value > 0.0)
                .map(|value| {
                    now + if value > 1_000_000.0 {
                        value / 1_000.0
                    } else {
                        value
                    }
                })
        })
}

fn epoch_seconds(value: f64) -> Option<f64> {
    if value > 1_000_000_000_000.0 {
        Some(value / 1_000.0)
    } else if value > 1_000_000_000.0 {
        Some(value)
    } else {
        None
    }
}

fn window_label(start: Option<&Value>, end: Option<&Value>) -> Option<String> {
    let start = start
        .and_then(|value| number(Some(value)))
        .and_then(epoch_seconds)?;
    let end = end
        .and_then(|value| number(Some(value)))
        .and_then(epoch_seconds)?;
    if end <= start {
        return None;
    }
    let hours = ((end - start) / 3_600.0).round() as i64;
    Some(match hours {
        4..=6 => "5 hours".to_string(),
        23..=25 => "Today".to_string(),
        _ if hours > 0 => format!("{hours} hours"),
        _ => return None,
    })
}

fn number(value: Option<&Value>) -> Option<f64> {
    value.and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_str()?.trim().parse().ok())
    })
}

fn int(value: Option<&Value>) -> Option<i64> {
    number(value).map(|value| value as i64)
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn slug(value: &str) -> String {
    let mut slug = String::new();
    for character in value.to_ascii_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() { "bucket" } else { slug }.to_string()
}

fn model_title(display: Option<&str>, model: Option<&str>) -> String {
    let display = display.unwrap_or("");
    let model = model.unwrap_or("");
    let lower_display = display.to_ascii_lowercase();
    let lower_model = model.to_ascii_lowercase();
    if lower_model == "general" {
        return "5 Hours".to_string();
    }
    if display.contains("文本生成") || lower_model.contains("minimax-m") {
        return "Text Generation".to_string();
    }
    if display.contains("语音合成") || lower_model.contains("speech") {
        return if lower_display.contains("hd")
            || display.contains("高保真")
            || lower_model.contains("hd")
        {
            "Text to Speech HD"
        } else {
            "Text to Speech"
        }
        .to_string();
    }
    if display.contains("视频生成") || lower_model.contains("hailuo") {
        return if display.contains("高速版")
            || lower_display.contains("fast")
            || lower_model.contains("fast")
        {
            "Video Generation Fast"
        } else {
            "Video Generation Standard"
        }
        .to_string();
    }
    if display.contains("音乐翻唱") || lower_model == "music-cover" {
        return "Music Cover".to_string();
    }
    if display.contains("音乐生成") || lower_model.starts_with("music-") {
        let suffix = version_suffix(display).or_else(|| version_suffix(model));
        return suffix.map_or_else(
            || "Music Generation".to_string(),
            |suffix| format!("Music Generation {suffix}"),
        );
    }
    if display.contains("歌词生成") || lower_model.contains("lyrics") {
        return "Lyrics Generation".to_string();
    }
    if display.contains("图像生成") || lower_model.starts_with("image-") {
        return "Image Generation".to_string();
    }
    if display.contains("图片理解") || lower_model == "coding-plan-vlm" {
        return "Image Understanding".to_string();
    }
    if display.contains("网络搜索") || lower_model == "coding-plan-search" {
        return "Web Search".to_string();
    }
    if !display.trim().is_empty() {
        display.trim().to_string()
    } else if !model.trim().is_empty() {
        model.trim().to_string()
    } else {
        "5 Hours".to_string()
    }
}

fn version_suffix(value: &str) -> Option<String> {
    for word in
        value.split(|character: char| !character.is_ascii_alphanumeric() && character != '.')
    {
        let raw = word
            .strip_prefix('v')
            .or_else(|| word.strip_prefix('V'))
            .unwrap_or(word);
        if raw.contains('.')
            && raw
                .chars()
                .all(|character| character.is_ascii_digit() || character == '.')
        {
            return Some(format!("v{raw}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn key_and_endpoint_priority() {
        let mut e = HashMap::new();
        e.insert("MINIMAX_API_KEY".into(), "env".into());
        e.insert("MINIMAX_CODING_API_KEY".into(), "coding".into());
        e.insert(
            "MINIMAX_REMAINS_URL".into(),
            "https://proxy.minimax.io/remains".into(),
        );
        assert_eq!(resolve_api_key(&e).as_deref(), Some("coding"));
        assert_eq!(
            remains_urls(&e, false)[0],
            "https://proxy.minimax.io/remains"
        );
        e.insert(
            "MINIMAX_REMAINS_URL".into(),
            "https://example.test/steal".into(),
        );
        assert_eq!(
            remains_urls(&e, false)[0],
            "https://www.minimax.io/v1/token_plan/remains"
        );
    }
    #[test]
    fn parses_model_and_weekly() {
        let j = r#"{"data":{"current_subscribe_title":"Pro","model_remains":[{"model_name":"MiniMax-M2","current_interval_total_count":100,"current_interval_usage_count":25,"end_time":2000000000,"current_weekly_total_count":1000,"current_weekly_usage_count":100,"weekly_end_time":2000000100}]}}"#;
        let (b, p) = parse(j.as_bytes(), 1_700_000_000.0).unwrap();
        assert_eq!(p.as_deref(), Some("Pro"));
        assert_eq!(b[0].id, "minimax.coding.0.minimax-m2");
        assert_eq!(b[1].id, "minimax.weekly");
        assert_eq!(b[1].reset_at, Some(2_000_000_100.0));
        assert_eq!(b[1].group_title.as_deref(), Some("900/1000 · weekly"));
    }
    #[test]
    fn remaining_percent_and_service_shapes_match_native() {
        let model = br#"{"model_remains":[{"model_name":"general","current_interval_total_count":0,"current_interval_remaining_percent":88,"current_weekly_total_count":0,"current_weekly_remaining_percent":82}],"base_resp":{"status_code":0}}"#;
        let (buckets, _) = parse(model, 1_700_000_000.0).unwrap();
        assert_eq!(buckets[0].used_percent, 12.0);
        assert_eq!(buckets[1].used_percent, 18.0);

        let service = br#"{"data":{"services":[{"service_type":"coding","window_type":"weekly","usage":"25","limit":"100","reset_in_seconds":"3600"}]}}"#;
        let (buckets, _) = parse(service, 1_700_000_000.0).unwrap();
        assert_eq!(buckets[0].id, "minimax.weekly");
        assert_eq!(buckets[0].used_percent, 25.0);
        assert_eq!(buckets[0].reset_at, Some(1_700_003_600.0));

        let monthly = br#"{"data":{"services":[{"window_type":"monthly","percent":0.25,"reset_in_seconds":2592000}]}}"#;
        let (buckets, _) = parse(monthly, 1_700_000_000.0).unwrap();
        assert_eq!(buckets[0].used_percent, 25.0);
        assert_eq!(buckets[0].reset_at, Some(1_702_592_000.0));
    }
    #[test]
    fn zero_limit_service_placeholders_do_not_hide_model_rows() {
        let body = br#"{"data":{"services":[{"service_type":"coding","limit":0},{"service_type":"coding","limit":100}],"model_remains":[{"model_name":"MiniMax-M2","current_interval_total_count":100,"current_interval_usage_count":25}]}}"#;
        let (buckets, _) = parse(body, 1_700_000_000.0).unwrap();
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].id, "minimax.coding.0.minimax-m2");
        assert_eq!(buckets[0].used_percent, 25.0);
    }
    #[test]
    fn dashboard_model_names_are_stable_english_labels() {
        assert_eq!(
            model_title(Some("音乐生成 · v2.5"), Some("music-2.5")),
            "Music Generation v2.5"
        );
        assert_eq!(
            model_title(Some("视频生成 · 高速版"), Some("hailuo-2.3-fast")),
            "Video Generation Fast"
        );
        assert_eq!(
            model_title(Some("图片理解"), Some("coding-plan-vlm")),
            "Image Understanding"
        );
    }
    #[test]
    fn auth_and_missing_rows_fail() {
        assert_eq!(
            parse(
                br#"{"base_resp":{"status_code":1004,"status_msg":"login fail"}}"#,
                0.0
            )
            .unwrap_err(),
            QuotaError::NeedsLogin
        );
        assert!(matches!(
            parse(br#"{"data":{"model_remains":[]}}"#, 0.0),
            Err(QuotaError::ParseFailure(_))
        ));
        assert!(matches!(
            parse(
                br#"{"data":{"model_remains":[{"model_name":"inactive","current_interval_total_count":0}]}}"#,
                0.0
            ),
            Err(QuotaError::ParseFailure(_))
        ));
    }
}
