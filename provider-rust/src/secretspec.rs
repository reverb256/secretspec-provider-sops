//! Phase 3 of the crate design (sops-provider-design.md → "Phasing"):
//! the SecretSpec-facing provider interface.
//!
//! **Status: SCAFFOLD (2026-07-26).** This module ships a self-contained
//! `SopsFileProvider` struct that delegates to the crate's `resolve_key`
//! and `resolve_bytes` free functions. The concrete `Provider` trait
//! shape + RPC plumbing will be aligned with cachix/secretspec#98
//! ([Secret Provider Protocol v1 draft](https://github.com/cachix/secretspec/pull/98),
//! OPEN as of 2026-07-26) once that PR lands upstream.
//!
//! Until then, the surface here is:
//!
//! - `SopsFileProvider::new(file)`: construct a provider anchored on a
//!   single SOPS file path. The intent is the same as
//!   `secretspec.toml`'s `[providers.sops.secrets.<file>]`
//!   one-file-per-secret shape.
//! - `SopsFileProvider::resolve_text(key, format_hint)`: text-format
//!   per-key extraction (yaml/json/dotenv). Delegates to `resolve_key`.
//! - `SopsFileProvider::resolve_bytes(format_hint)`: binary-format
//!   whole-file extraction (or text bytes if format isn't `bin`).
//!   Delegates to `resolve_bytes`.
//!
//! The crate is intentionally `secretspec` framework-agnostic per
//! `knowledge.md` § "Scope"; we do NOT depend on `secretspec = "0.x"`
//! (that would vendor-lock us to whichever upstream nixpkgs pin is
//! loaded, currently v0.12).

use crate::{resolve_key, SopsError};

/// `SopsFileProvider` — the closure-of-Phase-3 SecretSpec-facing
/// surface. Each instance anchors on a single SOPS-encrypted file
/// path; the methods translate SecretSpec calls into our
/// (`resolve_key`, `resolve_bytes`) primitives.
///
/// TODO(cachix/secretspec#98): once Secret Provider Protocol v1 lands
/// upstream, align this struct's method signatures + RPC shape with
/// the upstream provider-trait verbatim, then re-export the upstream
/// trait as our public surface. Until then, the struct is
/// the closest stable surface we can offer without depending on
/// upstream.
#[derive(Debug, Clone)]
pub struct SopsFileProvider {
    file: String,
}

impl SopsFileProvider {
    /// Create a new `SopsFileProvider` anchored on the given SOPS-
    /// encrypted file path. The file is decrypted lazily on each
    /// `resolve_text` / `resolve_bytes` call — there is no warm-up
    /// cache, matching the CLI shim's per-call invocation pattern.
    pub fn new(file: impl Into<String>) -> Self {
        Self { file: file.into() }
    }

    /// The encrypted file this provider is anchored on.
    pub fn file(&self) -> &str {
        &self.file
    }

    /// Resolve a text-keyed secret from the encrypted file. For
    /// `yaml`/`json`/`dotenv` formats, returns the extracted value
    /// as a `String`.
    ///
    /// Equivalent to calling the lib-level `resolve_key` directly.
    pub async fn resolve_text(
        &self,
        key: &str,
        format_hint: Option<&str>,
    ) -> Result<String, SopsError> {
        resolve_key(&self.file, key, format_hint).await
    }

    /// Resolve a secret value as raw bytes.
    ///
    /// **Bin-mode only (Phase 3 SCAFFOLD):** for `format_hint =
    /// Some("bin")`, returns the whole-file plaintext (no UTF-8
    /// coercion) — the file IS the secret. For other formats, the
    /// upstream `Provider` trait shape from cachix/secretspec#98
    /// will surface a key parameter; until that lands, callers that
    /// need per-key bytes should use `resolve_text` and `.into_bytes()`.
    pub async fn resolve_bytes(
        &self,
        format_hint: Option<&str>,
    ) -> Result<Vec<u8>, SopsError> {
        // The empty-string key is ignored by `extract_bin` (whole-file
        // mode; see module-level docs for why we don't surface a key
        // parameter here yet).
        crate::resolve_bytes(&self.file, "", format_hint).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sops_file_provider_constructs_with_path() {
        let p = SopsFileProvider::new("secrets.yaml");
        assert_eq!(p.file(), "secrets.yaml");
    }

    #[test]
    fn sops_file_provider_accepts_borrowed_str() {
        let path = String::from("secrets.bin");
        let p = SopsFileProvider::new(path.as_str());
        assert_eq!(p.file(), "secrets.bin");
    }

    #[test]
    fn provider_is_cloneable() {
        let p = SopsFileProvider::new("secrets.yaml");
        let p2 = p.clone();
        assert_eq!(p.file(), p2.file());
    }
}
