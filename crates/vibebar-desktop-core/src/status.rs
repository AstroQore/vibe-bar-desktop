//! Public, credential-free service status for the first cross-platform slice.
//!
//! The engine seeds from native's shared cache, then refreshes the public
//! Claude, Google AI, and Cursor feeds. It never writes `service_status.json`.

use std::sync::RwLock;
use std::time::Duration;

use chrono::{DateTime, Datelike, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::{ToolType, CLOCK_SKEW_TOLERANCE_SECONDS};
use crate::paths::DataRoot;
use crate::shared::service_status;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const REFRESH_TIMEOUT: Duration = Duration::from_secs(30);
const GOOGLE_REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_GOOGLE_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_STATUS_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const GOOGLE_GEMINI_PRODUCT_ID: &str = "npdyhgECDJ6tB66MxXyo";
const GOOGLE_INCIDENTS_URL: &str = "https://www.google.com/appsstatus/dashboard/incidents.json";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatusView {
    pub providers: Vec<ProviderStatus>,
    pub updated_at: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub tool: ToolType,
    pub indicator: String,
    pub description: String,
    pub updated_at: Option<f64>,
    pub incidents: Vec<StatusIncident>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatusIncident {
    pub id: String,
    pub name: String,
    pub status: String,
    pub impact: String,
    pub created_at: Option<f64>,
    pub updated_at: Option<f64>,
}

#[derive(Debug, Error)]
pub enum ServiceStatusError {
    #[error("service status network failure: {0}")]
    Network(String),
    #[error("service status response was not usable: {0}")]
    Parse(String),
}

pub struct ServiceStatusEngine {
    client: reqwest::Client,
    cached: RwLock<ServiceStatusView>,
    refresh_gate: tokio::sync::Mutex<()>,
    is_demo: bool,
}

impl ServiceStatusEngine {
    pub fn new(root: DataRoot) -> Self {
        Self {
            client: public_client(),
            cached: RwLock::new(seed(&root)),
            refresh_gate: tokio::sync::Mutex::new(()),
            is_demo: root.is_demo(),
        }
    }

    /// Synchronous, in-memory snapshot. No disk read and no network call.
    pub fn cached(&self) -> ServiceStatusView {
        self.cached
            .read()
            .map(|value| value.clone())
            .unwrap_or_else(|_| ServiceStatusView {
                providers: Vec::new(),
                updated_at: None,
            })
    }

    /// All-or-nothing public refresh. A failed provider leaves the last good
    /// memory snapshot entirely intact rather than mixing fresh and stale rows.
    pub async fn refresh(&self) -> Result<ServiceStatusView, ServiceStatusError> {
        if self.is_demo {
            return Ok(self.cached());
        }
        let _refresh_guard = self.refresh_gate.lock().await;
        let fetch = async {
            tokio::try_join!(
                fetch_source(&self.client, SOURCES[0]),
                fetch_source(&self.client, SOURCES[1]),
                fetch_source(&self.client, SOURCES[2]),
                fetch_google_gemini(&self.client, now_unix())
            )
        };
        let (claude, cursor, codex, gemini) =
            tokio::time::timeout(REFRESH_TIMEOUT, fetch)
                .await
                .map_err(|_| ServiceStatusError::Network("refresh timed out".into()))??;
        let mut providers = vec![claude, cursor, codex, gemini];
        providers.sort_by_key(|status| status.tool.raw_value());
        let updated_at = providers
            .iter()
            .filter_map(|status| status.updated_at)
            .max_by(f64::total_cmp);
        let next = ServiceStatusView {
            providers,
            updated_at,
        };
        if let Ok(mut cached) = self.cached.write() {
            *cached = next.clone();
        }
        Ok(next)
    }
}

#[derive(Clone, Copy)]
struct Source {
    tool: ToolType,
    base: &'static str,
}

const SOURCES: [Source; 3] = [
    Source {
        tool: ToolType::Claude,
        base: "https://status.claude.com",
    },
    Source {
        tool: ToolType::Cursor,
        base: "https://status.cursor.com",
    },
    Source {
        tool: ToolType::Codex,
        base: "https://status.openai.com",
    },
];

fn public_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT.max(GOOGLE_REQUEST_TIMEOUT))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap_or_default()
}

