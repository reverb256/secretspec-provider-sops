//! `secretspec-provider-sops` binary entrypoint for
//! cachix/secretspec#98 Secret Provider Protocol v1.
//!
//! Wire format: line-delimited JSON over stdio (stdin reads commands,
//! stdout writes responses). See `docs/spec/provider-protocol.md` for
//! the canonical protocol contract (vendored from
//! https://github.com/cachix/secretspec/pull/98).
//!
//! Operates as a long-running subprocess. Reads one request per line
//! until EOF (host closes stdin) or a `bye` request. Writes one
//! response per line. NO secrets are ever written to stderr;
//! protocol-level errors carry the kind/message via the wire envelope
//! (see `protocol::Response::error`).

use std::collections::BTreeMap;
use std::path::Path;

use secretspec_provider_sops::protocol::{
    error_kind, HelloResponse, ReflectResponse, Response, SecretSchema,
};
use secretspec_provider_sops::provider::SopsProvider;
use tokio::io::{stdin, stdout};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let mut input = BufReader::new(stdin()).lines();
    let mut output = stdout();

    while let Some(line) = input.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: serde_json::Result<secretspec_provider_sops::protocol::Request> =
            serde_json::from_str(trimmed);
        let resp = match req {
            Ok(r) => dispatch(r).await,
            Err(e) => Response::error(error_kind::INVALID_REQUEST, format!("parse: {e}")),
        };
        let mut out_line = serde_json::to_string(&resp)?;
        out_line.push('\n');
        output.write_all(out_line.as_bytes()).await?;
        output.flush().await?;
    }
    Ok(())
}

async fn dispatch(req: secretspec_provider_sops::protocol::Request) -> Response {
    use secretspec_provider_sops::protocol::Request;
    match req {
        Request::Hello(h) => {
            if h.protocol_version != 1 {
                Response::error(
                    error_kind::UNSUPPORTED_VERSION,
                    format!(
                        "protocol_version {} not supported (plugin supports 1)",
                        h.protocol_version
                    ),
                )
            } else {
                Response::Hello(HelloResponse::v1())
            }
        }
        Request::Get(g) => handle_get(g).await,
        Request::Set(s) => handle_set(s).await,
        Request::BatchGet(b) => handle_batch_get(b).await,
        Request::Reflect(r) => handle_reflect(r),
        Request::Bye => Response::Bye(secretspec_provider_sops::protocol::ByeResponse { ok: true }),
    }
}

/// Max directory depth for recursive yaml file search.
/// Fine for the homelab (< 10² entries, local NVMe); if adding NFS-backed
/// secret roots, wrap the directory scan in `spawn_blocking`.
const SEARCH_MAX_DEPTH: u32 = 8;

/// Helper: search all `.yaml` / `.yml` files under `base_dir` for `key`.
/// Returns the first match. Recurses into subdirectories up to `max_depth`.
/// Uses `std::fs::read_dir` (not tokio::fs) so the `fs` tokio feature is not
/// required — directory enumeration is negligible compared to the sops
/// subprocess decryption latency per file.
async fn search_yaml_files(base_dir: &str, key: &str, max_depth: u32) -> Option<String> {
    if max_depth == 0 {
        return None;
    }
    let entries: Vec<_> = std::fs::read_dir(base_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .collect();
    for entry in entries {
        let path = entry.path();
        // Skip symlinks to avoid following cycles into unexpected paths.
        if path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            if let Some(value) = Box::pin(search_yaml_files(
                &path.to_string_lossy(),
                key,
                max_depth - 1,
            ))
            .await
            {
                return Some(value);
            }
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "yaml" || e == "yml")
        {
            let provider = SopsProvider::new();
            if let Ok(value) = provider.get(&path.to_string_lossy(), key, None).await {
                return Some(value);
            }
        }
    }
    None
}

