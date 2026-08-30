//! Codex credential discovery: `~/.codex/auth.json` and the CLI's
//! `Codex Auth` keychain item.

use std::path::Path;

use crate::credentials::{jwt_claims, non_empty_string, CredentialSource};
use crate::error::QuotaError;

const KEYCHAIN_SERVICE: &str = "Codex Auth";
const AUTH_KEY: &str = "https://api.openai.com/auth";
const PROFILE_KEY: &str = "https://api.openai.com/profile";

#[derive(Debug, Clone)]
pub struct CodexCredential {
    /// Sensitive: never log, never persist outside its origin.
    pub access_token: String,
    pub account_id: Option<String>,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub source: CredentialSource,
}

/// Resolve a Codex credential: the CLI's `auth.json` first (it is the durable
/// OAuth material), then the keychain item the CLI mirrors it into.
pub fn load(home: &Path) -> Result<CodexCredential, QuotaError> {
    let auth_path = home.join(".codex/auth.json");
    if auth_path.is_file() {
        if let Ok(bytes) = std::fs::read(&auth_path) {
            match decode(&bytes, CredentialSource::OauthCli) {
                Ok(credential) => return Ok(credential),
                // A present-but-unusable auth.json is worth reporting rather
                // than masking with a keychain hit that may be staler.
                Err(QuotaError::NotImplemented) => return Err(QuotaError::NotImplemented),
                Err(_) => {}
            }
        }
    }
    if let Some(raw) = crate::credentials::keychain::read_generic_password(KEYCHAIN_SERVICE) {
        return decode(raw.as_bytes(), CredentialSource::CliDetected);
    }
    Err(QuotaError::NoCredential)
}

pub fn decode(bytes: &[u8], source: CredentialSource) -> Result<CodexCredential, QuotaError> {
    let root: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|_| QuotaError::ParseFailure("auth.json is not a JSON object".into()))?;
    if !root.is_object() {
        return Err(QuotaError::ParseFailure(
            "auth.json is not a JSON object".into(),
        ));
    }

    let auth_mode = root
        .get("auth_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tokens = root.get("tokens");

    let pick = |key_snake: &str, key_camel: &str| -> Option<String> {
        tokens
            .and_then(|t| non_empty_string(t.get(key_snake)))
            .or_else(|| tokens.and_then(|t| non_empty_string(t.get(key_camel))))
            .or_else(|| non_empty_string(root.get(key_snake)))
            .or_else(|| non_empty_string(root.get(key_camel)))
    };

    let access_token = pick("access_token", "accessToken").ok_or(QuotaError::NeedsLogin)?;
    let id_token = pick("id_token", "idToken");
    let claims = id_token.as_deref().and_then(jwt_claims);

    // API-key mode has no subscription quota to report.
    if !auth_mode.is_empty() && auth_mode != "chatgpt" {
        return Err(QuotaError::NotImplemented);
    }

    let account_id = pick("account_id", "accountId").or_else(|| {
        claims.as_ref().and_then(|c| {
            non_empty_string(c.get(AUTH_KEY).and_then(|a| a.get("chatgpt_account_id")))
                .or_else(|| non_empty_string(c.get("chatgpt_account_id")))
        })
    });
    let email = claims.as_ref().and_then(|c| {
        non_empty_string(c.get("email"))
            .or_else(|| non_empty_string(c.get(PROFILE_KEY).and_then(|p| p.get("email"))))
    });
    let plan = claims.as_ref().and_then(|c| {
        non_empty_string(c.get(AUTH_KEY).and_then(|a| a.get("chatgpt_plan_type")))
            .or_else(|| non_empty_string(c.get("chatgpt_plan_type")))
    });

    Ok(CodexCredential {
        access_token,
        account_id,
        email,
        plan,
        source,
    })
}

/// The account id this credential's quota is cached under, matching the
/// native `AccountStore` naming so both clients agree on identity.
pub fn account_id(credential: &CodexCredential) -> String {
    credential
        .account_id
        .clone()
        .unwrap_or_else(|| match credential.source {
            CredentialSource::OauthCli => "oauth-codex".to_string(),
            CredentialSource::CliDetected => "cli-codex".to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn synthetic_id_token() -> String {
        let payload = serde_json::json!({
            "email": "user@example.com",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "11111111-2222-3333-4444-555555555555",
                "chatgpt_plan_type": "pro"
            }
        });
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).unwrap());
        format!("hdr.{encoded}.sig")
    }

    #[test]
    fn decodes_the_cli_auth_json_shape() {
        let body = serde_json::json!({
            "auth_mode": "chatgpt",
            "last_refresh": "2026-08-30T04:00:00Z",
            "tokens": {
                "access_token": "synthetic-access-token",
                "refresh_token": "synthetic-refresh-token",
                "id_token": synthetic_id_token()
            }
        });
        let cred = decode(body.to_string().as_bytes(), CredentialSource::OauthCli).unwrap();
        assert_eq!(cred.access_token, "synthetic-access-token");
        assert_eq!(
            cred.account_id.as_deref(),
            Some("11111111-2222-3333-4444-555555555555")
        );
        assert_eq!(cred.plan.as_deref(), Some("pro"));
        assert_eq!(cred.email.as_deref(), Some("user@example.com"));
        assert_eq!(account_id(&cred), "11111111-2222-3333-4444-555555555555");
    }

    #[test]
    fn api_key_mode_has_no_quota() {
        let body = serde_json::json!({
            "auth_mode": "apikey",
            "tokens": {"access_token": "sk-synthetic"}
        });
        assert_eq!(
            decode(body.to_string().as_bytes(), CredentialSource::OauthCli).unwrap_err(),
            QuotaError::NotImplemented
        );
    }

    #[test]
    fn missing_token_reads_as_needs_login() {
        let body = serde_json::json!({"auth_mode": "chatgpt", "tokens": {}});
        assert_eq!(
            decode(body.to_string().as_bytes(), CredentialSource::OauthCli).unwrap_err(),
            QuotaError::NeedsLogin
        );
    }

    #[test]
    fn absent_home_reports_no_credential() {
        let dir = tempfile::tempdir().unwrap();
        // No ~/.codex and (on CI) no keychain item.
        match load(dir.path()) {
            Err(QuotaError::NoCredential) => {}
            Ok(_) => { /* a developer machine with a real keychain item */ }
            Err(other) => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn falls_back_to_stable_ids_without_an_account_claim() {
        let body = serde_json::json!({"tokens": {"access_token": "t"}});
        let cred = decode(body.to_string().as_bytes(), CredentialSource::OauthCli).unwrap();
        assert_eq!(account_id(&cred), "oauth-codex");
        let cred = decode(body.to_string().as_bytes(), CredentialSource::CliDetected).unwrap();
        assert_eq!(account_id(&cred), "cli-codex");
    }
}
