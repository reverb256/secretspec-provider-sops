//! `secretspec-provider-sops` CLI surface.
//!
//! Subcommands:
//! - `get <file> <key> [--format yaml|json|dotenv|bin]` — one-shot decrypt,
//!   prints the resolved value to stdout. Exit 0 on hit; exit 1 on miss
//!   with the error on stderr.
//! - `doctor` — report local env (sops/age binary versions, keyfile path)
//!   as a JSON document on stdout.
//! - `--help` / `--version` — clap auto-generated docs printed to stdout,
//!   exit 0.
//!
//! This is the user-facing binary; the long-running NDJSON protocol
//! dispatcher lives in a separate bin target `secretspec-provider-sops-protocol`
//! (see [`src/main.rs`]).

use clap::{Parser, Subcommand};
use secretspec_provider_sops::{SopsProvider, SopsUri};
use std::process::ExitCode;
use std::str::FromStr;

#[derive(Parser, Debug)]
#[command(
    name = "secretspec-provider-sops",
    version,
    about = "SOPS provider for SecretSpec — one-shot decryption + doctor diagnostic",
    long_about = None,
    // Unix convention: no subcommand → print help + exit 2. clap handles it.
    arg_required_else_help = true,
)]
struct Cli {
    #[command(subcommand)]
    subcommand: Subcmd,
}

#[derive(Subcommand, Debug)]
enum Subcmd {
    /// Decrypt and print a single value from a SOPS-encrypted file
    Get {
        /// Path to the SOPS-encrypted file. Plain path (`./secrets.yaml`)
        /// or full `sops://./secrets.yaml` URI both accepted.
        file: String,
        /// Secret key (or dot path) to extract from the file.
        key: String,
        /// Format hint: `yaml` | `json` | `dotenv` | `bin`.
        /// If absent, inferred from the file extension.
        #[arg(short, long, value_name = "FMT")]
        format: Option<String>,
    },
    /// Report local env (sops/age binary versions, keyfile path) as JSON.
    Doctor,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.subcommand {
        Subcmd::Get { file, key, format } => run_get(&file, &key, format.as_deref()).await,
        Subcmd::Doctor => run_doctor().await,
    }
}

/// Resolve `argv.file` as either a bare path or a `sops://…` URI and
/// dispatch to [`SopsProvider::get_bytes`]. Errors → stderr + exit 1;
/// success → stdout bytes + exit 0.
///
/// `get_bytes` (not `get`) so `?f=bin` works: bin-mode returns the raw
/// decrypted plaintext bytes via `extract_bin` (no UTF-8 coercion).
/// For text formats (yaml/json/dotenv), `get_bytes` returns the same
/// content as `get(...).into_bytes()` — symmetric with the bin path.
async fn run_get(file_arg: &str, key: &str, format_hint: Option<&str>) -> ExitCode {
    // Compute (effective_file, effective_fmt) once: URI-parsed if
    // `file_arg` is a `sops://…` URI, else treated as a bare path.
    // Both arms produce `(String, Option<String>)` so the joined tuple
    // type is uniform; the `--format` flag takes precedence over a
    // `?f=` query param (we OR the flag's owned copy in last).
    let (file_path, fmt): (String, Option<String>) =
        match SopsUri::from_str(file_arg) {
            Ok(u) => (
                u.file,
                format_hint.map(String::from).or(u.format),
            ),
            Err(_) => (file_arg.to_string(), format_hint.map(String::from)),
        };
    match SopsProvider::new()
        .get_bytes(&file_path, key, fmt.as_deref())
        .await
    {
        Ok(bytes) => {
            use tokio::io::AsyncWriteExt;
            let mut stdout = tokio::io::stdout();
            // Best-effort flush; ignore broken-pipe on shell-piped consumers.
            if stdout.write_all(&bytes).await.is_err() {
                return ExitCode::from(0);
            }
            let _ = stdout.flush().await;
            ExitCode::from(0)
        }
        Err(e) => run_get_err(e),
    }
}

fn run_get_err(e: secretspec_provider_sops::SopsError) -> ExitCode {
    // Per cli_smoke test expectation: missing-key errors MUST mention
    // the key name in stderr. SopsError::KeyNotFound carries that info
    // already; we just prepend `error: ` for shell scripting clarity.
    eprintln!("error: {e}");
    ExitCode::from(1)
}

async fn run_doctor() -> ExitCode {
    let provider = SopsProvider::new();
    let report = provider.doctor().await;
    match serde_json::to_string_pretty(&report) {
        Ok(s) => {
            println!("{s}");
            ExitCode::from(0)
        }
        Err(e) => {
            eprintln!("error: doctor report serialization failed: {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    #[test]
    fn parse_cli_get() {
        let cli = Cli::try_parse_from([
            "secretspec-provider-sops",
            "get",
            "secrets.yaml",
            "nvidia_api_key",
            "--format",
            "yaml",
        ])
        .expect("parse");
        match cli.subcommand {
            Subcmd::Get {
                ref file,
                ref key,
                ref format,
            } => {
                assert_eq!(file, "secrets.yaml");
                assert_eq!(key, "nvidia_api_key");
                assert_eq!(format.as_deref(), Some("yaml"));
            }
        }
    }

    #[test]
    fn parse_cli_get_no_format() {
        let cli = Cli::try_parse_from([
            "secretspec-provider-sops",
            "get",
            "secrets.env",
            "N8N_API_KEY",
        ])
        .expect("parse");
        match cli.subcommand {
            Subcmd::Get { file, key, format } => {
                assert_eq!(file, "secrets.env");
                assert_eq!(key, "N8N_API_KEY");
                assert!(format.is_none());
            }
        }
    }

    #[test]
    fn parse_cli_doctor() {
        let cli = Cli::try_parse_from(["secretspec-provider-sops", "doctor"]).expect("parse");
        assert!(matches!(cli.subcommand, Subcmd::Doctor));
    }

    #[test]
    fn parse_cli_no_subcommand_yields_help_error() {
        // With `arg_required_else_help = true`, no subcommand → clap
        // emits DisplayHelp (printed to stderr by clap's default exit
        // handler, then exits 2). try_parse_from returns the error so
        // we can assert the kind without consuming the exit.
        let res = Cli::try_parse_from(["secretspec-provider-sops"]);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), ErrorKind::DisplayHelp);
    }

    #[test]
    fn parse_cli_help_flag_short_circuits_before_subcommand() {
        // `--help` short-circuits regardless of subcommand; clap emits
        // DisplayHelp just like the no-subcommand case.
        let res = Cli::try_parse_from(["secretspec-provider-sops", "--help"]);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), ErrorKind::DisplayHelp);
    }
}