fn now_unix() -> f64 {
    Utc::now().timestamp_millis() as f64 / 1_000.0
}

fn seed(root: &DataRoot) -> ServiceStatusView {
    let cached = service_status::load(root);
    let now = now_unix();
    let plausible = |snapshot: &&service_status::ServiceStatusSnapshot| {
        snapshot
            .updated_at
            .is_none_or(|updated_at| updated_at <= now + CLOCK_SKEW_TOLERANCE_SECONDS)
    };
    let mut providers = [ToolType::Claude, ToolType::Cursor, ToolType::Codex]
        .into_iter()
        .filter_map(|tool| {
            cached
                .get(&tool)
                .filter(plausible)
                .map(|snapshot| ProviderStatus {
                    tool,
                    indicator: normalize_indicator(snapshot.indicator.as_deref())
                        .unwrap_or("unknown")
                        .to_string(),
                    description: snapshot
                        .description
                        .clone()
                        .unwrap_or_else(|| "Status unavailable".to_string()),
                    updated_at: snapshot.updated_at,
                    incidents: Vec::new(),
                })
        })
        .collect::<Vec<_>>();
    let google = [ToolType::Gemini, ToolType::Antigravity]
        .into_iter()
        .filter_map(|tool| cached.get(&tool))
        .filter(plausible)
        .max_by(|left, right| {
            let left_indicator =
                normalize_indicator(left.indicator.as_deref()).unwrap_or("unknown");
            let right_indicator =
                normalize_indicator(right.indicator.as_deref()).unwrap_or("unknown");
            impact_severity(left_indicator)
                .cmp(&impact_severity(right_indicator))
                .then_with(|| {
                    left.updated_at
                        .unwrap_or(f64::NEG_INFINITY)
                        .total_cmp(&right.updated_at.unwrap_or(f64::NEG_INFINITY))
                })
        });
    if let Some(snapshot) = google {
        providers.push(ProviderStatus {
            tool: ToolType::Gemini,
            indicator: normalize_indicator(snapshot.indicator.as_deref())
                .unwrap_or("unknown")
                .to_string(),
            description: snapshot
                .description
                .clone()
                .unwrap_or_else(|| "Status unavailable".to_string()),
            updated_at: snapshot.updated_at,
            incidents: Vec::new(),
        });
    }
    providers.sort_by_key(|status| status.tool.raw_value());
    let updated_at = providers
        .iter()
        .filter_map(|status| status.updated_at)
        .max_by(f64::total_cmp);
    ServiceStatusView {
        providers,
        updated_at,
    }
}

async fn fetch_source(
    client: &reqwest::Client,
    source: Source,
) -> Result<ProviderStatus, ServiceStatusError> {
    let summary_url = format!("{}/api/v2/summary.json", source.base);
    let incidents_url = format!("{}/api/v2/incidents.json", source.base);
    let (summary, incidents): (Summary, Incidents) = tokio::try_join!(
        get_json(client, &summary_url),
        get_json(client, &incidents_url)
    )?;
    let indicator = normalize_indicator(Some(&summary.status.indicator))
        .ok_or_else(|| ServiceStatusError::Parse("unknown status indicator".into()))?
        .to_string();
    let mut incidents = incidents
        .incidents
        .into_iter()
        .map(StatusIncident::from)
        .filter(is_active_incident)
        .collect::<Vec<_>>();
    incidents.sort_by(|left, right| incident_time(right).total_cmp(&incident_time(left)));
    incidents.truncate(4);
    Ok(ProviderStatus {
        tool: source.tool,
        indicator,
        description: summary.status.description,
        updated_at: parse_time(Some(&summary.page.updated_at)),
        incidents,
    })
}

