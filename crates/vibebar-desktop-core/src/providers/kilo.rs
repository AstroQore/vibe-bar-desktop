//! Kilo quota adapter: tRPC batch endpoint and CLI API-key contract.

use crate::{
    error::QuotaError,
    model::{AccountQuota, QuotaBucket, QuotaOrigin, ToolType},
};
use chrono::DateTime;
use serde_json::Value;
use std::{collections::HashMap, io::Read, path::Path};

const PROCEDURES: &[&str] = &[
    "user.getCreditBlocks",
    "kiloPass.getState",
    "user.getAutoTopUpPaymentMethod",
];

pub fn resolve_token(
    home: &Path,
    env: &HashMap<String, String>,
    allow_cli: bool,
) -> Option<String> {
    if let Some(value) = env
        .get("KILO_API_KEY")
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
    {
        return Some(value.trim_matches(['\'', '"']).to_owned());
    }
    if !allow_cli {
        return None;
    }
    let path = home.join(".local/share/kilo/auth.json");
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > 1024 * 1024 {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() > 1024 * 1024 {
        return None;
    }
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    value
        .get("kilo")?
        .get("access")?
        .as_str()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

pub fn batch_url(base: &str) -> Result<String, QuotaError> {
    let base = base.trim_end_matches('/');
    let input = format!(
        "{{{}}}",
        PROCEDURES
            .iter()
            .enumerate()
            .map(|(i, _)| format!("\"{i}\":{{\"json\":null}}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    Ok(format!(
        "{base}/{}?batch=1&input={}",
        PROCEDURES.join(","),
        urlencoding(&input)
    ))
}

fn urlencoding(input: &str) -> String {
    input
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub async fn fetch(client: &reqwest::Client, home: &Path) -> Result<AccountQuota, QuotaError> {
    let env: HashMap<String, String> = std::env::vars().collect();
    let token = resolve_token(home, &env, true).ok_or(QuotaError::NoCredential)?;
    let base = env
        .get("KILO_API_URL")
        .map(String::as_str)
        .map(str::trim)
        .filter(|url| reqwest::Url::parse(url).is_ok())
        .unwrap_or("https://app.kilo.ai/api/trpc");
    let response = client
        .get(batch_url(base)?)
        .timeout(super::REQUEST_TIMEOUT)
        .bearer_auth(token)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| super::classify_transport(&e))?;
    match response.status().as_u16() {
        200 => {}
        401 | 403 => return Err(QuotaError::NeedsLogin),
        404 => return Err(QuotaError::Network("Kilo tRPC endpoint not found".into())),
        429 => return Err(QuotaError::RateLimited),
        status @ 500..=599 => {
            return Err(QuotaError::Network(format!(
                "Kilo service unavailable (HTTP {status})"
            )))
        }
        status => return Err(QuotaError::Network(format!("Kilo returned HTTP {status}"))),
    }
    let body = response
        .bytes()
        .await
        .map_err(|e| super::classify_transport(&e))?;
    let (buckets, plan) = parse(&body, super::now_unix())?;
    Ok(AccountQuota {
        account_id: "misc-kilo".into(),
        tool: ToolType::Kilo,
        buckets,
        plan,
        queried_at: super::now_unix(),
        origin: QuotaOrigin::Live,
        error: None,
    })
}

pub fn parse(body: &[u8], now: f64) -> Result<(Vec<QuotaBucket>, Option<String>), QuotaError> {
    let root: Value =
        serde_json::from_slice(body).map_err(|e| QuotaError::ParseFailure(e.to_string()))?;
    let entries: Vec<(usize, &Value)> = match &root {
        Value::Array(values) => values.iter().enumerate().collect(),
        Value::Object(o) if o.keys().all(|k| k.parse::<usize>().is_ok()) => {
            let mut indexed: Vec<(usize, &Value)> = o
                .iter()
                .filter_map(|(key, value)| key.parse().ok().map(|index| (index, value)))
                .collect();
            indexed.sort_by_key(|(i, _)| *i);
            indexed
        }
        Value::Object(_) => vec![(0, &root)],
        _ => {
            return Err(QuotaError::ParseFailure(
                "Kilo response had unexpected tRPC shape".into(),
            ))
        }
    };
    let mut credits = None;
    let mut pass = None;
    for (index, entry) in entries {
        if let Some(error) = entry.get("error") {
            let text = error.to_string().to_ascii_lowercase();
            if text.contains("unauthorized") || text.contains("forbidden") {
                return Err(QuotaError::NeedsLogin);
            }
            if text.contains("not_found") || text.contains("not found") {
                return Err(QuotaError::Network("Kilo tRPC endpoint not found".into()));
            }
            if index < 2 {
                return Err(QuotaError::ParseFailure("Kilo tRPC error payload".into()));
            }
        }
        let payload = entry
            .get("result")
            .and_then(|r| r.get("data"))
            .and_then(|d| d.get("json"))
            .or_else(|| entry.get("result").and_then(|r| r.get("json")))
            .or_else(|| entry.get("result").and_then(|r| r.get("data")));
        if index == 0 {
            credits = payload;
        } else if index == 1 {
            pass = payload;
        }
    }
    let mut buckets = Vec::new();
    if let Some(value) = credits {
        if let Some((used, total, remaining)) = amounts(value) {
            let total = total.or_else(|| used.zip(remaining).map(|(u, r)| u + r));
            if let Some(total) = total {
                let used = used.unwrap_or_else(|| (total - remaining.unwrap_or(0.0)).max(0.0));
                buckets.push(QuotaBucket::new(
                    "kilo.credits",
                    "Credits",
                    "Credits",
                    if total > 0.0 {
                        used / total * 100.0
                    } else {
                        100.0
                    },
                    None,
                    None,
                    Some(format!("{used:.2}/{total:.2} credits")),
                ));
            }
        }
    }
    let mut plan = None;
    if let Some(value) = pass {
        plan = plan_name(value);
        let subscription = value.get("subscription").unwrap_or(value);
        let base = number(subscription, "currentPeriodBaseCreditsUsd");
        let bonus = number(subscription, "currentPeriodBonusCreditsUsd").unwrap_or(0.0);
        let used = number(subscription, "currentPeriodUsageUsd");
        if let Some(total) = base
            .map(|v| v + bonus)
            .or_else(|| amounts(value).and_then(|(_, t, _)| t))
        {
            let reset = ["nextBillingAt", "nextRenewalAt", "renewsAt", "renewAt"]
                .iter()
                .find_map(|key| subscription.get(*key).and_then(epoch))
                .filter(|v| *v > now);
            let used = used.unwrap_or(0.0);
            let base = base.unwrap_or((total - bonus).max(0.0));
            let group = if bonus > 0.0 {
                format!(
                    "{} / {} (+ {} bonus)",
                    money(used),
                    money(base),
                    money(bonus)
                )
            } else {
                format!("{} / {}", money(used), money(base))
            };
            buckets.push(QuotaBucket::new(
                "kilo.pass",
                "Kilo Pass",
                "Pass",
                if total > 0.0 {
                    used / total * 100.0
                } else {
                    100.0
                },
                reset,
                None,
                Some(group),
            ));
        }
    }
    if buckets.is_empty() {
        return Err(QuotaError::ParseFailure(
            "Kilo response had no usable credit windows".into(),
        ));
    }
    Ok((buckets, plan))
}

fn amounts(value: &Value) -> Option<(Option<f64>, Option<f64>, Option<f64>)> {
    if let Some(blocks) = value.get("creditBlocks").and_then(Value::as_array) {
        let total = blocks
            .iter()
            .filter_map(|b| number(b, "amount_mUsd"))
            .sum::<f64>()
            / 1_000_000.0;
        let remaining = blocks
            .iter()
            .filter_map(|b| number(b, "balance_mUsd"))
            .sum::<f64>()
            / 1_000_000.0;
        if total > 0.0 || remaining > 0.0 {
            return Some((
                Some((total - remaining).max(0.0)),
                Some(total),
                Some(remaining),
            ));
        }
    }
    let contexts = [
        value,
        value.get("data").unwrap_or(value),
        value.get("subscription").unwrap_or(value),
    ];
    let find = |keys: &[&str]| {
        contexts
            .iter()
            .find_map(|context| keys.iter().find_map(|key| number(context, key)))
    };
    let used = find(&["used", "usage", "currentPeriodUsageUsd", "creditsUsed"]);
    let total = find(&[
        "total",
        "limit",
        "currentPeriodBaseCreditsUsd",
        "creditsTotal",
    ]);
    let remaining = find(&["remaining", "balance", "creditsRemaining"]);
    if used.is_none() && total.is_none() && remaining.is_none() {
        None
    } else {
        Some((used, total, remaining))
    }
}

fn number(value: &Value, key: &str) -> Option<f64> {
    value
        .get(key)
        .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
}

fn plan_name(value: &Value) -> Option<String> {
    let subscription = value.get("subscription");
    if let Some(tier) = subscription
        .and_then(|value| value.get("tier"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(
            match tier {
                "tier_19" => "Starter",
                "tier_49" => "Pro",
                "tier_199" => "Expert",
                _ => tier,
            }
            .to_string(),
        );
    }
    for context in [value, value.get("data").unwrap_or(value)] {
        for key in [
            "planName",
            "tier",
            "tierName",
            "passName",
            "subscriptionName",
        ] {
            if let Some(plan) = context
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                return Some(plan.to_string());
            }
        }
    }
    subscription
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn money(value: f64) -> String {
    format!("${value:.2}")
}

fn epoch(value: &Value) -> Option<f64> {
    if let Some(number) = value.as_f64() {
        return Some(if number > 1e12 {
            number / 1000.0
        } else {
            number
        });
    }
    let text = value.as_str()?.trim();
    if let Ok(number) = text.parse::<f64>() {
        return Some(if number > 1e12 {
            number / 1000.0
        } else {
            number
        });
    }
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|value| value.timestamp() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn env_key_and_cli_fallback_are_bounded() {
        let mut env = HashMap::new();
        env.insert("KILO_API_KEY".into(), "'env-key'".into());
        assert_eq!(
            resolve_token(Path::new("/synthetic"), &env, false).as_deref(),
            Some("env-key")
        );
        assert!(batch_url("https://app.kilo.ai/api/trpc")
            .unwrap()
            .contains("batch=1&input="));
    }
    #[test]
    fn parses_credit_and_pass_shapes() {
        let j=br#"[{"result":{"data":{"creditBlocks":[{"amount_mUsd":100000000,"balance_mUsd":75000000} ]}}},{"result":{"data":{"subscription":{"tier":"tier_49","currentPeriodUsageUsd":5,"currentPeriodBaseCreditsUsd":50,"currentPeriodBonusCreditsUsd":10,"nextBillingAt":1800000000}}}}]"#;
        let (b, p) = parse(j, 1700000000.0).unwrap();
        assert_eq!(p.as_deref(), Some("Pro"));
        assert_eq!(
            b.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(),
            vec!["kilo.credits", "kilo.pass"]
        );
        assert_eq!(b[0].used_percent, 25.0);
        assert!((b[1].used_percent - 8.333_333).abs() < 0.001);
        assert_eq!(
            b[1].group_title.as_deref(),
            Some("$5.00 / $50.00 (+ $10.00 bonus)")
        );
        assert_eq!(b[1].reset_at, Some(1_800_000_000.0));
    }
    #[test]
    fn malformed_and_auth_errors_classify() {
        assert!(matches!(
            parse(b"{}", 0.0),
            Err(QuotaError::ParseFailure(_))
        ));
        let e = parse(br#"[{"error":{"message":"UNAUTHORIZED"}}]"#, 0.0).unwrap_err();
        assert_eq!(e, QuotaError::NeedsLogin);
    }
}
