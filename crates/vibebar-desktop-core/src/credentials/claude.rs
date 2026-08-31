//! Claude credential discovery: `~/.claude/.credentials.json`,
//! `~/.config/claude/.credentials.json`, and the CLI's
//! `Claude Code-credentials` keychain item.

use std::path::Path;

use crate::credentials::{non_empty_string, CredentialSource};
use crate::error::QuotaError;

const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

#[derive(Debug, Clone)]
pub struct ClaudeCredential {
    /// Sensitive: never log, never persist outside its origin.
    pub access_token: String,
    /// Unix epoch seconds, when the credential states one.
    pub expires_at: Option<f64>,
    pub rate_limit_tier: Option<String>,
    pub source: CredentialSource,
}

impl ClaudeCredential {
    /// Whether the token is past its own stated expiry. An expired token is
    /// still sent once — the endpoint is the authority, and clock skew
    /// shouldn't manufacture a login prompt — but it explains a 401.
    pub fn is_expired(&self, now_unix: f64) -> bool {
        self.expires_at.is_some_and(|expiry| expiry <= now_unix)
    }
}

pub fn load(home: &Path) -> Result<ClaudeCredential, QuotaError> {
    for relative in [
        ".claude/.credentials.json",
        ".config/claude/.credentials.json",
    ] {
        let path = home.join(relative);
        if path.is_file() {
            if let Ok(bytes) = std::fs::read(&path) {
                if let Ok(credential) = decode(&bytes, CredentialSource::OauthCli) {
                    return Ok(credential);
                }
            }
        }
    }
    if let Some(raw) = crate::credentials::keychain::read_generic_password(KEYCHAIN_SERVICE) {
        return decode(raw.as_bytes(), CredentialSource::CliDetected);
    }
    Err(QuotaError::NoCredential)
}

pub fn decode(bytes: &[u8], source: CredentialSource) -> Result<ClaudeCredential, QuotaError> {
    let root: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|_| QuotaError::ParseFailure("credentials json is not an object".into()))?;
    if !root.is_object() {
        return Err(QuotaError::ParseFailure(
            "credentials json is not an object".into(),
        ));
    }
    let oauth = root
        .get("claudeAiOauth")
        .or_else(|| root.get("claude.ai_oauth"))
        .unwrap_or(&root);

    let access_token = non_empty_string(oauth.get("accessToken"))
        .or_else(|| non_empty_string(oauth.get("access_token")))
        .ok_or(QuotaError::NeedsLogin)?;

    let expires_at = oauth
        .get("expiresAt")
        .or_else(|| oauth.get("expires_at"))
        .and_then(parse_epoch);
    let rate_limit_tier = non_empty_string(oauth.get("rateLimitTier"))
        .or_else(|| non_empty_string(oauth.get("rate_limit_tier")));

    Ok(ClaudeCredential {
        access_token,
        expires_at,
        rate_limit_tier,
        source,
    })
}

/// Claude has shipped this field as both seconds and milliseconds. Values
/// beyond year ~33658 in seconds are read as milliseconds — the same
/// threshold the native reader uses.
fn parse_epoch(value: &serde_json::Value) -> Option<f64> {
    let raw = match value {
        serde_json::Value::Number(n) => n.as_f64()?,
        serde_json::Value::String(s) => s.parse::<f64>().ok()?,
        _ => return None,
    };
    Some(if raw > 1_000_000_000_000.0 {
        raw / 1000.0
    } else {
        raw
    })
}

/// Account id matching the native `AccountStore` naming.
pub fn account_id(credential: &ClaudeCredential) -> String {
    match credential.source {
        CredentialSource::OauthCli => "oauth-claude".to_string(),
        CredentialSource::CliDetected => "cli-claude".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_nested_oauth_shape() {
        let body = serde_json::json!({
            "claudeAiOauth": {
                "accessToken": "synthetic-token",
                "expiresAt": 1_788_038_405_000_i64,
                "rateLimitTier": "Max"
            }
        });
        let cred = decode(body.to_string().as_bytes(), CredentialSource::OauthCli).unwrap();
        assert_eq!(cred.access_token, "synthetic-token");
        // Milliseconds normalized to seconds.
        assert_eq!(cred.expires_at, Some(1_788_038_405.0));
        assert_eq!(cred.rate_limit_tier.as_deref(), Some("Max"));
        assert_eq!(account_id(&cred), "oauth-claude");
    }

    #[test]
    fn decodes_the_flat_shape_with_seconds() {
        let body = serde_json::json!({
            "access_token": "synthetic-token",
            "expires_at": 1_788_038_405_i64
        });
        let cred = decode(body.to_string().as_bytes(), CredentialSource::CliDetected).unwrap();
        assert_eq!(cred.expires_at, Some(1_788_038_405.0));
        assert!(cred.is_expired(1_788_038_500.0));
        assert!(!cred.is_expired(1_788_000_000.0));
        assert_eq!(account_id(&cred), "cli-claude");
    }

    #[test]
    fn missing_token_reads_as_needs_login() {
        let body = serde_json::json!({"claudeAiOauth": {"scopes": ["user:inference"]}});
        assert_eq!(
            decode(body.to_string().as_bytes(), CredentialSource::OauthCli).unwrap_err(),
            QuotaError::NeedsLogin
        );
    }

    #[test]
    fn reads_a_credentials_file_from_either_location() {
        for relative in [".claude", ".config/claude"] {
            let dir = tempfile::tempdir().unwrap();
            let home = dir.path();
            std::fs::create_dir_all(home.join(relative)).unwrap();
            std::fs::write(
                home.join(relative).join(".credentials.json"),
                serde_json::json!({"claudeAiOauth": {"accessToken": "synthetic"}}).to_string(),
            )
            .unwrap();
            let cred = load(home).unwrap();
            assert_eq!(cred.access_token, "synthetic");
            assert_eq!(cred.source, CredentialSource::OauthCli);
        }
    }
}
