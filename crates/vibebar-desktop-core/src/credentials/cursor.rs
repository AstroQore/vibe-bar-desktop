//! Cursor.app's local session: the `cursorAuth/accessToken` row of the app's
//! `state.vscdb`, read read-only and turned into the first-party
//! `WorkosCursorSessionToken` cookie cursor.com accepts — the native
//! `CursorAppAuth`.
//!
//! Vibe Bar never refreshes Cursor's token. A session that is missing,
//! malformed, or within a minute of expiry is reported as such, and the fix is
//! to open Cursor and sign in; the browser-cookie fallback the native app also
//! has waits on a cookie reader.

use std::path::{Path, PathBuf};

use crate::error::QuotaError;

const ACCESS_TOKEN_KEY: &str = "cursorAuth/accessToken";

#[derive(Debug, Clone, PartialEq)]
pub struct CursorSession {
    /// Sensitive: never log, never persist outside its origin.
    access_token: String,
    /// The last `|`-separated segment of the JWT's `sub`, which is what
    /// cursor.com expects in front of the token in the session cookie.
    pub user_id: String,
    pub email: Option<String>,
    /// Unix seconds from the JWT's `exp`, when it carries one.
    pub expires_at: Option<f64>,
}

impl CursorSession {
    /// Parse the identity out of the access token's unsigned payload. Only
    /// display fields are read; the signature is irrelevant because the token
    /// goes straight back to its issuer.
    pub fn from_access_token(token: &str) -> Result<Self, QuotaError> {
        let token = token.trim();
        let claims = jwt_payload(token)
            .ok_or_else(|| QuotaError::ParseFailure("Cursor access token is not a JWT".into()))?;
        let subject = claims.get("sub").and_then(|v| v.as_str()).unwrap_or("");
        let candidate = subject
            .split('|')
            .rfind(|part| !part.is_empty())
            .unwrap_or("");
        let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
        if candidate.is_empty() || !candidate.chars().all(allowed) {
            return Err(QuotaError::ParseFailure(
                "Cursor access token carries no user id".into(),
            ));
        }
        Ok(Self {
            access_token: token.to_string(),
            user_id: candidate.to_string(),
            email: claims
                .get("email")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|e| !e.is_empty())
                .map(str::to_string),
            expires_at: claims.get("exp").and_then(|v| v.as_f64()),
        })
    }

    /// Usable means "will still be accepted for the next minute": an
    /// expiring token is not worth a request that comes back 401.
    pub fn is_usable(&self, now_unix: f64) -> bool {
        match self.expires_at {
            Some(expires_at) => expires_at - now_unix > 60.0,
            None => false,
        }
    }

    /// The `Cookie` header value cursor.com's dashboard endpoints accept.
    pub fn cookie_header(&self) -> String {
        format!(
            "WorkosCursorSessionToken={}%3A%3A{}",
            self.user_id, self.access_token
        )
    }
}

/// Where Cursor.app keeps its global state on each platform.
pub fn state_db_path(home: &Path) -> PathBuf {
    let relative = if cfg!(target_os = "macos") {
        "Library/Application Support/Cursor/User/globalStorage/state.vscdb"
    } else if cfg!(windows) {
        "AppData/Roaming/Cursor/User/globalStorage/state.vscdb"
    } else {
        ".config/Cursor/User/globalStorage/state.vscdb"
    };
    home.join(relative)
}

/// Load the app's current session. Missing app or row is
/// [`QuotaError::NoCredential`]; a present but expired or malformed token is
/// [`QuotaError::NeedsLogin`] — the person opens Cursor and signs in.
pub fn load(home: &Path) -> Result<CursorSession, QuotaError> {
    load_from(&state_db_path(home), crate::providers::now_unix())
}

pub fn load_from(path: &Path, now_unix: f64) -> Result<CursorSession, QuotaError> {
    if !path.is_file() {
        return Err(QuotaError::NoCredential);
    }
    let token = read_item(path, ACCESS_TOKEN_KEY)?.ok_or(QuotaError::NoCredential)?;
    let session = CursorSession::from_access_token(&token).map_err(|_| QuotaError::NeedsLogin)?;
    if !session.is_usable(now_unix) {
        return Err(QuotaError::NeedsLogin);
    }
    Ok(session)
}

/// One `ItemTable` value, opened read-only with a short busy timeout so a
/// running Cursor.app holding the file never stalls a refresh.
fn read_item(path: &Path, key: &str) -> Result<Option<String>, QuotaError> {
    use rusqlite::{types::Value, OpenFlags};
    let connection = rusqlite::Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| QuotaError::Unknown(format!("Cursor state database: {error}")))?;
    connection
        .busy_timeout(std::time::Duration::from_millis(250))
        .map_err(|error| QuotaError::Unknown(format!("Cursor state database: {error}")))?;
    let mut statement = connection
        .prepare("SELECT value FROM ItemTable WHERE key = ?1 LIMIT 1")
        .map_err(|error| QuotaError::Unknown(format!("Cursor state database: {error}")))?;
    let mut rows = statement
        .query([key])
        .map_err(|error| QuotaError::Unknown(format!("Cursor state database: {error}")))?;
    let Some(row) = rows
        .next()
        .map_err(|error| QuotaError::Unknown(format!("Cursor state database: {error}")))?
    else {
        return Ok(None);
    };
    let value: Value = row
        .get(0)
        .map_err(|error| QuotaError::Unknown(format!("Cursor state database: {error}")))?;
    let text = match value {
        Value::Text(text) => text,
        Value::Blob(bytes) => String::from_utf8(bytes)
            .map_err(|_| QuotaError::ParseFailure("Cursor access token is not UTF-8".into()))?,
        _ => return Ok(None),
    };
    let text = text.trim();
    Ok((!text.is_empty()).then(|| text.to_string()))
}

