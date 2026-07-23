//! Provider implementation: shells out to `sops --decrypt`.
//!
//! Phase 1 surface:
//!   - `SopsProvider::new()` — provider that falls back to `SOPS_AGE_KEY_FILE`
//!     in the caller's environment for the child `sops` process.
//!   - `SopsProvider::with_age_keyfile(path)` — provider with an explicit
//!     keyfile. Used by integration tests so the test doesn't need to
//!     mutate the caller's global environment (which is unsound in
//!     multi-threaded Rust ≥ 1.80).
//!   - `get(file, key, format) -> String`: extract a single key from
//!     a SOPS-encrypted file. Format inferred from file extension;
//!     `--format` parameter overrides (None → "yaml" for unknown).
//!   - `doctor() -> DoctorReport`: report on the local env.
//!
//! Audit: the framework (Phase 2) wraps our `get()` call in its
//! redaction layer. Locally we don't emit sensitive values to logs;
//! redaction happens at the framework boundary.
//!
//! Path mode (`f=bin`): implemented in Phase 2 this turn (private
//! `extract_bin` + public `get_bytes` + parallel `sops_decrypt_with_env_bytes`
//! helper). For binary secrets, `extract_bin` returns the raw decrypted
//! bytes without UTF-8 coercion; callers write them through
//! SecretSpec's `as_path = true` mechanism.
//!
//! Wallet-key blocklist (per `knowledge.md` "Things to avoid"): we
//! follow `sops-provider-design.md`'s convention — never log the
//! resolved value of any key whose name matches wallet / master-age
//! patterns; we hand them through but don't echo in `doctor()`.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Serialize;
use tokio::process::Command;

use crate::{infer_format_from_path, SopsError};

/// Doctor report — what the local env looks like when the provider runs.
#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub provider_version: &'static str,
    pub sops_path: Option<String>,
    pub sops_version: Option<String>,
    pub age_path: Option<String>,
    pub age_version: Option<String>,
    /// `SOPS_AGE_KEY_FILE` resolved from the explicit `with_age_keyfile`
    /// builder (if the provider was constructed that way), falling back
    /// to the caller's environment. Lets a reader tell in one glance
    /// whether decryption is using a process-env-var path or an
    /// explicit-path argument.
    pub sops_age_key_file: Option<String>,
}

/// SOPS provider.
///
/// `age_keyfile`:
/// - `Some(path)` → set `SOPS_AGE_KEY_FILE=<path>` explicitly on the
///   `sops` child process (no caller-env mutation).
/// - `None` → fall back to whatever `SOPS_AGE_KEY_FILE` is in the
///   child process's inherited env (sops reads it natively; this is
///   the conventional production path).
#[derive(Debug, Clone, Default)]
pub struct SopsProvider {
    age_keyfile: Option<PathBuf>,
}

