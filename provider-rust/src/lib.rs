//! `secretspec-provider-sops` — SOPS provider for SecretSpec.
//!
//! Phase 1 surface (live):
//! - `SopsProvider::new()` / `with_age_keyfile(path)` — provider instances
//! - `SopsProvider::get(...)` / `get_bytes(...)` — extract a single key
//!   from a SOPS-encrypted file. Format inferred from extension; hints
//!   accepted via `--format` flag or `format_hint` argument.
//! - `SopsProvider::doctor()` — emit a `DoctorReport` JSON dump of the
//!   local `sops` / `age` environment
//! - `parse_dotenv_line` / `strip_inline_comment` — exposed because
//!   tests need them; not part of the upstream v0.1 SecretSpec surface
//!
//! Phase 2 surface (added this turn, per Domen Kozar's accept-criteria):
//! - `SopsUri` / `FieldSpec` (`uri` module) — provider-URI parser with
//!   credential-leakage prevention. Sensitives render as `***` in
//!   `Display` / `to_string`. Implements Domen's #2 ("no credential
//!   leakage").
//! - `CredentialsChain` / `CredentialValue` (`credentials` module) —
//!   provider credentials chain. Resolves each `FieldSpec` URI through
//!   a framework-supplied resolver callback; sensitive values are
//!   masked in `Debug` / `Display`. Implements Domen's #1 ("provider
//!   credentials pattern").
//!
//! Decoupling: the lib is `secretspec` framework-agnostic. SecretSpec
//! consumes the resolved value through its generic provider interface
//! (cachix/secretspec#98 Secret Provider Protocol v1); the crate does
//! not depend on `secretspec = "0.x"` to avoid the upstream churn
//! (v0.12 nixpkgs pin, v0.16 upstream stable).

pub mod credentials;
pub mod provider;
pub mod protocol;
pub mod uri;

pub use credentials::{
    CredentialValue, CredentialsChain, CredentialsError, ResolverFn,
};
pub use provider::{parse_dotenv_line, strip_inline_comment, DoctorReport, SopsProvider};
pub use uri::{FieldSpec, SopsUri, UriError};

use std::path::Path;
use thiserror::Error;

/// Errors returned by the provider. Mapped into `anyhow::Error` by
/// callers via `Result<_, anyhow::Error>`.
#[derive(Debug, Error)]
pub enum SopsError {
    #[error("`sops` binary not found in PATH; install with `nix profile install nixpkgs#sops`")]
    SopsBinaryMissing,

    #[error("key `{key}` not found in `{file}`")]
    KeyNotFound { file: String, key: String },

    #[error("format `{format}` not supported by the crate (closed set: yaml, json, dotenv, bin)")]
    UnsupportedFormat { format: String },

    #[error("sops decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Resolve a single key from a SOPS-encrypted file. Used by the
/// protocol binary (`src/main.rs`) and by integration tests; not part
/// of the v0.1.0 SecretSpec integration surface (Phase 2 wraps this
/// via the cachix/secretspec#98 protocol).
pub async fn resolve_key(
    file: &str,
    key: &str,
    format_hint: Option<&str>,
) -> Result<String, SopsError> {
    let provider = SopsProvider::new();
    provider.get(file, key, format_hint).await
}

/// Resolve a secret value as raw bytes (binary counterpart to
/// `resolve_key`). For text formats, returns UTF-8 bytes; for `bin`,
/// raw plaintext bytes with no UTF-8 coercion.
///
/// # Examples
///
/// See the full doctest in `provider.rs::SopsProvider::get_bytes` —
/// the surface here mirrors it.
pub async fn resolve_bytes(
    file: &str,
    key: &str,
    format_hint: Option<&str>,
) -> Result<Vec<u8>, SopsError> {
    let provider = SopsProvider::new();
    provider.get_bytes(file, key, format_hint).await
}

/// Best-effort path inference (kept at the lib level for reuse by tests
/// and the binary). Returns the normalized format name; see
/// `infer_format_from_path`.
///
/// # Examples
///
/// ```
/// use secretspec_provider_sops::infer_format_from_path;
///
/// assert_eq!(infer_format_from_path("secrets.yaml"), Some("yaml".to_string()));
/// assert_eq!(infer_format_from_path("secrets.yml"), Some("yaml".to_string()));
/// assert_eq!(infer_format_from_path("data.json"), Some("json".to_string()));
/// assert_eq!(infer_format_from_path(".env"), Some("dotenv".to_string()));
/// assert_eq!(infer_format_from_path(".env.local"), Some("dotenv".to_string()));
/// assert_eq!(infer_format_from_path("data.bin"), Some("bin".to_string()));
/// assert_eq!(infer_format_from_path("no-ext"), None);
/// ```
pub fn infer_format_from_path(file: &str) -> Option<String> {
    let path = Path::new(file);
    // Special-case dotenv dotfiles. `.env` has no extension; `.env.local`,
    // `.env.production` etc. have extension `local`/`production` that
    // isn't useful — basename-only matching handles all of them.
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name == ".env" || name.starts_with(".env.") {
            return Some("dotenv".to_string());
        }
    }
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
        .as_deref()
    {
        Some("yaml") | Some("yml") => Some("yaml".to_string()),
        Some("json") => Some("json".to_string()),
        Some("env") => Some("dotenv".to_string()),
        Some(other) => Some(other.to_string()),
        None => None,
    }
}
