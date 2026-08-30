use thiserror::Error;

/// Mirrors the native app's `QuotaError` taxonomy so both clients classify
/// the same failure the same way (and a UI written against one reads the
/// other's cache correctly).
#[derive(Debug, Clone, PartialEq, Eq, Error, serde::Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub enum QuotaError {
    /// No credential of any accepted kind was found for this provider.
    #[error("no credential")]
    NoCredential,
    /// A credential exists but the provider rejected it.
    #[error("needs login")]
    NeedsLogin,
    #[error("rate limited")]
    RateLimited,
    #[error("network error: {0}")]
    Network(String),
    #[error("parse failure: {0}")]
    ParseFailure(String),
    /// This provider has no adapter in this build.
    #[error("not implemented")]
    NotImplemented,
    #[error("timed out")]
    TimedOut,
    #[error("unknown error: {0}")]
    Unknown(String),
}

#[derive(Debug, Error)]
pub enum CoreError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Session(#[from] agent_session_core::SessionCoreError),
    /// A write was attempted outside the Desktop client namespace. This is a
    /// programming error, not a runtime condition: the shared data root is
    /// read-only for this client.
    #[error("refusing to write outside the Desktop client namespace: {0}")]
    WriteOutsideClientNamespace(String),
}
