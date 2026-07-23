//! Smoke tests for the `secretspec-provider-sops` CLI binary.
//!
//! Round-trips real `.age`-encrypted YAML + dotenv through the actual
//! binary (via `std::process::Command` invoking the bin target),
//! asserting stdout, stderr, and exit-code for each subcommand.
//!
//! Requires the system binaries `sops`, `age`, and `age-keygen` on PATH
//! — same prerequisite as the lib-level integration tests but exercised
//! at the CLI surface this time. Catches regressions in `main.rs`
//! independently from `lib.rs`.
//!
//! **TempDir lifetime:** `encrypt_yaml_fixture` / `encrypt_env_fixture`
//! return `(TempDir, PathBuf, PathBuf)` — the `TempDir` must stay
//! bound in the calling test's scope for the duration of the CLI
//! subprocess call. Dropping it early deletes the on-disk directory
//! while sops is still trying to read it. Tests use `let (_tmp, ...)`
//! to keep the binding alive.

use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Path to the `secretspec-provider-sops` binary for the current test run.
/// `env!("CARGO_BIN_EXE_<bin>")` is set by cargo when running integration
/// tests against an associated `[[bin]]` target (Rust ≥ 1.43).
fn bin_path() -> &'static str {
    env!("CARGO_BIN_EXE_secretspec-provider-sops")
}

/// Smoke: `secretspec-provider-sops get <file> <key> --format yaml`
/// exits 0 and prints the resolved secret value to stdout
/// (followed by a single newline; nothing else on stdout).
#[test]
fn cli_get_yaml_exits_zero_and_prints_value() {
    let (_tmp, age_keyfile, secrets_yaml) = encrypt_yaml_fixture(
        "nvidia_api_key: nvapi-smoke-replace-me\nopenai_org_id: org-smoke-replace-me\n",
    );

    let output = Command::new(bin_path())
        .args([
            "get",
            secrets_yaml.to_str().unwrap(),
            "nvidia_api_key",
            "--format",
            "yaml",
        ])
        .env("SOPS_AGE_KEY_FILE", &age_keyfile)
        .output()
        .expect("spawn secretspec-provider-sops");

    assert!(
        output.status.success(),
        "cli exited non-zero (code {:?}); stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim_end(),
        "nvapi-smoke-replace-me"
    );
}

/// Smoke: missing-key `get` exits 1, mentions the missing key in stderr.
#[test]
fn cli_get_yaml_missing_key_exits_one_and_logs_error_to_stderr() {
    let (_tmp, age_keyfile, secrets_yaml) = encrypt_yaml_fixture("real_key: present\n");

    let output = Command::new(bin_path())
        .args([
            "get",
            secrets_yaml.to_str().unwrap(),
            "not_a_real_key",
            "--format",
            "yaml",
        ])
        .env("SOPS_AGE_KEY_FILE", &age_keyfile)
        .output()
        .expect("spawn");

    assert!(
        !output.status.success(),
        "expected non-zero exit; got code {:?}",
        output.status.code()
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not_a_real_key"),
        "stderr should mention missing key; got: {stderr}"
    );
}

