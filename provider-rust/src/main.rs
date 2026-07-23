//! `secretspec-provider-sops` CLI binary entrypoint.
//!
//! Two subcommands:
//!   - `get <file> <key> [--format <f>]`: resolve a single secret
//!     from a SOPS-encrypted file. Format is inferred from extension
//!     when `--format` is omitted. The resolved value is printed to
//!     stdout (followed by a single newline); nothing else on stdout.
//!     All logs go to stderr, with `tracing`'s default config.
//!   - `doctor`: print a JSON report of the local env (sops version,
//!     age version, SOPS_AGE_KEY_FILE, etc.) — useful for
//!     `sops-secretspec.toml ---` debugging and for the bash
//!     orchestrator's pre-flight check.

use clap::{Parser, Subcommand};
use secretspec_provider_sops::{infer_format_from_path, resolve_bytes, resolve_key, SopsProvider};

#[derive(Parser, Debug)]
#[command(
    name = "secretspec-provider-sops",
    version,
    about = "SOPS provider backend for SecretSpec — Phase 1: CLI shim wrapping `sops --decrypt`"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Resolve a single secret value from a SOPS-encrypted file.
    /// Prints the resolved value to stdout; errors and logs go to stderr.
    Get {
        /// Path to the SOPS-encrypted file.
        file: String,
        /// Key to resolve (YAML path / JSON path / dotenv variable name).
        key: String,
        /// Optional format hint: yaml, json, dotenv (or env), bin.
        /// Defaults to extension inference.
        #[arg(short, long)]
        format: Option<String>,
    },
    /// Print a JSON report of the local provider environment.
    Doctor,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Get { file, key, format } => {
            // Resolve the format early so we can dispatch the binary path
            // through `resolve_bytes` (raw) vs the text path through
            // `resolve_key` (String). For `bin`, the `key` argument is
            // ignored since the whole file IS the secret.
            use std::io::Write;
            let fmt = format
                .as_deref()
                .map(|f| f.to_lowercase())
                .or_else(|| infer_format_from_path(&file));
            let is_bin = matches!(fmt.as_deref(), Some("bin"));

            if is_bin {
                match resolve_bytes(&file, &key, format.as_deref()).await {
                    Ok(bytes) => {
                        std::io::stdout()
                            .write_all(&bytes)
                            .expect("write bytes to stdout");
                        Ok(())
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                match resolve_key(&file, &key, format.as_deref()).await {
                    Ok(value) => {
                        println!("{}", value);
                        Ok(())
                    }
                    Err(e) => {
                        eprintln!("error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        Cmd::Doctor => {
            let provider = SopsProvider::new();
            let report = provider.doctor().await;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
    }
}
