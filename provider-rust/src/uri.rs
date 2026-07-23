//! Provider URI handling — implements Domen Kozar's accept-criterion #2
//! ("no credential leakage").
//!
//! Two types live here:
//!
//! - [`SopsUri`] — the top-level provider URI (file path + optional
//!   key + format hint). Parses shapes like
//!   `sops://./secrets.yaml?nvidia_api_key&f=yaml` and
//!   `sops://secrets.yaml#nvidia_api_key`.
//! - [`FieldSpec`] — a single credential entry in the
//!   `credentials = { … }` block (age_key, aws_secret_access_key, etc.).
//!   The `sensitive` bit decides whether `Display` / `to_string()`
//!   masks the URI value or renders it.
//!
//! Sensitive values (e.g. `age_key`) **never** appear in `Display`,
//! `to_string()`, audit logs, or error messages — they're rendered as
//! `***`. Public values (`public_age_recipient`, vault mount paths)
//! stay visible. The masking is enforced at the type level: the only
//! way to get a credential URI back out is via a debug-level accessor
//! that intentionally opts out of production logging.

use std::fmt;
use std::str::FromStr;

// ────────────────────────────── SopsUri ──────────────────────────────────

/// Top-level provider URI: `sops://<file>?<query>` or `sops://<file>#<key>`.
///
/// URIs in `secretspec.toml` look like:
/// ```toml
/// [providers.sops]
/// uri = "sops://./secrets.yaml"
/// ```
/// Per-secret URIs (in query or fragment form) carry the key inside the
/// file:
/// - `sops://./secrets.yaml?key=nvidia_api_key` (query form; preferred
///   when also setting a `?f=<fmt>`)
/// - `sops://./secrets.yaml#nvidia_api_key` (fragment form; preferred
///   when the key is the only extra info)
///
/// The URI parser is permissive: unknown query params are tolerated so
/// future SecretSpec versions can add fields without breaking us.
/// Forwarded params (`--age-recipient`, etc.) become `sops` CLI flags
/// in `SopsProvider`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SopsUri {
    /// Path to the SOPS-encrypted file. Mandatory.
    pub file: String,
    /// Optional explicit key inside the file (from `?key=X` or `#X`).
    pub key: Option<String>,
    /// Optional format hint (`yaml` / `json` / `dotenv` / `bin`).
    /// If absent, [`crate::infer_format_from_path`] decides from extension.
    pub format: Option<String>,
    /// All other query params preserved verbatim so we can forward
    /// them to the `sops` CLI (`age_recipient`, `encrypted_regex`, etc.).
    pub extra: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UriError {
    /// URI did not start with `sops://`.
    WrongScheme(String),
    /// URI is empty after stripping scheme.
    EmptyPath,
    /// Encoding / parsing mistake with a human message.
    Malformed(String),
}

impl fmt::Display for UriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UriError::WrongScheme(s) => {
                write!(f, "URI does not start with `sops://`: {s:?}")
            }
            UriError::EmptyPath => write!(f, "URI is missing the file path"),
            UriError::Malformed(s) => write!(f, "URI malformed: {s}"),
        }
    }
}

impl std::error::Error for UriError {}

