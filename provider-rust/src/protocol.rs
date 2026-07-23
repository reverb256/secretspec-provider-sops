//! NDJSON protocol types for cachix/secretspec#98 Secret Provider Protocol v1.
//!
//! Source-of-truth: docs/spec/provider-protocol.md (vendored from
//! https://github.com/cachix/secretspec/pull/98, Domen Kožar, 318-line docs addition).
//!
//! Wire format: line-delimited JSON over stdio (stdin = commands, stdout = responses).
//! First request is `Hello` carrying `config_file`, `protocol_version`, `uri`,
//! `context`. Operations: hello, get, set, batch_get, reflect, bye.
//! `get` and `batch_get` return `value: null` (NOT error) for missing keys.
//!
//! Error envelope: `{ok: false, error: {kind, message}}` with `kind` in the
//! closed set not_found | auth_failed | permission_denied | rate_limited |
//! unsupported | unsupported_version | invalid_request | internal.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Top-level wire envelope for any incoming line from the host.
///
/// `op` is the discriminator (per spec section 5). Operations absent from the
/// spec's `### 5.1..5.5` enumeration return an `Error` with `kind = "unsupported"`.
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    #[serde(rename = "hello")]
    Hello(HelloRequest),
    #[serde(rename = "get")]
    Get(SecretRequest),
    #[serde(rename = "set")]
    Set(SetRequest),
    #[serde(rename = "batch_get")]
    BatchGet(BatchGetRequest),
    #[serde(rename = "reflect")]
    Reflect(ReflectRequest),
    #[serde(rename = "bye")]
    Bye,
}

