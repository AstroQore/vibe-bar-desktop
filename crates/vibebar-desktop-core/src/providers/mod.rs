//! Provider quota adapters.
//!
//! Each adapter owns one provider's whole story: which credential it accepts,
//! which endpoint it calls, and how that response becomes buckets. Parsing is
//! kept in free functions taking bytes so the wire shapes are unit-tested
//! against synthetic fixtures without a network.
//!
//! This preview slice ships ten live adapters. The remaining providers render
//! from the shared cache, attributed as such, until their adapters land.

pub mod alibaba;
pub mod claude;
pub mod codex;
pub mod copilot;
pub mod kilo;
pub mod kiro;
pub mod minimax;
pub mod openrouter;
pub mod warp;
pub mod zai;

use std::path::Path;
use std::time::Duration;
use std::{collections::HashMap, env};

use crate::error::QuotaError;
use crate::model::{AccountQuota, ToolType};

/// Per-adapter ceiling. The native app quarantines an adapter that blows this
/// budget; here it simply becomes [`QuotaError::TimedOut`], and the previous
/// observation stays on screen.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Fetch one provider's live quota. Providers without an adapter in this
/// build report [`QuotaError::NotImplemented`] and are rendered from the
/// shared cache instead.
pub async fn fetch(
    tool: ToolType,
    home: &Path,
    client: &reqwest::Client,
) -> Result<AccountQuota, QuotaError> {
    let result = tokio::time::timeout(REQUEST_TIMEOUT, async {
        match tool {
            ToolType::Codex => codex::fetch(home, client).await,
            ToolType::Claude => claude::fetch(home, client).await,
            ToolType::Alibaba => alibaba::fetch(home, client).await,
            ToolType::Copilot => copilot::fetch(home, client).await,
            ToolType::Zai => zai::fetch(home, client).await,
            ToolType::Minimax => minimax::fetch(client).await,
            ToolType::Kilo => kilo::fetch(client, home).await,
            ToolType::Kiro => {
                let environment: Vec<(String, String)> = std::env::vars().collect();
                kiro::fetch(home, &environment).await
            }
            ToolType::OpenRouter => openrouter::fetch(client).await,
            ToolType::Warp => warp::fetch(client).await,
            _ => Err(QuotaError::NotImplemented),
        }
    })
    .await;
    match result {
        Ok(result) => result,
        Err(_) => Err(QuotaError::TimedOut),
    }
}

pub(crate) fn trusted_https_url(raw: &str, allowed_domains: &[&str]) -> Option<reqwest::Url> {
    let url = reqwest::Url::parse(raw.trim()).ok()?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    allowed_domains
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
        .then_some(url)
}

/// Read only the named environment variables, ignoring non-Unicode values.
///
/// `std::env::vars` panics when any unrelated variable is not valid Unicode,
/// so provider adapters must use an explicit allowlist instead.
pub(crate) fn read_env(keys: &[&str]) -> HashMap<String, String> {
    keys.iter()
        .filter_map(|key| env::var(key).ok().map(|value| ((*key).to_string(), value)))
        .collect()
}

/// Map a transport failure onto the shared error taxonomy.
pub(crate) fn classify_transport(error: &reqwest::Error) -> QuotaError {
    if error.is_timeout() {
        QuotaError::TimedOut
    } else {
        QuotaError::Network(error.to_string())
    }
}

/// Map an HTTP status onto the shared error taxonomy.
pub(crate) fn classify_status(status: reqwest::StatusCode) -> Option<QuotaError> {
    match status.as_u16() {
        200..=299 => None,
        401 | 403 => Some(QuotaError::NeedsLogin),
        429 => Some(QuotaError::RateLimited),
        other => Some(QuotaError::Unknown(format!("HTTP {other}"))),
    }
}

/// Seconds since the Unix epoch, as a float.
pub(crate) fn now_unix() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_classification_matches_the_shared_taxonomy() {
        assert!(classify_status(reqwest::StatusCode::OK).is_none());
        assert_eq!(
            classify_status(reqwest::StatusCode::UNAUTHORIZED),
            Some(QuotaError::NeedsLogin)
        );
        assert_eq!(
            classify_status(reqwest::StatusCode::FORBIDDEN),
            Some(QuotaError::NeedsLogin)
        );
        assert_eq!(
            classify_status(reqwest::StatusCode::TOO_MANY_REQUESTS),
            Some(QuotaError::RateLimited)
        );
        assert_eq!(
            classify_status(reqwest::StatusCode::INTERNAL_SERVER_ERROR),
            Some(QuotaError::Unknown("HTTP 500".into()))
        );
    }

    #[test]
    fn credentialed_overrides_require_https_provider_hosts() {
        assert!(
            trusted_https_url("https://proxy.openrouter.ai/api/v1", &["openrouter.ai"]).is_some()
        );
        assert!(trusted_https_url("http://openrouter.ai/api/v1", &["openrouter.ai"]).is_none());
        assert!(trusted_https_url("https://example.test/api/v1", &["openrouter.ai"]).is_none());
        assert!(trusted_https_url(
            "https://openrouter.ai@example.test/api/v1",
            &["openrouter.ai"]
        )
        .is_none());
    }
}