async fn get_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
) -> Result<T, ServiceStatusError> {
    let response = client
        .get(url)
        .timeout(Duration::from_secs(8))
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|error| ServiceStatusError::Network(error.to_string()))?;
    if !response.status().is_success() {
        return Err(ServiceStatusError::Network(format!(
            "HTTP {}",
            response.status()
        )));
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_STATUS_RESPONSE_BYTES as u64)
    {
        return Err(ServiceStatusError::Parse(
            "status response exceeded 4 MiB".into(),
        ));
    }
    let mut response = response;
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| ServiceStatusError::Network(error.to_string()))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_STATUS_RESPONSE_BYTES {
            return Err(ServiceStatusError::Parse(
                "status response exceeded 4 MiB".into(),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|error| ServiceStatusError::Parse(error.to_string()))
}

async fn fetch_google_gemini(
    client: &reqwest::Client,
    now: f64,
) -> Result<ProviderStatus, ServiceStatusError> {
    let response = client
        .get(GOOGLE_INCIDENTS_URL)
        .timeout(GOOGLE_REQUEST_TIMEOUT)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, "Vibe Bar/1")
        .send()
        .await
        .map_err(|error| ServiceStatusError::Network(error.to_string()))?;
    if !response.status().is_success() {
        return Err(ServiceStatusError::Network(format!(
            "Google Apps Status HTTP {}",
            response.status()
        )));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_GOOGLE_RESPONSE_BYTES as u64)
    {
        return Err(ServiceStatusError::Parse(
            "Google Apps Status response exceeded 4 MiB".into(),
        ));
    }

    let mut response = response;
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| ServiceStatusError::Network(error.to_string()))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_GOOGLE_RESPONSE_BYTES {
            return Err(ServiceStatusError::Parse(
                "Google Apps Status response exceeded 4 MiB".into(),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    google_provider_from_bytes(&bytes, now)
}

fn google_provider_from_bytes(
    bytes: &[u8],
    now: f64,
) -> Result<ProviderStatus, ServiceStatusError> {
    if bytes.len() > MAX_GOOGLE_RESPONSE_BYTES {
        return Err(ServiceStatusError::Parse(
            "Google Apps Status response exceeded 4 MiB".into(),
        ));
    }
    let incidents: Vec<GoogleAppsIncident> = serde_json::from_slice(bytes)
        .map_err(|error| ServiceStatusError::Parse(error.to_string()))?;
    Ok(google_provider_from_incidents(incidents, now))
}

fn google_provider_from_incidents(incidents: Vec<GoogleAppsIncident>, now: f64) -> ProviderStatus {
    let incidents = incidents
        .into_iter()
        .filter(|incident| {
            incident.service_key.as_deref() == Some(GOOGLE_GEMINI_PRODUCT_ID)
                || incident
                    .affected_products
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|product| product.id == GOOGLE_GEMINI_PRODUCT_ID)
        })
        .collect::<Vec<_>>();

    let current_impact = incidents
        .iter()
        .filter_map(|incident| {
            let begin = parse_time(Some(&incident.begin))?;
            let end = incident
                .end
                .as_deref()
                .and_then(|value| parse_time(Some(value)))
                .unwrap_or(now);
            google_is_current_day(begin, end, now).then(|| google_impact(incident))
        })
        .max_by_key(|impact| impact_severity(impact));
    let indicator = current_impact.unwrap_or("none");

    let mut recent = incidents
        .iter()
        .filter_map(|incident| {
            // This deliberately mirrors native's `created ?? begin`: a
            // present-but-malformed `created` is not silently rewritten.
            let created_at = parse_time(incident.created.as_deref().or(Some(&incident.begin)))?;
            let resolved_at = incident
                .end
                .as_deref()
                .and_then(|value| parse_time(Some(value)));
            let updated_at = incident
                .most_recent_update
                .as_ref()
                .and_then(|update| update.when.as_deref())
                .and_then(|value| parse_time(Some(value)))
                .or_else(|| {
                    parse_time(
                        incident
                            .modified
                            .as_deref()
                            .or(incident.created.as_deref())
                            .or(Some(&incident.begin)),
                    )
                });
            Some(StatusIncident {
                id: incident.id.clone(),
                name: incident
                    .external_desc
                    .as_deref()
                    .or(incident.service_name.as_deref())
                    .unwrap_or("Gemini incident")
                    .to_string(),
                status: if resolved_at.is_some() {
                    "resolved".to_string()
                } else {
                    incident
                        .most_recent_update
                        .as_ref()
                        .and_then(|update| update.status.as_deref())
                        .unwrap_or("investigating")
                        .to_string()
                },
                impact: google_impact(incident).to_string(),
                created_at: Some(created_at),
                updated_at,
            })
        })
        .collect::<Vec<_>>();
    recent.sort_by(|left, right| incident_time(right).total_cmp(&incident_time(left)));
    recent.truncate(4);

    let updated_at = incidents
        .iter()
        .filter_map(|incident| {
            incident
                .most_recent_update
                .as_ref()
                .and_then(|update| update.when.as_deref())
                .and_then(|value| parse_time(Some(value)))
                .or_else(|| {
                    parse_time(
                        incident
                            .modified
                            .as_deref()
                            .or(incident.created.as_deref())
                            .or(Some(&incident.begin)),
                    )
                })
        })
        .max_by(f64::total_cmp)
        .or(Some(now));

    ProviderStatus {
        tool: ToolType::Gemini,
        indicator: indicator.to_string(),
        description: indicator_description(indicator).to_string(),
        updated_at,
        incidents: recent,
    }
}

fn google_is_current_day(begin: f64, end: f64, now: f64) -> bool {
    let Some(now) = DateTime::<Utc>::from_timestamp_millis((now * 1_000.0).round() as i64) else {
        return false;
    };
    let Some(start) = Utc
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
    else {
        return false;
    };
    let end_of_day = start + chrono::Duration::days(1) - chrono::Duration::milliseconds(1);
    begin <= end_of_day.timestamp_millis() as f64 / 1_000.0
        && end >= start.timestamp_millis() as f64 / 1_000.0
}

fn google_impact(incident: &GoogleAppsIncident) -> &'static str {
    match incident
        .severity
        .as_deref()
        .map(|value| value.to_ascii_lowercase())
    {
        Some(severity) if severity == "critical" => "critical",
        Some(severity) if severity == "high" => "major",
        Some(severity) if severity == "medium" || severity == "low" => "minor",
        _ => {
            let impact = incident
                .status_impact
                .as_deref()
                .unwrap_or("")
                .to_ascii_uppercase();
            if impact.contains("DISRUPT") || impact.contains("OUTAGE") {
                "critical"
            } else if impact.contains("DEGRADED") || impact.contains("DELAY") {
                "minor"
            } else if impact.contains("MAINTENANCE") {
                "maintenance"
            } else {
                "minor"
            }
        }
    }
}

