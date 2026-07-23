//! Integration test: round-trip a real `.age`-encrypted YAML through
//! `sops` and our provider.
//!
//! Requires the system binaries `sops`, `age`, and `age-keygen` on PATH.
//! Lokkit does NOT mock these — the test runs against the real toolchain
//! so the `.age` decrypted output goes through our code end-to-end.
//!
//! Thread-safety: tests pass the age keyfile to the provider via
//! `SopsProvider::with_age_keyfile`, so we never mutate the global
//! environment (`std::env::set_var` is UB in multi-threaded Rust ≥ 1.80).
//! Tests can run in parallel under `cargo test` without a
//! `Mutex<()>` orchestrator — each invocation spawns its own `sops`
//! child process with the keyfile set on the child only.

use secretspec_provider_sops::{parse_dotenv_line, SopsProvider};
use std::process::Command;
use tempfile::TempDir;

/// Parse an `age-keygen` stderr line and extract the public key.
///
/// `age-keygen -o <privkey>` writes the private key to `<privkey>` and
/// prints the public key to stderr. The exact format depends on the
/// `age` version:
///
/// - newer (v1.2+): `Public key: age1…`
/// - older or some distros: bare `age1…` on its own line
///
/// We accept either shape by searching for the `age1` substring and
/// returning the substring from there to end-of-line, trimmed.
fn parse_age_pubkey_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with("age1") {
        return Some(trimmed.to_string());
    }
    let idx = trimmed.find("age1")?;
    Some(trimmed[idx..].trim().to_string())
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn parse_age_pubkey_line_accepts_bare() {
        assert_eq!(
            parse_age_pubkey_line("age1abc123"),
            Some("age1abc123".to_string())
        );
    }

    #[test]
    fn parse_age_pubkey_line_accepts_prefixed() {
        assert_eq!(
            parse_age_pubkey_line("Public key: age1abc123"),
            Some("age1abc123".to_string())
        );
    }

    #[test]
    fn parse_age_pubkey_line_returns_none_for_garbage() {
        assert_eq!(parse_age_pubkey_line(""), None);
        assert_eq!(parse_age_pubkey_line("not a key"), None);
    }
}

/// Generate an age keypair, encrypt a YAML with sops, then ask our
/// provider to extract a key. Assert that the decrypted value matches
/// the original plaintext.
///
/// Thread-safety: each test generates its own age keyfile in its own
/// `TempDir` and passes it to the provider via `with_age_keyfile`,
/// so tests can run concurrently under `cargo test`.
#[tokio::test]
async fn extract_yaml_key_via_sops_round_trip() {
    let dir = TempDir::new().expect("tempdir");
    let age_keyfile = dir.path().join("age.key");
    let secrets_yaml = dir.path().join("secrets.yaml");

    // `age-keygen -o <file>` writes the private key to <file> and prints
    // ONLY the public key to stderr. That's the cleanest path on NixOS
    // (no race on stdout/stderr ordering, no manual key parsing).
    let age_out = Command::new("age-keygen")
        .arg("-o")
        .arg(&age_keyfile)
        .output()
        .expect("age-keygen executable lookup failed; install with `nix profile install nixpkgs#age`");
    assert!(age_out.status.success(), "age-keygen exited non-zero");
    let pubkey = String::from_utf8_lossy(&age_out.stderr)
        .lines()
        .find_map(|l| parse_age_pubkey_line(l))
        .expect("no `age1…` public key found in age-keygen stderr");

    // Plaintext YAML.
    let plaintext = "nvidia_api_key: nvapi-test-replace-me\nopenai_org_id: org-test-replace-me\n";
    std::fs::write(&secrets_yaml, plaintext).expect("write plaintext yaml");

    // Encrypt in place. SOPS default behavior writes encrypted output
    // to stdout; `--in-place` is required to overwrite the source file.
    let encrypt_status = Command::new("sops")
        .args(["--encrypt", "--in-place", "--age", &pubkey])
        .arg(&secrets_yaml)
        .status()
        .expect("sops executable lookup failed; install with `nix profile install nixpkgs#sops`");
    assert!(encrypt_status.success(), "sops --encrypt failed");

    // Sanity: the file is now encrypted (contains SOPS markers).
    let encrypted_contents = std::fs::read_to_string(&secrets_yaml).unwrap();
    assert!(encrypted_contents.contains("sops:") || encrypted_contents.contains("ENC["), "post-encrypt file should contain SOPS envelope markers");

    // Per-test provider: keys live in this TempDir, no global env mutate.
    let provider = SopsProvider::with_age_keyfile(&age_keyfile);
    let nvidia_value = provider
        .get(secrets_yaml.to_str().unwrap(), "nvidia_api_key", Some("yaml"))
        .await
        .expect("extract nvidia_api_key");
    let openai_value = provider
        .get(secrets_yaml.to_str().unwrap(), "openai_org_id", Some("yaml"))
        .await
        .expect("extract openai_org_id");

    assert_eq!(nvidia_value, "nvapi-test-replace-me");
    assert_eq!(openai_value, "org-test-replace-me");
}

