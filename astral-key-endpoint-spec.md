# astral-key → SecretSpec: Vault KV v2 compatible endpoint spec

This is the contract the `astral-key` Rust microservice exposes so
SecretSpec's existing `vault://` provider (v0.15+) can read secrets
without a `secretspec` fork. The secret resolution path in
`CONTEXT.md` → "astral-key Integration" → "Decision: Vault-compatible
endpoint (Option A)" depends on this contract.

This file is the hand-off spec for **upstream**
[github.com/reverb256/astral-key](https://github.com/reverb256/astral-key)
issue [#16](https://github.com/reverb256/astral-key/issues/16). Once the
endpoint ships, this repo flips the `astral_vault` provider from
**forward-declared** to **active** in `secretspec.toml`.

## Endpoint shape

`GET /api/v1/secret/data/<path>` — fully Vault KV v2-compatible path
shape (the `/data/<path>` suffix is canonical KV v2, scratch paths
under `/v1/secret/destroy/<path>` and `/v1/secret/undelete/<path>` are
not required for our use case but are documented for parity).

### Request

```
GET /api/v1/secret/data/<path> HTTP/1.1
Host: astral-key:8080
X-Vault-Request-ID: <optional uuid>
X-Vault-Token: <role_id>.<secret_id>
X-Vault-Namespace: <optional, defaults to "" single-tenant mode>
```

The `X-Vault-Token` carries **AppRole-style credentials** as a
`<role_id>.<secret_id>` pair. `<role_id>` is sourced from SecretSpec's
`credentials = { role_id = { provider = "keyring://" } }` chain; the
client-composed value follows Vault's AppRole wrapping convention so
SecretSpec, on its end, doesn't need to know we're not "real" Vault.

`<path>` is the canonical SOPS-style `snake_case` key name. Examples:
`nvidia_api_key`, `cloudflare_tunnel_token`, `kubeconfig_admin`. The
path has no prefix by default; if a per-tenant prefix is needed,
`X-Vault-Namespace` or a query parameter can encode it.

### Response (success)

```json
{
  "request_id": "…",
  "lease_id": "",
  "renewable": false,
  "lease_duration": 0,
  "data": {
    "data": {
      "<path>": "<the secret value, opaque bytes allowed>",
      "metadata": {
        "created_time": "2026-07-23T00:00:00Z",
        "deletion_time": "",
        "destroyed": false,
        "version": 1
      }
    },
    "metadata": {
      "created_time": "2026-07-23T00:00:00Z",
      "deletion_time": "",
      "destroyed": false,
      "version": 1
    }
  },
  "wrap_info": null,
  "warnings": null,
  "auth": null
}
```

The secret value lives at `data.data.<path>` — the *exact* shape
`vault://` SecretSpec already parses (see cachix/secretspec
[PR #58 review history](https://github.com/cachix/secretspec/pull/58)
for the comment that flagged this).

### Response (lookup miss)

```json
{
  "request_id": "…",
  "lease_id": "",
  "renewable": false,
  "lease_duration": 0,
  "data": null,
  "wrap_info": null,
  "warnings": null,
  "auth": null,
  "errors": [
    "secret not found"
  ]
}
```

HTTP status **404**. The body matches real Vault KV v2's standard
error envelope: `data: null` **and** `errors: ["..."]` are both
present. SecretSpec's `vault://` keys on the HTTP status for
fallback-chain ordering, but downstream tooling (test fixtures in
`sops-provider-design.md` "Tests", custom error layers) should key
on the `errors` array, not on `data`'s presence/absence.

### Response (auth failure)

```json
{
  "errors": [
    "invalid role_id or secret_id"
  ]
}
```

HTTP status **403**. Same error envelope as the lookup-miss case (with
`data: null` and `errors: ["..."]`), distinct error string.

Multiple unmade requests should rate-limit to **429** after 5
failures within 60s, with the same envelope shape. Per standard
Vault KV v2 error envelope: `data: null` + `errors: ["..."]`
are both present.

## Authentication flow: AppRole emulation

The `astral-key` service performs AppRole auth on its backend **without
exposing Vault's AppRole endpoints** — the client composes
`<role_id>.<secret_id>` directly. The flow:

1. **Initial login (out-of-band, used by SecretSpec's `credentials. role_id` chain)**
   - The user (or a bootstrap script) puts the plaintext `<role_id>` and
     `<secret_id>` into the OS keyring (labelled `astral-role-id` and
     `astral-secret-id`) the first time. Long-lived.
   - The homelab's `sops-secrets-registry.nix` already-style bootstrap
     populates these as part of Phase 1 hand-off.

2. **Each `secretspec check` call**
   - `keyring://` provider returns `role_id` from the keyring.
   - `keyring://` provider returns `secret_id` from the keyring (note:
     this is "raw" — NOT a wrapped token; our emulation assembles the
     token client-side).
   - Client composes `<role_id>.<secret_id>` as `X-Vault-Token`.
   - Sends the GET. astral-key verifies the pair against the user's
     Vaultwarden entry in its backend, returns the secret value as
     KV v2 JSON.

3. **Token rotation**
   - The AppRole pair is effectively a static bearer. There's no
     `X-Vault-Token` rotation step in this emulation (real Vault
     renews `/v1/auth/token/renew-self`).
   - Trade-off: simpler to implement, simpler to dogfood; loses the
     "real" Vault lease semantics. Acceptable for the homelab trust
     model since `keyring://` is per-machine already.

## Failover semantics SecretSpec relies on

| Failure mode | Returned status | SecretSpec behavior |
|--------------|-----------------|---------------------|
| secret exists, role_id/secret_id valid | 200 + `data` | resolve to `data.data.<path>` |
| role_id valid, secret_id invalid (revoked) | 403 + `errors` | provider fails; chain moves to next provider in `[providers.<name>].credentials` |
| `role_id` missing from keyring | (keyring-level skip) | chain drops to `env://` |
| astral-key service unreachable | TCP RST / timeout | chain drops to next provider |
| secret exists at `data.data.<path>` but value is empty string | 200 + `data` | resolve to empty string (SecretSpec's framework-level error or empty deployment) |

## What this spec **does not** require

- No `POST /v1/auth/approle/login` endpoint — AppRole pair is composed client-side.
- No `POST /v1/auth/token/lookup-self` — no token introspection.
- No `aws/sts` or `gcp/iam` delegation — short-term, single-tenant
  homelab use case.
- No `cubbyhole/` writes — read-only integration. Writes go through
  Vaultwarden CLI on a workstation, not through this endpoint.
- No audit-log persistence at the protocol level — SecretSpec's own
  audit log ([CONTEXT.md "Audit compliance"](https://github.com/cachix/secretspec))
  records reads; astral-key's log layer is independent.

## Versioning rule

`X-Astral-Key-API-Version: 1` header is **expected** on every response
(not strictly required on requests). Incrementing to `2` requires at
least 90 days of overlap with `1`. Mirrors Vault's API versioning
convention.

## When this endpoint isn't enough

- **Multi-tenant / namespace isolation** — requires
  `X-Vault-Namespace` end-to-end support. Currently a single-tenant
  homelab; punt to v2 if multi-tenant becomes a need.
- **Real AppRole flow (`POST …/login` returning wrapped token)** —
  needed if the secret material should not sit in
  `<role_id>.<secret_id>` plaintext at the client. Punt until we run a
  threat model.
- **KV v2 versioning > 1** — current spec uses implicit `version: 1`.
  Real versioning (`destroy`, `undelete`, `metadata.version`) is
  desired for compliance audit but punted to v2 along with namespaces.

## Cross-references

- `CONTEXT.md` → "astral-key Integration" → "Decision: Vault-compatible
  endpoint (Option A)" — what this spec implements.
- `CONTEXT.md` → "astral-key Integration" → "Provider credentials
  chain" — the SecretSpec-side TOML shape that emits these HTTP calls.
- `sops-provider-design.md` → "Provider credentials pattern" — same
  credentials chain is reused for the SOPS provider's `age_key` lookup.
- `migration-matrix.md` — the per-secret Phase 3 final-provider picks
  that this endpoint enables.
