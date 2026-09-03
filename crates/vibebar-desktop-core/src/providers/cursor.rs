//! Cursor (cursor.com) quota — the native `CursorQuotaAdapter`.
//!
//! Auth is Cursor.app's own read-only local session
//! ([`crate::credentials::cursor`]); the browser-cookie fallback the native app
//! also has waits on a cookie reader. Four endpoints, all first-party:
//!
//! 1. `GET /api/usage-summary` — Pro / Business / Enterprise / Free.
//! 2. `GET /api/auth/me` — identity (email, plan).
//! 3. `POST /api/dashboard/get-sand-usage-status` — Grok Bot weekly quota.
//! 4. `GET /api/usage?user=<id>` — fallback for legacy "request plan"
//!    accounts whose summary has no plan block.
//!
//! Output, in the native app's words: **Cursor Models / Monthly** (the
//! first-party pool), **Other Models / Monthly** (named third-party models),
//! **Grok Bot / Weekly** (the cloud-only Bot allowance). On-demand spend is
//! billing state, not a quota lane, and never becomes a bucket.

use std::path::Path;

use reqwest::{Client, Url};
use serde::Deserialize;

use crate::error::QuotaError;
use crate::model::{AccountQuota, QuotaBucket, QuotaOrigin, ToolType};

/// The id the native app already writes into `accounts.json`, the quota
/// cache, the fill and forecast timelines and the subscription history —
/// a naming accident from when Cursor lived on the Misc page, kept because
/// renaming it would orphan every one of those caches.
const ACCOUNT_ID: &str = "misc-cursor";
const BASE_URL: &str = "https://cursor.com";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";

pub async fn fetch(home: &Path, client: &Client) -> Result<AccountQuota, QuotaError> {
    let session = crate::credentials::cursor::load(home)?;
    let cookie = session.cookie_header();

    let summary = decode_summary(&get(client, "/api/usage-summary", &cookie).await?)?;
    let (user_info, grok_bot) = tokio::join!(
        get(client, "/api/auth/me", &cookie),
        post_empty(client, "/api/dashboard/get-sand-usage-status", &cookie),
    );
    // Identity is decoration: it names the account and, for a legacy plan,
    // supplies the id the fallback needs. Losing it costs no lane.
    let user_info = user_info
        .ok()
        .and_then(|body| serde_json::from_slice::<UserInfo>(&body).ok());
    // The Bot lane is not decoration. Publishing a snapshot without it would
    // write the lane out of the shared cache, so anything other than "this
    // account has no Bot dashboard" fails the read and leaves the last good
    // observation in place.
    let grok_bot = match grok_bot {
        Ok(Fetched::Body(body)) => Some(serde_json::from_slice::<GrokBotUsage>(&body).map_err(
            |error| {
                QuotaError::ParseFailure(format!("Cursor Grok Bot status not parseable: {error}"))
            },
        )?),
        Ok(Fetched::NotFound) => None,
        Err(error) => return Err(error),
    };

    // The legacy request-plan fallback fires only when the summary yields no
    // primary lane at all. An Enterprise or team payload reporting through
    // `overall` or `pooled` is already a usable reading, and asking the
    // legacy endpoint for it would risk failing a refresh that had its
    // answer.
    let mut request_usage = None;
    if !parse_summary(&summary, None, None).has_primary_lane() {
        let user_id = user_info
            .as_ref()
            .and_then(|u| u.sub.clone().or_else(|| u.id.clone()));
        if let Some(user_id) = user_id {
            // This is the only source of a plan-less account's numbers, so its
            // failure is the read's failure. Swallowing it would publish 0%
            // over the last good observation, in both caches.
            request_usage = Some(fetch_request_usage(client, &user_id, &cookie).await?);
        }
    }

    let snapshot = parse_summary(&summary, request_usage.as_ref(), grok_bot.as_ref());
    // The Bot lane is an add-on. A snapshot carrying only that one means the
    // account's own quota went unread — publishing it would write the Cursor
    // lanes out of the shared cache.
    if !snapshot.has_primary_lane() {
        return Err(QuotaError::ParseFailure(
            "Cursor usage-summary carried no usage".into(),
        ));
    }
    Ok(AccountQuota {
        account_id: ACCOUNT_ID.to_string(),
        tool: ToolType::Cursor,
        buckets: snapshot.buckets,
        plan: snapshot.plan,
        queried_at: super::now_unix(),
        origin: QuotaOrigin::Live,
        error: None,
    })
}

