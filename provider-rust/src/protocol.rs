//! NDJSON protocol types for cachix/secretspec#98 Secret Provider Protocol v1.
//!
//! Source-of-truth schema: /tmp/secretspec-spec/schema.json
//! Source-of-truth spec: cachix/secretspec#98 (protocol-v1.md, sections 4–7).
//!
//! Wire format: line-delimited JSON over stdio (stdin = commands, stdout = responses).
//! First request is `Hello` carrying `config_file`, `protocol_version`, `uri`,
//! `context`. Operations: hello, get, set, batch_get, reflect, bye.
//! `get` and `batch_get` return `value: null` (NOT error) for missing keys.
//!
//! Error envelope: `{ok: false, error: {kind, message}}` with `kind` in the
//! closed set not_found | auth_failed | permission_denied | rate_limited |
//! unsupported | unsupported_version | invalid_request | internal.
//!
//! ## Reconciliation history
//!
//! This module is the canonical Rust shape derived from the JSON Schema. Field
//! variance between the schema and Rust types:
//
//! | Schema field | Rust field | Notes |
//! |---|---|---|
//! | `hello.configFile` (nullable) | `Option<String>` | accepts `null` per spec §4 |
//! | `hello.context` (string-only map per §7) | `BTreeMap<String, String>` | rejects nested values |
//! | `helloResponse.name` (optional) | `Option<String>` + `skip_serializing_if=None` | nullable per §4 prose |
//! | `helloResponse.version` (optional, §4) | `Option<String>` + `skip_serializing_if=None` | only emitted in `HelloResponse::v1()` |
//! | `getRequest.profile` (REQUIRED) | `String` (no `#[serde(default)]`) | mandatory per §5.1 |
//! | `setRequest.value` (REQUIRED string) | `String` (no `#[serde(default)]`) | mandatory per §5.2 |
//! | `setRequest.profile` (REQUIRED) | `String` | mandatory per §5.2 |
//! | `batchGetRequest.profile` (REQUIRED) | `String` | mandatory per §5.3 |
//! | `reflectRequest.project` (REQUIRED) | `String` | mandatory per §5.4 (asymmetric — no profile) |

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Top-level wire envelope for any incoming line from the host.
///
/// `op` is the discriminator (per spec §5). Operations absent from the
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
    /// Absolute path to `secretspec.toml`, or `null` when host resolved secrets
    /// without an on-disk config (programmatic SDK caller) per spec §4.
    pub config_file: Option<String>,
    /// Per spec §7: context values are strings. Nested objects are NOT
    /// supported in v1; serde will reject any non-string value at this layer.
    #[serde(default)]
    pub context: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct SecretRequest {
    pub project: String,
    pub key: String,
    /// Mandatory per spec §5.1 example shape — no `#[serde(default)]` so a
    /// missing `profile` fails deserialization at the protocol layer.
    pub profile: String,
}

#[derive(Debug, Deserialize)]
pub struct SetRequest {
    pub project: String,
    pub key: String,
    /// Mandatory string per spec §5.2 — was previously `serde_json::Value`
    /// which accepted any JSON shape. Narrow: hosts must send a string value.
    pub value: String,
    /// Mandatory per spec §5.2 example shape.
    pub profile: String,
}

