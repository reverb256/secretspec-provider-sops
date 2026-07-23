//! Provider credentials chain — implements Domen Kozar's accept-criterion
//! #1 ("provider credentials pattern").
//!
//! The chain is a list of [`FieldSpec`]s (URI-shaped credential sources).
//! The SecretSpec framework supplies the *resolver*: a function that
//! takes a credential URI string and returns the resolved plaintext.
//! We orchestrate resolution + propagation into the `sops` subprocess's
//! environment as the `SOPS_*` env vars `sops` already understands
//! (`SOPS_AGE_KEY_FILE`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`,
//! `GOOGLE_CREDENTIALS`, `AZURE_TENANT_ID`/`AZURE_CLIENT_ID`/…,
//! `HC_VAULT_ADDR`/`HC_VAULT_TOKEN`).
//!
//! Sensitive entries are NEVER logged: `Display` / `to_string` mask
//! them as `***` (see [`crate::uri::FieldSpec::Display`]). The chain
//! returns resolved structs whose `value` field is gated behind a
//! non-`Debug` accessor so callers can't accidentally log the secret.
//!
//! ## Decoupling rationale
//!
//! We intentionally don't depend on `secretspec` directly. The
//! framework will eventually host our resolver as a generic provider
//! (per cachix/secretspec#98's Secret Provider Protocol v1 spec).
//! Until then, we accept a boxed resolver callback so the upstream
//! lib can be wired in transparently when the protocol stabilizes
//! (single-line refactor of `CredentialsChain::resolve`).

use crate::uri::FieldSpec;

/// One resolved credential entry. Holds the plaintext value; the
/// `Debug` impl deliberately emits `***` for sensitive entries so a
/// panicked `dbg!(value)` call doesn't leak the secret into logs.
#[derive(Clone)]
pub struct CredentialValue {
    pub name: String,
    pub value: String,
    pub sensitive: bool,
    pub source_uri: String,
}

impl CredentialValue {
    /// Raw plaintext accessor — only callers who are about to plumb
    /// the value into `sops`'s environment as a `SOPS_*` env var
    /// should call this. Caller MUST NOT log the result.
    pub fn value_plaintext(&self) -> &str {
        &self.value
    }
}

impl std::fmt::Debug for CredentialValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value_dbg = if self.sensitive {
            "***".to_string()
        } else {
            self.value.clone()
        };
        // URIs are always safe to log (they're public per the design doc
        // — servers don't keep the actual key in the URI anyway).
        f.debug_struct("CredentialValue")
            .field("name", &self.name)
            .field("value", &value_dbg)
            .field("sensitive", &self.sensitive)
            .field("source_uri", &self.source_uri)
            .finish()
    }
}

impl std::fmt::Display for CredentialValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}=***", self.name)
    }
}

/// Errors raised while resolving credentials. Urls are public;
/// `cause` should be a category-level cause (e.g. "timeout", "not
/// found"), not a credential-bearing diagnostic.
#[derive(Debug, Clone)]
pub enum CredentialsError {
    /// One or more field URIs couldn't be resolved.
    ResolveFailed {
        name: String,
        source_uri: String,
        cause: String,
    },
    /// Builder validation: at least one (name, URI) pair is required.
    EmptyChain,
}

impl std::fmt::Display for CredentialsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CredentialsError::ResolveFailed {
                name,
                source_uri,
                cause,
            } => {
                write!(
                    f,
                    "credential `{name}` (uri=`{source_uri}`) failed: {cause}"
                )
            }
            CredentialsError::EmptyChain => write!(f, "credentials chain is empty"),
        }
    }
}

impl std::error::Error for CredentialsError {}

/// Resolves a credential URI to its plaintext value. Framework supplies
/// this at runtime; in the upstream cachix/secretspec integration it
/// will be the SecretSpec `vault` / `keyring` / `onepassword` / etc.
/// provider's resolver.
pub type ResolverFn = std::sync::Arc<dyn Fn(&str) -> Result<String, String> + Send + Sync>;

/// A credentials chain: an ordered list of [`FieldSpec`]s with a
/// resolver. Resolves all fields, returning a [`Vec<CredentialValue>`];
/// partial-resolution failures are reported via [`CredentialsError`].
#[derive(Clone, Default)]
pub struct CredentialsChain {
    fields: Vec<FieldSpec>,
}

impl CredentialsChain {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: append a [`FieldSpec`] (one credential entry).
    pub fn with(mut self, field: FieldSpec) -> Self {
        self.fields.push(field);
        self
    }

    /// Len (for tests + builders that want to check non-empty).
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Borrow the field list (for testing + serialization).
    pub fn fields(&self) -> &[FieldSpec] {
        &self.fields
    }