fn impact_severity(impact: &str) -> u8 {
    match impact {
        "critical" => 4,
        "major" => 3,
        "minor" => 2,
        "maintenance" => 1,
        _ => 0,
    }
}

fn indicator_description(indicator: &str) -> &'static str {
    match indicator {
        "none" => "All services operational",
        "maintenance" => "Under maintenance",
        "minor" => "Service issue",
        "major" => "Partial outage",
        "critical" => "Major outage",
        _ => "Status unavailable",
    }
}

fn normalize_indicator(value: Option<&str>) -> Option<&'static str> {
    match value.map(str::trim) {
        Some("none") => Some("none"),
        Some("minor") => Some("minor"),
        Some("major") => Some("major"),
        Some("critical") => Some("critical"),
        Some("maintenance") => Some("maintenance"),
        _ => None,
    }
}

fn parse_time(value: Option<&str>) -> Option<f64> {
    value
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.timestamp_millis() as f64 / 1_000.0)
}

fn incident_time(incident: &StatusIncident) -> f64 {
    incident.created_at.unwrap_or(f64::NEG_INFINITY)
}

fn is_active_incident(incident: &StatusIncident) -> bool {
    !matches!(
        incident.status.trim().to_ascii_lowercase().as_str(),
        "resolved" | "postmortem" | "completed"
    )
}