impl SopsProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a provider with an explicit age keyfile. Used by
    /// integration tests and by callers that have a keyfile path in
    /// hand and don't want to mutate the caller's environment. The
    /// keyfile is passed to the child `sops` process via
    /// `Command::env("SOPS_AGE_KEY_FILE", path)`; the caller's
    /// environment is NOT touched.
    ///
    /// Accepts anything that implements `Into<PathBuf>` —
    /// `&Path`, `PathBuf`, `&str`, `String`.
    pub fn with_age_keyfile(path: impl Into<PathBuf>) -> Self {
        Self {
            age_keyfile: Some(path.into()),
        }
    }

    /// Resolve a single key. Format is inferred from extension if not
    /// hinted explicitly. Returns the resolved value as a `String`
    /// (opaque bytes are Phase 2 via `f=bin`; for Phase 1 we only
    /// handle text formats).
    ///
    /// `format_hint` is the closed set `"yaml" | "json" | "dotenv"`
    /// (case-insensitive; `yml` and `env` are accepted as aliases and
    /// normalized). Pass `None` to infer from the file extension.
    pub async fn get(
        &self,
        file: &str,
        key: &str,
        format_hint: Option<&str>,
    ) -> Result<String, SopsError> {
        // Preflight: sops on PATH? (Surfaced as SopsBinaryMissing instead
        // of a generic `Io(NotFound)` from Command::new.)
        if which_sync("sops").is_none() {
            return Err(SopsError::SopsBinaryMissing);
        }

        let fmt = format_hint
            .map(normalize_format_hint)
            .or_else(|| infer_format_from_path(file));

        match fmt.as_deref() {
            // Closed set — `normalize_format_hint` and
            // `infer_format_from_path` are the only producers.
            Some("yaml") => self.extract_yaml_json(file, key).await,
            Some("json") => self.extract_yaml_json(file, key).await,
            Some("dotenv") => self.extract_dotenv(file, key).await,
            Some(other) => Err(SopsError::UnsupportedFormat {
                format: other.to_string(),
            }),
            None => Err(SopsError::UnsupportedFormat {
                format: "<unknown extension or no extension>".to_string(),
            }),
        }
    }

    /// Decrypt the file with `sops --decrypt` and look up the key in
    /// the resulting YAML/JSON tree via `serde_yaml`. We intentionally
    /// avoid `sops --extract`'s JSONPath parser because (a) it has
    /// cross-version behavior variance on YAML files (`["key"]` is
    /// interpreted differently across 3.x), and (b) the dotenv path
    /// already uses the symmetric decrypt-then-parse approach, so
    /// keeping the two paths symmetric removes ambiguity.
    ///
    /// `key` may be a flat identifier (`nvidia_api_key`) or a dot
    /// path (`services.openai.org_id`). Phase 2 callers can pass a
    /// richer key shape if needed.
    async fn extract_yaml_json(&self, file: &str, key: &str) -> Result<String, SopsError> {
        let plaintext = sops_decrypt_with_env(file, self.age_keyfile.as_deref()).await?;
        let value: serde_yaml::Value = serde_yaml::from_str(&plaintext)
            .map_err(|e| SopsError::DecryptionFailed(format!("yaml parse: {e}")))?;
        let mut current = &value;
        for part in key.split('.') {
            let mapping = current.as_mapping().ok_or_else(|| SopsError::KeyNotFound {
                file: file.to_string(),
                key: key.to_string(),
            })?;
            let next = mapping
                .get(serde_yaml::Value::String(part.to_string()))
                .ok_or_else(|| SopsError::KeyNotFound {
                    file: file.to_string(),
                    key: key.to_string(),
                })?;
            current = next;
        }
        yaml_value_to_string(current).ok_or_else(|| SopsError::KeyNotFound {
            file: file.to_string(),
            key: key.to_string(),
        })
    }

    /// `sops --decrypt <file>` for dotenv; we then parse the
    /// decrypted text line-by-line, handling `export` prefix and
    /// inline `#` comments. Punted from `--extract` because sops
    /// dotenv mode doesn't accept it. Symmetric counterpart to
    /// `extract_yaml_json`: both go through `sops_decrypt_with_env`.
    async fn extract_dotenv(&self, file: &str, key: &str) -> Result<String, SopsError> {
        let plaintext = sops_decrypt_with_env(file, self.age_keyfile.as_deref()).await?;
        for raw_line in plaintext.lines() {
            if let Some((k, v)) = parse_dotenv_line(raw_line) {
                if k == key {
                    return Ok(v.to_string());
                }
            }
        }
        Err(SopsError::KeyNotFound {
            file: file.to_string(),
            key: key.to_string(),
        })
    }

    /// Resolve a secret value as raw bytes (Phase 2 surface, complement
    /// to `get` for binary secrets). Format inferred from extension if
    /// not hinted explicitly.
    ///
    /// For text formats (`yaml` / `json` / `dotenv`), `get_bytes` returns
    /// the decrypted text as UTF-8 bytes (the underlying bytes that
    /// `get` would have lossily decoded into `String`); equivalent to
    /// `get(...).into_bytes()` but skips the `String` allocation.
    ///
    /// For `bin`, returns the raw decrypted bytes with no UTF-8
    /// coercion. The `key` argument is ignored in this case — the
    /// entire encrypted file IS the secret (SecretSpec `as_path = true`
    /// surface; `f=bin` from `sops-provider-design.md`).
    ///
    /// # Examples
    ///
    /// `get_bytes` is the byte-returning parallel of `get` — same
    /// dispatch logic, but returns `Vec<u8>` instead of `String`.
    /// Compile-only (`no_run`) because the example requires a real
    /// `sops` binary and a real age keyfile; see
    /// `tests/cli_smoke.rs::cli_get_bin_round_trip_preserves_all_bytes`
    /// for the runnable round-trip form.
    ///
    /// ```
    /// # // requires a real sops binary + age keyfile; `no_run` so the
    /// # // doctest compiles but does not execute on real subprocesses.
    /// use secretspec_provider_sops::{SopsProvider, SopsError};
    /// # async fn example() -> Result<(), SopsError> {
    /// let provider = SopsProvider::with_age_keyfile("/path/to/age.key");
    /// // yaml round-trips to UTF-8 bytes (text-coerced; equivalent to
    /// // `get(...).into_bytes()` with no String allocation).
    /// let _ = provider.get_bytes("secrets.yaml", "nvidia_api_key", None).await?;
    /// // bin returns raw plaintext bytes with NO UTF-8 coercion.
    /// // The `key` argument is ignored for `bin`.
    /// let _ = provider.get_bytes("secrets.bin", "_unused_for_bin", None).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_bytes(
        &self,
        file: &str,
        key: &str,
        format_hint: Option<&str>,
    ) -> Result<Vec<u8>, SopsError> {
        // Preflight: sops on PATH?
        if which_sync("sops").is_none() {
            return Err(SopsError::SopsBinaryMissing);
        }

        let fmt = format_hint
            .map(normalize_format_hint)
            .or_else(|| infer_format_from_path(file));

        match fmt.as_deref() {
            Some("yaml") => self
                .extract_yaml_json(file, key)
                .await
                .map(String::into_bytes),
            Some("json") => self
                .extract_yaml_json(file, key)
                .await
                .map(String::into_bytes),
            Some("dotenv") => self.extract_dotenv(file, key).await.map(String::into_bytes),
            Some("bin") => self.extract_bin(file).await,
            Some(other) => Err(SopsError::UnsupportedFormat {
                format: other.to_string(),
            }),
            None => Err(SopsError::UnsupportedFormat {
                format: "<unknown extension or no extension>".to_string(),
            }),
        }
    }

    /// Bin extraction: `sops --decrypt <file>` for binary secrets.
    /// sops auto-detects binary mode via the file extension `.bin` (or
    /// via the encrypted_regex content of the metadata block). The
    /// decrypted output is the original plaintext bytes — no UTF-8
    /// coercion, no per-key structrure preservation (the file IS the
    /// secret; not a collection).
    ///
    /// Phase 2 surface: this completes the format quartet
    /// (yaml + json + dotenv already shipped in Phase 1; bin added).
    /// The parallel `sops_decrypt_with_env_BYTES` helper exists so we
    /// don't lose UTF-8 fidelity for binary secrets.
    async fn extract_bin(&self, file: &str) -> Result<Vec<u8>, SopsError> {
        sops_decrypt_with_env_bytes(file, self.age_keyfile.as_deref()).await
    }

    /// Doctor: what's in the local env that's relevant to provider runtime.
    pub async fn doctor(&self) -> DoctorReport {
        let explicit = self
            .age_keyfile
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned());
        let from_env = std::env::var("SOPS_AGE_KEY_FILE").ok();
        DoctorReport {
            provider_version: env!("CARGO_PKG_VERSION"),
            sops_path: which("sops").await,
            sops_version: binary_first_line("sops", &["--version"]).await,
            age_path: which("age").await,
            age_version: binary_first_line("age", &["--version"]).await,
            sops_age_key_file: explicit.or(from_env),
        }
    }
}