#[derive(Debug, Deserialize)]
pub struct HelloRequest {
    pub protocol_version: u32,
    pub uri: String,
    pub config_file: String,
    #[serde(default)]
    pub context: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub struct SecretRequest {
    pub project: String,
    pub key: String,
    #[serde(default)]
    pub profile: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetRequest {
    pub project: String,
    pub key: String,
    #[serde(default)]
    pub value: serde_json::Value,
    #[serde(default)]
    pub profile: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BatchGetRequest {
    pub project: String,
    pub keys: Vec<String>,
    #[serde(default)]
    pub profile: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReflectRequest {
    #[serde(default)]
    pub project: Option<String>,
}

/// Wire envelope for any outgoing line to the host.
///
/// `ok: true` is a success. `ok: false` carries `error: {kind, message}`.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Response {
    Hello(HelloResponse),
    Get(GetResponse),
    Set(SetResponse),
    BatchGet(BatchGetResponse),
    Reflect(ReflectResponse),
    Bye(ByeResponse),
    Error(ErrorResponse),
}

#[derive(Debug, Serialize)]
pub struct HelloResponse {
    pub ok: bool,
    pub protocol_version: u32,
    pub name: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct GetResponse {
    pub ok: bool,
    pub value: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SetResponse {
    pub ok: bool,
}

#[derive(Debug, Serialize)]
pub struct BatchGetResponse {
    pub ok: bool,
    /// Map of `key -> resolved_value`. `None` per key indicates the
    /// protocol's mandated `value: null` for missing entries.
    pub values: BTreeMap<String, Option<String>>,
}

#[derive(Debug, Serialize)]
pub struct ReflectResponse {
    pub ok: bool,
    /// Schema per key. Phase 1 implementation: minimal schema
    /// surface — every key advertises `type: "string"` with no
    /// further metadata. Phase 2 will widen to per-secret types
    /// (`password`, `token`, `binary`, etc.) when the host
    /// `secretspec check` validator wants type-aware check.
    pub secrets: BTreeMap<String, SecretSchema>,
}

#[derive(Debug, Serialize)]
pub struct SecretSchema {
    #[serde(rename = "type")]
    pub ty: String,
}

#[derive(Debug, Serialize)]
pub struct ByeResponse {
    pub ok: bool,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub ok: bool,
    pub error: ErrorBody,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub kind: String,
    pub message: String,
}

/// Closed set of protocol error kinds per spec section 6.
pub mod error_kind {
    pub const NOT_FOUND: &str = "not_found";
    pub const AUTH_FAILED: &str = "auth_failed";
    pub const PERMISSION_DENIED: &str = "permission_denied";
    pub const RATE_LIMITED: &str = "rate_limited";
    pub const UNSUPPORTED: &str = "unsupported";
    pub const UNSUPPORTED_VERSION: &str = "unsupported_version";
    pub const INVALID_REQUEST: &str = "invalid_request";
    pub const INTERNAL: &str = "internal";
}

impl HelloResponse {
    /// Construct from the spec's exact field shape: protocol_version is u32 (1),
    /// name is "sops", capabilities lists supported ops.
    pub fn v1() -> Self {
        HelloResponse {
            ok: true,
            protocol_version: 1,
            name: "sops".to_string(),
            capabilities: vec![
                "get".to_string(),
                "set".to_string(),
                "batch_get".to_string(),
                "reflect".to_string(),
                "bye".to_string(),
            ],
        }
    }
}

impl Response {
    /// Construct an Error envelope from kind + message; helper to keep
    /// shape consistent across handlers.
    pub fn error(kind: impl Into<String>, message: impl Into<String>) -> Response {
        Response::Error(ErrorResponse {
            ok: false,
            error: ErrorBody {
                kind: kind.into(),
                message: message.into(),
            },
        })
    }
    /// Construct a successful Hello (used by the dispatcher after handshake).
    pub fn hello() -> Response {
        Response::Hello(Self::v1_helper())
    }
    fn v1_helper() -> HelloResponse {
        HelloResponse::v1()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_request_deserializes_from_spec_example() {
        let s = r#"{"op":"hello","protocol_version":1,"uri":"opproxy://vault/Production?reason=build","config_file":"/abs/path/to/secretspec.toml","context":{"reason":"building api image"}}"#;
        let r: Request = serde_json::from_str(s).unwrap();
        match r {
            Request::Hello(h) => {
                assert_eq!(h.protocol_version, 1);
                assert_eq!(h.uri, "opproxy://vault/Production?reason=build");
                assert_eq!(h.config_file, "/abs/path/to/secretspec.toml");
                assert_eq!(h.context.get("reason").unwrap().as_str(), Some("building api image"));
            }
            _ => panic!("expected Hello"),
        }
    }

    #[test]
    fn get_request_deserializes_from_spec_example() {
        let s = r#"{"op":"get","project":"myapp","key":"DATABASE_URL","profile":"production"}"#;
        let r: Request = serde_json::from_str(s).unwrap();
        match r {
            Request::Get(g) => {
                assert_eq!(g.project, "myapp");
                assert_eq!(g.key, "DATABASE_URL");
                assert_eq!(g.profile.as_deref(), Some("production"));
            }
            _ => panic!("expected Get"),
        }
    }

    #[test]
    fn batch_get_request_deserializes_from_spec_example() {
        let s = r#"{"op":"batch_get","project":"myapp","profile":"production","keys":["DB_URL","API_KEY"]}"#;
        let r: Request = serde_json::from_str(s).unwrap();
        match r {
            Request::BatchGet(b) => {
                assert_eq!(b.project, "myapp");
                assert_eq!(b.keys, vec!["DB_URL", "API_KEY"]);
                assert_eq!(b.profile.as_deref(), Some("production"));
            }
            _ => panic!("expected BatchGet"),
        }
    }

    #[test]
    fn get_response_serializes_null_for_miss() {
        let resp = Response::Get(GetResponse { ok: true, value: None });
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains(""value":null"), "got: {s}");
    }

    #[test]
    fn get_response_serializes_string_for_hit() {
        let resp = Response::Get(GetResponse {
            ok: true,
            value: Some("postgres://example".into()),
        });
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains(""value":"postgres://example""), "got: {s}");
    }

    #[test]
    fn hello_response_supports_v1() {
        let resp = HelloResponse::v1();
        assert_eq!(resp.protocol_version, 1);
        assert_eq!(resp.name, "sops");
        let caps: std::collections::HashSet<_> = resp.capabilities.iter().cloned().collect();
        assert!(caps.contains("get"));
        assert!(caps.contains("set"));
        assert!(caps.contains("batch_get"));
        assert!(caps.contains("reflect"));
        assert!(caps.contains("bye"));
    }

    #[test]
    fn error_response_envelope_shape() {
        let resp = Response::error("not_found", "key `k` not in `secrets.yaml`");
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains(""ok":false"), "got: {s}");
        assert!(s.contains(""kind":"not_found""), "got: {s}");
        assert!(s.contains(""message":"), "got: {s}");
    }
}
