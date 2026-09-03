//! `~/.grok/auth.json`, written by `grok login` — the native
//! `GrokCredentialsStore`.
//!
//! The file is a map keyed by scope URL. The OIDC scope
//! (`https://auth.x.ai::<client-id>`, what SuperGrok subscribers get) wins
//! over the legacy session scope, and an entry without a `key` is not a
//! credential at all. Nothing here is written back.

use std::path::Path;

use crate::error::QuotaError;

/// Top-level OIDC scope `grok login` uses for SuperGrok subscribers.
const OIDC_SCOPE_PREFIX: &str = "https://auth.x.ai::";
/// Legacy session scope from older `grok login` flows.
const LEGACY_SESSION_SCOPE: &str = "https://accounts.x.ai/sign-in";

#[derive(Debug, Clone, PartialEq)]
pub struct GrokCredential {
    /// Sensitive: never log, never persist outside its origin.
    pub access_token: String,
    pub scope: String,
    pub auth_mode: Option<String>,
    pub email: Option<String>,
    pub subscription_tier: Option<String>,
    /// Unix seconds, when the file carries an expiry.
    pub expires_at: Option<f64>,
}

impl GrokCredential {
    pub fn is_expired(&self, now_unix: f64) -> bool {
        self.expires_at.is_some_and(|at| now_unix >= at)
    }

    /// The badge label. SuperGrok is the only tier today; a legacy session
    /// login says so, so the two can be told apart in the popover.
    pub fn plan_label(&self) -> Option<String> {
        if let Some(tier) = self
            .subscription_tier
            .as_deref()
            .and_then(crate::providers::plan_display::grok)
        {
            return Some(tier);
        }
        match self.auth_mode.as_deref().map(str::to_ascii_lowercase) {
            Some(mode) if mode == "oidc" => Some("SuperGrok".into()),
            Some(mode) if mode == "session" => Some("Session".into()),
            None => None,
            _ => self.auth_mode.clone(),
        }
    }
}

pub fn auth_file(home: &Path) -> std::path::PathBuf {
    home.join(".grok/auth.json")
}

pub fn load(home: &Path) -> Result<GrokCredential, QuotaError> {
    let path = auth_file(home);
    if !path.is_file() {
        return Err(QuotaError::NoCredential);
    }
    let bytes = std::fs::read(&path)
        .map_err(|error| QuotaError::ParseFailure(format!("could not read auth.json: {error}")))?;
    parse(&bytes)
}

pub fn parse(bytes: &[u8]) -> Result<GrokCredential, QuotaError> {
    let root: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| QuotaError::ParseFailure(format!("auth.json is not JSON: {error}")))?;
    let object = root
        .as_object()
        .ok_or_else(|| QuotaError::ParseFailure("auth.json root is not an object".into()))?;

    let mut oidc = None;
    let mut legacy = None;
    for (scope, value) in object {
        let Some(entry) = value.as_object() else {
            continue;
        };
        let has_key = entry
            .get("key")
            .and_then(|v| v.as_str())
            .is_some_and(|key| !key.is_empty());
        if !has_key {
            continue;
        }
        if scope.starts_with(OIDC_SCOPE_PREFIX) {
            oidc = Some((scope, entry));
        } else if scope == LEGACY_SESSION_SCOPE || scope.contains("/sign-in") {
            legacy = Some((scope, entry));
        }
    }
    let (scope, entry) = oidc.or(legacy).ok_or(QuotaError::NoCredential)?;

    let text =
        |key: &str| -> Option<String> { crate::credentials::non_empty_string(entry.get(key)) };
    let access_token = text("key").ok_or(QuotaError::NoCredential)?;
    Ok(GrokCredential {
        access_token,
        scope: scope.clone(),
        auth_mode: text("auth_mode"),
        email: text("email"),
        subscription_tier: text("subscription_tier")
            .or_else(|| text("plan_name"))
            .or_else(|| text("plan"))
            .or_else(|| text("tier")),
        expires_at: text("expires_at").as_deref().and_then(parse_timestamp),
    })
}

/// RFC 3339 with or without fractional seconds, as Unix seconds.
fn parse_timestamp(raw: &str) -> Option<f64> {
    let parsed = chrono::DateTime::parse_from_rfc3339(raw.trim()).ok()?;
    Some(parsed.timestamp() as f64 + f64::from(parsed.timestamp_subsec_nanos()) / 1e9)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTH: &str = r#"{
      "https://accounts.x.ai/sign-in": {"key": "legacy-token", "auth_mode": "session"},
      "https://auth.x.ai::client-abc": {
        "key": "oidc-token", "auth_mode": "oidc", "email": " person@example.com ",
        "first_name": "Person", "subscription_tier": "SUPER_GROK_HEAVY",
        "expires_at": "2026-09-01T00:00:00Z"
      },
      "unrelated": {"key": "nope"},
      "https://auth.x.ai::no-key": {"other": true}
    }"#;

    #[test]
    fn the_oidc_scope_wins_over_the_legacy_session() {
        let credential = parse(AUTH.as_bytes()).unwrap();
        assert_eq!(credential.access_token, "oidc-token");
        assert_eq!(credential.scope, "https://auth.x.ai::client-abc");
        assert_eq!(credential.email.as_deref(), Some("person@example.com"));
        assert_eq!(credential.plan_label().as_deref(), Some("SuperGrok Heavy"));
        assert_eq!(credential.expires_at, Some(1_788_220_800.0));
        assert!(credential.is_expired(1_788_220_801.0));
        assert!(!credential.is_expired(1_788_219_800.0));
    }

    #[test]
    fn a_session_only_file_still_works() {
        let credential =
            parse(br#"{"https://accounts.x.ai/sign-in": {"key": "t", "auth_mode": "session"}}"#)
                .unwrap();
        assert_eq!(credential.access_token, "t");
        assert_eq!(credential.plan_label().as_deref(), Some("Session"));
        // No expiry in the file means nothing to expire against.
        assert!(!credential.is_expired(f64::MAX));
    }

    #[test]
    fn a_file_with_no_usable_entry_is_no_credential() {
        for raw in [
            r#"{}"#,
            r#"{"unrelated": {"key": "x"}}"#,
            r#"{"https://auth.x.ai::c": {"key": ""}}"#,
            r#"{"https://auth.x.ai::c": "not an object"}"#,
        ] {
            assert!(
                matches!(parse(raw.as_bytes()), Err(QuotaError::NoCredential)),
                "{raw}"
            );
        }
        assert!(matches!(parse(b"[]"), Err(QuotaError::ParseFailure(_))));
        assert!(matches!(
            parse(b"not json"),
            Err(QuotaError::ParseFailure(_))
        ));
    }

    #[test]
    fn a_missing_file_is_no_credential() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(load(dir.path()), Err(QuotaError::NoCredential)));
        assert!(auth_file(Path::new("/Users/example")).ends_with(".grok/auth.json"));
    }
}