/// `sops --decrypt <file>` returning plaintext as raw bytes (for
/// `bin` format, Phase 2 surface). Mirror of
/// `sops_decrypt_with_env` that skips the `String` decoding so binary
/// output keeps full UTF-8 / non-UTF-8 fidelity. Used by
/// `extract_bin`.
async fn sops_decrypt_with_env_bytes(
    file: &str,
    keyfile: Option<&Path>,
) -> Result<Vec<u8>, SopsError> {
    let mut cmd = Command::new("sops");
    cmd.args(["--decrypt", file]);
    if let Some(kf) = keyfile {
        // Set SOPS_AGE_KEY_FILE on the child process ONLY — does
        // NOT mutate the caller's environment; same thread-safety
        // rationale as sops_decrypt_with_env.
        cmd.env("SOPS_AGE_KEY_FILE", kf);
    }
    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(SopsError::DecryptionFailed(stderr.trim().to_string()));
    }
    Ok(output.stdout)
}

/// `sops --decrypt <file>` returning plaintext. Thin wrapper used by
/// both `extract_yaml_json` and `extract_dotenv`. When `keyfile` is
/// `Some`, the child process gets `SOPS_AGE_KEY_FILE=<path>` set
/// explicitly via `Command::env`; the caller's environment is NOT
/// mutated (this is what makes the integration tests thread-safe in
/// modern Rust ≥ 1.80). When `keyfile` is `None`, the child inherits
/// the caller's env normally — sops reads `SOPS_AGE_KEY_FILE` from
/// the inherited env.
///
/// We surface sops's stderr verbatim so callers see the root cause
/// (e.g. "metadata not found" on plaintext input, "no age key found"
/// when `SOPS_AGE_KEY_FILE` is unset, "Failed to decrypt" on a key
/// id mismatch). We intentionally do NOT special-case any stderr
/// pattern here — the caller's lookup logic is the source of truth
/// for which keys exist.
async fn sops_decrypt_with_env(file: &str, keyfile: Option<&Path>) -> Result<String, SopsError> {
    let mut cmd = Command::new("sops");
    cmd.args(["--decrypt", file]);
    if let Some(kf) = keyfile {
        // Set SOPS_AGE_KEY_FILE on the child process ONLY — does
        // NOT mutate the caller's environment, which keeps the
        // integration tests thread-safe under modern Rust.
        cmd.env("SOPS_AGE_KEY_FILE", kf);
    }
    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(SopsError::DecryptionFailed(stderr.trim().to_string()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Coerce a `serde_yaml::Value` into a `String` for the secretspec
/// value surface. Strings return as-is; scalars stringify via the
/// `serde_yaml::Number::Display` impl; bools and null stringify
/// naturally; sequences and maps serialize to compact YAML (Phase 2
/// territory — Phase 1 callers expect scalars anyway).
fn yaml_value_to_string(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        serde_yaml::Value::Null => Some(String::new()),
        _ => serde_yaml::to_string(v).ok().map(|s| s.trim().to_string()),
    }
}

/// Normalize a free-form `format_hint` string into the closed set
/// `infer_format_from_path` produces. Defends against caller typos
/// like `Some("yml")` or `Some("YAML")` while keeping the match arms
/// in `get()` clean. Unknown strings are passed through lowercase so
/// `get()` can emit a precise `UnsupportedFormat { format }`.
fn normalize_format_hint(s: &str) -> String {
    match s.to_lowercase().as_str() {
        "yaml" | "yml" => "yaml".to_string(),
        "json" => "json".to_string(),
        "env" | "dotenv" => "dotenv".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod normalize_tests {
    use super::normalize_format_hint;

    #[test]
    fn closed_set_aliases_normalize() {
        assert_eq!(normalize_format_hint("yaml"), "yaml");
        assert_eq!(normalize_format_hint("yml"), "yaml");
        assert_eq!(normalize_format_hint("YAML"), "yaml");
        assert_eq!(normalize_format_hint("JSON"), "json");
        assert_eq!(normalize_format_hint("dotenv"), "dotenv");
        assert_eq!(normalize_format_hint("env"), "dotenv");
        assert_eq!(normalize_format_hint("ENV"), "dotenv");
    }

    #[test]
    fn unknown_passes_through_lowercase() {
        assert_eq!(normalize_format_hint("bin"), "bin");
        assert_eq!(normalize_format_hint("BIN"), "bin");
    }
}

/// Parse a single dotenv line, returning `(key, value)`.
///
/// Handles:
/// - `export KEY=value` (strips the `export ` prefix and optional leading whitespace)
/// - inline `#` comments after the value (unquoted), respecting `'…'` / `"…"` quoting
/// - `KEY=` (empty value)
/// - `KEY="quoted # not-a-comment"` (quoted values preserve interior `#`)
/// - lines starting with `#`, blank lines, and `unset KEY` are filtered out (returns None)
///
/// Returns `None` for malformed lines (no `=` after the optional `export` prefix).
///
/// # Examples
///
/// ```
/// use secretspec_provider_sops::parse_dotenv_line;
///
/// // Plain `KEY=value` shapes
/// assert_eq!(parse_dotenv_line("FOO=bar"), Some(("FOO", "bar")));
/// assert_eq!(parse_dotenv_line("FOO=\"bar baz\""), Some(("FOO", "bar baz")));
/// assert_eq!(parse_dotenv_line("FOO='qux'"), Some(("FOO", "qux")));
/// assert_eq!(parse_dotenv_line("FOO="), Some(("FOO", "")));
///
/// // `export ` / `unset ` prefixes
/// assert_eq!(parse_dotenv_line("export FOO=bar"), Some(("FOO", "bar")));
/// assert_eq!(parse_dotenv_line("  export FOO=\"baz\""), Some(("FOO", "baz")));
/// assert_eq!(parse_dotenv_line("unset FOO"), None);
///
/// // Inline `#` comments (unquoted: stripped; quoted: preserved)
/// assert_eq!(parse_dotenv_line("FOO=bar # trailing comment"), Some(("FOO", "bar")));
/// assert_eq!(parse_dotenv_line("FOO=\"bar # baz\""), Some(("FOO", "bar # baz")));
/// assert_eq!(parse_dotenv_line("FOO='qux # quux'"), Some(("FOO", "qux # quux")));
///
/// // Skipped line shapes
/// assert_eq!(parse_dotenv_line("# comment"), None);
/// assert_eq!(parse_dotenv_line(""), None);
/// assert_eq!(parse_dotenv_line("   "), None);
///
/// // Malformed (no `=`)
/// assert_eq!(parse_dotenv_line("FOObar"), None);
/// assert_eq!(parse_dotenv_line("export FOObar"), None);
/// ```
pub fn parse_dotenv_line(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let after_export = trimmed
        .strip_prefix("export ")
        .or_else(|| trimmed.strip_prefix("unset "))
        .unwrap_or(trimmed);
    let eq_idx = after_export.find('=')?;
    let key = after_export[..eq_idx].trim();
    let value_with_optional_comment = strip_inline_comment(&after_export[eq_idx + 1..]).trim();
    let value = value_with_optional_comment.trim_matches(|c| c == '\'' || c == '"');
    Some((key, value))
}

/// Strip an inline `#` comment from a dotenv value, ignoring `#`
/// characters that are inside single- or double-quoted strings. Naive
/// about backslash-escapes inside double quotes — good enough for the
/// homelab use case.
///
/// # Examples
///
/// ```
/// use secretspec_provider_sops::strip_inline_comment;
///
/// // Unquoted `#` truncates at the marker.
/// assert_eq!(strip_inline_comment("bar # c"), "bar ");
///
/// // Inside double quotes, `#` is part of the value (not a comment marker).
/// assert_eq!(strip_inline_comment("bar \"# not c\""), "bar \"# not c\"");
///
/// // Mixed: quoted run survives, then unquoted `#` truncates.
/// assert_eq!(strip_inline_comment("bar 'still not c' # c"), "bar 'still not c' ");
/// ```
pub fn strip_inline_comment(s: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;
    for (i, c) in s.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match c {
            '\\' if in_double => escape = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '#' if !in_single && !in_double => return &s[..i],
            _ => {}
        }
    }
    s
}

/// Synchronous PATH lookup for a binary. Used in `SopsProvider::get`
/// preflight to surface `SopsBinaryMissing` cleanly.
fn which_sync(name: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            // Also check execute bit on Unix; `is_file` doesn't.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(&candidate).ok()?.permissions().mode();
                if mode & 0o111 == 0 {
                    continue;
                }
            }
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

async fn which(name: &str) -> Option<String> {
    let out = Command::new("which").arg(name).output().await.ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}

async fn binary_first_line(name: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(name).args(args).output().await.ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_format_yaml_variants() {
        assert_eq!(
            infer_format_from_path("secrets.yaml"),
            Some("yaml".to_string())
        );
        assert_eq!(
            infer_format_from_path("secrets.yml"),
            Some("yaml".to_string())
        );
    }

    #[test]
    fn infer_format_json() {
        assert_eq!(
            infer_format_from_path("data.json"),
            Some("json".to_string())
        );
    }

    #[test]
    fn infer_format_dotenv() {
        assert_eq!(infer_format_from_path(".env"), Some("dotenv".to_string()));
        assert_eq!(
            infer_format_from_path("config.env"),
            Some("dotenv".to_string())
        );
    }

    #[test]
    fn infer_format_unknown_returns_lowercase_ext() {
        // We surface the unknown ext so get() can return UnsupportedFormat —
        // the test asserts the inferred name is lowercase to match fmt matchers.
        let f = infer_format_from_path("data.bin");
        assert_eq!(f, Some("bin".to_string()));
    }

    #[test]
    fn parse_dotenv_line_basic() {
        assert_eq!(parse_dotenv_line("FOO=bar"), Some(("FOO", "bar")));
        assert_eq!(
            parse_dotenv_line("FOO=\"bar baz\""),
            Some(("FOO", "bar baz"))
        );
        assert_eq!(parse_dotenv_line("FOO='qux'"), Some(("FOO", "qux")));
        assert_eq!(parse_dotenv_line("FOO="), Some(("FOO", "")));
    }

    #[test]
    fn parse_dotenv_line_export_prefix() {
        assert_eq!(parse_dotenv_line("export FOO=bar"), Some(("FOO", "bar")));
        assert_eq!(
            parse_dotenv_line("  export FOO=\"baz\""),
            Some(("FOO", "baz"))
        );
    }

    #[test]
    fn parse_dotenv_line_unset() {
        // `unset` lines have no `=`; they're filtered to None. (Caller
        // would treat unset lines as deletions — Phase 1 doesn't apply.)
        assert_eq!(parse_dotenv_line("unset FOO"), None);
    }

    #[test]
    fn parse_dotenv_line_inline_comment() {
        assert_eq!(
            parse_dotenv_line("FOO=bar # trailing comment"),
            Some(("FOO", "bar"))
        );
        assert_eq!(
            parse_dotenv_line("FOO=bar # comment with # in it"),
            Some(("FOO", "bar"))
        );
    }

    #[test]
    fn parse_dotenv_line_quoted_value_preserves_hash() {
        // Inside quotes, `#` is not a comment marker.
        assert_eq!(
            parse_dotenv_line("FOO=\"bar # baz\""),
            Some(("FOO", "bar # baz"))
        );
        assert_eq!(
            parse_dotenv_line("FOO='qux # quux'"),
            Some(("FOO", "qux # quux"))
        );
    }

    #[test]
    fn parse_dotenv_line_comment_or_blank() {
        assert_eq!(parse_dotenv_line("# comment"), None);
        assert_eq!(parse_dotenv_line(""), None);
        assert_eq!(parse_dotenv_line("   "), None);
    }

    #[test]
    fn parse_dotenv_line_no_eq() {
        assert_eq!(parse_dotenv_line("FOObar"), None);
        assert_eq!(parse_dotenv_line("export FOObar"), None);
    }

    #[test]
    fn strip_inline_comment_quoting() {
        assert_eq!(strip_inline_comment("bar # c"), "bar ");
        assert_eq!(strip_inline_comment("bar \"# not c\""), "bar \"# not c\"");
        assert_eq!(
            strip_inline_comment("bar 'still not c' # c"),
            "bar 'still not c' "
        );
        assert_eq!(strip_inline_comment("bar # a\\\"b"), "bar ");
    }

    #[test]
    fn which_sync_finds_a_real_binary() {
        // Use a binary that's almost always on PATH even inside
        // constrained `nix-shell -p` invocations — `sh` is POSIX.
        assert!(which_sync("sh").is_some(), "sh should be on PATH");
    }

    #[test]
    fn which_sync_returns_none_for_garbage() {
        assert!(which_sync("xyz-not-a-real-binary-1234567890").is_none());
    }
}