/// The JWT payload as JSON, tolerant of both padded and unpadded base64url —
/// Cursor's tokens have been seen both ways.
fn jwt_payload(token: &str) -> Option<serde_json::Value> {
    use base64::Engine;
    let payload = token.split('.').nth(1)?;
    let unpadded = payload.trim_end_matches('=');
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(unpadded)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
pub(crate) fn synthetic_token(sub: &str, email: Option<&str>, exp: f64) -> String {
    use base64::Engine;
    let mut payload = serde_json::json!({ "sub": sub, "exp": exp });
    if let Some(email) = email {
        payload["email"] = serde_json::Value::String(email.to_string());
    }
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&payload).unwrap());
    format!("eyJhbGciOiJIUzI1NiJ9.{encoded}.signature")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_comes_from_the_last_subject_segment() {
        let token = synthetic_token(
            "auth0|user_01ABC",
            Some(" person@example.com "),
            2_000_000_000.0,
        );
        let session = CursorSession::from_access_token(&token).unwrap();
        assert_eq!(session.user_id, "user_01ABC");
        assert_eq!(session.email.as_deref(), Some("person@example.com"));
        assert_eq!(session.expires_at, Some(2_000_000_000.0));
        assert_eq!(
            session.cookie_header(),
            format!("WorkosCursorSessionToken=user_01ABC%3A%3A{token}")
        );
    }

    #[test]
    fn a_padded_payload_still_parses() {
        use base64::Engine;
        let payload =
            serde_json::to_vec(&serde_json::json!({"sub": "user_1", "exp": 1.0})).unwrap();
        let padded = base64::engine::general_purpose::URL_SAFE.encode(payload);
        assert!(padded.ends_with('='));
        let session = CursorSession::from_access_token(&format!("h.{padded}.s")).unwrap();
        assert_eq!(session.user_id, "user_1");
    }

    #[test]
    fn a_subject_with_unsafe_characters_is_rejected() {
        for sub in ["", "|", "auth0|user/1", "auth0|user 1", "auth0|user@1"] {
            let token = synthetic_token(sub, None, 2_000_000_000.0);
            assert!(CursorSession::from_access_token(&token).is_err(), "{sub:?}");
        }
        assert!(CursorSession::from_access_token("not-a-jwt").is_err());
    }

    #[test]
    fn usable_means_more_than_a_minute_left() {
        let session =
            CursorSession::from_access_token(&synthetic_token("u", None, 1_000.0)).unwrap();
        assert!(session.is_usable(900.0));
        assert!(!session.is_usable(950.0));
        assert!(!session.is_usable(1_100.0));
        let mut no_expiry = session.clone();
        no_expiry.expires_at = None;
        assert!(!no_expiry.is_usable(0.0));
    }

    fn state_db(dir: &Path, rows: &[(&str, &str)]) -> PathBuf {
        let path = dir.join("state.vscdb");
        let _ = std::fs::remove_file(&path);
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);",
            )
            .unwrap();
        for (key, value) in rows {
            connection
                .execute(
                    "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
                    [key, value],
                )
                .unwrap();
        }
        path
    }

    #[test]
    fn loads_the_session_from_the_state_database() {
        let dir = tempfile::tempdir().unwrap();
        let token = synthetic_token("auth0|user_42", None, 2_000_000_000.0);
        let path = state_db(
            dir.path(),
            &[("cursorAuth/accessToken", &token), ("other", "x")],
        );
        let session = load_from(&path, 1_900_000_000.0).unwrap();
        assert_eq!(session.user_id, "user_42");
    }

    #[test]
    fn missing_expired_and_malformed_sessions_are_told_apart() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            load_from(&dir.path().join("absent.vscdb"), 0.0),
            Err(QuotaError::NoCredential)
        ));
        let path = state_db(dir.path(), &[("unrelated", "x")]);
        assert!(matches!(
            load_from(&path, 0.0),
            Err(QuotaError::NoCredential)
        ));
        let expired = synthetic_token("auth0|user_42", None, 1_000.0);
        let path = state_db(dir.path(), &[("cursorAuth/accessToken", &expired)]);
        assert!(matches!(
            load_from(&path, 2_000.0),
            Err(QuotaError::NeedsLogin)
        ));
        let path = state_db(dir.path(), &[("cursorAuth/accessToken", "garbage")]);
        assert!(matches!(load_from(&path, 0.0), Err(QuotaError::NeedsLogin)));
    }

    #[test]
    fn the_state_path_follows_the_platform() {
        let path = state_db_path(Path::new("/Users/example"));
        assert!(path.ends_with("Cursor/User/globalStorage/state.vscdb"));
        assert!(path.starts_with("/Users/example"));
    }
}