async fn handle_get(g: secretspec_provider_sops::protocol::SecretRequest) -> Response {
    // Per spec section 5.1: missing keys return `value: null` (NOT error).
    //
    // The host sends `project` (= the sops:// URI path) and `key`.
    // Two resolution strategies:
    //
    // 1. If `key` contains a `#` or `/`, treat `key` as a relative
    //    file-path indicator: the part before `#` is the file path
    //    (resolved relative to `project`), the part after `#` is the
    //    YAML/dotenv key within that file. For example:
    //    project="/etc/nixos/secrets" key="ai/nvidia-api-key.yaml#nvidia_api_key"
    //    → decrypt "/etc/nixos/secrets/ai/nvidia-api-key.yaml"
    //    → extract key "nvidia_api_key"
    //
    // 2. If `key` is a flat name (no `/` or `#`), search every `.yaml`
    //    / `.yml` file under `project` for the key (depth-limited).
    //    This handles Convention-addressed secrets where the host only
    //    knows the key name, not the file path.
    //
    // Any resolution error (file missing, key missing, sops binary
    // unavailable) collapses to `value = None` per spec §5.1.
    // Audit hooks (Phase 3) can distinguish error kinds for telemetry;
    // wire-protocol observers just see a clean `value: null`.
    let provider = SopsProvider::new();
    let key = &g.key;

    let get_response = if key.contains('#') || key.contains('/') {
        // Strategy 1: key contains file-path hint (e.g. "ai/nvidia-api-key.yaml#nvidia_api_key")
        let parts: Vec<&str> = key.splitn(2, '#').collect();
        // Guard against empty rel_path (key starts with `#`): default to "."
        // so Path::new(base).join("") doesn't return bare base_dir.
        let rel_path = if parts[0].is_empty() { "." } else { parts[0] };
        let yaml_key = parts.get(1).copied().unwrap_or(key);
        let base = if g.project.is_empty() {
            "."
        } else {
            &g.project
        };
        let full_path = Path::new(base).join(rel_path);
        let file_str = full_path.to_string_lossy().to_string();
        match provider.get(&file_str, yaml_key, None).await {
            Ok(value) => Response::Get(secretspec_provider_sops::protocol::GetResponse {
                ok: true,
                value: Some(value),
            }),
            Err(e) => {
                tracing::warn!(
                    target: "secretspec_provider_sops::audit",
                    error = %e,
                    file = %file_str,
                    key = %yaml_key,
                    "resolve failed; returning null per spec §5.1"
                );
                Response::Get(secretspec_provider_sops::protocol::GetResponse {
                    ok: true,
                    value: None,
                })
            }
        }
    } else {
        // Strategy 2: flat key name — search all yaml files
        match search_yaml_files(&g.project, key, SEARCH_MAX_DEPTH).await {
            Some(value) => Response::Get(secretspec_provider_sops::protocol::GetResponse {
                ok: true,
                value: Some(value),
            }),
            None => Response::Get(secretspec_provider_sops::protocol::GetResponse {
                ok: true,
                value: None,
            }),
        }
    };

    get_response
}

async fn handle_set(_s: secretspec_provider_sops::protocol::SetRequest) -> Response {
    // SOPS immutability: cannot `set` encrypted values into an
    // age-encrypted file in-place. Return permission_denied until a
    // encryption-time hook is added (Phase 2+).
    Response::error(
        error_kind::PERMISSION_DENIED,
        "set is not supported by the SOPS provider (SOPS files are immutable; rotate via sops --encrypt + commit)",
    )
}

async fn handle_batch_get(b: secretspec_provider_sops::protocol::BatchGetRequest) -> Response {
    // Per spec section 5.3: missing keys return `value: null` (NOT error).
    // Phase 1 returns null for every key (host-side enumeration
    // pending); the response shape is still correct.
    let _ = &b;
    let mut values: BTreeMap<String, Option<String>> = BTreeMap::new();
    for k in b.keys {
        values.insert(k, None);
    }
    Response::BatchGet(secretspec_provider_sops::protocol::BatchGetResponse { ok: true, values })
}

fn handle_reflect(_r: secretspec_provider_sops::protocol::ReflectRequest) -> Response {
    // Phase 1 minimal schema: every key advertises type=string. The
    // host reads this to populate `secretspec check` output. Phase 2
    // will surface our `54_keys` from `secretspec.toml` with richer
    // per-secret metadata.
    let mut secrets: BTreeMap<String, SecretSchema> = BTreeMap::new();
    // Single bootstrap entry — Phase 2 will widen to per-project schemas.
    secrets.insert(
        "__bootstrap__".into(),
        SecretSchema {
            ty: "string".into(),
        },
    );
    Response::Reflect(ReflectResponse { ok: true, secrets })
}