/// Negative-test: extracting a missing key returns `SopsError::KeyNotFound`.
#[tokio::test]
async fn extract_missing_yaml_key_returns_key_not_found() {
    let dir = TempDir::new().expect("tempdir");
    let age_keyfile = dir.path().join("age.key");
    let secrets_yaml = dir.path().join("secrets.yaml");

    let age_out = Command::new("age-keygen")
        .arg("-o")
        .arg(&age_keyfile)
        .output()
        .expect("age-keygen");
    assert!(age_out.status.success(), "age-keygen exited non-zero");
    let pubkey = String::from_utf8_lossy(&age_out.stderr)
        .lines()
        .find_map(|l| parse_age_pubkey_line(l))
        .expect("no `age1…` public key found in age-keygen stderr");

    std::fs::write(&secrets_yaml, "real_key: present\n").expect("write plaintext");
    // Encrypt in place with the same discipline as the round-trip test:
    // capture stderr so a non-zero exit surfaces the real failure (no
    // silent pass-through), and assert the post-encrypt file actually
    // contains SOPS envelope markers (otherwise sops --decrypt later
    // would return "metadata not found" and we'd chase the wrong error).
    let encrypt_out = Command::new("sops")
        .args(["--encrypt", "--in-place", "--age", &pubkey])
        .arg(&secrets_yaml)
        .output()
        .expect("sops executable lookup failed; install with `nix profile install nixpkgs#sops`");
    assert!(
        encrypt_out.status.success(),
        "sops --encrypt failed: stderr={}",
        String::from_utf8_lossy(&encrypt_out.stderr)
    );
    let encrypted_contents = std::fs::read_to_string(&secrets_yaml)
        .expect("read post-encrypt file");
    assert!(
        encrypted_contents.contains("sops:") || encrypted_contents.contains("ENC["),
        "post-encrypt file should contain SOPS envelope markers"
    );

    // Per-test provider: gives us thread-safety without global env mutate.
    let provider = SopsProvider::with_age_keyfile(&age_keyfile);
    let result = provider
        .get(secrets_yaml.to_str().unwrap(), "not_a_real_key", Some("yaml"))
        .await;

    match result {
        Err(secretspec_provider_sops::SopsError::KeyNotFound { .. }) => {}
        other => panic!("expected KeyNotFound, got {other:?}"),
    }
}

/// Round-trip a dotenv file: `sops --decrypt` then `parse_dotenv_line`
/// finds the right key. We hit `extract_dotenv` via `format_hint` since
/// the `provider.get(...)` match arm is the only public way in.
#[tokio::test]
async fn extract_dotenv_key_via_sops_round_trip() {
    let dir = TempDir::new().expect("tempdir");
    let age_keyfile = dir.path().join("age.key");
    let secrets_env = dir.path().join("secrets.env");

    let age_out = Command::new("age-keygen")
        .arg("-o")
        .arg(&age_keyfile)
        .output()
        .expect("age-keygen");
    assert!(age_out.status.success(), "age-keygen exited non-zero");
    let pubkey = String::from_utf8_lossy(&age_out.stderr)
        .lines()
        .find_map(|l| parse_age_pubkey_line(l))
        .expect("public key in stderr");

    // Plaintext dotenv. Uses `export` + quoted + comment forms so
    // every parser path is exercised on valid input.
    std::fs::write(
        &secrets_env,
        "\
N8N_API_KEY=replace-with-real-key
N8N_WEBHOOK_SECRET=\"abc#def\"  # trailing comment
# leading comment
NIX_PACKAGES_CACHE_TOKEN=replace-with-real-token
",
    )
    .expect("write plaintext env");

    let encrypt_out = Command::new("sops")
        .args(["--encrypt", "--in-place", "--age", &pubkey])
        .arg(&secrets_env)
        .output()
        .expect("sops executable lookup failed");
    assert!(
        encrypt_out.status.success(),
        "sops --encrypt failed: stderr={}",
        String::from_utf8_lossy(&encrypt_out.stderr)
    );

    let provider = SopsProvider::with_age_keyfile(&age_keyfile);
    let n8n_value = provider
        .get(secrets_env.to_str().unwrap(), "N8N_API_KEY", Some("dotenv"))
        .await
        .expect("extract N8N_API_KEY");
    let webhook_value = provider
        .get(secrets_env.to_str().unwrap(), "N8N_WEBHOOK_SECRET", Some("dotenv"))
        .await
        .expect("extract N8N_WEBHOOK_SECRET");
    let cache_token = provider
        .get(secrets_env.to_str().unwrap(), "NIX_PACKAGES_CACHE_TOKEN", Some("dotenv"))
        .await
        .expect("extract NIX_PACKAGES_CACHE_TOKEN");

    assert_eq!(n8n_value, "replace-with-real-key");
    // Quoted interior `#` survives — verifies our parser's quote awareness.
    assert_eq!(webhook_value, "abc#def");
    assert_eq!(cache_token, "replace-with-real-token");
}

/// Direct unit tests of the dotenv parser without going through `get()`.
#[test]
fn parse_dotenv_line_via_export() {
    assert_eq!(parse_dotenv_line("export FOO=bar"), Some(("FOO", "bar")));
}

#[test]
fn parse_dotenv_line_ignores_blank_and_comment() {
    assert_eq!(parse_dotenv_line(""), None);
    assert_eq!(parse_dotenv_line("# comment"), None);
}
