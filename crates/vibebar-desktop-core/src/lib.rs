//! Platform-independent core of Vibe Bar Desktop.
//!
//! Vibe Bar is one product with two client implementations: the macOS
//! native app and this cross-platform desktop client. They share the user's
//! canonical data root (`~/.vibebar` on macOS), but they depend on nothing
//! of each other's code — Desktop must run identically on a Mac that has
//! never seen the native app.
//!
//! **Write boundary (enforced, not aspirational).** In this preview slice
//! Desktop is a strict *reader* of every shared store and writes only inside
//! its own namespace, `<data root>/client/desktop/`. Becoming a writer of
//! shared state requires the cross-process storage contract (single-writer
//! lease, schema negotiation, fail-closed migrations) that does not exist
//! yet on either side. See `docs/SHARED-STORAGE.md`.

pub mod client_store;
pub mod cost;
pub mod credentials;
pub mod error;
pub mod mcp;
pub mod model;
pub mod paths;
pub mod providers;
pub mod refresh;
pub mod sessions;
pub mod shared;
pub mod skills;
pub mod status;
pub mod tokens;

pub use error::{CoreError, QuotaError};
pub use model::{AccountQuota, QuotaBucket, ToolType};
pub use paths::DataRoot;

/// Crate version — the Desktop client's own `productVersion` during the
/// pre-parity preview. Once Desktop reaches feature parity it adopts the
/// shared Vibe Bar release train and this tracks the native app's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