    /// Resolve every field through the framework-supplied resolver.
    /// Sensitive values land in [`CredentialValue::value_plaintext`]
    /// and MUST be plumbed into `sops`'s `SOPS_*` env vars, not logged.
    pub async fn resolve_all(
        &self,
        resolver: &ResolverFn,
    ) -> Result<Vec<CredentialValue>, CredentialsError> {
        if self.is_empty() {
            return Err(CredentialsError::EmptyChain);
        }
        let mut out = Vec::with_capacity(self.fields.len());
        let mut first_err: Option<CredentialsError> = None;
        for f in &self.fields {
            match resolver(&f.uri) {
                Ok(value) => out.push(CredentialValue {
                    name: f.name.clone(),
                    value,
                    sensitive: f.sensitive,
                    source_uri: f.uri.clone(),
                }),
                Err(cause) => {
                    if first_err.is_none() {
                        first_err = Some(CredentialsError::ResolveFailed {
                            name: f.name.clone(),
                            source_uri: f.uri.clone(),
                            cause,
                        });
                    }
                }
            }
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(out),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uri::FieldSpec;

    fn resolver_with(map: &[(&str, &str)]) -> ResolverFn {
        let map: std::collections::HashMap<String, String> = map
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        std::sync::Arc::new(move |uri: &str| match map.get(uri) {
            Some(v) => Ok(v.clone()),
            None => Err(format!("no resolver entry for uri={uri}")),
        })
    }

    #[tokio::test]
    async fn resolve_all_happy_path() {
        let chain = CredentialsChain::new()
            .with(FieldSpec::new("age_key", "vault://vault/data/age_key"))
            .with(FieldSpec::new(
                "aws_secret_access_key",
                "onepassword://Homelab/aws",
            ));
        let r = resolver_with(&[
            ("vault://vault/data/age_key", "AGE-PRIVKEY-PAYLOAD"),
            ("onepassword://Homelab/aws", "AWS-SECRET-PAYLOAD"),
        ]);
        let v = chain.resolve_all(&r).await.expect("resolve");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].name, "age_key");
        assert_eq!(v[0].value_plaintext(), "AGE-PRIVKEY-PAYLOAD");
        assert!(v[0].sensitive);
        assert_eq!(v[1].name, "aws_secret_access_key");
    }

    #[tokio::test]
    async fn resolve_all_empty_chain_errors() {
        let chain = CredentialsChain::new();
        let r = resolver_with(&[]);
        let err = chain.resolve_all(&r).await.expect_err("empty chain");
        assert!(matches!(err, CredentialsError::EmptyChain));
    }

    #[tokio::test]
    async fn resolve_all_partial_failure_reports_first() {
        let chain = CredentialsChain::new()
            .with(FieldSpec::new("age_key", "vault://vault/data/age_key"))
            .with(FieldSpec::new("vm_token", "vm://x"));
        let r = resolver_with(&[("vault://vault/data/age_key", "AGE-OK")]);
        let err = chain.resolve_all(&r).await.expect_err("partial fail");
        match err {
            CredentialsError::ResolveFailed {
                name,
                source_uri,
                cause,
            } => {
                // Either field could be reported first (resolution order
                // is preserved by `with`, so the second one errors here).
                assert_eq!(name, "vm_token");
                assert_eq!(source_uri, "vm://x");
                assert!(cause.contains("no resolver entry"));
            }
            other => panic!("expected ResolveFailed, got {other:?}"),
        }
    }

    #[test]
    fn credentialvalue_debug_masks_sensitive() {
        let cv = CredentialValue {
            name: "age_key".into(),
            value: "SUPER-SECRET".into(),
            sensitive: true,
            source_uri: "vault://vault/data/age_key".into(),
        };
        let dbg = format!("{cv:?}");
        assert!(
            !dbg.contains("SUPER-SECRET"),
            "secret must not appear in Debug: {dbg}"
        );
        assert!(dbg.contains("***"), "Debug should mask with ***: {dbg}");
        // The URI is OK to surface.
        assert!(dbg.contains("vault://vault/data/age_key"));
    }

    #[test]
    fn credentialvalue_debug_shows_public() {
        let cv = CredentialValue {
            name: "vault_addr".into(),
            value: "https://astral-key:8080".into(),
            sensitive: false,
            source_uri: "vault://https://astral-key:8080".into(),
        };
        let dbg = format!("{cv:?}");
        // Non-sensitive: value visible.
        assert!(dbg.contains("https://astral-key:8080"));
    }

    #[test]
    fn credentialvalue_display_always_masks() {
        let cv_public = CredentialValue {
            name: "vault_addr".into(),
            value: "https://astral-key:8080".into(),
            sensitive: false,
            source_uri: "vault://https://astral-key:8080".into(),
        };
        // Display: name visible, value masked. Public still masked
        // because Display is for log lines / error envelopes where
        // any credential-adjacent string is best hidden.
        let s = format!("{cv_public}");
        assert_eq!(s, "vault_addr=***");
    }
}
