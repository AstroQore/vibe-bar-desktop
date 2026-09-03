//! Google AntiGravity quota, read from the language server running on this
//! machine — the native `AntigravityQuotaAdapter`.
//!
//! There is no remote credential. The IDE runs a local language server whose
//! command line carries a CSRF token; [`probe`] finds the process and its
//! listening ports, and this module asks it two ConnectRPC questions over
//! loopback:
//!
//! - `RetrieveUserQuotaSummary`, the four real lanes (Gemini and
//!   Claude-plus-GPT, each 5-hour and weekly).
//! - `GetUserStatus`, for the account identity and, when a weekly pool is
//!   exhausted and the summary drops a group, a conservative 5-hour fallback.
//!
//! A lane whose remaining fraction is missing or unusable is omitted rather
//! than reported as exhausted: not knowing is not the same as being out.

pub mod probe;

use std::collections::BTreeMap;

use crate::error::QuotaError;
use crate::model::{AccountQuota, QuotaBucket, QuotaOrigin, ToolType};

const ACCOUNT_ID: &str = "local-antigravity";
const QUOTA_SUMMARY_PATH: &str =
    "/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary";
const USER_STATUS_PATH: &str = "/exa.language_server_pb.LanguageServerService/GetUserStatus";

pub async fn fetch() -> Result<AccountQuota, QuotaError> {
    let client = probe::LocalClient::new()?;
    let endpoints = probe::connected_endpoints(&client.timeout()).await?;

    let mut best: Option<Snapshot> = None;
    let mut last_error = None;
    for endpoint in &endpoints {
        match snapshot_from(&client, endpoint).await {
            // A complete answer ends the walk: no further endpoint could add
            // a lane to it.
            Ok(snapshot) if snapshot.is_complete() => {
                return Ok(quota(snapshot));
            }
            Ok(snapshot) => {
                if best
                    .as_ref()
                    .is_none_or(|current| snapshot.buckets.len() > current.buckets.len())
                {
                    best = Some(snapshot);
                }
            }
            Err(error) => last_error = Some(error),
        }
    }
    match best {
        Some(snapshot) => Ok(quota(snapshot)),
        None => Err(last_error.unwrap_or(QuotaError::NoCredential)),
    }
}

fn quota(snapshot: Snapshot) -> AccountQuota {
    AccountQuota {
        account_id: ACCOUNT_ID.to_string(),
        tool: ToolType::Antigravity,
        buckets: snapshot.buckets,
        plan: snapshot.plan,
        queried_at: super::now_unix(),
        origin: QuotaOrigin::Live,
        error: None,
    }
}