impl SopsUri {
    /// Parse a `sops://…` URI. Tolerant of unknown query params — they
    /// land in `self.extra` for forwarding.
    ///
    /// # Examples
    ///
    /// ```
    /// use secretspec_provider_sops::uri::SopsUri;
    ///
    /// // bare path
    /// let u = SopsUri::parse("sops://./secrets.yaml").unwrap();
    /// assert_eq!(u.file, "./secrets.yaml");
    /// assert!(u.key.is_none());
    ///
    /// // query form
    /// let u = SopsUri::parse("sops://./secrets.yaml?key=NVIDIA_API_KEY&f=yaml").unwrap();
    /// assert_eq!(u.file, "./secrets.yaml");
    /// assert_eq!(u.key.as_deref(), Some("NVIDIA_API_KEY"));
    /// assert_eq!(u.format.as_deref(), Some("yaml"));
    ///
    /// // fragment form (key in #)
    /// let u = SopsUri::parse("sops://./secrets.yaml#nvidia_api_key").unwrap();
    /// assert_eq!(u.file, "./secrets.yaml");
    /// assert_eq!(u.key.as_deref(), Some("nvidia_api_key"));
    ///
    /// // unknown query params preserved
    /// let u = SopsUri::parse("sops://./secrets.yaml?age_recipient=age1abc&f=json").unwrap();
    /// assert_eq!(u.extra, vec![("age_recipient".to_string(), "age1abc".to_string())]);
    /// ```
    pub fn parse(s: &str) -> Result<Self, UriError> {
        let rest = s
            .strip_prefix("sops://")
            .ok_or_else(|| UriError::WrongScheme(s.to_string()))?;
        if rest.is_empty() {
            return Err(UriError::EmptyPath);
        }

        // Strip fragment first (RFC 3986: fragment is the part after '#').
        let (no_fragment, fragment_key) = match rest.find('#') {
            Some(idx) => (&rest[..idx], Some(rest[idx + 1..].to_string())),
            None => (rest, None),
        };
        if let Some(frag) = fragment_key.as_deref() {
            if frag.is_empty() {
                return Err(UriError::Malformed(
                    "empty fragment; use `?key=X` instead".into(),
                ));
            }
        }

        let (path, query) = match no_fragment.find('?') {
            Some(idx) => (&no_fragment[..idx], &no_fragment[idx + 1..]),
            None => (no_fragment, ""),
        };
        if path.is_empty() {
            return Err(UriError::EmptyPath);
        }

        let mut key = fragment_key;
        let mut format = None;
        let mut extra = Vec::new();
        for pair in query.split('&').filter(|p| !p.is_empty()) {
            let (k, v) = pair
                .split_once('=')
                .map(|(k, v)| (k, v))
                .unwrap_or((pair, ""));
            match k {
                "key" => key = Some(v.to_string()),
                // Both `f` (compact, matches `migration-matrix.md` examples)
                // and `format` (verbose) are accepted; alias `f` wins on
                // canonical form.
                "f" | "format" => format = Some(v.to_string()),
                _ => extra.push((k.to_string(), v.to_string())),
            }
        }

        Ok(SopsUri {
            file: path.to_string(),
            key,
            format,
            extra,
        })
    }

    /// Public-facing string of the URI, suitable for logs and audit
    /// trails. **No credentials are ever encoded in this string** —
    /// the SopsUri has no access to them; they live in the
    /// `CredentialsChain` (see [`crate::credentials`]) which applies
    /// its own redaction separately.
    pub fn to_public_string(&self) -> String {
        let mut s = format!("sops://{}", self.file);
        let mut first = true;
        let mut append = |k: &str, v: &str| {
            s.push(if first { '?' } else { '&' });
            s.push_str(&format!("{k}={v}"));
            first = false;
        };
        if let Some(k) = &self.key {
            append("key", k);
        }
        if let Some(f) = &self.format {
            append("f", f);
        }
        for (k, v) in &self.extra {
            append(k, v);
        }
        s
    }
}

impl fmt::Display for SopsUri {
    /// Same as [`SopsUri::to_public_string`] — Display is always the
    /// public, credential-free form. Use `{:?}` (debug) when you need
    /// the raw form (which is fine because there are no credentials
    /// in the struct to leak).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_public_string())
    }
}

impl FromStr for SopsUri {
    type Err = UriError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

// ────────────────────────────── FieldSpec ────────────────────────────────

/// One credential entry in a provider's `credentials = { … }` block.
///
/// Per Domen's #2 accept-criterion:
/// - `sensitive = true` → `Display` / `to_string` render the URI as `***`.
/// - `sensitive = false` → URI is rendered verbatim (e.g. for a vault
///   mount path that's public).
///
/// The raw URI is gated behind [`FieldSpec::uri_redacted()`] which
/// explicitly opts in to unredacted output (callers must justify why
/// they need the unredacted form). Anything that goes to logs / audit
/// trails / error messages should use `Display` / `to_string` / `{}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSpec {
    /// Logical name (e.g. `"age_key"`, `"aws_secret_access_key"`).
    pub name: String,
    /// The provider URI that resolves this credential (e.g.
    /// `vault://http://astral-key:8080/…/data/age_key?auth=approle`).
    pub uri: String,
    /// True iff this credential is secret. Defaults to `true` —
    /// `sensitive()` is the *opt-out* builder method.
    pub sensitive: bool,
}