fn url(path: &str) -> Url {
    Url::parse(&format!("{BASE_URL}{path}")).expect("the built-in Cursor URL is valid")
}

async fn get(client: &Client, path: &str, cookie: &str) -> Result<Vec<u8>, QuotaError> {
    let response = client
        .get(url(path))
        .timeout(super::REQUEST_TIMEOUT)
        .header("Cookie", cookie)
        .header("Accept", "application/json")
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|error| super::classify_transport(&error))?;
    body(path, response).await
}

async fn post_empty(client: &Client, path: &str, cookie: &str) -> Result<Fetched, QuotaError> {
    let response = client
        .post(url(path))
        .timeout(super::REQUEST_TIMEOUT)
        .header("Cookie", cookie)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("Origin", BASE_URL)
        .header("User-Agent", USER_AGENT)
        .body("{}")
        .send()
        .await
        .map_err(|error| super::classify_transport(&error))?;
    fetched(path, response).await
}

/// A response body, or the reason there is none. `NotFound` is separated out
/// because for one endpoint it is an answer rather than a failure.
enum Fetched {
    Body(Vec<u8>),
    NotFound,
}

async fn body(path: &str, response: reqwest::Response) -> Result<Vec<u8>, QuotaError> {
    match fetched(path, response).await? {
        Fetched::Body(body) => Ok(body),
        Fetched::NotFound => Err(QuotaError::Network(format!(
            "Cursor {path} returned HTTP 404"
        ))),
    }
}

async fn fetched(path: &str, response: reqwest::Response) -> Result<Fetched, QuotaError> {
    match response.status().as_u16() {
        200 => {}
        401 | 403 => return Err(QuotaError::NeedsLogin),
        404 => return Ok(Fetched::NotFound),
        429 => return Err(QuotaError::RateLimited),
        status => {
            return Err(QuotaError::Network(format!(
                "Cursor {path} returned HTTP {status}"
            )))
        }
    }
    Ok(Fetched::Body(
        response
            .bytes()
            .await
            .map_err(|error| super::classify_transport(&error))?
            .to_vec(),
    ))
}

async fn fetch_request_usage(
    client: &Client,
    user_id: &str,
    cookie: &str,
) -> Result<RequestUsage, QuotaError> {
    let mut endpoint = url("/api/usage");
    endpoint.query_pairs_mut().append_pair("user", user_id);
    let response = client
        .get(endpoint)
        .timeout(super::REQUEST_TIMEOUT)
        .header("Cookie", cookie)
        .header("Accept", "application/json")
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        // This URL carries the account id in its query, and a transport error
        // stringifies the URL it failed on. The error is shown in the UI, so
        // the id is stripped before it becomes text.
        .map_err(|error| super::classify_transport(&error.without_url()))?;
    let bytes = body("/api/usage", response).await?;
    serde_json::from_slice(&bytes)
        .map_err(|_| QuotaError::ParseFailure("Cursor /api/usage not parseable".into()))
}