/// Ask one endpoint both questions. The quota summary is authoritative; user
/// status only fills gaps, and either one failing is survivable as long as
/// the other answered.
async fn snapshot_from(
    client: &probe::LocalClient,
    endpoint: &probe::Endpoint,
) -> Result<Snapshot, QuotaError> {
    let summary = match client
        .post(endpoint, QUOTA_SUMMARY_PATH, br#"{"forceRefresh":true}"#)
        .await
    {
        Ok(body) => parse_quota_summary(&body),
        Err(error) => Err(error),
    };
    let status = client
        .post(endpoint, USER_STATUS_PATH, b"{}")
        .await
        .and_then(|body| parse_user_status(&body));

    match (summary, status) {
        (Ok(summary), status) => Ok(summary.merging(status.ok())),
        (Err(_), Ok(status)) if !status.buckets.is_empty() => Ok(status),
        (Err(summary_error), _) => Err(summary_error),
    }
}

// MARK: - Lanes

/// The two quota families AntiGravity reports, and the only place their
/// display names are spelled out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Group {
    Gemini,
    ClaudeGpt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Cadence {
    FiveHour,
    Weekly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Slot {
    group: Group,
    cadence: Cadence,
}

impl Slot {
    /// The order the four lanes appear in, everywhere.
    const DISPLAY_ORDER: [Slot; 4] = [
        Slot {
            group: Group::Gemini,
            cadence: Cadence::FiveHour,
        },
        Slot {
            group: Group::Gemini,
            cadence: Cadence::Weekly,
        },
        Slot {
            group: Group::ClaudeGpt,
            cadence: Cadence::FiveHour,
        },
        Slot {
            group: Group::ClaudeGpt,
            cadence: Cadence::Weekly,
        },
    ];

    fn id(self) -> &'static str {
        match (self.group, self.cadence) {
            (Group::Gemini, Cadence::FiveHour) => "gemini_five_hour",
            (Group::Gemini, Cadence::Weekly) => "gemini_weekly",
            (Group::ClaudeGpt, Cadence::FiveHour) => "claude_gpt_five_hour",
            (Group::ClaudeGpt, Cadence::Weekly) => "claude_gpt_weekly",
        }
    }

    fn short_label(self) -> &'static str {
        match (self.group, self.cadence) {
            (Group::Gemini, Cadence::FiveHour) => "Gemini 5 Hours",
            (Group::Gemini, Cadence::Weekly) => "Gemini Weekly",
            (Group::ClaudeGpt, Cadence::FiveHour) => "Claude + GPT 5 Hours",
            (Group::ClaudeGpt, Cadence::Weekly) => "Claude + GPT Weekly",
        }
    }

    fn group_title(self) -> &'static str {
        match self.group {
            Group::Gemini => "Gemini Models",
            Group::ClaudeGpt => "Claude and GPT Models",
        }
    }

    fn bucket(self, remaining_fraction: f64, reset_at: Option<f64>) -> QuotaBucket {
        let remaining = remaining_fraction.clamp(0.0, 1.0);
        QuotaBucket::new(
            self.id(),
            if self.cadence == Cadence::FiveHour {
                "5 Hours"
            } else {
                "Weekly"
            },
            self.short_label(),
            (1.0 - remaining) * 100.0,
            reset_at,
            Some(if self.cadence == Cadence::FiveHour {
                18_000
            } else {
                604_800
            }),
            Some(self.group_title().to_string()),
        )
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Snapshot {
    pub buckets: Vec<QuotaBucket>,
    pub plan: Option<String>,
    pub email: Option<String>,
    /// `model id → label` for every config the server described, whether or
    /// not it carried quota. The cost scanner will need these to turn a
    /// placeholder id into a real model name.
    pub model_labels: BTreeMap<String, String>,
}

impl Snapshot {
    fn is_complete(&self) -> bool {
        self.buckets.len() == Slot::DISPLAY_ORDER.len()
    }

    /// The one merge policy for this provider: the receiver wins every field
    /// it has, and `other` only fills gaps — lanes the receiver is missing
    /// and identity it left empty. The result is always in display order.
    fn merging(mut self, other: Option<Snapshot>) -> Snapshot {
        let Some(other) = other else { return self };
        let known: Vec<String> = self.buckets.iter().map(|b| b.id.clone()).collect();
        self.buckets
            .extend(other.buckets.into_iter().filter(|b| !known.contains(&b.id)));
        let mut ordered = Vec::new();
        for slot in Slot::DISPLAY_ORDER {
            if let Some(position) = self.buckets.iter().position(|b| b.id == slot.id()) {
                ordered.push(self.buckets.remove(position));
            }
        }
        let mut model_labels = other.model_labels;
        // Labels are additive; both sources describe the same catalog, and
        // the receiver's spelling wins where they disagree.
        model_labels.extend(self.model_labels);
        Snapshot {
            buckets: ordered,
            plan: self.plan.or(other.plan),
            email: self.email.or(other.email),
            model_labels,
        }
    }
}

// MARK: - Parsing

/// AntiGravity 2.x's grouped summary, folded into the four stable lanes. An
/// unknown group or cadence, and a bucket with no usable remaining fraction,
/// are skipped rather than reported as exhausted.
pub fn parse_quota_summary(body: &[u8]) -> Result<Snapshot, QuotaError> {
    let root: serde_json::Value = serde_json::from_slice(body)
        .map_err(|error| QuotaError::ParseFailure(format!("Antigravity quota summary: {error}")))?;
    check_code(&root)?;

    let groups = ["response", "summary"]
        .iter()
        .find_map(|key| root.get(*key).and_then(|v| v.get("groups")))
        .or_else(|| root.get("groups"))
        .and_then(|v| v.as_array())
        .filter(|groups| !groups.is_empty())
        .ok_or_else(|| {
            QuotaError::ParseFailure("Antigravity quota summary had no groups".into())
        })?;

    let mut by_slot: BTreeMap<Slot, QuotaBucket> = BTreeMap::new();
    for group in groups {
        let group_name = group.get("displayName").and_then(|v| v.as_str());
        for payload in group
            .get("buckets")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            if payload.get("disabled").and_then(|v| v.as_bool()) == Some(true) {
                continue;
            }
            let bucket_id = payload.get("bucketId").and_then(|v| v.as_str());
            let Some(group_kind) = group_kind(group_name, bucket_id) else {
                continue;
            };
            let Some(cadence) = cadence(payload) else {
                continue;
            };
            let Some(remaining) = remaining_fraction(payload).filter(|f| f.is_finite()) else {
                continue;
            };
            let slot = Slot {
                group: group_kind,
                cadence,
            };
            let candidate = slot.bucket(
                remaining,
                payload
                    .get("resetTime")
                    .and_then(|v| v.as_str())
                    .and_then(parse_timestamp),
            );
            // Where two rows describe one lane, the more consumed one is the
            // safer thing to show.
            match by_slot.get(&slot) {
                Some(current) if current.used_percent >= candidate.used_percent => {}
                _ => {
                    by_slot.insert(slot, candidate);
                }
            }
        }
    }

    let buckets: Vec<QuotaBucket> = Slot::DISPLAY_ORDER
        .iter()
        .filter_map(|slot| by_slot.remove(slot))
        .collect();
    if buckets.is_empty() {
        return Err(QuotaError::ParseFailure(
            "Antigravity quota summary had no usable 5-hour or weekly buckets".into(),
        ));
    }
    Ok(Snapshot {
        buckets,
        ..Default::default()
    })
}

