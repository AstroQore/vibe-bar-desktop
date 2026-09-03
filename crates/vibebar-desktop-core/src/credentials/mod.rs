//! Reading the credentials the coding CLIs already wrote.
//!
//! **Boundary.** This client never asks the user for a provider password, and
//! in this slice it never writes a credential either — it reads what the
//! Codex and Claude CLIs left on disk (or in the macOS login keychain, where
//! those CLIs put it), and the session Cursor.app keeps in its own state
//! database, and uses each for exactly one thing: that provider's own usage
//! endpoint.
//!
//! **Refresh is deliberately not implemented here.** Codex's `auth.json`
//! rewrite is a three-way race between the Codex CLI, the native app, and
//! this client, and none of the three take a lock. Until the shared storage
//! contract lands, an expired OAuth token is reported as
//! [`QuotaError::NeedsLogin`] and the user re-runs the CLI's own login —
//! which is annoying once, versus corrupting the credential all three share.

pub mod claude;
pub mod codex;
pub mod cursor;
pub mod keychain;

use serde::Serialize;

/// Where a credential came from, mirroring the native `CredentialSource`
/// vocabulary so both clients describe the same route identically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CredentialSource {
    /// A credential file the CLI wrote (`~/.codex/auth.json`, …).
    OauthCli,
    /// A keychain item the CLI wrote.
    CliDetected,
}

/// Best-effort JWT claim extraction. Only the unsigned payload is read, and
/// only for non-secret display fields (account id, plan, email); the token is
/// never logged and the signature is irrelevant here because the token is
/// being handed straight back to its issuer.
pub(crate) fn jwt_claims(token: &str) -> Option<serde_json::Value> {
    use base64::Engine;
    let mut parts = token.split('.');
    let _header = parts.next()?;
    let payload = parts.next()?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    serde_json::from_slice(&decoded).ok()
}

/// Read a string field, treating whitespace-only as absent.
pub(crate) fn non_empty_string(value: Option<&serde_json::Value>) -> Option<String> {
    let text = value?.as_str()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_synthetic_jwt_payload() {
        use base64::Engine;
        let payload = serde_json::json!({
            "email": "user@example.com",
            "https://api.openai.com/auth": {"chatgpt_account_id": "acct-123",
                                            "chatgpt_plan_type": "pro"}
        });
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).unwrap());
        let token = format!("header.{encoded}.signature");
        let claims = jwt_claims(&token).unwrap();
        assert_eq!(claims["email"], "user@example.com");
        assert_eq!(
            claims["https://api.openai.com/auth"]["chatgpt_plan_type"],
            "pro"
        );
    }

    #[test]
    fn malformed_tokens_yield_none() {
        assert!(jwt_claims("").is_none());
        assert!(jwt_claims("only-one-part").is_none());
        assert!(jwt_claims("a.!!!not-base64!!!.c").is_none());
    }
}
