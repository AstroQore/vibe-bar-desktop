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
    parse_at(&bytes, crate::providers::now_unix())
}

pub fn parse(bytes: &[u8]) -> Result<GrokCredential, QuotaError> {
    parse_at(bytes, crate::providers::now_unix())
}

pub fn parse_at(bytes: &[u8], now_unix: f64) -> Result<GrokCredential, QuotaError> {
    let root: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| QuotaError::ParseFailure(format!("auth.json is not JSON: {error}")))?;
    let object = root
        .as_object()
        .ok_or_else(|| QuotaError::ParseFailure("auth.json root is not an object".into()))?;

    // Every matching entry, not just the last one seen: a file can carry
    // several OIDC scopes after the CLI changes client ids, and one of them
    // being expired says nothing about the others.
    let mut oidc = Vec::new();
    let mut legacy = Vec::new();
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
            oidc.push((scope, entry));
        } else if scope == LEGACY_SESSION_SCOPE || scope.contains("/sign-in") {
            legacy.push((scope, entry));
        }
    }
    // The OIDC scope is preferred, but not when it has expired and the legacy
    // session has not: an expired preference is no credential at all.
    // OIDC scopes first, in map order, then the legacy sessions.
    let candidates: Vec<(&String, &serde_json::Map<String, serde_json::Value>)> =
        oidc.into_iter().chain(legacy).collect();
    let mut first: Option<GrokCredential> = None;
    for (scope, entry) in candidates {
        let credential = decode_entry(scope, entry).ok_or(QuotaError::NoCredential)?;
        if !credential.is_expired(now_unix) {
            return Ok(credential);
        }
        first.get_or_insert(credential);
    }
    // Everything on file has expired. Report the preferred one, so the error
    // names the credential the person would renew.
    first.ok_or(QuotaError::NoCredential)
}

fn decode_entry(
    scope: &str,
    entry: &serde_json::Map<String, serde_json::Value>,
) -> Option<GrokCredential> {
    let text =
        |key: &str| -> Option<String> { crate::credentials::non_empty_string(entry.get(key)) };
    Some(GrokCredential {
        access_token: text("key")?,
        scope: scope.to_string(),
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

    /// Well before the fixture's expiry.
    const BEFORE: f64 = 1_700_000_000.0;

    #[test]
    fn the_oidc_scope_wins_over_the_legacy_session() {
        let credential = parse_at(AUTH.as_bytes(), BEFORE).unwrap();
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
        let credential = parse_at(
            br#"{"https://accounts.x.ai/sign-in": {"key": "t", "auth_mode": "session"}}"#,
            BEFORE,
        )
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
                matches!(
                    parse_at(raw.as_bytes(), BEFORE),
                    Err(QuotaError::NoCredential)
                ),
                "{raw}"
            );
        }
        assert!(matches!(
            parse_at(b"[]", BEFORE),
            Err(QuotaError::ParseFailure(_))
        ));
        assert!(matches!(
            parse_at(b"not json", BEFORE),
            Err(QuotaError::ParseFailure(_))
        ));
    }

    #[test]
    fn a_missing_file_is_no_credential() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(load(dir.path()), Err(QuotaError::NoCredential)));
        assert!(auth_file(Path::new("/Users/example")).ends_with(".grok/auth.json"));
    }

    #[test]
    fn an_expired_oidc_entry_does_not_shadow_a_usable_session() {
        // The fixture's OIDC entry expires 2026-09-01; the legacy one never
        // does. After that date the session is the credential.
        let after = 1_788_220_801.0;
        let credential = parse_at(AUTH.as_bytes(), after).unwrap();
        assert_eq!(credential.access_token, "legacy-token");
        assert_eq!(credential.plan_label().as_deref(), Some("Session"));
        // With both expired, the preferred one is reported, so the error the
        // caller raises names the credential worth renewing.
        let both = r#"{
          "https://accounts.x.ai/sign-in": {"key": "legacy", "expires_at": "2026-01-01T00:00:00Z"},
          "https://auth.x.ai::c": {"key": "oidc", "expires_at": "2026-02-01T00:00:00Z"}
        }"#;
        let credential = parse_at(both.as_bytes(), after).unwrap();
        assert_eq!(credential.access_token, "oidc");
        assert!(credential.is_expired(after));
    }

    #[test]
    fn a_second_oidc_scope_is_tried_when_the_first_has_expired() {
        // Two OIDC scopes, as after a client-id change: the expired one must
        // not decide for the usable one, whichever order the map yields.
        let raw = r#"{
          "https://auth.x.ai::old-client": {"key": "old", "expires_at": "2026-01-01T00:00:00Z"},
          "https://auth.x.ai::new-client": {"key": "new", "expires_at": "2030-01-01T00:00:00Z"},
          "https://accounts.x.ai/sign-in": {"key": "legacy"}
        }"#;
        let credential = parse_at(raw.as_bytes(), 1_788_220_801.0).unwrap();
        assert_eq!(credential.access_token, "new");
    }
}