/// `GetUserStatus`: identity, the plan badge, the model label catalog, and a
/// conservative per-group 5-hour lane for when the summary dropped one.
pub fn parse_user_status(body: &[u8]) -> Result<Snapshot, QuotaError> {
    let root: serde_json::Value = serde_json::from_slice(body)
        .map_err(|error| QuotaError::ParseFailure(format!("Antigravity response: {error}")))?;
    check_code(&root)?;
    let status = root.get("userStatus").ok_or_else(|| {
        QuotaError::ParseFailure("Antigravity response had no userStatus envelope".into())
    })?;

    let mut model_labels = BTreeMap::new();
    let mut fallback: BTreeMap<Group, QuotaBucket> = BTreeMap::new();
    for config in status
        .get("cascadeModelConfigData")
        .and_then(|v| v.get("clientModelConfigs"))
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
    {
        let label = config.get("label").and_then(|v| v.as_str());
        let model = config
            .get("modelOrAlias")
            .and_then(|v| v.get("model"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|model| !model.is_empty());
        if let (Some(model), Some(label)) = (model, label) {
            model_labels.insert(model.to_string(), label.to_string());
        }
        let quota_info = config.get("quotaInfo");
        let Some(remaining) = quota_info
            .and_then(|v| v.get("remainingFraction"))
            .and_then(|v| v.as_f64())
            .filter(|f| f.is_finite())
        else {
            continue;
        };
        let Some(group) = group_kind(label, model) else {
            continue;
        };
        let candidate = Slot {
            group,
            cadence: Cadence::FiveHour,
        }
        .bucket(
            remaining,
            quota_info
                .and_then(|v| v.get("resetTime"))
                .and_then(|v| v.as_str())
                .and_then(parse_timestamp),
        );
        match fallback.get(&group) {
            Some(current) if current.used_percent >= candidate.used_percent => {}
            _ => {
                fallback.insert(group, candidate);
            }
        }
    }

    let plan = status
        .get("userTier")
        .and_then(|tier| non_empty(tier.get("name")))
        .or_else(|| {
            let info = status.get("planStatus")?.get("planInfo")?;
            [
                "planDisplayName",
                "displayName",
                "productName",
                "planName",
                "planShortName",
            ]
            .iter()
            .find_map(|key| non_empty(info.get(*key)))
        });

    let buckets = Slot::DISPLAY_ORDER
        .iter()
        .filter(|slot| slot.cadence == Cadence::FiveHour)
        .filter_map(|slot| fallback.remove(&slot.group))
        .collect();
    Ok(Snapshot {
        buckets,
        plan,
        email: non_empty(status.get("email")),
        model_labels,
    })
}

/// AntiGravity's `code` is a number in some builds and a string in others,
/// and anything that is not zero or "ok" is the server refusing.
fn check_code(root: &serde_json::Value) -> Result<(), QuotaError> {
    let Some(code) = root.get("code") else {
        return Ok(());
    };
    let raw = match code {
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::String(text) => text.clone(),
        _ => return Ok(()),
    };
    if raw == "0" || raw.eq_ignore_ascii_case("ok") {
        return Ok(());
    }
    let message = non_empty(root.get("message")).unwrap_or(raw);
    Err(QuotaError::Network(format!("Antigravity: {message}")))
}

fn non_empty(value: Option<&serde_json::Value>) -> Option<String> {
    let text = value?.as_str()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn group_kind(group_name: Option<&str>, bucket_id: Option<&str>) -> Option<Group> {
    let value = format!(
        "{} {}",
        group_name.unwrap_or_default(),
        bucket_id.unwrap_or_default()
    )
    .to_lowercase();
    if value.contains("gemini") {
        return Some(Group::Gemini);
    }
    if value.contains("claude") || value.contains("gpt") || value.contains("3p-") {
        return Some(Group::ClaudeGpt);
    }
    None
}

/// The cadence is spelled several ways across builds, and sometimes only in
/// the bucket id's suffix.
fn cadence(payload: &serde_json::Value) -> Option<Cadence> {
    const SESSION_ALIASES: [&str; 5] = ["session", "5h", "5-hour", "five hour", "five-hour"];
    let mut candidates: Vec<String> = Vec::new();
    for key in ["bucketId", "displayName", "window"] {
        let Some(raw) = payload.get(key).and_then(|v| v.as_str()) else {
            continue;
        };
        let normalized = raw.trim().to_lowercase().replace('_', "-");
        if normalized.is_empty() {
            continue;
        }
        if let Some(stripped) = normalized.strip_suffix(" limit") {
            candidates.push(stripped.to_string());
        }
        candidates.push(normalized);
    }
    let mut expanded = candidates.clone();
    for candidate in &candidates {
        for alias in SESSION_ALIASES.iter().chain(std::iter::once(&"weekly")) {
            if candidate.ends_with(&format!("-{alias}")) {
                expanded.push((*alias).to_string());
            }
        }
    }
    if expanded
        .iter()
        .any(|candidate| SESSION_ALIASES.contains(&candidate.as_str()))
    {
        return Some(Cadence::FiveHour);
    }
    expanded
        .iter()
        .any(|candidate| candidate == "weekly")
        .then_some(Cadence::Weekly)
}

fn remaining_fraction(payload: &serde_json::Value) -> Option<f64> {
    if let Some(value) = payload.get("remainingFraction").and_then(|v| v.as_f64()) {
        return Some(value);
    }
    let remaining = payload.get("remaining")?;
    if let Some(value) = remaining.get("remainingFraction").and_then(|v| v.as_f64()) {
        return Some(value);
    }
    // The protobuf-JSON oneof shape: `{"case": "remainingFraction", "value": …}`.
    if remaining.get("case").and_then(|v| v.as_str()) == Some("remainingFraction") {
        return remaining.get("value").and_then(|v| v.as_f64());
    }
    None
}

/// RFC 3339 with or without fractional seconds, or bare Unix seconds.
fn parse_timestamp(raw: &str) -> Option<f64> {
    let raw = raw.trim();
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Some(parsed.timestamp() as f64 + f64::from(parsed.timestamp_subsec_nanos()) / 1e9);
    }
    raw.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUMMARY: &str = r#"{
      "code": "OK",
      "response": {"groups": [
        {"displayName": "Gemini Models", "buckets": [
          {"bucketId": "gemini-session", "remainingFraction": 0.8,
           "resetTime": "2026-09-03T18:00:00Z"},
          {"bucketId": "gemini-weekly", "remaining": {"remainingFraction": 0.55},
           "resetTime": "1788220800"}
        ]},
        {"displayName": "Claude and GPT Models", "buckets": [
          {"bucketId": "3p-session", "displayName": "5-Hour Limit",
           "remaining": {"case": "remainingFraction", "value": 0.25}},
          {"bucketId": "3p-weekly", "window": "WEEKLY", "remainingFraction": 0.9},
          {"bucketId": "3p-weekly", "window": "WEEKLY", "remainingFraction": 0.4},
          {"bucketId": "3p-daily", "remainingFraction": 0.5},
          {"bucketId": "3p-weekly", "window": "WEEKLY", "remainingFraction": 0.99,
           "disabled": true}
        ]}
      ]}
    }"#;

    #[test]
    fn the_four_lanes_come_out_in_display_order() {
        let snapshot = parse_quota_summary(SUMMARY.as_bytes()).unwrap();
        let ids: Vec<&str> = snapshot.buckets.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "gemini_five_hour",
                "gemini_weekly",
                "claude_gpt_five_hour",
                "claude_gpt_weekly"
            ]
        );
        assert!(snapshot.is_complete());
        let gemini = &snapshot.buckets[0];
        assert_eq!(gemini.title, "5 Hours");
        assert_eq!(gemini.group_title.as_deref(), Some("Gemini Models"));
        assert!((gemini.used_percent - 20.0).abs() < 0.001);
        assert_eq!(gemini.raw_window_seconds, Some(18_000));
        assert_eq!(gemini.reset_at, Some(1_788_458_400.0));
        // A bare Unix-seconds reset parses too.
        assert_eq!(snapshot.buckets[1].reset_at, Some(1_788_220_800.0));
        // The oneof shape, and the weekly window spelled in `window`.
        assert!((snapshot.buckets[2].used_percent - 75.0).abs() < 0.001);
        assert_eq!(snapshot.buckets[3].raw_window_seconds, Some(604_800));
        // Two rows for one lane: the more consumed one wins, and a disabled
        // row does not count at all.
        assert!((snapshot.buckets[3].used_percent - 60.0).abs() < 0.001);
    }

    #[test]
    fn unknown_groups_cadences_and_errors_are_refused_rather_than_guessed() {
        // A daily bucket in a known group is not one of the four lanes.
        let only_daily = r#"{"groups":[{"displayName":"Gemini","buckets":[
            {"bucketId":"gemini-daily","remainingFraction":0.5}]}]}"#;
        assert!(matches!(
            parse_quota_summary(only_daily.as_bytes()),
            Err(QuotaError::ParseFailure(_))
        ));
        // An unknown group, likewise.
        let unknown_group = r#"{"groups":[{"displayName":"Mystery","buckets":[
            {"bucketId":"mystery-weekly","remainingFraction":0.5}]}]}"#;
        assert!(matches!(
            parse_quota_summary(unknown_group.as_bytes()),
            Err(QuotaError::ParseFailure(_))
        ));
        // A bucket with no fraction is omitted, not reported as exhausted.
        let no_fraction = r#"{"groups":[{"displayName":"Gemini","buckets":[
            {"bucketId":"gemini-weekly"}]}]}"#;
        assert!(matches!(
            parse_quota_summary(no_fraction.as_bytes()),
            Err(QuotaError::ParseFailure(_))
        ));
        assert!(matches!(
            parse_quota_summary(br#"{"groups":[]}"#),
            Err(QuotaError::ParseFailure(_))
        ));
        // A server-side error is that error, not a parse failure.
        match parse_quota_summary(br#"{"code": 7, "message": "signed out"}"#) {
            Err(QuotaError::Network(message)) => assert!(message.contains("signed out")),
            other => panic!("{other:?}"),
        }
        match parse_quota_summary(br#"{"code": "PERMISSION_DENIED"}"#) {
            Err(QuotaError::Network(message)) => assert!(message.contains("PERMISSION_DENIED")),
            other => panic!("{other:?}"),
        }
    }

    const STATUS: &str = r#"{
      "code": 0,
      "userStatus": {
        "email": " person@example.com ",
        "userTier": {"id": "t2", "name": "Google AI Ultra"},
        "planStatus": {"planInfo": {"planName": "ultra"}},
        "cascadeModelConfigData": {"clientModelConfigs": [
          {"label": "Gemini 3.5 Flash (High)", "modelOrAlias": {"model": "MODEL_PLACEHOLDER_M132"},
           "quotaInfo": {"remainingFraction": 0.6, "resetTime": "2026-09-03T18:00:00Z"}},
          {"label": "Gemini 3.5 Pro", "modelOrAlias": {"model": "MODEL_PLACEHOLDER_M99"},
           "quotaInfo": {"remainingFraction": 0.3}},
          {"label": "Claude Sonnet 5", "modelOrAlias": {"model": "MODEL_PLACEHOLDER_C7"}},
          {"label": "Unlabelled thing", "modelOrAlias": {"model": "  "}}
        ]}
      }
    }"#;

    #[test]
    fn user_status_gives_identity_labels_and_a_conservative_five_hour_lane() {
        let snapshot = parse_user_status(STATUS.as_bytes()).unwrap();
        assert_eq!(snapshot.email.as_deref(), Some("person@example.com"));
        assert_eq!(snapshot.plan.as_deref(), Some("Google AI Ultra"));
        assert_eq!(
            snapshot.model_labels.get("MODEL_PLACEHOLDER_M132").map(String::as_str),
            Some("Gemini 3.5 Flash (High)")
        );
        assert_eq!(snapshot.model_labels.len(), 3);
        // Only the 5-hour lane, and the more consumed of the two Gemini
        // configs. Claude has no quota info, so it contributes no lane.
        let ids: Vec<&str> = snapshot.buckets.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, ["gemini_five_hour"]);
        assert!((snapshot.buckets[0].used_percent - 70.0).abs() < 0.001);
    }

    #[test]
    fn the_summary_wins_and_status_only_fills_gaps() {
        let partial = r#"{"groups":[{"displayName":"Gemini Models","buckets":[
            {"bucketId":"gemini-weekly","remainingFraction":0.5}]}]}"#;
        let merged = parse_quota_summary(partial.as_bytes())
            .unwrap()
            .merging(Some(parse_user_status(STATUS.as_bytes()).unwrap()));
        // Display order, with the status lane slotted in ahead of the weekly.
        let ids: Vec<&str> = merged.buckets.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, ["gemini_five_hour", "gemini_weekly"]);
        assert_eq!(merged.plan.as_deref(), Some("Google AI Ultra"));
        assert_eq!(merged.email.as_deref(), Some("person@example.com"));
        assert!(!merged.model_labels.is_empty());
        assert!(!merged.is_complete());

        // Where both describe a lane, the summary's number is the one kept.
        let full = parse_quota_summary(SUMMARY.as_bytes()).unwrap();
        let gemini_five_hour = full.buckets[0].used_percent;
        let merged = full.merging(Some(parse_user_status(STATUS.as_bytes()).unwrap()));
        assert!((merged.buckets[0].used_percent - gemini_five_hour).abs() < 0.001);
        assert_eq!(merged.buckets.len(), 4);
    }

    #[test]
    fn a_status_response_without_its_envelope_is_a_parse_failure() {
        assert!(matches!(
            parse_user_status(br#"{"code": 0}"#),
            Err(QuotaError::ParseFailure(_))
        ));
        assert!(matches!(
            parse_user_status(b"<html>"),
            Err(QuotaError::ParseFailure(_))
        ));
    }
}