// MARK: - Wire types

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub individual_usage: Option<IndividualUsage>,
    pub team_usage: Option<TeamUsage>,
    pub membership_type: Option<String>,
    pub billing_cycle_start: Option<String>,
    pub billing_cycle_end: Option<String>,
}

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IndividualUsage {
    pub plan: Option<PlanUsage>,
    pub on_demand: Option<OnDemandUsage>,
    pub overall: Option<PlanUsage>,
}

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanUsage {
    pub used: Option<i64>,
    pub limit: Option<i64>,
    pub auto_percent_used: Option<f64>,
    pub api_percent_used: Option<f64>,
    pub total_percent_used: Option<f64>,
}

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OnDemandUsage {
    pub used: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TeamUsage {
    pub pooled: Option<PlanUsage>,
    pub on_demand: Option<OnDemandUsage>,
}

#[derive(Debug, Default, Deserialize, PartialEq)]
pub struct UserInfo {
    pub email: Option<String>,
    pub id: Option<String>,
    pub sub: Option<String>,
}

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GrokBotUsage {
    pub current_period_start: Option<String>,
    pub next_reset_timestamp_utc: Option<String>,
    pub usage_percent: Option<f64>,
    pub included_limit_zero: Option<bool>,
    pub has_available_usage: Option<bool>,
    pub has_non_zero_included_limit: Option<bool>,
}

#[derive(Debug, Default, Deserialize, PartialEq)]
pub struct RequestUsage {
    #[serde(rename = "gpt-4")]
    pub gpt4: Option<RequestUsageEntry>,
}

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestUsageEntry {
    pub num_requests: Option<i64>,
    pub num_requests_total: Option<i64>,
    pub max_request_usage: Option<i64>,
}

// MARK: - Parsing

#[derive(Debug, PartialEq)]
pub struct Snapshot {
    pub buckets: Vec<QuotaBucket>,
    pub plan: Option<String>,
}

impl Snapshot {
    /// Whether this says anything about Cursor's own quota, as opposed to
    /// only the Grok Bot add-on that rides along with it.
    pub fn has_primary_lane(&self) -> bool {
        self.buckets
            .iter()
            .any(|bucket| bucket.id != "grok_bot_weekly")
    }
}

pub fn decode_summary(body: &[u8]) -> Result<UsageSummary, QuotaError> {
    serde_json::from_slice(body).map_err(|error| {
        QuotaError::ParseFailure(format!("Cursor usage-summary not parseable: {error}"))
    })
}

/// Assemble the buckets. Free of I/O so every wire shape the native tests
/// pin — Pro fractional percent, Enterprise `overall`, pooled team usage,
/// the legacy request plan, on-demand spend, Grok Bot — is checked here.
pub fn parse_summary(
    summary: &UsageSummary,
    request_usage: Option<&RequestUsage>,
    grok_bot: Option<&GrokBotUsage>,
) -> Snapshot {
    let plan = summary
        .individual_usage
        .as_ref()
        .and_then(|u| u.plan.as_ref());
    let overall = summary
        .individual_usage
        .as_ref()
        .and_then(|u| u.overall.as_ref());
    let pooled = summary.team_usage.as_ref().and_then(|u| u.pooled.as_ref());

    // Cursor's percent fields are already in percent units even when
    // fractional: 0.36 means 0.36%, not 36%. Clamp, never scale.
    let auto_pct = plan.and_then(|p| p.auto_percent_used).map(clamp_percent);
    let api_pct = plan.and_then(|p| p.api_percent_used).map(clamp_percent);

    let ratio = |used: Option<i64>, limit: Option<i64>| -> Option<f64> {
        match (used, limit) {
            (Some(used), Some(limit)) if limit > 0 => {
                Some(clamp_percent(used as f64 / limit as f64 * 100.0))
            }
            _ => None,
        }
    };
    // Every fallback in order; `None` at the end means the response carried no
    // usage at all, which is not a zero — it is nothing to report.
    let total_pct = plan
        .and_then(|p| p.total_percent_used)
        .map(clamp_percent)
        .or_else(|| match (auto_pct, api_pct) {
            (Some(auto), Some(api)) => Some(clamp_percent((auto + api) / 2.0)),
            (None, Some(api)) => Some(api),
            (Some(auto), None) => Some(auto),
            (None, None) => None,
        })
        .or_else(|| plan.and_then(|p| ratio(p.used, p.limit)))
        .or_else(|| overall.and_then(|o| ratio(o.used, o.limit)))
        .or_else(|| pooled.and_then(|p| ratio(p.used, p.limit)))
        .or_else(|| {
            // Legacy request plan: usage / max if present.
            let entry = request_usage.and_then(|r| r.gpt4.as_ref())?;
            let max = entry.max_request_usage.filter(|max| *max > 0)?;
            // An absent count is not a count of zero: a payload with a
            // maximum and no usage figure says nothing about this account.
            let used = entry.num_requests_total.or(entry.num_requests)?;
            Some(clamp_percent(used as f64 / max as f64 * 100.0))
        });

    let cycle_start = parse_timestamp(summary.billing_cycle_start.as_deref());
    let cycle_end = parse_timestamp(summary.billing_cycle_end.as_deref());
    let cycle_window = window_seconds(cycle_start, cycle_end);

    let mut buckets = Vec::new();
    // Cursor renamed the old Auto/API lanes in August 2026. The wire fields
    // keep their names; the product's current labels are used.
    if let Some(auto) = auto_pct {
        buckets.push(QuotaBucket::new(
            "models",
            "Monthly",
            "Cursor",
            auto,
            cycle_end,
            cycle_window,
            Some("Cursor Models".into()),
        ));
    }
    if let Some(api) = api_pct {
        buckets.push(QuotaBucket::new(
            "other_models",
            "Monthly",
            "Other",
            api,
            cycle_end,
            cycle_window,
            Some("Other Models".into()),
        ));
    }
    // Older plan shapes expose one aggregate or request quota rather than
    // the two modern pools; that usage stays visible under Cursor Models.
    // A response with no usage anywhere produces no bucket at all rather than
    // a confident zero.
    if let (true, Some(total)) = (buckets.is_empty(), total_pct) {
        buckets.push(QuotaBucket::new(
            "models",
            "Monthly",
            "Cursor",
            total,
            cycle_end,
            cycle_window,
            Some("Cursor Models".into()),
        ));
    }

    if let Some(bot) = grok_bot {
        if let Some(percent) = bot.usage_percent {
            // Both spellings of "no Bot allowance": the current flag, and the
            // older payload's inverted one. Either means no lane to draw.
            let has_allowance = bot.has_non_zero_included_limit != Some(false)
                && bot.included_limit_zero != Some(true);
            if has_allowance {
                let period_start = parse_timestamp(bot.current_period_start.as_deref());
                let reset_at = parse_timestamp(bot.next_reset_timestamp_utc.as_deref());
                buckets.push(QuotaBucket::new(
                    "grok_bot_weekly",
                    "Weekly",
                    "Grok Bot",
                    clamp_percent(percent),
                    reset_at,
                    window_seconds(period_start, reset_at),
                    Some("Grok Bot".into()),
                ));
            }
        }
    }

    Snapshot {
        buckets,
        plan: plan_name(summary.membership_type.as_deref(), request_usage),
    }
}

fn clamp_percent(value: f64) -> f64 {
    value.clamp(0.0, 100.0)
}

/// RFC 3339 with or without fractional seconds, as Unix seconds.
fn parse_timestamp(raw: Option<&str>) -> Option<f64> {
    let raw = raw?.trim();
    let parsed = chrono::DateTime::parse_from_rfc3339(raw).ok()?;
    Some(parsed.timestamp() as f64 + f64::from(parsed.timestamp_subsec_nanos()) / 1e9)
}

fn window_seconds(start: Option<f64>, end: Option<f64>) -> Option<i64> {
    match (start, end) {
        (Some(start), Some(end)) if end > start => Some((end - start).round() as i64),
        _ => None,
    }
}

fn plan_name(membership: Option<&str>, request_usage: Option<&RequestUsage>) -> Option<String> {
    let raw = membership.map(str::trim).unwrap_or("");
    if !raw.is_empty() {
        return Some(match raw.to_ascii_lowercase().as_str() {
            "free" => "Free".to_string(),
            "free_trial" => "Free Trial".to_string(),
            "pro" => "Pro".to_string(),
            "business" => "Business".to_string(),
            "enterprise" => "Enterprise".to_string(),
            _ => capitalized(raw),
        });
    }
    if request_usage
        .and_then(|r| r.gpt4.as_ref())
        .and_then(|e| e.max_request_usage)
        .is_some()
    {
        return Some("Legacy".to_string());
    }
    None
}

/// Swift's `capitalized`: every word's first letter upper, the rest lower.
fn capitalized(raw: &str) -> String {
    raw.split(' ')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(json: &str) -> Snapshot {
        parse_summary(&decode_summary(json.as_bytes()).unwrap(), None, None)
    }

    fn bucket<'a>(snapshot: &'a Snapshot, id: &str) -> &'a QuotaBucket {
        snapshot
            .buckets
            .iter()
            .find(|b| b.id == id)
            .unwrap_or_else(|| panic!("no bucket {id}"))
    }

    /// Cursor's percent fields are already in percent units even when
    /// fractional (0.36 means 0.36%, not 36%): the parser must not scale.
    #[test]
    fn pro_fractional_percent_is_not_scaled() {
        let snap = snapshot(
            r#"{
              "membershipType": "pro",
              "billingCycleStart": "2026-05-01T00:00:00Z",
              "billingCycleEnd": "2026-06-01T00:00:00Z",
              "individualUsage": {
                "plan": {"used": 7384, "limit": 20000, "totalPercentUsed": 0.36,
                         "autoPercentUsed": 0.20, "apiPercentUsed": 0.52},
                "onDemand": {"used": 0, "limit": 0}
              }
            }"#,
        );
        assert_eq!(snap.plan.as_deref(), Some("Pro"));
        let titles: Vec<&str> = snap.buckets.iter().map(|b| b.title.as_str()).collect();
        assert_eq!(titles, ["Monthly", "Monthly"]);
        let groups: Vec<Option<&str>> = snap
            .buckets
            .iter()
            .map(|b| b.group_title.as_deref())
            .collect();
        assert_eq!(groups, [Some("Cursor Models"), Some("Other Models")]);
        let models = bucket(&snap, "models");
        assert!((models.used_percent - 0.20).abs() < 0.001);
        assert_eq!(models.raw_window_seconds, Some(2_678_400));
        assert_eq!(models.reset_at, Some(1_780_272_000.0));
        assert!((bucket(&snap, "other_models").used_percent - 0.52).abs() < 0.001);
    }

    /// Enterprise / team-member personal cap under `individualUsage.overall`.
    #[test]
    fn enterprise_falls_back_to_overall() {
        let snap = snapshot(
            r#"{"membershipType": "enterprise",
                "individualUsage": {"overall": {"used": 7500, "limit": 10000}}}"#,
        );
        assert_eq!(snap.plan.as_deref(), Some("Enterprise"));
        assert!((bucket(&snap, "models").used_percent - 75.0).abs() < 0.01);
    }

    /// Shared pool under `teamUsage.pooled` when neither plan nor overall is there.
    #[test]
    fn a_team_falls_back_to_the_pooled_quota() {
        let snap = snapshot(
            r#"{"membershipType": "business", "individualUsage": {},
                "teamUsage": {"pooled": {"used": 4000, "limit": 50000}}}"#,
        );
        assert_eq!(snap.plan.as_deref(), Some("Business"));
        assert!((bucket(&snap, "models").used_percent - 8.0).abs() < 0.01);
    }

    /// Legacy "request plan": no plan block, so the numbers come from
    /// `/api/usage`'s `gpt-4` entry.
    #[test]
    fn a_legacy_request_plan_uses_the_request_counts() {
        let summary = decode_summary(br#"{ "individualUsage": {} }"#).unwrap();
        let requests: RequestUsage =
            serde_json::from_str(r#"{"gpt-4": {"numRequestsTotal": 350, "maxRequestUsage": 500}}"#)
                .unwrap();
        let snap = parse_summary(&summary, Some(&requests), None);
        assert_eq!(snap.plan.as_deref(), Some("Legacy"));
        assert!((bucket(&snap, "models").used_percent - 70.0).abs() < 0.01);
    }

    /// On-demand spend is billing state, not a subscription quota lane.
    #[test]
    fn on_demand_never_becomes_a_bucket() {
        let snap = snapshot(
            r#"{"membershipType": "pro",
                "individualUsage": {"plan": {"used": 1000, "limit": 2000, "totalPercentUsed": 50.0},
                                    "onDemand": {"used": 730, "limit": 2000}}}"#,
        );
        let ids: Vec<&str> = snap.buckets.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, ["models"]);
        assert!((bucket(&snap, "models").used_percent - 50.0).abs() < 0.01);
    }

    #[test]
    fn grok_bot_weekly_uses_its_own_reset_window() {
        let summary = decode_summary(
            br#"{"membershipType": "ultra",
                 "billingCycleStart": "2026-08-12T05:36:22.000Z",
                 "billingCycleEnd": "2026-09-12T05:36:22.000Z",
                 "individualUsage": {"plan": {"autoPercentUsed": 1, "apiPercentUsed": 2}}}"#,
        )
        .unwrap();
        let bot: GrokBotUsage = serde_json::from_str(
            r#"{"currentPeriodStart": "2026-08-12T05:39:26.906Z",
                "nextResetTimestampUtc": "2026-08-19T05:39:26.906Z",
                "usagePercent": 5.361195, "hasAvailableUsage": true,
                "hasNonZeroIncludedLimit": true}"#,
        )
        .unwrap();
        let snap = parse_summary(&summary, None, Some(&bot));
        assert_eq!(snap.plan.as_deref(), Some("Ultra"));
        let weekly = bucket(&snap, "grok_bot_weekly");
        assert_eq!(weekly.title, "Weekly");
        assert_eq!(weekly.group_title.as_deref(), Some("Grok Bot"));
        assert!((weekly.used_percent - 5.361195).abs() < 0.000_001);
        assert_eq!(weekly.raw_window_seconds, Some(604_800));
        // Either spelling of "no Bot allowance" means no lane to show.
        for none in [
            GrokBotUsage {
                has_non_zero_included_limit: Some(false),
                ..GrokBotUsage {
                    usage_percent: bot.usage_percent,
                    ..Default::default()
                }
            },
            GrokBotUsage {
                included_limit_zero: Some(true),
                usage_percent: bot.usage_percent,
                ..Default::default()
            },
        ] {
            let snap = parse_summary(&summary, None, Some(&none));
            assert!(!snap.buckets.iter().any(|b| b.id == "grok_bot_weekly"));
        }
    }

    /// An unknown or missing plan is `None`, so the card suppresses its
    /// badge instead of showing "Nil".
    /// A summary with no usage anywhere is nothing to report, not 0%: the
    /// adapter turns the empty bucket list into a parse failure rather than
    /// letting a fabricated zero replace the last good observation.
    #[test]
    fn a_summary_with_no_usage_produces_no_bucket() {
        assert!(snapshot(r#"{ "individualUsage": {} }"#).buckets.is_empty());
        assert!(snapshot(r#"{ "membershipType": "pro" }"#)
            .buckets
            .is_empty());
        // A legacy entry with no maximum is no evidence either — and neither
        // is one with a maximum but no count.
        let summary = decode_summary(br#"{ "individualUsage": {} }"#).unwrap();
        for raw in [
            r#"{"gpt-4": {"numRequestsTotal": 350}}"#,
            r#"{"gpt-4": {"maxRequestUsage": 500}}"#,
        ] {
            let requests: RequestUsage = serde_json::from_str(raw).unwrap();
            assert!(
                parse_summary(&summary, Some(&requests), None)
                    .buckets
                    .is_empty(),
                "{raw}"
            );
        }
        // The Bot add-on alone is not the account's own quota.
        let bot: GrokBotUsage = serde_json::from_str(
            r#"{"usagePercent": 5.0, "nextResetTimestampUtc": "2026-09-10T00:00:00Z"}"#,
        )
        .unwrap();
        let only_bot = parse_summary(&summary, None, Some(&bot));
        assert_eq!(only_bot.buckets.len(), 1);
        assert!(!only_bot.has_primary_lane());
    }

    #[test]
    fn an_unknown_membership_has_no_plan_name() {
        assert_eq!(snapshot(r#"{ "individualUsage": {} }"#).plan, None);
        assert_eq!(snapshot(r#"{ "membershipType": "  " }"#).plan, None);
        assert_eq!(
            snapshot(r#"{ "membershipType": "free_trial" }"#)
                .plan
                .as_deref(),
            Some("Free Trial")
        );
    }

    #[test]
    fn a_broken_summary_is_a_parse_failure() {
        assert!(matches!(
            decode_summary(b"<html>"),
            Err(QuotaError::ParseFailure(_))
        ));
    }
}