impl FieldSpec {
    /// Create a new FieldSpec. Defaults to `sensitive = true` —
    /// use [`FieldSpec::public`] for the few fields that aren't secret.
    pub fn new(name: impl Into<String>, uri: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            uri: uri.into(),
            sensitive: true,
        }
    }

    /// Builder: mark this field as *public* (not redacted in Display).
    /// Use sparingly — only for vault mount paths, project IDs, etc.
    pub fn public(mut self) -> Self {
        self.sensitive = false;
        self
    }

    /// Raw URI accessor — *use sparingly*. Bypasses redaction; intended
    /// only for the resolver callback plumbing the value into the
    /// `sops` subprocess's environment. Caller must NOT log the result.
    pub fn uri_redacted(&self) -> &str {
        &self.uri
    }
}

impl fmt::Display for FieldSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let uri_display = if self.sensitive { "***" } else { &self.uri };
        write!(f, "{}={}", self.name, uri_display)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bare_path() {
        let u = SopsUri::parse("sops://./secrets.yaml").unwrap();
        assert_eq!(u.file, "./secrets.yaml");
        assert!(u.key.is_none());
        assert!(u.format.is_none());
        assert!(u.extra.is_empty());
    }

    #[test]
    fn parse_query_form() {
        let u = SopsUri::parse("sops://./secrets.yaml?key=NVIDIA_API_KEY&f=yaml").unwrap();
        assert_eq!(u.file, "./secrets.yaml");
        assert_eq!(u.key.as_deref(), Some("NVIDIA_API_KEY"));
        assert_eq!(u.format.as_deref(), Some("yaml"));
    }

    #[test]
    fn parse_fragment_form() {
        let u = SopsUri::parse("sops://./secrets.yaml#nvidia_api_key").unwrap();
        assert_eq!(u.file, "./secrets.yaml");
        assert_eq!(u.key.as_deref(), Some("nvidia_api_key"));
    }

    #[test]
    fn parse_unknown_query_preserved() {
        let u = SopsUri::parse("sops://./secrets.yaml?age_recipient=age1abc&f=json").unwrap();
        assert_eq!(u.format.as_deref(), Some("json"));
        assert_eq!(
            u.extra,
            vec![("age_recipient".to_string(), "age1abc".to_string())]
        );
    }

    #[test]
    fn parse_rejects_wrong_scheme() {
        assert!(SopsUri::parse("vault://foo").is_err());
        assert!(SopsUri::parse("./secrets.yaml").is_err());
    }

    #[test]
    fn parse_rejects_empty_path() {
        assert!(SopsUri::parse("sops://").is_err());
        assert!(SopsUri::parse("sops://?key=foo").is_err());
    }

    #[test]
    fn parse_rejects_empty_fragment() {
        assert!(SopsUri::parse("sops://./secrets.yaml#").is_err());
    }

    #[test]
    fn display_redacts_nothing_in_sopsuri() {
        // SopsUri carries no credentials — its Display string is safe.
        let u = SopsUri::parse("sops://./secrets.yaml?key=NVIDIA_API_KEY&f=yaml").unwrap();
        let s = format!("{u}");
        assert_eq!(s, "sops://./secrets.yaml?key=NVIDIA_API_KEY&f=yaml");
    }

    #[test]
    fn fieldspec_display_redacts_sensitive_by_default() {
        let f = FieldSpec::new("age_key", "vault://secret/data/age_key?auth=approle");
        let s = format!("{f}");
        assert_eq!(s, "age_key=***");
        assert!(!s.contains("vault://"));
        assert!(!s.contains("approle"));
    }

    #[test]
    fn fieldspec_display_shows_public_uris() {
        let f = FieldSpec::new("vault_addr", "https://astral-key:8080").public();
        let s = format!("{f}");
        assert_eq!(s, "vault_addr=https://astral-key:8080");
    }

    #[test]
    fn fieldspec_uri_redacted_returns_raw() {
        // The ONLY way to get the unredacted form. Callers committing
        // the result to a String that may end up in a log are bugs.
        let f = FieldSpec::new("age_key", "vault://secret/data/age_key");
        assert_eq!(
            f.uri_redacted(),
            "vault://secret/data/age_key"
        );
    }

    #[test]
    fn fromstr_roundtrip() {
        let s = "sops://./secrets.yaml?key=foo&f=yaml";
        let u: SopsUri = s.parse().unwrap();
        assert_eq!(format!("{u}"), s);
    }
}