#[derive(Deserialize)]
struct Summary {
    page: Page,
    status: SummaryStatus,
}
#[derive(Deserialize)]
struct Page {
    updated_at: String,
}
#[derive(Deserialize)]
struct SummaryStatus {
    indicator: String,
    description: String,
}
#[derive(Deserialize)]
struct Incidents {
    #[serde(default)]
    incidents: Vec<Incident>,
}
#[derive(Deserialize)]
struct Incident {
    id: String,
    name: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    impact: String,
    created_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Deserialize)]
struct GoogleAppsIncident {
    id: String,
    begin: String,
    end: Option<String>,
    created: Option<String>,
    modified: Option<String>,
    external_desc: Option<String>,
    service_key: Option<String>,
    service_name: Option<String>,
    severity: Option<String>,
    status_impact: Option<String>,
    affected_products: Option<Vec<GoogleAppsProduct>>,
    most_recent_update: Option<GoogleAppsUpdate>,
}

#[derive(Deserialize)]
struct GoogleAppsProduct {
    id: String,
}

#[derive(Deserialize)]
struct GoogleAppsUpdate {
    status: Option<String>,
    when: Option<String>,
}

impl From<Incident> for StatusIncident {
    fn from(value: Incident) -> Self {
        Self {
            id: value.id,
            name: value.name,
            status: value.status,
            impact: value.impact,
            created_at: parse_time(value.created_at.as_deref()),
            updated_at: parse_time(value.updated_at.as_deref()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_current_status_and_sorts_four_newest_incidents() {
        let summary: Summary = serde_json::from_slice(br#"{"page":{"updated_at":"2026-08-17T08:00:00Z"},"status":{"indicator":"minor","description":"Elevated errors"}}"#).unwrap();
        assert_eq!(
            normalize_indicator(Some(&summary.status.indicator)),
            Some("minor")
        );
        assert_eq!(
            parse_time(Some(&summary.page.updated_at)),
            Some(1_786_953_600.0)
        );
        let incidents: Incidents = serde_json::from_slice(br#"{"incidents":[{"id":"resolved-newer","name":"Resolved","status":"resolved","impact":"minor","created_at":"2026-08-04T00:00:00Z"},{"id":"active","name":"Active","status":"investigating","impact":"major","created_at":"2026-08-03T00:00:00Z"}]}"#).unwrap();
        let mut values = incidents
            .incidents
            .into_iter()
            .map(StatusIncident::from)
            .filter(is_active_incident)
            .collect::<Vec<_>>();
        values.sort_by(|left, right| incident_time(right).total_cmp(&incident_time(left)));
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].id, "active");
    }

    #[test]
    fn seed_reads_shared_status_and_prefers_the_worse_google_ai_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        std::fs::create_dir_all(root.shared()).unwrap();
        std::fs::write(root.service_status_file(), r#"["claude",{"indicator":"none","description":"Operational","updatedAt":809731205},"cursor",{"indicator":"major","description":"Outage"},"gemini",{"indicator":"none","description":"Operational","updatedAt":809731300},"antigravity",{"indicator":"minor","description":"Shared Gemini issue","updatedAt":809731100}]"#).unwrap();
        let view = ServiceStatusEngine::new(root).cached();
        assert_eq!(view.providers.len(), 3);
        assert_eq!(view.providers[0].tool, ToolType::Claude);
        assert_eq!(view.providers[1].indicator, "major");
        assert_eq!(view.providers[2].tool, ToolType::Gemini);
        assert_eq!(view.providers[2].indicator, "minor");
    }

    #[test]
    fn seed_rejects_a_future_google_ai_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        std::fs::create_dir_all(root.shared()).unwrap();
        let future_apple_seconds = now_unix() - 978_307_200.0 + 86_400.0;
        std::fs::write(
            root.service_status_file(),
            serde_json::json!([
                "gemini",
                {"indicator": "none", "description": "Current"},
                "antigravity",
                {"indicator": "critical", "description": "Future", "updatedAt": future_apple_seconds}
            ])
            .to_string(),
        )
        .unwrap();
        let view = ServiceStatusEngine::new(root).cached();
        assert_eq!(view.providers.len(), 1);
        assert_eq!(view.providers[0].tool, ToolType::Gemini);
        assert_eq!(view.providers[0].indicator, "none");
    }

    #[test]
    fn unknown_indicator_fails_closed() {
        assert_eq!(normalize_indicator(Some("future")), None);
    }

    #[test]
    fn codex_uses_public_statuspage_source_and_standard_incident_shape() {
        let source = SOURCES
            .iter()
            .find(|source| source.tool == ToolType::Codex)
            .unwrap();
        assert_eq!(source.base, "https://status.openai.com");
        let summary: Summary = serde_json::from_slice(
            br#"{"page":{"updated_at":"2026-08-17T08:00:00Z"},"status":{"indicator":"minor","description":"Elevated errors"}}"#,
        ).unwrap();
        assert_eq!(summary.status.indicator, "minor");
        let incidents: Incidents = serde_json::from_slice(
            br#"{"incidents":[{"id":"new","name":"API","status":"investigating","impact":"major","created_at":"2026-08-17T01:00:00Z"},{"id":"old","name":"Old","status":"resolved","impact":"minor","created_at":"2026-08-16T01:00:00Z"}]}"#,
        ).unwrap();
        let active = incidents
            .incidents
            .into_iter()
            .map(StatusIncident::from)
            .filter(is_active_incident)
            .collect::<Vec<_>>();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "new");
    }

    #[test]
    fn google_filters_gemini_and_uses_current_utc_day_impact() {
        let now = 1_786_953_600.0; // 2026-08-17T08:00:00Z
        let payload = br#"[
          {"id":"ignore","begin":"2026-08-17T01:00:00Z","severity":"critical"},
          {"id":"affected","begin":"2026-08-17T01:00:00Z","end":"2026-08-17T02:00:00Z","created":"2026-08-17T01:00:00Z","external_desc":"Earlier today","severity":"high","affected_products":[{"id":"npdyhgECDJ6tB66MxXyo"}]},
          {"id":"open","begin":"2026-08-16T23:00:00Z","created":"2026-08-16T23:00:00Z","service_key":"npdyhgECDJ6tB66MxXyo","affected_products":null,"status_impact":"degraded performance","most_recent_update":{"status":"monitoring","when":"2026-08-17T07:00:00Z"}}
        ]"#;
        let view = google_provider_from_bytes(payload, now).unwrap();
        assert_eq!(view.tool, ToolType::Gemini);
        assert_eq!(view.indicator, "major");
        assert_eq!(view.description, "Partial outage");
        assert_eq!(view.incidents.len(), 2);
        assert_eq!(view.incidents[0].id, "affected");
        assert_eq!(view.incidents[0].status, "resolved");
        assert_eq!(view.incidents[1].impact, "minor");
        assert_eq!(view.updated_at, Some(1_786_950_000.0));
    }

