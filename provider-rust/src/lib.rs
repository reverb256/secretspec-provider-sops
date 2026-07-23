//! `secretspec-provider-sops` — Phase 1: SOPS provider CLI shim.
//!
//! Phase 1 wraps `sops --decrypt` with format-aware extraction. Phase 2
//! implements the SecretSpec generic provider interface (consumed via
//! JSON). Phase 3 candidates live behind feature flags.
//!
//! Decoupling: the lib is `secretspec` framework-agnostic. SecretSpec
//! consumes the resolved value through its generic provider interface;
//! the crate does not depend on `secretspec = "0.x"` to avoid the
//! upstream churn (v0.12 nixpkgs pin, v0.16 upstream stable).

pub mod provider;
pub mod secretspec;

pub use provider::{DoctorReport, SopsProvider, parse_dotenv_line, strip_inline_comment};
pub use secretspec::SopsFileProvider;

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

/// Resolve a single key from a SOPS-encrypted file. Used by `main.rs`
/// and by integration tests; not part of the v0.1.0 SecretSpec
/// integration surface (Phase 2 wraps this).
pub async fn resolve_key(
    file: &str,
    key: &str,
    format_hint: Option<&str>,
) -> Result<String, SopsError> {
    let provider = SopsProvider::new();
    provider.get(file, key, format_hint).await
}

/// Resolve a secret value as raw bytes (Phase 2 surface; binary
/// counterpart to `resolve_key`). For text formats, returns the
/// decrypted UTF-8 bytes; for `bin`, returns the raw decrypted
/// bytes with no UTF-8 coercion.
///
/// # Examples
///
/// `resolve_bytes` is the byte-returning parallel of `resolve_key`.
/// Compile-only (`no_run`) because the example requires a real
/// `sops` binary and a real `SOPS_AGE_KEY_FILE`.
///
/// ```
/// # // requires real sops binary + SOPS_AGE_KEY_FILE; `no_run`
/// # // so the doctest compiles but does not execute.
/// use secretspec_provider_sops::{resolve_bytes, SopsError};
/// # async fn example() -> Result<(), SopsError> {
/// // Text path: returns UTF-8 bytes for the resolved key.
/// let _ = resolve_bytes("secrets.yaml", "nvidia_api_key", None).await?;
/// // Bin path: returns raw plaintext bytes; the `key` arg is ignored.
/// let _ = resolve_bytes("secrets.bin", "_unused_for_bin", Some("bin")).await?;
/// # Ok(())
/// # }
/// ```
pub async fn resolve_bytes(
    file: &str,
    key: &str,
    format_hint: Option<&str>,
) -> Result<Vec<u8>, SopsError> {
    let provider = SopsProvider::new();
    provider.get_bytes(file, key, format_hint).await
}

/// Best-effort path inference (kept at the lib level for reuse by tests
/// and the binary).
///
/// Returns a normalized format name so that the `SopsProvider::get`
/// match arms can switch on a small closed set:
///
/// - `.yaml` / `.yml` → `"yaml"`  (yml normalized to yaml)
/// - `.json`         → `"json"`
/// - `.env` (the dotfile basename itself, no extension) and
///   `.env.local` / `.env.production` (dotfile-prefixed basenames)
///                   → `"dotenv"`
/// - `*.env`         → `"dotenv"`  (env extension normalized)
/// - other exts      → returned lowercase (e.g. `"bin"`) so `get()`
///                     emits `UnsupportedFormat { format: "bin" }`
/// - `None` (no ext, not a known dotfile basename) → `None`
///
/// # Examples
///
/// ```
/// use secretspec_provider_sops::infer_format_from_path;
///
/// // YAML (yml normalized to yaml)
/// assert_eq!(infer_format_from_path("secrets.yaml"), Some("yaml".to_string()));
/// assert_eq!(infer_format_from_path("secrets.yml"), Some("yaml".to_string()));
///
/// // JSON
/// assert_eq!(infer_format_from_path("data.json"), Some("json".to_string()));
///
/// // dotenv dotfiles: `.env` (no ext) and `.env.<suffix>` (unhelpful ext)
/// assert_eq!(infer_format_from_path(".env"), Some("dotenv".to_string()));
/// assert_eq!(infer_format_from_path("config.env"), Some("dotenv".to_string()));
/// assert_eq!(infer_format_from_path(".env.production"), Some("dotenv".to_string()));
/// assert_eq!(infer_format_from_path(".env.local"), Some("dotenv".to_string()));
///
/// // Unknown ext is surfaced verbatim lowercase so the caller can emit
/// // a precise `UnsupportedFormat { format: "bin" }` error.
/// assert_eq!(infer_format_from_path("data.bin"), Some("bin".to_string()));
///
/// // No extension, not a known dotfile basename — `None` (caller errors).
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
