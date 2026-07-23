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

use secretspec_provider_sops::protocol::{
    error_kind, HelloResponse, ReflectResponse, Response, SecretSchema,
};
use secretspec_provider_sops::provider::SopsProvider;
use secretspec_provider_sops::SopsError;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::io::{stdin, stdout};

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
        Request::Hello(_h) => Response::Hello(HelloResponse::v1()),
        Request::Get(g) => handle_get(g).await,
        Request::Set(s) => handle_set(s).await,
        Request::BatchGet(b) => handle_batch_get(b).await,
        Request::Reflect(r) => handle_reflect(r),
        Request::Bye => Response::Bye(secretspec_provider_sops::protocol::ByeResponse { ok: true }),
    }
}

async fn handle_get(g: secretspec_provider_sops::protocol::SecretRequest) -> Response {
    // Per spec section 5.1: missing keys return `value: null` (NOT error).
    let provider = SopsProvider::new();
    // proj -> file mapping: Phase 1 convention is one `.sops.yaml` per
    // project; the SOPS file path is derived from `uri` in the SECRET
    // layer (Host sends `uri = "sops://./secrets.yaml?<key=...>"`).
    // For Phase 1, project is treated as 1-1 with a file basename;
    // the host passes the actual file path via the get request's `key`
    // field as `dot.path` (file_basename.key), OR we resolve from
    // config_file plus convention. Until the host side stabilizes,
    // return not_found for safety.
    //
    // TODO(cachix/secretspec#98): when host enumerates where to look,
    // wire `project` -> `[x-sops].projects.<name>` table reader.
    let _ = (g.project.as_str(), &provider);
    Response::Get(secretspec_provider_sops::protocol::GetResponse {
        ok: true,
        value: None, // Phase 1: not_found for all keys until host-side enumeration ships
    })
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
    Response::BatchGet(secretspec_provider_sops::protocol::BatchGetResponse {
        ok: true,
        values,
    })
}

fn handle_reflect(_r: secretspec_provider_sops::protocol::ReflectRequest) -> Response {
    // Phase 1 minimal schema: every key advertises type=string. The
    // host reads this to populate `secretspec check` output. Phase 2
    // will surface our `54_keys` from `secretspec.toml` with richer
    // per-secret metadata.
    let mut secrets: BTreeMap<String, SecretSchema> = BTreeMap::new();
    // Single bootstrap entry — Phase 2 will widen to per-project schemas.
    secrets.insert("__bootstrap__".into(), SecretSchema { ty: "string".into() });
    Response::Reflect(ReflectResponse { ok: true, secrets })
}

// Suppress unused warnings on the imported-but-not-yet-called crash surface.
#[allow(dead_code)]
fn _suppress_unused(_e: &SopsError, _v: &Value) {}