#[derive(Debug, Deserialize)]
pub struct BatchGetRequest {
    pub project: String,
    /// Mandatory per spec §5.3 example shape.
    pub profile: String,
    pub keys: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReflectRequest {
    /// Mandatory per spec §5.4 (asymmetric — no `profile` field). Used by
    /// `secretspec import`.
    pub project: String,
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
    /// OPTIONAL per spec §4 prose — both `name` and `version` are NOT mandated
    /// for protocol compliance. `v1()` emits both; downstream code MAY construct
    /// a `HelloResponse` with both `None` for the simplest v1 handshake.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
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

/// Closed set of protocol error kinds per spec §6.
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
    /// Construct from the spec's typical v1 field shape: protocol_version is 1,
    /// name is "sops", version is the crate's `CARGO_PKG_VERSION`. Both
    /// `name` and `version` are emitted; both are schema-optional.
    pub fn v1() -> Self {
        HelloResponse {
            ok: true,
            protocol_version: 1,
            name: Some("sops".to_string()),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
            capabilities: vec![
                "get".to_string(),
                "set".to_string(),
                "batch_get".to_string(),
                "reflect".to_string(),
                "bye".to_string(),
            ],
        }
    }

    /// Minimal v1 hello with `name` and `version` both omitted. Useful for
    /// conformance tests of the §9 forward-compat "tolerate unknown fields"
    /// rule (the protocol layer doesn't care which optional fields are present).
    pub fn minimal_v1() -> Self {
        HelloResponse {
            ok: true,
            protocol_version: 1,
            name: None,
            version: None,
            capabilities: vec!["get".to_string()],
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
        Response::Hello(HelloResponse::v1())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ────────────────────────────────────────────────────────────
    // Happy-path: spec examples deserialized verbatim
    // ────────────────────────────────────────────────────────────

    #[test]
    fn hello_request_deserializes_from_spec_example() {
        let s = r#"{"op":"hello","protocol_version":1,"uri":"opproxy://vault/Production?reason=build","config_file":"/abs/path/to/secretspec.toml","context":{"reason":"building api image"}}"#;
        let r: Request = serde_json::from_str(s).unwrap();
        match r {
            Request::Hello(h) => {
                assert_eq!(h.protocol_version, 1);
                assert_eq!(h.uri, "opproxy://vault/Production?reason=build");
                assert_eq!(h.config_file.as_deref(), Some("/abs/path/to/secretspec.toml"));
                assert_eq!(
                    h.context.get("reason").map(|s| s.as_str()),
                    Some("building api image")
                );
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
                assert_eq!(g.profile, "production");
            }
            _ => panic!("expected Get"),
        }
    }

    #[test]
    fn set_request_deserializes_from_spec_example() {
        let s = r#"{"op":"set","project":"myapp","key":"DATABASE_URL","value":"postgres://...","profile":"production"}"#;
        let r: Request = serde_json::from_str(s).unwrap();
        match r {
            Request::Set(s) => {
                assert_eq!(s.project, "myapp");
                assert_eq!(s.key, "DATABASE_URL");
                assert_eq!(s.value, "postgres://...");
                assert_eq!(s.profile, "production");
            }
            _ => panic!("expected Set"),
        }
    }

    #[test]
    fn batch_get_request_deserializes_from_spec_example() {
        let s = r#"{"op":"batch_get","project":"myapp","profile":"production","keys":["DB_URL","API_KEY"]}"#;
        let r: Request = serde_json::from_str(s).unwrap();
        match r {
            Request::BatchGet(b) => {
                assert_eq!(b.project, "myapp");
                assert_eq!(b.profile, "production");
                assert_eq!(b.keys, vec!["DB_URL", "API_KEY"]);
            }
            _ => panic!("expected BatchGet"),
        }
    }

    #[test]
    fn reflect_request_deserializes_from_spec_example() {
        let s = r#"{"op":"reflect","project":"myapp"}"#;
        let r: Request = serde_json::from_str(s).unwrap();
        match r {
            Request::Reflect(r) => {
                assert_eq!(r.project, "myapp");
            }
            _ => panic!("expected Reflect"),
        }
    }

    #[test]
    fn bye_request_deserializes() {
        let s = r#"{"op":"bye"}"#;
        let r: Request = serde_json::from_str(s).unwrap();
        assert!(matches!(r, Request::Bye));
    }

    // ────────────────────────────────────────────────────────────
    // Hello: schema fidelity — null configFile, nested-context rejection,
    // version forward-compat (missing in input is OK)
    // ────────────────────────────────────────────────────────────

    #[test]
    fn hello_request_accepts_null_config_file_per_spec_section_4() {
        // spec §4: "config_file is the absolute path ... or null if the host
        // resolved secrets without an on-disk config (e.g. a purely
        // programmatic SDK caller)."
        let s = r#"{"op":"hello","protocol_version":1,"uri":"sops://v","config_file":null,"context":{}}"#;
        let r: Request = serde_json::from_str(s).expect("null config_file must parse");
        match r {
            Request::Hello(h) => {
                assert!(h.config_file.is_none(), "null config_file → None");
            }
            _ => panic!("expected Hello"),
        }
    }

    #[test]
    fn hello_request_accepts_missing_context_via_default() {
        // The spec example always includes `context`, but a host MAY legitimately
        // send an empty body when the layer chooses to omit the field. The
        // schema marks context optional; serde(default) handles it.
        let s = r#"{"op":"hello","protocol_version":1,"uri":"sops://v","config_file":"/x"}"#;
        let r: Request = serde_json::from_str(s).expect("missing context must parse");
        match r {
            Request::Hello(h) => {
                assert!(h.context.is_empty(), "missing context → empty BTreeMap");
            }
            _ => panic!("expected Hello"),
        }
    }

    #[test]
    fn hello_request_rejects_non_string_context_value_per_spec_section_7() {
        // spec §7: "Context values are strings. Nested objects are NOT supported in v1."
        // A JSON object value MUST fail deserialization because Rust's
        // BTreeMap<String, String> only accepts strings.
        let s = r#"{"op":"hello","protocol_version":1,"uri":"sops://v","config_file":"/x","context":{"reason":{"nested":"x"}}}"#;
        let result = serde_json::from_str::<Request>(s);
        assert!(
            result.is_err(),
            "nested-object context value MUST fail per spec §7; got: {result:?}"
        );
    }

    #[test]
    fn hello_request_rejects_numeric_context_value_per_spec_section_7() {
        let s = r#"{"op":"hello","protocol_version":1,"uri":"sops://v","config_file":"/x","context":{"reason":42}}"#;
        let result = serde_json::from_str::<Request>(s);
        assert!(
            result.is_err(),
            "numeric context value MUST fail per spec §7; got: {result:?}"
        );
    }

    // ────────────────────────────────────────────────────────────
    // Profile is REQUIRED per spec §5.1, §5.2, §5.3 — verify rejection.
    // ────────────────────────────────────────────────────────────

    #[test]
    fn get_request_rejects_missing_profile() {
        let s = r#"{"op":"get","project":"myapp","key":"DATABASE_URL"}"#;
        let result = serde_json::from_str::<Request>(s);
        assert!(
            result.is_err(),
            "missing profile MUST fail per spec §5.1; got: {result:?}"
        );
    }

    #[test]
    fn set_request_rejects_missing_value() {
        let s = r#"{"op":"set","project":"myapp","key":"K","profile":"production"}"#;
        let result = serde_json::from_str::<Request>(s);
        assert!(
            result.is_err(),
            "missing value MUST fail per spec §5.2; got: {result:?}"
        );
    }

    #[test]
    fn set_request_rejects_non_string_value() {
        // Schema narrows value to string; a JSON number/object/array fails.
        let s = r#"{"op":"set","project":"p","key":"K","value":42,"profile":"production"}"#;
        let result = serde_json::from_str::<Request>(s);
        assert!(
            result.is_err(),
            "numeric value MUST fail per spec §5.2 (string-only); got: {result:?}"
        );
    }

    #[test]
    fn set_request_rejects_missing_profile() {
        let s = r#"{"op":"set","project":"p","key":"K","value":"v"}"#;
        let result = serde_json::from_str::<Request>(s);
        assert!(
            result.is_err(),
            "missing profile MUST fail per spec §5.2; got: {result:?}"
        );
    }

    #[test]
    fn batch_get_request_rejects_missing_profile() {
        let s = r#"{"op":"batch_get","project":"p","keys":["K"]}"#;
        let result = serde_json::from_str::<Request>(s);
        assert!(
            result.is_err(),
            "missing profile MUST fail per spec §5.3; got: {result:?}"
        );
    }

    #[test]
    fn reflect_request_rejects_missing_project() {
        let s = r#"{"op":"reflect"}"#;
        let result = serde_json::from_str::<Request>(s);
        assert!(
            result.is_err(),
            "missing project MUST fail per spec §5.4; got: {result:?}"
        );
    }

    // ────────────────────────────────────────────────────────────
    // HelloResponse: serialization fidelity (optional name/version emission)
    // ────────────────────────────────────────────────────────────

    #[test]
    fn hello_response_v1_emits_name_and_version() {
        let resp = HelloResponse::v1();
        let s = serde_json::to_string(&resp).unwrap();
        assert!(
            s.contains(r#""name":"sops""#),
            "v1() MUST emit name; got: {s}"
        );
        assert!(
            s.contains(r#""version":"#),
            "v1() MUST emit version (CARGO_PKG_VERSION); got: {s}"
        );
        assert!(
            s.contains(r#""protocol_version":1"#),
            "v1() MUST emit protocol_version:1; got: {s}"
        );
    }

    #[test]
    fn hello_response_minimal_v1_omits_name_and_version() {
        // Per spec §9 forward-compat: plugins MAY omit name and version (both
        // optional per spec §4 prose). With skip_serializing_if=Option::is_none,
        // minimal_v1() emits no name, no version key on the wire.
        let resp = HelloResponse::minimal_v1();
        let s = serde_json::to_string(&resp).unwrap();
        assert!(
            !s.contains("\"name\""),
            "minimal_v1() MUST omit name; got: {s}"
        );
        assert!(
            !s.contains("\"version\""),
            "minimal_v1() MUST omit version; got: {s}"
        );
        assert!(
            s.contains("\"capabilities\":[\"get\"]"),
            "minimal_v1() MUST emit capabilities; got: {s}"
        );
    }

    #[test]
    fn hello_response_supports_v1_factory_caps() {
        let resp = HelloResponse::v1();
        assert_eq!(resp.protocol_version, 1);
        assert_eq!(resp.name.as_deref(), Some("sops"));
        assert!(resp.version.is_some(), "version MUST be Some on v1()");
        let caps: std::collections::HashSet<_> = resp.capabilities.iter().cloned().collect();
        assert!(caps.contains("get"));
        assert!(caps.contains("set"));
        assert!(caps.contains("batch_get"));
        assert!(caps.contains("reflect"));
        assert!(caps.contains("bye"));
    }

    // ────────────────────────────────────────────────────────────
    // GetResponse: miss = value:null (not error). spec §5.1.
    // ────────────────────────────────────────────────────────────

    #[test]
    fn get_response_serializes_null_for_miss() {
        let resp = Response::Get(GetResponse {
            ok: true,
            value: None,
        });
        let s = serde_json::to_string(&resp).unwrap();
        let needle = r#""value":null"#;
        assert!(s.contains(needle), "missing value:null marker in: {s}");
    }

    #[test]
    fn get_response_serializes_string_for_hit() {
        let resp = Response::Get(GetResponse {
            ok: true,
            value: Some("postgres://example".to_string()),
        });
        let s = serde_json::to_string(&resp).unwrap();
        let needle = r#""value":"postgres://example""#;
        assert!(
            s.contains(needle),
            "missing value:<postgres://example> in: {s}"
        );
    }

    #[test]
    fn error_response_envelope_shape() {
        let resp = Response::error("not_found", "key `k` not in `secrets.yaml`");
        let s = serde_json::to_string(&resp).unwrap();
        let needle_ok = r#""ok":false"#;
        let needle_kind = r#""kind":"not_found""#;
        let needle_msg = r#""message":"#;
        assert!(s.contains(needle_ok), "missing ok:false flag in: {s}");
        assert!(
            s.contains(needle_kind),
            "missing kind:not_found tag in: {s}"
        );
        assert!(
            s.contains(needle_msg),
            "missing message field in: {s}"
        );
    }
}