    #[test]
    fn google_empty_and_bad_time_payloads_are_honest() {
        let now = 1_786_953_600.0;
        let empty = google_provider_from_bytes(br#"[]"#, now).unwrap();
        assert_eq!(empty.indicator, "none");
        assert_eq!(empty.description, "All services operational");
        assert_eq!(empty.updated_at, Some(now));

        let bad_time = br#"[{"id":"bad","begin":"not-a-time","service_key":"npdyhgECDJ6tB66MxXyo","severity":"critical"}]"#;
        let view = google_provider_from_bytes(bad_time, now).unwrap();
        assert_eq!(view.indicator, "none");
        assert!(view.incidents.is_empty());
        assert_eq!(view.updated_at, Some(now));
    }

    #[test]
    fn google_rejects_malformed_and_oversized_payloads() {
        assert!(google_provider_from_bytes(b"not-json", 1.0).is_err());
        assert!(
            google_provider_from_bytes(&vec![b'x'; MAX_GOOGLE_RESPONSE_BYTES + 1], 1.0).is_err()
        );
    }

    #[tokio::test]
    async fn demo_refresh_never_goes_to_the_network() {
        let dir = tempfile::tempdir().unwrap();
        let engine = ServiceStatusEngine::new(DataRoot::at(dir.path().join(".vibebar")));
        assert_eq!(engine.refresh().await.unwrap().providers, Vec::new());
    }
}