/// Smoke: `secretspec-provider-sops doctor` exits 0 with valid JSON
/// containing the expected schema fields.
#[test]
fn cli_doctor_exits_zero_and_emits_valid_json() {
    let output = Command::new(bin_path())
        .args(["doctor"])
        .output()
        .expect("spawn doctor");

    assert!(
        output.status.success(),
        "doctor exited non-zero; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("doctor stdout is valid JSON");
    assert!(
        parsed.get("provider_version").is_some(),
        "doctor JSON missing provider_version: {parsed:?}"
    );
    assert!(
        parsed.get("sops_path").is_some(),
        "doctor JSON missing sops_path: {parsed:?}"
    );
}

/// Smoke: dotenv round-trip through the binary extracts ALL three
/// fixture keys (no dead-fixture lines left over from the integration
/// test fork). Preserves quoted interior `#` chars on the quoted-hash
/// line.
#[test]
fn cli_get_dotenv_round_trip_extracts_all_three_keys() {
    let plaintext = "\
N8N_API_KEY=replace-with-real-key
N8N_WEBHOOK_SECRET=\"abc#def\"  # trailing comment
# leading comment
NIX_PACKAGES_CACHE_TOKEN=replace-with-real-token
";
    let (_tmp, age_keyfile, secrets_env) = encrypt_env_fixture(plaintext);

    for (lookup_key, expected) in [
        ("N8N_API_KEY", "replace-with-real-key"),
        ("N8N_WEBHOOK_SECRET", "abc#def"),
        ("NIX_PACKAGES_CACHE_TOKEN", "replace-with-real-token"),
    ] {
        let output = Command::new(bin_path())
            .args([
                "get",
                secrets_env.to_str().unwrap(),
                lookup_key,
                "--format",
                "dotenv",
            ])
            .env("SOPS_AGE_KEY_FILE", &age_keyfile)
            .output()
            .expect("spawn");

        assert!(
            output.status.success(),
            "cli exit non-zero for {lookup_key}; stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim_end(),
            expected,
            "extract mismatch for {lookup_key}"
        );
    }
}

/// Smoke: `--format bin` round-trips a real binary file through
/// `sops --decrypt <file>` byte-for-byte (no UTF-8 coercion; the
/// whole file IS the secret; the `key` arg is ignored for bin).
#[test]
fn cli_get_bin_round_trip_preserves_all_bytes() {
    // Pick plaintext that can't be confused for any text format: all
    // 32 control bytes plus the upper half of a byte sequence. sops's
    // bin mode treats the file as opaque bytes and emits them verbatim
    // on decrypt.
    let plaintext: Vec<u8> = (0u8..=31).chain(0x80u8..=0x90u8).collect();

    let (_tmp, age_keyfile, secrets_bin) = encrypt_bin_fixture(&plaintext);

    let output = Command::new(bin_path())
        .args([
            "get",
            secrets_bin.to_str().unwrap(),
            "_unused_for_bin_mode",
            "--format",
            "bin",
        ])
        .env("SOPS_AGE_KEY_FILE", &age_keyfile)
        .output()
        .expect("spawn bin get");

    assert!(
        output.status.success(),
        "bin get exited non-zero (code {:?}); stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = output.stdout;
    assert_eq!(
        stdout,
        plaintext,
        "bytewise round-trip mismatched; got {} bytes, want {}",
        stdout.len(),
        plaintext.len()
    );
}

/// Smoke: `--help` is processed by clap derive and exits 0, with both
/// `get` and `doctor` subcommands documented in the help text.
#[test]
fn cli_help_exits_zero_and_documents_subcommands() {
    let output = Command::new(bin_path())
        .args(["--help"])
        .output()
        .expect("spawn --help");
    assert!(output.status.success(), "--help exited non-zero");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("secretspec-provider-sops"),
        "--help should mention binary name; got: {stdout}"
    );
    assert!(
        stdout.contains("get"),
        "--help should mention `get` subcommand; got: {stdout}"
    );
    assert!(
        stdout.contains("doctor"),
        "--help should mention `doctor` subcommand; got: {stdout}"
    );
}

/// Generate an age keypair, write + sops-encrypt the given plaintext YAML.
/// Returns the `TempDir` (must remain in caller scope — Drop deletes the
/// on-disk directory) plus the (keyfile, target path).
fn encrypt_yaml_fixture(plaintext: &str) -> (TempDir, PathBuf, PathBuf) {
    encrypt_fixture_inner("secrets.yaml", plaintext)
}

/// Same as `encrypt_yaml_fixture` but writes a `.env` leaf.
fn encrypt_env_fixture(plaintext: &str) -> (TempDir, PathBuf, PathBuf) {
    encrypt_fixture_inner("secrets.env", plaintext)
}

fn encrypt_fixture_inner(leaf: &str, plaintext: &str) -> (TempDir, PathBuf, PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let age_keyfile = dir.path().join("age.key");
    let target = dir.path().join(leaf);

    let age_out = Command::new("age-keygen")
        .arg("-o")
        .arg(&age_keyfile)
        .output()
        .expect("age-keygen");
    assert!(age_out.status.success(), "age-keygen exited non-zero");
    let pubkey = String::from_utf8_lossy(&age_out.stderr)
        .lines()
        .find_map(|l| {
            let t = l.trim();
            if t.starts_with("age1") {
                Some(t.to_string())
            } else {
                t.find("age1").map(|idx| t[idx..].trim().to_string())
            }
        })
        .expect("age1 pubkey from age-keygen stderr");

    std::fs::write(&target, plaintext).expect("write plaintext");

    let encrypt = Command::new("sops")
        .args(["--encrypt", "--in-place", "--age", &pubkey])
        .arg(&target)
        .output()
        .expect("sops");
    assert!(
        encrypt.status.success(),
        "sops --encrypt failed: stderr={}",
        String::from_utf8_lossy(&encrypt.stderr)
    );

    let post = std::fs::read_to_string(&target).expect("read post-encrypt file");
    assert!(
        post.contains("sops:") || post.contains("ENC["),
        "post-encrypt file should contain SOPS envelope markers"
    );

    (dir, age_keyfile, target)
}

/// Build a fixture whose plaintext is arbitrary bytes (control + binary)
/// and whose encrypted-leaf extension is `.bin`, so sops uses its
/// binary-mode round-trip. Returns `(TempDir, keyfile_path, target)`.
///
/// We deliberately do NOT assert on the post-encrypt envelope shape
/// here — bin-mode sops output may or may not contain `sops:` textually
/// (the SOPS metadata header is JSON; the encrypted body is binary).
/// Our only invariant is the round-trip: bin encrypt + bin decrypt
/// yields the original plaintext byte-for-byte.
fn encrypt_bin_fixture(plaintext: &[u8]) -> (TempDir, PathBuf, PathBuf) {
    let dir = TempDir::new().expect("tempdir");
    let age_keyfile = dir.path().join("age.key");
    let target = dir.path().join("secrets.bin");

    let age_out = Command::new("age-keygen")
        .arg("-o")
        .arg(&age_keyfile)
        .output()
        .expect("age-keygen");
    assert!(age_out.status.success(), "age-keygen exited non-zero");
    let pubkey = String::from_utf8_lossy(&age_out.stderr)
        .lines()
        .find_map(|l| {
            let t = l.trim();
            if t.starts_with("age1") {
                Some(t.to_string())
            } else {
                t.find("age1").map(|idx| t[idx..].trim().to_string())
            }
        })
        .expect("age1 pubkey from age-keygen stderr");

    std::fs::write(&target, plaintext).expect("write plaintext");

    let encrypt = Command::new("sops")
        .args(["--encrypt", "--in-place", "--age", &pubkey])
        .arg(&target)
        .output()
        .expect("sops");
    assert!(
        encrypt.status.success(),
        "sops --encrypt (bin mode) failed: stderr={}",
        String::from_utf8_lossy(&encrypt.stderr)
    );

    (dir, age_keyfile, target)
}
