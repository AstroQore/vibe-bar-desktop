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
    /// The UI did not present a transcript capability issued for a current
    /// session listing. Keep this deliberately path-free: session paths are
    /// local implementation details, never part of the IPC error surface.
    #[error("session transcript is no longer available")]
    SessionReferenceInvalid,
    #[error("session transcript is unavailable")]
    TranscriptUnavailable,
    #[error("Desktop client document is unavailable at a newer or unreadable schema: {0}")]
    ClientDocumentUnavailable(&'static str),
    #[error("invalid Desktop client snapshot: {0}")]
    InvalidClientSnapshot(String),
    /// The shared settings file exists but cannot be read as an object —
    /// malformed, not an object, or past its size cap. A save must fail here
    /// rather than replace it: rebuilding the file from the keys this client
    /// happens to be submitting would delete every setting in it.
    #[error("the shared settings file cannot be read, so it will not be replaced")]
    SharedSettingsUnreadable,
}
