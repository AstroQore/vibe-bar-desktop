//! Public, credential-free service status for the first cross-platform slice.
//!
//! The engine seeds from native's shared cache, but refreshes only the public
//! Statuspage v2 feeds for Claude and Cursor. It never writes `service_status.json`.

use std::sync::RwLock;
use std::time::Duration;

use chrono::DateTime;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::ToolType;
use crate::paths::DataRoot;
use crate::shared::service_status;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const REFRESH_TIMEOUT: Duration = Duration::from_secs(30);

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
                fetch_source(&self.client, SOURCES[1])
            )
        };
        let (claude, cursor) = tokio::time::timeout(REFRESH_TIMEOUT, fetch)
            .await
            .map_err(|_| ServiceStatusError::Network("refresh timed out".into()))??;
        let mut providers = vec![claude, cursor];
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

const SOURCES: [Source; 2] = [
    Source {
        tool: ToolType::Claude,
        base: "https://status.claude.com",
    },
    Source {
        tool: ToolType::Cursor,
        base: "https://status.cursor.com",
    },
];

fn public_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap_or_default()
}

fn seed(root: &DataRoot) -> ServiceStatusView {
    let cached = service_status::load(root);
    let mut providers = [ToolType::Claude, ToolType::Cursor]
        .into_iter()
        .filter_map(|tool| {
            cached.get(&tool).map(|snapshot| ProviderStatus {
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
    response
        .json::<T>()
        .await
        .map_err(|error| ServiceStatusError::Parse(error.to_string()))
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
    fn seed_reads_shared_claude_cursor_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let root = DataRoot::at(dir.path().join(".vibebar"));
        std::fs::create_dir_all(root.shared()).unwrap();
        std::fs::write(root.service_status_file(), r#"["claude",{"indicator":"none","description":"Operational","updatedAt":809731205},"cursor",{"indicator":"major","description":"Outage"}]"#).unwrap();
        let view = ServiceStatusEngine::new(root).cached();
        assert_eq!(view.providers.len(), 2);
        assert_eq!(view.providers[0].tool, ToolType::Claude);
        assert_eq!(view.providers[1].indicator, "major");
    }

    #[test]
    fn unknown_indicator_fails_closed() {
        assert_eq!(normalize_indicator(Some("future")), None);
    }

    #[tokio::test]
    async fn demo_refresh_never_goes_to_the_network() {
        let dir = tempfile::tempdir().unwrap();
        let engine = ServiceStatusEngine::new(DataRoot::at(dir.path().join(".vibebar")));
        assert_eq!(engine.refresh().await.unwrap().providers, Vec::new());
    }
}
